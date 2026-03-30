//! Hosted web UIs: static assets served over loopback and bound to a CEF surface (see [`VmuxWebviewSurface`]).
//!
//! ## Plugin order
//!
//! Implementations register embedded HTTP against `VmuxServerShutdownRegistry` after `VmuxServerPlugin`
//! (from the `vmux_server` crate; add `VmuxServerPlugin` before the hosted plugin).

use bevy::prelude::*;

/// Which vmux CEF surface a hosted UI targets.
///
/// Type-level label for [`VmuxHostedWebPlugin`]; entities still use [`Pane`](crate::Pane),
/// [`PaneChromeStrip`](crate::PaneChromeStrip), etc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VmuxWebviewSurface {
    /// Primary document webview for each layout pane.
    MainPane,
    /// Bottom status / chrome strip ([`PaneChromeStrip`](crate::PaneChromeStrip)).
    PaneChrome,
}

/// A Bevy [`Plugin`] that serves a web app from loopback and wires it into a [`VmuxWebviewSurface`].
pub trait VmuxHostedWebPlugin: Plugin {
    const SURFACE: VmuxWebviewSurface;
}
