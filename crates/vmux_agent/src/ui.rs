//! The webview half: what the pages render.
//!
//! Gated once here rather than per module, so what ships to wasm and iOS is what this module
//! names and nothing else. Neither page's source sits under this module — the agents manager is
//! `crate::agents_page`, the chat page is `vmux_chat::ui` — so `ui` is the namespace that names
//! both rather than a directory wrapping one of them.

/// The agents manager page. Re-exported so that `vmux_agent::ui::agents::Page` keeps naming the
/// same component as when this module owned the file.
pub use crate::agents_page as agents;

/// The chat page, which lives in `vmux_chat` — a conversation is not owned by the agent driving
/// it, and the group chat it is becoming will have no single agent to belong to. Re-exported so
/// that `vmux_agent::ui::chat::Page` keeps naming the same component as before the move.
pub use vmux_chat::ui as chat;
