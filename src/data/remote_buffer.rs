//! Remote scrollback ring buffer for the web frontend.
//!
//! The TUI/GUI store text pre-wrapped to window width (`TextContent.lines`),
//! which a phone browser can't reflow. This buffer captures styled but
//! *unwrapped* lines at the point in the message pipeline where they are
//! finalized (after highlighting, before wrapping), so remote clients can
//! wrap to their own viewport.
//!
//! Every line gets a globally monotonic sequence number, which the web
//! protocol uses as its reconnect-resume cursor. Lines are stored as
//! `Arc<StyledLine>` so the ring and the broadcast channel share one
//! allocation per line.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::widget::StyledLine;

/// Default per-stream line cap (matches the plan's ~2,000 line guidance).
pub const DEFAULT_MAX_LINES_PER_STREAM: usize = 2_000;

/// A single buffered line with its global sequence number.
#[derive(Clone, Debug)]
pub struct RemoteLine {
    pub seq: u64,
    pub stream: String,
    pub line: Arc<StyledLine>,
}

/// Per-stream ring buffer of finalized, unwrapped styled lines.
#[derive(Debug)]
pub struct RemoteBuffer {
    streams: HashMap<String, VecDeque<RemoteLine>>,
    next_seq: u64,
    max_lines_per_stream: usize,
    /// Highest seq ever evicted from any ring (0 = nothing evicted).
    /// A resume cursor `after` is fillable iff `after >= evicted_through`.
    evicted_through: u64,
}

impl RemoteBuffer {
    pub fn new(max_lines_per_stream: usize) -> Self {
        Self {
            streams: HashMap::new(),
            next_seq: 1,
            max_lines_per_stream,
            evicted_through: 0,
        }
    }

    /// Append a finalized line to a stream's ring, assigning it the next
    /// global sequence number. Returns the assigned seq.
    pub fn push(&mut self, stream: &str, line: Arc<StyledLine>) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let ring = self.streams.entry(stream.to_string()).or_default();
        if ring.len() >= self.max_lines_per_stream {
            if let Some(evicted) = ring.pop_front() {
                self.evicted_through = self.evicted_through.max(evicted.seq);
            }
        }
        ring.push_back(RemoteLine {
            seq,
            stream: stream.to_string(),
            line,
        });
        seq
    }

    /// Highest sequence number assigned so far (0 = nothing buffered yet).
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// Names of all streams that have buffered at least one line.
    pub fn stream_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.streams.keys().cloned().collect();
        names.sort();
        names
    }

    /// Last `n` lines of one stream, oldest first.
    pub fn tail(&self, stream: &str, n: usize) -> Vec<RemoteLine> {
        match self.streams.get(stream) {
            Some(ring) => ring.iter().rev().take(n).rev().cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Last `n` lines per stream across all streams, merged and sorted by
    /// seq (oldest first). Used to build connect-time snapshots.
    pub fn snapshot_tail(&self, n_per_stream: usize) -> Vec<RemoteLine> {
        let mut lines: Vec<RemoteLine> = self
            .streams
            .values()
            .flat_map(|ring| ring.iter().rev().take(n_per_stream).rev().cloned())
            .collect();
        lines.sort_by_key(|l| l.seq);
        lines
    }

    /// All buffered lines with seq strictly greater than `after`, merged
    /// across streams and sorted by seq. Returns `None` when lines in the
    /// gap have already been evicted, signalling the caller to fall back
    /// to a fresh snapshot.
    pub fn lines_since(&self, after: u64) -> Option<Vec<RemoteLine>> {
        if after >= self.last_seq() {
            return Some(Vec::new());
        }
        if after < self.evicted_through {
            return None;
        }
        let mut lines: Vec<RemoteLine> = self
            .streams
            .values()
            .flat_map(|ring| ring.iter().filter(|l| l.seq > after).cloned())
            .collect();
        lines.sort_by_key(|l| l.seq);
        Some(lines)
    }
}

impl Default for RemoteBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LINES_PER_STREAM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, stream: &str) -> Arc<StyledLine> {
        Arc::new(StyledLine {
            segments: vec![crate::data::widget::TextSegment::plain(text)],
            stream: stream.to_string(),
        })
    }

    #[test]
    fn push_assigns_monotonic_seq_across_streams() {
        let mut buf = RemoteBuffer::new(10);
        assert_eq!(buf.push("main", line("a", "main")), 1);
        assert_eq!(buf.push("thoughts", line("b", "thoughts")), 2);
        assert_eq!(buf.push("main", line("c", "main")), 3);
        assert_eq!(buf.last_seq(), 3);
    }

    #[test]
    fn ring_evicts_oldest_per_stream() {
        let mut buf = RemoteBuffer::new(2);
        buf.push("main", line("a", "main"));
        buf.push("main", line("b", "main"));
        buf.push("main", line("c", "main"));
        let tail = buf.tail("main", 10);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].seq, 2);
        assert_eq!(tail[1].seq, 3);
    }

    #[test]
    fn snapshot_tail_merges_streams_in_seq_order() {
        let mut buf = RemoteBuffer::new(10);
        buf.push("main", line("a", "main"));
        buf.push("thoughts", line("b", "thoughts"));
        buf.push("main", line("c", "main"));
        let snap = buf.snapshot_tail(10);
        assert_eq!(
            snap.iter().map(|l| l.seq).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn lines_since_returns_only_newer_lines() {
        let mut buf = RemoteBuffer::new(10);
        buf.push("main", line("a", "main"));
        buf.push("main", line("b", "main"));
        buf.push("thoughts", line("c", "thoughts"));
        let since = buf.lines_since(1).expect("gap should be fillable");
        assert_eq!(since.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn lines_since_up_to_date_returns_empty() {
        let mut buf = RemoteBuffer::new(10);
        buf.push("main", line("a", "main"));
        assert_eq!(buf.lines_since(1).unwrap().len(), 0);
        assert_eq!(buf.lines_since(99).unwrap().len(), 0);
    }

    #[test]
    fn lines_since_reports_gap_after_eviction() {
        let mut buf = RemoteBuffer::new(2);
        for i in 0..5 {
            buf.push("main", line(&format!("l{i}"), "main"));
        }
        // Client last saw seq 1; seqs 2-3 were evicted (ring holds 4,5).
        assert!(buf.lines_since(1).is_none());
        // Client saw everything up to the oldest retained line.
        assert!(buf.lines_since(3).is_some());
    }

    #[test]
    fn tail_unknown_stream_is_empty() {
        let buf = RemoteBuffer::default();
        assert!(buf.tail("nope", 5).is_empty());
    }
}
