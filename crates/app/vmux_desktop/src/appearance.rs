use bevy::prelude::*;
use vmux_setting::{ResolvedScheme, SystemAppearance};

pub(crate) struct DesktopAppearancePlugin;

impl Plugin for DesktopAppearancePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, seed_system_appearance);
    }
}

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

fn seed_system_appearance(
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
