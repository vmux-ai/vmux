use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_core::terminal::{ProcessExited, Terminal};

#[allow(clippy::type_complexity)]
pub fn active_terminal_for_tab(
    tab: Option<Entity>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let tab = tab?;
    terminals
        .iter()
        .find_map(|(entity, child_of)| (child_of.get() == tab).then_some(entity))
}

#[allow(clippy::type_complexity)]
pub fn parse_terminal_target(
    s: &str,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let bits = s.parse::<u64>().ok()?;
    let entity = Entity::try_from_bits(bits)?;
    terminals.iter().any(|(e, _)| e == entity).then_some(entity)
}
