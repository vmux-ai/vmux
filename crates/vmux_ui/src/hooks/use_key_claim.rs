//! Publishing a page's key context, and acting on what the core claimed back.
//!
//! A printable key has to reach the `<textarea>` in the same tick, so a page cannot wait for the
//! core to rule on it. [`KeyClaim`] composes three things that fail in three different ways: the
//! set the core pushed on the page's last context change, which goes briefly stale and costs at
//! most a dropped shortcut; a predicate the page answers itself from the caret and its own mode,
//! which is never stale; and [`Unclaimed`], the standing policy for a key nobody spoke for. The
//! rule combining them is [`KeyVerdict::of`], next to the wire types both sides read.
//!
//! There is deliberately no fourth part for a half-typed chord. Suppression happens in the Bevy
//! process, upstream of the page — a CEF browser here is offscreen and sees no key the host did not
//! forward — so the second key of a chord never arrives and there is no round trip to lose.
//!
//! Nothing here knows what a key *means*. It tests membership and asks the page a yes/no question.

use crate::hooks::use_event::use_event;
use crate::host::event_listener::send;
use crate::key_stroke::WebKey;
use dioxus::prelude::*;
use vmux_core::input::{
    KEY_CLAIMS_EVENT, KeyClaims, KeyStroke, KeyVerdict, PageKeyContext, Unclaimed,
};

/// Subscribe a page to the keyboard seam.
///
/// `context` is read reactively: whatever signals it touches become the trigger for republishing,
/// so a page says what is true of it now by reading its own state rather than by remembering to
/// announce a change. The core answers with the strokes this page must hand over.
pub fn use_key_claim(
    unclaimed: Unclaimed,
    context: impl Fn() -> Vec<String> + 'static,
) -> KeyClaim {
    let claims = use_event::<KeyClaims>(KEY_CLAIMS_EVENT, KeyClaims::default);

    use_effect(move || {
        let _ = send(&PageKeyContext { keys: context() });
    });

    KeyClaim { claims, unclaimed }
}

/// A page's keyboard: what the core has claimed from it, and what it does with the rest.
#[derive(Clone, Copy)]
pub struct KeyClaim {
    claims: Signal<KeyClaims>,
    unclaimed: Unclaimed,
}

impl KeyClaim {
    /// Handle one `keydown`. This is the whole body of a migrated page's `onkeydown`.
    ///
    /// `wanted_locally` is the page's own question about this stroke, asked in the same tick, so it
    /// never disagrees with what the user can see. A page with nothing to ask passes `|_| false`.
    ///
    /// An IME composing keypress returns before any of that: it belongs to the input method, which
    /// will deliver the finished character itself, and `prevent_default` here would take it away.
    pub fn on_keydown(
        &self,
        event: &Event<KeyboardData>,
        wanted_locally: impl FnOnce(&KeyStroke) -> bool,
    ) {
        let data = event.data();
        let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
            return;
        };
        let Some(stroke) = WebKey::new(raw).stroke() else {
            return;
        };
        if stroke.is_modifier_key() {
            return;
        }
        let verdict = KeyVerdict::of(
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
