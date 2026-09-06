//! Test module of the parent facade, split out for size —
//! `super` is still the parent module, so private access and
//! `use super::*` semantics are identical to the inline mod.

use super::*;

#[test]
fn injury_name_to_level_maps_wounds_scars_and_nsys() {
    // Standard wounds.
    assert_eq!(injury_name_to_level("rightLeg", "Injury1"), 1);
    assert_eq!(injury_name_to_level("chest", "Injury3"), 3);
    // Scars are levels 4-6.
    assert_eq!(injury_name_to_level("leftArm", "Scar1"), 4);
    assert_eq!(injury_name_to_level("leftArm", "Scar3"), 6);
    // Nervous system reports under its OWN prefix — the regression: these
    // must map like Injury, not fall through to 0.
    assert_eq!(injury_name_to_level("nsys", "Nsys1"), 1);
    assert_eq!(injury_name_to_level("nsys", "Nsys2"), 2);
    assert_eq!(injury_name_to_level("nsys", "Nsys3"), 3);
    // Cleared: name equals the body-part id, or an unknown name.
    assert_eq!(injury_name_to_level("nsys", "nsys"), 0);
    assert_eq!(injury_name_to_level("rightLeg", "rightLeg"), 0);
    assert_eq!(injury_name_to_level("blood", "Transparent"), 0);
}

// ===========================================
// Stream routing precedence (route_for)
// ===========================================

fn routes(entries: &[(&str, StreamRoute)]) -> std::collections::BTreeMap<String, StreamRoute> {
    entries
        .iter()
        .map(|(id, route)| (id.to_string(), route.clone()))
        .collect()
}

fn deliver(candidates: &[&str]) -> RouteDecision {
    RouteDecision::Deliver {
        candidates: candidates.iter().map(|c| c.to_string()).collect(),
    }
}

#[test]
fn route_subscribed_window_always_wins() {
    // Even a discard route loses to a subscribed window.
    let map = routes(&[("speech", StreamRoute::Discard)]);
    assert_eq!(
        route_for("speech", true, &map, "main"),
        RouteDecision::Subscribed
    );
}

#[test]
fn route_discard_drops_orphaned_stream() {
    let map = routes(&[("speech", StreamRoute::Discard)]);
    assert_eq!(
        route_for("speech", false, &map, "main"),
        RouteDecision::Discard
    );
    // Lookup is case-insensitive, matching the legacy drop list.
    assert_eq!(
        route_for("SPEECH", false, &map, "main"),
        RouteDecision::Discard
    );
}

#[test]
fn route_main_delivers_to_main() {
    let map = routes(&[("ooc", StreamRoute::Main)]);
    assert_eq!(route_for("ooc", false, &map, "story"), deliver(&["main"]));
}

#[test]
fn route_window_prefers_window_then_fallback_then_main() {
    let map = routes(&[("bounty", StreamRoute::Window("bounty".to_string()))]);
    // Delivery takes the first candidate window that exists, so a
    // missing "bounty" window falls back to "story", then "main" —
    // never auto-creating or auto-opening anything.
    assert_eq!(
        route_for("bounty", false, &map, "story"),
        deliver(&["bounty", "story", "main"])
    );
    // Duplicates collapse (fallback already "main").
    assert_eq!(
        route_for("bounty", false, &map, "main"),
        deliver(&["bounty", "main"])
    );
}

#[test]
fn route_unrouted_stream_keeps_fallback_behavior() {
    let map = routes(&[("speech", StreamRoute::Discard)]);
    assert_eq!(
        route_for("bounty", false, &map, "story"),
        deliver(&["story", "main"])
    );
    assert_eq!(route_for("bounty", false, &map, "main"), deliver(&["main"]));
    let empty = routes(&[]);
    assert_eq!(
        route_for("anything", false, &empty, "main"),
        deliver(&["main"])
    );
}

// ===========================================
// Helper function to create minimal processor for testing
// ===========================================

fn create_test_processor() -> MessageProcessor {
    let config = Config::default();
    MessageProcessor::new(config, SavedDialogPositions::default())
}

fn make_redirect_pattern(pattern: &str) -> crate::config::HighlightPattern {
    crate::config::HighlightPattern {
        pattern: pattern.to_string(),
        fg: None,
        bg: None,
        bold: false,
        color_entire_line: false,
        fast_parse: true,
        case_insensitive: false,
        sound: None,
        sound_volume: None,
        rumble: None,
        category: None,
        squelch: false,
        silent_prompt: false,
        redirect_to: Some("alerts".to_string()),
        redirect_mode: crate::config::RedirectMode::RedirectOnly,
        replace: None,
        stream: None,
        window: None,
        set_status: None,
        status_duration: None,
        clear_status: None,
        alert: None,
        compiled_regex: None,
    }
}

// ===========================================
// GameObjects registry dual-write (migration step 2)
// ===========================================

