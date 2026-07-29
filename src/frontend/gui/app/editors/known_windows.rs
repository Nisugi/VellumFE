//! The "Known Windows" list: every window the game has offered to show
//! (containers, dialogs, streams), each with a show/hide checkbox. This is
//! the user-facing face of the WindowOffer registry — one place to see
//! what the game wants to display and toggle each on or off, replacing the
//! all-or-nothing container-discovery toggle.

use crate::core::window_offers::{OfferKind, Policy};
use crate::frontend::gui::app::VellumGuiApp;

/// Minimal editor state: just an open flag (the data lives on GameState's
/// WindowOffers registry, so there's nothing to buffer).
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

        // Snapshot the offers (id, title, kind, shown) so the closure
        // doesn't borrow self while we mutate policy.
        let rows: Vec<(String, String, OfferKind, bool)> = self
            .app_core
            .game_state
            .window_offers
            .all()
            .map(|o| (o.id.clone(), o.title.clone(), o.kind, o.should_show()))
            .collect();

        // (offer_id, new policy) chosen this frame.
        let mut toggle: Option<(String, Policy)> = None;

        egui::Window::new("Known Windows")
            .id(egui::Id::new("gui_known_windows"))
            .open(&mut open)
            .default_width(320.0)
            .default_height(400.0)
            .show(ctx, |ui| {
                ui.label(
                    "Windows the game has offered to show. Tick one to open \
                     it, untick to hide. Choices are remembered.",
                );
                ui.separator();

                if rows.is_empty() {
                    ui.weak("None yet — look in a container or open a game dialog.");
                    return;
                }

                for kind in [OfferKind::Container, OfferKind::Dialog, OfferKind::Stream] {
                    let group: Vec<&(String, String, OfferKind, bool)> =
                        rows.iter().filter(|(_, _, k, _)| *k == kind).collect();
                    if group.is_empty() {
                        continue;
                    }
                    let heading = match kind {
                        OfferKind::Container => "Containers",
                        OfferKind::Dialog => "Dialogs",
                        OfferKind::Stream => "Streams",
                    };
                    egui::CollapsingHeader::new(heading)
                        .id_salt(kind.label())
                        .default_open(true)
                        .show(ui, |ui| {
                            for (id, title, _, shown) in group {
                                let mut checked = *shown;
                                if ui.checkbox(&mut checked, title).changed() {
                                    toggle = Some((
                                        id.clone(),
                                        if checked { Policy::Shown } else { Policy::Hidden },
                                    ));
                                }
                            }
                        });
                }
            });

        if let Some((id, policy)) = toggle {
            let (w, h) = self.core_layout_size;
            self.app_core
                .set_window_offer_policy(&id, policy, w, h);
        }
        if !open {
            self.known_windows_editor = None;
        }
    }
}
