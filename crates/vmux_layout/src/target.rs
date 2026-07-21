use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_core::terminal::{ProcessExited, Terminal};

use crate::pane::{Pane, PaneSplit};
use crate::stack::Stack;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTarget {
    Pane(Entity),
    Stack(Entity),
}

#[allow(clippy::type_complexity)]
pub fn parse_pane_target(
    s: &str,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<Entity> {
    let bits = match vmux_wire::protocol::parse_id(s) {
        Ok((vmux_wire::protocol::NodeKind::Pane, bits)) => bits,
        Ok(_) => return None,
        Err(_) => s.parse::<u64>().ok()?,
    };
    let entity = Entity::try_from_bits(bits)?;
    panes.contains(entity).then_some(entity)
}

pub fn parse_browser_target(
    value: &str,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: &Query<Entity, With<Stack>>,
) -> Option<BrowserTarget> {
    if let Ok((kind, bits)) = vmux_wire::protocol::parse_id(value) {
        let entity = Entity::try_from_bits(bits)?;
        return match kind {
            vmux_wire::protocol::NodeKind::Pane if panes.contains(entity) => {
                Some(BrowserTarget::Pane(entity))
            }
            vmux_wire::protocol::NodeKind::Stack if stacks.contains(entity) => {
                Some(BrowserTarget::Stack(entity))
            }
            _ => None,
        };
    }
    parse_pane_target(value, panes).map(BrowserTarget::Pane)
}

pub fn webview_for_target<B: Component>(
    target: BrowserTarget,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &vmux_core::LastActivatedAt), With<Stack>>,
    browsers: &Query<(Entity, &ChildOf), With<B>>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
) -> Option<Entity> {
    let stack = match target {
        BrowserTarget::Pane(pane) => {
            crate::stack::active_stack_in_pane(pane, pane_children, stack_ts)
        }
        BrowserTarget::Stack(stack) => Some(stack),
    };
    active_webview_for_tab(stack, browsers, terminals)
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
