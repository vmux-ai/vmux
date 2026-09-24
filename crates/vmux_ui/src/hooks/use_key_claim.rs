use crate::hooks::use_ui_state::use_ui_state;
use crate::key_stroke::PressedKey;
use crate::transport::event_listener::send;
use dioxus::prelude::*;
use vmux_core::input::{KeyClaims, KeyStroke, KeyVerdict, PageKeyContext, Unclaimed};

pub fn use_key_claim(
    unclaimed: Unclaimed,
    context: impl Fn() -> Vec<String> + 'static,
) -> KeyClaim {
    let claims = use_ui_state::<KeyClaims>();
    let resolves = use_hook(crate::transport::Host::resolves_keys);

    use_effect(move || {
        if !resolves {
            return;
        }
        let _ = send(&PageKeyContext { keys: context() });
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