#[test]
fn inventory_scan_captures_status_then_prompt_writes_registry() {
    use crate::data::widget::{LinkData, TextSegment};
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    // Start the scan (the caller would send the returned command).
    assert_eq!(processor.start_inventory_scan(), Some("inventory full"));
    assert!(processor.inventory_scan_in_flight());
    // Starting again while in flight is a no-op.
    assert_eq!(processor.start_inventory_scan(), None);

    // Feed reply lines as segments (what the flush path would pass).
    let link = |id: &str, noun: &str, name: &str| TextSegment {
        text: name.to_string(),
        link_data: Some(LinkData {
            exist_id: id.to_string(),
            noun: noun.to_string(),
            text: name.to_string(),
            coord: None,
        }),
        ..Default::default()
    };
    // header (no link) — captured for the window, no status.
    processor
        .inv_scan
        .ingest_segments(&[TextSegment::plain("You are currently wearing:")]);
    processor.inv_scan.ingest_segments(&[
        TextSegment::plain("  some "),
        link("1", "gloves", "triton hide gloves"),
        TextSegment::plain(" with knuckles (registered) (marked)"),
    ]);
    processor
        .inv_scan
        .ingest_segments(&[TextSegment::plain("  a "), link("2", "ring", "plain ring")]);

    // The prompt finalizes into the registry.
    let prompt = ParsedElement::Prompt {
        time: "0".to_string(),
        text: ">".to_string(),
    };
    processor.process_element(
        &prompt,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    assert!(!processor.inventory_scan_in_flight());
    let s1 = game_state.objects.status_of("1").unwrap();
    assert_eq!(s1.registered, Some(true));
    assert_eq!(s1.marked, Some(true));
    let s2 = game_state.objects.status_of("2").unwrap();
    assert_eq!(s2.registered, Some(false), "in reply, no marker = false");
    assert_eq!(s2.marked, Some(false));
}

#[test]
fn discovery_routes_container_signal_bank_popup_stream_queue() {
    // U3: no offer registry. A container sets the newly_registered
    // signal; a dialog (bank) pops up only once the user shows it; a
    // streamWindow pushes a WindowDiscovery for AppCore to bind.
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let feed = [
        ParsedElement::Container {
            id: "77".to_string(),
            title: "Backpack".to_string(),
            target: Some("#77".to_string()),
        },
        ParsedElement::DialogOpen {
            id: "bank".to_string(),
            title: Some("Bank".to_string()),
            save: true,
            location: None,
        },
        ParsedElement::StreamWindow {
            id: "thoughts".to_string(),
            subtitle: None,
            title: Some("Thoughts".to_string()),
        },
    ];
    for element in &feed {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    // Container → registry + newly-registered signal.
    assert!(game_state.objects.container("77").is_some());
    assert_eq!(
        processor.newly_registered_container,
        Some(("77".to_string(), "Backpack".to_string()))
    );
    // U6: bank does NOT pop up by default (hidden-until-shown —
    // nothing pops unless its id is in shown_dialog_ids).
    assert!(ui_state.active_dialog.is_none());
    // Stream → a WindowDiscovery for AppCore to register.
    let disc = &ui_state.pending_window_discoveries;
    assert!(disc
        .iter()
        .any(|d| d.id == "thoughts" && d.kind == crate::data::WindowDiscoveryKind::Stream));

    // But once the user shows "bank", its re-sent openDialog pops up.
    ui_state.shown_dialog_ids.insert("bank".to_string());
    processor.process_element(
        &ParsedElement::DialogOpen {
            id: "bank".to_string(),
            title: Some("Bank".to_string()),
            save: true,
            location: None,
        },
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    assert!(ui_state
        .active_dialog
        .as_ref()
        .is_some_and(|d| d.id == "bank"));
}

#[test]
#[ignore = "diagnostic: prints resolved rects for the real frame"]
fn uberbar_dump_resolved_rects() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    let frame1 = include_str!("../../../tests/fixtures/uberbar_frame.xml");
    let frame2 = include_str!("../../../tests/fixtures/uberbar_update_frame.xml");
    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    let mut feed =
        |ui_state: &mut UiState, gs: &mut GameState, frame: &str, proc: &mut MessageProcessor| {
            for element in &parser.parse_line(frame) {
                proc.process_element(
                    element,
                    gs,
                    ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
        };
    feed(&mut ui_state, &mut game_state, frame1, &mut processor);
    eprintln!("--- after OPEN frame (clear=t) ---");
    {
        let d = ui_state.dialog_store.get("UberBar").unwrap();
        eprintln!(
            "  labels in store: {}",
            d.display_labels
                .iter()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    feed(&mut ui_state, &mut game_state, frame2, &mut processor);
    eprintln!("--- after UPDATE frame (no clear) ---");
    let dialog = ui_state.dialog_store.get("UberBar").unwrap();
    eprintln!(
        "  labels in store: {}",
        dialog
            .display_labels
            .iter()
            .map(|l| l.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    let (controls, size) = dialog.positioned_controls().unwrap();
    eprintln!("canvas = {:?}", size);
    use crate::data::ui_state::PositionedControlKind as K;
    for c in &controls {
        let name = match c.kind {
            K::Skin(i) => format!("skin:{}", dialog.skins[i].id),
            K::ProgressBar(i) => format!("bar:{}", dialog.progress_bars[i].id),
            K::Label(i) => format!(
                "label:{}={}",
                dialog.display_labels[i].id, dialog.display_labels[i].value
            ),
            K::Button(i) => format!("btn:{}", dialog.buttons[i].id),
            K::DropDown(i) => format!("dd:{}", dialog.dropdowns[i].id),
            K::Image(i) => format!("img:{}", dialog.images[i].id),
            K::Link(i) => format!("link:{}", dialog.links[i].id),
            K::SpinBox(i) => format!("spin:{}", dialog.spinboxes[i].id),
        };
        eprintln!(
            "  {:<22} x={:6.1} y={:6.1} w={:6.1} h={:5.1}",
            name, c.rect.0, c.rect.1, c.rect.2, c.rect.3
        );
    }
}

#[test]
fn uberbar_partial_update_preserves_the_label_column() {
    // Resident panels re-send only CHANGED rows (no clear='t'). The bug:
    // display_labels were REPLACED wholesale, so an update carrying a few
    // values wiped the whole label column — the "Today:/Pulse:" labels
    // vanished and value labels jumped (their anchor_left target gone).
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    let mut feed = |ui: &mut UiState, gs: &mut GameState, f: &str, p: &mut MessageProcessor| {
        for e in &parser.parse_line(f) {
            p.process_element(
                e,
                gs,
                ui,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    };

    // Open: a label + its value, both positioned.
    feed(&mut ui_state, &mut game_state,
            "<openDialog id='UB' resident='true'><dialogData id='UB' clear='t'>\
             <label id='ublog' value='Today:' anchor_left='ubinjury' top='5' left='5' width='50' height='15'/>\
             <label id='ublogv' value='0' justify='6' anchor_left='ublog' top='5' left='0' width='50' height='15'/>\
             </dialogData></openDialog>", &mut processor);

    // Update: ONLY the value changes, no clear, label not re-sent.
    feed(&mut ui_state, &mut game_state,
            "<dialogData id='UB'>\
             <label id='ublogv' value='42' anchor_left='ublog' top='5' left='0' width='50' height='15'/>\
             </dialogData>", &mut processor);

    let d = ui_state.dialog_store.get("UB").unwrap();
    assert!(
        d.display_labels
            .iter()
            .any(|l| l.id == "ublog" && l.value == "Today:"),
        "the label column must survive a partial update; labels: {:?}",
        d.display_labels.iter().map(|l| &l.id).collect::<Vec<_>>()
    );
    assert!(
        d.display_labels
            .iter()
            .any(|l| l.id == "ublogv" && l.value == "42"),
        "the value must update in place"
    );
    // And its anchor still resolves (value sits right of the label, not
    // collapsed to an absolute fallback).
    let (controls, _) = d.positioned_controls().unwrap();
    use crate::data::ui_state::PositionedControlKind as K;
    let xof = |id: &str| {
        d.display_labels
            .iter()
            .position(|l| l.id == id)
            .and_then(|i| {
                controls
                    .iter()
                    .find(|c| c.kind == K::Label(i))
                    .map(|c| c.rect.0)
            })
    };
    assert!(
        xof("ublogv").unwrap() > xof("ublog").unwrap(),
        "value stays right of its label"
    );
}

#[test]
fn uberbar_real_frame_multispace_positions() {
    // The REAL frame uses multi-space attribute formatting and a
    // PanelBackground skin ('ubbars') that health anchors to via
    // anchor_top='ubbars'. Assert positioned_controls() returns Some
    // (the DialogPanel renderer has no flow fallback, so None = blank).
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let frame = "<openDialog type='dynamic' id='UberBar' title='x' location='main' resident='true'><dialogData id='UberBar' clear='t'>\
<skin id='ubinjury'    name='InjuriesPanel'    controls='nsys,head' top='5' left='5' width='100' height='150' align='nw'/>\
<label id='ublog'    value='Today:'      justify='4'  anchor_left='ubinjury'  align='n'    top='5' left='5' height='15' width='50'/>\
<image id='ubbars'    name='PanelBackground'    justify='4'  anchor_left='ubinjury'  align='n'    top='3' left='5' height='120' width='0'/>\
<progressBar id='health'    value='100'  text='193/193'  customText='t' anchor_left='ubinjury' anchor_top='ubbars'  top='3' left='4' width='100' height='15'/>\
</dialogData></openDialog>";

    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    for element in &parser.parse_line(frame) {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let dialog = ui_state.dialog_store.get("UberBar").expect("store entry");
    assert!(
        dialog.progress_bars.iter().any(|b| b.id == "health"),
        "health bar ingested"
    );
    assert!(
        dialog.display_labels.iter().any(|l| l.id == "ublog"),
        "label ingested"
    );
    let positioned = dialog.positioned_controls();
    assert!(
            positioned.is_some(),
            "positioned_controls() is None -> panel renders BLANK. bars={:?} labels={:?} their layouts: bar={:?} label={:?}",
            dialog.progress_bars.iter().map(|b| &b.id).collect::<Vec<_>>(),
            dialog.display_labels.iter().map(|l| &l.id).collect::<Vec<_>>(),
            dialog.progress_bars.first().map(|b| b.layout.is_some()),
            dialog.display_labels.first().map(|l| l.layout.is_some()),
        );
}

#[test]
fn uberbar_real_frame_populates_the_dialog_store() {
    // A faithful slice of the REAL on-the-wire frame (from Nisugi's log),
    // including the unescaped apostrophe in title='Nisugi's Uberbar'.
    // After processing, the UberBar dialog store must hold the bars and
    // labels — if it does, an empty panel is a RENDER bug, not parse/ingest.
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let frame = "<closeDialog id='UberBar'/><openDialog type='dynamic' id='UberBar' title='Nisugi's Uberbar' target='UberBar' location='main' height='282' width='190' resident='true'><dialogData id='UberBar' clear='t'><skin id='ubinjury' name='InjuriesPanel' controls='nsys,head' top='5' left='5' width='100' height='150' align='nw'/><image id='nsys' name='nsys' cmd='cure nerves' tooltip='cure nerves' height='0' width='0'/><label id='ublog' value='Today:' justify='4' anchor_left='ubinjury' align='n' top='5' left='5' height='15' width='50'/><label id='ublogv' value='1234' justify='6' anchor_left='ublog' align='n' top='5' left='0' height='15' width='50'/><progressBar id='health' value='95' text='95/100' customText='t' anchor_left='ubinjury' anchor_top='ubbars' top='3' left='4' width='100' height='15'/></dialogData></openDialog>";

    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    let elements = parser.parse_line(frame);
    for element in &elements {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let dialog = ui_state
        .dialog_store
        .get("UberBar")
        .expect("UberBar dialog store entry must exist after processing");
    assert!(
        dialog.progress_bars.iter().any(|b| b.id == "health"),
        "health bar not in store; bars: {:?}",
        dialog
            .progress_bars
            .iter()
            .map(|b| &b.id)
            .collect::<Vec<_>>()
    );
    assert!(
        dialog
            .display_labels
            .iter()
            .any(|l| l.id == "ublogv" && l.value == "1234"),
        "value label not in store; labels: {:?}",
        dialog
            .display_labels
            .iter()
            .map(|l| (&l.id, &l.value))
            .collect::<Vec<_>>()
    );
    assert!(
        dialog.skins.iter().any(|s| s.name == "InjuriesPanel"),
        "InjuriesPanel skin not in store"
    );
    // And the anchor grid must resolve to positioned controls (non-flow).
    assert!(
        dialog.positioned_controls().is_some(),
        "positioned_controls returned None — panel would render nothing positioned"
    );
}

#[test]
fn uberbar_resident_openDialog_registers_a_dialogpanel_discovery() {
    // Bug repro: launching uberbar_eo (a resident openDialog id='UberBar')
    // never adds a row to the Windows list. Trace parse -> process and
    // assert a DialogPanel discovery is queued for AppCore to register.
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    // The real opening frame the script emits (trimmed).
    let frame = "<closeDialog id='UberBar'/>\
            <openDialog type='dynamic' id='UberBar' title=\"Nisugi's Uberbar\" target='UberBar' location='main' height='282' width='190' resident='true'>\
            <dialogData id='UberBar' clear='t'>\
            <skin id='ubinjury' name='InjuriesPanel' controls='nsys,leftArm,rightArm' top='5' left='5' width='100' height='150' align='nw'/>\
            <progressBar id='health' value='95' text='95/100' customText='t' anchor_left='ubinjury' anchor_top='ubbars' top='3' left='4' width='100' height='15'/>\
            </dialogData></openDialog>";

    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    let elements = parser.parse_line(frame);

    // The parser must emit a DialogPanelOpen for the resident dialog.
    assert!(
        elements
            .iter()
            .any(|e| matches!(e, ParsedElement::DialogPanelOpen { id, .. } if id == "UberBar")),
        "parser did not emit DialogPanelOpen for UberBar; got: {:?}",
        elements
            .iter()
            .map(|e| format!("{:?}", std::mem::discriminant(e)))
            .collect::<Vec<_>>()
    );

    for element in &elements {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    // And processing it must queue a DialogPanel discovery.
    assert!(
        ui_state
            .pending_window_discoveries
            .iter()
            .any(|d| d.id == "UberBar" && d.kind == crate::data::WindowDiscoveryKind::DialogPanel),
        "no DialogPanel discovery queued for UberBar (is claims_dialog('UberBar') wrongly true? \
             discoveries: {:?})",
        ui_state
            .pending_window_discoveries
            .iter()
            .map(|d| (&d.id, &d.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn dialog_popup_gated_on_shown_dialog_ids() {
    // U6: a dialog pops up ONLY if the user has shown it (its id in
    // shown_dialog_ids). Empty set = nothing pops up.
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    let open = |id: &str| ParsedElement::DialogOpen {
        id: id.to_string(),
        title: Some(id.to_string()),
        save: false,
        location: None,
    };

    // Not shown → no popup.
    processor.process_element(
        &open("shop"),
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    assert!(ui_state.active_dialog.is_none());

    // Shown → pops up.
    ui_state.shown_dialog_ids.insert("shop".to_string());
    processor.process_element(
        &open("shop"),
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    assert!(ui_state
        .active_dialog
        .as_ref()
        .is_some_and(|d| d.id == "shop"));
}

#[test]
fn hidden_combat_dialogdata_never_opens_popup() {
    // Real shapes from a 2026-07-28 session log: the combat window is a
    // RESIDENT openDialog (so no DialogOpen is emitted) whose dialogData
    // then arrives both embedded and standalone. The user never showed
    // 'combat', so none of it may create the generic popup.
    let mut parser = crate::parser::XmlParser::new();
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let lines = [
            "<openDialog type='dynamic' id='combat' title='Combat' location='right' target='combat' height='288' resident='true'><dialogData id='combat' clear='t'><image id='unsheathe' name='SwordBtn' cmd='_ready weapon' tooltip='Unsheathe Weapon' echo='ready weapon' align='n' top='3' left='-50' height='29' width='29'/></dialogData></openDialog>",
            "<dialogData id='combat'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='51' width='130' height='16' left='0' align='n' tooltip='Percent of stance contributing to defense'/></dialogData>",
            "<dialogData id='combat'><cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' tooltip='Assume a Defensive Stance' echo='stance defensive' height='20' width='55' top='70' left='0' align='nw'/><cmdButton id='cmdTarget' value='target' cmd='target random' tooltip='Select a Random Target' height='20' width='55' top='93' left='0' align='nw'/></dialogData>",
        ];
    for line in &lines {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    assert!(
        ui_state.active_dialog.is_none(),
        "hidden combat dialogData opened the generic popup: {:?}",
        ui_state.active_dialog.as_ref().map(|d| &d.id)
    );
    // It was recorded as a DialogPanel discovery (Hidden by default).
    let disc = ui_state
        .pending_window_discoveries
        .iter()
        .find(|d| d.id == "combat");
    assert!(
        disc.is_some_and(|d| d.kind == crate::data::WindowDiscoveryKind::DialogPanel),
        "combat should be a DialogPanel discovery"
    );
    // Even hidden, its full state accumulated in the store, so showing
    // it later renders fully formed rather than from deltas.
    let stored = ui_state.dialog_store.get("combat").expect("combat stored");
    assert_eq!(stored.progress_bars.len(), 1, "stance bar stored");
    assert_eq!(stored.buttons.len(), 2, "both stance/target buttons stored");
}

#[test]
fn combat_registers_as_resident_and_ingests_all_controls() {
    // The real login-time combat panel (2026-01 log): resident
    // openDialog + the full set of dialogData chunks. It must register
    // as a RESIDENT dialog offer and accumulate every control type in
    // the store (icons, links, spinbox, buttons, dropdowns, bar).
    let mut parser = crate::parser::XmlParser::new();
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let lines = [
            "<openDialog type='dynamic' id='combat' title='Combat' location='right' target='combat' height='288' resident='true'><dialogData id='combat' clear='t'><image id='unsheathe' name='SwordBtn' cmd='_ready weapon' tooltip='Unsheathe Weapon' align='n' top='3' left='-50' height='29' width='29'/><link id='lnConfigure' value='configure' cmd='_cmbtpl configure dialog' top='30' align='n' left='0'/></dialogData></openDialog>",
            "<dialogData id='combat'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='51' width='130' height='16' left='0' align='n'/></dialogData>",
            "<dialogData id='combat'><cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' top='70' left='0' align='nw'/><cmdButton id='cmdOffStance' value='offense' cmd='_stance offensive' top='70' left='0' align='ne'/></dialogData>",
            "<dialogData id='combat'><dropDownBox id='dDBStance' value='defensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70' anchor_left='cmdDefStance' anchor_right='cmdOffStance'/></dialogData>",
            "<dialogData id='combat'><upDownEditBox id='uDEQuickstrike' min='-60' max='60' value='-1' top='231' left='0' width='50' height='26'/><cmdButton id='cmdQuickstrike' value='prepare to quickstrike' cmd='quickstrike %uDEQuickstrike%' top='234' left='53'/></dialogData>",
            "<dialogData id='combat'><link id='lnSkin' value='skin' cmd='_skin' top='260' left='0'/><link id='mstrike' value='multistrike' cmd='mstrike'/></dialogData>",
        ];
    for line in &lines {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    // Combat is recorded as a DialogPanel discovery (Hidden by default)
    // and never pops up as a transient dialog.
    let disc = ui_state
        .pending_window_discoveries
        .iter()
        .find(|d| d.id == "combat")
        .expect("combat discovery");
    assert_eq!(disc.kind, crate::data::WindowDiscoveryKind::DialogPanel);
    assert!(ui_state.active_dialog.is_none(), "no transient popup");

    // Store accumulated the whole panel.
    let s = ui_state.dialog_store.get("combat").expect("stored");
    assert_eq!(s.images.len(), 1, "sword icon");
    assert_eq!(s.buttons.len(), 3, "defense + offense + quickstrike");
    assert_eq!(s.dropdowns.len(), 1, "stance");
    assert_eq!(s.spinboxes.len(), 1, "quickstrike offset");
    assert_eq!(s.progress_bars.len(), 1, "stance bar");
    assert_eq!(s.links.len(), 3, "configure + skin + multistrike");
    // %id% resolves the spinbox value in a button command.
    assert_eq!(
        s.command_with_placeholders("quickstrike %uDEQuickstrike%"),
        "quickstrike -1"
    );
}

#[test]
fn always_ingest_store_accumulates_hidden_dialog() {
    // The core fix: the game sends combat's definition (here as a batch);
    // combat was never shown so no popup appears, but the store ingests
    // the whole panel so showing it later renders fully formed.
    let mut parser = crate::parser::XmlParser::new();
    let mut processor = create_test_processor();
    let mut app = crate::core::AppCore::new_for_test();

    let lines = [
            "<dialogData id='combat' clear='t'><progressBar id='pbarStance' value='100' text='defensive (100%)' top='51'/></dialogData>",
            "<dialogData id='combat'><cmdButton id='cmdDefStance' value='defense' cmd='_stance defensive' top='70' left='0' align='nw'/><cmdButton id='cmdAttack' value='attack' cmd='attack' top='93' left='55' align='ne'/><dropDownBox id='dDBStance' value='defensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70'/></dialogData>",
        ];
    for line in &lines {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut app.game_state,
                &mut app.ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    // Hidden → no transient popup, but fully stored.
    assert!(app.ui_state.active_dialog.is_none());
    let stored = app.ui_state.dialog_store.get("combat").expect("stored");
    assert_eq!(stored.buttons.len(), 2);
    assert_eq!(stored.dropdowns.len(), 1);
    assert_eq!(stored.progress_bars.len(), 1);
}

#[test]
fn shown_dialog_updates_replace_controls_by_id() {
    // The always-ingest store replaces same-id controls (no pile-up) on
    // every dialogData refresh — independent of whether it's shown.
    // Assert against the store (the update-by-id happens there).
    let mut parser = crate::parser::XmlParser::new();
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let chunk = "<dialogData id='combat'><cmdButton id='cmdTarget' value='target' cmd='target random' top='93' left='0'/><cmdButton id='cmdAttack' value='attack' cmd='attack' top='93' left='55'/><dropDownBox id='dDBStance' value='defensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70'/></dialogData>";
    let updated = "<dialogData id='combat'><dropDownBox id='dDBStance' value='offensive' cmd='_stance %dDBStance%' content_text='offensive,defensive' content_value='offensive,defensive' top='70'/></dialogData>";
    for line in [chunk, chunk, updated] {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    let dialog = ui_state.dialog_store.get("combat").expect("stored");
    // Re-sent controls replaced their same-id entries, no pile-up
    // (the old extend produced target/attack duplicates live).
    assert_eq!(dialog.buttons.len(), 2, "buttons: {:?}", dialog.buttons);
    assert_eq!(dialog.dropdowns.len(), 1);
    // The refresh updated the dropdown's current value...
    assert_eq!(dialog.dropdowns[0].value, "offensive");
    // ...which %id% substitution resolves in sibling commands.
    assert_eq!(
        dialog.command_with_placeholders("_stance %dDBStance%"),
        "_stance offensive"
    );
}

#[test]
fn container_feed_populates_registry_in_parallel() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    // Real look-in-container sequence: <container> then
    // <clearContainer> then header + item <inv> lines.
    let feed = [
        ParsedElement::Container {
            id: "77".to_string(),
            title: "Bandolier".to_string(),
            target: Some("#77".to_string()),
        },
        ParsedElement::ClearContainer {
            id: "77".to_string(),
        },
        ParsedElement::ContainerItem {
            container_id: "77".to_string(),
            content: r#"In the <a exist="77" noun="bandolier">bandolier</a>:"#.to_string(),
        },
        ParsedElement::ContainerItem {
            container_id: "77".to_string(),
            content: r#" a <a exist="101" noun="crystal">quartz crystal</a>"#.to_string(),
        },
        ParsedElement::ContainerItem {
            container_id: "77".to_string(),
            content: r#" a <a exist="102" noun="sword">short sword</a>"#.to_string(),
        },
    ];
    for element in &feed {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    // Registry holds the two items, header skipped, ids intact.
    let items = game_state.objects.items_in("77");
    assert_eq!(items.len(), 2, "header excluded, both items kept");
    assert_eq!(items[0].id, "101");
    assert_eq!(items[0].name, "quartz crystal");
    assert_eq!(items[1].id, "102");
}

#[test]
fn stow_container_feed_targets_object_in_registry() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let feed = [
        ParsedElement::Container {
            id: "stow".to_string(),
            title: "My Shroud".to_string(),
            target: Some("#691".to_string()),
        },
        ParsedElement::ClearContainer {
            id: "stow".to_string(),
        },
        ParsedElement::ContainerItem {
            container_id: "stow".to_string(),
            content: r#"In the <a exist="691" noun="shroud">shroud</a>:"#.to_string(),
        },
        ParsedElement::ContainerItem {
            container_id: "stow".to_string(),
            content: r#" a <a exist="742" noun="feather">disir feather</a>"#.to_string(),
        },
    ];
    for element in &feed {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    // Header (the shroud object) skipped via command_target, feather
    // kept; the command target is the object id, not "#stow".
    let items = game_state.objects.items_in("stow");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "742");
    assert_eq!(
        game_state
            .objects
            .container("stow")
            .unwrap()
            .command_target(),
        "691"
    );
}

// ===========================================
// Active effect expiry derivation
// ===========================================

#[test]
fn test_active_effect_derives_expires_at_from_game_time() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    game_state.game_time = 1_000_000;

    let element = ParsedElement::ActiveEffect {
        category: "Buffs".to_string(),
        id: "509".to_string(),
        value: 92,
        text: "Strength of the Bull".to_string(),
        time: "00:01:05".to_string(),
    };
    processor.process_element(
        &element,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    let store = game_state.effects.get("Buffs").expect("Buffs store");
    assert_eq!(store.effects.len(), 1);
    assert_eq!(store.effects[0].expires_at, Some(1_000_065));

    // Unparseable duration -> no expiry
    let element = ParsedElement::ActiveEffect {
        category: "Buffs".to_string(),
        id: "905".to_string(),
        value: 100,
        text: "Prestidigitation".to_string(),
        time: "Indefinite".to_string(),
    };
    processor.process_element(
        &element,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    let store = game_state.effects.get("Buffs").expect("Buffs store");
    let indef = store.effects.iter().find(|e| e.id == "905").unwrap();
    assert_eq!(indef.expires_at, None);
}

// ===========================================
// dashboard runtime auto-discovery of status indicators
// ===========================================

fn feed_indicator(
    processor: &mut MessageProcessor,
    game_state: &mut GameState,
    ui_state: &mut UiState,
    id: &str,
    active: bool,
) {
    let element = ParsedElement::StatusIndicator {
        id: id.to_string(),
        active,
    };
    processor.process_element(
        &element,
        game_state,
        ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
}

fn dashboard_ids(ui_state: &UiState) -> Vec<String> {
    match &ui_state.windows.get("dash").expect("dash window").content {
        crate::data::WindowContent::Dashboard { indicators } => {
            indicators.iter().map(|(id, _)| id.clone()).collect()
        }
        _ => panic!("dash is not a dashboard"),
    }
}

fn dash_ui() -> UiState {
    use crate::data::{
        geometry::{Col, Height, Row, Width},
        WidgetType, WindowContent, WindowPosition, WindowState,
    };
    let mut ui_state = UiState::default();
    let win = WindowState {
        name: "dash".to_string(),
        widget_type: WidgetType::Dashboard,
        content: WindowContent::Dashboard {
            indicators: Vec::new(),
        },
        position: WindowPosition {
            x: Col::new(0),
            y: Row::new(0),
            width: Width::new(20),
            height: Height::new(3),
        },
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("dash".to_string(), win);
    ui_state
}

#[test]
fn dashboard_auto_discovers_unclaimed_indicator() {
    // No template claims STANDING -> the game's indicator auto-adds a cell.
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "STANDING",
        true,
    );
    assert_eq!(dashboard_ids(&ui_state), vec!["STANDING"]);
}

/// CHARACTERIZATION: the ten indicator ids that `element.rs` actually writes
/// into `GameState.status`. Pins the parser->GameState boundary before the
/// general-map refactor. Note JOINED is absent from this list by design --
/// see `characterize_joined_indicator_is_dropped` below.
#[test]
fn characterize_statusinfo_fields_written_by_parser() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    for id in [
        "STUNNED",
        "BLEEDING",
        "HIDDEN",
        "INVISIBLE",
        "WEBBED",
        "DEAD",
        "STANDING",
        "KNEELING",
        "SITTING",
        "PRONE",
    ] {
        feed_indicator(&mut processor, &mut game_state, &mut ui_state, id, true);
    }

    let s = &game_state.status;
    assert!(s.stunned() && s.bleeding() && s.hidden() && s.invisible() && s.webbed());
    assert!(s.dead() && s.standing() && s.kneeling() && s.sitting() && s.prone());

    // Clearing round-trips too.
    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "STUNNED",
        false,
    );
    assert!(!game_state.status.stunned());
}

/// FIXED (was a defect): `element.rs` had no `"joined"` match arm, so
/// `IconJOINED` was swallowed by the `_ => {}` fallthrough and `status.joined`
/// stayed false forever -- despite being serialized to remote clients. Group
/// membership is a prerequisite for the multi-account roster.
#[test]
fn joined_indicator_reaches_gamestate() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "JOINED",
        true,
    );
    assert!(game_state.status.joined());

    // And it still reaches the dashboard widget path.
    assert!(dashboard_ids(&ui_state).contains(&"JOINED".to_string()));

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "JOINED",
        false,
    );
    assert!(!game_state.status.joined());
}

/// FIXED (was a defect): POISONED/DISEASED are real game indicators and
/// shipped presets, but had no `StatusInfo` field, so they reached the
/// dashboard while nothing in core could read them. The general map stores
/// every id the game sends.
#[test]
fn unmapped_indicators_now_reach_gamestate() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    for id in ["POISONED", "DISEASED"] {
        feed_indicator(&mut processor, &mut game_state, &mut ui_state, id, true);
    }

    // Widget path still sees them...
    let ids = dashboard_ids(&ui_state);
    assert!(ids.contains(&"POISONED".to_string()));
    assert!(ids.contains(&"DISEASED".to_string()));

    // ...and now so does GameState.
    assert!(game_state.status.poisoned());
    assert!(game_state.status.diseased());
}

/// Stance reaches GameState via the bare `<progressBar>` route.
///
/// Previously the stance bar rendered only into a window widget, so a client
/// with no stance window -- headless, remote, or the multi-account display --
/// had no stance value at all.
#[test]
fn stance_progress_bar_reaches_gamestate() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    let element = ParsedElement::ProgressBar {
        id: "pbarStance".to_string(),
        value: 80,
        max: 100,
        text: "defensive (80%)".to_string(),
    };
    processor.process_element(
        &element,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    assert_eq!(game_state.stance.value, 80);
    assert_eq!(game_state.stance.text, "defensive");
}

/// Stance also reaches GameState via the `<dialogData>` route, which is how
/// the server usually frames it. Both paths must populate state or stance
/// would be present only on some connections.
#[test]
fn stance_dialog_progress_bar_reaches_gamestate() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    let element = ParsedElement::DialogProgressBars {
        id: "stance".to_string(),
        clear: false,
        progress_bars: vec![crate::parser::DialogProgressBarSpec {
            id: "pbarStance".to_string(),
            value: 0,
            text: "offensive (0%)".to_string(),
            layout: None,
        }],
    };
    processor.process_element(
        &element,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    assert_eq!(game_state.stance.value, 0);
    assert_eq!(game_state.stance.text, "offensive");
}

/// An id with no typed accessor and no preset must still round-trip, so a new
/// game indicator needs no code change to become readable by conditions.
#[test]
fn novel_indicator_ids_round_trip_without_code_changes() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "SOMETHINGNEW",
        true,
    );
    assert!(game_state.status.get("somethingnew"));
    assert!(game_state.status.is_known("SOMETHINGNEW"));
}

/// CHARACTERIZATION: the parser strips the `Icon` prefix but preserves case,
/// and the GameState write is case-insensitive. Both castings must land.
/// This behavior must SURVIVE the refactor unchanged.
#[test]
fn characterize_indicator_write_is_case_insensitive() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "stunned",
        true,
    );
    assert!(game_state.status.stunned(), "lowercase id must write");

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "STUNNED",
        false,
    );
    assert!(!game_state.status.stunned(), "uppercase id must write too");
}

#[test]
fn dashboard_suppresses_claimed_indicator() {
    // A combined POSTURE indicator claims STANDING/KNEELING/PRONE/SITTING;
    // the raw ids must NOT auto-add as orphan cells (no double-up).
    let mut processor = create_test_processor();
    processor.set_claimed_indicator_ids(
        ["STANDING", "KNEELING", "PRONE", "SITTING"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    for id in ["STANDING", "KNEELING", "PRONE", "SITTING"] {
        feed_indicator(&mut processor, &mut game_state, &mut ui_state, id, true);
    }
    assert!(
        dashboard_ids(&ui_state).is_empty(),
        "claimed posture ids must not auto-add: {:?}",
        dashboard_ids(&ui_state)
    );

    // An UNclaimed id still auto-discovers alongside the claimed ones.
    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "BLEEDING",
        true,
    );
    assert_eq!(dashboard_ids(&ui_state), vec!["BLEEDING"]);
}

#[test]
fn dashboard_claim_is_case_insensitive() {
    // Claimed set is uppercase; the game may send any casing.
    let mut processor = create_test_processor();
    processor.set_claimed_indicator_ids(["STANDING".to_string()].into_iter().collect());
    let mut game_state = GameState::new();
    let mut ui_state = dash_ui();

    feed_indicator(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "standing",
        true,
    );
    assert!(dashboard_ids(&ui_state).is_empty());
}

// ===========================================
// seen-streams registry (custom-window authoring source)
// ===========================================

#[test]
fn test_note_seen_stream_records_ids_sorted() {
    let mut processor = create_test_processor();
    processor.note_seen_stream("familiar", None);
    processor.note_seen_stream("bounty", None);
    let seen = processor.seen_streams();
    assert_eq!(
        seen,
        vec![("bounty".to_string(), None), ("familiar".to_string(), None),]
    );
}

#[test]
fn test_note_seen_stream_skips_main_and_blank() {
    let mut processor = create_test_processor();
    processor.note_seen_stream("main", None);
    processor.note_seen_stream("MAIN", None);
    processor.note_seen_stream("   ", None);
    assert!(processor.seen_streams().is_empty());
}

#[test]
fn test_note_seen_stream_label_fills_without_clobber() {
    let mut processor = create_test_processor();
    // First seen with no label, then a title arrives -> label fills in.
    processor.note_seen_stream("room", None);
    processor.note_seen_stream("room", Some("Room"));
    assert_eq!(
        processor.seen_streams(),
        vec![("room".to_string(), Some("Room".to_string()))]
    );
    // A later push without a title must not wipe the known label.
    processor.note_seen_stream("room", None);
    assert_eq!(
        processor.seen_streams(),
        vec![("room".to_string(), Some("Room".to_string()))]
    );
}

// ===========================================
// map_stream_to_window tests - core game streams
// ===========================================

#[test]
fn test_map_stream_main() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("main"), "main");
}

#[test]
fn test_map_stream_room() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("room"), "room");
}

#[test]
fn test_map_stream_inventory() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("inv"), "inventory");
}

// ===========================================
// Redirect match tests
// ===========================================

#[test]
fn test_redirect_fast_parse_ignores_empty_literals() {
    let mut config = Config::default();
    config.highlight_settings.redirect_enabled = true;
    config
        .highlights
        .insert("empty_redirect".to_string(), make_redirect_pattern("||"));

    let processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let result = processor.check_redirect_match("anything");
    assert!(result.is_none());
}

#[test]
fn test_redirect_fast_parse_longest_match_wins() {
    let mut config = Config::default();
    config.highlight_settings.redirect_enabled = true;
    config.highlights.insert(
        "longest_redirect".to_string(),
        make_redirect_pattern("a|ab|abc"),
    );

    let processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let result = processor.check_redirect_match("zz abc zz");
    assert!(matches!(
        result,
        Some((_window, crate::config::RedirectMode::RedirectOnly, 3))
    ));
}

// ===========================================
// Emoji shortcode toggle tests
// ===========================================

#[test]
fn test_emoji_shortcodes_applied_when_enabled() {
    let mut processor = create_test_processor();
    assert!(processor.config.ui.emoji_shortcodes, "default must be on");
    processor.current_segments = vec![TextSegment::plain("You :grin: at 12:30:45.")];
    processor.apply_emoji_shortcodes();
    assert_eq!(
        processor.current_segments[0].text,
        "You \u{1F601} at 12:30:45."
    );
}

#[test]
fn test_emoji_shortcodes_toggle_off_passthrough() {
    let mut config = Config::default();
    config.ui.emoji_shortcodes = false;
    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    processor.current_segments = vec![TextSegment::plain("You :grin: at :notarealcode:.")];
    processor.apply_emoji_shortcodes();
    assert_eq!(
        processor.current_segments[0].text,
        "You :grin: at :notarealcode:."
    );
}

// ===========================================
// Widget data generation tests
// ===========================================

fn process_component(
    processor: &mut MessageProcessor,
    game_state: &mut GameState,
    id: &str,
    value: &str,
) {
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_dirty = false;
    processor.handle_component(
        id,
        value,
        game_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_dirty,
    );
}

#[test]
fn test_room_component_generations_bump_on_change_only() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    let players_v1 = "Also here: <a exist='-123' noun='Bob'>Bob</a>";
    process_component(&mut processor, &mut game_state, "room players", players_v1);
    assert_eq!(game_state.room_players_generation, 1);
    assert_eq!(game_state.room_players.len(), 1);

    // Identical re-send: previous_room_components dedup must skip processing
    process_component(&mut processor, &mut game_state, "room players", players_v1);
    assert_eq!(
        game_state.room_players_generation, 1,
        "unchanged component must not bump"
    );

    // Real change bumps again
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: <a exist='-456' noun='Alice'>Alice</a>",
    );
    assert_eq!(game_state.room_players_generation, 2);
}

/// Brief mode: a plain living player, no status.
#[test]
fn test_room_players_plain_living() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: <a exist=\"-1\" noun=\"Bob\">Bob</a>",
    );
    assert_eq!(game_state.room_players.len(), 1);
    let p = &game_state.room_players[0];
    assert_eq!(p.name, "Bob");
    assert!(!p.dead);
    assert_eq!(p.primary_status, None);
    assert_eq!(p.secondary_status, None);
}

/// Brief mode: parenthetical status "(sitting)".
#[test]
fn test_room_players_brief_paren_status() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: <a exist=\"-1\" noun=\"Kerl\">Kerl</a> (sitting), \
             <a exist=\"-2\" noun=\"Zoleta\">Zoleta</a>",
    );
    assert_eq!(game_state.room_players.len(), 2);
    assert_eq!(
        game_state.room_players[0].secondary_status.as_deref(),
        Some("sitting")
    );
    // The following player must not absorb Kerl's status.
    assert_eq!(game_state.room_players[1].secondary_status, None);
    assert_eq!(game_state.room_players[1].name, "Zoleta");
}

/// Verbose mode: "who is lying down" maps to the canonical "prone".
#[test]
fn test_room_players_verbose_lying_down_maps_to_prone() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: <a exist=\"-1\" noun=\"Ruuzakilr\">Ruuzakilr</a> who is lying down, \
             <a exist=\"-2\" noun=\"Torgaben\">Torgaben</a>",
    );
    assert_eq!(game_state.room_players.len(), 2);
    assert_eq!(
        game_state.room_players[0].secondary_status.as_deref(),
        Some("prone")
    );
    assert!(!game_state.room_players[0].dead);
    assert_eq!(game_state.room_players[1].secondary_status, None);
}

