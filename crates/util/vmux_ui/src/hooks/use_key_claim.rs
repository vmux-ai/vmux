use crate::hooks::UiState;
use crate::key_stroke::PressedKey;
use crate::transport::event_listener::send;
use dioxus::prelude::*;
use vmux_api::input::{KeyContextRequest, KeyStroke, UiKeyClaims};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unclaimed {
    Types,
    Forwards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyVerdict {
    Browser,
    Send,
}

impl KeyVerdict {
    fn decide(
        claims: &UiKeyClaims,
        unclaimed: Unclaimed,
        stroke: &KeyStroke,
        wanted_locally: bool,
    ) -> Self {
        if wanted_locally {
            return Self::Browser;
        }
        if claims.contains(stroke) {
            return Self::Send;
        }
        match unclaimed {
            Unclaimed::Types => Self::Browser,
            Unclaimed::Forwards => Self::Send,
        }
    }
}

pub fn use_key_claim(
    unclaimed: Unclaimed,
    context: impl Fn() -> Vec<String> + 'static,
) -> KeyClaim {
    let claims = UiState::<UiKeyClaims>::use_context().state;
    let resolves = use_hook(crate::transport::Host::resolves_keys);

    use_effect(move || {
        if !resolves {
            return;
        }
        send(&KeyContextRequest { keys: context() });
    });

    KeyClaim {
        claims,
        unclaimed,
        resolves,
    }
}

#[derive(Clone, Copy)]
pub struct KeyClaim {
    claims: Signal<UiKeyClaims>,
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
        send(&stroke);
    }
}
