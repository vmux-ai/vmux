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

use super::{
    ChromeModel, ChromeModelEvent, ChromeStableIds, ChromeTab, ChromeWindow, extension_visible_url,
};
use crate::extensions::bridge_page::ExtensionBridgeWebview;

struct WindowCandidate {
    entity: Entity,
    primary: bool,
    focused: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

struct PageCandidate {
    entity: Entity,
    host_window: Option<Entity>,
    activated_at: i64,
    url: String,
    title: String,
    status: String,
}

struct ProjectedTab {
    entity: Entity,
    activated_at: i64,
    tab: ChromeTab,
}

pub(crate) fn rebuild_chrome_model(world: &mut World) {
    let previous = world.resource::<ChromeModel>().clone();
    let focused_stack = world
        .get_resource::<FocusedStack>()
        .and_then(|focused| focused.stack);
    let windows = collect_windows(world);
    let primary_window = windows
        .iter()
        .find(|window| window.primary)
        .or_else(|| windows.first())
        .map(|window| window.entity);
    let pages = collect_pages(world);

    let (chrome_windows, mut projected_tabs) = {
        let mut stable_ids = world.resource_mut::<ChromeStableIds>();
        let window_ids = windows
            .iter()
            .map(|window| (window.entity, stable_ids.window(window.entity)))
            .collect::<HashMap<_, _>>();
        let chrome_windows = windows
            .iter()
            .map(|window| ChromeWindow {
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
                let tab = ChromeTab {
                    id: stable_ids.tab(page.entity),
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
        (chrome_windows, projected_tabs)
    };

    select_active_tabs(&previous, focused_stack, &mut projected_tabs);
    let model = ChromeModel {
        windows: chrome_windows,
        tabs: projected_tabs.into_iter().map(|item| item.tab).collect(),
    };
    emit_model_events(world, &previous, &model);
    if previous != model {
        *world.resource_mut::<ChromeModel>() = model;
    }
}

fn collect_windows(world: &mut World) -> Vec<WindowCandidate> {
    let mut query = world.query::<(Entity, &Window, Has<PrimaryWindow>)>();
    let mut windows = query
        .iter(world)
        .map(|(entity, window, primary)| {
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

fn collect_pages(world: &mut World) -> Vec<PageCandidate> {
    let mut spaces_query = world.query_filtered::<(Entity, Option<&Order>), With<Space>>();
    let mut spaces = spaces_query
        .iter(world)
        .map(|(entity, order)| (order.map_or(u32::MAX, |order| order.0), entity))
        .collect::<Vec<_>>();
    spaces.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
    let mut pages = Vec::new();
    for (_, space) in spaces {
        let mut tabs = world
            .get::<Children>(space)
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|entity| world.get::<Tab>(*entity).is_some())
            .map(|entity| {
                (
                    world.get::<Order>(entity).map_or(u32::MAX, |order| order.0),
                    entity,
                )
            })
            .collect::<Vec<_>>();
        tabs.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
        for (_, tab) in tabs {
            let mut stacks = Vec::new();
            collect_stacks(world, tab, &mut stacks);
            for stack in stacks {
                if let Some(page) = page_candidate(world, stack) {
                    pages.push(page);
                }
            }
        }
    }
    pages
}

fn collect_stacks(world: &World, entity: Entity, stacks: &mut Vec<Entity>) {
    if world.get::<Stack>(entity).is_some() {
        stacks.push(entity);
        return;
    }
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.iter() {
            collect_stacks(world, child, stacks);
        }
    }
}

fn page_candidate(world: &World, entity: Entity) -> Option<PageCandidate> {
    if world.get::<ExtensionBridgeWebview>(entity).is_some() {
        return None;
    }
    let metadata = world.get::<PageMetadata>(entity)?;
    if !extension_visible_url(&metadata.url) {
        return None;
    }
    let loading = world.get::<Loading>(entity).is_some()
        || world.get::<Children>(entity).is_some_and(|children| {
            children
                .iter()
                .any(|child| world.get::<Loading>(child).is_some())
        });
    Some(PageCandidate {
        entity,
        host_window: host_window_for(world, entity),
        activated_at: world
            .get::<LastActivatedAt>(entity)
            .map_or(0, |activated| activated.0),
        url: metadata.url.clone(),
        title: metadata.title.clone(),
        status: if loading { "loading" } else { "complete" }.into(),
    })
}

fn host_window_for(world: &World, entity: Entity) -> Option<Entity> {
    if let Some(host) = world.get::<HostWindow>(entity) {
        return Some(host.0);
    }
    if let Some(host) = world.get::<Children>(entity).and_then(|children| {
        children
            .iter()
            .find_map(|child| world.get::<HostWindow>(child))
    }) {
        return Some(host.0);
    }
    let mut current = entity;
    while let Some(parent) = world.get::<ChildOf>(current).map(Relationship::get) {
        if let Some(host) = world.get::<HostWindow>(parent) {
            return Some(host.0);
        }
        current = parent;
    }
    None
}

fn select_active_tabs(
    previous: &ChromeModel,
    focused_stack: Option<Entity>,
    projected: &mut [ProjectedTab],
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

fn emit_model_events(world: &mut World, previous: &ChromeModel, current: &ChromeModel) {
    let old_windows = previous
        .windows
        .iter()
        .map(|window| (window.id, window))
        .collect::<HashMap<_, _>>();
    let new_windows = current
        .windows
        .iter()
        .map(|window| (window.id, window))
        .collect::<HashMap<_, _>>();
    let old_tabs = previous
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
    for old in &previous.windows {
        if !new_windows.contains_key(&old.id) {
            events.push(ChromeModelEvent::WindowRemoved { window_id: old.id });
        }
    }
    for new in &current.windows {
        match old_windows.get(&new.id) {
            None => events.push(ChromeModelEvent::WindowCreated(new.clone())),
            Some(old)
                if old.left != new.left
                    || old.top != new.top
                    || old.width != new.width
                    || old.height != new.height =>
            {
                events.push(ChromeModelEvent::WindowBoundsChanged(new.clone()));
            }
            Some(_) => {}
        }
    }
    let old_focused = previous
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
        events.push(ChromeModelEvent::WindowFocusChanged {
            window_id: new_focused,
        });
    }
    for old in &previous.tabs {
        if !new_tabs.contains_key(&old.id) {
            events.push(ChromeModelEvent::TabRemoved {
                tab_id: old.id,
                window_id: old.window_id,
            });
        }
    }
    for new in &current.tabs {
        match old_tabs.get(&new.id) {
            None => events.push(ChromeModelEvent::TabCreated(new.clone())),
            Some(old) if *old != new => events.push(ChromeModelEvent::TabUpdated {
                old: (*old).clone(),
                new: new.clone(),
            }),
            Some(_) => {}
        }
        if new.active && !old_tabs.get(&new.id).is_some_and(|old| old.active) {
            events.push(ChromeModelEvent::TabActivated {
                tab_id: new.id,
                window_id: new.window_id,
            });
        }
    }
    for event in events {
        world.write_message(event);
    }
}
