//! Centralized runtime performance telemetry collection.
//!
//! `PerformanceStats` keeps rolling metrics for draw cadence, render/parse/
//! event timing, network IO, and process CPU/memory so the UI can surface
//! them in the performance monitor and `.performance dump` reports.
//!
//! The [`PERF_METRICS`] table is the single source of truth for what the
//! monitor shows: each metric declares its label, which frontends measure
//! it, how it formats, an optional severity function (threshold coloring),
//! and an optional sparkline series. Both frontends render by walking this
//! table filtered to their own scope — a frontend never renders a metric it
//! doesn't record, so no row can silently read zero. The registry test
//! pins every metric to a `ui.perf_show_<id>` settings-registry entry.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
#[cfg(feature = "desktop")]
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};

/// How many spikes the spike log retains.
const SPIKE_LOG_CAP: usize = 10;
/// Minimum spacing between logged spikes of the same kind.
const SPIKE_DEBOUNCE: Duration = Duration::from_millis(500);
/// Per-window cost samples kept per window.
const WINDOW_COST_SAMPLES: usize = 30;
/// Windows not rendered for this long drop out of the top-cost list.
const WINDOW_COST_STALE: Duration = Duration::from_secs(5);
/// History length (seconds) for per-second sparkline series.
const SERIES_CAP: usize = 60;
/// Draw-stamp retention for the draws/sec window.
const DRAW_WINDOW: Duration = Duration::from_secs(5);
/// Time span of the render avg/p95/max window. Time-based, not
/// sample-count-based: frame rate varies ~100x between idle (2/s) and busy
/// (150+/s), so a fixed sample count would summarize anywhere from half a
/// second to a minute and the stats would jump around with cadence.
const RENDER_STAT_WINDOW: Duration = Duration::from_secs(60);
/// Hard cap on retained render samples (safety valve; ~166 fps sustained).
const RENDER_STAT_CAP: usize = 10_000;

/// One logged outlier: when it happened, what was slow, and what the client
/// was doing at that moment.
#[derive(Debug, Clone)]
pub struct PerfSpike {
    pub at: chrono::DateTime<chrono::Local>,
    /// "render" | "event" | "parse"
    pub kind: &'static str,
    pub ms: f64,
    /// Activity snapshot: net bytes this second, elements parsed, queue depth.
    pub context: String,
}

impl PerfSpike {
    pub fn format_line(&self) -> String {
        format!(
            "{}  {:<6} {:>7.1} ms  {}",
            self.at.format("%H:%M:%S"),
            self.kind,
            self.ms,
            self.context
        )
    }
}

/// Threshold classification for a metric's current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfSeverity {
    Normal,
    Warn,
    Crit,
}

/// Rolling per-window render cost.
#[derive(Debug)]
struct WindowCost {
    samples: VecDeque<Duration>,
    last_seen: Instant,
}

/// Performance statistics tracker
#[derive(Debug)]
pub struct PerformanceStats {
    // Draw cadence: timestamps of recent frame draws
    draw_stamps: VecDeque<Instant>,

    // Network stats
    bytes_received: u64,
    bytes_sent: u64,
    network_sample_start: Instant,
    bytes_received_last_second: u64,
    bytes_sent_last_second: u64,
    net_in_history: VecDeque<u64>,
    net_out_history: VecDeque<u64>,

    // Parser stats
    parse_times: VecDeque<Duration>,
    chunks_parsed: u64,
    parse_sample_start: Instant,
    chunks_parsed_last_second: u64,
    max_parse_samples: usize,

    // General
    app_start_time: Instant,

    // Render timing: total pass ("render") and widget-only pass ("draw",
    // TUI: widgets before terminal flush; unused in the GUI)
    render_times: VecDeque<(Instant, Duration)>,
    ui_render_times: VecDeque<Duration>,
    text_wrap_times: VecDeque<Duration>,
    max_render_samples: usize,
    render_spike_threshold_ms: f64,

    // System/process sampling (desktop-only; stubs report zeros elsewhere)
    #[cfg(feature = "desktop")]
    sysinfo: System,
    #[cfg(feature = "desktop")]
    sysinfo_pid: Option<Pid>,
    last_sys_sample: Instant,
    process_cpu_percent: f32,
    system_cpu_percent: f32,
    process_rss_bytes: u64,
    process_virt_bytes: u64,
    cpu_history: VecDeque<f32>,

    // Event processing
    event_process_times: VecDeque<Duration>,
    events_processed: u64,
    max_event_samples: usize,
    event_queue_depth_max: u64,
    event_queue_depth_last: u64,

    // Buffered content
    total_lines_buffered: usize,
    active_window_count: usize,

    // Element counts
    elements_parsed: u64,
    elements_sample_start: Instant,
    elements_parsed_last_second: u64,
    elems_history: VecDeque<u64>,

    // Spike log
    spike_log: VecDeque<PerfSpike>,
    last_spike_at: HashMap<&'static str, Instant>,

