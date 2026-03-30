//! Entry points named after [tmux(1)](https://man.openbsd.org/tmux.1) commands — aliases for the `try_*` helpers in [`crate::pane_ops`].
//!
//! | tmux command | Rust |
//! |--------------|------|
//! | [`select-pane`](https://man.openbsd.org/tmux.1#select-pane) (`-L`/`-R`/`-U`/`-D`) | [`select_pane`] |
//! | [`swap-pane`](https://man.openbsd.org/tmux.1#swap-pane) | [`swap_pane`] |
//! | [`split-window`](https://man.openbsd.org/tmux.1#split-window) | [`split_window`] |
//! | [`kill-pane`](https://man.openbsd.org/tmux.1#kill-pane) | [`kill_pane`] |
//! | [`rotate-window`](https://man.openbsd.org/tmux.1#rotate-window) | [`rotate_window`] |
//! | [`resize-pane -Z`](https://man.openbsd.org/tmux.1#resize-pane) | [`resize_pane_zoom`] |
//! | next / previous pane (focus) | [`select_pane_next`] |

pub use crate::PaneSwapDir;
pub use crate::neighbor_pane_in_direction as select_pane_neighbor;

pub use crate::pane_ops::try_cycle_pane_focus as select_pane_next;
pub use crate::pane_ops::try_kill_active_pane as kill_pane;
pub use crate::pane_ops::try_mirror_pane_layout as mirror_window;
pub use crate::pane_ops::try_rotate_window as rotate_window;
pub use crate::pane_ops::try_select_pane_direction as select_pane;
pub use crate::pane_ops::try_split_active_pane as split_window;
pub use crate::pane_ops::try_swap_active_pane as swap_pane;
pub use crate::pane_ops::try_toggle_zoom_pane as resize_pane_zoom;
