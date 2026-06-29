use bevy::prelude::*;
use vmux_setting::{ResolvedScheme, SystemAppearance};

#[cfg(target_os = "macos")]
fn read_system_appearance() -> Option<ResolvedScheme> {
    use objc2_app_kit::NSApp;

    let mtm = objc2::MainThreadMarker::new()?;
    let app = NSApp(mtm);
    let name = app.effectiveAppearance().name();
    if name.to_string().contains("Dark") {
        Some(ResolvedScheme::Dark)
    } else {
        Some(ResolvedScheme::Light)
    }
}

#[cfg(not(target_os = "macos"))]
fn read_system_appearance() -> Option<ResolvedScheme> {
    None
}

/// Seed [`SystemAppearance`] once at startup so Device mode resolves correctly on
/// the first frame; winit only reports theme *changes* afterward.
///
/// Takes [`NonSendMarker`] so Bevy pins it to the main thread — `read_system_appearance`
/// needs `MainThreadMarker`, which is `None` on a worker thread.
pub(crate) fn seed_system_appearance(
    _non_send: bevy::ecs::system::NonSendMarker,
    mut system: ResMut<SystemAppearance>,
) {
    if system.0.is_some() {
        return;
    }
    if let Some(scheme) = read_system_appearance() {
        system.0 = Some(scheme);
    }
}
