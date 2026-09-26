use bevy::prelude::*;
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_core::input::KeyStroke;
use vmux_service::{client::ServiceRequest, plugin::ServiceConnected};

use super::plugin::ServiceMessageSet;

pub(crate) struct PromptPlugin;

impl Plugin for PromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, flush_buffered_agent_prompt.after(ServiceMessageSet));
    }
}

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct BufferedAgentPrompt {
    pub text: String,
    pub submit: bool,
}

#[derive(Component, Debug, Clone, Default)]
pub struct PromptCapture {
    pub draft: String,
    pub skipped: bool,
}

impl PromptCapture {
    pub(crate) fn wants_paste(event: &KeyStroke) -> bool {
        event.mods.super_key && event.code == "KeyV"
    }

    pub(crate) fn apply(&mut self, event: &KeyStroke, pasted: Option<String>) -> bool {
        if event.mods.ctrl && event.code == "KeyC" {
            self.draft.clear();
            self.skipped = false;
            return true;
        }
        if Self::wants_paste(event) {
            let Some(pasted) = pasted else { return false };
            if !self.draft.is_empty() && !self.draft.ends_with(char::is_whitespace) {
                self.draft.push(' ');
            }
            self.draft.push_str(&pasted);
            self.skipped = false;
            return true;
        }
        match event.key.as_str() {
            "Escape" => {
                self.draft.clear();
                self.skipped = true;
                true
            }
            "Backspace" => self.draft.pop().is_some(),
            _ if event.is_text_input() => {
                self.draft.push_str(event.typed_text());
                self.skipped = false;
                true
            }
            _ => false,
        }
    }
}

fn agent_prompt_flush_bytes(alt_screen: bool, prompt: &BufferedAgentPrompt) -> Option<Vec<u8>> {
    if !alt_screen {
        return None;
    }
    let bytes = crate::shell_input::bracketed_paste_input(&prompt.text, prompt.submit);
    (!bytes.is_empty()).then_some(bytes)
}

fn flush_buffered_agent_prompt(
    prompts: Query<
        (Entity, &ProcessId, &BufferedAgentPrompt),
        With<vmux_core::agent::AgentSession>,
    >,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_none() {
        return;
    }
    for (entity, process_id, prompt) in &prompts {
        if let Some(data) = agent_prompt_flush_bytes(true, prompt) {
            service_requests.write(ServiceRequest(ClientMessage::ProcessInput {
                process_id: *process_id,
                data,
            }));
        }
        commands.entity(entity).remove::<BufferedAgentPrompt>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::input::KeyModifiers;

    const CTRL: KeyModifiers = KeyModifiers {
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    };
    const SUPER: KeyModifiers = KeyModifiers {
        ctrl: false,
        shift: false,
        alt: false,
        super_key: true,
    };

    fn press(key: &str, code: &str, mods: KeyModifiers) -> KeyStroke {
        KeyStroke {
            key: key.to_string(),
            code: code.to_string(),
            mods,
            text: None,
            repeat: false,
        }
    }

    fn typed(key: &str, code: &str) -> KeyStroke {
        press(key, code, KeyModifiers::default())
    }

    #[test]
    fn the_draft_takes_text_and_refuses_everything_else() {
        let mut capture = PromptCapture::default();

        assert!(capture.apply(&typed("h", "KeyH"), None));
        assert!(capture.apply(&typed("i", "KeyI"), None));
        assert_eq!(capture.draft, "hi");

        assert!(!capture.apply(&press("i", "KeyI", CTRL), None));
        assert!(!capture.apply(&typed("Enter", "Enter"), None));
        assert!(!capture.apply(&typed("F5", "F5"), None));
        assert_eq!(capture.draft, "hi", "a chord or a bare action is not text");

        assert!(capture.apply(&typed("Backspace", "Backspace"), None));
        assert_eq!(capture.draft, "h");
    }

    #[test]
    fn escape_declines_the_prompt_and_ctrl_c_only_clears_it() {
        let mut capture = PromptCapture::default();
        capture.apply(&typed("h", "KeyH"), None);

        assert!(capture.apply(&typed("Escape", "Escape"), None));
        assert_eq!((capture.draft.as_str(), capture.skipped), ("", true));

        capture.apply(&typed("h", "KeyH"), None);
        assert_eq!((capture.draft.as_str(), capture.skipped), ("h", false));

        assert!(capture.apply(&press("c", "KeyC", CTRL), None));
        assert_eq!((capture.draft.as_str(), capture.skipped), ("", false));
    }

    #[test]
    fn a_press_that_changes_nothing_reports_no_change() {
        let mut capture = PromptCapture::default();

        assert!(!capture.apply(&typed("Backspace", "Backspace"), None));
        assert!(!capture.apply(&typed("Shift", "ShiftLeft"), None));
        assert!(capture.draft.is_empty());
    }

    #[test]
    fn paste_is_separated_from_the_draft_it_joins() {
        let paste = press("v", "KeyV", SUPER);
        let mut capture = PromptCapture::default();
        capture.apply(&typed("g", "KeyG"), None);

        assert!(capture.apply(&paste, Some("o run".to_string())));
        assert_eq!(capture.draft, "g o run");

        assert!(!capture.apply(&paste, None));
        assert_eq!(capture.draft, "g o run");
    }
}
