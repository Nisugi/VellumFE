//! Active `INVENTORY FULL` scan: the one place the registry issues a
//! command instead of reading the passive feed.
//!
//! Mark/registration status lives ONLY in the `INVENTORY FULL` reply's
//! trailing `(registered)` / `(marked)` column — the passive `<inv>` feed
//! doesn't carry it (verified against real feeds; foreach.lic reads it the
//! same way, `INVFULL_PATTERN` group 7). So a feature that filters on
//! marked/registered must trigger a scan, whose echo we squelch and whose
//! reply we parse into per-item `ItemStatus`.
//!
//! This module is the mechanical core: parse one reply line into
//! `(exist_id, ItemStatus)`, plus a small state machine tracking a scan
//! in flight. The message pipeline drives it (squelch while capturing,
//! finalize at the prompt); foreach consumes the resulting statuses.

use super::{parse_anchor, ItemStatus};

/// The command that produces the status-bearing reply.
pub const INVENTORY_FULL_COMMAND: &str = "inventory full";

/// Parse one `INVENTORY FULL` reply line into `(exist_id, status)`.
/// Returns None for non-item lines (the "You are currently wearing:"
/// header, the "(Items: N | Matches: M)" footer, blank lines).
///
/// The reply line carries the item as an `<a exist noun>` anchor followed
/// by an optional trailing status column, e.g. `... </a> (registered)
/// (marked)`. Status is presence-based: "registered"/"marked" anywhere
/// after the anchor set that flag true; absence sets it false (an item
/// that appears in the full reply but lacks a marker is known-unmarked,
/// not unknown).
pub fn parse_status_line(line: &str) -> Option<(String, ItemStatus)> {
    let item = parse_anchor(line)?;
    // Everything after the closing </a> is the status region.
    let tail = line
        .split_once("</a>")
        .map(|(_, rest)| rest)
        .unwrap_or("")
        .to_ascii_lowercase();
    Some((
        item.id,
        ItemStatus {
            registered: Some(tail.contains("registered")),
            marked: Some(tail.contains("marked")),
        },
    ))
}

/// State of an `INVENTORY FULL` scan in flight.
///
/// Lifecycle: `start()` → the pipeline squelches and feeds each reply line
/// to `ingest_line()` while `capturing` → the prompt calls `finish()`,
/// yielding the collected statuses for the registry. A guard limits how
/// long we stay in capture mode in case the expected prompt never lands.
#[derive(Debug, Default)]
pub struct InvScan {
    capturing: bool,
    collected: Vec<(String, ItemStatus)>,
    /// Reply lines seen so far this scan (bounds the capture window in
    /// case the terminating prompt is missed).
    lines_seen: usize,
}

/// Hard cap on captured lines before we bail out of a scan, so a missed
/// prompt can't leave us squelching forever. A full worn+containers reply
/// is well under this.
const MAX_SCAN_LINES: usize = 2000;

impl InvScan {
    /// Begin capturing. The caller separately queues the command.
    pub fn start(&mut self) {
        self.capturing = true;
        self.collected.clear();
        self.lines_seen = 0;
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    /// Feed one raw reply line while capturing (string form; used by
    /// tests and any raw-line caller). Returns true if it was a
    /// status-bearing item line.
    pub fn ingest_line(&mut self, line: &str) -> bool {
        if !self.tick_window() {
            return false;
        }
        if let Some((id, status)) = parse_status_line(line) {
            self.collected.push((id, status));
            true
        } else {
            false
        }
    }

    /// Feed one reply line as parsed segments (the live pipeline path).
    /// Reuses the main parser's resolved `link_data` for the exist id and
    /// reads the status column from the concatenated text after the link.
    /// Returns true if the line was a status-bearing item line.
    pub fn ingest_segments(
        &mut self,
        segments: &[crate::data::widget::TextSegment],
    ) -> bool {
        if !self.tick_window() {
            return false;
        }
        // The item's link segment carries the exist id.
        let Some(link_seg) = segments.iter().find(|s| s.link_data.is_some()) else {
            return false;
        };
        let id = link_seg.link_data.as_ref().unwrap().exist_id.clone();
        // Status text follows the link — scan segments after the linked one.
        let link_idx = segments
            .iter()
            .position(|s| std::ptr::eq(s, link_seg))
            .unwrap_or(0);
        let tail: String = segments[link_idx + 1..]
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
            .to_ascii_lowercase();
        self.collected.push((
            id,
            ItemStatus {
                registered: Some(tail.contains("registered")),
                marked: Some(tail.contains("marked")),
            },
        ));
        true
    }

    /// Advance the capture window; returns false (and stops capturing) when
    /// the runaway guard trips.
    fn tick_window(&mut self) -> bool {
        if !self.capturing {
            return false;
        }
        self.lines_seen += 1;
        if self.lines_seen > MAX_SCAN_LINES {
            self.capturing = false;
            return false;
        }
        true
    }

    /// End the scan (at the prompt), returning the collected statuses.
    pub fn finish(&mut self) -> Vec<(String, ItemStatus)> {
        self.capturing = false;
        std::mem::take(&mut self.collected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_column_presence() {
        let both = r#"  some <a exist="1" noun="gloves">triton hide gloves</a> with knuckles (registered) (marked)"#;
        let (id, s) = parse_status_line(both).unwrap();
        assert_eq!(id, "1");
        assert_eq!(s.registered, Some(true));
        assert_eq!(s.marked, Some(true));

        let marked_only =
            r#"  a <a exist="2" noun="bandolier">iron boar hide bandolier</a> (marked)"#;
        let (_, s) = parse_status_line(marked_only).unwrap();
        assert_eq!(s.registered, Some(false));
        assert_eq!(s.marked, Some(true));

        // In the reply but no column = known-false, not unknown.
        let plain = r#"  a <a exist="3" noun="ring">plain ring</a>"#;
        let (_, s) = parse_status_line(plain).unwrap();
        assert_eq!(s.registered, Some(false));
        assert_eq!(s.marked, Some(false));

        // Header/footer lines have no anchor.
        assert!(parse_status_line("You are currently wearing:").is_none());
        assert!(parse_status_line("(Items: 7 | Matches: 6)").is_none());
    }

    #[test]
    fn scan_captures_between_start_and_finish() {
        let mut scan = InvScan::default();
        assert!(!scan.is_capturing());
        // Not capturing: lines ignored.
        assert!(!scan.ingest_line(r#" a <a exist="1" noun="x">x</a> (marked)"#));

        scan.start();
        assert!(scan.is_capturing());
        assert!(!scan.ingest_line("You are currently wearing:")); // header
        assert!(scan.ingest_line(r#"  a <a exist="1" noun="x">x</a> (marked)"#));
        assert!(scan.ingest_line(r#"  a <a exist="2" noun="y">y</a> (registered)"#));

        let statuses = scan.finish();
        assert!(!scan.is_capturing());
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].0, "1");
        assert_eq!(statuses[0].1.marked, Some(true));
        assert_eq!(statuses[1].0, "2");
        assert_eq!(statuses[1].1.registered, Some(true));
    }

    #[test]
    fn runaway_guard_stops_capturing() {
        let mut scan = InvScan::default();
        scan.start();
        for i in 0..(MAX_SCAN_LINES + 5) {
            scan.ingest_line(&format!(r#" a <a exist="{i}" noun="x">x</a>"#));
        }
        assert!(!scan.is_capturing(), "runaway guard tripped");
    }
}
