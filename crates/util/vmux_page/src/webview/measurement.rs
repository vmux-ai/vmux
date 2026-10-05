use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use dioxus_html::{MountedError, MountedResult};

use crate::webview::embed::{LifecycleSink, LifecycleUpdate};

pub(crate) type Measured = [f64; 4];

#[derive(Clone)]
pub(crate) struct PendingReads {
    slots: Rc<RefCell<HashMap<u64, Slot>>>,
    last_token: Rc<Cell<u64>>,
    lifecycle: Rc<dyn LifecycleSink>,
}

enum Slot {
    Asked(Option<Waker>),
    Answered(Option<Measured>),
}

impl PendingReads {
    pub(crate) fn new(lifecycle: Rc<dyn LifecycleSink>) -> Self {
        Self {
            slots: Rc::default(),
            last_token: Rc::default(),
            lifecycle,
        }
    }

    pub(crate) fn clear(&self) {
        if self.slots.borrow().is_empty() {
            return;
        }
        self.slots.borrow_mut().clear();
        self.lifecycle.update(LifecycleUpdate::MeasurementsCleared);
    }

    pub(crate) fn ask(&self) -> Measurement {
        let token = self.last_token.get().wrapping_add(1);
        self.last_token.set(token);
        self.slots.borrow_mut().insert(token, Slot::Asked(None));
        self.lifecycle.update(LifecycleUpdate::MeasurementStarted);

        Measurement {
            token,
            reads: self.clone(),
        }
    }

    pub(crate) fn answer(&self, token: u64, measured: Option<Measured>) {
        let mut slots = self.slots.borrow_mut();
        let Some(slot) = slots.get_mut(&token) else {
            return;
        };
        let asked = std::mem::replace(slot, Slot::Answered(measured));
        drop(slots);

        if let Slot::Asked(Some(waker)) = asked {
            waker.wake();
        }
    }
}

pub(crate) struct Measurement {
    token: u64,
    reads: PendingReads,
}

impl Measurement {
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
                self.reads
                    .lifecycle
                    .update(LifecycleUpdate::MeasurementFinished);

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
    fn drop(&mut self) {
        if self.reads.slots.borrow_mut().remove(&self.token).is_some() {
            self.reads
                .lifecycle
                .update(LifecycleUpdate::MeasurementFinished);
        }
    }
}

struct IgnoreLifecycle;

impl LifecycleSink for IgnoreLifecycle {
    fn update(&self, _update: LifecycleUpdate) {}
}

impl Default for PendingReads {
    fn default() -> Self {
        Self::new(Rc::new(IgnoreLifecycle))
    }
}

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
        fn poll_once(&mut self) -> Poll<MountedResult<Measured>> {
            Pin::new(self).poll(&mut Context::from_waker(Waker::noop()))
        }
    }

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
