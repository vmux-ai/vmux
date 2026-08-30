use dioxus::prelude::*;

use crate::platform::now_millis;

pub fn use_ime_guard() -> ImeGuard {
    ImeGuard {
        composition: use_signal(Composition::default),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ImeGuard {
    composition: Signal<Composition>,
}

impl ImeGuard {
    pub fn active(self) -> bool {
        (self.composition)().active
    }

    pub fn start(mut self) {
        let started = self.composition.peek().started();
        self.composition.set(started);
    }

    pub fn commit(mut self) {
        let committed = self.composition.peek().committed(now_millis());
        self.composition.set(committed);
    }

    pub fn swallows(mut self, event: &Event<KeyboardData>) -> bool {
        let data = event.data();
        let (next, verdict) =
            self.composition
                .peek()
                .saw_key(&data.key(), data.is_composing(), now_millis());
        if *self.composition.peek() != next {
            self.composition.set(next);
        }
        match verdict {
            ImeVerdict::Editor => false,
            ImeVerdict::Composing => true,
            ImeVerdict::Committed => {
                event.prevent_default();
                true
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ImeVerdict {
    Editor,
    Composing,
    Committed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Composition {
    active: bool,
    committed_at: Option<i64>,
}

impl Composition {
    const COMMIT_GRACE_MS: i64 = 60;

    fn started(self) -> Self {
        Self {
            active: true,
            committed_at: None,
        }
    }

    fn committed(self, at: i64) -> Self {
        Self {
            active: false,
            committed_at: Some(at),
        }
    }

    fn saw_key(self, key: &Key, composing: bool, at: i64) -> (Self, ImeVerdict) {
        if composing {
            return (self.started(), ImeVerdict::Composing);
        }
        let candidate = Self::candidate_window_key(key);
        if self.active {
            let verdict = match candidate {
                true => ImeVerdict::Committed,
                false => ImeVerdict::Editor,
            };
            return (Self::default(), verdict);
        }
        let Some(committed_at) = self.committed_at else {
            return (self, ImeVerdict::Editor);
        };
        if candidate && at.saturating_sub(committed_at) <= Self::COMMIT_GRACE_MS {
            return (Self::default(), ImeVerdict::Committed);
        }
        (Self::default(), ImeVerdict::Editor)
    }

    fn candidate_window_key(key: &Key) -> bool {
        if let Key::Character(text) = key {
            return text == " ";
        }
        matches!(
            key,
            Key::Enter
                | Key::Escape
                | Key::Tab
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::Accept
                | Key::Convert
                | Key::NonConvert
                | Key::Process
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after_commit(at: i64) -> Composition {
        Composition::default().started().committed(at)
    }

    #[test]
    fn the_enter_that_confirms_a_conversion_never_reaches_the_editor() {
        let composed = after_commit(1_000);

        assert_eq!(
            composed.saw_key(&Key::Enter, false, 1_010).1,
            ImeVerdict::Committed,
            "macOS reports compositionend before the Enter that caused it, so the key arrives \
             claiming no composition is in flight and would otherwise break the line"
        );
        assert_eq!(
            Composition::default()
                .started()
                .saw_key(&Key::Enter, true, 1_000)
                .1,
            ImeVerdict::Composing
        );
    }

    #[test]
    fn one_commit_absorbs_one_key_and_no_more() {
        let (next, verdict) = after_commit(1_000).saw_key(&Key::Enter, false, 1_005);

        assert_eq!(verdict, ImeVerdict::Committed);
        assert_eq!(
            next.saw_key(&Key::Enter, false, 1_006).1,
            ImeVerdict::Editor,
            "a reader who commits a conversion and then wants a new line presses Enter twice"
        );
    }

    #[test]
    fn a_key_the_candidate_window_never_took_goes_straight_through() {
        let composed = after_commit(1_000);

        assert_eq!(
            composed
                .saw_key(&Key::Character("a".to_string()), false, 1_005)
                .1,
            ImeVerdict::Editor
        );
        assert_eq!(
            composed.saw_key(&Key::Backspace, false, 1_005).1,
            ImeVerdict::Editor
        );
        assert_eq!(
            composed
                .saw_key(&Key::Character(" ".to_string()), false, 1_005)
                .1,
            ImeVerdict::Committed,
            "space walks the candidate list on every Japanese IME"
        );
    }

    #[test]
    fn a_key_arriving_after_the_grace_window_is_the_readers_own() {
        assert_eq!(
            after_commit(1_000)
                .saw_key(&Key::Enter, false, 1_000 + Composition::COMMIT_GRACE_MS + 1)
                .1,
            ImeVerdict::Editor
        );
    }

    #[test]
    fn a_composition_that_never_ends_clears_on_the_next_ordinary_key() {
        let stuck = Composition::default().started();
        let (next, verdict) = stuck.saw_key(&Key::Character("a".to_string()), false, 1_000);

        assert_eq!(
            verdict,
            ImeVerdict::Editor,
            "a missed compositionend must not swallow ordinary typing forever"
        );
        assert_eq!(next, Composition::default());
        assert_eq!(
            next.saw_key(&Key::Enter, false, 1_001).1,
            ImeVerdict::Editor
        );
    }

    #[test]
    fn a_key_seen_while_composing_repairs_a_missed_start() {
        let (next, verdict) = Composition::default().saw_key(&Key::ArrowDown, true, 1_000);

        assert_eq!(verdict, ImeVerdict::Composing);
        assert!(next.active);
    }

    #[test]
    fn a_fresh_page_routes_every_key_to_the_editor() {
        for key in [Key::Enter, Key::Escape, Key::Tab, Key::ArrowUp] {
            assert_eq!(
                Composition::default().saw_key(&key, false, 1_000).1,
                ImeVerdict::Editor
            );
        }
    }
}
