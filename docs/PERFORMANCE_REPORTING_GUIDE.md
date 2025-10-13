# VellumFE Performance Reporting - Implementation Guide

## Overview

This guide provides step-by-step instructions for implementing performance reporting features in VellumFE. It covers exporting metrics, creating reports, building dashboards, and integrating with external monitoring systems.

## Target Audience

- Backend developers implementing metric exporters
- DevOps engineers setting up monitoring infrastructure
- QA engineers creating performance test suites
- Contributors adding new performance instrumentation

## Current State

VellumFE includes a complete performance metrics collection system (`src/performance.rs`) and real-time visualization widget (`src/ui/performance_stats.rs`). This guide describes how to extend these systems with reporting capabilities.

## Implementation Roadmap

### Phase 1: Metric Snapshot API (30 minutes)

Create a serializable snapshot of all current metrics for export.

#### Step 1: Add Serde Dependencies

Edit `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"  # For JSON export
```

#### Step 2: Create Metrics Snapshot Structure

Add to `src/performance.rs`:

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    // Timestamp
    pub timestamp: u64,  // Unix timestamp in seconds
    pub uptime_seconds: u64,

    // Frame metrics
    pub fps: f64,
    pub avg_frame_time_ms: f64,
    pub min_frame_time_ms: f64,
    pub max_frame_time_ms: f64,

    // Render metrics
    pub avg_render_time_ms: f64,
    pub max_render_time_ms: f64,
    pub avg_ui_render_time_ms: f64,
    pub avg_text_wrap_time_us: f64,

    // Network metrics
    pub bytes_received_per_sec: u64,
    pub bytes_sent_per_sec: u64,

    // Parser metrics
    pub avg_parse_time_us: f64,
    pub chunks_per_sec: u64,
    pub elements_per_sec: u64,

    // Event metrics
    pub avg_event_process_time_us: f64,
    pub max_event_process_time_us: f64,
    pub total_events_processed: u64,

    // Memory metrics
    pub total_lines_buffered: usize,
    pub active_window_count: usize,
    pub estimated_memory_mb: f64,
}

impl PerformanceStats {
    /// Create a snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            uptime_seconds: self.uptime().as_secs(),

            fps: self.fps(),
            avg_frame_time_ms: self.avg_frame_time_ms(),
            min_frame_time_ms: self.min_frame_time_ms(),
            max_frame_time_ms: self.max_frame_time_ms(),

            avg_render_time_ms: self.avg_render_time_ms(),
            max_render_time_ms: self.max_render_time_ms(),
            avg_ui_render_time_ms: self.avg_ui_render_time_ms(),
            avg_text_wrap_time_us: self.avg_text_wrap_time_us(),

            bytes_received_per_sec: self.bytes_received_per_sec(),
            bytes_sent_per_sec: self.bytes_sent_per_sec(),

            avg_parse_time_us: self.avg_parse_time_us(),
            chunks_per_sec: self.chunks_per_sec(),
            elements_per_sec: self.elements_per_sec(),

            avg_event_process_time_us: self.avg_event_process_time_us(),
            max_event_process_time_us: self.max_event_process_time_us(),
            total_events_processed: self.total_events_processed(),

            total_lines_buffered: self.total_lines_buffered(),
            active_window_count: self.active_window_count(),
            estimated_memory_mb: self.estimated_memory_mb(),
        }
    }
}
```

#### Step 3: Test Snapshot Creation

Add a dot command to `src/app.rs` in `handle_dot_command()`:

```rust
"snapshot" | "perfsnap" => {
    let snapshot = self.perf_stats.snapshot();
    let json = serde_json::to_string_pretty(&snapshot).unwrap();
    self.add_system_message(&format!("Performance Snapshot:\n{}", json));
}
```

**Usage**: Type `.snapshot` in VellumFE to see JSON metrics output.

### Phase 2: Metrics Logging (1 hour)

Implement periodic metrics logging to file for post-session analysis.

#### Step 1: Create Metrics Logger

Create `src/metrics_logger.rs`:

```rust
use crate::performance::{PerformanceStats, MetricsSnapshot};
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::interval;

