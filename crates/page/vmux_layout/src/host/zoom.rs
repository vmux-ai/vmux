use bevy::{
    ecs::{lifecycle::HookContext, relationship::Relationship, world::DeferredWorld},
    prelude::*,
};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use crate::{
    pane::{FocusRequest, OpenRequest, Pane, PaneSplit, ToggleZoomRequest},
    stack::{ActiveTabParam, Stack, focused_stack},
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
                handle_zoom_command
                    .in_set(LayoutRequestSet::Prepare)
                    .before(ArrangementSet),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_zoom_visibility.before(LayoutSystems::Layout),
                    clear_zoom_on_pane_removal,
                ),
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
                let hidden = zoomed.hidden.clone();
                for entity in hidden {
                    if let Some(mut node) = world.get_mut::<Node>(entity) {
                        node.display = Display::Flex;
                    }
                }
            });
    }
}

fn clear_zoom_on_pane_removal(
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
            commands.entity(tab).remove::<Zoomed>();
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

fn handle_zoom_command(
    mut focus_requests: MessageReader<FocusRequest>,
    mut open_requests: MessageReader<OpenRequest>,
    mut toggle_requests: MessageReader<ToggleZoomRequest>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab: ActiveTabParam,
    children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    panes: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<(Entity, &LastActivatedAt), With<Stack>>,
    parents: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    zoomed: Query<(), With<Zoomed>>,
    mut commands: Commands,
) {
    let unzoom_only = focus_requests.read().count() > 0 || open_requests.read().count() > 0;
    let toggle_count = toggle_requests.read().count();
    if !unzoom_only && toggle_count == 0 {
        return;
    }
    let (_, active_pane, _) = focused_stack(
        active_tab.get(),
        &children,
        &leaf_panes,
        &panes,
        &pane_children,
        &stacks,
    );
    let Some(active_pane) = active_pane else {
        return;
    };
    let Some(tab) = tab_of(active_pane, &parents, &tabs) else {
        return;
    };

    if unzoom_only {
        if zoomed.get(tab).is_ok() {
            commands.entity(tab).remove::<Zoomed>();
        }
        return;
    }

    for _ in 0..toggle_count {
        if zoomed.get(tab).is_ok() {
            commands.entity(tab).remove::<Zoomed>();
            continue;
        }
        let hidden = siblings_to_hide(active_pane, tab, &parents, &children, &splits);
        if !hidden.is_empty() {
            commands.entity(tab).insert(Zoomed {
                leaf: active_pane,
                hidden,
            });
        }
    }
}

fn sync_zoom_visibility(zoomed: Query<&Zoomed, Added<Zoomed>>, mut nodes: Query<&mut Node>) {
    for zoomed in &zoomed {
        for &entity in &zoomed.hidden {
            if let Ok(mut node) = nodes.get_mut(entity) {
                node.display = Display::None;
            }
        }
    }
}
