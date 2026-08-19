//! Keeping the caret in the launcher input.
//!
//! Re-asserting the claim against a host that grants focus late is
//! [`vmux_ui::focus::FocusClaim`]'s job, not this module's. What is left here is the part that
//! genuinely is about the launcher: it is a page with nothing to interact with but one input and
//! a list of results, so a click anywhere else should not blur it, and the claim has to stop the
//! moment an agent page takes over the document.
//!
//! Split out so [`super::page::Page`] is just a page. Everything here is inert off the browser,
//! which is what lets the launcher render somewhere with no `window` to argue with.

/// The launcher's claim on the caret.
pub struct StartFocus;

impl StartFocus {
    /// Ask the host for the caret, which it gives by queueing a script against the field.
    pub fn request() {
        vmux_command::page::focus_prompt_input();
    }

    /// Inert. The web build installs document listeners that take the caret *back* from whatever
    /// the browser gave it to; nothing here takes it away in the first place.
    pub fn install() {}

    /// Inert, for the same reason as [`Self::install`]: there is no capture to release.
    pub fn release_for_agent_transition() {}

    pub fn claim_on_mount() {
        Self::request();
    }
}