/// Dead marker: "the body of" sets the dead flag; name stays clean.
#[test]
fn test_room_players_dead_body_of() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: <a exist=\"-1\" noun=\"Braendon\">Braendon</a>, \
             the body of <a exist=\"-2\" noun=\"Briar\">Briar</a> (prone)",
    );
    assert_eq!(game_state.room_players.len(), 2);
    assert!(!game_state.room_players[0].dead);
    let briar = &game_state.room_players[1];
    assert_eq!(briar.name, "Briar");
    assert!(briar.dead, "\"the body of\" must set dead");
    assert_eq!(briar.secondary_status.as_deref(), Some("prone"));
}

/// The stacked case straight from live logs: dead AND verbose posture.
#[test]
fn test_room_players_dead_plus_verbose() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: the body of <a exist=\"-1\" noun=\"Lanthilas\">Lanthilas</a> who is lying down",
    );
    assert_eq!(game_state.room_players.len(), 1);
    let p = &game_state.room_players[0];
    assert_eq!(p.name, "Lanthilas");
    assert!(p.dead);
    assert_eq!(p.secondary_status.as_deref(), Some("prone"));
}

/// Title prefixes ("Arena Occultist", "Lord") must NOT become a status
/// (regression guard for the "-> [Occ]" / "-> [Lord]" bug).
#[test]
fn test_room_players_title_prefix_is_not_a_status() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: Arena Occultist <a exist=\"-1\" noun=\"Sugiin\">Sugiin</a>, \
             Lord <a exist=\"-2\" noun=\"Kazner\">Kazner</a> who is lying down",
    );
    assert_eq!(game_state.room_players.len(), 2);
    let sugiin = &game_state.room_players[0];
    assert_eq!(sugiin.name, "Sugiin");
    assert!(!sugiin.dead);
    assert_eq!(sugiin.primary_status, None, "title must not be a status");
    assert_eq!(sugiin.secondary_status, None);
    // Title + verbose posture together: title dropped, posture kept.
    let kazner = &game_state.room_players[1];
    assert_eq!(kazner.primary_status, None, "title must not be a status");
    assert_eq!(kazner.secondary_status.as_deref(), Some("prone"));
}

/// Legacy article-gated prepended status ("a stunned <link>") still works.
#[test]
fn test_room_players_article_gated_prepended_status() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: a stunned <a exist=\"-1\" noun=\"Bob\">Bob</a>",
    );
    assert_eq!(game_state.room_players.len(), 1);
    assert_eq!(
        game_state.room_players[0].primary_status.as_deref(),
        Some("stunned")
    );
}

/// Unknown verbose posture passes through raw (nothing silently dropped).
#[test]
fn test_room_players_unknown_verbose_passes_through() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    process_component(
        &mut processor,
        &mut game_state,
        "room players",
        "Also here: <a exist=\"-1\" noun=\"Bob\">Bob</a> who is floating serenely",
    );
    assert_eq!(game_state.room_players.len(), 1);
    assert_eq!(
        game_state.room_players[0].secondary_status.as_deref(),
        Some("floating serenely")
    );
}

#[test]
fn test_room_objs_bumps_creature_and_object_generations() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    let objs = "You also see <a exist='789' noun='rock'>a rock</a>.";
    process_component(&mut processor, &mut game_state, "room objs", objs);
    assert_eq!(game_state.room_creatures_generation, 1);
    assert_eq!(game_state.room_objects_generation, 1);

    // Identical re-send: no bumps
    process_component(&mut processor, &mut game_state, "room objs", objs);
    assert_eq!(game_state.room_creatures_generation, 1);
    assert_eq!(game_state.room_objects_generation, 1);
}

// Flatten a styled line's segments to plaintext for readable assertions.
fn line_text(line: &crate::data::widget::StyledLine) -> String {
    line.segments.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn test_room_desc_mirrors_styled_lines_with_links_to_game_state() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    // Fresh state: no prose, generation 0.
    assert!(game_state.room_description.is_empty());
    assert_eq!(game_state.room_description_generation, 0);

    // A desc component with a scenery link must be mirrored WITH its
    // styling and clickable link intact — not flattened to plaintext.
    let desc = "A mossy <a exist='1' noun='fountain'>marble fountain</a> stands here.";
    process_component(&mut processor, &mut game_state, "room desc", desc);
    assert_eq!(game_state.room_description.len(), 1);
    assert_eq!(
        line_text(&game_state.room_description[0]),
        "A mossy marble fountain stands here.",
        "prose text must be preserved"
    );
    // The clickable scenery link must survive — this is the whole point
    // of carrying styled lines rather than plaintext.
    let has_fountain_link = game_state.room_description[0]
        .segments
        .iter()
        .any(|s| s.link_data.as_ref().is_some_and(|l| l.exist_id == "1"));
    assert!(
        has_fountain_link,
        "the scenery link (exist_id=1) must survive to the phone: {:?}",
        game_state.room_description[0].segments
    );
    assert_eq!(game_state.room_description_generation, 1);
}

#[test]
fn test_room_desc_bumps_only_on_change() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    let desc = "A quiet clearing.";
    process_component(&mut processor, &mut game_state, "room desc", desc);
    assert_eq!(game_state.room_description_generation, 1);

    // Identical re-send: the component dedup skips it — no bump.
    process_component(&mut processor, &mut game_state, "room desc", desc);
    assert_eq!(
        game_state.room_description_generation, 1,
        "unchanged room desc must not bump the generation"
    );

    // A real change bumps and replaces.
    process_component(&mut processor, &mut game_state, "room desc", "A dark cave.");
    assert_eq!(game_state.room_description_generation, 2);
    assert_eq!(game_state.room_description.len(), 1);
    assert_eq!(line_text(&game_state.room_description[0]), "A dark cave.");
}

#[test]
fn test_room_desc_clears_on_empty_component() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room desc",
        "A grand hall.",
    );
    assert_eq!(game_state.room_description.len(), 1);
    assert_eq!(game_state.room_description_generation, 1);

    // An empty desc component clears the mirrored prose and bumps.
    process_component(&mut processor, &mut game_state, "room desc", "");
    assert!(
        game_state.room_description.is_empty(),
        "empty room desc component must clear the mirrored prose"
    );
    assert_eq!(game_state.room_description_generation, 2);
}

#[test]
fn test_spellbook_mirrors_to_game_state_on_prompt_flush() {
    // Drive a real Spells stream + prompt the way the game sends it: each
    // <pushStream id="Spells"> line accumulates, and the prompt flushes the
    // buffer. The spellbook must then be mirrored onto GameState as styled
    // lines (keeping spell coloring/links) for remote clients.
    let mut parser = crate::parser::XmlParser::new();
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();

    let lines = [
        "<pushStream id='Spells'/>Elemental Defense III (503)   00:14:59",
        "<popStream/>",
        "<pushStream id='Spells'/>Mana Leech (516)   00:29:42",
        "<popStream/>",
        "<prompt time='1700000000'>&gt;</prompt>",
    ];
    for line in &lines {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut game_state,
                &mut ui_state,
                &mut std::collections::HashMap::new(),
                &mut None,
                &mut false,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    assert_eq!(
        game_state.spellbook.len(),
        2,
        "both spell lines must mirror onto GameState, got {:?}",
        game_state.spellbook
    );
    assert!(
        line_text(&game_state.spellbook[0]).contains("Elemental Defense III"),
        "first spell line wrong: {:?}",
        game_state.spellbook
    );
    // The mirrored line must be a real styled line (segments present),
    // not a flattened string — that's what carries spell color/links.
    assert!(
        !game_state.spellbook[0].segments.is_empty(),
        "spellbook line must carry styled segments"
    );
    assert!(
        game_state.spellbook_generation >= 1,
        "spellbook generation must bump on first population"
    );
}

#[test]
fn inventory_pop_commits_styled_complete_replacements() {
    fn feed_snapshot(
        parser: &mut crate::parser::XmlParser,
        processor: &mut MessageProcessor,
        game_state: &mut GameState,
        ui_state: &mut UiState,
        xml: &str,
    ) {
        for raw_line in xml.lines() {
            for element in parser.parse_line(raw_line) {
                processor.process_element(
                    &element,
                    game_state,
                    ui_state,
                    &mut std::collections::HashMap::new(),
                    &mut None,
                    &mut false,
                    &mut None,
                    &mut None,
                    &mut None,
                    None,
                );
            }
            processor.flush_current_stream_with_tts(ui_state, None);
        }
    }

    let mut parser = crate::parser::XmlParser::new();
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    let mut inventory_window = crate::data::window::WindowState::new_text("inventory", 100);
    inventory_window.widget_type = crate::data::window::WidgetType::Inventory;
    inventory_window.content =
        WindowContent::Inventory(crate::data::TextContent::new("inventory".to_string(), 100));
    ui_state.set_window("inventory".to_string(), inventory_window);

    feed_snapshot(
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "<pushStream id='inv'/>Your worn items are:\n  a <a exist='535703780' noun='backpack'>patchwork dwarf skin backpack</a>\n<popStream/>",
    );
    assert_eq!(game_state.inventory.len(), 2);
    assert_eq!(game_state.inventory[0].stream, "inv");
    assert_eq!(line_text(&game_state.inventory[0]), "Your worn items are:");
    let link = game_state.inventory[1]
        .segments
        .iter()
        .find_map(|segment| segment.link_data.as_ref())
        .expect("inventory item link retained");
    assert_eq!(link.exist_id, "535703780");
    assert_eq!(link.noun, "backpack");
    assert_eq!(link.text, "patchwork dwarf skin backpack");
    let WindowContent::Inventory(content) = &ui_state.windows["inventory"].content else {
        panic!("test inventory window changed content type");
    };
    assert_eq!(content.lines.len(), 2, "open inventory window is populated");

    feed_snapshot(
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "<pushStream id='inv'/>You are holding:\n  a <a exist='42' noun='orb'>crystal orb</a>\n<popStream/>",
    );
    assert_eq!(game_state.inventory.len(), 2, "the new snapshot replaces");
    assert_eq!(line_text(&game_state.inventory[0]), "You are holding:");
    assert_eq!(
        game_state.inventory[1].segments.iter().find_map(|segment| {
            segment
                .link_data
                .as_ref()
                .map(|link| link.exist_id.as_str())
        }),
        Some("42")
    );

    feed_snapshot(
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        "<pushStream id='inv'/><popStream/>",
    );
    assert!(
        game_state.inventory.is_empty(),
        "an empty complete snapshot clears stale inventory"
    );
    assert!(
        game_state.inventory_received,
        "an empty complete snapshot is distinguishable from no snapshot"
    );
    let WindowContent::Inventory(content) = &ui_state.windows["inventory"].content else {
        panic!("test inventory window changed content type");
    };
    assert!(
        content.lines.is_empty(),
        "an empty complete snapshot clears an open inventory window"
    );
}

// ===========================================
// <crtrStatus> tests (fixtures captured from a live GST session,
// via lich-5 PR #1425's spec suite)
// ===========================================

#[test]
fn test_crtr_status_parsed_from_room_objs() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room objs",
        r#"  You notice<crtrStatus exist="607736" hostile="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b>."#,
    );

    assert_eq!(game_state.room_creatures.len(), 1);
    let nymph = &game_state.room_creatures[0];
    assert_eq!(nymph.name, "sea nymph");
    let flags = nymph.flags.as_ref().expect("crtrStatus flags attached");
    assert!(flags.hostile);
    assert!(!flags.dead);
    assert!(flags.statuses.is_empty());
}

