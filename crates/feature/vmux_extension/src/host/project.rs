use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowPosition};
use bevy_cef::prelude::HostWindow;
use std::collections::HashMap;
use vmux_ecs::{Order, PageMetadata, PopupWebview};
use vmux_flex::prelude::Visibility;
use vmux_history::LastActivatedAt;
use vmux_layout::Loading;
use vmux_layout::space::Space;
use vmux_layout::stack::{FocusedStack, Stack};
use vmux_layout::tab::Tab;

use super::bridge_page::ExtensionBridgeWebview;
use super::model::{
    ExtensionIdSequence, ExtensionModel, ExtensionModelEvent, ExtensionPopup, ExtensionTabId,
    ExtensionTabSnapshot, ExtensionWindowId, ExtensionWindowSnapshot,
};

pub(crate) struct ExtensionProjectPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExtensionProjectionSet;

impl Plugin for ExtensionProjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExtensionModelEvent>()
            .add_systems(Startup, spawn_model)
            .add_systems(
                Update,
                (
                    assign_ids,
                    bevy::ecs::schedule::ApplyDeferred,
                    rebuild_model,
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
    Option<&'static vmux_ecs::PageIdentity>,
    Has<ExtensionBridgeWebview>,
    Has<Loading>,
);

#[derive(SystemParam)]
struct ProjectionData<'w, 's> {
    focused_window: vmux_layout::window::FocusedWindow<'w, 's>,
    windows: Query<
        'w,
        's,
        (
            Entity,
            &'static Window,
            &'static ExtensionWindowId,
            Has<PrimaryWindow>,
        ),
    >,
    spaces: Query<'w, 's, (Entity, Option<&'static Order>), With<Space>>,
    hierarchy: Query<'w, 's, HierarchyData>,
    pages: Query<'w, 's, PageData>,
    popups: Query<
        'w,
        's,
        (
            &'static PageMetadata,
            &'static ExtensionTabId,
            &'static ExtensionWindowId,
        ),
        With<ExtensionPopup>,
    >,
    browser_popups: Query<
        'w,
        's,
        (
            &'static PageMetadata,
            &'static ExtensionTabId,
            &'static HostWindow,
            &'static Visibility,
            Has<Loading>,
        ),
        (With<PopupWebview>, Without<ExtensionPopup>),
    >,
}

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

fn spawn_model(mut commands: Commands) {
    commands.spawn((ExtensionModel::default(), ExtensionIdSequence::default()));
}

fn assign_ids(
    windows: Query<Entity, (With<Window>, Without<ExtensionWindowId>)>,
    tabs: Query<
        Entity,
        (
            Or<(With<Stack>, With<ExtensionPopup>, With<PopupWebview>)>,
            With<PageMetadata>,
            Without<ExtensionTabId>,
        ),
    >,
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

fn rebuild_model(
    projection: ProjectionData,
    focused_stack: FocusedStack,
    mut current: Single<&mut ExtensionModel>,
    mut events: MessageWriter<ExtensionModelEvent>,
) {
    let previous = (**current).clone();
    let focused_stack = focused_stack.as_ref().and_then(|focused| focused.stack);
    let windows = projection.windows();
    let primary_window = windows
        .iter()
        .find(|window| window.primary)
        .or_else(|| windows.first())
        .map(|window| window.entity);
    let pages = projection.pages();

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
    let mut tabs = projected_tabs
        .into_iter()
        .map(|item| item.tab)
        .collect::<Vec<_>>();
    for (metadata, id, window_id) in &projection.popups {
        tabs.push(ExtensionTabSnapshot {
            id: id.0,
            window_id: window_id.0,
            index: 0,
            active: false,
            highlighted: false,
            pinned: false,
            url: metadata.url.clone(),
            title: metadata.title.clone(),
            status: "complete".into(),
        });
    }
    let mut windows = projected_windows;
    for (metadata, id, host, visibility, loading) in &projection.browser_popups {
        if *visibility == Visibility::Hidden
            || !(metadata.url.starts_with("https://") || metadata.url.starts_with("http://"))
        {
            continue;
        }
        let Ok((_, _, window_id, _)) = projection.windows.get(host.0) else {
            continue;
        };
        for tab in &mut tabs {
            if tab.window_id == window_id.0 {
                tab.active = false;
                tab.highlighted = false;
            }
        }
        if projection.focused_window.entity() == Some(host.0) {
            for window in &mut windows {
                window.focused = window.id == window_id.0;
            }
        }
        let index = tabs
            .iter()
            .filter(|tab| tab.window_id == window_id.0)
            .map(|tab| tab.index)
            .max()
            .map_or(0, |index| index.saturating_add(1));
        tabs.push(ExtensionTabSnapshot {
            id: id.0,
            window_id: window_id.0,
            index,
            active: true,
            highlighted: true,
            pinned: false,
            url: metadata.url.clone(),
            title: metadata.title.clone(),
            status: if loading { "loading" } else { "complete" }.into(),
        });
    }
    let model = ExtensionModel { windows, tabs };
    for event in previous.events(&model) {
        events.write(event);
    }
    if previous != model {
        **current = model;
    }
}

impl ProjectionData<'_, '_> {
    fn windows(&self) -> Vec<WindowCandidate> {
        let mut windows = self
            .windows
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

    fn pages(&self) -> Vec<PageCandidate> {
        let mut spaces = self
            .spaces
            .iter()
            .map(|(entity, order)| (order.map_or(u32::MAX, |order| order.0), entity))
            .collect::<Vec<_>>();
        spaces.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
        let mut pages = Vec::new();
        for (_, space) in spaces {
            let Ok((Some(children), _, _, _, _, _, _)) = self.hierarchy.get(space) else {
                continue;
            };
            let mut tabs = children
                .iter()
                .filter_map(|entity| {
                    let Ok((_, _, order, is_tab, _, _, _)) = self.hierarchy.get(entity) else {
                        return None;
                    };
                    is_tab.then_some((order.map_or(u32::MAX, |order| order.0), entity))
                })
                .collect::<Vec<_>>();
            tabs.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
            for (_, tab) in tabs {
                let mut stacks = Vec::new();
                self.collect_stacks(tab, &mut stacks);
                for stack in stacks {
                    if let Some(page) = self.page(stack) {
                        pages.push(page);
                    }
                }
            }
        }
        pages
    }

    fn collect_stacks(&self, entity: Entity, stacks: &mut Vec<Entity>) {
        let Ok((children, _, _, _, is_stack, _, _)) = self.hierarchy.get(entity) else {
            return;
        };
        if is_stack {
            stacks.push(entity);
            return;
        }
        if let Some(children) = children {
            for child in children.iter() {
                self.collect_stacks(child, stacks);
            }
        }
    }

    fn page(&self, entity: Entity) -> Option<PageCandidate> {
        let (_, metadata, id, activated, identity, is_bridge, loading) =
            self.pages.get(entity).ok()?;
        if is_bridge || !PageCandidate::visible_url(&metadata.url) {
            return None;
        }
        let child_loading = self
            .hierarchy
            .get(entity)
            .ok()
            .and_then(|(children, _, _, _, _, _, _)| children)
            .is_some_and(|children| {
                children.iter().any(|child| {
                    self.hierarchy
                        .get(child)
                        .is_ok_and(|(_, _, _, _, _, _, loading)| loading)
                })
            });
        Some(PageCandidate {
            entity,
            id: id.0,
            host_window: self.host_window(entity),
            activated_at: activated.map_or(0, |activated| activated.0),
            url: metadata.url.clone(),
            title: PageCandidate::title(metadata, identity),
            status: if loading || child_loading {
                "loading"
            } else {
                "complete"
            }
            .into(),
        })
    }

    fn host_window(&self, entity: Entity) -> Option<Entity> {
        let (children, _, _, _, _, host, _) = self.hierarchy.get(entity).ok()?;
        if let Some(host) = host {
            return Some(host.0);
        }
        if let Some(host) = children
            .into_iter()
            .flat_map(|children| children.iter())
            .find_map(|child| {
                self.hierarchy
                    .get(child)
                    .ok()
                    .and_then(|(_, _, _, _, _, host, _)| host)
            })
        {
            return Some(host.0);
        }
        let mut current = entity;
        while let Some(parent) = self
            .hierarchy
            .get(current)
            .ok()
            .and_then(|(_, parent, _, _, _, _, _)| parent)
            .map(Relationship::get)
        {
            if let Ok((_, _, _, _, _, Some(host), _)) = self.hierarchy.get(parent) {
                return Some(host.0);
            }
            current = parent;
        }
        None
    }
}

impl PageCandidate {
    fn visible_url(url: &str) -> bool {
        url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("chrome-extension://")
    }

    fn title(metadata: &PageMetadata, identity: Option<&vmux_ecs::PageIdentity>) -> String {
        match identity.and_then(|identity| identity.title.as_deref()) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => metadata.title.clone(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::bridge::BridgeAuthorization;
    use crate::host::tabs::ExtensionTabs;
    use crate::host::windows::{ExtensionWindows, WindowDispatch};
    use crate::protocol::{ApiRequest, ExtensionCallerContext};
    use serde_json::json;
    use vmux_layout::active_pane::ActiveStack;

    #[test]
    fn a_visible_browser_popup_is_the_autofill_target_and_hiding_it_restores_the_content_tab() {
        let mut app = App::new();
        app.add_plugins(ExtensionProjectPlugin);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    focused: false,
                    ..default()
                },
                PrimaryWindow,
                vmux_ecs::Active,
            ))
            .id();
        let space = app.world_mut().spawn(Space).id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                HostWindow(window),
                ChildOf(tab),
                PageMetadata {
                    url: "https://example.com/".into(),
                    ..default()
                },
            ))
            .id();
        app.world_mut().spawn(
            ActiveStack {
                stack: Some(stack),
                ..default()
            }
            .local_bundle(),
        );
        let popup = app
            .world_mut()
            .spawn((
                PopupWebview,
                HostWindow(window),
                Visibility::Visible,
                PageMetadata {
                    url: "https://github.com/login/device".into(),
                    ..default()
                },
            ))
            .id();
        app.update();
        {
            let world = app.world_mut();
            let model = world.query::<&ExtensionModel>().single(world).unwrap();
            let active: Vec<_> = model.tabs.iter().filter(|tab| tab.active).collect();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].url, "https://github.com/login/device");
            assert!(model.windows[0].focused);
        }
        app.world_mut().entity_mut(popup).insert(Visibility::Hidden);
        app.update();
        let world = app.world_mut();
        let model = world.query::<&ExtensionModel>().single(world).unwrap();
        assert_eq!(model.tabs.len(), 1);
        assert_eq!(model.tabs[0].url, "https://example.com/");
        assert!(model.tabs[0].active);
    }

    #[test]
    fn authentication_popup_is_searchable_for_close_without_becoming_the_active_content_tab() {
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let url =
            format!("chrome-extension://{id}/popup/index.html?singleActionPopout=fido#/fido2");
        let mut app = App::new();
        app.add_plugins(ExtensionProjectPlugin);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let space = app.world_mut().spawn(Space).id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://accounts.google.com/".into(),
                    ..default()
                },
                HostWindow(window),
                ChildOf(tab),
            ))
            .id();
        app.world_mut().spawn(
            ActiveStack {
                stack: Some(stack),
                ..default()
            }
            .local_bundle(),
        );
        app.update();
        let mut model = app
            .world_mut()
            .query::<&ExtensionModel>()
            .single(app.world())
            .unwrap()
            .clone();
        let caller = ExtensionCallerContext::ServiceWorker {
            extension_id: id.into(),
            context_id: "worker".into(),
            url: None,
        };
        let authorization = BridgeAuthorization {
            permissions: ["tabs".to_string()].into(),
            ..default()
        };
        let mut windows = ExtensionWindows::default();
        let created = WindowDispatch::from_request(
            &ApiRequest {
                request_id: "create".into(),
                namespace: "windows".into(),
                method: "create".into(),
                arguments: json!([{ "type": "popup", "url": url }]),
                caller_context: caller.clone(),
            },
            &model,
            &mut windows,
            &authorization,
        )
        .unwrap();
        let window_id = created.result["id"].as_i64().unwrap() as i32;
        let popup = app
            .world_mut()
            .spawn((
                ExtensionPopup {
                    owner: window,
                    extension_id: id.into(),
                },
                ExtensionWindowId(window_id),
                PageMetadata {
                    url: url.clone(),
                    ..default()
                },
            ))
            .id();
        app.update();
        model = app
            .world_mut()
            .query::<&ExtensionModel>()
            .single(app.world())
            .unwrap()
            .clone();
        let tabs = ExtensionTabs::from(&model);
        let active = tabs
            .dispatch(
                &ApiRequest {
                    request_id: "active".into(),
                    namespace: "tabs".into(),
                    method: "query".into(),
                    arguments: json!([{ "active": true, "currentWindow": true }]),
                    caller_context: caller.clone(),
                },
                &authorization,
            )
            .unwrap();
        assert_eq!(active.as_array().unwrap().len(), 1);
        assert_eq!(active[0]["url"], "https://accounts.google.com/");
        let popouts = tabs.dispatch(&ApiRequest {
            request_id: "popouts".into(), namespace: "tabs".into(), method: "query".into(),
            arguments: json!([{ "url": format!("chrome-extension://{id}/popup/index.html*") }]), caller_context: caller.clone(),
        }, &authorization).unwrap();
        assert_eq!(popouts.as_array().unwrap().len(), 1);
        assert_eq!(popouts[0]["windowId"], window_id);
        let populated = WindowDispatch::from_request(
            &ApiRequest {
                request_id: "populated".into(),
                namespace: "windows".into(),
                method: "get".into(),
                arguments: json!([window_id, { "populate": true }]),
                caller_context: caller.clone(),
            },
            &model,
            &mut windows,
            &authorization,
        )
        .unwrap();
        assert_eq!(populated.result["tabs"].as_array().unwrap().len(), 1);
        assert_eq!(populated.result["tabs"][0]["id"], popouts[0]["id"]);
        let removed = WindowDispatch::from_request(
            &ApiRequest {
                request_id: "close".into(),
                namespace: "windows".into(),
                method: "remove".into(),
                arguments: json!([popouts[0]["windowId"]]),
                caller_context: caller,
            },
            &model,
            &mut windows,
            &authorization,
        )
        .unwrap();
        assert_eq!(removed.close_window.unwrap().window_id, Some(window_id));
        app.world_mut().despawn(popup);
        app.update();
        let model = app
            .world_mut()
            .query::<&ExtensionModel>()
            .single(app.world())
            .unwrap()
            .clone();
        assert_eq!(model.tabs.len(), 1);
        assert!(model.tabs[0].active);
    }

    #[test]
    fn filters_extension_visible_urls() {
        assert!(PageCandidate::visible_url("https://example.com/"));
        assert!(PageCandidate::visible_url("http://example.com/"));
        assert!(PageCandidate::visible_url(
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html"
        ));
        assert!(!PageCandidate::visible_url("vmux://terminal/"));
        assert!(!PageCandidate::visible_url("file:///Users/x/notes.md"));
    }
}
