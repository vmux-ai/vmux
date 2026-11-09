use dioxus::prelude::*;
use std::rc::Rc;

pub type Container = Signal<Option<Rc<MountedData>>>;

pub(super) struct Observation {
    pub top: i32,
    pub previous: i32,
    pub distance: i32,
    pub initial: bool,
}

impl Observation {
    pub fn pauses_following(&self) -> bool {
        !self.initial && self.distance > 0 && self.top < self.previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ChatItem, ChatSnapshot, ChatTranscriptState, ChatViewportState};
    use crate::state::ChatUiState;
    use std::cell::RefCell;

    #[component]
    fn InitialTranscript(
        probe: Rc<RefCell<Option<(Signal<ChatUiState>, super::super::state::Transcript)>>>,
    ) -> Element {
        let root = use_signal(|| ChatUiState {
            snapshot: ChatSnapshot {
                transcript_loading: true,
                ..Default::default()
            },
            transcript: ChatTranscriptState {
                items: vec![ChatItem::user("cached message")],
                ..Default::default()
            },
            transcript_revision: 1,
            viewport: ChatViewportState {
                session: "session".to_string(),
                position: None,
            },
            ..Default::default()
        });
        let transcript = super::super::state::use_transcript(root, "session".to_string());
        *probe.borrow_mut() = Some((root, transcript));
        rsx! { div {} }
    }

    #[test]
    fn placeholder_scroll_does_not_finish_initial_follow_before_messages_are_visible() {
        let probe = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            InitialTranscript,
            InitialTranscriptProps {
                probe: probe.clone(),
            },
        );
        dom.rebuild_to_vec();
        let (mut root, transcript) = probe.borrow().unwrap();
        dom.runtime().in_scope(ScopeId::ROOT, || {
            assert!(transcript.ready());
            transcript.observe_scroll(0, 0, 800);
            assert!(*transcript.initial_follow.peek());
            root.write().snapshot.transcript_loading = false;
            assert_eq!(transcript.initial_scroll(), "bottom");
            transcript.observe_scroll(0, 0, 0);
            assert!(*transcript.initial_follow.peek());
            transcript.observe_scroll(500, 400, 800);
            assert!(*transcript.initial_follow.peek());
            transcript.observe_scroll(900, 0, 800);
            assert!(*transcript.initial_follow.peek());
            assert!(*transcript.at_bottom.peek());
            transcript.note_scroll_input();
            assert!(!*transcript.initial_follow.peek());
            assert!(*transcript.at_bottom.peek());
        });
    }

    #[test]
    fn input_at_bottom_keeps_following_until_scrolled_away_and_resumes_on_return() {
        let probe = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            InitialTranscript,
            InitialTranscriptProps {
                probe: probe.clone(),
            },
        );
        dom.rebuild_to_vec();
        let (mut root, transcript) = probe.borrow().unwrap();
        dom.runtime().in_scope(ScopeId::ROOT, || {
            root.write().snapshot.transcript_loading = false;
            transcript.observe_scroll(900, 0, 800);
            transcript.note_scroll_input();
            assert!(*transcript.at_bottom.peek());
            transcript.observe_scroll(895, 5, 800);
            assert!(!*transcript.at_bottom.peek());
            transcript.observe_scroll(900, 0, 800);
            assert!(*transcript.at_bottom.peek());
            transcript.note_scroll_input();
            assert!(*transcript.at_bottom.peek());
        });
    }

    #[test]
    fn small_upward_scroll_stops_following_until_the_actual_bottom() {
        let moved = Observation {
            top: 995,
            previous: 1000,
            distance: 5,
            initial: false,
        };
        assert!(moved.pauses_following());
    }

    #[test]
    fn shrinking_content_at_the_bottom_does_not_cancel_following() {
        for initial in [true, false] {
            let observed = Observation {
                top: 8020,
                previous: 8193,
                distance: 0,
                initial,
            };
            assert!(!observed.pauses_following());
        }
    }

    #[test]
    fn initial_placement_does_not_become_a_manual_upward_scroll() {
        let initial = Observation {
            top: 8000,
            previous: 8193,
            distance: 400,
            initial: true,
        };
        assert!(!initial.pauses_following());
        let manual = Observation {
            top: 8000,
            previous: 8193,
            distance: 400,
            initial: false,
        };
        assert!(manual.pauses_following());
    }
}
