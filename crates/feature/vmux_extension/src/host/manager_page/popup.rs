use crate::store;
use bevy::ecs::system::SystemState;
use bevy::{prelude::*, window::WindowResized};
use bevy_cef::prelude::{
    Browsers, HostWindow, JsEmitEventPlugin, Receive, UiEventPlugin, UiInput, WebviewWindowed,
};
use vmux_api::extension::{
    ExtRow, ExtensionPopupAnchor, ExtensionPopupBoundsRequest, ExtensionPopupCloseRequest,
    ExtensionPopupEvent, ExtensionPopupOpenRequest, ExtensionPopupSizeEvent,
};
use vmux_ecs::overlay::WindowOverlay;
use vmux_ecs::{KeyboardOwner, UiStateWrite};
use vmux_flex::prelude::{LayoutSystems, Visibility};
use vmux_layout::{Browser, LayoutCef, state::LayoutUiState};

use super::super::model::ExtensionPopup;
use super::super::model::{ExtensionModel, ExtensionModelEvent, ExtensionWindowId};
use super::super::windows::{ExtensionWindows, WindowRequestSet};
use crate::catalog::ExtensionCatalog;

pub(super) struct PopupPlugin;

impl Plugin for PopupPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenRequest>()
            .add_message::<CloseRequest>()
            .add_plugins(UiEventPlugin::<(
                ExtensionPopupOpenRequest,
                ExtensionPopupBoundsRequest,
                ExtensionPopupCloseRequest,
            )>::default())
            .add_plugins(JsEmitEventPlugin::<ExtensionPopupSizeRequest>::default())
            .add_plugins(JsEmitEventPlugin::<DismissRequest>::default())
            .add_observer(open_request)
            .add_observer(bounds_request)
            .add_observer(close_request)
            .add_observer(size)
            .add_observer(dismiss)
            .add_systems(
                Update,
                (open, close, dispose).chain().after(WindowRequestSet),
            )
            .add_systems(Update, inject_sizing.after(vmux_browser::BrowserLoadSet))
            .add_systems(
                PostUpdate,
                present
                    .in_set(vmux_browser::BrowserOverlaySet)
                    .after(LayoutSystems::Layout),
            );
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenRequest {
    pub(crate) extension_id: String,
    pub(crate) url: String,
    pub(crate) window_id: i32,
    pub(crate) host_window_id: Option<i32>,
}