pub struct MetricsLogger {
    log_path: PathBuf,
    interval_seconds: u64,
}

impl MetricsLogger {
    pub fn new(log_path: PathBuf, interval_seconds: u64) -> Self {
        Self {
            log_path,
            interval_seconds,
        }
    }

    /// Start logging metrics at specified interval
    pub async fn start(
        &self,
        mut perf_stats_rx: tokio::sync::watch::Receiver<MetricsSnapshot>,
    ) -> Result<()> {
        let mut interval = interval(Duration::from_secs(self.interval_seconds));

        // Create/open log file with append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        // Write CSV header if file is empty
        let metadata = std::fs::metadata(&self.log_path)?;
        if metadata.len() == 0 {
            writeln!(
                file,
                "timestamp,uptime_s,fps,frame_ms,render_ms,parse_us,\
                 bytes_in,bytes_out,chunks_s,elements_s,events,lines,windows,memory_mb"
            )?;
        }

        loop {
            interval.tick().await;

            // Get latest snapshot
            let snapshot = perf_stats_rx.borrow_and_update().clone();

            // Write CSV row
            writeln!(
                file,
                "{},{},{:.1},{:.2},{:.2},{:.0},{},{},{},{},{},{},{},{:.1}",
                snapshot.timestamp,
                snapshot.uptime_seconds,
                snapshot.fps,
                snapshot.avg_frame_time_ms,
                snapshot.avg_render_time_ms,
                snapshot.avg_parse_time_us,
                snapshot.bytes_received_per_sec,
                snapshot.bytes_sent_per_sec,
                snapshot.chunks_per_sec,
                snapshot.elements_per_sec,
                snapshot.total_events_processed,
                snapshot.total_lines_buffered,
                snapshot.active_window_count,
                snapshot.estimated_memory_mb,
            )?;
            file.flush()?;
        }
    }
}
```

#### Step 2: Add Logger to Config

Edit `src/config.rs` to add metrics logging config:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable metrics logging
    #[serde(default)]
    pub enable_metrics_logging: bool,

    /// Metrics log interval in seconds
    #[serde(default = "default_metrics_interval")]
    pub metrics_log_interval: u64,

    /// Metrics log file path (relative to ~/.vellum-fe/)
    #[serde(default = "default_metrics_path")]
    pub metrics_log_path: String,
}

fn default_metrics_interval() -> u64 { 10 }
fn default_metrics_path() -> String { "metrics.csv".to_string() }

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_metrics_logging: false,
            metrics_log_interval: 10,
            metrics_log_path: "metrics.csv".to_string(),
        }
    }
}

// Add to Config struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ... existing fields ...
    #[serde(default)]
    pub performance: PerformanceConfig,
}
```

#### Step 3: Integrate Logger into App

Edit `src/app.rs`:

```rust
use tokio::sync::watch;

pub struct App {
    // ... existing fields ...
    metrics_snapshot_tx: watch::Sender<MetricsSnapshot>,
    metrics_snapshot_rx: watch::Receiver<MetricsSnapshot>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        // ... existing initialization ...

        // Create metrics snapshot channel
        let initial_snapshot = perf_stats.snapshot();
        let (metrics_snapshot_tx, metrics_snapshot_rx) = watch::channel(initial_snapshot);

        // ... rest of initialization ...

        Ok(Self {
            // ... existing fields ...
            metrics_snapshot_tx,
            metrics_snapshot_rx,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // ... existing setup ...

        // Spawn metrics logger if enabled
        if self.config.performance.enable_metrics_logging {
            let log_path = Config::get_base_dir()?.join(&self.config.performance.metrics_log_path);
            let logger = MetricsLogger::new(
                log_path,
                self.config.performance.metrics_log_interval,
            );
            let rx = self.metrics_snapshot_rx.clone();
            tokio::spawn(async move {
                if let Err(e) = logger.start(rx).await {
                    tracing::error!("Metrics logger error: {}", e);
                }
            });
        }

        // In main event loop, update snapshot periodically:
        // (Add this where perf stats are updated)
        if self.frame_count % 60 == 0 {  // Every 60 frames (~1 second)
            let snapshot = self.perf_stats.snapshot();
            let _ = self.metrics_snapshot_tx.send(snapshot);
        }

        // ... existing event loop ...
    }
}
```

