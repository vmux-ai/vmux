use crate::settings::AppSettings;
use crate::terminal::PtyExited;
use bevy::prelude::*;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

/// Check if confirmation is needed based on settings.
pub fn should_confirm(settings: &AppSettings) -> bool {
    settings
        .terminal
        .as_ref()
        .map_or(true, |t| t.confirm_close)
}

/// Check if a tab entity has any child terminal that is still running.
pub fn has_live_terminal(
    tab: Entity,
    children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
) -> bool {
    if let Ok(children) = children_q.get(tab) {
        children.iter().any(|child| terminal_q.contains(child))
    } else {
        false
    }
}

/// Check if a pane has any tab with a live terminal.
pub fn pane_has_live_terminal(
    pane: Entity,
    pane_children_q: &Query<&Children, With<crate::layout::pane::Pane>>,
    all_children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
) -> bool {
    if let Ok(tabs) = pane_children_q.get(pane) {
        tabs.iter()
            .any(|tab| has_live_terminal(tab, all_children_q, terminal_q))
    } else {
        false
    }
}

/// Show confirmation dialog for closing a terminal tab/pane.
/// Returns `true` if user confirms the close.
pub fn confirm_close_dialog() -> bool {
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Close Terminal?")
        .set_description("A process is still running in this terminal. Close anyway?")
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}

/// Show confirmation dialog for quitting with N running terminals.
/// Returns `true` if user confirms the quit.
pub fn confirm_quit_dialog(count: usize) -> bool {
    let msg = if count == 1 {
        "A terminal is still running. Quit anyway?".to_string()
    } else {
        format!("{count} terminals are still running. Quit anyway?")
    };
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Quit Vmux?")
        .set_description(&msg)
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}
