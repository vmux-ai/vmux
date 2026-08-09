//! Turning what the daemon sends into what a person reads.
//!
//! Separate from `ui` because none of it renders: no dioxus, and the tests run on a native
//! build where there is no webview at all.

pub mod approval;
pub mod composer;