#### Step 4: Enable in Config

Add to `~/.vellum-fe/configs/default.toml`:

```toml
[performance]
enable_metrics_logging = true
metrics_log_interval = 10  # Log every 10 seconds
metrics_log_path = "metrics.csv"
```

**Result**: Metrics logged to `~/.vellum-fe/metrics.csv` every 10 seconds.

### Phase 3: Prometheus Exporter (2 hours)

Expose metrics in Prometheus format for integration with Grafana dashboards.

#### Step 1: Add Prometheus Dependencies

Edit `Cargo.toml`:

```toml
[dependencies]
prometheus = "0.13"
prometheus-hyper = "0.1"  # For HTTP server
hyper = { version = "0.14", features = ["full"] }
```

#### Step 2: Create Prometheus Exporter

Create `src/prometheus_exporter.rs`:

```rust
use crate::performance::MetricsSnapshot;
use anyhow::Result;
use prometheus::{Encoder, Registry, TextEncoder};
use prometheus::{Gauge, IntCounter, IntGauge};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct PrometheusExporter {
    registry: Registry,

    // Gauges (float values)
    fps: Gauge,
    frame_time_ms: Gauge,
    render_time_ms: Gauge,
    parse_time_us: Gauge,
    event_time_us: Gauge,
    memory_mb: Gauge,

    // Integer gauges
    bytes_received_per_sec: IntGauge,
    bytes_sent_per_sec: IntGauge,
    chunks_per_sec: IntGauge,
    elements_per_sec: IntGauge,
    lines_buffered: IntGauge,
    window_count: IntGauge,

    // Counters
    total_events: IntCounter,
}

impl PrometheusExporter {
    pub fn new() -> Result<Self> {
        let registry = Registry::new();

        // Create metrics
        let fps = Gauge::new("vellum_fps", "Frames per second")?;
        let frame_time_ms = Gauge::new("vellum_frame_time_milliseconds", "Average frame time")?;
        let render_time_ms = Gauge::new("vellum_render_time_milliseconds", "Average render time")?;
        let parse_time_us = Gauge::new("vellum_parse_time_microseconds", "Average parse time")?;
        let event_time_us = Gauge::new("vellum_event_time_microseconds", "Average event time")?;
        let memory_mb = Gauge::new("vellum_memory_megabytes", "Estimated memory usage")?;

        let bytes_received_per_sec = IntGauge::new("vellum_bytes_received_per_second", "Bytes received per second")?;
        let bytes_sent_per_sec = IntGauge::new("vellum_bytes_sent_per_second", "Bytes sent per second")?;
        let chunks_per_sec = IntGauge::new("vellum_chunks_per_second", "Chunks parsed per second")?;
        let elements_per_sec = IntGauge::new("vellum_elements_per_second", "Elements parsed per second")?;
        let lines_buffered = IntGauge::new("vellum_lines_buffered", "Total lines in buffers")?;
        let window_count = IntGauge::new("vellum_windows", "Active window count")?;

        let total_events = IntCounter::new("vellum_events_total", "Total events processed")?;

        // Register metrics
        registry.register(Box::new(fps.clone()))?;
        registry.register(Box::new(frame_time_ms.clone()))?;
        registry.register(Box::new(render_time_ms.clone()))?;
        registry.register(Box::new(parse_time_us.clone()))?;
        registry.register(Box::new(event_time_us.clone()))?;
        registry.register(Box::new(memory_mb.clone()))?;
        registry.register(Box::new(bytes_received_per_sec.clone()))?;
        registry.register(Box::new(bytes_sent_per_sec.clone()))?;
        registry.register(Box::new(chunks_per_sec.clone()))?;
        registry.register(Box::new(elements_per_sec.clone()))?;
        registry.register(Box::new(lines_buffered.clone()))?;
        registry.register(Box::new(window_count.clone()))?;
        registry.register(Box::new(total_events.clone()))?;

        Ok(Self {
            registry,
            fps,
            frame_time_ms,
            render_time_ms,
            parse_time_us,
            event_time_us,
            memory_mb,
            bytes_received_per_sec,
            bytes_sent_per_sec,
            chunks_per_sec,
            elements_per_sec,
            lines_buffered,
            window_count,
            total_events,
        })
    }

    /// Update metrics from snapshot
    pub fn update(&self, snapshot: &MetricsSnapshot) {
        self.fps.set(snapshot.fps);
        self.frame_time_ms.set(snapshot.avg_frame_time_ms);
        self.render_time_ms.set(snapshot.avg_render_time_ms);
        self.parse_time_us.set(snapshot.avg_parse_time_us);
        self.event_time_us.set(snapshot.avg_event_process_time_us);
        self.memory_mb.set(snapshot.estimated_memory_mb);

        self.bytes_received_per_sec.set(snapshot.bytes_received_per_sec as i64);
        self.bytes_sent_per_sec.set(snapshot.bytes_sent_per_sec as i64);
        self.chunks_per_sec.set(snapshot.chunks_per_sec as i64);
        self.elements_per_sec.set(snapshot.elements_per_sec as i64);
        self.lines_buffered.set(snapshot.total_lines_buffered as i64);
        self.window_count.set(snapshot.active_window_count as i64);

        // Note: Prometheus counters can only increase, so we just set to current value
        // In reality, we'd need to track delta and call inc_by()
        // For now, expose as gauge-style metric
    }

    /// Render metrics in Prometheus text format
    pub fn render(&self) -> Result<Vec<u8>> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(buffer)
    }

    /// Start HTTP server to expose metrics
    pub async fn serve(
        exporter: Arc<Mutex<PrometheusExporter>>,
        mut snapshot_rx: tokio::sync::watch::Receiver<MetricsSnapshot>,
        port: u16,
    ) -> Result<()> {
        use hyper::service::{make_service_fn, service_fn};
        use hyper::{Body, Request, Response, Server};

        // Spawn task to update metrics from snapshots
        let exporter_update = exporter.clone();
        tokio::spawn(async move {
            loop {
                if snapshot_rx.changed().await.is_ok() {
                    let snapshot = snapshot_rx.borrow().clone();
                    let exporter = exporter_update.lock().await;
                    exporter.update(&snapshot);
                }
            }
        });

        // HTTP server handler
        let make_svc = make_service_fn(move |_conn| {
            let exporter = exporter.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |_req: Request<Body>| {
                    let exporter = exporter.clone();
                    async move {
                        let exporter = exporter.lock().await;
                        let metrics = exporter.render().unwrap_or_default();
                        Ok::<_, hyper::Error>(Response::new(Body::from(metrics)))
                    }
                }))
            }
        });

        let addr = ([127, 0, 0, 1], port).into();
        let server = Server::bind(&addr).serve(make_svc);

        tracing::info!("Prometheus metrics server listening on http://{}/metrics", addr);

        server.await?;
        Ok(())
    }
}
```

