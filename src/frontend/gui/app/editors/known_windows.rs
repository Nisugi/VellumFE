//! The "Known Windows" list: every window the client knows about (layout
//! widgets, game-discovered dialogs/streams, and session-only containers/
//! panels), each with a show/hide checkbox. U3: reads the unified
//! enumerate_known_windows — the layout + ephemeral runtime is the single
//! source of truth; there is no separate offer registry.

use crate::core::known_windows::KnownWindowKind;
use crate::frontend::gui::app::VellumGuiApp;

/// Minimal editor state: just an open flag (the data lives on the layout /
/// ui_state, so there's nothing to buffer).
pub(in super::super) struct KnownWindowsEditorState;

impl VellumGuiApp {
    pub(in super::super) fn open_known_windows_editor(&mut self) {
        if self.known_windows_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_known_windows"));
            return;
        }
        self.known_windows_editor = Some(KnownWindowsEditorState);
    }

    pub(in super::super) fn render_known_windows_editor(&mut self, ctx: &egui::Context) {
        if self.known_windows_editor.is_none() {
            return;
        }
        let mut open = true;

        // Snapshot (name, title, kind, shown) so the closure doesn't borrow
        // self while we mutate.
        let rows: Vec<(String, String, KnownWindowKind, bool)> = self
            .app_core
            .enumerate_known_windows()
            .into_iter()
            .map(|k| (k.name, k.title, k.kind, k.shown))
            .collect();

        // (window name, new shown state) chosen this frame.
        let mut toggle: Option<(String, bool)> = None;

        egui::Window::new("Windows")
            .id(egui::Id::new("gui_known_windows"))
            .open(&mut open)
            .default_width(320.0)
            .default_height(400.0)
            .show(ctx, |ui| {
                ui.label(
                    "Every window the client knows about. Tick to show, untick \
                     to hide. Hidden windows also stay hidden when the game \
                     re-sends them.",
                );
                ui.separator();

                if rows.is_empty() {
                    ui.weak("None yet — look in a container or open a game dialog.");
                    return;
                }

                for kind in KnownWindowKind::MENU_ORDER {
                    let group: Vec<&(String, String, KnownWindowKind, bool)> =
                        rows.iter().filter(|(_, _, k, _)| *k == kind).collect();
                    if group.is_empty() {
                        continue;
                    }
                    let heading = match kind {
                        KnownWindowKind::Layout => "Windows",
                        KnownWindowKind::Dialog => "Dialogs",
                        KnownWindowKind::Stream => "Streams",
                        KnownWindowKind::Container => "Containers (session)",
                    };
                    egui::CollapsingHeader::new(heading)
                        .id_salt(kind.label())
                        .default_open(true)
                        .show(ui, |ui| {
                            for (name, title, _, shown) in group {
                                let mut checked = *shown;
                                if ui.checkbox(&mut checked, title).changed() {
                                    toggle = Some((name.clone(), checked));
                                }
                            }
                        });
                }
            });

        if let Some((name, shown)) = toggle {
            let (w, h) = self.core_layout_size;
            self.app_core.set_known_window_shown(&name, shown, w, h);
        }
        if !open {
            self.known_windows_editor = None;
        }
    }
}
