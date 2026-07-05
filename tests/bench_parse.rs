//! Throughput benchmark for the parse → process hot path.
//!
//! Replays a raw Wrayth session log through XmlParser + MessageProcessor and
//! reports lines/sec plus allocation counts. This is a soak benchmark, not a
//! microbenchmark — run it in release mode and compare numbers across commits.
//!
//! The corpus comes from VELLUM_BENCH_CORPUS (a Lich session log where each
//! line is prefixed with "HH:MM:SS: "). Without the env var it falls back to
//! a bundled fixture repeated 200x so the test still runs anywhere.
//!
//! Run:
//!   VELLUM_BENCH_CORPUS="C:/Gemstone/Lich5/logs/GSIV-Nisugi/2026/02/2026-02-09_16-49-09.xml" \
//!   cargo test --release --test bench_parse -- --ignored --nocapture
//!
//! IMPORTANT: the window set and highlight patterns below are frozen so that
//! numbers stay comparable across commits. Do not change them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use vellum_fe::config::{Config, HighlightPattern, RedirectMode, SavedDialogPositions};
use vellum_fe::core::messages::MessageProcessor;
use vellum_fe::core::GameState;
use vellum_fe::data::ui_state::UiState;
use vellum_fe::data::window::WindowState;
use vellum_fe::parser::XmlParser;

// ---------------------------------------------------------------------------
// Counting allocator (only affects this test binary, never the shipped app)
// ---------------------------------------------------------------------------

struct CountingAlloc;

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

// ---------------------------------------------------------------------------
// Corpus loading
// ---------------------------------------------------------------------------

/// A Lich log line is "HH:MM:SS: <xml...>". Returns the XML payload, or None
/// for lines without the prefix (e.g. the date header) and client echoes.
fn strip_log_prefix(line: &str) -> Option<&str> {
    let b = line.as_bytes();
    let prefixed = b.len() >= 10
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2] == b':'
        && b[5] == b':'
        && b[8] == b':'
        && b[9] == b' ';
    if !prefixed {
        return None;
    }
    let payload = &line[10..];
    if payload.starts_with("<!-- CLIENT -->") {
        return None;
    }
    Some(payload)
}