#### Step 3: Add Exporter Config

Edit `src/config.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    // ... existing fields ...

    /// Enable Prometheus exporter
    #[serde(default)]
    pub enable_prometheus: bool,

    /// Prometheus HTTP server port
    #[serde(default = "default_prometheus_port")]
    pub prometheus_port: u16,
}

fn default_prometheus_port() -> u16 { 9090 }
```

#### Step 4: Integrate into App

Edit `src/app.rs`:

```rust
impl App {
    pub async fn run(&mut self) -> Result<()> {
        // ... existing setup ...

        // Spawn Prometheus exporter if enabled
        if self.config.performance.enable_prometheus {
            let exporter = Arc::new(Mutex::new(PrometheusExporter::new()?));
            let rx = self.metrics_snapshot_rx.clone();
            let port = self.config.performance.prometheus_port;
            tokio::spawn(async move {
                if let Err(e) = PrometheusExporter::serve(exporter, rx, port).await {
                    tracing::error!("Prometheus exporter error: {}", e);
                }
            });
        }

        // ... rest of event loop ...
    }
}
```

#### Step 5: Test Prometheus Export

Enable in config:
```toml
[performance]
enable_prometheus = true
prometheus_port = 9090
```

Verify output:
```bash
curl http://localhost:9090/metrics
```

