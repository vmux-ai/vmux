use crate::hooks::use_ui_state::use_ui_state;
use crate::key_stroke::PressedKey;
use crate::transport::event_listener::send;
use dioxus::prelude::*;
use vmux_core::input::{KeyClaims, KeyStroke, KeyVerdict, UiKeyContext, Unclaimed};

pub fn use_key_claim(
    unclaimed: Unclaimed,
    context: impl Fn() -> Vec<String> + 'static,
) -> KeyClaim {
    let claims = use_ui_state::<KeyClaims>().state;
    let resolves = use_hook(crate::transport::Host::resolves_keys);

    use_effect(move || {
        if !resolves {
            return;
        }
        let _ = send(&UiKeyContext { keys: context() });
    });

    KeyClaim {
        claims,
        unclaimed,
        resolves,
    }
}

#[derive(Clone, Copy)]
pub struct KeyClaim {
    claims: Signal<KeyClaims>,
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
        let verdict = KeyVerdict::decide(
            &self.claims.read(),
            self.unclaimed,
            &stroke,
            wanted_locally(&stroke),
        );
        if verdict == KeyVerdict::Browser {
            return;
        }
        event.prevent_default();
        let _ = send(&stroke);
    }
}
