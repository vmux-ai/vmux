use crate::hooks::use_ui_state::use_ui_state;
use crate::key_stroke::PressedKey;
use crate::transport::event_listener::send;
use dioxus::prelude::*;
use vmux_api::input::{KeyClaimsUiState, KeyContextRequest, KeyStroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unclaimed {
    Types,
    Forwards,
}

pub fn use_key_claim(
    unclaimed: Unclaimed,
    context: impl Fn() -> Vec<String> + 'static,
) -> KeyClaim {
    let claims = use_ui_state::<KeyClaimsUiState>().state;
    let resolves = use_hook(crate::transport::Host::resolves_keys);

    use_effect(move || {
        if !resolves {
            return;
        }
        let _ = send(&KeyContextRequest { keys: context() });
    });

    KeyClaim {
        claims,
        unclaimed,
        resolves,
    }
}

#[derive(Clone, Copy)]
pub struct KeyClaim {
    claims: Signal<KeyClaimsUiState>,
    unclaimed: Unclaimed,
    resolves: bool,
}

impl KeyClaim {
    pub fn resolves(&self) -> bool {
        self.resolves
    }

    pub fn command(&self, event: &Event<KeyboardData>) -> Option<String> {
        let stroke = PressedKey::new(&event.data()).stroke()?;
        self.claims.read().command(&stroke).map(str::to_string)
    }

    pub fn on_keydown(
        &self,
        event: &Event<KeyboardData>,
        wanted_locally: impl FnOnce(&KeyStroke) -> bool,
    ) {
        let data = event.data();
        let Some(stroke) = PressedKey::new(&data).stroke() else {
            return;
        };
        if stroke.is_modifier_key() {
            return;
        }
        if !self.should_send(&stroke, wanted_locally(&stroke)) {
            return;
        }
        event.prevent_default();
        let _ = send(&stroke);
    }

    fn should_send(&self, stroke: &KeyStroke, wanted_locally: bool) -> bool {
        if wanted_locally {
            return false;
        }
        if self.claims.read().contains(stroke) {
            return true;
        }
        match self.unclaimed {
            Unclaimed::Types => false,
            Unclaimed::Forwards => true,
        }
    }
}