Expected output:
```
# HELP vellum_fps Frames per second
# TYPE vellum_fps gauge
vellum_fps 60.0
# HELP vellum_frame_time_milliseconds Average frame time
# TYPE vellum_frame_time_milliseconds gauge
vellum_frame_time_milliseconds 16.67
...
```

### Phase 4: Performance Reports (1 hour)

Generate human-readable performance reports on-demand.

#### Step 1: Create Report Generator

Create `src/performance_report.rs`:

```rust
use crate::performance::MetricsSnapshot;
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct PerformanceReport;

impl PerformanceReport {
    /// Generate markdown performance report
    pub fn generate_markdown(snapshot: &MetricsSnapshot) -> String {
        format!(
            r#"# VellumFE Performance Report

**Generated**: {timestamp}
**Uptime**: {uptime}

## Executive Summary

| Metric | Value | Status |
|--------|-------|--------|
| FPS | {fps:.1} | {fps_status} |
| Frame Time | {frame_ms:.2}ms | {frame_status} |
| Memory Usage | {memory_mb:.1} MB | {memory_status} |

## Detailed Metrics

### Frame Rendering
- **FPS**: {fps:.1} (target: 60)
- **Average Frame Time**: {frame_ms:.2}ms (target: <16.67ms)
- **Min Frame Time**: {min_frame_ms:.2}ms
- **Max Frame Time**: {max_frame_ms:.2}ms
- **Average Render Time**: {render_ms:.2}ms
- **Max Render Time**: {max_render_ms:.2}ms
- **UI Render Time**: {ui_render_ms:.2}ms
- **Text Wrap Time**: {wrap_us:.0}μs

### Network
- **Received**: {bytes_in:.2} KB/s
- **Sent**: {bytes_out:.2} KB/s

### Parser
- **Parse Time**: {parse_us:.0}μs (target: <50μs)
- **Chunks/sec**: {chunks_s}
- **Elements/sec**: {elements_s}

### Events
- **Average Event Time**: {event_us:.0}μs (target: <100μs)
- **Max Event Time**: {max_event_us:.0}μs
- **Total Events**: {total_events}

### Memory
- **Lines Buffered**: {lines}
- **Active Windows**: {windows}
- **Estimated Memory**: {memory_mb:.1} MB

## Performance Assessment

{assessment}

## Recommendations

{recommendations}
"#,
            timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(snapshot.timestamp as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
            uptime = format_duration(snapshot.uptime_seconds),
            fps = snapshot.fps,
            fps_status = if snapshot.fps >= 55.0 { "✓ Excellent" } else if snapshot.fps >= 30.0 { "⚠ Acceptable" } else { "✗ Poor" },
            frame_ms = snapshot.avg_frame_time_ms,
            frame_status = if snapshot.avg_frame_time_ms <= 17.0 { "✓ Good" } else if snapshot.avg_frame_time_ms <= 33.0 { "⚠ Fair" } else { "✗ Slow" },
            memory_mb = snapshot.estimated_memory_mb,
            memory_status = if snapshot.estimated_memory_mb < 100.0 { "✓ Normal" } else if snapshot.estimated_memory_mb < 200.0 { "⚠ High" } else { "✗ Critical" },
            min_frame_ms = snapshot.min_frame_time_ms,
            max_frame_ms = snapshot.max_frame_time_ms,
            render_ms = snapshot.avg_render_time_ms,
            max_render_ms = snapshot.max_render_time_ms,
            ui_render_ms = snapshot.avg_ui_render_time_ms,
            wrap_us = snapshot.avg_text_wrap_time_us,
            bytes_in = snapshot.bytes_received_per_sec as f64 / 1024.0,
            bytes_out = snapshot.bytes_sent_per_sec as f64 / 1024.0,
            parse_us = snapshot.avg_parse_time_us,
            chunks_s = snapshot.chunks_per_sec,
            elements_s = snapshot.elements_per_sec,
            event_us = snapshot.avg_event_process_time_us,
            max_event_us = snapshot.max_event_process_time_us,
            total_events = snapshot.total_events_processed,
            lines = snapshot.total_lines_buffered,
            windows = snapshot.active_window_count,
            assessment = Self::generate_assessment(snapshot),
            recommendations = Self::generate_recommendations(snapshot),
        )
    }

    fn generate_assessment(snapshot: &MetricsSnapshot) -> String {
        let mut issues = Vec::new();

        if snapshot.fps < 30.0 {
            issues.push("⚠ **Low Frame Rate**: FPS below 30 indicates performance issues.");
        }
        if snapshot.avg_frame_time_ms > 33.0 {
            issues.push("⚠ **Slow Frames**: Frame time exceeds 33ms (30fps threshold).");
        }
        if snapshot.avg_parse_time_us > 100.0 {
            issues.push("⚠ **Slow Parsing**: Parser taking >100μs per line.");
        }
        if snapshot.estimated_memory_mb > 150.0 {
            issues.push("⚠ **High Memory**: Memory usage above 150 MB.");
        }
        if snapshot.max_frame_time_ms > 100.0 {
            issues.push("⚠ **Frame Spikes**: Max frame time >100ms indicates stuttering.");
        }

        if issues.is_empty() {
            "✓ **Performance is excellent.** All metrics within optimal ranges.".to_string()
        } else {
            format!("**Issues Detected:**\n\n{}", issues.join("\n"))
        }
    }

    fn generate_recommendations(snapshot: &MetricsSnapshot) -> String {
        let mut recs = Vec::new();

        if snapshot.fps < 30.0 {
            recs.push("- Reduce active window count or buffer sizes");
            recs.push("- Close unused windows with `.deletewindow`");
        }
        if snapshot.avg_parse_time_us > 100.0 {
            recs.push("- Review highlight regex patterns for complexity");
            recs.push("- Consider reducing number of highlight patterns");
        }
        if snapshot.estimated_memory_mb > 150.0 {
            recs.push("- Reduce `buffer_size` in window configs (default: 10000)");
            recs.push("- Close inactive tabbed window tabs with `.removetab`");
        }
        if snapshot.max_frame_time_ms > 50.0 {
            recs.push("- Profile render pipeline with `.snapshot` during lag spikes");
            recs.push("- Check for mouse drag operations on large windows");
        }

        if recs.is_empty() {
            "No optimization needed at this time.".to_string()
        } else {
            recs.join("\n")
        }
    }

    /// Write report to file
    pub fn write_to_file<P: AsRef<Path>>(snapshot: &MetricsSnapshot, path: P) -> Result<()> {
        let report = Self::generate_markdown(snapshot);
        let mut file = File::create(path)?;
        file.write_all(report.as_bytes())?;
        Ok(())
    }
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}
```

