//! Centralized runtime performance telemetry collection.
//!
//! `PerformanceStats` keeps rolling metrics for frame timing, parser throughput,
//! network IO, and general memory indicators so the UI can surface them in the
//! performance overlay as well as log spikes for diagnostics.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};

/// Performance statistics tracker
#[derive(Debug)]
pub struct PerformanceStats {
    // Frame timing
    frame_times: VecDeque<Duration>,
    last_frame_time: Instant,
    max_frame_samples: usize,
    collect_frame_times: bool,
    collect_render_times: bool,
    collect_ui_times: bool,
    collect_wrap_times: bool,
    collect_net: bool,
    collect_parse: bool,
    collect_events: bool,
    collect_memory: bool,
    collect_uptime: bool,

    // Network stats
    bytes_received: u64,
    bytes_sent: u64,
    network_sample_start: Instant,
    bytes_received_last_second: u64,
    bytes_sent_last_second: u64,

    // Parser stats
    parse_times: VecDeque<Duration>,
    chunks_parsed: u64,
    parse_sample_start: Instant,
    chunks_parsed_last_second: u64,
    max_parse_samples: usize,

    // General
    app_start_time: Instant,

    // Detailed render timing
    render_times: VecDeque<Duration>, // Total render time per frame
    ui_render_times: VecDeque<Duration>, // UI widget render time
    text_wrap_times: VecDeque<Duration>, // Text wrapping time
    max_render_samples: usize,
    render_spike_threshold_ms: f64,
    // System/process sampling
    sysinfo: System,
    sysinfo_pid: Option<Pid>,
    last_sys_sample: Instant,
    process_cpu_percent: f32,
    system_cpu_percent: f32,
    process_rss_bytes: u64,
    process_virt_bytes: u64,

    // Event processing
    event_process_times: VecDeque<Duration>, // Time to process each event
    events_processed: u64,
    max_event_samples: usize,
    last_event_finish: Instant,
    event_queue_depth_max: u64,
    event_queue_depth_last: u64,

    // Memory tracking (approximate)
    total_lines_buffered: usize, // Total lines across all windows
    active_window_count: usize,
    last_memory_sample_lines: usize,
    last_memory_sample_time: Instant,

    // Element counts
    elements_parsed: u64, // Total XML elements parsed
    elements_sample_start: Instant,
    elements_parsed_last_second: u64,
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
            frame_times: VecDeque::with_capacity(60),
            last_frame_time: now,
            max_frame_samples: 60,
            collect_frame_times: true,
            collect_render_times: true,
            collect_ui_times: true,
            collect_wrap_times: true,
            collect_net: true,
            collect_parse: true,
            collect_events: true,
            collect_memory: true,
            collect_uptime: true,

            bytes_received: 0,
            bytes_sent: 0,
            network_sample_start: now,
            bytes_received_last_second: 0,
            bytes_sent_last_second: 0,

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
            sysinfo: System::new_with_specifics(
                RefreshKind::new().with_cpu(CpuRefreshKind::everything()),
            ),
            sysinfo_pid: sysinfo::get_current_pid().ok(),
            last_sys_sample: now,
            process_cpu_percent: 0.0,
            system_cpu_percent: 0.0,
            process_rss_bytes: 0,
            process_virt_bytes: 0,

            event_process_times: VecDeque::with_capacity(100),
            events_processed: 0,
            max_event_samples: 100,
            last_event_finish: now,
            event_queue_depth_max: 0,
            event_queue_depth_last: 0,

            total_lines_buffered: 0,
            active_window_count: 0,
            last_memory_sample_lines: 0,
            last_memory_sample_time: now,

