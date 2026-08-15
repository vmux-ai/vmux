//! The command bar's wiring to the workspace.
//!
//! The bar itself is `vmux_command`; the layout is `vmux_layout`. This is the join: it reads
//! the workspace model to answer what the bar should offer, and turns what the user picked
//! into layout mutations. It lived in `vmux_layout` and made the layout the owner of a
//! surface it only hosts; it cannot live in `vmux_command` either, which would invert the
//! dependency into a cycle. Composition is this crate's job, so it belongs here.

pub mod handler;
pub mod key;
pub mod panel;
pub mod state;
pub mod work_snapshot;
