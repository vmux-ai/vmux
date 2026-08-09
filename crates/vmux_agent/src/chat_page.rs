//! The `vmux://agent` chat page: a native Dioxus UI that renders an agent session's
//! conversation + run-state (pushed from ECS) and sends prompt/approval intents back.
//! This is the single agent front-end; it replaced the legacy CLI-install setup page.

pub mod event;

#[cfg(any(test, frontend))]
pub(crate) mod approval;
#[cfg(any(test, frontend))]
pub(crate) mod composer;

#[cfg(frontend)]
pub mod page;
#[cfg(frontend)]
mod scroll;
#[cfg(frontend)]
mod state;

#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub use plugin::*;
