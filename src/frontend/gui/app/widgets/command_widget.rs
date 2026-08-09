//! The command-input widget: bound editing operations over text+cursor
//! and the input row renderer.

use super::*;

impl VellumGuiApp {
    /// Apply one bound editing op to the input text + cursor. `range` is
    /// (primary, secondary) in CHARS — primary is the moving end, secondary
    /// the anchor; `extend` keeps the anchor (Shift-selection). Text ops are
    /// pure string surgery so tests can drive them without an egui frame;
    /// only Copy touches the Context (clipboard out).
    pub(in crate::frontend::gui::app) fn apply_command_edit_op(
        ctx: &egui::Context,
        text: &mut String,
        range: &mut (usize, usize),
        op: CommandEditOp,
        extend: bool,
    ) {
        fn byte_at(text: &str, char_idx: usize) -> usize {
            text.char_indices()
                .nth(char_idx)
                .map(|(b, _)| b)
                .unwrap_or(text.len())
        }
        fn remove_chars(text: &mut String, from: usize, to: usize) {
            let (a, b) = (byte_at(text, from), byte_at(text, to));
            text.drain(a..b);
        }
        fn place(range: &mut (usize, usize), pos: usize, extend: bool) {
            range.0 = pos;
            if !extend {
                range.1 = pos;
            }
        }

        let char_len = text.chars().count();
        let (p, s) = *range;
        let (sel_min, sel_max) = (p.min(s).min(char_len), p.max(s).min(char_len));
        let has_selection = sel_min != sel_max;
        let chars: Vec<char> = text.chars().collect();
        let word_left = |from: usize| {
            let mut i = from.min(chars.len());
            while i > 0 && chars[i - 1].is_whitespace() {
                i -= 1;
            }
            while i > 0 && !chars[i - 1].is_whitespace() {
                i -= 1;
            }
            i
        };
        let word_right = |from: usize| {
            let mut i = from.min(chars.len());
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            i
        };

        match op {
            CommandEditOp::Left => {
                let pos = if !extend && has_selection {
                    sel_min
                } else {
                    p.min(char_len).saturating_sub(1)
                };
                place(range, pos, extend);
            }
            CommandEditOp::Right => {
                let pos = if !extend && has_selection {
                    sel_max
                } else {
                    (p + 1).min(char_len)
                };
                place(range, pos, extend);
            }
            CommandEditOp::WordLeft => place(range, word_left(p), extend),
            CommandEditOp::WordRight => place(range, word_right(p), extend),
            CommandEditOp::Home => place(range, 0, extend),
            CommandEditOp::End => place(range, char_len, extend),
            CommandEditOp::Backspace => {
                if has_selection {
                    remove_chars(text, sel_min, sel_max);
                    *range = (sel_min, sel_min);
                } else if p > 0 && p <= char_len {
                    remove_chars(text, p - 1, p);
                    *range = (p - 1, p - 1);
                }
            }
            CommandEditOp::Delete => {
                if has_selection {
                    remove_chars(text, sel_min, sel_max);
                    *range = (sel_min, sel_min);
                } else if p < char_len {
                    remove_chars(text, p, p + 1);
                    *range = (p, p);
                }
            }
            CommandEditOp::DeleteWord => {
                if has_selection {
                    remove_chars(text, sel_min, sel_max);
                    *range = (sel_min, sel_min);
                } else {
                    let target = word_left(p);
                    remove_chars(text, target, p.min(char_len));
                    *range = (target, target);
                }
            }
            CommandEditOp::SelectAll => *range = (char_len, 0),
            CommandEditOp::Copy => {
                if has_selection {
                    let (a, b) = (byte_at(text, sel_min), byte_at(text, sel_max));
                    ctx.copy_text(text[a..b].to_string());
                }
            }
            CommandEditOp::Paste => {
                if let Ok(clip) = crate::clipboard::paste() {
                    // Single-line input: fold line breaks into spaces.
                    let clip = clip.replace(['\r', '\n'], " ");
                    if clip.is_empty() {
                        return;
                    }
                    if has_selection {
                        remove_chars(text, sel_min, sel_max);
                    }
                    let insert_at = if has_selection { sel_min } else { p.min(char_len) };
                    let byte = byte_at(text, insert_at);
                    text.insert_str(byte, &clip);
                    let after = insert_at + clip.chars().count();
                    *range = (after, after);
                }
            }
        }
    }

