//! A question a mounted component asked about its element, waiting for the page to answer it.
//!
//! The instructions in [`Element`](crate::webview::element::Element) need no reply and
//! resolve the moment they are queued. These need one, and it comes back over `window.ipc` rather
//! than on a request. Unlike the caret in [`event_selection`](crate::webview::event_selection), a
//! `RenderedElementBacking` read is a future with no deadline to beat — so the reply owes nothing
//! to any particular request, and IPC is both prompter than waiting for the page to ask for its
//! next frame and free of what a header may hold.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use dioxus_html::{MountedError, MountedResult};

/// What a measurement answers with.
///
/// Four numbers whichever question was asked, because the wire is uniform and the future that asked
/// knows how to read them back: a rect takes all four, a size and an offset the first two.
pub(crate) type Measured = [f64; 4];

/// The questions in flight, by token.
#[derive(Clone, Default)]
pub(crate) struct PendingReads {
    slots: Rc<RefCell<HashMap<u64, Slot>>>,
    last_token: Rc<Cell<u64>>,
}

enum Slot {
    /// Nobody has answered. The waker is the asking task's, left on the first poll.
    Asked(Option<Waker>),
    /// `None` means the page looked and found no element.
    Answered(Option<Measured>),
}

impl PendingReads {
    /// Open a slot, and hand back the future that closes it.
    pub(crate) fn ask(&self) -> Measurement {
        let token = self.last_token.get().wrapping_add(1);
        self.last_token.set(token);
        self.slots.borrow_mut().insert(token, Slot::Asked(None));

        Measurement {
            token,
            reads: self.clone(),
        }
    }

    /// The page measured. `None` means the node was gone by the time it looked.
    pub(crate) fn answer(&self, token: u64, measured: Option<Measured>) {
        let mut slots = self.slots.borrow_mut();
        let Some(slot) = slots.get_mut(&token) else {
            return;
        };
        let asked = std::mem::replace(slot, Slot::Answered(measured));
        // Before waking, because a woken task may poll straight back into the map.
        drop(slots);

        if let Slot::Asked(Some(waker)) = asked {
            waker.wake();
        }
    }
}

/// One question, resolved when the page answers it.
pub(crate) struct Measurement {
    token: u64,
    reads: PendingReads,
}

impl Measurement {
    /// Travels with the request, and comes back with the answer.
    pub(crate) fn token(&self) -> u64 {
        self.token
    }
}

impl Future for Measurement {
    type Output = MountedResult<Measured>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slots = self.reads.slots.borrow_mut();
        match slots.get_mut(&self.token) {
            Some(Slot::Answered(measured)) => {
                let measured = measured.take();
                slots.remove(&self.token);

                Poll::Ready(measured.ok_or_else(|| MountedError::OperationFailed(Box::new(Gone))))
            }
            Some(slot) => {
                *slot = Slot::Asked(Some(context.waker().clone()));

                Poll::Pending
            }
            None => Poll::Ready(Err(MountedError::OperationFailed(Box::new(Gone)))),
        }
    }
}

impl Drop for Measurement {
    /// A component that unmounts while waiting takes its task with it, and the slot would be left
    /// for nobody. Pages come and go with panes, so the map has to shrink as well as grow.
    fn drop(&mut self) {
        self.reads.slots.borrow_mut().remove(&self.token);
    }
}

/// The element was gone by the time the page looked at it.
#[derive(Debug)]
struct Gone;

impl std::fmt::Display for Gone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the element was gone before the page could measure it")
    }
}

impl std::error::Error for Gone {}

#[cfg(test)]
mod tests {
    use super::*;

    impl Measurement {
        /// Poll once against a waker that records nothing, which is all a resolved future needs.
        fn poll_once(&mut self) -> Poll<MountedResult<Measured>> {
            Pin::new(self).poll(&mut Context::from_waker(Waker::noop()))
        }
    }

    /// The whole point of the token: two questions in flight at once must not take each other's
    /// answer, which is silent — a scroll height read as a client rect simply scrolls somewhere
    /// plausible and wrong.
    #[test]
    fn an_answer_resolves_the_question_that_carried_its_token() {
        let reads = PendingReads::default();
        let mut first = reads.ask();
        let mut second = reads.ask();

        reads.answer(second.token(), Some([1.0, 2.0, 3.0, 4.0]));

        assert!(matches!(first.poll_once(), Poll::Pending));
        assert!(matches!(
            second.poll_once(),
            Poll::Ready(Ok([1.0, 2.0, 3.0, 4.0]))
        ));
    }

    /// A slot left behind by every dropped or answered question is a leak that grows for as long as
    /// the page lives, and nothing else would ever notice.
    #[test]
    fn nothing_is_left_behind_once_a_question_is_dropped_or_resolved() {
        let reads = PendingReads::default();
        {
            let _abandoned = reads.ask();
        }
        let mut answered = reads.ask();
        reads.answer(answered.token(), Some([0.0; 4]));
        let _ = answered.poll_once();
        drop(answered);

        assert!(reads.slots.borrow().is_empty());
    }
}
