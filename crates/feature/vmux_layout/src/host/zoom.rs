use bevy::{
    ecs::{lifecycle::HookContext, relationship::Relationship, world::DeferredWorld},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use crate::{
    pane::{FocusRequest, OpenRequest, Pane, PaneSize, PaneSplit, ToggleZoomRequest},
    stack::{ActiveTabParam, LayoutFocus},
    tab::Tab,
};

use super::{command::LayoutRequestSet, pane::ArrangementSet};

pub(crate) struct PaneZoomPlugin;

impl Plugin for PaneZoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FocusRequest>()
            .add_message::<OpenRequest>()
            .add_message::<ToggleZoomRequest>()
            .add_systems(
                Update,
                handle_command
                    .in_set(LayoutRequestSet::Prepare)
                    .before(ArrangementSet),
            )
            .add_systems(
                PostUpdate,
                (clear_on_pane_removal, sync_visibility, animate_zoom)
                    .chain()
                    .before(LayoutSystems::Layout),
            );
        Zoomed::register_hooks(app);
    }
}

#[derive(Component, Debug)]
pub struct Zoomed {
    pub leaf: Entity,
    pub hidden: Vec<Entity>,
}

impl Zoomed {
    fn register_hooks(app: &mut App) {
        app.world_mut()
            .register_component_hooks::<Self>()
            .on_remove(|mut world: DeferredWorld, ctx: HookContext| {
                let Some(zoomed) = world.get::<Self>(ctx.entity) else {
                    return;
                };
                if world.get::<PaneZoomTransition>(ctx.entity).is_some() {
                    return;
                }
                let hidden = zoomed.hidden.clone();
                for entity in hidden {
                    let flex_grow = world
                        .get::<PaneSize>(entity)
                        .map(|size| size.flex_grow)
                        .unwrap_or(1.0);
                    if let Some(mut node) = world.get_mut::<Node>(entity) {
                        node.display = Display::Flex;
                        node.flex_grow = flex_grow;
                    }
                }
            });
    }
}

#[derive(Clone, Copy, Debug)]
struct PaneZoomWeight {
    entity: Entity,
    start: f32,
    end: f32,
}

#[derive(Clone, Copy, Debug)]
struct PaneZoomGap {
    entity: Entity,
    direction: crate::pane::PaneSplitDirection,
    start: f32,
    end: f32,
}

#[derive(Component, Debug)]
struct PaneZoomTransition {
    leaf: Entity,
    hidden: Vec<Entity>,
    weights: Vec<PaneZoomWeight>,
    gaps: Vec<PaneZoomGap>,
    target_zoomed: bool,
    elapsed: f32,
}

impl PaneZoomTransition {
    const DURATION: f32 = 0.24;

    fn progress(&mut self, delta: f32) -> f32 {
        self.elapsed = (self.elapsed + delta).min(Self::DURATION);
        let linear = self.elapsed / Self::DURATION;
        linear * linear * (3.0 - 2.0 * linear)
    }

    fn complete(&self) -> bool {
        self.elapsed >= Self::DURATION
    }

    fn value(start: f32, end: f32, progress: f32) -> f32 {
        start + (end - start) * progress
    }
}

fn clear_on_pane_removal(
    mut removed: RemovedComponents<Pane>,
    zoomed: Query<(Entity, &Zoomed)>,
    mut commands: Commands,
) {
    let removed: Vec<Entity> = removed.read().collect();
    if removed.is_empty() {
        return;
    }
    for (tab, zoomed) in &zoomed {
        if removed.contains(&zoomed.leaf) {
            commands
                .entity(tab)
                .remove::<(Zoomed, PaneZoomTransition)>();
        }
    }
}

