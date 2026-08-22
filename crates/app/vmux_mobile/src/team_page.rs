//! Serving `vmux://team/` out of the world.
//!
//! The same split as [`start_page`](crate::start_page): [`vmux_team::roster`] keeps the payload
//! current and knows nothing about how a page is reached, and the id it is delivered under lives
//! here, in the app that owns the pages.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_team::roster::{Team, TeamProjection, TeamRosterPlugin};
use vmux_wire::team::TEAM_EVENT;

use crate::runtime::PageEmit;

/// The team roster, model and delivery both.
pub struct TeamPagePlugin;

impl Plugin for TeamPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TeamRosterPlugin).add_systems(
            Update,
            emit_team
                .after(TeamProjection)
                .run_if(resource_changed::<Team>),
        );
    }
}

/// Push a rebuilt roster to the page, if one is listening.
fn emit_team(team: Res<Team>, mut emits: MessageWriter<PageEmit>) {
    let Some(bytes) = crate::page_host::encode(&team.0) else {
        return;
    };
    emits.write(PageEmit {
        id: TEAM_EVENT,
        bytes,
    });
}
