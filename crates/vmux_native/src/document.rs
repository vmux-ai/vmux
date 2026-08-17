//! The document a natively-hosted page finds when it reaches for one, and what it refuses.
//!
//! A page with no document at all is answered by `dioxus_document`'s no-op, which logs an error on
//! every mount and leaves the page unable to tell that anything was declined. Providing this one
//! turns that into a deliberate answer.
//!
//! **It runs no script.** `eval` is a hole straight through the typed capabilities a page reaches
//! the host through — focus, selection, caret — and once a page can hand the host a string of
//! JavaScript, nothing about what a page may do to its document is expressed in a type any more.
//! So it is refused, loudly, with the caller's script in the log so the capability it wanted can be
//! added properly.
//!
//! What a page needs from its document that is *not* arbitrary script — a title, an element in the
//! head — has no route here yet either, because both are written by evaluating one. Each is a
//! capability to add to [`PageHost`](vmux_ui::transport::PageHost) when a page asks for it.

use std::rc::Rc;
use std::task::{Context, Poll};

use dioxus_document::{
    Document, Eval, EvalError, Evaluator, LinkProps, MetaProps, ScriptProps, StyleProps,
};
use tracing::warn;

pub(crate) struct SurfaceDocument;

impl SurfaceDocument {
    pub(crate) fn of() -> Rc<dyn Document> {
        Rc::new(Self)
    }
}

impl Document for SurfaceDocument {
    fn eval(&self, js: String) -> Eval {
        warn!("vmux_native: a page asked to evaluate script, which a native page may not do: {js}");

        // The owner is dropped here, as it is in the no-op document upstream: nothing awaits an
        // evaluator that can only answer `Unsupported`.
        let owner = generational_box::Owner::default();
        Eval::new(owner.insert(Box::new(RefusedEvaluator)))
    }

    /// The defaults for these are written in terms of [`Self::eval`], so each has to be answered
    /// here or a refusal would be reported as a script the page tried to run.
    fn set_title(&self, _title: String) {}

    fn create_head_element(
        &self,
        _name: &str,
        _attributes: &[(&str, String)],
        _contents: Option<String>,
    ) {
    }

    fn create_meta(&self, _props: MetaProps) {}

    fn create_script(&self, _props: ScriptProps) {}

    fn create_style(&self, _props: StyleProps) {}

    fn create_link(&self, _props: LinkProps) {}
}

/// Answers for a script that was never run: there is no reply, and no channel to carry one.
struct RefusedEvaluator;

impl Evaluator for RefusedEvaluator {
    fn send(&self, _data: serde_json::Value) -> Result<(), EvalError> {
        Err(EvalError::Unsupported)
    }

    fn poll_recv(
        &mut self,
        _context: &mut Context<'_>,
    ) -> Poll<Result<serde_json::Value, EvalError>> {
        Poll::Ready(Err(EvalError::Unsupported))
    }

    fn poll_join(
        &mut self,
        _context: &mut Context<'_>,
    ) -> Poll<Result<serde_json::Value, EvalError>> {
        Poll::Ready(Err(EvalError::Unsupported))
    }
}