fn tab_of(
    leaf: Entity,
    parents: &Query<&ChildOf>,
    tabs: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> Option<Entity> {
    let mut current = leaf;
    loop {
        if tabs.get(current).is_ok() {
            return Some(current);
        }
        current = parents.get(current).ok()?.get();
    }
}

fn siblings_to_hide(
    leaf: Entity,
    tab: Entity,
    parents: &Query<&ChildOf>,
    children: &Query<&Children>,
    splits: &Query<&PaneSplit>,
) -> Vec<Entity> {
    let mut hidden = Vec::new();
    let mut current = leaf;
    while current != tab {
        let Ok(parent) = parents.get(current).map(Relationship::get) else {
            break;
        };
        if splits.get(parent).is_ok()
            && let Ok(children) = children.get(parent)
        {
            for child in children.iter() {
                if child != current {
                    hidden.push(child);
                }
            }
        }
        current = parent;
    }
    hidden
}

fn handle_command(
    mut focus_requests: MessageReader<FocusRequest>,
    mut open_requests: MessageReader<OpenRequest>,
    mut toggle_requests: MessageReader<ToggleZoomRequest>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab: ActiveTabParam,
    children: Query<&Children>,
    focus: LayoutFocus,
    parents: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    zoomed: Query<&Zoomed>,
    transitions: Query<&PaneZoomTransition>,
    pane_sizes: Query<&PaneSize>,
    mut nodes: Query<&mut Node>,
    mut commands: Commands,
) {
    let unzoom_only = focus_requests.read().count() > 0 || open_requests.read().count() > 0;
    let toggle_count = toggle_requests.read().count();
    if !unzoom_only && toggle_count == 0 {
        return;
    }
    let (_, active_pane, _) = focus.resolve(active_tab.get());
    let Some(active_pane) = active_pane else {
        return;
    };
    let Some(tab) = tab_of(active_pane, &parents, &tabs) else {
        return;
    };

    let transition = transitions.get(tab).ok();
    let existing_zoom = zoomed.get(tab).ok();
    let current_target = transition
        .map(|transition| transition.target_zoomed)
        .unwrap_or(existing_zoom.is_some());
    let target_zoomed = if unzoom_only {
        false
    } else if toggle_count % 2 == 0 {
        current_target
    } else {
        !current_target
    };
    if target_zoomed == current_target && transition.is_none() {
        return;
    }
    let (leaf, hidden) = transition
        .map(|transition| (transition.leaf, transition.hidden.clone()))
        .or_else(|| {
            zoomed
                .get(tab)
                .ok()
                .map(|zoomed| (zoomed.leaf, zoomed.hidden.clone()))
        })
        .unwrap_or_else(|| {
            (
                active_pane,
                siblings_to_hide(active_pane, tab, &parents, &children, &splits),
            )
        });
    if hidden.is_empty() {
        return;
    }
    let transition = PaneZoomTransition::new(
        leaf,
        hidden.clone(),
        target_zoomed,
        &parents,
        &splits,
        &pane_sizes,
        &mut nodes,
    );
    let mut tab = commands.entity(tab);
    tab.insert(transition);
    if target_zoomed {
        tab.insert(Zoomed { leaf, hidden });
    } else {
        tab.remove::<Zoomed>();
    }
}

impl PaneZoomTransition {
    fn new(
        leaf: Entity,
        hidden: Vec<Entity>,
        target_zoomed: bool,
        parents: &Query<&ChildOf>,
        splits: &Query<&PaneSplit>,
        pane_sizes: &Query<&PaneSize>,
        nodes: &mut Query<&mut Node>,
    ) -> Self {
        let mut weights = Vec::with_capacity(hidden.len());
        let mut split_entities = Vec::new();
        for &entity in &hidden {
            let restore = pane_sizes
                .get(entity)
                .map(|size| size.flex_grow)
                .unwrap_or(1.0);
            if let Ok(mut node) = nodes.get_mut(entity) {
                node.display = Display::Flex;
                weights.push(PaneZoomWeight {
                    entity,
                    start: node.flex_grow,
                    end: if target_zoomed { 0.0 } else { restore },
                });
            }
            if let Ok(parent) = parents.get(entity).map(Relationship::get)
                && splits.contains(parent)
                && !split_entities.contains(&parent)
            {
                split_entities.push(parent);
            }
        }
        let mut gaps = Vec::with_capacity(split_entities.len());
        for entity in split_entities {
            let Ok(split) = splits.get(entity) else {
                continue;
            };
            let Ok(node) = nodes.get_mut(entity) else {
                continue;
            };
            let start = match split.direction {
                crate::pane::PaneSplitDirection::Row => match node.column_gap {
                    Val::Px(value) => value,
                    _ => crate::event::PANE_GAP_PX,
                },
                crate::pane::PaneSplitDirection::Column => match node.row_gap {
                    Val::Px(value) => value,
                    _ => crate::event::PANE_GAP_PX,
                },
            };
            gaps.push(PaneZoomGap {
                entity,
                direction: split.direction,
                start,
                end: if target_zoomed {
                    0.0
                } else {
                    crate::event::PANE_GAP_PX
                },
            });
        }
        Self {
            leaf,
            hidden,
            weights,
            gaps,
            target_zoomed,
            elapsed: 0.0,
        }
    }
}

fn sync_visibility(
    zoomed: Query<(Entity, &Zoomed), Added<Zoomed>>,
    transitions: Query<(), With<PaneZoomTransition>>,
    mut nodes: Query<&mut Node>,
) {
    for (tab, zoomed) in &zoomed {
        if transitions.contains(tab) {
            continue;
        }
        for &entity in &zoomed.hidden {
            if let Ok(mut node) = nodes.get_mut(entity) {
                node.flex_grow = 0.0;
                node.display = Display::None;
            }
        }
    }
}

fn animate_zoom(
    time: Res<Time>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut transitions: Query<(Entity, &mut PaneZoomTransition)>,
    mut nodes: Query<&mut Node>,
    mut commands: Commands,
) {
    for (tab, mut transition) in &mut transitions {
        let progress = transition.progress(time.delta_secs());
        for weight in &transition.weights {
            if let Ok(mut node) = nodes.get_mut(weight.entity) {
                node.display = Display::Flex;
                node.flex_grow = PaneZoomTransition::value(weight.start, weight.end, progress);
            }
        }
        for gap in &transition.gaps {
            let Ok(mut node) = nodes.get_mut(gap.entity) else {
                continue;
            };
            let value = Val::Px(PaneZoomTransition::value(gap.start, gap.end, progress));
            match gap.direction {
                crate::pane::PaneSplitDirection::Row => node.column_gap = value,
                crate::pane::PaneSplitDirection::Column => node.row_gap = value,
            }
        }
        if transition.complete() {
            if transition.target_zoomed {
                for entity in &transition.hidden {
                    if let Ok(mut node) = nodes.get_mut(*entity) {
                        node.flex_grow = 0.0;
                        node.display = Display::None;
                    }
                }
            }
            commands.entity(tab).remove::<PaneZoomTransition>();
        } else if let Some(proxy) = proxy.as_deref() {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }
    }
}
