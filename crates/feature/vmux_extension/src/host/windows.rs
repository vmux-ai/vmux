use crate::protocol::{ApiRequest, ExtensionApiError, ExtensionCallerContext};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowPosition};
use bevy_cef::prelude::RequestNavigate;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};
use vmux_ecs::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_layout::stack::{CloseStackRequest, Stack};

use super::bridge::BridgeAuthorization;
use super::model::ExtensionPopup;
use super::model::{
    ExtensionModel, ExtensionModelEvent, ExtensionTabId, ExtensionTabSnapshot, ExtensionWindowId,
    ExtensionWindowSnapshot,
};

pub(crate) struct ExtensionWindowsPlugin;

impl Plugin for ExtensionWindowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_message::<OpenExtensionWindowRequest>()
            .add_message::<CloseExtensionWindowRequest>()
            .add_message::<UpdateHostWindowRequest>()
            .add_systems(
                Update,
                sync.in_set(super::ExtensionSystemSet::SyncWindows)
                    .after(super::project::ExtensionProjectionSet),
            )
            .add_systems(
                Update,
                (open, route_close, host_update).after(super::ExtensionSystemSet::DrainBridge),
            );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Extension windows"), ExtensionWindows::default()));
}

pub const WINDOW_ID_NONE: i32 = -1;
pub const WINDOW_ID_CURRENT: i32 = -2;
const FIRST_EXTENSION_WINDOW_ID: i32 = 1_000_000_000;
const FALLBACK_HOST_WINDOW_ID: i32 = 1;

#[derive(Clone, Debug)]
struct ExtensionWindow {
    window: ExtensionWindowSnapshot,
    urls: Vec<String>,
    tab_ids: Vec<i32>,
}

#[derive(Component)]
pub struct ExtensionWindows {
    next_id: i32,
    windows: BTreeMap<i32, ExtensionWindow>,
    last_focused: Option<i32>,
}

impl Default for ExtensionWindows {
    fn default() -> Self {
        Self {
            next_id: FIRST_EXTENSION_WINDOW_ID,
            windows: BTreeMap::new(),
            last_focused: None,
        }
    }
}

impl ExtensionWindows {
    fn contains_id(&self, id: i32, model: &ExtensionModel) -> bool {
        self.windows.contains_key(&id) || model.windows.iter().any(|window| window.id == id)
    }

    fn current_id(&self, caller: &ExtensionCallerContext, model: &ExtensionModel) -> Option<i32> {
        if let Some(caller_url) = caller.url()
            && let Some(id) = self.windows.iter().find_map(|(id, window)| {
                window
                    .urls
                    .iter()
                    .any(|url| ExtensionWindow::page_matches(url, caller_url))
                    .then_some(*id)
            })
        {
            return Some(id);
        }
        self.windows
            .values()
            .find(|window| window.window.focused)
            .map(|window| window.window.id)
            .or_else(|| model.focused_window_id())
            .or_else(|| model.windows.first().map(|window| window.id))
            .or(Some(FALLBACK_HOST_WINDOW_ID))
    }

    fn resolve_id(
        &self,
        id: i32,
        caller: &ExtensionCallerContext,
        model: &ExtensionModel,
    ) -> Result<i32, ExtensionApiError> {
        if matches!(id, WINDOW_ID_NONE | WINDOW_ID_CURRENT) {
            return self.current_id(caller, model).ok_or_else(|| {
                ExtensionApiError::new("window_not_found", "current window is unavailable")
            });
        }
        if id < 0 {
            return Err(ExtensionApiError::new(
                "invalid_arguments",
                "windowId is invalid",
            ));
        }
        Ok(id)
    }

    fn resolve_native_alias(
        &self,
        id: i32,
        model: &ExtensionModel,
    ) -> Result<i32, ExtensionApiError> {
        if self.contains_id(id, model) {
            return Ok(id);
        }
        if id >= FIRST_EXTENSION_WINDOW_ID && id < self.next_id {
            return Err(ExtensionApiError::new(
                "window_not_found",
                "extension window is unavailable",
            ));
        }
        Ok(model
            .focused_window_id()
            .or_else(|| model.windows.first().map(|window| window.id))
            .unwrap_or(id))
    }
}

impl ExtensionWindow {
    fn same_document(expected: &str, actual: &str) -> bool {
        let (Ok(mut expected), Ok(mut actual)) =
            (url::Url::parse(expected), url::Url::parse(actual))
        else {
            return expected == actual;
        };
        expected.set_fragment(None);
        actual.set_fragment(None);
        expected == actual
    }