impl OpenRequest {
    pub(crate) fn owns_url(extension_id: &str, url: &str) -> bool {
        let Ok(url) = url::Url::parse(url) else {
            return false;
        };
        url.scheme() == "chrome-extension" && url.host_str() == Some(extension_id)
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct CloseRequest {
    pub(crate) window_id: i32,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PopupHost<'w, 's> {
    popups: Query<
        'w,
        's,
        (
            Entity,
            &'static ExtensionPopup,
            &'static vmux_ecs::PageMetadata,
            Option<&'static ExtensionWindowId>,
        ),
    >,
    layouts: Query<'w, 's, (Entity, Option<&'static HostWindow>), With<LayoutCef>>,
    host_windows: Query<'w, 's, &'static HostWindow>,
    native_windows: Query<'w, 's, (Entity, &'static Window, &'static ExtensionWindowId)>,
    catalog: Query<'w, 's, &'static ExtensionCatalog>,
    profile: vmux_ecs::profile::CurrentProfile<'w, 's>,
    model: Single<'w, 's, &'static ExtensionModel>,
    virtual_windows: Single<'w, 's, &'static mut ExtensionWindows>,
    events: MessageWriter<'w, ExtensionModelEvent>,
    browsers: NonSend<'w, Browsers>,
    commands: Commands<'w, 's>,
}

impl PopupHost<'_, '_> {
    fn entry(&self, id: &str) -> Option<ExtRow> {
        if let Some(catalog) = self.catalog.iter().next()
            && catalog.loaded()
        {
            return catalog.enabled(id).cloned();
        }
        let (profile, paths) = self.profile.profile().zip(self.profile.paths())?;
        let snapshot = store::ExtensionStore::at(paths.extensions())
            .snapshot(profile.id())
            .ok()?;
        snapshot
            .extensions
            .into_iter()
            .find(|entry| entry.id == id && entry.enabled && !entry.needs_approval)
    }

    fn owner_for_surface(&self, source: Entity) -> Option<Entity> {
        if self.layouts.contains(source) {
            return Some(source);
        }
        let source_window = self.host_windows.get(source).ok()?.0;
        self.layouts.iter().find_map(|(layout, host)| {
            host.is_some_and(|host| host.0 == source_window)
                .then_some(layout)
        })
    }

    fn owner_for_window(&self, window_id: Option<i32>) -> Option<Entity> {
        let id = window_id.or_else(|| {
            self.model
                .windows
                .iter()
                .find(|window| window.focused)
                .or_else(|| self.model.windows.first())
                .map(|window| window.id)
        })?;
        let entity = self
            .native_windows
            .iter()
            .find_map(|(entity, _, window_id)| (window_id.0 == id).then_some(entity))?;
        self.layouts
            .iter()
            .find_map(|(layout, host)| host.is_some_and(|host| host.0 == entity).then_some(layout))
    }

    fn anchor(&self, owner: Entity) -> Option<ExtensionPopupAnchor> {
        let (_, host) = self.layouts.get(owner).ok()?;
        let (_, window, _) = self.native_windows.get(host?.0).ok()?;
        Some(ExtensionPopupAnchor {
            right: (window.resolution.width().round() as i32 - 12).max(48),
            bottom: 40,
        })
    }

    fn show(
        &mut self,
        owner: Entity,
        entry: ExtRow,
        url: String,
        anchor: ExtensionPopupAnchor,
        window_id: Option<i32>,
    ) {
        if !OpenRequest::owns_url(&entry.id, &url) {
            return;
        }
        if let Some(id) = window_id
            && !self.virtual_windows.has_popup(id, &entry.id)
        {
            return;
        }
        let existing = self
            .popups
            .iter()
            .find_map(|(entity, popup, metadata, id)| {
                (popup.owner == owner && popup.extension_id == entry.id).then_some((
                    entity,
                    metadata.url.clone(),
                    id.map(|id| id.0),
                ))
            });
        let existing_id = existing
            .as_ref()
            .and_then(|(_, _, id)| id.filter(|id| self.virtual_windows.has_popup(*id, &entry.id)));
        let id = match window_id.or(existing_id) {
            Some(id) => id,
            None => {
                let Some((_, Some(host))) = self.layouts.get(owner).ok() else {
                    return;
                };
                let Ok((_, _, id)) = self.native_windows.get(host.0) else {
                    return;
                };
                let Some(window) = self.model.windows.iter().find(|window| window.id == id.0)
                else {
                    return;
                };
                let popup = self.virtual_windows.register_popup(url.clone(), window);
                let id = popup.id;
                self.events.write(ExtensionModelEvent::WindowCreated(popup));
                id
            }
        };
        if let Some(event) = self.virtual_windows.focus_popup(id) {
            self.events.write(event);
        }
        if let Some((entity, previous_url, previous_id)) = existing {
            if previous_id != Some(id)
                && let Some(previous_id) = previous_id
            {
                for event in self.virtual_windows.dismiss_popup(previous_id, &self.model) {
                    self.events.write(event);
                }
            }
            self.commands
                .entity(entity)
                .insert((ExtensionWindowId(id), KeyboardOwner));
            if window_id.is_some() && previous_url != url {
                self.commands.trigger(bevy_cef::prelude::RequestNavigate {
                    webview: entity,
                    url,
                });
            } else if previous_id != Some(id) {
                self.browsers
                    .execute_js(&entity, &format!("globalThis.__vmuxPopupWindowId = {id};"));
            }
        } else {
            self.close(owner);
            let Ok((_, host)) = self.layouts.get(owner) else {
                return;
            };
            let mut entity = self
                .commands
                .spawn(Browser::new_with_title(&url, &entry.name));
            entity.insert((
                Name::new(format!("Extension popup: {}", entry.name)),
                WindowOverlay,
                ExtensionPopup {
                    owner,
                    extension_id: entry.id.clone(),
                },
                ExtensionWindowId(id),
                vmux_browser::PopupWebview,
                Visibility::Hidden,
            ));
            if let Some(host) = host {
                entity.insert(HostWindow(host.0));
            }
            self.commands
                .trigger(UiStateWrite::<LayoutUiState>::from_event(
                    owner,
                    &ExtensionPopupSizeEvent {
                        id: entry.id.clone(),
                        width: 360.0,
                        height: 600.0,
                    },
                ));
        }
        self.commands
            .trigger(UiStateWrite::<LayoutUiState>::from_event(
                owner,
                &ExtensionPopupEvent {
                    id: entry.id,
                    name: entry.name,
                    icon: entry.icon,
                    anchor,
                },
            ));
    }

    fn retire(&mut self, entity: Entity, id: Option<i32>) {
        self.browsers.hide_child_window(&entity);
        self.commands.entity(entity).try_despawn();
        if let Some(id) = id {
            for event in self.virtual_windows.dismiss_popup(id, &self.model) {
                self.events.write(event);
            }
        }
    }

    fn close(&mut self, owner: Entity) {
        let mut targets = Vec::new();
        for (entity, popup, _, id) in &self.popups {
            if popup.owner == owner {
                targets.push((entity, id.map(|id| id.0)));
            }
        }
        for (entity, id) in &targets {
            self.retire(*entity, *id);
        }
        if !targets.is_empty() {
            self.commands
                .trigger(UiStateWrite::<LayoutUiState>::from_event(
                    owner,
                    &ExtensionPopupEvent::default(),
                ));
        }
    }
}

fn open(world: &mut World, state: &mut SystemState<(MessageReader<OpenRequest>, PopupHost)>) {
    let requests: Vec<_> = {
        let Ok((mut requests, _)) = state.get_mut(world) else {
            return;
        };
        requests.read().cloned().collect()
    };
    for request in requests {
        {
            let Ok((_, mut host)) = state.get_mut(world) else {
                continue;
            };
            let Some(entry) = host.entry(&request.extension_id) else {
                continue;
            };
            let Some(owner) = host.owner_for_window(request.host_window_id) else {
                continue;
            };
            let Some(anchor) = host.anchor(owner) else {
                continue;
            };
            host.show(
                owner,
                entry,
                request.url.clone(),
                anchor,
                Some(request.window_id),
            );
        }
        state.apply(world);
    }
}

fn close(mut requests: MessageReader<CloseRequest>, mut host: PopupHost) {
    for request in requests.read() {
        let mut targets = Vec::new();
        for (entity, popup, _, id) in &host.popups {
            if id.is_some_and(|id| id.0 == request.window_id) {
                targets.push((entity, popup.owner));
            }
        }
        for (entity, owner) in targets {
            host.retire(entity, Some(request.window_id));
            host.commands
                .trigger(UiStateWrite::<LayoutUiState>::from_event(
                    owner,
                    &ExtensionPopupEvent::default(),
                ));
        }
    }
}

fn dispose(mut host: PopupHost) {
    let mut targets = Vec::new();
    for (entity, popup, _, id) in &host.popups {
        if !host.layouts.contains(popup.owner) {
            targets.push((entity, id.map(|id| id.0)));
        }
    }
    for (entity, id) in targets {
        host.retire(entity, id);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExtensionPopupBounds {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl ExtensionPopupBounds {
    fn from_request(request: &ExtensionPopupBoundsRequest) -> Option<Self> {
        if !request.left.is_finite()
            || !request.top.is_finite()
            || !request.width.is_finite()
            || !request.height.is_finite()
            || request.width <= 0.0
            || request.height <= 0.0
        {
            return None;
        }
        Some(Self {
            left: request.left,
            top: request.top,
            width: request.width,
            height: request.height,
        })
    }
}

#[derive(Component)]
pub(crate) struct ExtensionPopupPresented;

#[derive(serde::Deserialize)]
struct ExtensionPopupSizeRequest {
    channel: String,
    width: f32,
    height: f32,
}

#[derive(serde::Deserialize)]
struct DismissRequest {
    channel: String,
    window_id: i32,
}

fn dismiss(trigger: On<Receive<DismissRequest>>, mut host: PopupHost) {
    if trigger.payload.channel != "vmux-extension-popup-close" {
        return;
    }
    let Ok((_, popup, _, id)) = host.popups.get(trigger.webview) else {
        return;
    };
    if id.is_none_or(|id| id.0 != trigger.payload.window_id) {
        return;
    }
    let owner = popup.owner;
    host.close(owner);
}

const POPUP_SIZE_CHANNEL: &str = "vmux-extension-popup-size";

fn open_request(trigger: On<UiInput<ExtensionPopupOpenRequest>>, mut host: PopupHost) {
    let Some(entry) = host.entry(&trigger.event().payload.id) else {
        return;
    };
    let Some(owner) = host.owner_for_surface(trigger.event().webview) else {
        return;
    };
    let Some(popup) = &entry.popup else { return };
    let url = format!("chrome-extension://{}/{popup}", entry.id);
    host.show(owner, entry, url, trigger.event().payload.anchor, None);
}

fn bounds_request(
    trigger: On<UiInput<ExtensionPopupBoundsRequest>>,
    popups: Query<(Entity, &ExtensionPopup)>,
    mut commands: Commands,
) {
    let Some(bounds) = ExtensionPopupBounds::from_request(&trigger.event().payload) else {
        return;
    };
    for (entity, popup) in &popups {
        if popup.owner == trigger.event().webview {
            commands
                .entity(entity)
                .insert((bounds, Visibility::Visible, KeyboardOwner));
        }
    }
}

fn close_request(trigger: On<UiInput<ExtensionPopupCloseRequest>>, mut host: PopupHost) {
    host.close(trigger.event().webview);
}

fn inject_sizing(
    mut events: MessageReader<vmux_browser::WebviewLoadCompleted>,
    popups: Query<&ExtensionWindowId, With<ExtensionPopup>>,
    browsers: NonSend<Browsers>,
) {
    for event in events.read() {
        let Ok(id) = popups.get(event.webview) else {
            continue;
        };
        let mut script = format!("globalThis.__vmuxPopupWindowId = {};\n", id.0);
        script.push_str(r#"
(() => {
  globalThis.__vmuxHostedPopup = true;
  if (globalThis.__vmuxPopupSizer) return;
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    cef.emit({ channel: "vmux-extension-popup-close", window_id: globalThis.__vmuxPopupWindowId });
  });
  let scheduled = false;
  let lastWidth = 0;
  let lastHeight = 0;
  const measure = () => {
    scheduled = false;
    const body = document.body;
    if (!body) return;
    const root = document.documentElement;
    const viewportHeight = root.clientHeight || globalThis.innerHeight;
    const viewportWidth = root.clientWidth || globalThis.innerWidth;
    let height = 0;
    let width = 0;
    for (const element of body.querySelectorAll("*")) {
      const style = getComputedStyle(element);
      if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse") {
        continue;
      }
      const rect = element.getBoundingClientRect();
      if (!Number.isFinite(rect.bottom) || !Number.isFinite(rect.right)) {
        continue;
      }
      const fillsHeight = rect.top <= 1 && Math.abs(rect.height - viewportHeight) <= 1 && element.scrollHeight <= element.clientHeight + 1;
      const fillsWidth = rect.left <= 1 && Math.abs(rect.width - viewportWidth) <= 1 && element.scrollWidth <= element.clientWidth + 1;
      if (!fillsHeight) {
        height = Math.max(height, rect.bottom);
      }
      if (!fillsWidth) {
        width = Math.max(width, rect.right);
      }
      if (element.scrollHeight > element.clientHeight + 1) {
        height = Math.max(height, rect.top + element.scrollHeight);
      }
      if (element.scrollWidth > element.clientWidth + 1) {
        width = Math.max(width, rect.left + element.scrollWidth);
      }
      resize.observe(element);
    }
    const style = getComputedStyle(body);
    height = Math.ceil(height + parseFloat(style.paddingBottom || 0) + parseFloat(style.marginBottom || 0));
    width = Math.ceil(width + parseFloat(style.paddingRight || 0) + parseFloat(style.marginRight || 0));
    if (height === lastHeight && width === lastWidth) return;
    lastHeight = height;
    lastWidth = width;
    cef.emit({ channel: "vmux-extension-popup-size", width, height });
  };
  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(measure);
  };
  const resize = new ResizeObserver(schedule);
  resize.observe(document.documentElement);
  resize.observe(document.body);
  const mutation = new MutationObserver(schedule);
  mutation.observe(document.body, { childList: true, subtree: true, attributes: true, characterData: true });
  globalThis.__vmuxPopupSizer = { resize, mutation };
  schedule();
})();
"#,
        );
        browsers.execute_js(&event.webview, &script);
    }
}

fn present(
    browsers: NonSend<Browsers>,
    popups: Query<
        (
            Entity,
            &ExtensionPopupBounds,
            Option<&HostWindow>,
            Option<&ExtensionWindowId>,
            Has<ExtensionPopupPresented>,
        ),
        (
            With<ExtensionPopup>,
            With<WindowOverlay>,
            With<WebviewWindowed>,
        ),
    >,
    windows: Query<&Window>,
    focused_window: vmux_layout::window::FocusedWindow,
    mut resized: MessageReader<WindowResized>,
    mut commands: Commands,
) {
    let resized_windows = resized.read().map(|event| event.window).collect::<Vec<_>>();
    if !resized_windows.is_empty() {
        for (entity, _, host, id, _) in &popups {
            if host.is_some_and(|host| resized_windows.contains(&host.0))
                && let Some(id) = id
            {
                commands.write_message(CloseRequest { window_id: id.0 });
                browsers.hide_child_window(&entity);
            }
        }
        return;
    }
    for (entity, bounds, host_window, _, presented) in &popups {
        let window_entity = host_window.map(|host| host.0).or(focused_window.entity());
        let Some(window_entity) = window_entity else {
            continue;
        };
        let Ok(window) = windows.get(window_entity) else {
            continue;
        };
        if !browsers.has_browser(entity) {
            continue;
        }
        let scale = window.resolution.scale_factor();
        browsers.resize(&entity, Vec2::new(bounds.width, bounds.height), scale);
        browsers.set_windowed_corner_radius(&entity, 14.0 * scale, scale, true);
        if !browsers.host_in_child_window(
            &entity,
            bounds.left as f64,
            bounds.top as f64,
            bounds.width as f64,
            bounds.height as f64,
        ) {
            continue;
        }
        browsers.set_windowed_hidden(&entity, false);
        if presented {
            continue;
        }
        browsers.set_windowed_focus(&entity, true);
        browsers.nudge_windowed_repaint(&entity);
        commands.entity(entity).insert(ExtensionPopupPresented);
    }
}

fn size(
    trigger: On<Receive<ExtensionPopupSizeRequest>>,
    popups: Query<&ExtensionPopup>,
    mut commands: Commands,
) {
    let request = &trigger.payload;
    if request.channel != POPUP_SIZE_CHANNEL
        || !request.width.is_finite()
        || !request.height.is_finite()
    {
        return;
    }
    let Ok(popup) = popups.get(trigger.event().webview) else {
        return;
    };
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        popup.owner,
        &ExtensionPopupSizeEvent {
            id: popup.extension_id.clone(),
            width: request.width.clamp(200.0, 360.0),
            height: request.height.max(80.0),
        },
    ));
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::host::bridge::BridgeAuthorization;
    use crate::host::model::{ExtensionTabSnapshot, ExtensionWindowSnapshot};
    use crate::host::windows::{ExtensionWindowsPlugin, WindowDispatch};
    use crate::protocol::{ApiRequest, ExtensionCallerContext};
    use serde_json::{Value, json};
    use vmux_api::extension::{ExtStatus, ExtensionsUiState};
    use vmux_layout::stack::{CloseStackRequest, Stack};

    const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[derive(Resource, Default)]
    struct Navigations(Vec<(Entity, String)>);

    fn navigate(
        request: On<bevy_cef::prelude::RequestNavigate>,
        mut navigations: ResMut<Navigations>,
    ) {
        navigations.0.push((request.webview, request.url.clone()));
    }

    struct Fixture {
        app: App,
        layout: Entity,
        stack: Entity,
        window: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<WindowResized>()
                .add_message::<vmux_browser::WebviewLoadCompleted>()
                .add_message::<ExtensionModelEvent>()
                .add_message::<vmux_layout::stack::OpenRequest>()
                .add_message::<CloseStackRequest>()
                .insert_non_send(Browsers::default())
                .init_resource::<bevy_cef::prelude::IpcEventRawBuffer>()
                .init_resource::<Navigations>()
                .add_observer(navigate)
                .add_plugins((ExtensionWindowsPlugin, PopupPlugin));
            let window = app
                .world_mut()
                .spawn((
                    Window {
                        resolution: (1280, 800).into(),
                        ..default()
                    },
                    ExtensionWindowId(1),
                ))
                .id();
            let layout = app.world_mut().spawn((LayoutCef, HostWindow(window))).id();
            let stack = app
                .world_mut()
                .spawn((
                    Stack::default(),
                    vmux_ecs::PageMetadata {
                        url: "https://accounts.google.com/".into(),
                        ..default()
                    },
                ))
                .id();
            app.world_mut().spawn(ExtensionModel {
                windows: vec![ExtensionWindowSnapshot {
                    id: 1,
                    focused: true,
                    left: 0,
                    top: 0,
                    width: 1280,
                    height: 800,
                    incognito: false,
                    window_type: "normal".into(),
                    state: "normal".into(),
                    always_on_top: false,
                }],
                tabs: vec![ExtensionTabSnapshot {
                    id: 7,
                    window_id: 1,
                    index: 0,
                    active: true,
                    highlighted: true,
                    pinned: false,
                    url: "https://accounts.google.com/".into(),
                    title: "Google".into(),
                    status: "complete".into(),
                }],
            });
            let mut catalog = ExtensionCatalog::default();
            catalog.replace(ExtensionsUiState {
                extensions: vec![ExtRow {
                    id: ID.into(),
                    name: "Extension".into(),
                    version: "1".into(),
                    icon: None,
                    popup: Some("popup/index.html".into()),
                    enabled: true,
                    pinned: true,
                    needs_approval: false,
                    required_permissions: Vec::new(),
                    required_host_permissions: Vec::new(),
                    status: ExtStatus::Installed,
                }],
                ..default()
            });
            app.world_mut().spawn(catalog);
            app.update();
            Self {
                app,
                layout,
                stack,
                window,
            }
        }

        fn dispatch(&mut self, method: &str, arguments: Value) -> WindowDispatch {
            let model = self
                .app
                .world_mut()
                .query::<&ExtensionModel>()
                .single(self.app.world())
                .unwrap()
                .clone();
            let world = self.app.world_mut();
            let mut query = world.query::<&mut ExtensionWindows>();
            let mut windows = query.single_mut(world).unwrap();
            WindowDispatch::from_request(
                &ApiRequest {
                    request_id: "request".into(),
                    namespace: "windows".into(),
                    method: method.into(),
                    arguments,
                    caller_context: ExtensionCallerContext::ServiceWorker {
                        extension_id: ID.into(),
                        context_id: "worker".into(),
                        url: None,
                    },
                },
                &model,
                &mut windows,
                &BridgeAuthorization {
                    permissions: ["tabs".to_string()].into(),
                    ..Default::default()
                },
            )
            .unwrap()
        }

        fn open(&mut self, url: &str) -> i32 {
            let dispatch = self.dispatch("create", json!([{ "type": "popup", "url": url }]));
            let id = dispatch.result["id"].as_i64().unwrap() as i32;
            self.app
                .world_mut()
                .write_message(dispatch.open_window.unwrap());
            self.app.update();
            id
        }

        fn popup(&mut self) -> Entity {
            self.app
                .world_mut()
                .query_filtered::<Entity, With<ExtensionPopup>>()
                .single(self.app.world())
                .unwrap()
        }

        fn reopen(&mut self) {
            self.app.world_mut().trigger(UiInput {
                webview: self.layout,
                payload: ExtensionPopupOpenRequest {
                    id: ID.into(),
                    anchor: ExtensionPopupAnchor {
                        right: 1200,
                        bottom: 40,
                    },
                },
            });
            self.app.update();
        }
    }

