use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowPosition};
use bevy_cef::prelude::HostWindow;
use std::collections::HashMap;
use vmux_core::{Order, PageMetadata};
use vmux_history::LastActivatedAt;
use vmux_layout::Loading;
use vmux_layout::space::Space;
use vmux_layout::stack::{FocusedStack, Stack};
use vmux_layout::tab::Tab;

use super::model::{
    ExtensionIdSequence, ExtensionModel, ExtensionModelEvent, ExtensionTabId, ExtensionTabSnapshot,
    ExtensionWindowId, ExtensionWindowSnapshot, extension_visible_url,
};
use crate::extension::bridge_page::ExtensionBridgeWebview;

pub(crate) struct ExtensionProjectPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExtensionProjectionSet;

impl Plugin for ExtensionProjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExtensionModelEvent>()
            .add_systems(Startup, spawn_extension_model)
            .add_systems(
                Update,
                (
                    assign_extension_ids,
                    bevy::ecs::schedule::ApplyDeferred,
                    rebuild_extension_model,
                )
                    .chain()
                    .in_set(ExtensionProjectionSet)
                    .after(vmux_layout::LayoutCefStateSet::Apply)
                    .after(vmux_layout::stack::ComputeFocusSet),
            );
    }
}

type HierarchyData = (
    Option<&'static Children>,
    Option<&'static ChildOf>,
    Option<&'static Order>,
    Has<Tab>,
    Has<Stack>,
    Option<&'static HostWindow>,
    Has<Loading>,
);

type PageData = (
    Entity,
    &'static PageMetadata,
    &'static ExtensionTabId,
    Option<&'static LastActivatedAt>,
    Option<&'static vmux_core::PageIdentity>,
    Has<ExtensionBridgeWebview>,
    Has<Loading>,
);