fn load_corpus() -> (Vec<String>, String) {
    match std::env::var("VELLUM_BENCH_CORPUS") {
        Ok(path) => {
            let raw = std::fs::read(&path).expect("failed to read VELLUM_BENCH_CORPUS file");
            let text = String::from_utf8_lossy(&raw);
            let lines: Vec<String> = text
                .lines()
                .filter_map(strip_log_prefix)
                .map(str::to_string)
                .collect();
            (lines, path)
        }
        Err(_) => {
            // Fallback: bundled fixture repeated so the test runs anywhere.
            let fixture = include_str!("fixtures/session_start.xml");
            let mut lines = Vec::new();
            for _ in 0..200 {
                lines.extend(fixture.lines().map(str::to_string));
            }
            (lines, "tests/fixtures/session_start.xml x200 (fallback)".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Frozen pipeline configuration — DO NOT CHANGE (comparability across commits)
// ---------------------------------------------------------------------------

fn bench_highlight(pattern: &str) -> HighlightPattern {
    HighlightPattern {
        pattern: pattern.to_string(),
        fg: None,
        bg: None,
        bold: false,
        color_entire_line: false,
        fast_parse: false,
        sound: None,
        sound_volume: None,
        category: None,
        squelch: false,
        silent_prompt: false,
        redirect_to: None,
        redirect_mode: RedirectMode::default(),
        replace: None,
        stream: None,
        window: None,
        compiled_regex: None,
    }
}

/// Optional real-world highlight set via VELLUM_BENCH_HIGHLIGHTS (a
/// highlights.toml). Replaces the frozen synthetic set, so runs using it are
/// NOT comparable with default runs - the output labels which was used.
fn load_bench_highlights() -> Option<(std::collections::HashMap<String, HighlightPattern>, String)> {
    let path = std::env::var("VELLUM_BENCH_HIGHLIGHTS").ok()?;
    let contents = std::fs::read_to_string(&path).expect("failed to read VELLUM_BENCH_HIGHLIGHTS file");
    let mut highlights: std::collections::HashMap<String, HighlightPattern> =
        toml::from_str(&contents).expect("failed to parse VELLUM_BENCH_HIGHLIGHTS as highlights toml");
    // Mirror app startup: compile regexes once at load
    Config::compile_highlight_patterns(&mut highlights);
    Some((highlights, path))
}

fn bench_config() -> (Config, String) {
    let mut config = Config::default();

    if let Some((highlights, path)) = load_bench_highlights() {
        let label = format!("{} patterns from {}", highlights.len(), path);
        config.highlights = highlights;
        return (config, label);
    }

    // 2 color highlights matching common words
    let mut h1 = bench_highlight("You");
    h1.fg = Some("#ff0000".to_string());
    config.highlights.insert("bench_color_you".to_string(), h1);
    let mut h2 = bench_highlight("disk");
    h2.fg = Some("#00ff00".to_string());
    config.highlights.insert("bench_color_disk".to_string(), h2);

    // 2 fast_parse redirects
    let mut r1 = bench_highlight("You feel|You sense");
    r1.fast_parse = true;
    r1.redirect_to = Some("thoughts".to_string());
    r1.redirect_mode = RedirectMode::RedirectCopy;
    config.highlights.insert("bench_redirect_feel".to_string(), r1);
    let mut r2 = bench_highlight("roundtime|Roundtime");
    r2.fast_parse = true;
    r2.redirect_to = Some("speech".to_string());
    r2.redirect_mode = RedirectMode::RedirectCopy;
    config.highlights.insert("bench_redirect_rt".to_string(), r2);

    // 2 squelches (patterns unlikely to match much, but exercise the matcher)
    let mut s1 = bench_highlight("ZZBENCHSQUELCHZZ");
    s1.fast_parse = true;
    s1.squelch = true;
    config.highlights.insert("bench_squelch_1".to_string(), s1);
    let mut s2 = bench_highlight("YYBENCHSQUELCHYY");
    s2.fast_parse = true;
    s2.squelch = true;
    config.highlights.insert("bench_squelch_2".to_string(), s2);

    (config, "6 frozen synthetic patterns (default)".to_string())
}

fn bench_ui_state(mp: &mut MessageProcessor) -> UiState {
    let mut ui_state = UiState::new();
    // Frozen window set: name -> subscribed streams
    let windows: [(&str, &[&str]); 6] = [
        ("main", &["main"]),
        ("thoughts", &["thoughts"]),
        ("speech", &["speech", "talk"]),
        ("logons", &["logons"]),
        ("death", &["death"]),
        ("familiar", &["familiar"]),
    ];
    for (name, streams) in windows {
        let mut ws = WindowState::new_text(name, 1000);
        if let vellum_fe::data::WindowContent::Text(ref mut content) = ws.content {
            content.streams = streams.iter().map(|s| s.to_string()).collect();
        }
        ui_state.windows.insert(name.to_string(), ws);
    }
    mp.update_text_stream_subscribers(&ui_state);
    ui_state
}

// ---------------------------------------------------------------------------
// The benchmark
// ---------------------------------------------------------------------------

#[test]
#[ignore = "throughput soak; run explicitly in release with --ignored --nocapture"]
fn bench_parse_process_throughput() {
    let (lines, corpus_name) = load_corpus();
    assert!(!lines.is_empty(), "corpus is empty");
    println!("corpus: {} ({} XML lines)", corpus_name, lines.len());

    let mut best_lps = 0f64;

    for iteration in 1..=3 {
        // Fresh state per iteration, built outside the timed region
        let mut parser = XmlParser::new();
        let (config, highlights_label) = bench_config();
        if iteration == 1 {
            println!("highlights: {}", highlights_label);
        }
        let mut mp = MessageProcessor::new(config, SavedDialogPositions::default());
        let mut ui_state = bench_ui_state(&mut mp);
        let mut game_state = GameState::new();

        let mut room_components = std::collections::HashMap::new();
        let mut current_room_component = None;
        let mut room_dirty = false;
        let mut nav_room_id = None;
        let mut lich_room_id = None;
        let mut room_subtitle = None;

        let mut total_elements = 0u64;

        let allocs_before = ALLOC_COUNT.load(Ordering::Relaxed);
        let bytes_before = ALLOC_BYTES.load(Ordering::Relaxed);
        let start = Instant::now();

        for line in &lines {
            let elements = parser.parse_line(line);
            total_elements += elements.len() as u64;
            for element in &elements {
                mp.process_element(
                    element,
                    &mut game_state,
                    &mut ui_state,
                    &mut room_components,
                    &mut current_room_component,
                    &mut room_dirty,
                    &mut nav_room_id,
                    &mut lich_room_id,
                    &mut room_subtitle,
                    None,
                );
            }
            mp.flush_current_stream(&mut ui_state);
            // Drain side buffers like process_server_data does, so they don't
            // accumulate across the run
            mp.pending_sounds.clear();
            let _ = mp.take_bounty_buffer();
            let _ = mp.take_society_buffer();
        }

        let elapsed = start.elapsed();
        let allocs = ALLOC_COUNT.load(Ordering::Relaxed) - allocs_before;
        let bytes = ALLOC_BYTES.load(Ordering::Relaxed) - bytes_before;

        let lps = lines.len() as f64 / elapsed.as_secs_f64();
        best_lps = best_lps.max(lps);

        println!(
            "iter {}: {:>10.0} lines/sec | {:>7} elements | {:.3}s | {:>10} allocs | {:>12} bytes",
            iteration,
            lps,
            total_elements,
            elapsed.as_secs_f64(),
            allocs,
            bytes,
        );
    }

    println!("best: {:.0} lines/sec", best_lps);
}
