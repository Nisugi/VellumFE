//! Alert pack browser (`.alertpacks`): installed packs, per-pack enable
//! toggles, and the trust digest.
//!
//! Distinct from `packs.rs`, which handles `.vellumpack` UI sharing.
//!
//! The digest is the point of this panel. A pack can carry `replace` and
//! `redirect` rules, which rewrite and reroute game text; those stay inert
//! until the user approves that pack's exact contents. A gate the user
//! cannot read is not a gate, so this shows every sensitive rule in plain
//! language, with its pattern, before asking for approval.

use super::super::VellumGuiApp;
use crate::config::{AlertPack, AlertPackApprovals, Config};
use eframe::egui;

pub(in super::super) struct AlertPacksEditorState {
    /// Packs as they were on disk when the panel opened (or last refreshed).
    packs: Vec<AlertPack>,
    /// Local enable/approval record, edited in place and saved on change.
    approvals: AlertPackApprovals,
    /// Pack whose digest is expanded, if any.
    expanded: Option<String>,
}

/// Deferred mutation, so the pack list isn't borrowed while we mutate state.
enum Op {
    Enable(String, bool),
    Approve(String),
    Revoke(String),
    /// Expand this pack's digest, or collapse if it is already expanded.
    ToggleDetails(String),
    Refresh,
}

