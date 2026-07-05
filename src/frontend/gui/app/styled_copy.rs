//! Styled clipboard copy: rebuild ANSI or HTML styling for text the user
//! copied out of a text window.
//!
//! egui's label selection puts plain text on the clipboard. To honor
//! `CopyBehavior::AnsiCodes`/`Html`, the copied plain text is matched back
//! against the window line buffers (each copied fragment corresponds to one
//! source line; the first and last may be partial), and the matching
//! `StyledLine` segments are re-serialized with their colors and weight.
//! If no window matches — e.g. text copied from an editor field — the copy
//! stays plain.

use super::widgets::parse_hex_color;
use super::*;
use std::ops::Range;
use std::sync::{Arc, Mutex};

/// State shared between the app (which snapshots window buffers when a copy
/// chord arrives) and the egui plugin (which rewrites the copy command).
#[derive(Default)]
pub(super) struct StyledCopyShared {
    pub(super) behavior: CopyBehavior,
    /// Window line buffers captured on the frame the copy happened.
    snapshot: Option<Vec<Vec<StyledLine>>>,
}

/// egui plugin that rewrites `CopyText` commands per the copy behavior.
///
/// egui's label selection flushes the selected text in its own end-of-pass
/// plugin hook — after the app's frame code has run — so the only place the
/// plain-text `CopyText` command can be observed is `output_hook`, which
/// runs after all plugins and before eframe hands the commands to the OS.
pub(super) struct StyledCopyPlugin {
    shared: Arc<Mutex<StyledCopyShared>>,
}

impl StyledCopyPlugin {
    pub(super) fn new(shared: Arc<Mutex<StyledCopyShared>>) -> Self {
        Self { shared }
    }
}

impl egui::Plugin for StyledCopyPlugin {
    fn debug_name(&self) -> &'static str {
        "vellum_styled_copy"
    }

    fn output_hook(&mut self, output: &mut egui::FullOutput) {
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        let Some(snapshot) = shared.snapshot.take() else {
            return;
        };
        if matches!(shared.behavior, CopyBehavior::PlainText) {
            return;
        }

        let commands = &mut output.platform_output.commands;
        let copied = commands.iter().find_map(|command| match command {
            egui::OutputCommand::CopyText(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        });
        let Some(copied) = copied else {
            return;
        };

        // First window buffer that contains the copied text verbatim wins;
        // if none does (e.g. text copied out of an editor field), the copy
        // stays plain.
        let styled = snapshot.iter().find_map(|lines| {
            let refs: Vec<&StyledLine> = lines.iter().collect();
            let matched = match_copied_text(&copied, &refs)?;
            Some(match shared.behavior {
                CopyBehavior::AnsiCodes => copied_lines_to_ansi(&matched),
                CopyBehavior::Html => copied_lines_to_html(&matched),
                CopyBehavior::PlainText => unreachable!(),
            })
        });
        let Some(styled) = styled else {
            return;
        };

        match shared.behavior {
            CopyBehavior::AnsiCodes => {
                for command in commands.iter_mut() {
                    if let egui::OutputCommand::CopyText(text) = command {
                        *text = styled.clone();
                    }
                }
            }
            CopyBehavior::Html => {
                commands.retain(|command| !matches!(command, egui::OutputCommand::CopyText(_)));
                if let Err(err) = crate::clipboard::copy_html(&styled, &copied) {
                    tracing::warn!("HTML copy failed, falling back to plain text: {err}");
                    commands.push(egui::OutputCommand::CopyText(copied));
                }
            }
            CopyBehavior::PlainText => {}
        }
    }
}