#### Step 2: Add Report Dot Command

Edit `src/app.rs`:

```rust
"perfreport" | "report" => {
    let snapshot = self.perf_stats.snapshot();
    let report_path = Config::get_base_dir()?.join("performance_report.md");
    match PerformanceReport::write_to_file(&snapshot, &report_path) {
        Ok(_) => {
            self.add_system_message(&format!(
                "Performance report written to: {}",
                report_path.display()
            ));
        }
        Err(e) => {
            self.add_system_message(&format!("Failed to write report: {}", e));
        }
    }
}
```

**Usage**: Type `.perfreport` to generate `~/.vellum-fe/performance_report.md`.

### Phase 5: Historical Trend Analysis (2 hours)

Analyze metrics over time to detect regressions and trends.

#### Step 1: Create Trend Analyzer

Create `src/trend_analyzer.rs`:

```rust
use crate::performance::MetricsSnapshot;
use anyhow::Result;
use std::collections::VecDeque;

pub struct TrendAnalyzer {
    history: VecDeque<MetricsSnapshot>,
    max_history: usize,
}

impl TrendAnalyzer {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
        }
    }

    /// Add a snapshot to history
    pub fn record(&mut self, snapshot: MetricsSnapshot) {
        self.history.push_back(snapshot);
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Calculate linear regression trend for a metric
    fn calculate_trend<F>(&self, metric_fn: F) -> (f64, f64)
    where
        F: Fn(&MetricsSnapshot) -> f64,
    {
        if self.history.len() < 2 {
            return (0.0, 0.0);  // slope, intercept
        }

        let n = self.history.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2 = 0.0;

        for (i, snapshot) in self.history.iter().enumerate() {
            let x = i as f64;
            let y = metric_fn(snapshot);
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
        }

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
        let intercept = (sum_y - slope * sum_x) / n;

        (slope, intercept)
    }

    /// Get trend analysis report
    pub fn analyze(&self) -> TrendReport {
        TrendReport {
            fps_trend: self.calculate_trend(|s| s.fps),
            frame_time_trend: self.calculate_trend(|s| s.avg_frame_time_ms),
            memory_trend: self.calculate_trend(|s| s.estimated_memory_mb),
            parse_time_trend: self.calculate_trend(|s| s.avg_parse_time_us),
            sample_count: self.history.len(),
        }
    }
}

#[derive(Debug)]
pub struct TrendReport {
    pub fps_trend: (f64, f64),          // (slope, intercept)
    pub frame_time_trend: (f64, f64),
    pub memory_trend: (f64, f64),
    pub parse_time_trend: (f64, f64),
    pub sample_count: usize,
}

impl TrendReport {
    pub fn format(&self) -> String {
        format!(
            r#"# Performance Trend Analysis

**Samples**: {samples}

## Trends (per sample)

| Metric | Trend | Assessment |
|--------|-------|------------|
| FPS | {fps_slope:+.2} fps | {fps_assessment} |
| Frame Time | {frame_slope:+.2} ms | {frame_assessment} |
| Memory | {memory_slope:+.2} MB | {memory_assessment} |
| Parse Time | {parse_slope:+.2} μs | {parse_assessment} |

{legend}
"#,
            samples = self.sample_count,
            fps_slope = self.fps_trend.0,
            fps_assessment = trend_assessment(self.fps_trend.0, true),
            frame_slope = self.frame_time_trend.0,
            frame_assessment = trend_assessment(self.frame_time_trend.0, false),
            memory_slope = self.memory_trend.0,
            memory_assessment = trend_assessment(self.memory_trend.0, false),
            parse_slope = self.parse_time_trend.0,
            parse_assessment = trend_assessment(self.parse_time_trend.0, false),
            legend = r#"**Legend:**
- ✓ Improving trend (positive direction)
- → Stable (no significant change)
- ⚠ Degrading trend (negative direction)"#,
        )
    }
}

fn trend_assessment(slope: f64, higher_is_better: bool) -> &'static str {
    let threshold = 0.01;  // Minimum slope to be considered significant
    if slope.abs() < threshold {
        "→ Stable"
    } else if (higher_is_better && slope > 0.0) || (!higher_is_better && slope < 0.0) {
        "✓ Improving"
    } else {
        "⚠ Degrading"
    }
}
```