#[test]
fn test_crtr_status_two_creatures_one_line() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room objs",
        r#"  You notice<crtrStatus exist="607744" hostile="1"/><b> <pushBold/>a <a exist="607744" noun="worm">carrion worm</a><popBold/></b> and<crtrStatus exist="607736" hostile="1" stunned="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b> (stunned)."#,
    );

    assert_eq!(game_state.room_creatures.len(), 2);
    let worm = &game_state.room_creatures[0];
    let nymph = &game_state.room_creatures[1];
    assert_eq!(worm.id, "#607744");
    assert!(worm.flags.as_ref().unwrap().statuses.is_empty());
    assert_eq!(
        nymph.flags.as_ref().unwrap().statuses,
        vec!["stunned".to_string()]
    );
    assert_eq!(nymph.display_statuses(), vec!["stunned".to_string()]);
}

#[test]
fn test_crtr_status_full_snapshot_reconciles() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room objs",
        r#"  You notice<crtrStatus exist="607736" hostile="1" stunned="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b> (stunned)."#,
    );
    // Dead snapshot: stunned absent means inactive, not unknown
    process_component(
        &mut processor,
        &mut game_state,
        "room objs",
        r#"  You notice<crtrStatus exist="607736" hostile="1" dead="1" prone="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b> (dead)."#,
    );

    let nymph = &game_state.room_creatures[0];
    let flags = nymph.flags.as_ref().unwrap();
    assert!(flags.dead);
    assert_eq!(flags.statuses, vec!["prone".to_string()]);
    assert!(nymph.is_dead());
    // Display leads with dead, then transient statuses
    assert_eq!(
        nymph.display_statuses(),
        vec!["dead".to_string(), "prone".to_string()]
    );
}

#[test]
fn test_crtr_status_flag_zero_means_inactive() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room objs",
        r#"  You notice<crtrStatus exist="999001" hostile="0"/><b> <pushBold/>a <a exist="999001" noun="rabbit">field rabbit</a><popBold/></b>."#,
    );

    let rabbit = &game_state.room_creatures[0];
    let flags = rabbit
        .flags
        .as_ref()
        .expect("flags attached even when all inactive");
    assert!(!flags.hostile);
}

#[test]
fn test_crtr_status_maps_immobile_to_immobilized() {
    let flags = crate::core::state::CreatureFlags::from_xml_attrs([
        ("immobile", "1"),
        ("AscensionBoss", "1"),
        ("challenging", "0"),
    ]);
    assert_eq!(flags.statuses, vec!["immobilized".to_string()]);
    assert!(flags.ascension_boss);
    assert!(flags.is_boss());
    assert!(!flags.challenging);
}

#[test]
fn test_crtr_status_standalone_element_updates_existing_creature() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room objs",
        r#"  You notice<crtrStatus exist="607736" hostile="1"/><b> <pushBold/>a <a exist="607736" noun="nymph">sea nymph</a><popBold/></b>."#,
    );
    let generation = game_state.room_creatures_generation;

    // Standalone update (outside a component): patches the known creature
    let element = ParsedElement::CreatureStatus {
        id: "607736".to_string(),
        attrs: vec![
            ("hostile".to_string(), "1".to_string()),
            ("stunned".to_string(), "1".to_string()),
        ],
    };
    processor.process_element(
        &element,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    let nymph = &game_state.room_creatures[0];
    assert_eq!(
        nymph.flags.as_ref().unwrap().statuses,
        vec!["stunned".to_string()]
    );
    assert_eq!(game_state.room_creatures_generation, generation + 1);

    // Unknown id: no-op, no generation bump
    let element = ParsedElement::CreatureStatus {
        id: "111111".to_string(),
        attrs: vec![("stunned".to_string(), "1".to_string())],
    };
    processor.process_element(
        &element,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    assert_eq!(game_state.room_creatures_generation, generation + 1);
}

// ===========================================
// Stream subscriber index tests
// ===========================================

#[test]
fn test_stream_subscribers_case_insensitive_and_trimmed() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    let mut ws = crate::data::window::WindowState::new_text("thoughts", 100);
    if let WindowContent::Text(ref mut c) = ws.content {
        // Config may carry stray whitespace and any casing
        c.streams = vec![" Thoughts ".to_string()];
    }
    ui_state.windows.insert("thoughts".to_string(), ws);
    processor.update_text_stream_subscribers(&ui_state);

    // Lookups match regardless of case/whitespace
    assert_eq!(processor.get_stream_subscribers("thoughts").len(), 1);
    assert_eq!(processor.get_stream_subscribers("THOUGHTS").len(), 1);
    assert_eq!(processor.get_stream_subscribers(" Thoughts ").len(), 1);
    assert!(processor.stream_has_subscribers("tHoUgHtS"));
    assert!(!processor.stream_has_subscribers("speech"));
}

#[test]
fn test_stream_subscribers_dedupe_window() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    let mut ws = crate::data::window::WindowState::new_text("combat", 100);
    if let WindowContent::Text(ref mut c) = ws.content {
        // Duplicate stream entries must not double-deliver lines
        c.streams = vec!["combat".to_string(), "Combat".to_string()];
    }
    ui_state.windows.insert("combat".to_string(), ws);
    processor.update_text_stream_subscribers(&ui_state);

    assert_eq!(processor.get_stream_subscribers("combat").len(), 1);
}

