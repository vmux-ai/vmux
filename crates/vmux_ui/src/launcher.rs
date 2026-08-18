//! The launcher surface: ranking results, rendering a row, and the keys that move between them.
//!
//! Two things draw it — the `vmux://start/` page and the command bar's palette — and they are the
//! same surface, so it lives here rather than in either. It sat in `vmux_start` while the start
//! page was its only owner; the palette reaching across for it made `vmux_command` depend on
//! `vmux_start`, which is the edge that stopped the start page from ever moving to the crate that
//! answers its url.
//!
//! Nothing here knows which of the two is drawing it, or what a result does when chosen. Ranking
//! takes rows and a query; a row takes what to show and a callback.

pub mod keyboard;
pub mod results;
pub mod row;
pub mod style;