struct WindowCandidate {
    entity: Entity,
    id: i32,
    primary: bool,
    focused: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

struct PageCandidate {
    entity: Entity,
    id: i32,
    host_window: Option<Entity>,
    activated_at: i64,
    url: String,
    title: String,
    status: String,
}

struct ProjectedTab {
    entity: Entity,
    activated_at: i64,
    tab: ExtensionTabSnapshot,
}

fn spawn_extension_model(mut commands: Commands) {
    commands.spawn((ExtensionModel::default(), ExtensionIdSequence::default()));
}

fn assign_extension_ids(
    windows: Query<Entity, (With<Window>, Without<ExtensionWindowId>)>,
    tabs: Query<Entity, (With<Stack>, With<PageMetadata>, Without<ExtensionTabId>)>,
    mut sequence: Single<&mut ExtensionIdSequence>,
    mut commands: Commands,
) {
    for entity in &windows {
        commands.entity(entity).insert(sequence.next_window());
    }
    for entity in &tabs {
        commands.entity(entity).insert(sequence.next_tab());
    }
}

fn rebuild_extension_model(
    window_query: Query<(Entity, &Window, &ExtensionWindowId, Has<PrimaryWindow>)>,
    space_query: Query<(Entity, Option<&Order>), With<Space>>,
    hierarchy: Query<HierarchyData>,
    page_query: Query<PageData>,
    focused_stack: FocusedStack,
    mut current: Single<&mut ExtensionModel>,
    mut events: MessageWriter<ExtensionModelEvent>,
) {
    let previous = (**current).clone();
    let focused_stack = focused_stack.as_ref().and_then(|focused| focused.stack);
    let windows = WindowCandidate::collect(&window_query);
    let primary_window = windows
        .iter()
        .find(|window| window.primary)
        .or_else(|| windows.first())
        .map(|window| window.entity);
    let pages = PageCandidate::collect(&space_query, &hierarchy, &page_query);

    let (projected_windows, mut projected_tabs) = {
        let window_ids = windows
            .iter()
            .map(|window| (window.entity, window.id))
            .collect::<HashMap<_, _>>();
        let projected_windows = windows
            .iter()
            .map(|window| ExtensionWindowSnapshot {
                id: window_ids[&window.entity],
                focused: window.focused,
                left: window.left,
                top: window.top,
                width: window.width,
                height: window.height,
                incognito: false,
                window_type: "normal".into(),
                state: "normal".into(),
                always_on_top: false,
            })
            .collect::<Vec<_>>();
        let mut indices = HashMap::<i32, u32>::new();
        let projected_tabs = pages
            .into_iter()
            .filter_map(|page| {
                let window_entity = page
                    .host_window
                    .filter(|entity| window_ids.contains_key(entity))
                    .or(primary_window)?;
                let window_id = window_ids[&window_entity];
                let index = indices.entry(window_id).or_default();
                let tab = ExtensionTabSnapshot {
                    id: page.id,
                    window_id,
                    index: *index,
                    active: false,
                    highlighted: false,
                    pinned: false,
                    url: page.url,
                    title: page.title,
                    status: page.status,
                };
                *index += 1;
                Some(ProjectedTab {
                    entity: page.entity,
                    activated_at: page.activated_at,
                    tab,
                })
            })
            .collect::<Vec<_>>();
        (projected_windows, projected_tabs)
    };

    ProjectedTab::select_active(&previous, focused_stack, &mut projected_tabs);
    let model = ExtensionModel {
        windows: projected_windows,
        tabs: projected_tabs.into_iter().map(|item| item.tab).collect(),
    };
    for event in previous.events(&model) {
        events.write(event);
    }
    if previous != model {
        **current = model;
    }
}

impl WindowCandidate {
    fn collect(
        query: &Query<(Entity, &Window, &ExtensionWindowId, Has<PrimaryWindow>)>,
    ) -> Vec<Self> {
        let mut windows = query
            .iter()
            .map(|(entity, window, id, primary)| {
                let scale = window.resolution.scale_factor().max(f32::EPSILON);
                let (left, top) = match window.position {
                    WindowPosition::At(position) => (
                        (position.x as f32 / scale).round() as i32,
                        (position.y as f32 / scale).round() as i32,
                    ),
                    _ => (0, 0),
                };
                WindowCandidate {
                    entity,
                    id: id.0,
                    primary,
                    focused: window.focused,
                    left,
                    top,
                    width: window.resolution.width().round() as i32,
                    height: window.resolution.height().round() as i32,
                }
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| (!window.primary, window.entity.to_bits()));
        windows
    }
}

impl PageCandidate {
    fn collect(
        space_query: &Query<(Entity, Option<&Order>), With<Space>>,
        hierarchy: &Query<HierarchyData>,
        page_query: &Query<PageData>,
    ) -> Vec<Self> {
        let mut spaces = space_query
            .iter()
            .map(|(entity, order)| (order.map_or(u32::MAX, |order| order.0), entity))
            .collect::<Vec<_>>();
        spaces.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
        let mut pages = Vec::new();
        for (_, space) in spaces {
            let Ok((Some(children), _, _, _, _, _, _)) = hierarchy.get(space) else {
                continue;
            };
            let mut tabs = children
                .iter()
                .filter_map(|entity| {
                    let Ok((_, _, order, is_tab, _, _, _)) = hierarchy.get(entity) else {
                        return None;
                    };
                    is_tab.then_some((order.map_or(u32::MAX, |order| order.0), entity))
                })
                .collect::<Vec<_>>();
            tabs.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
            for (_, tab) in tabs {
                let mut stacks = Vec::new();
                Self::collect_stacks(hierarchy, tab, &mut stacks);
                for stack in stacks {
                    if let Some(page) = Self::from_entity(hierarchy, page_query, stack) {
                        pages.push(page);
                    }
                }
            }
        }
        pages
    }

    fn collect_stacks(hierarchy: &Query<HierarchyData>, entity: Entity, stacks: &mut Vec<Entity>) {
        let Ok((children, _, _, _, is_stack, _, _)) = hierarchy.get(entity) else {
            return;
        };
        if is_stack {
            stacks.push(entity);
            return;
        }
        if let Some(children) = children {
            for child in children.iter() {
                Self::collect_stacks(hierarchy, child, stacks);
            }
        }
    }

    fn from_entity(
        hierarchy: &Query<HierarchyData>,
        page_query: &Query<PageData>,
        entity: Entity,
    ) -> Option<Self> {
        let (_, metadata, id, activated, identity, is_bridge, loading) =
            page_query.get(entity).ok()?;
        if is_bridge || !extension_visible_url(&metadata.url) {
            return None;
        }
        let child_loading = hierarchy
            .get(entity)
            .ok()
            .and_then(|(children, _, _, _, _, _, _)| children)
            .is_some_and(|children| {
                children.iter().any(|child| {
                    hierarchy
                        .get(child)
                        .is_ok_and(|(_, _, _, _, _, _, loading)| loading)
                })
            });
        Some(Self {
            entity,
            id: id.0,
            host_window: Self::host_window(hierarchy, entity),
            activated_at: activated.map_or(0, |activated| activated.0),
            url: metadata.url.clone(),
            title: crate::state::PagePresentation::title(metadata, identity),
            status: if loading || child_loading {
                "loading"
            } else {
                "complete"
            }
            .into(),
        })
    }

