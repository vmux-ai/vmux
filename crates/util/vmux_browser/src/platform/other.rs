use bevy::prelude::*;

pub(super) struct FallbackBrowserPlugin;

impl Plugin for FallbackBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, report_missing_renderer);
    }
}

fn report_missing_renderer() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        warn!("browser_platform: no renderer on this platform, native pages will be missing");
    }
}
