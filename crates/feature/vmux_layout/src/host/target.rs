use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::terminal::{ProcessExited, Terminal};

use crate::cef::Browser;
use crate::hosted_page::StackSurface;
use crate::pane::{Pane, PaneSplit};
use crate::presentation::Retiring;
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
pub struct BrowserTargets<'w, 's> {
    panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<'w, 's, Entity, (With<Stack>, Without<Retiring>)>,
    focus: LayoutFocus<'w, 's>,
    surfaces: StackSurface<'w, 's>,
    blocked: Query<
        'w,
        's,
        (),
        Or<(
            With<crate::hosted_page::SurfaceBlocked>,
            With<vmux_ecs::PageTransition>,
        )>,
    >,
    stack_activity: Query<
        'w,
        's,
        (Entity, &'static vmux_ecs::LastActivatedAt),
        (With<Stack>, Without<Retiring>),
    >,
    browsers: Query<'w, 's, (Entity, &'static ChildOf), With<Browser>>,
    terminals: Query<'w, 's, (Entity, &'static ChildOf), (With<Terminal>, Without<ProcessExited>)>,
}

impl BrowserTargets<'_, '_> {
    pub fn contains_pane(&self, pane: Entity) -> bool {
        self.panes.contains(pane)
    }

    pub fn contains_webview(&self, webview: Entity) -> bool {
        self.browsers.contains(webview)
    }

    pub fn is_settled_webview(&self, webview: Entity) -> bool {
        let Ok((_, parent)) = self.browsers.get(webview) else {
            return false;
        };
        !self.blocked.contains(webview) && self.surfaces.is_settled(parent.parent(), webview)
    }

    pub fn pane(&self, value: &str) -> Option<Entity> {
        let bits = match vmux_api::protocol::NodeKind::parse_id(value) {
            Ok((vmux_api::protocol::NodeKind::Pane, bits)) => bits,
            Ok(_) => return None,
            Err(_) => value.parse::<u64>().ok()?,
        };
        let entity = Entity::try_from_bits(bits)?;
        self.panes.contains(entity).then_some(entity)
    }

    pub fn target(&self, value: &str) -> Option<BrowserTarget> {
        if let Ok((kind, bits)) = vmux_api::protocol::NodeKind::parse_id(value) {
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
        let entity = self.surfaces.current(stack)?;
        (!self.terminals.contains(entity)).then_some(entity)
    }

    pub fn most_recent_webview(&self) -> Option<Entity> {
        self.stack_activity
            .iter()
            .filter_map(|(stack, timestamp)| {
                let entity = self.surfaces.current(stack)?;
                if self.terminals.contains(entity) {
                    return None;
                }
                Some((entity, timestamp.0))
            })
            .max_by_key(|&(_, timestamp)| timestamp)
            .map(|(entity, _)| entity)
    }
}