    fn host_window(hierarchy: &Query<HierarchyData>, entity: Entity) -> Option<Entity> {
        let (children, _, _, _, _, host, _) = hierarchy.get(entity).ok()?;
        if let Some(host) = host {
            return Some(host.0);
        }
        if let Some(host) = children
            .into_iter()
            .flat_map(|children| children.iter())
            .find_map(|child| {
                hierarchy
                    .get(child)
                    .ok()
                    .and_then(|(_, _, _, _, _, host, _)| host)
            })
        {
            return Some(host.0);
        }
        let mut current = entity;
        while let Some(parent) = hierarchy
            .get(current)
            .ok()
            .and_then(|(_, parent, _, _, _, _, _)| parent)
            .map(Relationship::get)
        {
            if let Ok((_, _, _, _, _, Some(host), _)) = hierarchy.get(parent) {
                return Some(host.0);
            }
            current = parent;
        }
        None
    }
}

impl ProjectedTab {
    fn select_active(
        previous: &ExtensionModel,
        focused_stack: Option<Entity>,
        projected: &mut [Self],
    ) {
        let mut selected = HashMap::<i32, i32>::new();
        if let Some(focused) = focused_stack
            && let Some(item) = projected.iter().find(|item| item.entity == focused)
        {
            selected.insert(item.tab.window_id, item.tab.id);
        }
        for previous_tab in previous.tabs.iter().filter(|tab| tab.active) {
            if selected.contains_key(&previous_tab.window_id) {
                continue;
            }
            if projected.iter().any(|item| item.tab.id == previous_tab.id) {
                selected.insert(previous_tab.window_id, previous_tab.id);
            }
        }
        let window_ids = projected
            .iter()
            .map(|item| item.tab.window_id)
            .collect::<std::collections::HashSet<_>>();
        for window_id in window_ids {
            if selected.contains_key(&window_id) {
                continue;
            }
            if let Some(item) = projected
                .iter()
                .filter(|item| item.tab.window_id == window_id)
                .max_by_key(|item| item.activated_at)
            {
                selected.insert(window_id, item.tab.id);
            }
        }
        for item in projected {
            item.tab.active = selected.get(&item.tab.window_id) == Some(&item.tab.id);
            item.tab.highlighted = item.tab.active;
        }
    }
}

impl ExtensionModel {
    fn events(&self, current: &Self) -> Vec<ExtensionModelEvent> {
        let old_windows = self
            .windows
            .iter()
            .map(|window| (window.id, window))
            .collect::<HashMap<_, _>>();
        let new_windows = current
            .windows
            .iter()
            .map(|window| (window.id, window))
            .collect::<HashMap<_, _>>();
        let old_tabs = self
            .tabs
            .iter()
            .map(|tab| (tab.id, tab))
            .collect::<HashMap<_, _>>();
        let new_tabs = current
            .tabs
            .iter()
            .map(|tab| (tab.id, tab))
            .collect::<HashMap<_, _>>();
        let mut events = Vec::new();
        for old in &self.windows {
            if !new_windows.contains_key(&old.id) {
                events.push(ExtensionModelEvent::WindowRemoved { window_id: old.id });
            }
        }
        for new in &current.windows {
            match old_windows.get(&new.id) {
                None => events.push(ExtensionModelEvent::WindowCreated(new.clone())),
                Some(old)
                    if old.left != new.left
                        || old.top != new.top
                        || old.width != new.width
                        || old.height != new.height =>
                {
                    events.push(ExtensionModelEvent::WindowBoundsChanged(new.clone()));
                }
                Some(_) => {}
            }
        }
        let old_focused = self
            .windows
            .iter()
            .find(|window| window.focused)
            .map_or(-1, |window| window.id);
        let new_focused = current
            .windows
            .iter()
            .find(|window| window.focused)
            .map_or(-1, |window| window.id);
        if old_focused != new_focused {
            events.push(ExtensionModelEvent::WindowFocusChanged {
                window_id: new_focused,
            });
        }
        for old in &self.tabs {
            if !new_tabs.contains_key(&old.id) {
                events.push(ExtensionModelEvent::TabRemoved {
                    tab_id: old.id,
                    window_id: old.window_id,
                });
            }
        }
        for new in &current.tabs {
            match old_tabs.get(&new.id) {
                None => events.push(ExtensionModelEvent::TabCreated(new.clone())),
                Some(old) if *old != new => events.push(ExtensionModelEvent::TabUpdated {
                    old: (*old).clone(),
                    new: new.clone(),
                }),
                Some(_) => {}
            }
            if new.active && !old_tabs.get(&new.id).is_some_and(|old| old.active) {
                events.push(ExtensionModelEvent::TabActivated {
                    tab_id: new.id,
                    window_id: new.window_id,
                });
            }
        }
        events
    }
}