#[test]
fn test_event_pattern_feeds_stun_countdown() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();
    let mut ws = crate::data::window::WindowState::new_text("stuntime", 10);
    ws.content = WindowContent::Countdown(crate::data::CountdownData {
        end_time: 0,
        label: "Stun".to_string(),
        countdown_id: "stuntime".to_string(),
        color: None,
        show_when_zero: false,
        count_past_zero: false,
    });
    ui_state.windows.insert("stuntime".to_string(), ws);

    let end_time_of = |ui_state: &UiState| match &ui_state
        .windows
        .get("stuntime")
        .expect("stuntime window")
        .content
    {
        WindowContent::Countdown(cd) => cd.end_time,
        _ => panic!("not a countdown"),
    };

    // Set: end_time lands ~duration seconds from now (server offset 0).
    let set = ParsedElement::Event {
        event_type: "stun".to_string(),
        action: crate::config::EventAction::Set,
        duration: 15,
    };
    processor.process_element(
        &set,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    let end = end_time_of(&ui_state);
    let now = chrono::Utc::now().timestamp();
    assert!(
        (now + 13..=now + 17).contains(&end),
        "end_time {} not ~now+15",
        end
    );

    // Clear: recovery patterns zero the countdown.
    let clear = ParsedElement::Event {
        event_type: "stun".to_string(),
        action: crate::config::EventAction::Clear,
        duration: 0,
    };
    processor.process_element(
        &clear,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    assert_eq!(end_time_of(&ui_state), 0);
}

#[test]
fn test_pulse_feeds_state_and_countdown() {
    // <pulse min max mana>: bumps the generation counter, records the
    // next-pulse window in the server clock domain, flags whether the NEXT
    // pulse restores mana, and arms the "pulse" countdown at now+min.
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();
    let mut ws = crate::data::window::WindowState::new_text("pulse", 10);
    ws.content = WindowContent::Countdown(crate::data::CountdownData {
        end_time: 0,
        label: "Pulse".to_string(),
        countdown_id: "pulse".to_string(),
        color: None,
        show_when_zero: true,
        count_past_zero: false,
    });
    ui_state.windows.insert("pulse".to_string(), ws);

    let pulse = ParsedElement::Pulse {
        mana: true,
        min: 46,
        max: 75,
    };
    processor.process_element(
        &pulse,
        &mut game_state,
        &mut ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    assert_eq!(game_state.pulse_count, 1);
    assert!(game_state.next_pulse_mana);
    let now = chrono::Utc::now().timestamp();
    let earliest = game_state.pulse_next_earliest.expect("earliest set");
    let latest = game_state.pulse_next_latest.expect("latest set");
    assert!(
        (now + 44..=now + 48).contains(&earliest),
        "earliest {} not ~now+46",
        earliest
    );
    assert_eq!(latest - earliest, 75 - 46, "window spans max-min seconds");

    let end = match &ui_state.windows.get("pulse").unwrap().content {
        WindowContent::Countdown(cd) => cd.end_time,
        _ => panic!("not a countdown"),
    };
    assert_eq!(end, earliest, "countdown armed at the earliest next pulse");
}

#[test]
fn test_vellum_timer_feeds_countdown_by_id() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();
    let mut ws = crate::data::window::WindowState::new_text("cataclysm", 10);
    ws.content = WindowContent::Countdown(crate::data::CountdownData {
        end_time: 0,
        label: "Cataclysm".to_string(),
        countdown_id: "dark-cataclyst".to_string(),
        color: None,
        show_when_zero: false,
        count_past_zero: false,
    });
    ui_state.windows.insert("cataclysm".to_string(), ws);

    let end_time_of = |ui_state: &UiState| match &ui_state
        .windows
        .get("cataclysm")
        .expect("countdown window")
        .content
    {
        WindowContent::Countdown(cd) => cd.end_time,
        _ => panic!("not a countdown"),
    };

    let mut process = |processor: &mut MessageProcessor,
                       game_state: &mut GameState,
                       ui_state: &mut UiState,
                       value: i64| {
        let element = ParsedElement::VellumTimer {
            id: "dark-cataclyst".to_string(),
            value,
        };
        processor.process_element(
            &element,
            game_state,
            ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    };

    process(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        1_764_904_999,
    );
    assert_eq!(end_time_of(&ui_state), 1_764_904_999);

    // 0 clears; negative values clamp to cleared instead of going weird.
    process(&mut processor, &mut game_state, &mut ui_state, 0);
    assert_eq!(end_time_of(&ui_state), 0);
    process(&mut processor, &mut game_state, &mut ui_state, -5);
    assert_eq!(end_time_of(&ui_state), 0);
}

fn make_text_window(name: &str, streams: &[&str]) -> crate::data::window::WindowState {
    let mut ws = crate::data::window::WindowState::new_text(name, 100);
    if let WindowContent::Text(ref mut c) = ws.content {
        c.streams = streams.iter().map(|s| s.to_string()).collect();
    }
    ws
}

fn push_test_segment(processor: &mut MessageProcessor, text: &str) {
    processor.current_segments.push(TextSegment {
        text: text.to_string(),
        fg: None,
        bg: None,
        bold: false,
        mono: false,
        span_type: SpanType::Normal,
        link_data: None,
        custom_emoji: None,
        inline_image: None,
    });
}

/// Room components are a latest-state projection, but the remote Story is a
/// chronological feed. A real movement must copy one styled room block into
/// Story, while a LOOK/same-room component refresh must only update Room.
#[test]
fn remote_story_gets_one_styled_block_per_room_identity() {
    fn replay_room_xml(
        xml: &str,
        parser: &mut crate::parser::XmlParser,
        processor: &mut MessageProcessor,
        game_state: &mut GameState,
        ui_state: &mut UiState,
        room_components: &mut std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        current_room_component: &mut Option<String>,
        room_window_dirty: &mut bool,
        nav_room_id: &mut Option<String>,
        lich_room_id: &mut Option<String>,
        room_subtitle: &mut Option<String>,
    ) {
        for element in parser.parse_line(xml) {
            processor.process_element(
                &element,
                game_state,
                ui_state,
                room_components,
                current_room_component,
                room_window_dirty,
                nav_room_id,
                lich_room_id,
                room_subtitle,
                None,
            );
        }
    }

    let mut processor = create_test_processor();
    let (sink, handles, _events) = crate::core::remote::RemoteSink::new(100);
    processor.remote = Some(sink);

    let mut parser = crate::parser::XmlParser::new();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;
    let mut nav_room_id = None;
    let mut lich_room_id = None;
    let mut room_subtitle = None;

    replay_room_xml(
        r#"<nav rm='101'/><streamWindow id='room' title='Room' subtitle=' - First Room'/><pushStream id='room'/><compDef id='room desc'>A <a exist='11' noun='wall'>granite wall</a> surrounds you.</compDef><compDef id='room objs'><pushBold/>a troll<popBold/></compDef><compDef id='room players'>Also here: Great Noble <a exist='-12' noun='Hero'>Hero</a></compDef><compDef id='room exits'>Obvious exits: <d>north</d></compDef><popStream id='room'/>"#,
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut nav_room_id,
        &mut lich_room_id,
        &mut room_subtitle,
    );
    replay_room_xml(
        "<prompt time='1'></prompt>",
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut nav_room_id,
        &mut lich_room_id,
        &mut room_subtitle,
    );

    let first = handles.buffer.lock().unwrap().tail("main", 100);
    let first_text: Vec<String> = first
        .iter()
        .map(|line| {
            line.line
                .segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        first_text,
        vec![
            "[First Room - u101]",
            "A granite wall surrounds you.",
            "a troll",
            "Also here: Great Noble Hero",
            "Obvious exits: north",
        ],
        "header and canonical room components must reach Story in order"
    );
    assert!(
        first[1]
            .line
            .segments
            .iter()
            .any(|segment| segment.link_data.is_some()),
        "room-description links must retain their structured styling"
    );
    assert!(
        first[2]
            .line
            .segments
            .iter()
            .any(|segment| segment.span_type == SpanType::Monsterbold),
        "monsterbold styling must survive the Story copy"
    );
    assert!(
        first[3]
            .line
            .segments
            .iter()
            .any(|segment| segment.span_type == SpanType::PlayerTitle
                && segment.text == "Great Noble "
                && segment.link_data.is_none()),
        "component-only Story must style the title without making it clickable"
    );

    // Same room identity: a fresh LOOK may replace components, and Room must
    // reflect it, but Story must not receive another movement block.
    replay_room_xml(
        r#"<nav rm='101'/><streamWindow id='room' title='Room' subtitle=' - First Room'/><pushStream id='room'/><compDef id='room desc'>A refreshed description.</compDef><compDef id='room exits'>Obvious exits: <d>north</d></compDef><popStream id='room'/>"#,
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut nav_room_id,
        &mut lich_room_id,
        &mut room_subtitle,
    );
    replay_room_xml(
        "<prompt time='2'></prompt>",
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut nav_room_id,
        &mut lich_room_id,
        &mut room_subtitle,
    );
    assert_eq!(
        handles.buffer.lock().unwrap().tail("main", 100).len(),
        first.len(),
        "same-room component refresh must not duplicate Story"
    );
    let latest_room_desc: String = game_state.room_description[0]
        .segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    assert_eq!(latest_room_desc, "A refreshed description.");

    // Seed the prior room's Lich id. A changed navigation UID must invalidate
    // it so neither Story nor the remote Room header pairs the new UID with
    // the preceding room number.
    lich_room_id = Some("999".to_string());
    replay_room_xml(
        r#"<nav rm='102'/><streamWindow id='room' title='Room' subtitle=' - Second Room'/><pushStream id='room'/><compDef id='room desc'>The second description.</compDef><compDef id='room objs'></compDef><compDef id='room players'></compDef><compDef id='room exits'>Obvious exits: <d>south</d></compDef><popStream id='room'/>"#,
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut nav_room_id,
        &mut lich_room_id,
        &mut room_subtitle,
    );
    replay_room_xml(
        "<prompt time='3'></prompt>",
        &mut parser,
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut nav_room_id,
        &mut lich_room_id,
        &mut room_subtitle,
    );
    let all = handles.buffer.lock().unwrap().tail("main", 100);
    let all_text: Vec<String> = all
        .iter()
        .map(|line| {
            line.line
                .segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        &all_text[first.len()..],
        [
            "[Second Room - u102]",
            "The second description.",
            "Obvious exits: south",
        ],
        "next movement must append exactly one new canonical room block"
    );
    assert_eq!(
        all_text
            .iter()
            .filter(|line| line.as_str() == "The second description.")
            .count(),
        1,
        "the movement description must appear exactly once"
    );
    assert!(
        lich_room_id.is_none(),
        "new navigation UID must clear the preceding room's Lich id"
    );
}

/// Lich can send two projections for one movement: first a room-component
/// snapshot whose description is the sentinel used when its room window is
/// disabled, then the authoritative decorated room block on `main`.  The
/// remote Story must show the authoritative block once, and the Room snapshot
/// must retain its real prose. This mirrors the ordering captured from a live
/// session in Town Square Central (Lich room 228, navigation UID 7120).
#[test]
fn lich_native_room_block_replaces_disabled_component_without_story_duplicate() {
    fn replay_network_line(
        xml: &str,
        parser: &mut crate::parser::XmlParser,
        processor: &mut MessageProcessor,
        game_state: &mut GameState,
        ui_state: &mut UiState,
        room_components: &mut std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
        current_room_component: &mut Option<String>,
        room_window_dirty: &mut bool,
        nav_room_id: &mut Option<String>,
        lich_room_id: &mut Option<String>,
        room_subtitle: &mut Option<String>,
    ) {
        for element in parser.parse_line(xml) {
            processor.process_element(
                &element,
                game_state,
                ui_state,
                room_components,
                current_room_component,
                room_window_dirty,
                nav_room_id,
                lich_room_id,
                room_subtitle,
                None,
            );
        }
        // AppCore flushes after every network line; keeping that boundary in
        // the fixture is what reproduces the duplicate seen in the browser.
        processor.flush_current_stream_with_tts(ui_state, None);
    }

    let mut processor = create_test_processor();
    let (sink, handles, _events) = crate::core::remote::RemoteSink::new(100);
    processor.remote = Some(sink);

    let mut parser = crate::parser::XmlParser::new();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;
    let mut nav_room_id = None;
    let mut lich_room_id = None;
    let mut room_subtitle = None;

    let mut replay = |line: &str| {
        replay_network_line(
            line,
            &mut parser,
            &mut processor,
            &mut game_state,
            &mut ui_state,
            &mut room_components,
            &mut current_room_component,
            &mut room_window_dirty,
            &mut nav_room_id,
            &mut lich_room_id,
            &mut room_subtitle,
        );
    };

    replay("<nav rm='7120'/>");
    replay("<streamWindow id='room' title='Room' subtitle=' - Town Square Central'/>");
    replay("<pushStream id='room'/>");
    replay("<compDef id='room desc'>[Room window disabled at this location.]</compDef>");
    replay("<compDef id='room exits'>Obvious paths: <d>northeast</d>, <d>east</d>, <d>southeast</d>, <d>southwest</d>, <d>west</d>, <d>northwest</d></compDef>");
    replay("<popStream id='room'/>");
    replay("<style id='roomName'/>[Town Square Central - 228]<style id=''/> (u7120)");
    replay("This is the heart of the main square of Wehnimer's Landing.");
    replay("The oak is tall and straight, and it is apparent that the roots run deep.");
    replay("Obvious paths: <d>northeast</d>, <d>east</d>, <d>southeast</d>, <d>southwest</d>, <d>west</d>, <d>northwest</d>");
    replay("<prompt time='1'>&gt;</prompt>");

    drop(replay);
    let story = handles.buffer.lock().unwrap().tail("main", 100);
    let story_text: Vec<String> = story
        .iter()
        .map(|line| {
            line.line
                .segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect()
        })
        .collect();

    assert_eq!(
        story_text
            .iter()
            .filter(|line| line.contains("Town Square Central"))
            .count(),
        1,
        "one movement must create one Story room header: {story_text:?}"
    );
    assert!(
        story_text
            .iter()
            .any(|line| line == "[Town Square Central - 228] (u7120)"),
        "the canonical header must distinguish Lich room id 228 from navigation uid 7120: {story_text:?}"
    );
    assert_eq!(
        story_text
            .iter()
            .filter(|line| line.as_str()
                == "This is the heart of the main square of Wehnimer's Landing.")
            .count(),
        1,
        "authoritative room prose must appear once in Story: {story_text:?}"
    );
    assert!(
        !story_text
            .iter()
            .any(|line| line.contains("Room window disabled at this location")),
        "the disabled-room sentinel is transport metadata, not Story content: {story_text:?}"
    );

    let room_description: Vec<String> = game_state
        .room_description
        .iter()
        .map(|line| {
            line.segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        room_description,
        [
            "This is the heart of the main square of Wehnimer's Landing.",
            "The oak is tall and straight, and it is apparent that the roots run deep.",
        ],
        "Room must retain the complete authoritative native description"
    );
    assert_eq!(lich_room_id.as_deref(), Some("228"));
    assert_eq!(nav_room_id.as_deref(), Some("7120"));
}

#[test]
fn canonical_room_identity_keeps_lich_id_and_navigation_uid_distinct() {
    assert_eq!(
        canonical_room_id(Some("228"), Some("7120")).as_deref(),
        Some("228 (u7120)")
    );
    assert_eq!(
        canonical_room_id(Some("228"), Some("228")).as_deref(),
        Some("228")
    );
    assert_eq!(
        canonical_room_id(None, Some("7120")).as_deref(),
        Some("u7120")
    );
}

/// Rysk's mobile prompt spam (beta 43): on a headless host whose layout has
/// no thoughts/arrivals windows, those streams fall back into the LOCAL main
/// window and arm the prompt separator — but remote clients route the same
/// lines to their own feeds, so the phone's story collected a lone prompt
/// line for every background thought/arrival/death. The remote story feed
/// must gate its prompt separators on its OWN activity.
#[test]
fn remote_story_feed_skips_prompts_armed_only_by_fallback_text() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    processor.update_text_stream_subscribers(&ui_state);

    let (sink, handles, _events) = crate::core::remote::RemoteSink::new(100);
    processor.remote = Some(sink);

    fn run_prompt(
        processor: &mut MessageProcessor,
        game_state: &mut GameState,
        ui_state: &mut UiState,
    ) {
        processor.process_element(
            &ParsedElement::Prompt {
                time: "0".to_string(),
                text: "s>".to_string(),
            },
            game_state,
            ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }
    let remote_prompts = |handles: &crate::core::remote::RemoteServerHandles| -> usize {
        handles
            .buffer
            .lock()
            .unwrap()
            .tail("main", 100)
            .iter()
            .filter(|l| {
                l.line
                    .segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<String>()
                    .trim()
                    == "s>"
            })
            .count()
    };

    // Baseline: the first prompt is a change ("" -> "s>") and shows
    // everywhere, remote story included.
    run_prompt(&mut processor, &mut game_state, &mut ui_state);
    assert_eq!(remote_prompts(&handles), 1);

    // A background thought: no thoughts window, so it falls back into the
    // LOCAL main window, while the remote client shows it in its own feed.
    processor.current_stream = "thoughts".to_string();
    push_test_segment(&mut processor, "Someone thinks aloud.");
    processor.flush_current_stream_with_tts(&mut ui_state, None);
    processor.current_stream = "main".to_string();
    assert_eq!(
        handles.buffer.lock().unwrap().tail("thoughts", 100).len(),
        1,
        "the thought reaches the phone's own feed"
    );

    let local_lines_before = text_line_count(&ui_state, "main");
    run_prompt(&mut processor, &mut game_state, &mut ui_state);

    // Locally the separator still renders — the thought text landed in the
    // main window (Wrayth parity).
    assert_eq!(
        text_line_count(&ui_state, "main"),
        local_lines_before + 1,
        "local main window keeps its prompt separator"
    );
    // Remotely the story feed saw nothing this chunk: no stranded prompt.
    assert_eq!(
        remote_prompts(&handles),
        1,
        "an idle chunk must not push a lone prompt into the phone's story"
    );

    // Genuine story text re-arms the remote separator.
    push_test_segment(&mut processor, "You wave.");
    processor.flush_current_stream_with_tts(&mut ui_state, None);
    run_prompt(&mut processor, &mut game_state, &mut ui_state);
    assert_eq!(
        remote_prompts(&handles),
        2,
        "story text brings the separator back"
    );
}

fn text_line_count(ui_state: &UiState, window: &str) -> usize {
    match &ui_state.windows.get(window).expect("window exists").content {
        WindowContent::Text(c) => c.lines.len(),
        _ => panic!("not a text window"),
    }
}

fn text_lines(ui_state: &UiState, window: &str) -> Vec<crate::data::StyledLine> {
    match &ui_state.windows.get(window).expect("window exists").content {
        WindowContent::Text(c) => c.lines.iter().cloned().collect(),
        _ => panic!("not a text window"),
    }
}

// ===========================================
// Inline images (<vellumImg>)
// ===========================================

/// Drive one element through the processor with throwaway room/nav state.
fn process_one(
    processor: &mut MessageProcessor,
    element: &crate::parser::ParsedElement,
    ui_state: &mut UiState,
) {
    let mut game_state = crate::core::state::GameState::new();
    processor.process_element(
        element,
        &mut game_state,
        ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
}

/// A `<vellumImg>` tag must reach a text window as a segment carrying the
/// image reference, with a readable fallback in `text` for frontends that
/// cannot draw it.
#[test]
fn vellum_img_becomes_an_image_segment() {
    use crate::data::FloatAlign;
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    processor.update_text_stream_subscribers(&ui_state);
    processor.current_stream = "main".to_string();

    process_one(
        &mut processor,
        &crate::parser::ParsedElement::VellumImage {
            src: "banner".to_string(),
            rows: 4.0,
            align: FloatAlign::Right,
        },
        &mut ui_state,
    );
    processor.flush_current_stream(&mut ui_state);

    let lines = text_lines(&ui_state, "main");
    assert_eq!(lines.len(), 1);
    let image = lines[0]
        .segments
        .iter()
        .find_map(|s| s.inline_image.as_ref())
        .expect("segment carries the image");
    assert_eq!(image.name, "banner");
    assert_eq!(image.rows, 4.0);
    assert_eq!(image.align, FloatAlign::Right);
    // The fallback text keeps the line readable without art.
    assert!(
        lines[0].segments.iter().any(|s| s.text.contains("banner")),
        "expected a readable fallback label"
    );
}

/// Highlights rebuild segments from a flat char-style map that cannot carry
/// an image reference. A matching highlight must therefore leave an
/// image-bearing line alone rather than silently dropping the picture.
#[test]
fn highlights_do_not_strip_inline_images() {
    use crate::data::FloatAlign;
    let mut config = Config::default();
    let mut pattern = make_redirect_pattern("img");
    pattern.redirect_to = None;
    pattern.redirect_mode = crate::config::RedirectMode::RedirectOnly;
    pattern.fg = Some("#ff0000".to_string());
    config.highlights.insert("img_hl".to_string(), pattern);

    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    processor.update_text_stream_subscribers(&ui_state);
    processor.current_stream = "main".to_string();

    process_one(
        &mut processor,
        &crate::parser::ParsedElement::VellumImage {
            src: "banner".to_string(),
            rows: 2.0,
            align: FloatAlign::Left,
        },
        &mut ui_state,
    );
    processor.flush_current_stream(&mut ui_state);

    let lines = text_lines(&ui_state, "main");
    assert!(
        lines[0].segments.iter().any(|s| s.inline_image.is_some()),
        "highlight pass must not drop the image reference"
    );
}

/// Emoji resolution splits and rewrites segment text. An image segment's
/// text is a label, not prose, so it must pass through untouched or the
/// painter's reserved run would desync from the drawn image.
#[test]
fn emoji_pass_leaves_inline_image_segments_alone() {
    use crate::data::{FloatAlign, InlineImage};
    let mut segments = vec![TextSegment {
        text: "[img:grin]".to_string(),
        inline_image: Some(InlineImage {
            name: "grin".to_string(),
            rows: 3.0,
            align: FloatAlign::Left,
        }),
        ..Default::default()
    }];
    crate::core::emoji::apply_to_segments(&mut segments);
    assert_eq!(segments.len(), 1, "must not split");
    assert_eq!(segments[0].text, "[img:grin]", "must not rewrite the label");
    assert!(segments[0].inline_image.is_some());
}

#[test]
fn test_multi_subscriber_delivery() {
    // Two windows subscribe the same stream: both must receive the line
    // (the last subscriber receives it by move, the rest by clone)
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state.windows.insert(
        "alpha".to_string(),
        make_text_window("alpha", &["thoughts"]),
    );
    ui_state
        .windows
        .insert("beta".to_string(), make_text_window("beta", &["thoughts"]));
    processor.update_text_stream_subscribers(&ui_state);

    processor.current_stream = "thoughts".to_string();
    push_test_segment(&mut processor, "You hear the faint thoughts of someone.");
    processor.flush_current_stream(&mut ui_state);

    assert_eq!(text_line_count(&ui_state, "alpha"), 1);
    assert_eq!(text_line_count(&ui_state, "beta"), 1);
}

#[test]
fn test_redirect_copy_delivers_to_target_and_original() {
    let mut config = Config::default();
    config.highlight_settings.redirect_enabled = true;
    let mut r = make_redirect_pattern("hear");
    r.redirect_to = Some("alerts".to_string());
    r.redirect_mode = crate::config::RedirectMode::RedirectCopy;
    config.highlights.insert("copy_redirect".to_string(), r);

    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    ui_state.windows.insert(
        "alerts".to_string(),
        make_text_window("alerts", &["alerts"]),
    );
    processor.update_text_stream_subscribers(&ui_state);

    processor.current_stream = "main".to_string();
    push_test_segment(&mut processor, "You hear a noise.");
    processor.flush_current_stream(&mut ui_state);

    // RedirectCopy must deliver to the redirect target AND the original
    assert_eq!(text_line_count(&ui_state, "alerts"), 1);
    assert_eq!(text_line_count(&ui_state, "main"), 1);
}

#[test]
fn test_redirect_to_special_stream_restores_current_stream() {
    // A redirect whose target hits an early-return path (room/inv/
    // percWindow) must still restore current_stream, or the override
    // leaks into every following line of the chunk
    let mut config = Config::default();
    config.highlight_settings.redirect_enabled = true;
    let mut r = make_redirect_pattern("hear");
    r.redirect_to = Some("room".to_string());
    r.redirect_mode = crate::config::RedirectMode::RedirectOnly;
    config.highlights.insert("room_redirect".to_string(), r);

    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    processor.update_text_stream_subscribers(&ui_state);

    processor.current_stream = "main".to_string();
    push_test_segment(&mut processor, "You hear a noise.");
    processor.flush_current_stream(&mut ui_state);
    assert_eq!(processor.current_stream, "main");

    // The next (non-matching) line must land in main, not the target
    push_test_segment(&mut processor, "A rat scurries past.");
    processor.flush_current_stream(&mut ui_state);
    assert_eq!(text_line_count(&ui_state, "main"), 1);
}

fn make_hand_window(name: &str) -> crate::data::window::WindowState {
    let mut ws = crate::data::window::WindowState::new_text(name, 10);
    ws.widget_type = crate::data::window::WidgetType::Hand;
    ws.content = WindowContent::Hand {
        item: None,
        link: None,
    };
    ws
}

fn process_hand_element(
    processor: &mut MessageProcessor,
    game_state: &mut crate::core::state::GameState,
    ui_state: &mut UiState,
    element: &ParsedElement,
) {
    processor.process_element(
        element,
        game_state,
        ui_state,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
}

#[test]
fn test_bare_hand_refresh_keeps_link_for_unchanged_item() {
    let mut processor = MessageProcessor::new(Config::default(), SavedDialogPositions::default());
    let mut ui_state = UiState::new();
    let mut game_state = crate::core::state::GameState::new();
    ui_state
        .windows
        .insert("right".to_string(), make_hand_window("right"));
    ui_state.rebuild_widget_index();

    let link = crate::data::LinkData {
        exist_id: "123".to_string(),
        noun: "shard".to_string(),
        text: "jagged nephrite shard".to_string(),
        coord: None,
    };
    process_hand_element(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &ParsedElement::RightHand {
            item: "jagged nephrite shard".to_string(),
            link: Some(link),
        },
    );

    // A refresh repeating the same item without exist/noun must keep
    // the live link.
    process_hand_element(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &ParsedElement::RightHand {
            item: "jagged nephrite shard".to_string(),
            link: None,
        },
    );
    match &ui_state.windows.get("right").unwrap().content {
        WindowContent::Hand { item, link } => {
            assert_eq!(item.as_deref(), Some("jagged nephrite shard"));
            assert_eq!(
                link.as_ref().map(|l| l.exist_id.as_str()),
                Some("123"),
                "bare refresh must not clobber the link"
            );
        }
        _ => panic!("not a hand window"),
    }

    // A different item without a link must clear the stale link.
    process_hand_element(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &ParsedElement::RightHand {
            item: "a wooden club".to_string(),
            link: None,
        },
    );
    match &ui_state.windows.get("right").unwrap().content {
        WindowContent::Hand { link, .. } => {
            assert!(link.is_none(), "stale link must not follow a new item");
        }
        _ => panic!("not a hand window"),
    }

    // Emptying the hand clears both.
    process_hand_element(
        &mut processor,
        &mut game_state,
        &mut ui_state,
        &ParsedElement::RightHand {
            item: String::new(),
            link: None,
        },
    );
    match &ui_state.windows.get("right").unwrap().content {
        WindowContent::Hand { item, link } => {
            assert!(item.is_none());
            assert!(link.is_none());
        }
        _ => panic!("not a hand window"),
    }
}

#[test]
fn test_redirect_longest_match_wins_across_patterns() {
    let mut config = Config::default();
    config.highlight_settings.redirect_enabled = true;
    let mut short = make_redirect_pattern("hits");
    short.redirect_to = Some("short_win".to_string());
    config.highlights.insert("short".to_string(), short);
    let mut long = make_redirect_pattern("hits you");
    long.redirect_to = Some("long_win".to_string());
    config.highlights.insert("long".to_string(), long);

    let processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let result = processor.check_redirect_match("The troll hits you hard!");
    let (window, _, len) = result.expect("should match");
    assert_eq!(window, "long_win");
    assert_eq!(len, 8);
}

#[test]
fn test_redirect_regex_beats_shorter_literal() {
    let mut config = Config::default();
    config.highlight_settings.redirect_enabled = true;
    let mut lit = make_redirect_pattern("troll");
    lit.redirect_to = Some("lit_win".to_string());
    config.highlights.insert("lit".to_string(), lit);
    let mut rx = make_redirect_pattern(r"troll \w+ you");
    rx.fast_parse = false;
    rx.redirect_to = Some("rx_win".to_string());
    rx.compiled_regex = regex::Regex::new(r"troll \w+ you").ok();
    config.highlights.insert("rx".to_string(), rx);

    let processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let result = processor.check_redirect_match("The troll hits you hard!");
    let (window, _, len) = result.expect("should match");
    assert_eq!(window, "rx_win");
    assert_eq!(len, "troll hits you".len());
}

#[test]
fn test_map_stream_thoughts() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("thoughts"), "thoughts");
}

#[test]
fn test_map_stream_speech() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("speech"), "speech");
}

// ===========================================
// map_stream_to_window tests - communication streams
// ===========================================

#[test]
fn test_map_stream_announcements() {
    let processor = create_test_processor();
    assert_eq!(
        processor.map_stream_to_window("announcements"),
        "announcements"
    );
}

#[test]
fn test_map_stream_logons() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("logons"), "logons");
}

#[test]
fn test_map_stream_death() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("death"), "death");
}