    // Per-window render costs
    window_costs: HashMap<String, WindowCost>,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceStats {
    /// Construct a tracker with rolling windows sized for second-level summaries.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            draw_stamps: VecDeque::with_capacity(600),

            bytes_received: 0,
            bytes_sent: 0,
            network_sample_start: now,
            bytes_received_last_second: 0,
            bytes_sent_last_second: 0,
            net_in_history: VecDeque::with_capacity(SERIES_CAP),
            net_out_history: VecDeque::with_capacity(SERIES_CAP),

            parse_times: VecDeque::with_capacity(60),
            chunks_parsed: 0,
            parse_sample_start: now,
            chunks_parsed_last_second: 0,
            max_parse_samples: 60,

            app_start_time: now,

            render_times: VecDeque::with_capacity(60),
            ui_render_times: VecDeque::with_capacity(60),
            text_wrap_times: VecDeque::with_capacity(60),
            max_render_samples: 60,
            render_spike_threshold_ms: 10.0,
            #[cfg(feature = "desktop")]
            sysinfo: System::new_with_specifics(
                RefreshKind::new().with_cpu(CpuRefreshKind::everything()),
            ),
            #[cfg(feature = "desktop")]
            sysinfo_pid: sysinfo::get_current_pid().ok(),
            last_sys_sample: now,
            process_cpu_percent: 0.0,
            system_cpu_percent: 0.0,
            process_rss_bytes: 0,
            process_virt_bytes: 0,
            cpu_history: VecDeque::with_capacity(SERIES_CAP),

            event_process_times: VecDeque::with_capacity(100),
            events_processed: 0,
            max_event_samples: 100,
            event_queue_depth_max: 0,
            event_queue_depth_last: 0,

            total_lines_buffered: 0,
            active_window_count: 0,

            elements_parsed: 0,
            elements_sample_start: now,
            elements_parsed_last_second: 0,
            elems_history: VecDeque::with_capacity(SERIES_CAP),

            spike_log: VecDeque::with_capacity(SPIKE_LOG_CAP),
            last_spike_at: HashMap::new(),

            window_costs: HashMap::new(),
        }
    }

    /// Record a frame draw (a real repaint, not an idle tick).
    pub fn record_frame(&mut self) {
        let now = Instant::now();
        self.draw_stamps.push_back(now);
        // Drop stamps outside the draws/sec window.
        while let Some(front) = self.draw_stamps.front() {
            if now.duration_since(*front) > DRAW_WINDOW {
                self.draw_stamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Draws per second over the trailing window. Honest for event-driven
    /// frontends: near zero while idle, real cadence while active.
    pub fn draws_per_sec(&self) -> f64 {
        let now = Instant::now();
        let recent = self
            .draw_stamps
            .iter()
            .filter(|t| now.duration_since(**t) <= DRAW_WINDOW)
            .count();
        recent as f64 / DRAW_WINDOW.as_secs_f64()
    }

    /// Record bytes received from network
    pub fn record_bytes_received(&mut self, bytes: u64) {
        self.bytes_received += bytes;
        self.roll_network_second();
    }

    /// Record bytes sent to network
    pub fn record_bytes_sent(&mut self, bytes: u64) {
        self.bytes_sent += bytes;
        self.roll_network_second();
    }

    fn roll_network_second(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.network_sample_start) >= Duration::from_secs(1) {
            self.bytes_received_last_second = self.bytes_received;
            self.bytes_sent_last_second = self.bytes_sent;
            push_capped(&mut self.net_in_history, self.bytes_received, SERIES_CAP);
            push_capped(&mut self.net_out_history, self.bytes_sent, SERIES_CAP);
            self.bytes_received = 0;
            self.bytes_sent = 0;
            self.network_sample_start = now;
        }
    }

    /// Record a parse operation
    pub fn record_parse(&mut self, duration: Duration) {
        let now = Instant::now();

        self.parse_times.push_back(duration);
        if self.parse_times.len() > self.max_parse_samples {
            self.parse_times.pop_front();
        }

        self.chunks_parsed += 1;

        if now.duration_since(self.parse_sample_start) >= Duration::from_secs(1) {
            self.chunks_parsed_last_second = self.chunks_parsed;
            self.chunks_parsed = 0;
            self.parse_sample_start = now;
        }

        let ms = duration.as_secs_f64() * 1000.0;
        if ms > 5.0 {
            self.log_spike("parse", ms);
        }
    }

    /// Get bytes received per second
    pub fn bytes_received_per_sec(&self) -> u64 {
        self.bytes_received_last_second
    }

    /// Get bytes sent per second
    pub fn bytes_sent_per_sec(&self) -> u64 {
        self.bytes_sent_last_second
    }

    /// Get average parse time in microseconds
    pub fn avg_parse_time_us(&self) -> f64 {
        avg_us(&self.parse_times)
    }

    /// 95th-percentile parse time in microseconds
    pub fn p95_parse_time_us(&self) -> f64 {
        percentile_secs(&self.parse_times, 0.95) * 1_000_000.0
    }

    /// Get chunks parsed per second
    pub fn chunks_per_sec(&self) -> u64 {
        self.chunks_parsed_last_second
    }

    /// Get app uptime
    pub fn uptime(&self) -> Duration {
        Instant::now().duration_since(self.app_start_time)
    }

    /// Format uptime as HH:MM:SS
    pub fn uptime_formatted(&self) -> String {
        let uptime = self.uptime();
        let hours = uptime.as_secs() / 3600;
        let minutes = (uptime.as_secs() % 3600) / 60;
        let seconds = uptime.as_secs() % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }

    /// Record total render/paint time for a frame
    pub fn record_render_time(&mut self, duration: Duration) {
        let now = Instant::now();
        self.render_times.push_back((now, duration));
        while let Some((at, _)) = self.render_times.front() {
            if now.duration_since(*at) > RENDER_STAT_WINDOW
                || self.render_times.len() > RENDER_STAT_CAP
            {
                self.render_times.pop_front();
            } else {
                break;
            }
        }
        let ms = duration.as_secs_f64() * 1000.0;
        if ms > self.render_spike_threshold_ms {
            self.log_spike("render", ms);
        }
    }

    /// Record widget-only draw time (TUI: the widget pass before the
    /// terminal flush).
    pub fn record_ui_render_time(&mut self, duration: Duration) {
        self.ui_render_times.push_back(duration);
        if self.ui_render_times.len() > self.max_render_samples {
            self.ui_render_times.pop_front();
        }
    }

    /// Record text wrapping time
    pub fn record_text_wrap_time(&mut self, duration: Duration) {
        self.text_wrap_times.push_back(duration);
        if self.text_wrap_times.len() > self.max_render_samples {
            self.text_wrap_times.pop_front();
        }
    }

    /// Record event processing time
    pub fn record_event_process_time(&mut self, duration: Duration) {
        self.event_process_times.push_back(duration);
        if self.event_process_times.len() > self.max_event_samples {
            self.event_process_times.pop_front();
        }
        self.events_processed += 1;
        let ms = duration.as_secs_f64() * 1000.0;
        if ms > 10.0 {
            self.log_spike("event", ms);
        }
    }

    /// Record observed depth of the event queue
    pub fn record_event_queue_depth(&mut self, depth: u64) {
        self.event_queue_depth_last = depth;
        if depth > self.event_queue_depth_max {
            self.event_queue_depth_max = depth;
        }
    }

    /// Reset the queue-depth peak. Called when the monitor opens so the
    /// peak describes the current session of watching, not the login
    /// flood. The spike log deliberately survives: it is evidence of what
    /// already happened (timestamps make old entries obvious, and the
    /// 10-entry cap ages them out naturally).
    pub fn reset_peaks(&mut self) {
        self.event_queue_depth_max = self.event_queue_depth_last;
    }

    /// Update buffered-content tracking
    pub fn update_memory_stats(&mut self, total_lines: usize, window_count: usize) {
        self.total_lines_buffered = total_lines;
        self.active_window_count = window_count;
    }

    /// Record XML elements parsed
    pub fn record_elements_parsed(&mut self, count: u64) {
        let now = Instant::now();
        self.elements_parsed += count;

        if now.duration_since(self.elements_sample_start) >= Duration::from_secs(1) {
            self.elements_parsed_last_second = self.elements_parsed;
            push_capped(&mut self.elems_history, self.elements_parsed, SERIES_CAP);
            self.elements_parsed = 0;
            self.elements_sample_start = now;
        }
    }

    /// Sample system/process metrics (CPU/RSS) at most once per second
    pub fn sample_sysinfo(&mut self) {
        if Instant::now().duration_since(self.last_sys_sample) < Duration::from_secs(1) {
            return;
        }

        // Refresh global CPU usage plus our own process only - refreshing
        // ProcessRefreshKind::everything() enumerated every process on the
        // machine once per second
        #[cfg(feature = "desktop")]
        {
            self.sysinfo.refresh_specifics(
                RefreshKind::new().with_cpu(CpuRefreshKind::new().with_cpu_usage()),
            );

            self.system_cpu_percent = self.sysinfo.global_cpu_info().cpu_usage();

            if let Some(pid) = self.sysinfo_pid {
                self.sysinfo.refresh_process_specifics(
                    pid,
                    ProcessRefreshKind::new().with_cpu().with_memory(),
                );
                if let Some(proc) = self.sysinfo.process(pid) {
                    self.process_cpu_percent = proc.cpu_usage();
                    // sysinfo 0.30+ reports bytes directly (pre-0.30 was KiB).
                    self.process_rss_bytes = proc.memory();
                    self.process_virt_bytes = proc.virtual_memory();
                }
            }
        }

        push_capped(&mut self.cpu_history, self.process_cpu_percent, SERIES_CAP);
        self.last_sys_sample = Instant::now();
    }

    /// Record one window's render cost this frame.
    pub fn record_window_render(&mut self, name: &str, duration: Duration) {
        let now = Instant::now();
        let entry = self
            .window_costs
            .entry(name.to_string())
            .or_insert_with(|| WindowCost {
                samples: VecDeque::with_capacity(WINDOW_COST_SAMPLES),
                last_seen: now,
            });
        entry.samples.push_back(duration);
        if entry.samples.len() > WINDOW_COST_SAMPLES {
            entry.samples.pop_front();
        }
        entry.last_seen = now;

        // Keep the map from growing without bound as windows come and go.
        if self.window_costs.len() > 64 {
            self.window_costs
                .retain(|_, cost| now.duration_since(cost.last_seen) < WINDOW_COST_STALE);
        }
    }

    /// The most expensive recently rendered windows: (name, avg ms), sorted
    /// descending.
    pub fn top_window_costs(&self, n: usize) -> Vec<(String, f64)> {
        let now = Instant::now();
        let mut costs: Vec<(String, f64)> = self
            .window_costs
            .iter()
            .filter(|(_, cost)| now.duration_since(cost.last_seen) < WINDOW_COST_STALE)
            .map(|(name, cost)| (name.clone(), avg_us(&cost.samples) / 1000.0))
            .collect();
        costs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        costs.truncate(n);
        costs
    }

    /// Per-window costs from the most recent frame: each window's LAST
    /// sample (not the rolling average), so a one-frame blowup in a single
    /// window is attributed to that window instead of being diluted.
    /// Only windows rendered within the last quarter second count.
    fn last_frame_window_costs(&self, n: usize) -> Vec<(String, f64)> {
        let now = Instant::now();
        let mut costs: Vec<(String, f64)> = self
            .window_costs
            .iter()
            .filter(|(_, cost)| now.duration_since(cost.last_seen) < Duration::from_millis(250))
            .filter_map(|(name, cost)| {
                cost.samples
                    .back()
                    .map(|d| (name.clone(), d.as_secs_f64() * 1000.0))
            })
            .collect();
        costs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        costs.truncate(n);
        costs
    }

    fn log_spike(&mut self, kind: &'static str, ms: f64) {
        let now = Instant::now();
        if let Some(last) = self.last_spike_at.get(kind) {
            if now.duration_since(*last) < SPIKE_DEBOUNCE {
                return;
            }
        }
        self.last_spike_at.insert(kind, now);
        let mut context = format!(
            "{}, {} elems, queue {}",
            format_bytes(self.bytes_received),
            self.elements_parsed,
            self.event_queue_depth_last
        );
        if kind == "render" {
            let culprits = self.last_frame_window_costs(3);
            if !culprits.is_empty() {
                let list = culprits
                    .iter()
                    .map(|(name, ms)| format!("{} {:.1}", name, ms))
                    .collect::<Vec<_>>()
                    .join(", ");
                context.push_str(&format!(" · [{}]", list));
            }
        }
        self.spike_log.push_back(PerfSpike {
            at: chrono::Local::now(),
            kind,
            ms,
            context,
        });
        if self.spike_log.len() > SPIKE_LOG_CAP {
            self.spike_log.pop_front();
        }
    }

    /// Recent outliers, oldest first.
    pub fn spike_log(&self) -> impl Iterator<Item = &PerfSpike> {
        self.spike_log.iter()
    }

    // === Getters ===

    /// Get average render time in milliseconds (trailing 60s)
    pub fn avg_render_time_ms(&self) -> f64 {
        if self.render_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.render_times.iter().map(|(_, d)| *d).sum();
        total.as_secs_f64() * 1000.0 / self.render_times.len() as f64
    }

    /// 95th-percentile render time in milliseconds (trailing 60s)
    pub fn p95_render_time_ms(&self) -> f64 {
        let samples: VecDeque<Duration> = self.render_times.iter().map(|(_, d)| *d).collect();
        percentile_secs(&samples, 0.95) * 1000.0
    }

    /// Get max render time in milliseconds (trailing 60s)
    pub fn max_render_time_ms(&self) -> f64 {
        self.render_times
            .iter()
            .map(|(_, d)| d)
            .max()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get average widget-draw time in milliseconds
    pub fn avg_ui_render_time_ms(&self) -> f64 {
        avg_us(&self.ui_render_times) / 1000.0
    }

    /// 95th-percentile widget-draw time in milliseconds
    pub fn p95_ui_render_time_ms(&self) -> f64 {
        percentile_secs(&self.ui_render_times, 0.95) * 1000.0
    }

    /// Get average text wrap time in microseconds
    pub fn avg_text_wrap_time_us(&self) -> f64 {
        avg_us(&self.text_wrap_times)
    }

    /// Get average event process time in microseconds
    pub fn avg_event_process_time_us(&self) -> f64 {
        avg_us(&self.event_process_times)
    }

    /// Get max event process time in microseconds
    pub fn max_event_process_time_us(&self) -> f64 {
        self.event_process_times
            .iter()
            .max()
            .map(|d| d.as_secs_f64() * 1_000_000.0)
            .unwrap_or(0.0)
    }

    /// Get last recorded event queue depth
    pub fn last_event_queue_depth(&self) -> u64 {
        self.event_queue_depth_last
    }

    /// Get maximum observed event queue depth
    pub fn max_event_queue_depth(&self) -> u64 {
        self.event_queue_depth_max
    }

    /// Get total events processed
    pub fn total_events_processed(&self) -> u64 {
        self.events_processed
    }

    /// Get total lines buffered across all windows
    pub fn total_lines_buffered(&self) -> usize {
        self.total_lines_buffered
    }

    /// Get active window count
    pub fn active_window_count(&self) -> usize {
        self.active_window_count
    }

    /// Process resident set size in MB
    pub fn process_rss_mb(&self) -> f64 {
        self.process_rss_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Process virtual memory size in MB
    pub fn process_virt_mb(&self) -> f64 {
        self.process_virt_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn process_cpu_percent(&self) -> f64 {
        self.process_cpu_percent as f64
    }

    pub fn system_cpu_percent(&self) -> f64 {
        self.system_cpu_percent as f64
    }

    /// Get elements parsed per second
    pub fn elements_per_sec(&self) -> u64 {
        self.elements_parsed_last_second
    }

    // === Sparkline series (recent-last) ===

    pub fn render_series_ms(&self) -> Vec<f32> {
        self.render_times
            .iter()
            .map(|(_, d)| (d.as_secs_f64() * 1000.0) as f32)
            .collect()
    }

    pub fn net_in_series(&self) -> Vec<f32> {
        self.net_in_history.iter().map(|b| *b as f32).collect()
    }

    pub fn elems_series(&self) -> Vec<f32> {
        self.elems_history.iter().map(|e| *e as f32).collect()
    }

    pub fn cpu_series(&self) -> Vec<f32> {
        self.cpu_history.iter().copied().collect()
    }

    /// Full plain-text report for `.performance dump`: every metric this
    /// frontend measures (ignoring row toggles), the spike log, and the
    /// per-window costs. Frontends append their own sections (e.g. egui
    /// internals) before writing.
    pub fn dump_text(&self, frontend: PerfFrontend) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "VellumFE {} performance dump — {} frontend\n",
            env!("CARGO_PKG_VERSION"),
            frontend.name()
        ));
        out.push_str(&format!(
            "Captured {}\n\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        out.push_str("== Metrics ==\n");
        for metric in PERF_METRICS.iter().filter(|m| m.in_scope(frontend)) {
            let value = (metric.format)(self);
            for (i, line) in value.lines().enumerate() {
                if i == 0 {
                    out.push_str(&format!("{:<12} {}\n", metric.label, line));
                } else {
                    out.push_str(&format!("{:<12} {}\n", "", line));
                }
            }
        }

        out.push_str(&format!(
            "{:<12} {} total this session\n",
            "Events",
            self.total_events_processed()
        ));

        out.push_str("\n== Spike log (oldest first) ==\n");
        if self.spike_log.is_empty() {
            out.push_str("(no spikes recorded)\n");
        } else {
            for spike in &self.spike_log {
                out.push_str(&spike.format_line());
                out.push('\n');
            }
        }

        out.push_str("\n== Window render costs ==\n");
        let costs = self.top_window_costs(16);
        if costs.is_empty() {
            out.push_str("(no windows timed)\n");
        } else {
            for (name, ms) in costs {
                out.push_str(&format!("{:<24} {:>7.2} ms avg\n", name, ms));
            }
        }

        out
    }
}

fn push_capped<T>(deque: &mut VecDeque<T>, value: T, cap: usize) {
    deque.push_back(value);
    if deque.len() > cap {
        deque.pop_front();
    }
}

fn avg_us(samples: &VecDeque<Duration>) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let total: Duration = samples.iter().sum();
    total.as_secs_f64() * 1_000_000.0 / samples.len() as f64
}

/// Percentile over a rolling sample window, in seconds. Uses the
/// nearest-rank method on a sorted copy.
fn percentile_secs(samples: &VecDeque<Duration>, p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = samples.iter().map(|d| d.as_secs_f64()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Render a sample series as a fixed-width block-character sparkline
/// (`▁▂▃▄▅▆▇█`), normalized to the series max. Shared by the TUI (used
/// directly) and anything else that wants a text sparkline.
pub fn sparkline_string(values: &[f32], width: usize) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() || width == 0 {
        return String::new();
    }
    let max = values.iter().cloned().fold(0.0f32, f32::max);
    // Resample to `width` buckets (mean per bucket).
    let mut out = String::with_capacity(width * 3);
    for i in 0..width {
        let start = i * values.len() / width;
        let end = (((i + 1) * values.len()) / width)
            .max(start + 1)
            .min(values.len());
        let slice = &values[start..end];
        let v = slice.iter().sum::<f32>() / slice.len() as f32;
        let level = if max <= 0.0 {
            0
        } else {
            ((v / max) * 7.0).round().clamp(0.0, 7.0) as usize
        };
        out.push(BLOCKS[level]);
    }
    out
}

// ---- Metric registry --------------------------------------------------------

/// Which frontend is asking for metric rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfFrontend {
    Tui,
    Gui,
    /// Headless core (web/phone server): TUI-equivalent scope minus
    /// draw/render metrics.
    Headless,
}

impl PerfFrontend {
    pub fn name(self) -> &'static str {
        match self {
            PerfFrontend::Tui => "TUI",
            PerfFrontend::Gui => "GUI",
            PerfFrontend::Headless => "headless",
        }
    }
}

/// One row (or short section) of the performance monitor.
pub struct PerfMetric {
    /// Matches the `ui.perf_show_<id>` settings key and the
    /// `PerformanceWidgetData` field.
    pub id: &'static str,
    pub label: &'static str,
    pub tui: bool,
    pub gui: bool,
    /// Whether the headless runtime records this (for dump reports).
    pub headless: bool,
    /// May return multiple lines separated by '\n'.
    pub format: fn(&PerformanceStats) -> String,
    /// Threshold coloring for the row.
    pub severity: Option<fn(&PerformanceStats) -> PerfSeverity>,
    /// Recent sample series for a sparkline next to the row.
    pub spark: Option<fn(&PerformanceStats) -> Vec<f32>>,
}

impl PerfMetric {
    pub fn in_scope(&self, frontend: PerfFrontend) -> bool {
        match frontend {
            PerfFrontend::Tui => self.tui,
            PerfFrontend::Gui => self.gui,
            PerfFrontend::Headless => self.headless,
        }
    }

    /// Whether this metric's row is enabled in the given widget config.
    pub fn enabled_in(&self, cfg: &crate::config::PerformanceWidgetData) -> bool {
        match self.id {
            "fps" => cfg.show_fps,
            "render_times" => cfg.show_render_times,
            "ui_times" => cfg.show_ui_times,
            "wrap_times" => cfg.show_wrap_times,
            "net" => cfg.show_net,
            "parse" => cfg.show_parse,
            "events" => cfg.show_events,
            "cpu" => cfg.show_cpu,
            "memory" => cfg.show_memory,
            "lines" => cfg.show_lines,
            "uptime" => cfg.show_uptime,
            "spike_log" => cfg.show_spike_log,
            "per_window" => cfg.show_per_window,
            _ => false,
        }
    }
}

/// The metric table. Order here is display order in both frontends.
pub const PERF_METRICS: &[PerfMetric] = &[
    PerfMetric {
        id: "fps",
        label: "Draws/s",
        tui: true,
        gui: true,
        headless: false,
        format: |s| format!("{:.1}", s.draws_per_sec()),
        severity: None,
        spark: None,
    },
    PerfMetric {
        id: "render_times",
        label: "Render",
        tui: true,
        gui: true,
        headless: false,
        format: |s| {
            format!(
                "{:.2} ms · p95 {:.2} · max {:.2}",
                s.avg_render_time_ms(),
                s.p95_render_time_ms(),
                s.max_render_time_ms()
            )
        },
        severity: Some(|s| {
            let p95 = s.p95_render_time_ms();
            if p95 > 25.0 {
                PerfSeverity::Crit
            } else if p95 > 10.0 {
                PerfSeverity::Warn
            } else {
                PerfSeverity::Normal
            }
        }),
        spark: Some(|s| s.render_series_ms()),
    },
    PerfMetric {
        id: "ui_times",
        label: "Draw",
        tui: true,
        gui: false,
        headless: false,
        format: |s| {
            format!(
                "{:.2} ms · p95 {:.2}",
                s.avg_ui_render_time_ms(),
                s.p95_ui_render_time_ms()
            )
        },
        severity: None,
        spark: None,
    },
    PerfMetric {
        id: "wrap_times",
        label: "Wrap",
        tui: true,
        gui: false,
        headless: false,
        format: |s| format!("{:.0} µs", s.avg_text_wrap_time_us()),
        severity: None,
        spark: None,
    },
    PerfMetric {
        id: "net",
        label: "Net",
        tui: true,
        gui: true,
        headless: true,
        format: |s| {
            format!(
                "In {:.2} KB/s\nOut {:.2} KB/s",
                s.bytes_received_per_sec() as f64 / 1024.0,
                s.bytes_sent_per_sec() as f64 / 1024.0
            )
        },
        severity: None,
        spark: Some(|s| s.net_in_series()),
    },
    PerfMetric {
        id: "parse",
        label: "Parse",
        tui: true,
        gui: true,
        headless: true,
        format: |s| {
            format!(
                "{:.0} µs · p95 {:.0}\nChunks/s {} · Elems/s {}",
                s.avg_parse_time_us(),
                s.p95_parse_time_us(),
                s.chunks_per_sec(),
                s.elements_per_sec()
            )
        },
        severity: None,
        spark: Some(|s| s.elems_series()),
    },
    PerfMetric {
        id: "events",
        label: "Events",
        tui: true,
        gui: true,
        headless: false,
        format: |s| {
            format!(
                "{:.0} µs · max {:.0}\nQueue {} (peak {})",
                s.avg_event_process_time_us(),
                s.max_event_process_time_us(),
                s.last_event_queue_depth(),
                s.max_event_queue_depth()
            )
        },
        severity: Some(|s| {
            let depth = s.last_event_queue_depth();
            if depth > 50 {
                PerfSeverity::Crit
            } else if depth > 10 {
                PerfSeverity::Warn
            } else {
                PerfSeverity::Normal
            }
        }),
        spark: None,
    },
    PerfMetric {
        id: "cpu",
        label: "CPU",
        tui: true,
        gui: true,
        headless: true,
        format: |s| {
            format!(
                "{:.1}% (sys {:.1}%)",
                s.process_cpu_percent(),
                s.system_cpu_percent()
            )
        },
        severity: Some(|s| {
            let cpu = s.process_cpu_percent();
            if cpu > 70.0 {
                PerfSeverity::Crit
            } else if cpu > 30.0 {
                PerfSeverity::Warn
            } else {
                PerfSeverity::Normal
            }
        }),
        spark: Some(|s| s.cpu_series()),
    },
    PerfMetric {
        id: "memory",
        label: "Memory",
        tui: true,
        gui: true,
        headless: true,
        format: |s| {
            format!(
                "RSS {:.1} MB (virt {:.1} MB)",
                s.process_rss_mb(),
                s.process_virt_mb()
            )
        },
        severity: Some(|s| {
            let rss = s.process_rss_mb();
            if rss > 1500.0 {
                PerfSeverity::Crit
            } else if rss > 750.0 {
                PerfSeverity::Warn
            } else {
                PerfSeverity::Normal
            }
        }),
        spark: None,
    },
    PerfMetric {
        id: "lines",
        label: "Buffers",
        tui: true,
        gui: true,
        headless: false,
        format: |s| {
            format!(
                "{} lines in {} windows",
                s.total_lines_buffered(),
                s.active_window_count()
            )
        },
        severity: None,
        spark: None,
    },
    PerfMetric {
        id: "uptime",
        label: "Uptime",
        tui: true,
        gui: true,
        headless: true,
        format: |s| s.uptime_formatted(),
        severity: None,
        spark: None,
    },
    PerfMetric {
        id: "per_window",
        label: "Windows",
        tui: true,
        gui: true,
        headless: false,
        format: |s| {
            let costs = s.top_window_costs(3);
            if costs.is_empty() {
                "(no windows timed yet)".to_string()
            } else {
                costs
                    .iter()
                    .map(|(name, ms)| format!("{} {:.2} ms", name, ms))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        },
        severity: None,
        spark: None,
    },
    PerfMetric {
        id: "spike_log",
        label: "Spikes",
        tui: true,
        gui: true,
        headless: true,
        format: |s| {
            let lines: Vec<String> = s.spike_log().map(|spike| spike.format_line()).collect();
            if lines.is_empty() {
                "(none)".to_string()
            } else {
                lines.join("\n")
            }
        },
        severity: Some(|s| {
            if s.spike_log().next().is_some() {
                PerfSeverity::Warn
            } else {
                PerfSeverity::Normal
            }
        }),
        spark: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_performance_stats() {
        let stats = PerformanceStats::new();

        assert_eq!(stats.draws_per_sec(), 0.0);
        assert_eq!(stats.avg_render_time_ms(), 0.0);
        assert_eq!(stats.p95_render_time_ms(), 0.0);
        assert_eq!(stats.bytes_received_per_sec(), 0);
        assert_eq!(stats.bytes_sent_per_sec(), 0);
        assert_eq!(stats.avg_parse_time_us(), 0.0);
        assert_eq!(stats.chunks_per_sec(), 0);
        assert_eq!(stats.total_events_processed(), 0);
        assert_eq!(stats.total_lines_buffered(), 0);
        assert_eq!(stats.active_window_count(), 0);
        assert_eq!(stats.spike_log().count(), 0);
        assert!(stats.top_window_costs(3).is_empty());
    }

    #[test]
    fn test_draws_per_sec_counts_recent_draws() {
        let mut stats = PerformanceStats::new();
        for _ in 0..10 {
            stats.record_frame();
        }
        // 10 draws inside the 5s window -> 2 draws/sec
        let dps = stats.draws_per_sec();
        assert!((dps - 2.0).abs() < 0.2, "expected ~2 draws/s, got {}", dps);
    }

    #[test]
    fn test_render_time_recording() {
        let mut stats = PerformanceStats::new();

        let now = Instant::now();
        stats
            .render_times
            .push_back((now, Duration::from_millis(5)));
        stats
            .render_times
            .push_back((now, Duration::from_millis(10)));
        stats
            .render_times
            .push_back((now, Duration::from_millis(15)));

        let avg = stats.avg_render_time_ms();
        assert!((avg - 10.0).abs() < 0.1, "Expected 10ms, got {}", avg);
        assert!((stats.max_render_time_ms() - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_p95_nearest_rank() {
        let mut stats = PerformanceStats::new();
        // 20 samples: 19 at 2ms, 1 at 40ms. p95 over 20 samples = rank 19
        // (ceil(0.95*20)=19) -> still 2ms; max shows the outlier.
        let now = Instant::now();
        for _ in 0..19 {
            stats
                .render_times
                .push_back((now, Duration::from_millis(2)));
        }
        stats
            .render_times
            .push_back((now, Duration::from_millis(40)));
        let p95 = stats.p95_render_time_ms();
        assert!((p95 - 2.0).abs() < 0.01, "expected 2ms p95, got {}", p95);
        assert!((stats.max_render_time_ms() - 40.0).abs() < 0.01);

        // Make the tail 2/20 -> p95 rank catches it.
        stats.render_times.pop_front();
        stats
            .render_times
            .push_back((now, Duration::from_millis(40)));
        let p95 = stats.p95_render_time_ms();
        assert!((p95 - 40.0).abs() < 0.01, "expected 40ms p95, got {}", p95);
    }

    #[test]
    fn test_percentile_empty_returns_zero() {
        let samples: VecDeque<Duration> = VecDeque::new();
        assert_eq!(percentile_secs(&samples, 0.95), 0.0);
    }

    #[test]
    fn test_spike_log_records_context_and_caps() {
        let mut stats = PerformanceStats::new();
        stats.record_event_queue_depth(7);

        stats.record_render_time(Duration::from_millis(42));
        assert_eq!(stats.spike_log().count(), 1);
        let spike = stats.spike_log().next().unwrap();
        assert_eq!(spike.kind, "render");
        assert!((spike.ms - 42.0).abs() < 0.5);
        assert!(
            spike.context.contains("queue 7"),
            "context: {}",
            spike.context
        );

        // Debounce: an immediate second render spike is dropped.
        stats.record_render_time(Duration::from_millis(30));
        assert_eq!(stats.spike_log().count(), 1);

        // A different kind is not debounced by the render spike.
        stats.record_event_process_time(Duration::from_millis(20));
        assert_eq!(stats.spike_log().count(), 2);
    }

    #[test]
    fn test_spike_log_below_threshold_ignored() {
        let mut stats = PerformanceStats::new();
        stats.record_render_time(Duration::from_millis(2));
        stats.record_event_process_time(Duration::from_micros(500));
        stats.record_parse(Duration::from_micros(800));
        assert_eq!(stats.spike_log().count(), 0);
    }

    #[test]
    fn test_reset_peaks_resets_queue_max_but_keeps_spikes() {
        let mut stats = PerformanceStats::new();
        stats.record_event_queue_depth(80);
        stats.record_event_queue_depth(3);
        stats.record_render_time(Duration::from_millis(42));
        assert_eq!(stats.max_event_queue_depth(), 80);
        assert_eq!(stats.spike_log().count(), 1);

        stats.reset_peaks();
        assert_eq!(stats.max_event_queue_depth(), 3);
        // Spike history is evidence of what already happened; opening the
        // monitor must not destroy it.
        assert_eq!(stats.spike_log().count(), 1);
    }

    #[test]
    fn test_window_costs_top_and_sorting() {
        let mut stats = PerformanceStats::new();
        for _ in 0..5 {
            stats.record_window_render("main", Duration::from_millis(4));
            stats.record_window_render("thoughts", Duration::from_millis(1));
            stats.record_window_render("compass", Duration::from_micros(100));
        }
        let top = stats.top_window_costs(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "main");
        assert!((top[0].1 - 4.0).abs() < 0.1);
        assert_eq!(top[1].0, "thoughts");
    }

    #[test]
    fn test_event_process_time_recording() {
        let mut stats = PerformanceStats::new();

        stats.record_event_process_time(Duration::from_micros(100));
        stats.record_event_process_time(Duration::from_micros(200));
        stats.record_event_process_time(Duration::from_micros(300));

        assert_eq!(stats.total_events_processed(), 3);

        let avg = stats.avg_event_process_time_us();
        assert!((avg - 200.0).abs() < 0.1, "Expected 200us, got {}", avg);

        let max = stats.max_event_process_time_us();
        assert!((max - 300.0).abs() < 0.1, "Expected 300us, got {}", max);
    }

    #[test]
    fn test_memory_stats_update() {
        let mut stats = PerformanceStats::new();
        stats.update_memory_stats(1000, 5);
        assert_eq!(stats.total_lines_buffered(), 1000);
        assert_eq!(stats.active_window_count(), 5);
    }

    #[test]
    fn test_parse_time_recording() {
        let mut stats = PerformanceStats::new();

        stats.parse_times.push_back(Duration::from_micros(100));
        stats.parse_times.push_back(Duration::from_micros(200));
        stats.parse_times.push_back(Duration::from_micros(300));

        let avg = stats.avg_parse_time_us();
        assert!((avg - 200.0).abs() < 0.1, "Expected 200us, got {}", avg);
    }

    #[test]
    fn test_uptime_formatted_structure() {
        let stats = PerformanceStats::new();
        let formatted = stats.uptime_formatted();
        assert_eq!(formatted.len(), 8, "HH:MM:SS, got: {}", formatted);
        assert_eq!(&formatted[2..3], ":");
        assert_eq!(&formatted[5..6], ":");
    }

    #[test]
    fn test_sparkline_string_shapes() {
        assert_eq!(sparkline_string(&[], 10), "");
        let flat = sparkline_string(&[1.0; 8], 8);
        assert_eq!(flat.chars().count(), 8);
        // Ramp: last char must be the max block, first the min.
        let ramp: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let s = sparkline_string(&ramp, 8);
        assert_eq!(s.chars().count(), 8);
        assert_eq!(s.chars().last().unwrap(), '█');
        assert_eq!(s.chars().next().unwrap(), '▁');
    }

    #[test]
    fn test_dump_text_contains_sections() {
        let mut stats = PerformanceStats::new();
        stats.record_window_render("main", Duration::from_millis(3));
        stats.record_render_time(Duration::from_millis(42));
        let dump = stats.dump_text(PerfFrontend::Tui);
        assert!(dump.contains("== Metrics =="));
        assert!(dump.contains("== Spike log"));
        assert!(dump.contains("== Window render costs =="));
        assert!(dump.contains("main"));
        assert!(dump.contains("render"));
    }

    #[test]
    fn test_dump_scope_excludes_foreign_metrics() {
        let stats = PerformanceStats::new();
        let dump = stats.dump_text(PerfFrontend::Gui);
        // Wrap and Draw are TUI-only; the GUI dump must not list them.
        assert!(!dump.contains("Wrap"));
        assert!(!dump.lines().any(|l| l.starts_with("Draw ")));
    }

    #[test]
    fn test_metric_ids_unique_and_scoped() {
        let mut seen = std::collections::HashSet::new();
        for metric in PERF_METRICS {
            assert!(seen.insert(metric.id), "duplicate metric id {}", metric.id);
            assert!(
                metric.tui || metric.gui,
                "metric {} belongs to no frontend",
                metric.id
            );
        }
    }

    #[test]
    fn test_every_metric_has_settings_registry_toggle() {
        // The settings registry and the metric table must not drift: every
        // metric row has a ui.perf_show_<id> toggle, and every perf_show
        // toggle corresponds to a metric.
        let registry_keys: Vec<&'static str> = crate::config::registry::registry()
            .iter()
            .map(|def| def.key)
            .filter(|key| key.starts_with("ui.perf_show_"))
            .collect();
        for metric in PERF_METRICS {
            let expected = format!("ui.perf_show_{}", metric.id);
            assert!(
                registry_keys.iter().any(|k| *k == expected),
                "metric '{}' has no settings toggle '{}'",
                metric.id,
                expected
            );
        }
        for key in registry_keys {
            let id = key.trim_start_matches("ui.perf_show_");
            assert!(
                PERF_METRICS.iter().any(|m| m.id == id),
                "settings toggle '{}' has no metric in PERF_METRICS",
                key
            );
        }
    }
}
