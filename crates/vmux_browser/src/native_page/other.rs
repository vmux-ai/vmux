//! Nothing hosts a native page off macOS.
//!
//! `layout_cef_bundle` dropped its `Browser` on every platform, and only macOS has a replacement,
//! so a build without one has no chrome at all. Extending wry here is plausible — WebKitGTK is its
//! Linux backend — but the transparency the layout needs is a macOS question, and nobody is running
//! the desktop app there today.

use bevy::prelude::*;

pub(super) struct NativePagesOtherPlugin;

impl Plugin for NativePagesOtherPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, report_missing_renderer);
    }
}

/// Say it once rather than leaving a blank window to be diagnosed.
fn report_missing_renderer() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        warn!("native_page: no renderer on this platform, native pages will be missing");
    }
}