#### Step 2: Integrate Trend Analyzer

Edit `src/app.rs`:

```rust
pub struct App {
    // ... existing fields ...
    trend_analyzer: TrendAnalyzer,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        // ... existing initialization ...

        let trend_analyzer = TrendAnalyzer::new(360);  // 1 hour at 10s intervals

        Ok(Self {
            // ... existing fields ...
            trend_analyzer,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // ... in event loop where snapshot is created ...
        if self.frame_count % 600 == 0 {  // Every 10 seconds at 60fps
            let snapshot = self.perf_stats.snapshot();
            self.trend_analyzer.record(snapshot.clone());
            let _ = self.metrics_snapshot_tx.send(snapshot);
        }

        // Add dot command for trend report
        // ... in handle_dot_command() ...
    }
}

// Add to handle_dot_command():
"trend" | "trends" => {
    let report = self.trend_analyzer.analyze();
    self.add_system_message(&report.format());
}
```

**Usage**: Type `.trend` to see performance trends over the last hour.

## Testing Performance Reporting

### Unit Tests

Create `tests/performance_tests.rs`:

```rust
use vellum_fe::performance::{PerformanceStats, MetricsSnapshot};
use std::time::Duration;

#[test]
fn test_metrics_snapshot() {
    let mut stats = PerformanceStats::new();
    stats.record_frame();
    std::thread::sleep(Duration::from_millis(16));
    stats.record_frame();

    let snapshot = stats.snapshot();
    assert!(snapshot.fps > 0.0);
    assert!(snapshot.avg_frame_time_ms > 0.0);
}

#[test]
fn test_snapshot_serialization() {
    let mut stats = PerformanceStats::new();
    stats.record_parse(Duration::from_micros(50));

    let snapshot = stats.snapshot();
    let json = serde_json::to_string(&snapshot).unwrap();
    let deserialized: MetricsSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(snapshot.avg_parse_time_us, deserialized.avg_parse_time_us);
}
```

