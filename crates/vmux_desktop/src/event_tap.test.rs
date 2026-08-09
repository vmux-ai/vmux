use super::*;
use bevy::input::keyboard::KeyCode;
use vmux_command::{LayoutCommand, PaneCommand};

#[test]
fn cg_flags_map_to_modifiers() {
    let m = modifiers_from_cg_flags(CGEventFlags::MaskCommand);
    assert!(m.super_key && !m.ctrl && !m.alt && !m.shift);

    let m = modifiers_from_cg_flags(CGEventFlags::MaskControl | CGEventFlags::MaskShift);
    assert!(m.ctrl && m.shift && !m.alt && !m.super_key);

    let m = modifiers_from_cg_flags(CGEventFlags::MaskAlternate);
    assert!(m.alt && !m.ctrl && !m.shift && !m.super_key);

    let m = modifiers_from_cg_flags(CGEventFlags::empty());
    assert!(!m.ctrl && !m.shift && !m.alt && !m.super_key);
}

#[test]
fn combo_from_cg_resolves_known_keycode() {
    // 0x09 == kVK_ANSI_V
    let combo = combo_from_cg(0x09, CGEventFlags::MaskCommand).expect("combo");
    assert_eq!(combo.key, KeyCode::KeyV);
    assert!(combo.modifiers.super_key);
}

#[test]
fn combo_from_cg_rejects_unknown_keycode() {
    assert!(combo_from_cg(0xFFFF, CGEventFlags::empty()).is_none());
}

#[test]
fn gate_passes_without_classifying_when_not_frontmost() {
    let outcome = gate(false, || panic!("must not classify when not frontmost"));
    assert!(matches!(outcome, TapOutcome::Pass));
}

#[test]
fn gate_consumes_when_frontmost_and_classifier_consumes() {
    let cmd = AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SelectLeft));
    let outcome = gate(true, || KeyAction::Consume(Some(cmd.clone())));
    match outcome {
        TapOutcome::Consume(Some(c)) => assert_eq!(c, cmd),
        _ => panic!("expected consume"),
    }
}

#[test]
fn gate_passes_when_frontmost_and_classifier_passes() {
    let outcome = gate(true, || KeyAction::PassThrough);
    assert!(matches!(outcome, TapOutcome::Pass));
}
