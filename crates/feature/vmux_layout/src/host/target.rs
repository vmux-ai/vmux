use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_core::terminal::{ProcessExited, Terminal};

use crate::pane::{Pane, PaneSplit};
use crate::stack::{LayoutFocus, Stack};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiblingDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTarget {
    Pane(Entity),
    Stack(Entity),
}

#[derive(SystemParam)]
pub struct BrowserTargets<'w, 's, B: Component> {
    panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<'w, 's, Entity, With<Stack>>,
    focus: LayoutFocus<'w, 's>,
    stack_activity: Query<'w, 's, (Entity, &'static vmux_core::LastActivatedAt), With<Stack>>,
    browsers: Query<'w, 's, (Entity, &'static ChildOf), With<B>>,
    terminals: Query<'w, 's, (Entity, &'static ChildOf), (With<Terminal>, Without<ProcessExited>)>,
}

impl<B: Component> BrowserTargets<'_, '_, B> {
    pub fn contains_pane(&self, pane: Entity) -> bool {
        self.panes.contains(pane)
    }

    pub fn contains_webview(&self, webview: Entity) -> bool {
        self.browsers.contains(webview)
    }

    pub fn pane(&self, value: &str) -> Option<Entity> {
        let bits = match vmux_api::protocol::parse_id(value) {
            Ok((vmux_api::protocol::NodeKind::Pane, bits)) => bits,
            Ok(_) => return None,
            Err(_) => value.parse::<u64>().ok()?,
        };
        let entity = Entity::try_from_bits(bits)?;
        self.panes.contains(entity).then_some(entity)
    }

    pub fn target(&self, value: &str) -> Option<BrowserTarget> {
        if let Ok((kind, bits)) = vmux_api::protocol::parse_id(value) {
            let entity = Entity::try_from_bits(bits)?;
            return match kind {
                vmux_api::protocol::NodeKind::Pane if self.panes.contains(entity) => {
                    Some(BrowserTarget::Pane(entity))
                }
                vmux_api::protocol::NodeKind::Stack if self.stacks.contains(entity) => {
                    Some(BrowserTarget::Stack(entity))
                }
                _ => None,
            };
        }
        self.pane(value).map(BrowserTarget::Pane)
    }

    pub fn active_stack(&self, pane: Entity) -> Option<Entity> {
        self.focus.stack(pane)
    }

    pub fn webview(&self, target: BrowserTarget) -> Option<Entity> {
        let stack = match target {
            BrowserTarget::Pane(pane) => self.active_stack(pane),
            BrowserTarget::Stack(stack) => Some(stack),
        };
        self.active_webview(stack)
    }

    pub fn active_webview(&self, stack: Option<Entity>) -> Option<Entity> {
        let stack = stack?;
        self.browsers.iter().find_map(|(entity, child_of)| {
            if child_of.get() != stack {
                return None;
            }
            if self
                .terminals
                .iter()
                .any(|(terminal, _)| terminal == entity)
            {
                return None;
            }
            Some(entity)
        })
    }

    pub fn most_recent_webview(&self) -> Option<Entity> {
        self.browsers
            .iter()
            .filter_map(|(entity, child_of)| {
                if self
                    .terminals
                    .iter()
                    .any(|(terminal, _)| terminal == entity)
                {
                    return None;
                }
                let (_, timestamp) = self.stack_activity.get(child_of.get()).ok()?;
                Some((entity, timestamp.0))
            })
            .max_by_key(|&(_, timestamp)| timestamp)
            .map(|(entity, _)| entity)
    }
}