impl VellumGuiApp {
    /// When this frame carries a copy chord, snapshot the text-window line
    /// buffers so the plugin's `output_hook` can rebuild styling for
    /// whatever egui ends up copying. Costs nothing on other frames.
    pub(super) fn arm_styled_copy(&self, ctx: &egui::Context) {
        if matches!(self.copy_behavior, CopyBehavior::PlainText) {
            return;
        }
        let copy_requested =
            ctx.input(|input| input.events.iter().any(|e| matches!(e, egui::Event::Copy)));
        if !copy_requested {
            return;
        }

        let mut snapshot: Vec<Vec<StyledLine>> = Vec::new();
        for window in self.app_core.ui_state.windows.values() {
            match &window.content {
                WindowContent::Text(content) => {
                    snapshot.push(content.lines.iter().cloned().collect());
                }
                WindowContent::TabbedText(tabbed) => {
                    for tab in &tabbed.tabs {
                        snapshot.push(tab.content.lines.iter().cloned().collect());
                    }
                }
                _ => {}
            }
        }

        if let Ok(mut shared) = self.styled_copy_shared.lock() {
            shared.behavior = self.copy_behavior.clone();
            shared.snapshot = Some(snapshot);
        }
    }
}

/// One line of a matched copy selection.
pub(super) enum CopiedLine<'a> {
    /// A byte range of a window line (full for middle lines, possibly
    /// partial for the first and last line of the selection).
    Matched {
        line: &'a StyledLine,
        range: Range<usize>,
    },
    /// A blank line in the copied text (egui inserts one for the vertical
    /// gap left by an empty source line).
    Empty,
}

pub(super) fn line_plain_text(line: &StyledLine) -> String {
    line.segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect()
}

/// Match copied plain text against a window's lines.
///
/// Fragments (split on `\n`) must map to consecutive window lines: middle
/// fragments match a whole line, the first fragment matches a line suffix,
/// and the last fragment matches a line prefix. Returns the first match, or
/// None when the copied text did not come from these lines verbatim.
pub(super) fn match_copied_text<'a>(
    copied: &str,
    lines: &[&'a StyledLine],
) -> Option<Vec<CopiedLine<'a>>> {
    let fragments: Vec<&str> = copied.split('\n').collect();
    if fragments.iter().all(|fragment| fragment.is_empty()) {
        return None;
    }
    let plain: Vec<String> = lines.iter().map(|line| line_plain_text(line)).collect();

    'start: for start in 0..plain.len() {
        if start + fragments.len() > plain.len() {
            break;
        }
        let mut result = Vec::with_capacity(fragments.len());
        for (k, fragment) in fragments.iter().enumerate() {
            let line_idx = start + k;
            if fragment.is_empty() {
                result.push(CopiedLine::Empty);
                continue;
            }
            let text = &plain[line_idx];
            let is_first = k == 0;
            let is_last = k + 1 == fragments.len();
            let range = if is_first && is_last {
                text.find(fragment)
                    .map(|pos| pos..pos + fragment.len())
            } else if is_first {
                text.ends_with(fragment)
                    .then(|| text.len() - fragment.len()..text.len())
            } else if is_last {
                text.starts_with(fragment).then(|| 0..fragment.len())
            } else {
                (text == fragment).then(|| 0..fragment.len())
            };
            let Some(range) = range else {
                continue 'start;
            };
            result.push(CopiedLine::Matched {
                line: lines[line_idx],
                range,
            });
        }
        return Some(result);
    }
    None
}

/// The slices of a line's segments covered by a byte range, in order.
fn segment_slices<'a>(
    line: &'a StyledLine,
    range: &Range<usize>,
) -> Vec<(&'a TextSegment, &'a str)> {
    let mut slices = Vec::new();
    let mut offset = 0usize;
    for segment in &line.segments {
        let seg_start = offset;
        let seg_end = offset + segment.text.len();
        offset = seg_end;
        let start = range.start.max(seg_start);
        let end = range.end.min(seg_end);
        if start < end {
            slices.push((segment, &segment.text[start - seg_start..end - seg_start]));
        }
    }
    slices
}

