use super::*;

#[test]
fn prereq_needs_homebrew_logic() {
    if cfg!(target_os = "macos") {
        assert!(prereq_needs_homebrew("claude", false));
        assert!(prereq_needs_homebrew("codex", false));
        assert!(!prereq_needs_homebrew("claude", true));
    } else {
        assert!(!prereq_needs_homebrew("claude", false));
    }
    assert!(!prereq_needs_homebrew("vibe", false));
    assert!(!prereq_needs_homebrew("nope", false));
}

#[test]
fn install_outcome_gates_on_armed_and_presence() {
    assert_eq!(install_outcome(false, true), None);
    assert_eq!(install_outcome(false, false), None);
    assert_eq!(install_outcome(true, true), Some(true));
    assert_eq!(install_outcome(true, false), Some(false));
}

#[test]
fn successful_manager_install_closes_terminal_pane() {
    assert!(close_install_pane_after_success("vmux://agents"));
    assert!(close_install_pane_after_success("vmux://agents/"));
    assert!(!close_install_pane_after_success(
        "vmux://agent/codex/setup"
    ));
}
