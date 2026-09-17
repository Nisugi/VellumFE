//! Read-only text rendering of a Lich WebUI panel for the TUI.
//!
//! The GUI paints WebUI panels with native egui widgets and the phone renders
//! them as HTML; the terminal can't do either, but a WebUI window in a shared
//! layout shouldn't be dead chrome. This renders the panel's component tree as
//! a readable, indented text outline — headers, text, tables, logs, inputs
//! (shown with their current value), buttons (shown as labels), etc. Controls
//! are displayed but not operable in the TUI (v1); the outline tells the user
//! what the panel shows and its current state.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::data::webui::{WebUiNode, WebUiPanelContent};

/// Build the display lines for a WebUI panel.
pub fn render_lines(content: &WebUiPanelContent) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header: title + connection/ended state.
    let mut header = vec![Span::styled(
        content.title.clone(),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if let Some(reason) = &content.ended {
        header.push(Span::raw("  "));
        header.push(Span::styled(
            format!("[ended: {reason}]"),
            Style::default().fg(Color::Red),
        ));
    } else if !content.connected {
        header.push(Span::raw("  "));
        header.push(Span::styled(
            "[connecting…]",
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::from(header));

    match &content.tree {
        Some(tree) => render_node(tree, 0, &mut lines),
        None => lines.push(Line::from(Span::styled(
            "  (waiting for the panel to render…)",
            Style::default().fg(Color::DarkGray),
        ))),
    }

    lines
}

/// Indent string for a nesting depth (2 spaces per level).
fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn dim(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(Color::DarkGray))
}

fn label_span(text: impl Into<String>) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

/// Render one node (and its children) into `lines`.
fn render_node(node: &WebUiNode, depth: usize, lines: &mut Vec<Line<'static>>) {
    let pad = indent(depth);
    match node.t.as_str() {
        // Containers: render children (page shows nothing itself).
        "page" => {
            for child in node.children() {
                render_node(child, depth, lines);
            }
        }
        "columns" | "grid" => {
            // Lay columns/grid cells out vertically in the terminal, each cell
            // group indented under a marker so the structure is visible.
            for (i, child) in node.children().iter().enumerate() {
                lines.push(Line::from(dim(format!("{pad}· cell {}", i + 1))));
                render_node(child, depth + 1, lines);
            }
        }
        "text" => {
            // Strong emphasis is what the retired `header` node meant.
            let body = format!("{pad}{}", node.text.clone().unwrap_or_default());
            let span = match node.emphasis.as_deref() {
                Some("strong") => Span::styled(
                    body,
                    Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Some("subtle") => Span::styled(body, Style::default().add_modifier(Modifier::DIM)),
                _ => Span::raw(body),
            };
            lines.push(Line::from(span));
        }
        "markdown" => {
            // Show the markdown source verbatim; the terminal has no renderer.
            for raw in node.text.clone().unwrap_or_default().lines() {
                lines.push(Line::from(Span::raw(format!("{pad}{raw}"))));
            }
        }
        "divider" => {
            lines.push(Line::from(dim(format!("{pad}────────"))));
        }
        "button" => {
            let label = node.label.clone().unwrap_or_else(|| "button".into());
            let mut spans = vec![
                Span::raw(pad.clone()),
                Span::styled(format!("[ {label} ]"), Style::default().fg(Color::Yellow)),
            ];
            if node.disabled == Some(true) {
                spans.push(dim("  (disabled)"));
            }
            lines.push(Line::from(spans));
        }
        "text_input" | "password_input" | "number_input" => {
            let label = node.label.clone().unwrap_or_default();
            let shown = if node.t == "password_input" {
                "••••••".to_string()
            } else {
                node.value_str()
                    .map(str::to_string)
                    .or_else(|| node.value_f64().map(|v| v.to_string()))
                    .unwrap_or_default()
            };
            lines.push(Line::from(vec![
                Span::raw(pad),
                label_span(format!("{label}: ")),
                Span::raw(shown),
            ]));
        }
        "textarea" => {
            let label = node.label.clone().unwrap_or_default();
            lines.push(Line::from(vec![Span::raw(pad.clone()), label_span(label)]));
            for raw in node.value_str().unwrap_or_default().lines() {
                lines.push(Line::from(Span::raw(format!("{pad}  {raw}"))));
            }
        }
        "select" | "radio" => {
            let label = node.label.clone().unwrap_or_default();
            let current = node.value_str().unwrap_or_default();
            let opts = node.options.clone().unwrap_or_default();
            lines.push(Line::from(vec![
                Span::raw(pad.clone()),
                label_span(format!("{label}: ")),
                Span::raw(current.to_string()),
            ]));
            for opt in opts {
                let marker = if opt == current { "(•)" } else { "( )" };
                lines.push(Line::from(dim(format!("{pad}  {marker} {opt}"))));
            }
        }
        "checkbox" => {
            let mark = if node.checked == Some(true) {
                "[x]"
            } else {
                "[ ]"
            };
            let label = node.label.clone().unwrap_or_default();
            lines.push(Line::from(Span::raw(format!("{pad}{mark} {label}"))));
        }
        "slider" => {
            let label = node.label.clone().unwrap_or_default();
            let val = node.value_f64().unwrap_or(0.0);
            let (min, max) = (node.min.unwrap_or(0.0), node.max.unwrap_or(1.0));
            lines.push(Line::from(vec![
                Span::raw(pad),
                label_span(format!("{label}: ")),
                Span::raw(format!("{val}")),
                dim(format!("  ({min}..{max})")),
            ]));
        }
        "progress" => {
            let frac = node.value_f64().unwrap_or(0.0).clamp(0.0, 1.0);
            let filled = (frac * 10.0).round() as usize;
            let bar: String = "█".repeat(filled) + &"░".repeat(10usize.saturating_sub(filled));
            let text = node.text.clone().unwrap_or_default();
            lines.push(Line::from(vec![
                Span::raw(pad),
                Span::raw(format!("{bar} ")),
                dim(text),
            ]));
        }
        "log" => {
            for raw in node.lines.clone().unwrap_or_default() {
                lines.push(Line::from(Span::raw(format!("{pad}{raw}"))));
            }
        }
        "table" => {
            render_table(node, depth, lines);
        }
        // A menu is a definition other components point at by key, opened
        // with a secondary click the TUI has no way to deliver. Only a
        // `bar` menu is a visible strip; the rest would be noise in an
        // outline, so they are listed but not expanded.
        "menu" => {
            if node.bar.unwrap_or(false) {
                for child in node.children() {
                    render_node(child, depth, lines);
                }
            } else {
                let label = node.label.clone().unwrap_or_default();
                lines.push(Line::from(dim(format!("{pad}[menu {label}]"))));
            }
        }
        "menu_item" => {
            let label = node.label.clone().unwrap_or_default();
            let text = match node.kind.as_deref() {
                Some("separator") => format!("{pad}───"),
                Some("check") | Some("radio") => {
                    let mark = if node.active == Some(true) { "x" } else { " " };
                    format!("{pad}[{mark}] {label}")
                }
                _ => format!("{pad}• {label}"),
            };
            lines.push(Line::from(Span::raw(text)));
            for child in node.children() {
                render_node(child, depth + 1, lines);
            }
        }
        // Containers: the TUI renders an outline, so these are a heading
        // (where there is one) plus the children indented under it.
        "group" => {
            let label = node.label.clone().unwrap_or_default();
            let depth = if label.is_empty() {
                depth
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{pad}{label}"),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                depth + 1
            };
            for child in node.children() {
                render_node(child, depth, lines);
            }
        }
        // stack/scroll/overlay carry no text of their own; in an outline
        // they are their children. Scroll height and overlay z-order are
        // presentation the TUI has no way to express, so they are dropped
        // rather than faked.
        "stack" | "scroll" | "overlay" => {
            for child in node.children() {
                render_node(child, depth, lines);
            }
        }
        // Two named slots. The divider is worth showing because it tells
        // the reader the halves are independent panes, not a sequence.
        "split" => {
            let children = node.children();
            for (i, child) in children.iter().enumerate() {
                if i == 1 {
                    lines.push(Line::from(dim(format!("{pad}───"))));
                }
                render_node(child, depth, lines);
            }
        }
        // Same state and shape as a checkbox; only the widget differs, and
        // the TUI has no widgets.
        "toggle" => {
            let mark = if node.checked == Some(true) {
                "[on ]"
            } else {
                "[off]"
            };
            let label = node.label.clone().unwrap_or_default();
            lines.push(Line::from(Span::raw(format!("{pad}{mark} {label}"))));
        }
        "expander" => {
            let label = node.label.clone().unwrap_or_default();
            let marker = if node.open == Some(true) {
                "▾"
            } else {
                "▸"
            };
            lines.push(Line::from(vec![
                Span::raw(pad),
                Span::styled(
                    format!("{marker} {label}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]));
            if node.open != Some(false) {
                for child in node.children() {
                    render_node(child, depth + 1, lines);
                }
            }
        }
        "tabs" => {
            // Render each tab's label as a heading, then its children.
            for tab in node.children() {
                let label = tab.label.clone().unwrap_or_default();
                lines.push(Line::from(Span::styled(
                    format!("{pad}⟨ {label} ⟩"),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )));
                for child in tab.children() {
                    render_node(child, depth + 1, lines);
                }
            }
        }
        // A painted surface (the map). The TUI cannot draw it, but the
        // labels and named regions ARE readable content, so summarise
        // rather than emitting a bare "[composite]" that tells the reader
        // nothing about what is on screen.
        "composite" => {
            let layers = node.layers.as_deref().unwrap_or(&[]);
            let labels: Vec<&str> = layers
                .iter()
                .filter_map(|l| match l {
                    crate::data::webui::WebUiLayer::Label { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            let regions = layers
                .iter()
                .filter(|l| matches!(l, crate::data::webui::WebUiLayer::Region { .. }))
                .count();
            lines.push(Line::from(dim(format!(
                "{pad}▦ surface ({} layers, {regions} regions - view in the GUI)",
                layers.len()
            ))));
            for text in labels {
                lines.push(Line::from(Span::raw(format!("{pad}  {text}"))));
            }
        }
        "image" => {
            let alt = node
                .alt
                .clone()
                .or_else(|| node.src.clone())
                .unwrap_or_else(|| "image".into());
            lines.push(Line::from(dim(format!(
                "{pad}🖼 {alt} (image — view in the GUI)"
            ))));
        }
        other => {
            // Unknown/unsupported node: name it and still render any children
            // so the outline stays complete.
            lines.push(Line::from(dim(format!("{pad}[{other}]"))));
            for child in node.children() {
                render_node(child, depth + 1, lines);
            }
        }
    }
}

/// Render a `table` node as aligned columns.
fn render_table(node: &WebUiNode, depth: usize, lines: &mut Vec<Line<'static>>) {
    let pad = indent(depth);
    let headings = node.headings.clone().unwrap_or_default();
    let rows = node.rows.clone().unwrap_or_default();

    // Column widths = max over headings + all cells.
    let col_count = headings
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if col_count == 0 {
        return;
    }
    let mut widths = vec![0usize; col_count];
    let mut consider = |cells: &[String]| {
        for (i, cell) in cells.iter().enumerate() {
            if i < col_count {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    };
    consider(&headings);
    for row in &rows {
        consider(row);
    }

    let fmt_row = |cells: &[String]| -> String {
        let mut out = String::new();
        for i in 0..col_count {
            let cell = cells.get(i).map(String::as_str).unwrap_or("");
            let w = widths[i];
            out.push_str(cell);
            // Pad by char count, not bytes.
            let cell_w = cell.chars().count();
            out.push_str(&" ".repeat(w.saturating_sub(cell_w)));
            if i + 1 < col_count {
                out.push_str("  ");
            }
        }
        out
    };

    if !headings.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("{pad}{}", fmt_row(&headings)),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        let rule: String = widths
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(Line::from(dim(format!("{pad}{rule}"))));
    }
    for row in &rows {
        lines.push(Line::from(Span::raw(format!("{pad}{}", fmt_row(row)))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(t: &str) -> WebUiNode {
        WebUiNode {
            t: t.to_string(),
            ..Default::default()
        }
    }

    fn text_of(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn waiting_state_when_no_tree() {
        let content = WebUiPanelContent::new("demo/main", "Demo");
        let out = text_of(&render_lines(&content));
        assert!(out.contains("Demo"));
        assert!(out.contains("waiting"));
    }

    #[test]
    fn renders_text_and_button() {
        let mut content = WebUiPanelContent::new("demo/main", "Demo");
        let mut page = node("page");
        let mut heading = node("text");
        heading.text = Some("Status".into());
        heading.emphasis = Some("strong".into());
        let mut text = node("text");
        text.text = Some("All systems go".into());
        let mut button = node("button");
        button.label = Some("Refresh".into());
        page.children = Some(vec![heading, text, button]);
        content.tree = Some(page);

        let out = text_of(&render_lines(&content));
        assert!(out.contains("Status"));
        assert!(out.contains("All systems go"));
        assert!(out.contains("[ Refresh ]"));
    }

    /// A tag the contract does not define must be NAMED, not skipped. The
    /// retired VellumFE-only types (header, col, tab, cell) land here now,
    /// and so would a component from a newer contract.
    #[test]
    fn unknown_node_is_named_rather_than_dropped() {
        let mut content = WebUiPanelContent::new("demo/main", "Demo");
        let mut page = node("page");
        let mut retired = node("header");
        retired.text = Some("Status".into());
        page.children = Some(vec![retired]);
        content.tree = Some(page);

        let out = text_of(&render_lines(&content));
        assert!(
            out.contains("header"),
            "an unknown component must name itself; got: {out}"
        );
    }

    #[test]
    fn renders_table_aligned() {
        let mut content = WebUiPanelContent::new("demo/main", "Demo");
        let mut table = node("table");
        table.headings = Some(vec!["Name".into(), "HP".into()]);
        table.rows = Some(vec![
            vec!["Orc".into(), "40".into()],
            vec!["Goblin".into(), "8".into()],
        ]);
        content.tree = Some(table);

        let out = text_of(&render_lines(&content));
        assert!(out.contains("Name"));
        assert!(out.contains("Orc"));
        assert!(out.contains("Goblin"));
    }

    #[test]
    fn renders_checkbox_and_input_state() {
        let mut content = WebUiPanelContent::new("demo/main", "Demo");
        let mut page = node("page");
        let mut cb = node("checkbox");
        cb.label = Some("Enabled".into());
        cb.checked = Some(true);
        let mut input = node("text_input");
        input.label = Some("Name".into());
        input.value = Some(serde_json::Value::String("Nisugi".into()));
        page.children = Some(vec![cb, input]);
        content.tree = Some(page);

        let out = text_of(&render_lines(&content));
        assert!(out.contains("[x] Enabled"));
        assert!(out.contains("Name: "));
        assert!(out.contains("Nisugi"));
    }

    #[test]
    fn password_input_is_masked() {
        let mut content = WebUiPanelContent::new("demo/main", "Demo");
        let mut input = node("password_input");
        input.label = Some("Pass".into());
        input.value = Some(serde_json::Value::String("secret".into()));
        content.tree = Some(input);

        let out = text_of(&render_lines(&content));
        assert!(!out.contains("secret"), "password must be masked");
        assert!(out.contains("••••••"));
    }

    #[test]
    fn unknown_node_still_renders_children() {
        let mut content = WebUiPanelContent::new("demo/main", "Demo");
        let mut weird = node("some_future_widget");
        let mut text = node("text");
        text.text = Some("inner".into());
        weird.children = Some(vec![text]);
        content.tree = Some(weird);

        let out = text_of(&render_lines(&content));
        assert!(out.contains("[some_future_widget]"));
        assert!(out.contains("inner"));
    }

    /// The eight types added in Phase 5 must render something. A container
    /// that silently produced nothing is exactly the failure the generated
    /// vocabulary exists to prevent, so assert each one reaches the output
    /// rather than the name-the-unknown arm.
    #[test]
    fn phase5_container_types_render_their_children() {
        for tag in ["group", "stack", "scroll", "overlay", "split"] {
            let mut content = WebUiPanelContent::new("demo/main", "Demo");
            let mut page = node("page");
            let mut container = node(tag);
            let mut child = node("text");
            child.text = Some("inner".into());
            container.children = Some(vec![child]);
            page.children = Some(vec![container]);
            content.tree = Some(page);

            let out = text_of(&render_lines(&content));
            assert!(out.contains("inner"), "{tag} dropped its children: {out}");
            assert!(
                !out.contains(&format!("[{tag}]")),
                "{tag} fell through to the unknown arm: {out}"
            );
        }
    }

    /// toggle carries the same state as a checkbox and must show it.
    #[test]
    fn toggle_shows_its_state() {
        for (checked, want) in [(true, "[on ]"), (false, "[off]")] {
            let mut content = WebUiPanelContent::new("demo/main", "Demo");
            let mut page = node("page");
            let mut toggle = node("toggle");
            toggle.checked = Some(checked);
            toggle.label = Some("Sound".into());
            page.children = Some(vec![toggle]);
            content.tree = Some(page);

            let out = text_of(&render_lines(&content));
            assert!(out.contains(want), "toggle({checked}) missing {want}: {out}");
            assert!(out.contains("Sound"));
        }
    }

    /// A bar menu is a visible strip, so its items render. A popup menu is a
    /// definition opened by a gesture the TUI cannot deliver, so it is named
    /// rather than expanded — listing its items inline would imply they are
    /// on the page when they are not.
    #[test]
    fn menu_expands_only_when_it_is_a_bar() {
        let build = |bar: bool| {
            let mut content = WebUiPanelContent::new("demo/main", "Demo");
            let mut page = node("page");
            let mut menu = node("menu");
            menu.bar = Some(bar);
            let mut item = node("menu_item");
            item.label = Some("Save".into());
            menu.children = Some(vec![item]);
            page.children = Some(vec![menu]);
            content.tree = Some(page);
            text_of(&render_lines(&content))
        };
        assert!(build(true).contains("Save"), "a bar menu shows its items");
        let popup = build(false);
        assert!(!popup.contains("Save"), "a popup menu is not on the page: {popup}");
        assert!(popup.contains("menu"), "a popup menu still names itself: {popup}");
    }
}
