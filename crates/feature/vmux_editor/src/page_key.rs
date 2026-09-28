use dioxus::prelude::*;
use vmux_core::event::FilePanelState;
use vmux_core::input::{UiKeyContext, Unclaimed};
use vmux_ui::hooks::{KeyClaim, send, use_key_claim};

pub(crate) fn use_file_keys(panel: Signal<FilePanelState>) -> FileKeys {
    let keys = FileKeys {
        claim: use_key_claim(Unclaimed::Types, move || {
            let mut keys = vec!["files".to_string()];
            if panel().content.is_some() {
                keys.push("files.panel".to_string());
            }
            keys
        }),
    };
    use_drop(move || {
        let _ = send(&UiKeyContext { keys: Vec::new() });
    });
    keys
}

#[derive(Clone, Copy)]
pub struct FileKeys {
    claim: KeyClaim,
}

impl FileKeys {
    pub fn offer(&self, event: &Event<KeyboardData>) -> bool {
        self.claim.on_keydown(event, |_| false);
        !event.default_action_enabled()
    }
}
