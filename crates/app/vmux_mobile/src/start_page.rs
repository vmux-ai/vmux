//! Serving `vmux://start/` out of the world.
//!
//! [`vmux_start::roster`] keeps the launcher payload current and knows nothing about how a page is
//! reached. This is the other half: the id the page listens on, and the emit that carries the
//! payload there. Page transport lives here rather than in the page crate, so a crate that answers
//! a URL never has to learn how this app happens to deliver bytes.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_start::event::START_COMMAND_BAR_OPEN_EVENT;
use vmux_start::roster::{Launcher, LauncherProjection, StartRosterPlugin};

use crate::runtime::PageEmit;

/// The launcher: the model, and the delivery of what it produces.
pub struct StartPagePlugin;

impl Plugin for StartPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(StartRosterPlugin).add_systems(
            Update,
            emit_launcher
                .after(LauncherProjection)
                .run_if(resource_changed::<Launcher>),
        );
    }
}

/// Push a rebuilt launcher to the page, if one is listening.
///
/// Ordered after the projection so a roster that arrives this turn reaches the page this turn.
fn emit_launcher(launcher: Res<Launcher>, mut emits: MessageWriter<PageEmit>) {
    let Some(bytes) = crate::page_host::encode(&launcher.0) else {
        return;
    };
    emits.write(PageEmit {
        id: START_COMMAND_BAR_OPEN_EVENT,
        bytes,
    });
}