    fn page_matches(expected: &str, actual: &str) -> bool {
        if Self::same_document(expected, actual) {
            return true;
        }
        let (Ok(expected), Ok(actual)) = (url::Url::parse(expected), url::Url::parse(actual))
        else {
            return false;
        };
        expected.scheme() == "chrome-extension"
            && expected.scheme() == actual.scheme()
            && expected.host_str() == actual.host_str()
            && expected.path() == actual.path()
    }

    fn tabs(&self, model: &ExtensionModel) -> Vec<ExtensionTabSnapshot> {
        self.tab_ids
            .iter()
            .filter_map(|id| model.tabs.iter().find(|tab| tab.id == *id).cloned())
            .collect()
    }

    fn refresh_tabs(&mut self, model: &ExtensionModel, claimed: &mut HashSet<i32>) {
        self.tab_ids
            .retain(|id| model.tabs.iter().any(|tab| tab.id == *id));
        claimed.extend(self.tab_ids.iter().copied());
        for url in &self.urls {
            let exact = model
                .tabs
                .iter()
                .find(|tab| !claimed.contains(&tab.id) && Self::same_document(url, &tab.url));
            if let Some(tab) = exact {
                self.tab_ids.push(tab.id);
                claimed.insert(tab.id);
            }
        }
        self.tab_ids.sort_unstable();
        self.tab_ids.dedup();
    }
}

impl ExtensionModel {
    fn focused_window_id(&self) -> Option<i32> {
        self.windows
            .iter()
            .find(|window| window.focused)
            .map(|window| window.id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostWindowUpdate {
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub focused: Option<bool>,
    pub draw_attention: Option<bool>,
    pub state: Option<String>,
}

impl HostWindowUpdate {
    fn validate(&self) -> Result<(), ExtensionApiError> {
        if let Some(state) = self.state.as_deref() {
            Self::validate_state(state)?;
            if state != "normal"
                && (self.left.is_some()
                    || self.top.is_some()
                    || self.width.is_some()
                    || self.height.is_some())
            {
                return Err(ExtensionApiError::new(
                    "invalid_arguments",
                    "window bounds cannot be combined with this state",
                ));
            }
            if state == "minimized" && self.focused == Some(true) {
                return Err(ExtensionApiError::new(
                    "invalid_arguments",
                    "a minimized window cannot be focused",
                ));
            }
            if matches!(state, "fullscreen" | "maximized") && self.focused == Some(false) {
                return Err(ExtensionApiError::new(
                    "invalid_arguments",
                    "a fullscreen or maximized window cannot be unfocused",
                ));
            }
        }
        Ok(())
    }

    fn apply_to(&self, window: &mut ExtensionWindowSnapshot) {
        if let Some(left) = self.left {
            window.left = left;
        }
        if let Some(top) = self.top {
            window.top = top;
        }
        if let Some(width) = self.width {
            window.width = width as i32;
        }
        if let Some(height) = self.height {
            window.height = height as i32;
        }
        if let Some(focused) = self.focused {
            window.focused = focused;
        }
        if let Some(state) = &self.state {
            window.state.clone_from(state);
        }
    }

    fn validate_state(state: &str) -> Result<(), ExtensionApiError> {
        if matches!(state, "normal" | "minimized" | "maximized" | "fullscreen") {
            Ok(())
        } else {
            Err(ExtensionApiError::new(
                "invalid_arguments",
                "window state is invalid",
            ))
        }
    }
}

struct WindowCreateOptions {
    fields: Map<String, Value>,
    extension_id: String,
}

impl WindowCreateOptions {
    fn from_request(request: &ApiRequest) -> Self {
        Self {
            fields: request
                .argument(0)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            extension_id: request.caller_context.extension_id().to_string(),
        }
    }

    fn validate(&self) -> Result<(), ExtensionApiError> {
        if self.boolean("incognito") == Some(true) {
            return Err(ExtensionApiError::new(
                "unsupported_option",
                "incognito extension windows are unavailable",
            ));
        }
        let state = self.state();
        HostWindowUpdate::validate_state(state)?;
        if state != "normal"
            && ["left", "top", "width", "height"]
                .iter()
                .any(|key| self.fields.contains_key(*key))
        {
            return Err(ExtensionApiError::new(
                "invalid_arguments",
                "window bounds cannot be combined with this state",
            ));
        }
        if !matches!(self.window_type(), "normal" | "popup" | "panel") {
            return Err(ExtensionApiError::new(
                "invalid_arguments",
                "extension window type is invalid",
            ));
        }
        if self.fields.contains_key("tabId") {
            return Err(ExtensionApiError::new(
                "unsupported_option",
                "moving an existing tab into an extension window is unavailable",
            ));
        }
        Ok(())
    }

    fn urls(&self) -> Result<Vec<String>, ExtensionApiError> {
        match self.fields.get("url") {
            Some(Value::String(url)) => Ok(vec![self.resolve_url(url)?]),
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| {
                            ExtensionApiError::new("invalid_arguments", "window URL is invalid")
                        })
                        .and_then(|url| self.resolve_url(url))
                })
                .collect(),
            Some(_) => Err(ExtensionApiError::new(
                "invalid_arguments",
                "window URL is invalid",
            )),
            None => Ok(Vec::new()),
        }
    }