    /// The command input line, rendered wherever its window is docked (or
    /// in the fallback bottom panel). Render paths are `&self`, so buffer
    /// edits and key events are stashed as a `CommandInputEcho` in egui
    /// temp data and drained once per frame by the app update loop.
    pub(in crate::frontend::gui::app) fn render_command_input_widget(
        ui: &mut egui::Ui,
        seed: &str,
        completion: Option<&str>,
        drag_gutter: bool,
    ) {
        // Copy/Cut priority: when a game-window buffer selection is active, that
        // selection owns the clipboard, not this (often focused-but-unselected)
        // input. The focused TextEdit would otherwise consume the Copy event
        // during ui.add() and win the clipboard whenever the input held text.
        // Removing the Copy/Cut event here -- before the edit, regardless of
        // which window renders first -- lets the owning text window's buffer
        // copy path be the sole handler. With no active selection the event is
        // left in place, so copying from the input still works (bug #3).
        if Self::active_buffer_selection_present(ui.ctx()) {
            ui.ctx().input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Copy | egui::Event::Cut));
            });
        }

        let mut text = seed.to_string();
        let mut echo = CommandInputEcho::default();
        // Vertically center the single-line edit in whatever height the
        // window gives it.
        let edit_height = ui.text_style_height(&egui::TextStyle::Body) + 8.0;
        let pad = ((ui.available_height() - edit_height) / 2.0).max(0.0);
        if pad > 0.0 {
            ui.add_space(pad);
        }
        let edit_id = egui::Id::new(COMMAND_INPUT_EDIT_ID);
        let end = seed.chars().count();
        let completion_requested = completion.is_some()
            && ui.memory(|memory| memory.focused() == Some(edit_id))
            && egui::TextEdit::load_state(ui.ctx(), edit_id)
                .and_then(|state| state.cursor.char_range())
                .is_some_and(|range| {
                    range.primary.index.0 == end && range.secondary.index.0 == end
                })
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));

        // Bound editing actions: consume their combos BEFORE the TextEdit
        // runs (these are keys it would otherwise handle natively) and apply
        // them to the text + cursor by hand — for bound combos the keybind
        // config, not egui's built-ins, is the source of truth. A combo plus
        // Shift is the selection-extending variant of the cursor moves,
        // mirroring the TUI.
        let stash = ui
            .ctx()
            .data(|d| d.get_temp::<super::CommandInputKeys>(super::CommandInputKeys::id()))
            .unwrap_or_default();
        if ui.memory(|memory| memory.focused() == Some(edit_id)) {
            let start = egui::TextEdit::load_state(ui.ctx(), edit_id)
                .and_then(|state| state.cursor.char_range())
                .map(|range| (range.primary.index.0, range.secondary.index.0))
                .unwrap_or((end, end));
            let mut range = start;
            let mut ops: Vec<(CommandEditOp, bool)> = Vec::new();
            ui.input_mut(|input| {
                for (op, combos) in [
                    (CommandEditOp::Left, &stash.cursor_left),
                    (CommandEditOp::Right, &stash.cursor_right),
                    (CommandEditOp::WordLeft, &stash.cursor_word_left),
                    (CommandEditOp::WordRight, &stash.cursor_word_right),
                    (CommandEditOp::Home, &stash.cursor_home),
                    (CommandEditOp::End, &stash.cursor_end),
                    (CommandEditOp::Backspace, &stash.cursor_backspace),
                    (CommandEditOp::Delete, &stash.cursor_delete),
                    (CommandEditOp::DeleteWord, &stash.cursor_delete_word),
                    (CommandEditOp::SelectAll, &stash.select_all),
                    (CommandEditOp::Copy, &stash.copy),
                    (CommandEditOp::Paste, &stash.paste),
                ] {
                    for (key, mods) in combos {
                        // Shifted variant FIRST: consume_key matches
                        // modifiers logically (extra Shift is ignored), so
                        // checking the plain combo first would eat
                        // shift+Home as a plain Home and the selection
                        // extend would never fire.
                        if !mods.shift {
                            let extended = egui::Modifiers {
                                shift: true,
                                ..*mods
                            };
                            if input.consume_key(extended, *key) {
                                ops.push((op, true));
                            }
                        }
                        if input.consume_key(*mods, *key) {
                            ops.push((op, false));
                        }
                    }
                }
            });
            for (op, extend) in &ops {
                Self::apply_command_edit_op(ui.ctx(), &mut text, &mut range, *op, *extend);
            }
            if !ops.is_empty() && (range != start || text != seed) {
                let mut state =
                    egui::TextEdit::load_state(ui.ctx(), edit_id).unwrap_or_default();
                // two() normalizes order; overwrite both ends to keep the
                // primary/secondary DIRECTION (anchor vs moving end).
                let mut cursor_range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(range.1),
                    egui::text::CCursor::new(range.0),
                );
                cursor_range.primary = egui::text::CCursor::new(range.0);
                cursor_range.secondary = egui::text::CCursor::new(range.1);
                state.cursor.set_char_range(Some(cursor_range));
                state.store(ui.ctx(), edit_id);
            }
        }

        let edit = |ui: &mut egui::Ui, text: &mut String| {
            egui::TextEdit::singleline(text)
                .id(edit_id)
                .hint_text("Enter command...")
                .desired_width(ui.available_width())
                .lock_focus(true)
                .show(ui)
        };
        let output = if drag_gutter {
            // Title bar hidden: the TextEdit owns every drag in the body,
            // so this grip is the window's only drag surface. It is
            // hover-only on purpose — drags on it fall through to the
            // window body and move it.
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::Vec2::new(12.0, edit_height),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(rect) {
                    let color = ui.visuals().weak_text_color();
                    let center = rect.center();
                    for row in -1..=1i32 {
                        for col in 0..2i32 {
                            let pos = center
                                + egui::vec2(col as f32 * 4.0 - 2.0, row as f32 * 5.0);
                            ui.painter().circle_filled(pos, 1.2, color);
                        }
                    }
                }
                edit(ui, &mut text)
            })
            .inner
        } else {
            edit(ui, &mut text)
        };
        let response = &output.response;
        // Which keys drive submit/history/clear-line comes from the keybind
        // config (stashed each frame by stash_command_input_keys), so rebinding
        // works; the defaults Enter/↑/↓ are always included there too.
        let keys = ui
            .ctx()
            .data(|data| data.get_temp::<super::CommandInputKeys>(super::CommandInputKeys::id()))
            .unwrap_or_else(|| super::CommandInputKeys {
                submit: vec![egui::Key::Enter],
                history_prev: vec![egui::Key::ArrowUp],
                history_next: vec![egui::Key::ArrowDown],
                ..Default::default()
            });

        let pressed_submit = ui.input(|i| keys.submit.iter().any(|k| i.key_pressed(*k)));
        if response.lost_focus() && pressed_submit {
            echo.submit = true;
            response.request_focus();
        }
        // History browsing + clear-line. consume_key keeps these keys from
        // reaching anything else while the input has focus.
        let cursor_at_end = output.cursor_range.is_some_and(|range| {
            range.primary.index.0 == end && range.secondary.index.0 == end
        });
        if response.has_focus() {
            let accept_completion = completion_requested && text == seed && cursor_at_end;
            let up = keys
                .history_prev
                .iter()
                .any(|k| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, *k)));
            let down = keys
                .history_next
                .iter()
                .any(|k| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, *k)));
            let clear = keys
                .clear_line
                .iter()
                .any(|(k, m)| ui.input_mut(|i| i.consume_key(*m, *k)));
            if accept_completion {
                text.push_str(completion.unwrap_or_default());
                echo.text = Some(text.clone());
                echo.completion_accepted = true;
            } else if clear {
                text.clear();
                echo.text = Some(String::new());
            } else if up {
                echo.history_prev = true;
            } else if down {
                echo.history_next = true;
            }
        }
        if text == seed && cursor_at_end {
            if let (Some(suffix_text), Some(range)) = (completion, output.cursor_range) {
                let font_id = egui::TextStyle::Body.resolve(ui.style());
                let suffix = ui.painter().layout_no_wrap(
                    suffix_text.to_string(),
                    font_id,
                    ui.visuals().weak_text_color(),
                );
                let cursor = output.galley.pos_from_cursor(range.primary);
                let pos = output.galley_pos + cursor.min.to_vec2();
                ui.painter()
                    .with_clip_rect(output.text_clip_rect)
                    .galley(pos, suffix, ui.visuals().weak_text_color());
            }
        }
        if text != seed {
            echo.text = Some(text);
        }
        if !echo.is_empty() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(CommandInputEcho::id(), echo));
        }
    }

}