#[test]
fn test_map_stream_loot() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("loot"), "loot");
}

// ===========================================
// map_stream_to_window tests - misc streams
// ===========================================

#[test]
fn test_map_stream_spells() {
    let processor = create_test_processor();
    // Note: case-sensitive - "Spells" not "spells"
    assert_eq!(processor.map_stream_to_window("Spells"), "spells");
}

#[test]
fn test_map_stream_familiar() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("familiar"), "familiar");
}

#[test]
fn test_map_stream_ambients() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("ambients"), "ambients");
}

#[test]
fn test_map_stream_bounty() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("bounty"), "bounty");
}

// ===========================================
// map_stream_to_window tests - unknown streams default to main
// ===========================================

#[test]
fn test_map_stream_unknown_defaults_to_main() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("unknown_stream"), "main");
}

#[test]
fn test_map_stream_empty_defaults_to_main() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window(""), "main");
}

#[test]
fn test_map_stream_random_text_defaults_to_main() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("xyz123"), "main");
}

#[test]
fn test_map_stream_case_sensitive_spells() {
    let processor = create_test_processor();
    // "spells" (lowercase) should default to main, not "spells" window
    // Only "Spells" (capital S) maps to spells window
    assert_eq!(processor.map_stream_to_window("spells"), "main");
}

// ===========================================
// MessageProcessor construction tests
// ===========================================

#[test]
fn test_new_processor_has_main_stream() {
    let processor = create_test_processor();
    assert_eq!(processor.current_stream, "main");
}

#[test]
fn test_new_processor_segments_empty() {
    let processor = create_test_processor();
    assert!(processor.current_segments.is_empty());
}

#[test]
fn test_new_processor_buffers_empty() {
    let processor = create_test_processor();
    assert!(processor.inventory_buffer.is_empty());
}

#[test]
fn test_new_processor_not_discarding() {
    let processor = create_test_processor();
    assert!(!processor.discard_current_stream);
}

#[test]
fn test_new_processor_server_time_offset_zero() {
    let processor = create_test_processor();
    assert_eq!(processor.server_time_offset, 0);
}

// ===========================================
// clear_inventory_cache tests
// ===========================================

#[test]
fn test_clear_inventory_cache() {
    let mut processor = create_test_processor();
    // Add some fake previous inventory
    processor.previous_inventory = vec![vec![TextSegment {
        text: "test item".to_string(),
        fg: None,
        bg: None,
        bold: false,
        mono: false,
        span_type: SpanType::Normal,
        link_data: None,
        custom_emoji: None,
        inline_image: None,
    }]];
    assert!(!processor.previous_inventory.is_empty());

    // Clear cache
    processor.clear_inventory_cache();
    assert!(processor.previous_inventory.is_empty());
}

// ===========================================
// Reserve stream buffering tests
// ===========================================

fn make_reserve_window(name: &str) -> crate::data::window::WindowState {
    let mut ws = crate::data::window::WindowState::new_text(name, 100);
    ws.widget_type = crate::data::window::WidgetType::Reserve;
    let mut content = crate::data::TextContent::new(name.to_string(), 100);
    content.streams = vec!["reserve".to_string()];
    ws.content = WindowContent::Reserve(content);
    ws
}

fn reserve_line_count(ui_state: &UiState, window: &str) -> usize {
    match &ui_state.windows.get(window).expect("window exists").content {
        WindowContent::Reserve(c) => c.lines.len(),
        _ => panic!("not a reserve window"),
    }
}

#[test]
fn test_map_stream_reserve() {
    let processor = create_test_processor();
    assert_eq!(processor.map_stream_to_window("reserve"), "reserve");
}

#[test]
fn test_reserve_stream_buffers_then_flushes_snapshot() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("reserve".to_string(), make_reserve_window("reserve"));
    processor.update_text_stream_subscribers(&ui_state);

    // Line arrives while in the reserve stream: buffered, not delivered
    processor.current_stream = "reserve".to_string();
    push_test_segment(&mut processor, "a sprig of wild lilac");
    processor.flush_current_stream(&mut ui_state);
    assert_eq!(reserve_line_count(&ui_state, "reserve"), 0);
    assert_eq!(processor.reserve_buffer.len(), 1);

    // Stream pop flushes the snapshot into the window
    processor.flush_reserve_buffer(&mut ui_state);
    assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);
    assert!(processor.reserve_buffer.is_empty());
}

#[test]
fn test_reserve_identical_snapshot_skips_update_changed_replaces() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("reserve".to_string(), make_reserve_window("reserve"));
    processor.update_text_stream_subscribers(&ui_state);

    // First snapshot
    processor.current_stream = "reserve".to_string();
    push_test_segment(&mut processor, "a sprig of wild lilac");
    processor.flush_current_stream(&mut ui_state);
    processor.flush_reserve_buffer(&mut ui_state);
    assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);

    // Identical snapshot: dedupe leaves existing content untouched
    processor.current_stream = "reserve".to_string();
    push_test_segment(&mut processor, "a sprig of wild lilac");
    processor.flush_current_stream(&mut ui_state);
    processor.flush_reserve_buffer(&mut ui_state);
    assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);

    // Changed snapshot: content is replaced, not appended
    processor.current_stream = "reserve".to_string();
    push_test_segment(&mut processor, "a blue potion");
    processor.flush_current_stream(&mut ui_state);
    processor.flush_reserve_buffer(&mut ui_state);
    assert_eq!(reserve_line_count(&ui_state, "reserve"), 1);
}

#[test]
fn test_reserve_stream_discarded_without_window() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    processor.update_text_stream_subscribers(&ui_state);

    processor.current_stream = "reserve".to_string();
    push_test_segment(&mut processor, "a sprig of wild lilac");
    processor.flush_current_stream(&mut ui_state);

    // No reserve window: content dropped, nothing buffered, nothing in main
    assert!(processor.reserve_buffer.is_empty());
    assert_eq!(text_line_count(&ui_state, "main"), 0);
}

// ===========================================
// Stream mapping completeness tests
// ===========================================

#[test]
fn test_all_known_streams_mapped_correctly() {
    let processor = create_test_processor();

    // Test all documented stream -> window mappings
    let expected_mappings = [
        ("main", "main"),
        ("room", "room"),
        ("inv", "inventory"),
        ("thoughts", "thoughts"),
        ("speech", "speech"),
        ("announcements", "announcements"),
        ("loot", "loot"),
        ("death", "death"),
        ("logons", "logons"),
        ("familiar", "familiar"),
        ("ambients", "ambients"),
        ("bounty", "bounty"),
        ("Spells", "spells"),
    ];

    for (stream, expected_window) in expected_mappings {
        assert_eq!(
            processor.map_stream_to_window(stream),
            expected_window,
            "Stream '{}' should map to window '{}'",
            stream,
            expected_window
        );
    }
}