/// Serialize matched lines with ANSI truecolor escapes (SGR 38;2/48;2, bold).
pub(super) fn copied_lines_to_ansi(lines: &[CopiedLine<'_>]) -> String {
    let mut out = String::new();
    let mut styled = false;
    for (i, copied_line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let CopiedLine::Matched { line, range } = copied_line else {
            continue;
        };
        for (segment, slice) in segment_slices(line, range) {
            let mut codes: Vec<String> = Vec::new();
            if segment.bold {
                codes.push("1".to_string());
            }
            if let Some(color) = segment.fg.as_deref().and_then(parse_hex_color) {
                codes.push(format!("38;2;{};{};{}", color.r(), color.g(), color.b()));
            }
            if let Some(color) = segment.bg.as_deref().and_then(parse_hex_color) {
                codes.push(format!("48;2;{};{};{}", color.r(), color.g(), color.b()));
            }
            if styled {
                out.push_str("\x1b[0m");
                styled = false;
            }
            if !codes.is_empty() {
                out.push_str(&format!("\x1b[{}m", codes.join(";")));
                styled = true;
            }
            out.push_str(slice);
        }
    }
    if styled {
        out.push_str("\x1b[0m");
    }
    out
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Serialize matched lines as an HTML fragment with inline span styling.
pub(super) fn copied_lines_to_html(lines: &[CopiedLine<'_>]) -> String {
    let mut out = String::from("<pre style=\"font-family:monospace\">");
    for (i, copied_line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let CopiedLine::Matched { line, range } = copied_line else {
            continue;
        };
        for (segment, slice) in segment_slices(line, range) {
            let mut style = String::new();
            if let Some(color) = segment.fg.as_deref().and_then(parse_hex_color) {
                style.push_str(&format!(
                    "color:#{:02x}{:02x}{:02x};",
                    color.r(),
                    color.g(),
                    color.b()
                ));
            }
            if let Some(color) = segment.bg.as_deref().and_then(parse_hex_color) {
                style.push_str(&format!(
                    "background-color:#{:02x}{:02x}{:02x};",
                    color.r(),
                    color.g(),
                    color.b()
                ));
            }
            if segment.bold {
                style.push_str("font-weight:bold;");
            }
            if style.is_empty() {
                out.push_str(&html_escape(slice));
            } else {
                out.push_str(&format!(
                    "<span style=\"{}\">{}</span>",
                    style,
                    html_escape(slice)
                ));
            }
        }
    }
    out.push_str("</pre>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str, fg: Option<&str>, bold: bool) -> TextSegment {
        TextSegment {
            text: text.to_string(),
            fg: fg.map(str::to_string),
            bg: None,
            bold,
            ..Default::default()
        }
    }

    fn line(segments: Vec<TextSegment>) -> StyledLine {
        StyledLine {
            segments,
            stream: "main".to_string(),
        }
    }

    #[test]
    fn match_finds_substring_within_single_line() {
        let l = line(vec![segment("You see a rusty sword here.", None, false)]);
        let lines = vec![&l];

        let matched = match_copied_text("rusty sword", &lines).expect("should match");
        assert_eq!(matched.len(), 1);
        let CopiedLine::Matched { range, .. } = &matched[0] else {
            panic!("expected matched line");
        };
        assert_eq!(&line_plain_text(&l)[range.clone()], "rusty sword");
    }

    #[test]
    fn match_spans_multiple_lines_with_partial_edges() {
        let l1 = line(vec![segment("first line tail", None, false)]);
        let l2 = line(vec![segment("full middle line", None, false)]);
        let l3 = line(vec![segment("head of last", None, false)]);
        let lines = vec![&l1, &l2, &l3];

        let matched =
            match_copied_text("tail\nfull middle line\nhead", &lines).expect("should match");
        assert_eq!(matched.len(), 3);
        let CopiedLine::Matched { range, .. } = &matched[0] else {
            panic!("expected matched first line");
        };
        assert_eq!(&line_plain_text(&l1)[range.clone()], "tail");
        let CopiedLine::Matched { range, .. } = &matched[2] else {
            panic!("expected matched last line");
        };
        assert_eq!(&line_plain_text(&l3)[range.clone()], "head");
    }

    #[test]
    fn match_treats_blank_fragment_as_empty_source_line() {
        let l1 = line(vec![segment("above", None, false)]);
        let l2 = line(vec![]);
        let l3 = line(vec![segment("below", None, false)]);
        let lines = vec![&l1, &l2, &l3];

        let matched = match_copied_text("above\n\nbelow", &lines).expect("should match");
        assert_eq!(matched.len(), 3);
        assert!(matches!(matched[1], CopiedLine::Empty));
    }

    #[test]
    fn match_rejects_text_not_in_lines() {
        let l = line(vec![segment("some game text", None, false)]);
        let lines = vec![&l];

        assert!(match_copied_text("typed into an editor", &lines).is_none());
        assert!(match_copied_text("\n\n", &lines).is_none());
    }

    #[test]
    fn match_requires_consecutive_lines() {
        let l1 = line(vec![segment("alpha", None, false)]);
        let l2 = line(vec![segment("beta", None, false)]);
        let lines = vec![&l1, &l2];

        // "alpha" then "gamma" is not a consecutive match.
        assert!(match_copied_text("alpha\ngamma", &lines).is_none());
    }

    #[test]
    fn ansi_emits_truecolor_and_reset() {
        let l = line(vec![
            segment("plain ", None, false),
            segment("red", Some("#ff0000"), true),
            segment(" tail", None, false),
        ]);
        let lines = vec![&l];
        let matched = match_copied_text("plain red tail", &lines).expect("should match");

        let ansi = copied_lines_to_ansi(&matched);
        assert_eq!(ansi, "plain \x1b[1;38;2;255;0;0mred\x1b[0m tail");
    }

    #[test]
    fn ansi_slices_partial_segments() {
        let l = line(vec![
            segment("AB", Some("#00ff00"), false),
            segment("CD", Some("#0000ff"), false),
        ]);
        let lines = vec![&l];
        let matched = match_copied_text("BC", &lines).expect("should match");

        let ansi = copied_lines_to_ansi(&matched);
        assert_eq!(ansi, "\x1b[38;2;0;255;0mB\x1b[0m\x1b[38;2;0;0;255mC\x1b[0m");
    }

    #[test]
    fn ansi_joins_lines_with_newline() {
        let l1 = line(vec![segment("one", None, false)]);
        let l2 = line(vec![segment("two", None, false)]);
        let lines = vec![&l1, &l2];
        let matched = match_copied_text("one\ntwo", &lines).expect("should match");

        assert_eq!(copied_lines_to_ansi(&matched), "one\ntwo");
    }

    #[test]
    fn html_escapes_and_styles() {
        let l = line(vec![
            segment("a<b> & ", None, false),
            segment("bold", Some("#ff8800"), true),
        ]);
        let lines = vec![&l];
        let matched = match_copied_text("a<b> & bold", &lines).expect("should match");

        let html = copied_lines_to_html(&matched);
        assert_eq!(
            html,
            "<pre style=\"font-family:monospace\">a&lt;b&gt; &amp; \
             <span style=\"color:#ff8800;font-weight:bold;\">bold</span></pre>"
        );
    }

    #[test]
    fn html_preserves_blank_lines() {
        let l1 = line(vec![segment("above", None, false)]);
        let l2 = line(vec![]);
        let l3 = line(vec![segment("below", None, false)]);
        let lines = vec![&l1, &l2, &l3];
        let matched = match_copied_text("above\n\nbelow", &lines).expect("should match");

        assert_eq!(
            copied_lines_to_html(&matched),
            "<pre style=\"font-family:monospace\">above\n\nbelow</pre>"
        );
    }
}