### Integration Tests

1. **Metrics Logging Test**:
   ```bash
   # Enable metrics logging in config
   cargo run -- --character TestChar
   # Wait 30 seconds
   cat ~/.vellum-fe/metrics.csv
   # Verify CSV has 3+ rows
   ```

2. **Prometheus Export Test**:
   ```bash
   # Enable prometheus in config
   cargo run -- --character TestChar &
   sleep 5
   curl http://localhost:9090/metrics | grep vellum_fps
   # Verify metrics appear
   ```

3. **Report Generation Test**:
   ```bash
   cargo run -- --character TestChar
   # Type: .perfreport
   cat ~/.vellum-fe/performance_report.md
   # Verify markdown report exists
   ```

## Production Deployment Checklist

- [ ] Metrics logging enabled in production config
- [ ] Log rotation configured (logrotate or equivalent)
- [ ] Prometheus exporter enabled and firewalled appropriately
- [ ] Grafana dashboards created from Prometheus metrics
- [ ] Alert rules configured for performance degradation
- [ ] Automated trend reports scheduled (e.g., nightly cron job)
- [ ] Historical metrics archived to long-term storage
- [ ] Performance regression tests added to CI/CD pipeline

## Example Grafana Dashboard

```json
{
  "dashboard": {
    "title": "VellumFE Performance",
    "panels": [
      {
        "title": "FPS",
        "targets": [
          {
            "expr": "vellum_fps",
            "legendFormat": "FPS"
          }
        ],
        "type": "graph"
      },
      {
        "title": "Frame Time",
        "targets": [
          {
            "expr": "vellum_frame_time_milliseconds",
            "legendFormat": "Frame Time (ms)"
          }
        ],
        "type": "graph"
      },
      {
        "title": "Memory Usage",
        "targets": [
          {
            "expr": "vellum_memory_megabytes",
            "legendFormat": "Memory (MB)"
          }
        ],
        "type": "graph"
      }
    ]
  }
}
```

## Troubleshooting

### Metrics Not Appearing in Prometheus

- Check HTTP server started: `netstat -an | grep 9090`
- Verify snapshot channel working: Add debug logs to `PrometheusExporter::serve()`
- Test endpoint manually: `curl http://localhost:9090/metrics`

### CSV Log File Not Created

- Check file permissions on `~/.vellum-fe/` directory
- Verify `enable_metrics_logging = true` in config
- Check debug log for `MetricsLogger` errors

### Performance Report Empty

- Ensure at least one frame has been rendered before calling `.perfreport`
- Verify `chrono` crate added to dependencies for timestamp formatting

---

**Document Version**: 1.0
**Last Updated**: 2025-01-12
**Maintained By**: VellumFE Development Team
**Related Docs**: [Technical Specification](./PERFORMANCE_MONITORING_TECHNICAL.md), [ELI5 Guide](./PERFORMANCE_ELI5.md)