impl VellumGuiApp {
    pub(in super::super) fn open_alertpacks_editor(&mut self) {
        if self.alertpacks_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_alertpacks_editor"));
            return;
        }
        self.alertpacks_editor = Some(AlertPacksEditorState {
            packs: Config::load_alert_packs(),
            approvals: Config::load_alertpack_approvals(),
            expanded: None,
        });
    }

    pub(in super::super) fn render_alertpacks_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.alertpacks_editor.take() else {
            return;
        };
        let mut open = true;
        let mut op: Option<Op> = None;

        egui::Window::new("Alert Packs")
            .id(egui::Id::new("gui_alertpacks_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(520.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.weak(
                    "Shared rule files from global/alertpacks/. Enabling a pack adds its \
                     rules to your highlights; your own highlights are never modified.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Rescan folder").clicked() {
                        op = Some(Op::Refresh);
                    }
                    // Show the folder rather than shelling out to open it:
                    // the path is what the user needs to drop a file in, and
                    // it costs no platform-specific launcher code.
                    if let Ok(dir) = Config::alertpacks_dir() {
                        ui.weak(dir.display().to_string());
                    }
                });
                ui.separator();

                if state.packs.is_empty() {
                    ui.label("No alert packs installed.");
                    ui.weak(
                        "Drop a .toml rule file into global/alertpacks/, or install one \
                         with .jinx, then press Rescan.",
                    );
                    return;
                }

                egui::ScrollArea::vertical()
                    .id_salt("alertpacks_scroll")
                    .show(ui, |ui| {
                        for pack in &state.packs {
                            Self::render_pack_row(ui, pack, &state, &mut op);
                            ui.separator();
                        }
                    });
            });

        match op {
            Some(Op::Refresh) => {
                state.packs = Config::load_alert_packs();
                state.approvals = Config::load_alertpack_approvals();
            }
            Some(Op::Enable(name, on)) => {
                state.approvals.set_enabled(&name, on);
                self.commit_alertpack_change(&state.approvals);
            }
            Some(Op::Approve(name)) => {
                if let Some(pack) = state.packs.iter().find(|p| p.name == name) {
                    state.approvals.approve(&pack.name, &pack.hash);
                    self.commit_alertpack_change(&state.approvals);
                }
            }
            Some(Op::Revoke(name)) => {
                state.approvals.revoke(&name);
                self.commit_alertpack_change(&state.approvals);
            }
            Some(Op::ToggleDetails(name)) => {
                state.expanded = (state.expanded.as_deref() != Some(name.as_str())).then_some(name);
            }
            None => {}
        }

        if open {
            self.alertpacks_editor = Some(state);
        }
    }

    /// One pack's row: enable toggle, rule count, and — when it carries
    /// sensitive rules — its trust status and expandable digest.
    fn render_pack_row(
        ui: &mut egui::Ui,
        pack: &AlertPack,
        state: &AlertPacksEditorState,
        op: &mut Option<Op>,
    ) {
        let enabled = state.approvals.is_enabled(&pack.name);
        let sensitive = pack.sensitive_rules();
        let approved = state.approvals.is_approved(&pack.name, &pack.hash);

        ui.horizontal(|ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "").changed() {
                *op = Some(Op::Enable(pack.name.clone(), on));
            }
            ui.strong(&pack.name);
            ui.weak(format!("{} rules", pack.rules.len()));
        });

        // Scope is worth showing unprompted: "why didn't my pack fire?" is
        // almost always "you weren't in its area".
        if !pack.scope.is_unscoped() {
            let mut parts: Vec<String> = Vec::new();
            if !pack.scope.area.is_empty() {
                parts.push(pack.scope.area.join(", "));
            }
            if !pack.scope.zone.is_empty() {
                parts.push(format!(
                    "zone {}",
                    pack.scope
                        .zone
                        .iter()
                        .map(|z| z.to_string())
                        .collect::<Vec<_>>()
                        .join("/")
                ));
            }
            if !pack.scope.tags.is_empty() {
                parts.push(format!("tagged {}", pack.scope.tags.join("/")));
            }
            if !pack.scope.rooms.is_empty() {
                parts.push(format!("{} specific room(s)", pack.scope.rooms.len()));
            }
            ui.weak(format!("    Active in: {}", parts.join("; ")));
        }

        if sensitive.is_empty() {
            ui.weak("    No sensitive rules — this pack cannot alter game text.");
            return;
        }

        // Sensitive pack: say plainly what is being withheld and why.
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            if approved {
                ui.colored_label(
                    ui.visuals().weak_text_color(),
                    format!("Approved — {} rule(s) may alter game text", sensitive.len()),
                );
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "{} rule(s) can alter game text — withheld until approved",
                        sensitive.len()
                    ),
                );
            }
        });

        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let expanded = state.expanded.as_deref() == Some(pack.name.as_str());
            let label = if expanded {
                "Hide details"
            } else {
                "Review details"
            };
            if ui.button(label).clicked() {
                *op = Some(Op::ToggleDetails(pack.name.clone()));
            }
            if approved {
                if ui
                    .button("Revoke approval")
                    .on_hover_text("Withhold this pack's replace/redirect rules again.")
                    .clicked()
                {
                    *op = Some(Op::Revoke(pack.name.clone()));
                }
            } else if ui
                .button("Approve")
                .on_hover_text(
                    "Allow this pack's replace/redirect rules to run. Approval covers \
                     these exact contents; any later change re-arms the prompt.",
                )
                .clicked()
            {
                *op = Some(Op::Approve(pack.name.clone()));
            }
        });

        if state.expanded.as_deref() == Some(pack.name.as_str()) {
            ui.add_space(2.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.weak(format!(
                    "content hash {}",
                    &pack.hash[..8.min(pack.hash.len())]
                ));
                for (rule, what) in &sensitive {
                    ui.label(format!("• {rule}: {what}"));
                }
            });
        }
    }

    /// Persist an approval change and rebuild the highlight engine so it
    /// takes effect now rather than at next launch.
    fn commit_alertpack_change(&mut self, approvals: &AlertPackApprovals) {
        if let Err(err) = Config::save_alertpack_approvals(approvals) {
            self.app_core
                .add_system_message(&format!("Failed to save alert pack settings: {err}"));
            return;
        }
        self.app_core.reload_highlights();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlertPackScope, HighlightPattern, RoomScope};
    use std::collections::HashMap;

    fn rule(pattern: &str, replace: Option<&str>) -> HighlightPattern {
        let mut rule = HighlightPattern {
            pattern: pattern.to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            case_insensitive: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: Default::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        };
        rule.replace = replace.map(|r| r.to_string());
        rule
    }

    fn pack(name: &str, hash: &str, rules: Vec<(&str, HighlightPattern)>) -> AlertPack {
        AlertPack {
            name: name.to_string(),
            hash: hash.to_string(),
            rules: rules.into_iter().map(|(n, r)| (n.to_string(), r)).collect(),
            scope: AlertPackScope::default(),
        }
    }

    fn state(packs: Vec<AlertPack>, approvals: AlertPackApprovals) -> AlertPacksEditorState {
        AlertPacksEditorState {
            packs,
            approvals,
            expanded: None,
        }
    }

    #[test]
    fn toggling_details_expands_then_collapses() {
        let mut st = state(Vec::new(), AlertPackApprovals::default());
        // Mirrors the Op::ToggleDetails arm: same pack twice collapses.
        let apply = |st: &mut AlertPacksEditorState, name: &str| {
            st.expanded = (st.expanded.as_deref() != Some(name)).then(|| name.to_string());
        };
        apply(&mut st, "reim");
        assert_eq!(st.expanded.as_deref(), Some("reim"));
        apply(&mut st, "reim");
        assert_eq!(st.expanded, None, "clicking the same pack collapses it");
        apply(&mut st, "reim");
        apply(&mut st, "osa");
        assert_eq!(st.expanded.as_deref(), Some("osa"), "switches packs");
    }

    #[test]
    fn the_panel_reports_the_same_trust_state_the_engine_enforces() {
        // The digest must not drift from what actually runs: whatever the
        // panel calls "withheld" is exactly what merge_alert_packs strips.
        let sensitive = pack(
            "p",
            "hash1",
            vec![("rewrite", rule("kobold", Some("KOBOLD")))],
        );
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("p", true);

        // Panel's view: unapproved, one sensitive rule.
        assert_eq!(sensitive.sensitive_rules().len(), 1);
        assert!(!approvals.is_approved("p", &sensitive.hash));

        // Engine's behavior must agree.
        let mut highlights = HashMap::new();
        Config::merge_alert_packs(
            &mut highlights,
            &[sensitive.clone()],
            &approvals,
            &RoomScope::default(),
        );
        assert!(highlights["pack:p/rewrite"].replace.is_none());

        // After approval, both agree again.
        approvals.approve("p", &sensitive.hash);
        let mut highlights = HashMap::new();
        Config::merge_alert_packs(
            &mut highlights,
            &[sensitive],
            &approvals,
            &RoomScope::default(),
        );
        assert_eq!(
            highlights["pack:p/rewrite"].replace.as_deref(),
            Some("KOBOLD")
        );
    }

    #[test]
    fn a_harmless_pack_shows_no_approval_affordance() {
        let harmless = pack("ambiance", "h", vec![("color", rule("rain", None))]);
        assert!(
            harmless.sensitive_rules().is_empty(),
            "no digest, no Approve button, no prompt to train through"
        );
    }

    #[test]
    fn approving_one_pack_does_not_approve_another() {
        let a = pack("a", "hash-a", vec![("r", rule("x", Some("X")))]);
        let b = pack("b", "hash-b", vec![("r", rule("y", Some("Y")))]);
        let mut approvals = AlertPackApprovals::default();
        approvals.approve(&a.name, &a.hash);

        assert!(approvals.is_approved("a", &a.hash));
        assert!(!approvals.is_approved("b", &b.hash), "approval is per pack");
    }
}