#[test]
fn bank_open_dialog_block_sets_declared_size_and_grids_all_controls() {
    // End-to-end over the WIRE-VERBATIM bank block (GST log
    // 2026-02-08): parser -> processor -> anchor grid. Pins the
    // element ordering the declared-size capture relies on (the inner
    // dialogData controls create the store slot BEFORE the trailing
    // WindowHints element) and that links + spinboxes land in the
    // grid at compass-resolved rows instead of overlapping at the top
    // (live-test screenshot, 2026-08-06).
    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    let elements = parser.parse_line(
            "<openDialog type='dynamic' id='bank' title='Bank' location='right' save='t' height='130' width='0'><dialogData id=\"bank\"><label id=\"balance\" value=\"Balance: 5041236\" align=\"n\" top=\"0\" left=\"0\" height=\"20\" width=\"190\"/><cmdButton id=\"depositBtn\" value=\"Deposit\" echo=\"deposit %depositSB%\" cmd=\"deposit %depositSB%\" align=\"e\" top=\"-25\" left=\"0\" height=\"25\" width=\"80\"/><cmdButton id=\"withdrawBtn\" value=\"Withdraw\" echo=\"withdraw %withdrawSB%\" cmd=\"withdraw %withdrawSB%\" align=\"e\" top=\"5\" left=\"0\" height=\"25\" width=\"80\"/><upDownEditBox id=\"depositSB\" min=\"0\" max=\"0\" value=\"0\" align=\"w\" top=\"-25\" left=\"0\" height=\"25\" width=\"100\"/><upDownEditBox id=\"withdrawSB\" min=\"0\" max=\"5041236\" value=\"5000\" align=\"w\" top=\"5\" left=\"0\" height=\"25\" width=\"100\"/></dialogData></openDialog>",
        );
    assert!(!elements.is_empty());

    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    for element in &elements {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let dialog = ui_state
        .dialog_store
        .get("bank")
        .expect("bank slot ingested");
    assert_eq!(
        dialog.declared_size,
        Some((0.0, 130.0)),
        "openDialog height reached the store (ordering held)"
    );
    assert_eq!(dialog.spinboxes.len(), 2);

    let (controls, _) = dialog.positioned_controls().expect("positioned");
    use crate::data::ui_state::PositionedControlKind as K;
    let spin_rows: Vec<f32> = controls
        .iter()
        .filter(|c| matches!(c.kind, K::SpinBox(_)))
        .map(|c| c.rect.1)
        .collect();
    assert_eq!(spin_rows.len(), 2, "spinboxes are IN the grid");
    // Compass rows: deposit above withdraw, both center-referenced.
    let (lo, hi) = (
        spin_rows.iter().cloned().fold(f32::MAX, f32::min),
        spin_rows.iter().cloned().fold(f32::MIN, f32::max),
    );
    assert_eq!(lo, 65.0 - 12.5 - 25.0);
    assert_eq!(hi, 65.0 - 12.5 + 5.0);
}

#[test]
fn bug_dialog_box_popup_populates_despite_name_keyed_dialog_data() {
    // Wire-verbatim (GSIV log 2025-12-31): openDialog id='bugDialogBox'
    // whose INNER dialogData keys on name= — the embedded extractors
    // saw nothing, so the popup arrived empty and never usable
    // (live-test report: "we don't get any bug dialog windows").
    let mut parser =
        crate::parser::XmlParser::with_presets(Vec::new(), std::collections::HashMap::new());
    let elements = parser.parse_line(
            "<openDialog type='dynamic' id='bugDialogBox' title='Submit Bug Report' location='detach' height='190' width='500' save='false' noResize='' noDock=''><dialogData name='bugDialogBox'><label id='categoryLabel' value='Category' justify='4' top='5' left='25' width='65'/><dropDownBox id='category' value='ROOM' content_text='CHARACTER,ROOM,TYPO' content_value='CHARACTER,ROOM,TYPO' top='5' left='95' width='330'/><cmdButton id='submitBtn' value='Submit' cmd='bugreport submit' top='160' left='120' width='120'/></dialogData></openDialog>",
        );

    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut ui_state = UiState::default();
    for element in &elements {
        processor.process_element(
            element,
            &mut game_state,
            &mut ui_state,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let dialog = ui_state
        .dialog_store
        .get("bugDialogBox")
        .expect("store slot under the id");
    assert!(!dialog.dropdowns.is_empty(), "category dropdown ingested");
    assert!(!dialog.buttons.is_empty(), "submit button ingested");
    assert!(
        ui_state
            .active_dialog
            .as_ref()
            .is_some_and(|d| d.id == "bugDialogBox"),
        "the popup actually shows"
    );
}

/// A `<vellumImg>` inside a room component must reach the room window's
/// styled lines, so a script can float art into the room the same way it
/// can into the story window.
#[test]
fn vellum_img_inside_a_room_component_reaches_the_room() {
    use crate::data::FloatAlign;
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();

    process_component(
        &mut processor,
        &mut game_state,
        "room desc",
        "<vellumImg src='sunset' rows='4' align='right'/>A quiet clearing.",
    );

    let image = game_state
        .room_description
        .iter()
        .flat_map(|line| line.segments.iter())
        .find_map(|seg| seg.inline_image.as_ref())
        .expect("room description carries the image");
    assert_eq!(image.name, "sunset");
    assert_eq!(image.rows, 4.0);
    assert_eq!(image.align, FloatAlign::Right);

    // The prose alongside it survives.
    let text: String = game_state
        .room_description
        .iter()
        .flat_map(|line| line.segments.iter())
        .map(|seg| seg.text.as_str())
        .collect();
    assert!(text.contains("A quiet clearing."), "got {text:?}");
}

/// Reproduce the live report: one `<vellumImg>` plus prose in a room
/// component must yield ONE image and keep the text.
#[test]
fn room_component_image_appears_once_and_keeps_text() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_dirty = false;

    processor.handle_component(
        "room desc",
        "<vellumImg src='sunset' rows='4' align='left'/>Stretching like long fingers.",
        &mut game_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_dirty,
    );

    let buffer = room_components
        .get("room desc")
        .expect("component buffered");
    let images: usize = buffer
        .iter()
        .flat_map(|line| line.iter())
        .filter(|s| s.inline_image.is_some())
        .count();
    let text: String = buffer
        .iter()
        .flat_map(|line| line.iter())
        .filter(|s| s.inline_image.is_none())
        .map(|s| s.text.as_str())
        .collect();

    assert_eq!(images, 1, "exactly one image segment, got {images}");
    assert!(
        text.contains("Stretching like long fingers"),
        "prose must survive alongside the image, got {text:?}"
    );
    assert_eq!(buffer.len(), 1, "one line, got {}", buffer.len());
}

/// The game declares `<compDef id='sprite'>` on every room change but never
/// fills it (785k empty occurrences in the wire logs), so a script can put a
/// `<vellumImg>` there and have it land in the ROOM window's data — the room
/// stream's own art slot, no story-window detour.
#[test]
fn sprite_component_carries_an_inline_image() {
    use crate::data::FloatAlign;
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;

    processor.handle_component(
        "sprite",
        "<vellumImg src='sunset' rows='4' align='left'/>",
        &mut game_state,
        &mut room_components,
        &mut current,
        &mut dirty,
    );

    let image = room_components
        .get("sprite")
        .expect("sprite buffered")
        .iter()
        .flat_map(|line| line.iter())
        .find_map(|s| s.inline_image.as_ref())
        .expect("sprite carries the image");
    assert_eq!(image.name, "sunset");
    assert_eq!(image.align, FloatAlign::Left);
    assert!(dirty, "room window must repaint");
}

/// `<resource picture='N'/>` (STORY stream) tracks the game's room picture
/// when the user has art installed for that id; `picture='0'` (the
/// near-universal value) clears it, so art never carries between rooms.
#[test]
fn resource_picture_sets_and_clears_story_picture() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "32".into(),
        path: std::path::PathBuf::from("32.png"),
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    // One GameState across all three sends, so each case starts from the
    // PREVIOUS room's value — that is what makes the clearing meaningful.
    let mut game_state = crate::core::state::GameState::new();

    let mut send = |processor: &mut MessageProcessor,
                    gs: &mut crate::core::state::GameState,
                    id: u32,
                    ui: &mut UiState| {
        processor.process_element(
            &crate::parser::ParsedElement::RoomPicture { id },
            gs,
            ui,
            &mut std::collections::HashMap::new(),
            &mut None,
            &mut false,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    };

    // Installed art resolves.
    send(&mut processor, &mut game_state, 32, &mut ui_state);
    assert_eq!(game_state.story_picture.as_deref(), Some("32"));

    // 0 clears the art the previous room set.
    send(&mut processor, &mut game_state, 0, &mut ui_state);
    assert_eq!(game_state.story_picture, None);

    // An id with NO installed art also clears rather than leaving the last
    // picture up.
    send(&mut processor, &mut game_state, 32, &mut ui_state);
    assert_eq!(game_state.story_picture.as_deref(), Some("32"));
    send(&mut processor, &mut game_state, 999, &mut ui_state);
    assert_eq!(game_state.story_picture, None, "unknown id must clear");
}

/// The `<component id='sprite'>` form must work as well as `<compDef>`, so a
/// script can use whichever it already uses for other room content.
#[test]
fn sprite_accepts_the_component_form_too() {
    let mut processor = create_test_processor();
    let mut game_state = GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;

    processor.handle_component(
        "sprite",
        "<vellumImg src='sunset' rows='3'/>",
        &mut game_state,
        &mut room_components,
        &mut current,
        &mut dirty,
    );
    assert!(room_components
        .get("sprite")
        .expect("buffered")
        .iter()
        .flat_map(|l| l.iter())
        .any(|s| s.inline_image.is_some()));
}

// ===========================================
// Room art injection (room_images.toml)
// ===========================================

/// A processor with room art enabled and one image mapped to `rooms`.
fn processor_with_room_art(
    enabled: bool,
    image: &str,
    rooms: &[u64],
    install_art: bool,
) -> (MessageProcessor, std::sync::MutexGuard<'static, ()>) {
    // set_for_test writes process-wide state; hold the lock for the test.
    let guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::config::room_images::{RoomImageDef, RoomImageIndex, RoomImagesConfig};
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};

    // Injection deliberately skips art the user has not installed.
    let mut registry = CustomEmojiRegistry::default();
    if install_art {
        registry.insert_for_test(CustomEmoji {
            name: image.to_string(),
            path: std::path::PathBuf::from(format!("{image}.png")),
            format: EmojiFormat::Png,
        });
    }
    crate::core::inline_image::set_for_test(registry);

    let mut config = Config::default();
    config.room_images.enabled = enabled;
    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    processor.set_room_image_index(RoomImageIndex::build(&RoomImagesConfig {
        images: vec![RoomImageDef {
            name: image.to_string(),
            rooms: rooms.to_vec(),
            rows: None,
            align: None,
            variants: Vec::new(),
        }],
        names: Default::default(),
    }));
    (processor, guard)
}

/// Drive one room change the way the game does: `<nav rm=uid>` first, then
/// the empty `sprite` slot later in the same block.
fn enter_room(
    processor: &mut MessageProcessor,
    uid: &str,
    ui_state: &mut UiState,
) -> Option<String> {
    let mut game_state = crate::core::state::GameState::new();
    let mut components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;
    process_one(
        processor,
        &crate::parser::ParsedElement::RoomId {
            id: uid.to_string(),
        },
        ui_state,
    );
    processor.handle_component(
        "sprite",
        "",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    components
        .get("sprite")?
        .iter()
        .flat_map(|line| line.iter())
        .find_map(|s| s.inline_image.as_ref())
        .map(|i| i.name.clone())
}

/// The core promise: walk into a mapped room and the game's empty sprite
/// slot is filled with that room's art.
#[test]
fn mapped_room_gets_its_art_injected() {
    let (mut processor, _art_guard) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    assert_eq!(
        enter_room(&mut processor, "7118245", &mut ui).as_deref(),
        Some("pier")
    );
}

/// Every room sends an EMPTY sprite, so the unchanged-value dedup must not
/// short-circuit it — otherwise art appears only in the first mapped room of
/// a session and never again.
#[test]
fn art_injects_on_every_room_not_just_the_first() {
    let (mut processor, _art_guard) =
        processor_with_room_art(true, "pier", &[7118245, 7118250], true);
    let mut ui = UiState::new();
    assert_eq!(
        enter_room(&mut processor, "7118245", &mut ui).as_deref(),
        Some("pier")
    );
    assert_eq!(
        enter_room(&mut processor, "7118250", &mut ui).as_deref(),
        Some("pier"),
        "dedup must not suppress the repeated empty sprite"
    );
}

/// A variant whose condition matches but whose art file is NOT installed must
/// fall back to the entry's installed base art — not take the room's art down
/// with it. (A typo'd night variant otherwise blanked the room every night.)
#[test]
fn missing_variant_art_falls_back_to_the_base_image() {
    use crate::config::room_images::{
        RoomImageDef, RoomImageIndex, RoomImageVariant, RoomImagesConfig,
    };
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Only the BASE art is installed; the variant art is not.
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "pier".to_string(),
        path: std::path::PathBuf::from("pier.png"),
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let mut config = Config::default();
    config.room_images.enabled = true;
    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    processor.set_room_image_index(RoomImageIndex::build(&RoomImagesConfig {
        images: vec![RoomImageDef {
            name: "pier".to_string(),
            rooms: vec![7118245],
            rows: None,
            align: None,
            variants: vec![RoomImageVariant {
                name: "pier_nite".to_string(), // not installed (typo/deleted)
                // An empty All is vacuously true: the variant always matches.
                when: crate::config::Condition::All { conditions: vec![] },
            }],
        }],
        names: Default::default(),
    }));

    let mut ui = UiState::new();
    assert_eq!(
        enter_room(&mut processor, "7118245", &mut ui).as_deref(),
        Some("pier"),
        "missing variant art must fall back to the base image, not blank the room"
    );
}

/// An unmapped room leaves the slot empty — no art, no placeholder label.
#[test]
fn unmapped_room_gets_no_art() {
    let (mut processor, _art_guard) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    assert_eq!(enter_room(&mut processor, "9999999", &mut ui), None);
}

/// Walking from a mapped room to an unmapped one must CLEAR the art, not
/// leave the previous room's picture up.
#[test]
fn art_clears_when_leaving_a_mapped_room() {
    let (mut processor, _art_guard) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    assert!(enter_room(&mut processor, "7118245", &mut ui).is_some());
    assert_eq!(
        enter_room(&mut processor, "9999999", &mut ui),
        None,
        "previous room's art must not persist"
    );
}

/// The master toggle suppresses injection entirely.
#[test]
fn disabled_toggle_suppresses_injection() {
    let (mut processor, _art_guard) = processor_with_room_art(false, "pier", &[7118245], true);
    let mut ui = UiState::new();
    assert_eq!(enter_room(&mut processor, "7118245", &mut ui), None);
}

/// A mapping naming art the user has not installed leaves the slot empty
/// rather than emitting a broken `[img:]` label.
#[test]
fn missing_art_file_leaves_the_slot_empty() {
    let (mut processor, _art_guard) = processor_with_room_art(true, "missing", &[7118245], false);
    let mut ui = UiState::new();
    assert_eq!(enter_room(&mut processor, "7118245", &mut ui), None);
}

/// Script art always wins: a non-empty sprite is never overwritten.
#[test]
fn script_sprite_is_not_overwritten_by_room_art() {
    let (mut processor, _art_guard) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    process_one(
        &mut processor,
        &crate::parser::ParsedElement::RoomId {
            id: "7118245".into(),
        },
        &mut ui,
    );

    let mut game_state = crate::core::state::GameState::new();
    let mut components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;
    processor.handle_component(
        "sprite",
        "<vellumImg src='scripted' rows='2'/>",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    let image = components
        .get("sprite")
        .unwrap()
        .iter()
        .flat_map(|line| line.iter())
        .find_map(|s| s.inline_image.as_ref())
        .map(|i| i.name.clone());
    assert_eq!(
        image.as_deref(),
        Some("scripted"),
        "a script's own sprite must win over the room mapping"
    );
}

/// The phone/headless clients read `game_state.room_description`, not the
/// GUI's assembled room body — so room art must be mirrored there too, or
/// only the desktop GUI ever shows it.
#[test]
fn room_art_reaches_game_state_for_remote_clients() {
    let (mut processor, _art) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    let mut game_state = crate::core::state::GameState::new();

    process_one(
        &mut processor,
        &crate::parser::ParsedElement::RoomId {
            id: "7118245".into(),
        },
        &mut ui,
    );
    // The game sends sprite BEFORE room desc in the room block.
    let mut components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;
    processor.handle_component(
        "sprite",
        "",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    processor.handle_component(
        "room desc",
        "A quiet clearing.",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );

    let line = game_state
        .room_description
        .first()
        .expect("description mirrored");
    assert_eq!(
        line.segments
            .iter()
            .find_map(|s| s.inline_image.as_ref())
            .map(|i| i.name.as_str()),
        Some("pier"),
        "art must ride along with the mirrored description"
    );
    // Art LEADS, so the text wraps beside it rather than under it.
    assert!(
        line.segments[0].inline_image.is_some(),
        "art must be the first segment"
    );
    let text: String = line.segments.iter().map(|s| s.text.as_str()).collect();
    assert!(text.contains("A quiet clearing."), "prose kept: {text:?}");
}

/// A room with art but no description still shows the picture — the empty
/// `room desc` clear must not wipe it.
#[test]
fn room_art_survives_an_empty_description() {
    let (mut processor, _art) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    let mut game_state = crate::core::state::GameState::new();
    process_one(
        &mut processor,
        &crate::parser::ParsedElement::RoomId {
            id: "7118245".into(),
        },
        &mut ui,
    );
    let mut components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;
    processor.handle_component(
        "sprite",
        "",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    processor.handle_component(
        "room desc",
        "",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );

    assert!(
        game_state
            .room_description
            .first()
            .is_some_and(|l| l.segments.iter().any(|s| s.inline_image.is_some())),
        "art-only room must still mirror its picture"
    );
}

/// Leaving a mapped room clears the mirrored art, so the phone never shows
/// the previous room's picture.
#[test]
fn mirrored_art_clears_on_an_unmapped_room() {
    let (mut processor, _art) = processor_with_room_art(true, "pier", &[7118245], true);
    let mut ui = UiState::new();
    let mut game_state = crate::core::state::GameState::new();
    let mut components = std::collections::HashMap::new();
    let mut current = None;
    let mut dirty = false;

    process_one(
        &mut processor,
        &crate::parser::ParsedElement::RoomId {
            id: "7118245".into(),
        },
        &mut ui,
    );
    processor.handle_component(
        "sprite",
        "",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    processor.handle_component(
        "room desc",
        "First room.",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    assert!(game_state.room_description[0].segments[0]
        .inline_image
        .is_some());

    process_one(
        &mut processor,
        &crate::parser::ParsedElement::RoomId {
            id: "9999999".into(),
        },
        &mut ui,
    );
    processor.handle_component(
        "sprite",
        "",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    processor.handle_component(
        "room desc",
        "Second room.",
        &mut game_state,
        &mut components,
        &mut current,
        &mut dirty,
    );
    assert!(
        !game_state.room_description[0]
            .segments
            .iter()
            .any(|s| s.inline_image.is_some()),
        "unmapped room must not inherit the previous picture"
    );
}

/// Game art is OFF by default: the client must not send requests to
/// play.net because a user installed it. This pins the opt-in.
#[test]
fn game_art_is_off_by_default() {
    let config = Config::default();
    assert!(
        !config.game_art.enabled,
        "downloading from a third party must be an explicit opt-in"
    );
}

/// With the toggle off, a picture id resolves only against the user's own
/// pool — never a download.
#[test]
fn resource_picture_does_not_fetch_when_disabled() {
    use crate::core::custom_emoji::CustomEmojiRegistry;
    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());

    let mut config = Config::default();
    config.game_art.enabled = false;
    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let mut ui = UiState::new();
    let mut game_state = crate::core::state::GameState::new();

    processor.process_element(
        &crate::parser::ParsedElement::RoomPicture { id: 1 },
        &mut game_state,
        &mut ui,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );
    assert_eq!(
        game_state.story_picture, None,
        "no art installed and downloads off: the slot stays empty"
    );
}

/// `<resource picture='N'/>` must put the picture INTO the story line —
/// floated, with the room name wrapping beside it, the way Wrayth shows it.
/// Writing it to game_state alone rendered nothing at all.
#[test]
fn resource_picture_emits_a_floating_story_segment() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Pretend picture 7 has already been downloaded into the pool.
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: crate::core::game_art::pool_name(7),
        path: std::path::PathBuf::from("gs-art-7.jpg"),
        format: EmojiFormat::Jpeg,
    });
    crate::core::inline_image::set_for_test(registry);

    let mut config = Config::default();
    config.game_art.enabled = true;
    let mut processor = MessageProcessor::new(config, SavedDialogPositions::default());
    let mut ui = UiState::new();
    let mut game_state = crate::core::state::GameState::new();

    processor.process_element(
        &crate::parser::ParsedElement::RoomPicture { id: 7 },
        &mut game_state,
        &mut ui,
        &mut std::collections::HashMap::new(),
        &mut None,
        &mut false,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    let image = processor
        .current_segments
        .iter()
        .find_map(|s| s.inline_image.as_ref())
        .expect("the picture must ride the story line as a segment");
    assert_eq!(image.name, crate::core::game_art::pool_name(7));
    assert_eq!(image.align, crate::data::FloatAlign::Left, "floats left");

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

// ===========================================
// Prompt display after fallback-to-main streams (arena spectate)
// ===========================================

/// Stream text that falls back into the MAIN window (no subscriber window
/// exists) counts as main text, so the following unchanged prompt still
/// renders — Wrayth parity for spectate/familiar feeds shown in main.
#[test]
fn fallback_to_main_stream_text_keeps_prompts() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    processor.update_text_stream_subscribers(&ui_state);

    // Persistent game state so last_prompt carries across prompts (the skip
    // logic only fires when the prompt text is UNCHANGED).
    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;
    let mut drive = |processor: &mut MessageProcessor,
                     ui_state: &mut UiState,
                     game_state: &mut crate::core::state::GameState,
                     element: &crate::parser::ParsedElement| {
        processor.process_element(
            element,
            game_state,
            ui_state,
            &mut room_components,
            &mut current_room_component,
            &mut room_window_dirty,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    };

    use crate::parser::ParsedElement as E;
    let round = |processor: &mut MessageProcessor,
                 ui_state: &mut UiState,
                 game_state: &mut crate::core::state::GameState,
                 drive: &mut dyn FnMut(
        &mut MessageProcessor,
        &mut UiState,
        &mut crate::core::state::GameState,
        &E,
    ),
                 text: &str| {
        drive(
            processor,
            ui_state,
            game_state,
            &E::StreamPush {
                id: "watching".to_string(),
            },
        );
        drive(
            processor,
            ui_state,
            game_state,
            &E::Text {
                content: text.to_string(),
                stream: String::new(),
                fg_color: None,
                bg_color: None,
                bold: false,
                mono: false,
                span_type: crate::parser::SpanType::Normal,
                link_data: None,
            },
        );
        drive(processor, ui_state, game_state, &E::StreamPop);
        drive(
            processor,
            ui_state,
            game_state,
            &E::Prompt {
                time: "1786988768".to_string(),
                text: ">".to_string(),
            },
        );
    };

    let mut drive_ref = |p: &mut MessageProcessor,
                         u: &mut UiState,
                         g: &mut crate::core::state::GameState,
                         e: &E| drive(p, u, g, e);
    round(
        &mut processor,
        &mut ui_state,
        &mut game_state,
        &mut drive_ref,
        "Round 1 carnage!",
    );
    round(
        &mut processor,
        &mut ui_state,
        &mut game_state,
        &mut drive_ref,
        "Round 2 carnage!",
    );

    let lines = text_lines(&ui_state, "main");
    let prompt_lines = lines
        .iter()
        .filter(|l| {
            l.segments
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>()
                == ">"
        })
        .count();
    assert_eq!(
        prompt_lines,
        2,
        "both prompts must render after fallback-to-main spectate text; lines: {:?}",
        lines
            .iter()
            .map(|l| l
                .segments
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>())
            .collect::<Vec<_>>()
    );
}

/// Same as fallback_to_main_stream_text_keeps_prompts, but the main view is
/// a TAB in a tabbed text window (no window literally named "main") — the
/// layout that shipped the first version of this fix broken. Delivery goes
/// through the last-resort main-stream-subscriber path and must still count
/// as main text.
#[test]
fn fallback_to_tabbed_main_keeps_prompts() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    let mut story = crate::data::window::WindowState::new_text("story", 100);
    story.content = WindowContent::TabbedText(crate::data::TabbedTextContent::new(
        vec![(
            "Main".to_string(),
            vec!["main".to_string()],
            false,
            false,
            crate::config::TimestampPosition::End,
        )],
        100,
    ));
    ui_state.windows.insert("story".to_string(), story);
    processor.update_text_stream_subscribers(&ui_state);

    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;

    use crate::parser::ParsedElement as E;
    let elements = [
        E::StreamPush {
            id: "watching".to_string(),
        },
        E::Text {
            content: "Round 1 carnage!".to_string(),
            stream: String::new(),
            fg_color: None,
            bg_color: None,
            bold: false,
            mono: false,
            span_type: crate::parser::SpanType::Normal,
            link_data: None,
        },
        E::StreamPop,
        E::Prompt {
            time: "1786988768".to_string(),
            text: ">".to_string(),
        },
        E::StreamPush {
            id: "watching".to_string(),
        },
        E::Text {
            content: "Round 2 carnage!".to_string(),
            stream: String::new(),
            fg_color: None,
            bg_color: None,
            bold: false,
            mono: false,
            span_type: crate::parser::SpanType::Normal,
            link_data: None,
        },
        E::StreamPop,
        E::Prompt {
            time: "1786988769".to_string(),
            text: ">".to_string(),
        },
    ];
    for e in &elements {
        processor.process_element(
            e,
            &mut game_state,
            &mut ui_state,
            &mut room_components,
            &mut current_room_component,
            &mut room_window_dirty,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let window = ui_state.windows.get("story").expect("story window");
    let WindowContent::TabbedText(tabbed) = &window.content else {
        panic!("not tabbed");
    };
    let texts: Vec<String> = tabbed.tabs[0]
        .content
        .lines
        .iter()
        .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect())
        .collect();
    let prompts = texts.iter().filter(|t| t.as_str() == ">").count();
    assert_eq!(
        prompts, 2,
        "both prompts must render into the main tab; lines: {:?}",
        texts
    );
}

/// Injury updates must reach EVERY injury-doll window — per-window doll
/// sets mean several dolls render the same wound data; the old singleton
/// lookup left all but the first stale (owner report 2026-08-20: bound
/// doll windows never showed wounds).
#[test]
fn injury_updates_reach_every_injury_doll_window() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    for name in ["doll_a", "doll_b"] {
        let mut w = crate::data::window::WindowState::new_text(name, 10);
        w.widget_type = crate::data::WidgetType::InjuryDoll;
        w.content = WindowContent::InjuryDoll(crate::data::InjuryDollData::new());
        ui_state.windows.insert(name.to_string(), w);
    }

    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;
    processor.process_element(
        &crate::parser::ParsedElement::InjuryImage {
            id: "leftArm".to_string(),
            name: "Injury2".to_string(),
        },
        &mut game_state,
        &mut ui_state,
        &mut room_components,
        &mut current_room_component,
        &mut room_window_dirty,
        &mut None,
        &mut None,
        &mut None,
        None,
    );

    for name in ["doll_a", "doll_b"] {
        let WindowContent::InjuryDoll(doll) = &ui_state.windows[name].content else {
            panic!("not a doll");
        };
        assert_eq!(
            doll.injuries.get("leftArm").copied(),
            Some(2),
            "window '{name}' must receive the injury update"
        );
    }
    assert_eq!(game_state.injuries.get("leftArm").copied(), Some(2));
}

/// End-to-end: verbatim loot-stream lines from a 2026-01-24 Lich session
/// log, fed through the real parser, with the owner's live layout shape —
/// a hidden standalone "loot" text window AND a tabbed chat window whose
/// Loot tab subscribes the stream. BOTH must receive the lines (delivery
/// is per-subscriber, not first-match).
#[test]
fn loot_stream_reaches_tabbed_loot_tab_and_standalone_window() {
    let mut parser =
        crate::parser::XmlParser::with_presets(vec![], std::collections::HashMap::new());
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();

    let mut standalone = crate::data::window::WindowState::new_text("loot", 100);
    if let WindowContent::Text(content) = &mut standalone.content {
        content.streams = vec!["custom".to_string(), "loot".to_string()];
    }
    standalone.visible = false; // owner's layout hides it
    ui_state.windows.insert("loot".to_string(), standalone);

    let mut chat = crate::data::window::WindowState::new_text("chat", 100);
    chat.content = WindowContent::TabbedText(crate::data::TabbedTextContent::new(
        vec![
            (
                "Thoughts".to_string(),
                vec!["thoughts".to_string()],
                false,
                false,
                crate::config::TimestampPosition::End,
            ),
            (
                "Loot".to_string(),
                vec!["loot".to_string()],
                false,
                false,
                crate::config::TimestampPosition::End,
            ),
        ],
        100,
    ));
    ui_state.windows.insert("chat".to_string(), chat);
    processor.update_text_stream_subscribers(&ui_state);

    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;

    // Verbatim shape from the Lich log (bigshot.log.20260124 00:06:06).
    let log_lines = [
        r#"<pushStream id='loot'/>You search the area and find:"#,
        r#"(stowed in a <a exist="363871243" noun="longcoat">silver-threaded aquamarine byssus longcoat</a>)"#,
        r#"a <a exist="364964388" noun="pearl">tiny black pearl</a>"#,
        r#"<popStream/><prompt time="1786990406">&gt;</prompt>"#,
    ];
    for line in log_lines {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut game_state,
                &mut ui_state,
                &mut room_components,
                &mut current_room_component,
                &mut room_window_dirty,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    let tab_texts: Vec<String> = {
        let window = ui_state.windows.get("chat").expect("chat window");
        let WindowContent::TabbedText(tabbed) = &window.content else {
            panic!("not tabbed");
        };
        tabbed.tabs[1]
            .content
            .lines
            .iter()
            .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect())
            .collect()
    };
    assert!(
        tab_texts
            .iter()
            .any(|t| t.contains("You search the area and find:")),
        "loot lines must reach the tabbed Loot tab; tab lines: {tab_texts:?}"
    );
    assert!(
        tab_texts.iter().any(|t| t.contains("tiny black pearl")),
        "all loot lines must reach the tab; tab lines: {tab_texts:?}"
    );

    // Fragment glue may join the search results into fewer lines, so assert
    // on content, not line count.
    let standalone_texts: Vec<String> = {
        let window = ui_state.windows.get("loot").expect("loot window");
        let WindowContent::Text(content) = &window.content else {
            panic!("not text");
        };
        content
            .lines
            .iter()
            .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect())
            .collect()
    };
    assert!(
        standalone_texts
            .iter()
            .any(|t| t.contains("tiny black pearl")),
        "the standalone loot window must also receive the lines; lines: {standalone_texts:?}"
    );
}

/// End-to-end: verbatim lines from the 2026-08-17 13:13 spectate log, fed
/// through the real parser into the processor with a tabbed-main layout.
/// The fragment glue joins the announcer sentence and the trailing prompt
/// renders.
#[test]
fn spectate_log_lines_render_whole_with_prompts() {
    let mut parser =
        crate::parser::XmlParser::with_presets(vec![], std::collections::HashMap::new());
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    let mut story = crate::data::window::WindowState::new_text("story", 100);
    story.content = WindowContent::TabbedText(crate::data::TabbedTextContent::new(
        vec![(
            "Main".to_string(),
            vec!["main".to_string()],
            false,
            false,
            crate::config::TimestampPosition::End,
        )],
        100,
    ));
    ui_state.windows.insert("story".to_string(), story);
    processor.update_text_stream_subscribers(&ui_state);

    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;

    // Verbatim from the log (two prompts so the second is "unchanged").
    let log_lines = [
        r#"<prompt time="1786990405">&gt;</prompt>"#,
        r#"<pushStream id="familiar" ifClosedStyle="watching"/>An announcer shouts, "Round 4, send in<popStream/><pushStream id="familiar" ifClosedStyle="watching"/> another one!"  <popStream/><pushStream id="familiar" ifClosedStyle="watching"/>An iron portcullis is raised and<popStream/><pushStream id="familiar" ifClosedStyle="watching"/> <pushBold/>a <a exist="242449785" noun="ranger">grey-skinned gnoll ranger</a><popBold/> enters the arena!"#,
        r#"<popStream/><prompt time="1786990406">&gt;</prompt>"#,
    ];
    for line in log_lines {
        for element in parser.parse_line(line) {
            processor.process_element(
                &element,
                &mut game_state,
                &mut ui_state,
                &mut room_components,
                &mut current_room_component,
                &mut room_window_dirty,
                &mut None,
                &mut None,
                &mut None,
                None,
            );
        }
    }

    let window = ui_state.windows.get("story").expect("story window");
    let WindowContent::TabbedText(tabbed) = &window.content else {
        panic!("not tabbed");
    };
    let texts: Vec<String> = tabbed.tabs[0]
        .content
        .lines
        .iter()
        .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect())
        .collect();

    // The announcer sentence is glued back into one line.
    assert!(
        texts.iter().any(|t| t.contains(
            "An announcer shouts, \"Round 4, send in another one!\"  An iron portcullis is raised and a grey-skinned gnoll ranger enters the arena!"
        )),
        "expected the glued announcer line; lines: {:?}",
        texts
    );
    // The prompt AFTER the spectate text renders even though unchanged.
    assert_eq!(
        texts.last().map(|s| s.as_str()),
        Some(">"),
        "trailing prompt must render; lines: {:?}",
        texts
    );
}

/// With a familiar window present, each prompt after familiar-stream text
/// echoes into the familiar window as a round separator (arena spectate),
/// independent of the main window's prompt dedupe. Owner request 2026-08-17.
#[test]
fn prompts_echo_into_familiar_window_after_familiar_text() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    ui_state.windows.insert(
        "familiar".to_string(),
        make_text_window("familiar", &["familiar"]),
    );
    processor.update_text_stream_subscribers(&ui_state);

    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;

    use crate::parser::ParsedElement as E;
    let make_text = |t: &str| E::Text {
        content: t.to_string(),
        stream: String::new(),
        fg_color: None,
        bg_color: None,
        bold: false,
        mono: false,
        span_type: crate::parser::SpanType::Normal,
        link_data: None,
    };
    let elements = [
        E::StreamPush {
            id: "familiar".to_string(),
        },
        make_text("Round 1 carnage!"),
        E::StreamPop,
        E::Prompt {
            time: "1".to_string(),
            text: ">".to_string(),
        },
        E::StreamPush {
            id: "familiar".to_string(),
        },
        make_text("Round 2 carnage!"),
        E::StreamPop,
        E::Prompt {
            time: "2".to_string(),
            text: ">".to_string(),
        },
        // A prompt with NO familiar text since the last one: no echo.
        E::Prompt {
            time: "3".to_string(),
            text: ">".to_string(),
        },
    ];
    for e in &elements {
        processor.process_element(
            e,
            &mut game_state,
            &mut ui_state,
            &mut room_components,
            &mut current_room_component,
            &mut room_window_dirty,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let fam: Vec<String> = text_lines(&ui_state, "familiar")
        .iter()
        .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect())
        .collect();
    let prompts = fam.iter().filter(|t| t.as_str() == ">").count();
    assert_eq!(
        prompts, 2,
        "one echoed prompt per familiar-active round, none for idle prompts; familiar lines: {:?}",
        fam
    );
    // Order: text, prompt, text, prompt.
    assert_eq!(fam[0], "Round 1 carnage!");
    assert_eq!(fam[1], ">");
    assert_eq!(fam[2], "Round 2 carnage!");
    assert_eq!(fam[3], ">");
}

/// A redirect script that moves whole lines carries the game's prompt into
/// the familiar stream as plain text — uncolored and missing the roundtime
/// marker. Those moved prompt lines are STRIPPED; the prompt echo is the
/// single styled separator (owner decision 2026-08-19: strip the moved
/// copy, keep the echo — it has coloring and the R).
#[test]
fn familiar_stream_strips_moved_prompts_keeping_the_styled_echo() {
    let mut processor = create_test_processor();
    let mut ui_state = UiState::new();
    ui_state
        .windows
        .insert("main".to_string(), make_text_window("main", &["main"]));
    ui_state.windows.insert(
        "familiar".to_string(),
        make_text_window("familiar", &["familiar"]),
    );
    processor.update_text_stream_subscribers(&ui_state);

    let mut game_state = crate::core::state::GameState::new();
    let mut room_components = std::collections::HashMap::new();
    let mut current_room_component = None;
    let mut room_window_dirty = false;

    use crate::parser::ParsedElement as E;
    let make_text = |t: &str| E::Text {
        content: t.to_string(),
        stream: String::new(),
        fg_color: None,
        bg_color: None,
        bold: false,
        mono: false,
        span_type: crate::parser::SpanType::Normal,
        link_data: None,
    };
    let elements = [
        // Redirected chunk: the script moved the text AND the prompt line.
        E::StreamPush {
            id: "familiar".to_string(),
        },
        make_text("A caustic melody ushers in the arrival of a troll wraith."),
        E::StreamPop,
        E::StreamPush {
            id: "familiar".to_string(),
        },
        make_text(">"),
        E::StreamPop,
        // The real prompt carries roundtime — the echo shows it, the
        // stripped moved copy never could.
        E::Prompt {
            time: "1".to_string(),
            text: "R>".to_string(),
        },
        // Spectate-style chunk WITHOUT an embedded prompt: echo still fires.
        E::StreamPush {
            id: "familiar".to_string(),
        },
        make_text("The troll wraith looks miffed."),
        E::StreamPop,
        E::Prompt {
            time: "2".to_string(),
            text: ">".to_string(),
        },
    ];
    for e in &elements {
        processor.process_element(
            e,
            &mut game_state,
            &mut ui_state,
            &mut room_components,
            &mut current_room_component,
            &mut room_window_dirty,
            &mut None,
            &mut None,
            &mut None,
            None,
        );
    }

    let fam: Vec<String> = text_lines(&ui_state, "familiar")
        .iter()
        .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect())
        .collect();
    assert_eq!(
        fam,
        vec![
            "A caustic melody ushers in the arrival of a troll wraith.".to_string(),
            "R>".to_string(), // the styled echo, WITH the roundtime marker
            "The troll wraith looks miffed.".to_string(),
            ">".to_string(), // spectate echo still fires without a moved prompt
        ],
        "moved prompt stripped, styled echo kept; familiar lines: {fam:?}"
    );
}

// ===========================================
// PR #32 risk probes: remote room-story duplication (Risk A) and torn
// inventory snapshots (Risk B). Full-pipeline replays: raw XML through the
// real parser into process_element, flushing at every network-line boundary
// exactly as AppCore does.
// ===========================================

/// Full-pipeline replay harness mirroring AppCore's per-network-line flush.
struct RiskReplay {
    parser: crate::parser::XmlParser,
    processor: MessageProcessor,
    game_state: GameState,
    ui_state: UiState,
    room_components: std::collections::HashMap<String, Vec<Vec<TextSegment>>>,
    current_room_component: Option<String>,
    room_window_dirty: bool,
    nav_room_id: Option<String>,
    lich_room_id: Option<String>,
    room_subtitle: Option<String>,
    handles: crate::core::remote::RemoteServerHandles,
}

impl RiskReplay {
    fn new() -> Self {
        let mut processor = create_test_processor();
        let (sink, handles, _events) = crate::core::remote::RemoteSink::new(100);
        processor.remote = Some(sink);
        Self {
            parser: crate::parser::XmlParser::new(),
            processor,
            game_state: GameState::new(),
            ui_state: UiState::new(),
            room_components: std::collections::HashMap::new(),
            current_room_component: None,
            room_window_dirty: false,
            nav_room_id: None,
            lich_room_id: None,
            room_subtitle: None,
            handles,
        }
    }

    fn replay(&mut self, xml: &str) {
        for element in self.parser.parse_line(xml) {
            self.processor.process_element(
                &element,
                &mut self.game_state,
                &mut self.ui_state,
                &mut self.room_components,
                &mut self.current_room_component,
                &mut self.room_window_dirty,
                &mut self.nav_room_id,
                &mut self.lich_room_id,
                &mut self.room_subtitle,
                None,
            );
        }
        self.processor
            .flush_current_stream_with_tts(&mut self.ui_state, None);
    }

    fn story_text(&self) -> Vec<String> {
        self.handles
            .buffer
            .lock()
            .unwrap()
            .tail("main", 100)
            .iter()
            .map(|line| {
                line.line
                    .segments
                    .iter()
                    .map(|segment| segment.text.as_str())
                    .collect()
            })
            .collect()
    }
}

/// Risk A / variant 1: Lich header WITHOUT the `(uNNN)` suffix (uid display
/// off) but with `<nav>` arriving BEFORE the header, as in a normal Lich room
/// block. `native_room_header_identity` falls back to `current_room_uid`, so
/// the native block should be recognized and the fallback suppressed.
#[test]
fn risk_a_header_without_uid_suffix_nav_first_no_duplicate() {
    let mut rig = RiskReplay::new();
    rig.replay("<nav rm='7120'/>");
    rig.replay("<streamWindow id='room' title='Room' subtitle=' - Town Square Central'/>");
    rig.replay("<pushStream id='room'/>");
    rig.replay("<compDef id='room desc'>This is the heart of the main square.</compDef>");
    rig.replay("<compDef id='room exits'>Obvious paths: <d>northeast</d></compDef>");
    rig.replay("<popStream id='room'/>");
    rig.replay("<style id='roomName'/>[Town Square Central - 228]<style id=''/>");
    rig.replay("This is the heart of the main square.");
    rig.replay("Obvious paths: northeast");
    rig.replay("<prompt time='1'>&gt;</prompt>");

    let story = rig.story_text();
    assert_eq!(
        story
            .iter()
            .filter(|line| line.contains("Town Square Central"))
            .count(),
        1,
        "uid-suffix-off header with nav-first must not duplicate: {story:?}"
    );
    assert_eq!(
        story
            .iter()
            .filter(|line| line.as_str() == "This is the heart of the main square.")
            .count(),
        1,
        "room prose must appear once: {story:?}"
    );
}

/// Risk A / variant 2: header with NO ` - <number>` at all — the format the
/// game itself sends in direct/no-Lich mode (and Lich with room numbers
/// disabled): `[Town Square Central]`. `native_room_header_identity` requires
/// ` - <digits>`, so the native block goes unrecognized.
#[test]
fn risk_a_header_without_room_number_duplicates() {
    let mut rig = RiskReplay::new();
    rig.replay("<nav rm='7120'/>");
    rig.replay("<streamWindow id='room' title='Room' subtitle=' - Town Square Central'/>");
    rig.replay("<pushStream id='room'/>");
    rig.replay("<compDef id='room desc'>This is the heart of the main square.</compDef>");
    rig.replay("<compDef id='room exits'>Obvious paths: <d>northeast</d></compDef>");
    rig.replay("<popStream id='room'/>");
    rig.replay("<style id='roomName'/>[Town Square Central]<style id=''/>");
    rig.replay("This is the heart of the main square.");
    rig.replay("Obvious paths: northeast");
    rig.replay("<prompt time='1'>&gt;</prompt>");

    let story = rig.story_text();
    assert_eq!(
        story
            .iter()
            .filter(|line| line.contains("Town Square Central"))
            .count(),
        1,
        "numberless native header must still suppress the fallback: {story:?}"
    );
    assert_eq!(
        story
            .iter()
            .filter(|line| line.as_str() == "This is the heart of the main square.")
            .count(),
        1,
        "room prose must appear once: {story:?}"
    );
}

/// Risk A / variant 3: no uid suffix AND `<nav>` arriving AFTER the header
/// line (header printed first, nav late in the same block). The header falls
/// back to the STALE previous-room uid, mismatching the pending identity.
#[test]
fn risk_a_header_before_nav_stale_uid_duplicates() {
    let mut rig = RiskReplay::new();
    // Establish a previous room so current_room_uid holds a stale value.
    rig.replay("<nav rm='7119'/>");
    rig.replay("<streamWindow id='room' title='Room' subtitle=' - Old Room'/>");
    rig.replay("<pushStream id='room'/>");
    rig.replay("<compDef id='room desc'>The old room.</compDef>");
    rig.replay("<popStream id='room'/>");
    rig.replay("<style id='roomName'/>[Old Room - 227]<style id=''/>");
    rig.replay("The old room.");
    rig.replay("<prompt time='1'>&gt;</prompt>");

    // Movement where the decorated header precedes <nav>.
    rig.replay("<style id='roomName'/>[Town Square Central - 228]<style id=''/>");
    rig.replay("This is the heart of the main square.");
    rig.replay("Obvious paths: northeast");
    rig.replay("<nav rm='7120'/>");
    rig.replay("<streamWindow id='room' title='Room' subtitle=' - Town Square Central'/>");
    rig.replay("<pushStream id='room'/>");
    rig.replay("<compDef id='room desc'>This is the heart of the main square.</compDef>");
    rig.replay("<compDef id='room exits'>Obvious paths: <d>northeast</d></compDef>");
    rig.replay("<popStream id='room'/>");
    rig.replay("<prompt time='2'>&gt;</prompt>");

    let story = rig.story_text();
    assert_eq!(
        story
            .iter()
            .filter(|line| line.contains("Town Square Central"))
            .count(),
        1,
        "header-before-nav must not duplicate: {story:?}"
    );
    assert_eq!(
        story
            .iter()
            .filter(|line| line.as_str() == "This is the heart of the main square.")
            .count(),
        1,
        "room prose must appear once: {story:?}"
    );
}

/// Risk B: a `<prompt>` force-closing a mid-flight `inv` stream (eaten
/// popStream) makes the parser emit a synthetic StreamPop, and
/// `flush_inventory_buffer` commits whatever partial lines arrived as the
/// authoritative snapshot — replacing a complete previous inventory with a
/// truncated one.
#[test]
fn risk_b_prompt_torn_inv_stream_commits_truncated_snapshot() {
    let mut rig = RiskReplay::new();

    // A complete, healthy snapshot first.
    rig.replay("<pushStream id='inv'/>Your worn items are:");
    rig.replay("  <a exist='1' noun='cloak'>a wool cloak</a>");
    rig.replay("  <a exist='2' noun='boots'>some leather boots</a>");
    rig.replay("<popStream/>");
    rig.replay("<prompt time='1'>&gt;</prompt>");
    assert_eq!(
        rig.game_state.inventory.len(),
        3,
        "complete snapshot must commit 3 lines"
    );

    // Torn refresh: push + one line, then the popStream is eaten upstream
    // and a prompt arrives mid-stream.
    rig.replay("<pushStream id='inv'/>Your worn items are:");
    rig.replay("<prompt time='2'>&gt;</prompt>");

    let inv: Vec<String> = rig
        .game_state
        .inventory
        .iter()
        .map(|line| {
            line.segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        inv,
        vec![
            "Your worn items are:".to_string(),
            "  a wool cloak".to_string(),
            "  some leather boots".to_string(),
        ],
        "a torn inv stream must not replace a complete snapshot with a truncated one; committed: {inv:?}"
    );
}
