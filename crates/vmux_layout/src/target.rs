use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_core::terminal::{ProcessExited, Terminal};

use crate::pane::{Pane, PaneSplit};

#[allow(clippy::type_complexity)]
pub fn parse_pane_target(
    s: &str,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<Entity> {
    let bits = s.parse::<u64>().ok()?;
    let entity = Entity::try_from_bits(bits)?;
    panes.contains(entity).then_some(entity)
}

#[allow(clippy::type_complexity)]
pub fn active_webview_for_tab<B: Component>(
    tab: Option<Entity>,
    browsers: &Query<(Entity, &ChildOf), With<B>>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let tab = tab?;
    browsers.iter().find_map(|(entity, child_of)| {
        if child_of.get() != tab {
            return None;
        }
        if terminals.iter().any(|(t, _)| t == entity) {
            return None;
        }
        Some(entity)
    })
}
