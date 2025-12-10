use std::time::{Duration, Instant};
use std::collections::VecDeque;

/// Performance statistics tracker
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    // Frame timing
    frame_times: VecDeque<Duration>,
    last_frame_time: Instant,
    max_frame_samples: usize,

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
    render_times: VecDeque<Duration>,  // Total render time per frame
    ui_render_times: VecDeque<Duration>,  // UI widget render time
    text_wrap_times: VecDeque<Duration>,  // Text wrapping time
    max_render_samples: usize,

    // Event processing
    event_process_times: VecDeque<Duration>,  // Time to process each event
    events_processed: u64,
    max_event_samples: usize,

    // Memory tracking (approximate)
    total_lines_buffered: usize,  // Total lines across all windows
    active_window_count: usize,

    // Element counts
    elements_parsed: u64,  // Total XML elements parsed
    elements_sample_start: Instant,
    elements_parsed_last_second: u64,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceStats {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            frame_times: VecDeque::with_capacity(60),
            last_frame_time: now,
            max_frame_samples: 60,

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

            event_process_times: VecDeque::with_capacity(100),
            events_processed: 0,
            max_event_samples: 100,

            total_lines_buffered: 0,
            active_window_count: 0,

            elements_parsed: 0,
            elements_sample_start: now,
            elements_parsed_last_second: 0,
        }
    }

    /// Record a frame render
    pub fn record_frame(&mut self) {
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
        self.frame_times.iter()
            .min()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get maximum frame time in milliseconds
    pub fn max_frame_time_ms(&self) -> f64 {
        self.frame_times.iter()
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
        self.render_times.push_back(duration);
        if self.render_times.len() > self.max_render_samples {
            self.render_times.pop_front();
        }
    }

    /// Record UI widget render time
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
    }

    /// Update memory tracking stats
    pub fn update_memory_stats(&mut self, total_lines: usize, window_count: usize) {
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
        self.render_times.iter()
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
        self.event_process_times.iter()
            .max()
            .map(|d| d.as_secs_f64() * 1_000_000.0)
            .unwrap_or(0.0)
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
}