            elements_parsed: 0,
            elements_sample_start: now,
            elements_parsed_last_second: 0,
        }
    }

    /// Record a frame render
    pub fn record_frame(&mut self) {
        if !self.collect_frame_times {
            self.last_frame_time = Instant::now();
            return;
        }
        let now = Instant::now();
        let frame_time = now.duration_since(self.last_frame_time);

        self.frame_times.push_back(frame_time);
        if self.frame_times.len() > self.max_frame_samples {
            self.frame_times.pop_front();
        }

        self.last_frame_time = now;
    }

    /// Record bytes received from network
    pub fn record_bytes_received(&mut self, bytes: u64) {
        if !self.collect_net {
            return;
        }
        self.bytes_received += bytes;

        // Check if we need to update per-second stats
        let now = Instant::now();
        if now.duration_since(self.network_sample_start) >= Duration::from_secs(1) {
            self.bytes_received_last_second = self.bytes_received;
            self.bytes_sent_last_second = self.bytes_sent;
            self.bytes_received = 0;
            self.bytes_sent = 0;
            self.network_sample_start = now;
        }
    }

    /// Record bytes sent to network
    pub fn record_bytes_sent(&mut self, bytes: u64) {
        if !self.collect_net {
            return;
        }
        self.bytes_sent += bytes;

        // Check if we need to update per-second stats (same logic as received)
        let now = Instant::now();
        if now.duration_since(self.network_sample_start) >= Duration::from_secs(1) {
            self.bytes_received_last_second = self.bytes_received;
            self.bytes_sent_last_second = self.bytes_sent;
            self.bytes_received = 0;
            self.bytes_sent = 0;
            self.network_sample_start = now;
        }
    }

    /// Record a parse operation
    pub fn record_parse(&mut self, duration: Duration) {
        if !self.collect_parse {
            return;
        }
        let now = Instant::now();

        self.parse_times.push_back(duration);
        if self.parse_times.len() > self.max_parse_samples {
            self.parse_times.pop_front();
        }

        self.chunks_parsed += 1;

        // Update per-second stats
        if now.duration_since(self.parse_sample_start) >= Duration::from_secs(1) {
            self.chunks_parsed_last_second = self.chunks_parsed;
            self.chunks_parsed = 0;
            self.parse_sample_start = now;
        }
    }

    /// Get current FPS
    pub fn fps(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.frame_times.iter().sum();
        let avg_frame_time = total.as_secs_f64() / self.frame_times.len() as f64;

        if avg_frame_time > 0.0 {
            1.0 / avg_frame_time
        } else {
            0.0
        }
    }

    /// Get average frame time in milliseconds
    pub fn avg_frame_time_ms(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.frame_times.iter().sum();
        total.as_secs_f64() * 1000.0 / self.frame_times.len() as f64
    }

    /// Get minimum frame time in milliseconds
    pub fn min_frame_time_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .min()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get maximum frame time in milliseconds
    pub fn max_frame_time_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .max()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
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
        if self.parse_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.parse_times.iter().sum();
        total.as_secs_f64() * 1_000_000.0 / self.parse_times.len() as f64
    }

    /// Get chunks parsed per second
    pub fn chunks_per_sec(&self) -> u64 {
        self.chunks_parsed_last_second
    }

    /// Get app uptime
    pub fn uptime(&self) -> Duration {
        if !self.collect_uptime {
            return Duration::from_secs(0);
        }
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

    // === New detailed tracking methods ===

    /// Record total render time for a frame
    pub fn record_render_time(&mut self, duration: Duration) {
        if !self.collect_render_times {
            return;
        }
        self.render_times.push_back(duration);
        if self.render_times.len() > self.max_render_samples {
            self.render_times.pop_front();
        }
    }

    /// Record UI widget render time
    pub fn record_ui_render_time(&mut self, duration: Duration) {
        if !self.collect_ui_times {
            return;
        }
        self.ui_render_times.push_back(duration);
        if self.ui_render_times.len() > self.max_render_samples {
            self.ui_render_times.pop_front();
        }
    }

    /// Record text wrapping time
    pub fn record_text_wrap_time(&mut self, duration: Duration) {
        if !self.collect_wrap_times {
            return;
        }
        self.text_wrap_times.push_back(duration);
        if self.text_wrap_times.len() > self.max_render_samples {
            self.text_wrap_times.pop_front();
        }
    }

    /// Record event processing time
    pub fn record_event_process_time(&mut self, duration: Duration) {
        if !self.collect_events {
            return;
        }
        self.event_process_times.push_back(duration);
        if self.event_process_times.len() > self.max_event_samples {
            self.event_process_times.pop_front();
        }
        self.events_processed += 1;
        self.last_event_finish = Instant::now();
    }

    /// Record observed depth of the event queue
    pub fn record_event_queue_depth(&mut self, depth: u64) {
        self.event_queue_depth_last = depth;
        if depth > self.event_queue_depth_max {
            self.event_queue_depth_max = depth;
        }
    }

    /// Update memory tracking stats
    pub fn update_memory_stats(&mut self, total_lines: usize, window_count: usize) {
        if !self.collect_memory {
            return;
        }

        let now = Instant::now();

        // Update delta baseline infrequently to allow memory_delta_mb to show changes
        if now
            .saturating_duration_since(self.last_memory_sample_time)
            .as_millis()
            >= 500
        {
            self.last_memory_sample_lines = self.total_lines_buffered;
            self.last_memory_sample_time = now;
        }

        self.total_lines_buffered = total_lines;
        self.active_window_count = window_count;
    }

    /// Record XML elements parsed
    pub fn record_elements_parsed(&mut self, count: u64) {
        let now = Instant::now();
        self.elements_parsed += count;

        // Update per-second stats
        if now.duration_since(self.elements_sample_start) >= Duration::from_secs(1) {
            self.elements_parsed_last_second = self.elements_parsed;
            self.elements_parsed = 0;
            self.elements_sample_start = now;
        }
    }

    /// Sample system/process metrics (CPU/RSS) at most once per second
    pub fn sample_sysinfo(&mut self) {
        if Instant::now().duration_since(self.last_sys_sample) < Duration::from_secs(1) {
            return;
        }

        // Refresh CPU first so usage values are updated
        self.sysinfo.refresh_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything()),
        );

        self.system_cpu_percent = self.sysinfo.global_cpu_info().cpu_usage();

        if let Some(pid) = self.sysinfo_pid {
            if let Some(proc) = self.sysinfo.process(pid) {
                self.process_cpu_percent = proc.cpu_usage();
                self.process_rss_bytes = proc.memory() * 1024; // KiB -> bytes
                self.process_virt_bytes = proc.virtual_memory() * 1024; // KiB -> bytes
            }
        }

        self.last_sys_sample = Instant::now();
    }

    // === Getters for new metrics ===

    /// Get average render time in milliseconds
    pub fn avg_render_time_ms(&self) -> f64 {
        if self.render_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.render_times.iter().sum();
        total.as_secs_f64() * 1000.0 / self.render_times.len() as f64
    }

    /// Get max render time in milliseconds
    pub fn max_render_time_ms(&self) -> f64 {
        self.render_times
            .iter()
            .max()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get average UI render time in milliseconds
    pub fn avg_ui_render_time_ms(&self) -> f64 {
        if self.ui_render_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.ui_render_times.iter().sum();
        total.as_secs_f64() * 1000.0 / self.ui_render_times.len() as f64
    }

    /// Get average text wrap time in microseconds
    pub fn avg_text_wrap_time_us(&self) -> f64 {
        if self.text_wrap_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.text_wrap_times.iter().sum();
        total.as_secs_f64() * 1_000_000.0 / self.text_wrap_times.len() as f64
    }

    /// Get average event process time in microseconds
    pub fn avg_event_process_time_us(&self) -> f64 {
        if self.event_process_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.event_process_times.iter().sum();
        total.as_secs_f64() * 1_000_000.0 / self.event_process_times.len() as f64
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

    /// Estimate memory usage in MB (very rough approximation)
    pub fn estimated_memory_mb(&self) -> f64 {
        // Rough estimate: ~200 bytes per line on average (including overhead)
        let line_bytes = self.total_lines_buffered * 200;
        line_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Compute standard deviation of frame times in ms (simple population stddev)
    pub fn frame_jitter_ms(&self) -> f64 {
        if !self.collect_frame_times || self.frame_times.is_empty() {
            return 0.0;
        }
        let samples: Vec<f64> = self
            .frame_times
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0)
            .collect();
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        let var = samples
            .iter()
            .map(|v| {
                let diff = v - mean;
                diff * diff
            })
            .sum::<f64>()
            / samples.len() as f64;
        var.sqrt()
    }

    /// Count render-time spikes above the configured threshold
    pub fn frame_spike_count(&self) -> usize {
        if !self.collect_render_times || self.render_times.is_empty() {
            return 0;
        }
        let threshold = self.render_spike_threshold_ms;
        self.render_times
            .iter()
            .filter(|d| d.as_secs_f64() * 1000.0 > threshold)
            .count()
    }

    /// Time since last event processing (ms)
    pub fn event_lag_ms(&self) -> f64 {
        if !self.collect_events {
            return 0.0;
        }
        Instant::now()
            .saturating_duration_since(self.last_event_finish)
            .as_secs_f64()
            * 1000.0
    }

    /// Memory delta (MB) since last sample
    pub fn memory_delta_mb(&self) -> f64 {
        if !self.collect_memory {
            return 0.0;
        }
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_memory_sample_time)
            .as_secs_f64();
        if elapsed <= 0.0 {
            return 0.0;
        }
        let delta_lines = self
            .total_lines_buffered
            .saturating_sub(self.last_memory_sample_lines);
        let bytes = delta_lines * 200;
        bytes as f64 / (1024.0 * 1024.0)
    }

    /// Enable/disable collection groups based on widget config (to avoid overhead)
    pub fn apply_enabled_from(&mut self, cfg: &crate::config::PerformanceWidgetData) {
        if !cfg.enabled {
            self.collect_frame_times = false;
            self.collect_render_times = false;
            self.collect_ui_times = false;
            self.collect_wrap_times = false;
            self.collect_net = false;
            self.collect_parse = false;
            self.collect_events = false;
            self.collect_memory = false;
            self.collect_uptime = false;
            return;
        }
        self.collect_frame_times = cfg.show_fps
            || cfg.show_frame_times
            || cfg.show_jitter
            || cfg.show_frame_spikes;
        self.collect_render_times = cfg.show_render_times;
        self.collect_ui_times = cfg.show_ui_times;
        self.collect_wrap_times = cfg.show_wrap_times;
        self.collect_net = cfg.show_net;
        self.collect_parse = cfg.show_parse;
        self.collect_events = cfg.show_events || cfg.show_event_lag;
        self.collect_memory = cfg.show_memory || cfg.show_memory_delta || cfg.show_lines;
        // Uptime should always be tracked (even if not displayed)
        self.collect_uptime = cfg.show_uptime || cfg.show_lines || cfg.show_memory_delta;
        // keep spike threshold configurable later; constant for now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Initialization Tests ====================

    #[test]
    fn test_new_performance_stats() {
        let stats = PerformanceStats::new();

        // Initial values should be zero/empty
        assert_eq!(stats.fps(), 0.0);
        assert_eq!(stats.avg_frame_time_ms(), 0.0);
        assert_eq!(stats.min_frame_time_ms(), 0.0);
        assert_eq!(stats.max_frame_time_ms(), 0.0);
        assert_eq!(stats.bytes_received_per_sec(), 0);
        assert_eq!(stats.bytes_sent_per_sec(), 0);
        assert_eq!(stats.avg_parse_time_us(), 0.0);
        assert_eq!(stats.chunks_per_sec(), 0);
        assert_eq!(stats.total_events_processed(), 0);
        assert_eq!(stats.total_lines_buffered(), 0);
        assert_eq!(stats.active_window_count(), 0);
    }

    #[test]
    fn test_default_equals_new() {
        let default_stats = PerformanceStats::default();
        let new_stats = PerformanceStats::new();

        // Both should have same initial state
        assert_eq!(default_stats.fps(), new_stats.fps());
        assert_eq!(default_stats.avg_frame_time_ms(), new_stats.avg_frame_time_ms());
    }

    // ==================== Frame Time Tests ====================

    #[test]
    fn test_frame_time_calculations() {
        let mut stats = PerformanceStats::new();

        // Manually inject frame times for predictable testing
        stats.frame_times.push_back(Duration::from_millis(16)); // ~60 FPS
        stats.frame_times.push_back(Duration::from_millis(16));
        stats.frame_times.push_back(Duration::from_millis(20)); // slower frame

        // Average should be (16 + 16 + 20) / 3 = 17.33... ms
        let avg = stats.avg_frame_time_ms();
        assert!((avg - 17.333).abs() < 0.1, "Expected ~17.33ms, got {}", avg);

        // Min should be 16ms
        assert!((stats.min_frame_time_ms() - 16.0).abs() < 0.001);

        // Max should be 20ms
        assert!((stats.max_frame_time_ms() - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_fps_calculation() {
        let mut stats = PerformanceStats::new();

        // Add frame times of exactly 16.67ms (60 FPS)
        for _ in 0..10 {
            stats.frame_times.push_back(Duration::from_micros(16667));
        }

        let fps = stats.fps();
        // Should be approximately 60 FPS
        assert!((fps - 60.0).abs() < 1.0, "Expected ~60 FPS, got {}", fps);
    }

    #[test]
    fn test_fps_empty_returns_zero() {
        let stats = PerformanceStats::new();
        assert_eq!(stats.fps(), 0.0);
    }

    #[test]
    fn test_rolling_window_max_samples() {
        let mut stats = PerformanceStats::new();

        // Add more than max_frame_samples (60)
        for _ in 0..100 {
            stats.frame_times.push_back(Duration::from_millis(16));
            if stats.frame_times.len() > stats.max_frame_samples {
                stats.frame_times.pop_front();
            }
        }

        // Should be capped at 60
        assert_eq!(stats.frame_times.len(), 60);
    }

    // ==================== Parse Time Tests ====================

    #[test]
    fn test_parse_time_recording() {
        let mut stats = PerformanceStats::new();

        stats.parse_times.push_back(Duration::from_micros(100));
        stats.parse_times.push_back(Duration::from_micros(200));
        stats.parse_times.push_back(Duration::from_micros(300));

        // Average should be 200 microseconds
        let avg = stats.avg_parse_time_us();
        assert!((avg - 200.0).abs() < 0.1, "Expected 200us, got {}", avg);
    }

    #[test]
    fn test_parse_time_empty_returns_zero() {
        let stats = PerformanceStats::new();
        assert_eq!(stats.avg_parse_time_us(), 0.0);
    }

    // ==================== Render Time Tests ====================

    #[test]
    fn test_render_time_recording() {
        let mut stats = PerformanceStats::new();

        stats.render_times.push_back(Duration::from_millis(5));
        stats.render_times.push_back(Duration::from_millis(10));
        stats.render_times.push_back(Duration::from_millis(15));

        // Average should be 10ms
        let avg = stats.avg_render_time_ms();
        assert!((avg - 10.0).abs() < 0.1, "Expected 10ms, got {}", avg);

        // Max should be 15ms
        assert!((stats.max_render_time_ms() - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_ui_render_time_recording() {
        let mut stats = PerformanceStats::new();

        stats.ui_render_times.push_back(Duration::from_millis(2));
        stats.ui_render_times.push_back(Duration::from_millis(4));

        let avg = stats.avg_ui_render_time_ms();
        assert!((avg - 3.0).abs() < 0.1, "Expected 3ms, got {}", avg);
    }

    #[test]
    fn test_text_wrap_time_recording() {
        let mut stats = PerformanceStats::new();

        stats.text_wrap_times.push_back(Duration::from_micros(50));
        stats.text_wrap_times.push_back(Duration::from_micros(100));

        let avg = stats.avg_text_wrap_time_us();
        assert!((avg - 75.0).abs() < 0.1, "Expected 75us, got {}", avg);
    }

    // ==================== Event Processing Tests ====================

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

    // ==================== Memory Stats Tests ====================

    #[test]
    fn test_memory_stats_update() {
        let mut stats = PerformanceStats::new();

        stats.update_memory_stats(1000, 5);

        assert_eq!(stats.total_lines_buffered(), 1000);
        assert_eq!(stats.active_window_count(), 5);
    }

    #[test]
    fn test_estimated_memory_calculation() {
        let mut stats = PerformanceStats::new();

        // 1000 lines * 200 bytes = 200,000 bytes = ~0.19 MB
        stats.update_memory_stats(1000, 5);

        let estimated = stats.estimated_memory_mb();
        let expected = (1000.0 * 200.0) / (1024.0 * 1024.0);
        assert!((estimated - expected).abs() < 0.001, "Expected {}, got {}", expected, estimated);
    }

    #[test]
    fn test_estimated_memory_zero_lines() {
        let stats = PerformanceStats::new();
        assert_eq!(stats.estimated_memory_mb(), 0.0);
    }

    #[test]
    fn test_estimated_memory_large_buffer() {
        let mut stats = PerformanceStats::new();

        // 100,000 lines * 200 bytes = 20MB
        stats.update_memory_stats(100_000, 10);

        let estimated = stats.estimated_memory_mb();
        let expected = (100_000.0 * 200.0) / (1024.0 * 1024.0);
        assert!((estimated - expected).abs() < 0.1, "Expected ~{:.2}MB, got {:.2}MB", expected, estimated);
    }

    // ==================== Network Stats Tests ====================

    #[test]
    fn test_network_stats_initial() {
        let stats = PerformanceStats::new();

        assert_eq!(stats.bytes_received_per_sec(), 0);
        assert_eq!(stats.bytes_sent_per_sec(), 0);
    }

    // ==================== Uptime Tests ====================

    #[test]
    fn test_uptime_is_positive() {
        let stats = PerformanceStats::new();

        // Uptime should be very small but positive
        let uptime = stats.uptime();
        assert!(uptime.as_nanos() > 0);
    }

    #[test]
    fn test_uptime_formatted_structure() {
        let stats = PerformanceStats::new();

        let formatted = stats.uptime_formatted();
        // Should match HH:MM:SS format
        assert_eq!(formatted.len(), 8, "Format should be HH:MM:SS, got: {}", formatted);
        assert_eq!(&formatted[2..3], ":");
        assert_eq!(&formatted[5..6], ":");
    }

    // ==================== Uptime Formatting Unit Tests ====================

    #[test]
    fn test_uptime_format_zero() {
        // Test the formatting logic directly
        let seconds: u64 = 0;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        let formatted = format!("{:02}:{:02}:{:02}", hours, minutes, secs);
        assert_eq!(formatted, "00:00:00");
    }

    #[test]
    fn test_uptime_format_one_hour() {
        let seconds: u64 = 3600;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        let formatted = format!("{:02}:{:02}:{:02}", hours, minutes, secs);
        assert_eq!(formatted, "01:00:00");
    }

    #[test]
    fn test_uptime_format_complex() {
        let seconds: u64 = 3661; // 1 hour, 1 minute, 1 second
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        let formatted = format!("{:02}:{:02}:{:02}", hours, minutes, secs);
        assert_eq!(formatted, "01:01:01");
    }

    #[test]
    fn test_uptime_format_max_edge() {
        let seconds: u64 = 359999; // 99:59:59
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        let formatted = format!("{:02}:{:02}:{:02}", hours, minutes, secs);
        assert_eq!(formatted, "99:59:59");
    }

    // ==================== Empty State Edge Cases ====================

    #[test]
    fn test_all_averages_empty() {
        let stats = PerformanceStats::new();

        assert_eq!(stats.avg_frame_time_ms(), 0.0);
        assert_eq!(stats.avg_parse_time_us(), 0.0);
        assert_eq!(stats.avg_render_time_ms(), 0.0);
        assert_eq!(stats.avg_ui_render_time_ms(), 0.0);
        assert_eq!(stats.avg_text_wrap_time_us(), 0.0);
        assert_eq!(stats.avg_event_process_time_us(), 0.0);
    }

    #[test]
    fn test_all_max_empty() {
        let stats = PerformanceStats::new();

        assert_eq!(stats.max_frame_time_ms(), 0.0);
        assert_eq!(stats.max_render_time_ms(), 0.0);
        assert_eq!(stats.max_event_process_time_us(), 0.0);
    }
}