    #[test]
    fn authentication_opens_in_the_modal_without_replacing_the_browser_stack() {
        let mut fixture = Fixture::new();
        let url =
            format!("chrome-extension://{ID}/popup/index.html?singleActionPopout=fido#/fido2");
        let id = fixture.open(&url);
        let popup = fixture.popup();
        assert_eq!(
            fixture.app.world().get::<HostWindow>(popup).unwrap().0,
            fixture.window
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ExtensionWindowId>(popup)
                .unwrap()
                .0,
            id
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::PageMetadata>(popup)
                .unwrap()
                .url,
            url
        );
        assert!(fixture.app.world().get::<Stack>(popup).is_none());
        assert!(fixture.app.world().get::<Stack>(fixture.stack).is_some());
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<vmux_layout::stack::OpenRequest>>()
                .is_empty()
        );
    }

    #[test]
    fn toolbar_reopening_preserves_the_authentication_document_and_window_identity() {
        let mut fixture = Fixture::new();
        let url =
            format!("chrome-extension://{ID}/popup/index.html?singleActionPopout=fido#/fido2");
        let id = fixture.open(&url);
        let popup = fixture.popup();
        let url = format!("chrome-extension://{ID}/authentication/confirm.html?challenge=active");
        fixture
            .app
            .world_mut()
            .get_mut::<vmux_ecs::PageMetadata>(popup)
            .unwrap()
            .url = url.clone();
        fixture.reopen();
        assert_eq!(fixture.popup(), popup);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ExtensionWindowId>(popup)
                .unwrap()
                .0,
            id
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::PageMetadata>(popup)
                .unwrap()
                .url,
            url
        );
        assert!(fixture.app.world().resource::<Navigations>().0.is_empty());
        assert_eq!(
            fixture
                .dispatch("getAll", json!([{}]))
                .result
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn modal_keeps_the_browser_window_available_for_current_tab_queries() {
        let mut fixture = Fixture::new();
        fixture.open(&format!("chrome-extension://{ID}/popup/index.html#/unlock"));
        let world = fixture.app.world_mut();
        let mut query = world.query::<&mut ExtensionModel>();
        query.single_mut(world).unwrap().windows[0].focused = false;
        let all = fixture.dispatch("getAll", json!([{ "populate": true }]));
        let browser = all
            .result
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["type"] == "normal")
            .unwrap();
        assert_eq!(browser["focused"], true);
        assert_eq!(browser["tabs"][0]["url"], "https://accounts.google.com/");
    }

    #[test]
    fn extension_without_a_toolbar_document_can_open_an_authentication_modal() {
        let mut fixture = Fixture::new();
        let world = fixture.app.world_mut();
        let mut query = world.query::<&mut ExtensionCatalog>();
        let mut catalog = query.single_mut(world).unwrap();
        let mut entry = catalog.enabled(ID).unwrap().clone();
        entry.popup = None;
        catalog.replace(ExtensionsUiState {
            extensions: vec![entry],
            ..default()
        });
        fixture.open(&format!(
            "chrome-extension://{ID}/authentication.html#/unlock"
        ));
        let popup = fixture.popup();
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_browser::PopupWebview>(popup)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<vmux_layout::stack::OpenRequest>>()
                .is_empty()
        );
    }

    #[test]
    fn native_escape_dismisses_the_modal_and_its_virtual_window() {
        let mut fixture = Fixture::new();
        let id = fixture.open(&format!("chrome-extension://{ID}/popup/index.html#/unlock"));
        let popup = fixture.popup();
        fixture.app.world_mut().trigger(Receive {
            webview: popup,
            payload: DismissRequest {
                channel: "vmux-extension-popup-close".into(),
                window_id: id,
            },
        });
        fixture.app.update();
        assert!(fixture.app.world().get_entity(popup).is_err());
        assert!(fixture.app.world().get::<Stack>(fixture.stack).is_some());
        assert_eq!(
            fixture
                .dispatch("getAll", json!([{}]))
                .result
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn toolbar_popup_publishes_its_window_lifecycle() {
        let mut fixture = Fixture::new();
        let mut cursor = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>()
            .get_cursor();
        fixture.reopen();
        let popup = fixture.popup();
        let id = fixture
            .app
            .world()
            .get::<ExtensionWindowId>(popup)
            .unwrap()
            .0;
        let messages = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>();
        assert!(cursor.read(messages).any(
            |event| matches!(event, ExtensionModelEvent::WindowCreated(window) if window.id == id)
        ));
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.layout,
            payload: ExtensionPopupCloseRequest,
        });
        fixture.app.update();
        let messages = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>();
        assert!(cursor.read(messages).any(|event| matches!(event, ExtensionModelEvent::WindowRemoved { window_id } if *window_id == id)));
    }

    #[test]
    fn disposing_the_layout_retires_its_modal_and_virtual_window() {
        let mut fixture = Fixture::new();
        fixture.open(&format!("chrome-extension://{ID}/popup/index.html#/unlock"));
        let popup = fixture.popup();
        fixture.app.world_mut().despawn(fixture.layout);
        fixture.app.update();
        assert!(fixture.app.world().get_entity(popup).is_err());
        assert_eq!(
            fixture
                .dispatch("getAll", json!([{}]))
                .result
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn closing_the_parent_returns_window_focus_to_an_existing_window() {
        let mut fixture = Fixture::new();
        fixture.open(&format!("chrome-extension://{ID}/popup/index.html#/unlock"));
        let world = fixture.app.world_mut();
        let mut query = world.query::<&mut ExtensionModel>();
        let mut model = query.single_mut(world).unwrap();
        model.windows[0].id = 2;
        let mut cursor = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>()
            .get_cursor();
        fixture.app.world_mut().despawn(fixture.layout);
        fixture.app.update();
        let messages = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>();
        let focused = cursor.read(messages).find_map(|event| match event {
            ExtensionModelEvent::WindowFocusChanged { window_id } => Some(*window_id),
            _ => None,
        });
        assert_eq!(focused, Some(2));
    }

    #[test]
    fn requests_in_one_frame_keep_one_popup_entity_and_the_latest_window() {
        let mut fixture = Fixture::new();
        let mut latest = 0;
        for path in ["popup/index.html#/unlock", "authentication/confirm.html"] {
            let dispatch = fixture.dispatch(
                "create",
                json!([{ "type": "popup", "url": format!("chrome-extension://{ID}/{path}") }]),
            );
            latest = dispatch.result["id"].as_i64().unwrap() as i32;
            fixture
                .app
                .world_mut()
                .write_message(dispatch.open_window.unwrap());
        }
        fixture.app.update();
        let popup = fixture.popup();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ExtensionWindowId>(popup)
                .unwrap()
                .0,
            latest
        );
        assert_eq!(
            fixture
                .dispatch("getAll", json!([{}]))
                .result
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn removing_native_popup_preserves_matching_stack_and_closes_only_its_extra_tab() {
        let mut fixture = Fixture::new();
        let popup_url = format!("chrome-extension://{ID}/popup/index.html");
        fixture
            .app
            .world_mut()
            .get_mut::<vmux_ecs::PageMetadata>(fixture.stack)
            .unwrap()
            .url = popup_url.clone();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.stack)
            .insert(crate::host::model::ExtensionTabId(7));
        let extra_url = "https://example.com/confirmation";
        let extra = fixture
            .app
            .world_mut()
            .spawn((
                Stack::default(),
                vmux_ecs::PageMetadata {
                    url: extra_url.into(),
                    ..Default::default()
                },
                crate::host::model::ExtensionTabId(8),
            ))
            .id();
        {
            let world = fixture.app.world_mut();
            let mut model = world
                .query::<&mut ExtensionModel>()
                .single_mut(world)
                .unwrap();
            model.tabs[0].url = popup_url.clone();
            let mut tab = model.tabs[0].clone();
            tab.id = 8;
            tab.url = extra_url.into();
            model.tabs.push(tab);
        }
        let dispatch = fixture.dispatch(
            "create",
            json!([{ "type": "popup", "url": [popup_url, extra_url] }]),
        );
        let id = dispatch.result["id"].as_i64().unwrap();
        fixture
            .app
            .world_mut()
            .write_message(dispatch.open_window.unwrap());
        fixture.app.update();
        let removed = fixture.dispatch("remove", json!([id]));
        fixture
            .app
            .world_mut()
            .write_message(removed.close_window.unwrap());
        fixture.app.update();
        let closed: Vec<_> = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .drain()
            .map(|request| request.stack)
            .collect();
        assert_eq!(closed, [extra]);
        assert!(fixture.app.world().get_entity(fixture.stack).is_ok());
    }

    #[test]
    fn late_close_of_replaced_authentication_cannot_dismiss_the_current_popup() {
        let mut fixture = Fixture::new();
        let first = fixture.open(&format!("chrome-extension://{ID}/popup/index.html#/unlock"));
        let popup = fixture.popup();
        let next_url =
            format!("chrome-extension://{ID}/popup/index.html?singleActionPopout=fido#/fido2");
        let next = fixture.open(&next_url);
        assert_eq!(fixture.popup(), popup);
        assert_eq!(
            fixture.app.world().resource::<Navigations>().0,
            vec![(popup, next_url)]
        );
        fixture
            .app
            .world_mut()
            .write_message(CloseRequest { window_id: first });
        fixture.app.update();
        assert_eq!(fixture.popup(), popup);
        fixture.app.world_mut().trigger(Receive {
            webview: popup,
            payload: DismissRequest {
                channel: "vmux-extension-popup-close".into(),
                window_id: first,
            },
        });
        fixture.app.update();
        assert_eq!(fixture.popup(), popup);
        let removed = fixture.dispatch("remove", json!([next]));
        fixture
            .app
            .world_mut()
            .write_message(removed.close_window.unwrap());
        fixture.app.update();
        assert!(fixture.app.world().get_entity(popup).is_err());
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<CloseStackRequest>>()
                .is_empty()
        );
        assert_eq!(
            fixture
                .dispatch("getAll", json!([{}]))
                .result
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn popup_cannot_navigate_to_another_extension_or_a_web_origin() {
        let mut fixture = Fixture::new();
        let url = format!("chrome-extension://{ID}/popup/index.html#/unlock");
        let id = fixture.open(&url);
        let popup = fixture.popup();
        for target in [
            "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/popup.html",
            "https://example.com/",
        ] {
            fixture.app.world_mut().write_message(OpenRequest {
                extension_id: ID.into(),
                url: target.into(),
                window_id: id,
                host_window_id: Some(1),
            });
            fixture.app.update();
            assert_eq!(fixture.popup(), popup);
            assert_eq!(
                fixture
                    .app
                    .world()
                    .get::<vmux_ecs::PageMetadata>(popup)
                    .unwrap()
                    .url,
                url
            );
            assert!(fixture.app.world().resource::<Navigations>().0.is_empty());
        }
    }
}