    fn resolve_url(&self, url: &str) -> Result<String, ExtensionApiError> {
        let parsed = url::Url::parse(url).or_else(|_| {
            url::Url::parse(&format!("chrome-extension://{}/", self.extension_id))?.join(url)
        });
        let parsed =
            parsed.map_err(|_| ExtensionApiError::new("invalid_url", "window URL is invalid"))?;
        match parsed.scheme() {
            "http" | "https" => {}
            "chrome-extension" if parsed.host_str() == Some(&self.extension_id) => {}
            _ => {
                return Err(ExtensionApiError::new(
                    "invalid_url",
                    "window URL uses an unsupported scheme",
                ));
            }
        }
        Ok(parsed.to_string())
    }

    fn boolean(&self, key: &str) -> Option<bool> {
        self.fields.get(key).and_then(Value::as_bool)
    }

    fn integer(&self, key: &str) -> Option<i32> {
        self.fields
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
    }

    fn positive_integer(&self, key: &str) -> Option<u32> {
        self.fields
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    }

    fn focused(&self) -> bool {
        self.boolean("focused").unwrap_or(true)
    }

    fn window_type(&self) -> &str {
        self.fields
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("normal")
    }

    fn state(&self) -> &str {
        self.fields
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("normal")
    }
}

impl ExtensionWindowSnapshot {
    fn fallback(id: i32) -> Self {
        Self {
            id,
            focused: true,
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
            incognito: false,
            window_type: "normal".into(),
            state: "normal".into(),
            always_on_top: false,
        }
    }

    fn disclosed_value(
        &self,
        tabs: Vec<ExtensionTabSnapshot>,
        populate: bool,
        request: &ApiRequest,
        authorization: &BridgeAuthorization,
    ) -> Value {
        let mut value = serde_json::to_value(self).expect("extension window serializes");
        if populate {
            value.as_object_mut().expect("window object").insert(
                "tabs".into(),
                Value::Array(
                    tabs.into_iter()
                        .enumerate()
                        .map(|(index, tab)| {
                            tab.disclosed_value(self.id, index as u32, request, authorization)
                        })
                        .collect(),
                ),
            );
        }
        value
    }

