#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::input::KeyModifiers;
use vmux_ui::hooks::{send, use_theme};
use vmux_ui::i18n::translate;

use super::branches::BranchPromptDialog;
use super::dashboard::GitDashboard;
use super::empty::EmptyRepository;
use super::state::GitPageState;
use crate::event::{GitKeyRequest, GitShortcutHelpRequest};

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = GitPageState::use_state();
    use_context_provider(|| state);
    let GitPageState {
        snapshot,
        controller,
        ..
    } = state;

    rsx! {
        document::Title { {translate("git-title")} }
        div {
            class: "relative flex h-screen min-w-0 flex-col bg-background text-foreground outline-none",
            tabindex: "-1",
            autofocus: true,
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape && (state.controller)().shortcut_help_visible {
                    event.prevent_default();
                    event.stop_propagation();
                    let _ = send(&GitShortcutHelpRequest { visible: false });
                    return;
                }
                let modifiers = event.modifiers();
                let request = GitKeyRequest {
                    key: event.key().to_string(),
                    code: event.code().to_string(),
                    modifiers: KeyModifiers {
                        ctrl: modifiers.ctrl(),
                        shift: modifiers.shift(),
                        alt: modifiers.alt(),
                        super_key: modifiers.meta(),
                    },
                    repeat: event.is_auto_repeating(),
                };
                let captured = request.captures_browser_default(snapshot().repository.is_some());
                if send(&request).is_ok() && captured {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            if snapshot().repository.is_some() {
                GitDashboard {}
            } else {
                EmptyRepository {}
            }
            if controller().branch_prompt.is_some() {
                BranchPromptDialog {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::GitPanel;

    #[test]
    fn number_keys_select_panels() {
        assert_eq!(GitPanel::from_key("0"), Some(GitPanel::Status));
        assert_eq!(GitPanel::from_key("1"), Some(GitPanel::Status));
        assert_eq!(GitPanel::from_key("2"), Some(GitPanel::Files));
        assert_eq!(GitPanel::from_key("3"), Some(GitPanel::Branches));
        assert_eq!(GitPanel::from_key("4"), Some(GitPanel::Commits));
        assert_eq!(GitPanel::from_key("5"), Some(GitPanel::Stash));
        assert_eq!(GitPanel::from_key("6"), None);
    }

    #[test]
    fn tab_cycles_panels_in_both_directions() {
        assert_eq!(GitPanel::Status.next(false), GitPanel::Files);
        assert_eq!(GitPanel::Commits.next(false), GitPanel::Stash);
        assert_eq!(GitPanel::Stash.next(false), GitPanel::Status);
        assert_eq!(GitPanel::Status.next(true), GitPanel::Stash);
        assert_eq!(GitPanel::Stash.next(true), GitPanel::Commits);
        assert_eq!(GitPanel::Files.next(true), GitPanel::Status);
    }
}