    fn matches_type(&self, options: Option<&Value>) -> bool {
        options
            .and_then(|options| options.get("windowTypes"))
            .and_then(Value::as_array)
            .is_none_or(|types| {
                types
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|window_type| window_type == self.window_type)
            })
    }

    fn events_since(&self, before: &Self) -> Vec<ExtensionModelEvent> {
        let mut events = Vec::new();
        if before.left != self.left
            || before.top != self.top
            || before.width != self.width
            || before.height != self.height
        {
            events.push(ExtensionModelEvent::WindowBoundsChanged(self.clone()));
        }
        if before.focused != self.focused {
            events.push(ExtensionModelEvent::WindowFocusChanged {
                window_id: if self.focused {
                    self.id
                } else {
                    WINDOW_ID_NONE
                },
            });
        }
        events
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDispatch {
    pub result: Value,
    pub open_window: Option<OpenExtensionWindowRequest>,
    pub close_window: Option<CloseExtensionWindowRequest>,
    pub update_host_window: Option<UpdateHostWindowRequest>,
    pub events: Vec<ExtensionModelEvent>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenExtensionWindowRequest {
    pub extension_id: String,
    pub urls: Vec<Option<String>>,
    pub window_type: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct CloseExtensionWindowRequest {
    pub tab_ids: Vec<i32>,
    pub urls: Vec<String>,
}

impl CloseExtensionWindowRequest {
    fn matches_document(&self, actual: &str) -> bool {
        self.urls
            .iter()
            .any(|expected| ExtensionWindow::same_document(expected, actual))
    }

    fn matches_page(&self, actual: &str) -> bool {
        self.urls
            .iter()
            .any(|expected| ExtensionWindow::page_matches(expected, actual))
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct UpdateHostWindowRequest {
    pub window_id: i32,
    pub update: HostWindowUpdate,
}

fn open(
    mut requests: MessageReader<OpenExtensionWindowRequest>,
    popups: Query<(Entity, &ExtensionPopup)>,
    mut commands: Commands,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    for request in requests.read() {
        let popup = if request.window_type == "popup" {
            popups.iter().find_map(|(entity, popup)| {
                (popup.extension_id == request.extension_id).then_some(entity)
            })
        } else {
            None
        };
        let mut urls = request.urls.iter().cloned();
        if let Some(popup) = popup
            && let Some(Some(url)) = urls.next()
        {
            commands.trigger(RequestNavigate {
                webview: popup,
                url,
            });
        }
        for url in urls {
            stack_requests.write(vmux_layout::stack::OpenRequest { url });
        }
    }
}

impl WindowDispatch {
    fn success(result: Value) -> Self {
        Self {
            result,
            open_window: None,
            close_window: None,
            update_host_window: None,
            events: Vec::new(),
        }
    }

    pub fn from_request(
        request: &ApiRequest,
        model: &ExtensionModel,
        windows: &mut ExtensionWindows,
        authorization: &BridgeAuthorization,
    ) -> Result<Self, ExtensionApiError> {
        match request.method.as_str() {
            "get" => get(request, model, windows, authorization),
            "getCurrent" => get_current(request, model, windows, authorization),
            "getLastFocused" => get_last_focused(request, model, windows, authorization),
            "getAll" => get_all(request, model, windows, authorization),
            "create" => create(request, model, windows, authorization),
            "update" => update(request, model, windows, authorization),
            "remove" => remove(request, model, windows),
            _ => Err(ExtensionApiError::new(
                "unsupported_api",
                format!("windows.{} is not supported", request.method),
            )),
        }
    }
}

fn route_close(
    mut requests: MessageReader<CloseExtensionWindowRequest>,
    tab_ids: Query<(Entity, &ExtensionTabId)>,
    stacks: Query<(Entity, &PageMetadata, Option<&LastActivatedAt>), With<Stack>>,
    mut close_requests: MessageWriter<CloseStackRequest>,
) {
    for request in requests.read() {
        let mut targets = HashSet::new();
        for tab_id in &request.tab_ids {
            if let Some(entity) = tab_ids
                .iter()
                .find_map(|(entity, id)| (id.0 == *tab_id).then_some(entity))
                && stacks.contains(entity)
            {
                targets.insert(entity);
            }
        }
        if targets.is_empty() {
            for (entity, metadata, _) in &stacks {
                if request.matches_document(&metadata.url) {
                    targets.insert(entity);
                }
            }
        }
        if targets.is_empty()
            && let Some(entity) = stacks
                .iter()
                .filter(|(_, metadata, _)| request.matches_page(&metadata.url))
                .max_by_key(|(_, _, activated)| activated.map_or(0, |activated| activated.0))
                .map(|(entity, _, _)| entity)
        {
            targets.insert(entity);
        }
        for stack in targets {
            close_requests.write(CloseStackRequest::tidying(stack));
        }
    }
}

fn sync(model: Single<Ref<ExtensionModel>>, mut windows: Single<&mut ExtensionWindows>) {
    if !model.is_changed() {
        return;
    }
    let mut claimed = HashSet::new();
    for window in windows.windows.values_mut() {
        window.refresh_tabs(&model, &mut claimed);
    }
}

fn host_update(
    mut requests: MessageReader<UpdateHostWindowRequest>,
    mut native_windows: Query<(&ExtensionWindowId, &mut Window)>,
) {
    for request in requests.read() {
        let Some((_, mut window)) = native_windows
            .iter_mut()
            .find(|(id, _)| id.0 == request.window_id)
        else {
            continue;
        };
        if request.update.left.is_some() || request.update.top.is_some() {
            let current = match window.position {
                WindowPosition::At(position) => position,
                _ => IVec2::ZERO,
            };
            window.position = WindowPosition::At(IVec2::new(
                request.update.left.unwrap_or(current.x),
                request.update.top.unwrap_or(current.y),
            ));
        }
        if request.update.width.is_some() || request.update.height.is_some() {
            let width = request
                .update
                .width
                .map_or(window.resolution.width(), |width| width as f32);
            let height = request
                .update
                .height
                .map_or(window.resolution.height(), |height| height as f32);
            window.resolution.set(width, height);
        }
        match request.update.state.as_deref() {
            Some("fullscreen") => {
                window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
            }
            Some("normal") => window.mode = WindowMode::Windowed,
            _ => {}
        }
    }
}

fn get(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ExtensionApiError> {
    let id = request
        .argument(0)
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "windowId is required"))?;
    let options = request.argument(1);
    let id = windows.resolve_id(id, &request.caller_context, model)?;
    let id = windows.resolve_native_alias(id, model)?;
    let result = windows.value_by_id(id, options, model, request, authorization)?;
    Ok(WindowDispatch::success(result))
}

fn get_current(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ExtensionApiError> {
    let id = windows
        .current_id(&request.caller_context, model)
        .ok_or_else(|| {
            ExtensionApiError::new("window_not_found", "current window is unavailable")
        })?;
    let result = windows.value_by_id(id, request.argument(0), model, request, authorization)?;
    Ok(WindowDispatch::success(result))
}

fn get_last_focused(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ExtensionApiError> {
    let id = windows
        .last_focused
        .filter(|id| windows.contains_id(*id, model))
        .or_else(|| model.focused_window_id())
        .or_else(|| model.windows.first().map(|window| window.id))
        .unwrap_or(FALLBACK_HOST_WINDOW_ID);
    let result = windows.value_by_id(id, request.argument(0), model, request, authorization)?;
    Ok(WindowDispatch::success(result))
}

fn get_all(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ExtensionApiError> {
    let options = request.argument(0);
    let populate = options
        .and_then(|options| options.get("populate"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut claimed = HashSet::new();
    let ids = windows.windows.keys().copied().collect::<Vec<_>>();
    let mut virtual_values = Vec::new();
    for id in ids {
        let extension_window = windows
            .windows
            .get_mut(&id)
            .expect("known extension window");
        extension_window.refresh_tabs(model, &mut claimed);
        if extension_window.window.matches_type(options) {
            virtual_values.push(extension_window.window.disclosed_value(
                extension_window.tabs(model),
                populate,
                request,
                authorization,
            ));
        }
    }
    let mut values = model
        .windows
        .iter()
        .filter(|window| window.matches_type(options))
        .map(|window| {
            window.disclosed_value(
                model
                    .tabs
                    .iter()
                    .filter(|tab| tab.window_id == window.id && !claimed.contains(&tab.id))
                    .cloned()
                    .collect(),
                populate,
                request,
                authorization,
            )
        })
        .collect::<Vec<_>>();
    values.extend(virtual_values);
    Ok(WindowDispatch::success(Value::Array(values)))
}

fn create(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ExtensionApiError> {
    let options = WindowCreateOptions::from_request(request);
    options.validate()?;
    let urls = options.urls()?;
    let base = model
        .windows
        .iter()
        .find(|window| window.focused)
        .or_else(|| model.windows.first());
    let focused = options.focused();
    let window_type = options.window_type();
    let state = options.state();
    let id = windows.next_id;
    windows.next_id = windows
        .next_id
        .saturating_add(1)
        .max(FIRST_EXTENSION_WINDOW_ID);
    if focused {
        for entry in windows.windows.values_mut() {
            entry.window.focused = false;
        }
        windows.last_focused = Some(id);
    }
    let window = ExtensionWindowSnapshot {
        id,
        focused,
        left: options
            .integer("left")
            .or_else(|| base.map(|window| window.left))
            .unwrap_or(0),
        top: options
            .integer("top")
            .or_else(|| base.map(|window| window.top))
            .unwrap_or(0),
        width: options
            .positive_integer("width")
            .map(|value| value as i32)
            .or_else(|| base.map(|window| window.width))
            .unwrap_or(800),
        height: options
            .positive_integer("height")
            .map(|value| value as i32)
            .or_else(|| base.map(|window| window.height))
            .unwrap_or(600),
        incognito: false,
        window_type: window_type.into(),
        state: state.into(),
        always_on_top: false,
    };
    let extension_window = ExtensionWindow {
        window: window.clone(),
        urls: urls.clone(),
        tab_ids: Vec::new(),
    };
    let result = window.disclosed_value(Vec::new(), true, request, authorization);
    windows.windows.insert(id, extension_window);
    let mut events = vec![ExtensionModelEvent::WindowCreated(window)];
    if focused {
        events.push(ExtensionModelEvent::WindowFocusChanged { window_id: id });
    }
    let open = if urls.is_empty() {
        vec![None]
    } else {
        urls.into_iter().map(Some).collect()
    };
    Ok(WindowDispatch {
        result,
        open_window: Some(OpenExtensionWindowRequest {
            extension_id: request.caller_context.extension_id().to_string(),
            urls: open,
            window_type: window_type.into(),
        }),
        close_window: None,
        update_host_window: None,
        events,
    })
}

fn update(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ExtensionApiError> {
    let requested_id = request
        .argument(0)
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "windowId is required"))?;
    let update_value = request
        .argument(1)
        .cloned()
        .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "updateInfo is required"))?;
    let update: HostWindowUpdate = serde_json::from_value(update_value)
        .map_err(|_| ExtensionApiError::new("invalid_arguments", "updateInfo is invalid"))?;
    update.validate()?;
    let id = windows.resolve_id(requested_id, &request.caller_context, model)?;
    if windows.windows.contains_key(&id) {
        let was_focused = windows.windows[&id].window.focused;
        if update.focused == Some(true) {
            for entry in windows.windows.values_mut() {
                entry.window.focused = entry.window.id == id;
            }
            windows.last_focused = Some(id);
        }
        let entry = windows
            .windows
            .get_mut(&id)
            .expect("known extension window");
        let before = entry.window.clone();
        update.apply_to(&mut entry.window);
        let mut events = entry.window.events_since(&before);
        if update.focused == Some(false) && was_focused {
            entry.window.focused = false;
            let fallback = model.focused_window_id().unwrap_or(WINDOW_ID_NONE);
            windows.last_focused = (fallback >= 0).then_some(fallback);
            events.push(ExtensionModelEvent::WindowFocusChanged {
                window_id: fallback,
            });
        }
        return Ok(WindowDispatch {
            result: entry
                .window
                .disclosed_value(entry.tabs(model), true, request, authorization),
            open_window: None,
            close_window: None,
            update_host_window: None,
            events,
        });
    }
    let before = model
        .windows
        .iter()
        .find(|window| window.id == id)
        .cloned()
        .ok_or_else(|| ExtensionApiError::new("window_not_found", "window is unavailable"))?;
    let mut after = before.clone();
    update.apply_to(&mut after);
    if update.focused == Some(true) {
        windows.last_focused = Some(id);
    }
    Ok(WindowDispatch {
        result: after.disclosed_value(
            model
                .tabs
                .iter()
                .filter(|tab| tab.window_id == id)
                .cloned()
                .collect(),
            true,
            request,
            authorization,
        ),
        open_window: None,
        close_window: None,
        update_host_window: Some(UpdateHostWindowRequest {
            window_id: id,
            update,
        }),
        events: after.events_since(&before),
    })
}

fn remove(
    request: &ApiRequest,
    model: &ExtensionModel,
    windows: &mut ExtensionWindows,
) -> Result<WindowDispatch, ExtensionApiError> {
    let id = request
        .argument(0)
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "windowId is required"))?;
    let mut entry = windows.windows.remove(&id).ok_or_else(|| {
        ExtensionApiError::new("window_not_found", "extension window is unavailable")
    })?;
    entry.refresh_tabs(model, &mut HashSet::new());
    let mut events = vec![ExtensionModelEvent::WindowRemoved { window_id: id }];
    if entry.window.focused {
        let fallback = model.focused_window_id().unwrap_or(WINDOW_ID_NONE);
        windows.last_focused = (fallback >= 0).then_some(fallback);
        events.push(ExtensionModelEvent::WindowFocusChanged {
            window_id: fallback,
        });
    }
    Ok(WindowDispatch {
        result: Value::Null,
        open_window: None,
        close_window: Some(CloseExtensionWindowRequest {
            tab_ids: entry.tab_ids,
            urls: entry.urls,
        }),
        update_host_window: None,
        events,
    })
}

impl ExtensionWindows {
    fn value_by_id(
        &mut self,
        id: i32,
        options: Option<&Value>,
        model: &ExtensionModel,
        request: &ApiRequest,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let populate = options
            .and_then(|options| options.get("populate"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(window) = self.windows.get_mut(&id) {
            window.refresh_tabs(model, &mut HashSet::new());
            if !window.window.matches_type(options) {
                return Err(ExtensionApiError::new(
                    "window_not_found",
                    "window type is filtered out",
                ));
            }
            return Ok(window.window.disclosed_value(
                window.tabs(model),
                populate,
                request,
                authorization,
            ));
        }
        let fallback;
        let window = if let Some(window) = model.windows.iter().find(|window| window.id == id) {
            window
        } else {
            fallback = ExtensionWindowSnapshot::fallback(id);
            &fallback
        };
        if !window.matches_type(options) {
            return Err(ExtensionApiError::new(
                "window_not_found",
                "window type is filtered out",
            ));
        }
        Ok(window.disclosed_value(
            model
                .tabs
                .iter()
                .filter(|tab| tab.window_id == id)
                .cloned()
                .collect(),
            populate,
            request,
            authorization,
        ))
    }
}

impl ExtensionModelEvent {
    pub fn window_payload(&self) -> Option<(&'static str, Value)> {
        match self {
            Self::WindowCreated(window) => Some((
                "onCreated",
                json!([serde_json::to_value(window).expect("extension window serializes")]),
            )),
            Self::WindowRemoved { window_id } => Some(("onRemoved", json!([window_id]))),
            Self::WindowFocusChanged { window_id } => Some(("onFocusChanged", json!([window_id]))),
            Self::WindowBoundsChanged(window) => Some((
                "onBoundsChanged",
                json!([serde_json::to_value(window).expect("extension window serializes")]),
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn request(method: &str, arguments: Value) -> ApiRequest {
        ApiRequest {
            request_id: method.into(),
            namespace: "windows".into(),
            method: method.into(),
            arguments,
            caller_context: ExtensionCallerContext::ServiceWorker {
                extension_id: EXTENSION_ID.into(),
                context_id: "worker".into(),
                url: Some(format!("chrome-extension://{EXTENSION_ID}/background.js")),
            },
        }
    }

    fn model() -> ExtensionModel {
        ExtensionModel {
            windows: vec![ExtensionWindowSnapshot {
                id: 1,
                focused: true,
                left: 10,
                top: 20,
                width: 1200,
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
                url: "https://example.com/".into(),
                title: "Example".into(),
                status: "complete".into(),
            }],
        }
    }

    #[test]
    fn creates_queries_updates_and_removes_extension_window() {
        let mut model = model();
        let mut windows = ExtensionWindows::default();
        let popout_url = format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/fido2");
        let created = WindowDispatch::from_request(
            &request(
                "create",
                json!([{
                    "url": popout_url,
                    "type": "popup",
                    "width": 900,
                    "height": 700
                }]),
            ),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        let id = created.result["id"].as_i64().unwrap() as i32;
        assert_eq!(created.result["type"], "popup");
        assert!(created.open_window.is_some());
        model.tabs.push(ExtensionTabSnapshot {
            id: 8,
            window_id: 1,
            index: 1,
            active: true,
            highlighted: true,
            pinned: false,
            url: format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/vault"),
            title: "Bitwarden".into(),
            status: "complete".into(),
        });

        let all = WindowDispatch::from_request(
            &request("getAll", json!([{ "populate": true }])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        assert_eq!(all.result.as_array().unwrap().len(), 2);
        let virtual_window = all
            .result
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["id"] == id)
            .unwrap();
        assert_eq!(virtual_window["tabs"][0]["windowId"], id);

        let updated = WindowDispatch::from_request(
            &request("update", json!([id, { "left": 42, "focused": true }])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        assert_eq!(updated.result["left"], 42);

        let removed = WindowDispatch::from_request(
            &request("remove", json!([id])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        assert_eq!(removed.close_window.unwrap().tab_ids, vec![8]);
        assert!(matches!(
            removed.events[0],
            ExtensionModelEvent::WindowRemoved { window_id } if window_id == id
        ));
    }

    #[test]
    fn current_window_resolves_extension_page_url() {
        let model = model();
        let mut windows = ExtensionWindows::default();
        let created = WindowDispatch::from_request(
            &request(
                "create",
                json!([{ "url": format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/fido2") }]),
            ),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        let id = created.result["id"].as_i64().unwrap();
        let mut current = request("getCurrent", json!([]));
        current.caller_context = ExtensionCallerContext::ExtensionPage {
            extension_id: EXTENSION_ID.into(),
            context_id: "document".into(),
            url: format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/vault"),
            document_id: "document".into(),
        };

        let result = WindowDispatch::from_request(
            &current,
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();

        assert_eq!(result.result["id"], id);
    }

    #[test]
    fn get_maps_chromium_native_window_id_to_focused_host_window() {
        let model = model();
        let mut windows = ExtensionWindows::default();

        let result = WindowDispatch::from_request(
            &request("get", json!([1_798_152_106, { "populate": true }])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();

        assert_eq!(result.result["id"], 1);
        assert_eq!(result.result["left"], 10);
        assert_eq!(result.result["tabs"][0]["id"], 7);
    }

    #[test]
    fn get_maps_window_id_none_to_current_window_with_geometry() {
        let result = WindowDispatch::from_request(
            &request("get", json!([WINDOW_ID_NONE, { "populate": true }])),
            &model(),
            &mut ExtensionWindows::default(),
            &BridgeAuthorization::default(),
        )
        .unwrap();

        assert_eq!(result.result["id"], 1);
        assert_eq!(result.result["left"], 10);
        assert_eq!(result.result["top"], 20);
        assert_eq!(result.result["width"], 1200);
        assert_eq!(result.result["height"], 800);
        assert_eq!(result.result["tabs"][0]["id"], 7);
    }

    #[test]
    fn window_queries_return_fallback_geometry_before_host_projection() {
        let model = ExtensionModel {
            windows: Vec::new(),
            tabs: Vec::new(),
        };
        let mut windows = ExtensionWindows::default();

        let by_id = WindowDispatch::from_request(
            &request("get", json!([1_798_152_106, { "populate": true }])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        let current = WindowDispatch::from_request(
            &request("getCurrent", json!([{ "populate": true }])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();

        assert_eq!(by_id.result["id"], 1_798_152_106);
        assert_eq!(by_id.result["left"], 0);
        assert_eq!(by_id.result["top"], 0);
        assert_eq!(by_id.result["width"], 1920);
        assert_eq!(by_id.result["height"], 1080);
        assert_eq!(by_id.result["tabs"], json!([]));
        assert_eq!(current.result["id"], FALLBACK_HOST_WINDOW_ID);
        assert_eq!(current.result["width"], 1920);
    }

    #[test]
    fn populated_windows_redact_tab_details_without_permission() {
        let result = WindowDispatch::from_request(
            &request("getAll", json!([{ "populate": true }])),
            &model(),
            &mut ExtensionWindows::default(),
            &BridgeAuthorization::default(),
        )
        .unwrap();

        let tab = &result.result[0]["tabs"][0];
        assert_eq!(tab["id"], 7);
        assert!(tab.get("url").is_none());
        assert!(tab.get("title").is_none());
    }

    #[test]
    fn populated_windows_disclose_tab_details_with_tabs_permission() {
        let authorization = BridgeAuthorization {
            permissions: ["tabs".into()].into_iter().collect(),
            ..Default::default()
        };
        let result = WindowDispatch::from_request(
            &request("getAll", json!([{ "populate": true }])),
            &model(),
            &mut ExtensionWindows::default(),
            &authorization,
        )
        .unwrap();

        let tab = &result.result[0]["tabs"][0];
        assert_eq!(tab["url"], "https://example.com/");
        assert_eq!(tab["title"], "Example");
    }

    #[test]
    fn create_rejects_existing_tab_id() {
        let error = WindowDispatch::from_request(
            &request("create", json!([{ "tabId": 7 }])),
            &model(),
            &mut ExtensionWindows::default(),
            &BridgeAuthorization::default(),
        )
        .unwrap_err();

        assert_eq!(error.code, "unsupported_option");
    }

    #[test]
    fn close_fallback_selects_most_recent_matching_extension_page() {
        let mut app = App::new();
        app.add_message::<CloseExtensionWindowRequest>()
            .add_message::<CloseStackRequest>()
            .add_systems(Update, route_close);
        let older = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: format!("chrome-extension://{EXTENSION_ID}/popup/index.html#/vault"),
                    ..default()
                },
                LastActivatedAt(1),
            ))
            .id();
        let popout = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: format!("chrome-extension://{EXTENSION_ID}/popup/index.html#/fido2"),
                    ..default()
                },
                LastActivatedAt(2),
            ))
            .id();
        let mut cursor = app
            .world()
            .resource::<Messages<CloseStackRequest>>()
            .get_cursor();
        app.world_mut().write_message(CloseExtensionWindowRequest {
            tab_ids: Vec::new(),
            urls: vec![format!(
                "chrome-extension://{EXTENSION_ID}/popup/index.html?singleActionPopout=fido#/fido2"
            )],
        });

        app.update();

        let messages = app.world().resource::<Messages<CloseStackRequest>>();
        let closed = cursor
            .read(messages)
            .map(|request| request.stack)
            .collect::<Vec<_>>();
        assert_eq!(closed, vec![popout]);
        assert_ne!(closed[0], older);
    }
}
