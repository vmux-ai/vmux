use crate::protocol::{ApiRequest, ExtensionApiError, ExtensionCallerContext};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowPosition};
use serde_json::{Map, Value};
use std::collections::HashSet;
use vmux_ecs::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_layout::stack::{CloseStackRequest, Stack};

use super::bridge::BridgeAuthorization;
use super::manager_page::popup;
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
                sync.in_set(super::SyncWindowsSet)
                    .after(super::project::ExtensionProjectionSet),
            )
            .add_systems(
                Update,
                (open, route_close, host_update)
                    .in_set(WindowRequestSet)
                    .after(super::DrainBridgeSet),
            );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct WindowRequestSet;

pub const WINDOW_ID_NONE: i32 = -1;
pub const WINDOW_ID_CURRENT: i32 = -2;

#[derive(Component)]
struct ExtensionWindowSequence(i32);

impl ExtensionWindowSequence {
    const FIRST_ID: i32 = 1_000_000_000;
    const FALLBACK_HOST_ID: i32 = 1;

    fn next(&mut self) -> i32 {
        let id = self.0;
        self.0 = self.0.saturating_add(1).max(Self::FIRST_ID);
        id
    }
}

impl Default for ExtensionWindowSequence {
    fn default() -> Self {
        Self(Self::FIRST_ID)
    }
}

#[derive(Component, Default)]
struct LastFocusedExtensionWindow(Option<i32>);

#[derive(Component)]
struct VirtualExtensionWindow;

#[derive(Component, Clone, Debug)]
struct VirtualWindowSnapshot {
    focused: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    window_type: String,
    state: String,
}

impl VirtualWindowSnapshot {
    fn from_snapshot(window: &ExtensionWindowSnapshot) -> Self {
        Self {
            focused: window.focused,
            left: window.left,
            top: window.top,
            width: window.width,
            height: window.height,
            window_type: window.window_type.clone(),
            state: window.state.clone(),
        }
    }

    fn to_snapshot(&self, id: i32) -> ExtensionWindowSnapshot {
        ExtensionWindowSnapshot {
            id,
            focused: self.focused,
            left: self.left,
            top: self.top,
            width: self.width,
            height: self.height,
            incognito: false,
            window_type: self.window_type.clone(),
            state: self.state.clone(),
            always_on_top: false,
        }
    }

    fn fallback(id: i32) -> ExtensionWindowSnapshot {
        ExtensionWindowSnapshot {
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
        window: &ExtensionWindowSnapshot,
        tabs: Vec<ExtensionTabSnapshot>,
        populate: bool,
        request: &ApiRequest,
        authorization: &BridgeAuthorization,
    ) -> Value {
        let mut value = serde_json::to_value(window).expect("extension window serializes");
        if populate {
            let mut entries = Vec::with_capacity(tabs.len());
            for (index, tab) in tabs.into_iter().enumerate() {
                entries.push(tab.disclosed_value(window.id, index as u32, request, authorization));
            }
            value
                .as_object_mut()
                .expect("window object")
                .insert("tabs".into(), Value::Array(entries));
        }
        value
    }

    fn matches_type(window: &ExtensionWindowSnapshot, options: Option<&Value>) -> bool {
        let Some(types) = options
            .and_then(|options| options.get("windowTypes"))
            .and_then(Value::as_array)
        else {
            return true;
        };
        types
            .iter()
            .filter_map(Value::as_str)
            .any(|window_type| window_type == window.window_type)
    }

    fn events_since(
        before: &ExtensionWindowSnapshot,
        after: &ExtensionWindowSnapshot,
    ) -> Vec<ExtensionModelEvent> {
        let mut events = Vec::new();
        if before.left != after.left
            || before.top != after.top
            || before.width != after.width
            || before.height != after.height
        {
            events.push(ExtensionModelEvent::WindowBoundsChanged(after.clone()));
        }
        if before.focused != after.focused {
            events.push(ExtensionModelEvent::WindowFocusChanged {
                window_id: if after.focused {
                    after.id
                } else {
                    WINDOW_ID_NONE
                },
            });
        }
        events
    }
}

#[derive(Component, Clone, Copy, Debug)]
struct VirtualWindowHost(i32);

#[derive(Component, Clone, Debug)]
struct VirtualWindowUrls(Vec<String>);

impl VirtualWindowUrls {
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
}

#[derive(Component, Clone, Debug)]
struct VirtualPopupUrl(String);

#[derive(Component, Clone, Debug, Default)]
struct VirtualWindowTabs(Vec<i32>);

impl VirtualWindowTabs {
    fn snapshots(&self, model: &ExtensionModel) -> Vec<ExtensionTabSnapshot> {
        let mut tabs = Vec::new();
        for id in &self.0 {
            if let Some(tab) = model.tabs.iter().find(|tab| tab.id == *id) {
                tabs.push(tab.clone());
            }
        }
        tabs
    }

    fn refresh(
        &mut self,
        window_id: i32,
        urls: &[String],
        has_popup: bool,
        model: &ExtensionModel,
        claimed: &mut HashSet<i32>,
    ) {
        self.0
            .retain(|id| model.tabs.iter().any(|tab| tab.id == *id));
        claimed.extend(self.0.iter().copied());
        for tab in &model.tabs {
            if tab.window_id == window_id && !claimed.contains(&tab.id) {
                self.0.push(tab.id);
                claimed.insert(tab.id);
            }
        }
        for url in urls.iter().skip(usize::from(has_popup)) {
            if let Some(tab) = model.tabs.iter().find(|tab| {
                !claimed.contains(&tab.id) && VirtualWindowUrls::same_document(url, &tab.url)
            }) {
                self.0.push(tab.id);
                claimed.insert(tab.id);
            }
        }
        self.0.sort_unstable();
        self.0.dedup();
    }
}

#[derive(Message, Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateHostWindowRequest {
    pub window_id: i32,
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub focused: Option<bool>,
    pub draw_attention: Option<bool>,
    pub state: Option<String>,
}

impl UpdateHostWindowRequest {
    fn from_value(window_id: i32, value: &Value) -> Result<Self, ExtensionApiError> {
        let Some(fields) = value.as_object() else {
            return Err(ExtensionApiError::new(
                "invalid_arguments",
                "updateInfo is invalid",
            ));
        };
        let left = Self::optional_i32(fields, "left")?;
        let top = Self::optional_i32(fields, "top")?;
        let width = Self::optional_u32(fields, "width")?;
        let height = Self::optional_u32(fields, "height")?;
        let focused = Self::optional_bool(fields, "focused")?;
        let draw_attention = Self::optional_bool(fields, "drawAttention")?;
        let state = Self::optional_string(fields, "state")?;
        let update = Self {
            window_id,
            left,
            top,
            width,
            height,
            focused,
            draw_attention,
            state,
        };
        update.validate()?;
        Ok(update)
    }

    fn optional_i32(
        fields: &Map<String, Value>,
        key: &str,
    ) -> Result<Option<i32>, ExtensionApiError> {
        match fields.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .map(Some)
                .ok_or_else(|| {
                    ExtensionApiError::new("invalid_arguments", "updateInfo is invalid")
                }),
        }
    }

    fn optional_u32(
        fields: &Map<String, Value>,
        key: &str,
    ) -> Result<Option<u32>, ExtensionApiError> {
        match fields.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .map(Some)
                .ok_or_else(|| {
                    ExtensionApiError::new("invalid_arguments", "updateInfo is invalid")
                }),
        }
    }

    fn optional_bool(
        fields: &Map<String, Value>,
        key: &str,
    ) -> Result<Option<bool>, ExtensionApiError> {
        match fields.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_bool().map(Some).ok_or_else(|| {
                ExtensionApiError::new("invalid_arguments", "updateInfo is invalid")
            }),
        }
    }

    fn optional_string(
        fields: &Map<String, Value>,
        key: &str,
    ) -> Result<Option<String>, ExtensionApiError> {
        match fields.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_str()
                .map(|value| Some(value.to_string()))
                .ok_or_else(|| {
                    ExtensionApiError::new("invalid_arguments", "updateInfo is invalid")
                }),
        }
    }

    fn validate(&self) -> Result<(), ExtensionApiError> {
        let Some(state) = self.state.as_deref() else {
            return Ok(());
        };
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

    fn apply_to_virtual(&self, window: &mut VirtualWindowSnapshot) {
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

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenExtensionWindowRequest {
    pub window_id: i32,
    pub host_window_id: Option<i32>,
    pub extension_id: String,
    pub urls: Vec<Option<String>>,
    pub window_type: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct CloseExtensionWindowRequest {
    pub window_id: Option<i32>,
    pub tab_ids: Vec<i32>,
    pub urls: Vec<String>,
}

impl CloseExtensionWindowRequest {
    fn matches_document(&self, actual: &str) -> bool {
        self.urls
            .iter()
            .any(|expected| VirtualWindowUrls::same_document(expected, actual))
    }

    fn matches_page(&self, actual: &str) -> bool {
        self.urls
            .iter()
            .any(|expected| VirtualWindowUrls::page_matches(expected, actual))
    }
}

#[derive(SystemParam)]
pub(super) struct VirtualWindows<'w, 's> {
    sequence: Single<'w, 's, &'static mut ExtensionWindowSequence>,
    last_focused: Single<'w, 's, &'static mut LastFocusedExtensionWindow>,
    windows: Query<
        'w,
        's,
        (
            Entity,
            &'static ExtensionWindowId,
            &'static mut VirtualWindowSnapshot,
            Option<&'static VirtualWindowHost>,
            &'static VirtualWindowUrls,
            Option<&'static VirtualPopupUrl>,
            &'static mut VirtualWindowTabs,
        ),
        With<VirtualExtensionWindow>,
    >,
    open_window: MessageWriter<'w, OpenExtensionWindowRequest>,
    close_window: MessageWriter<'w, CloseExtensionWindowRequest>,
    update_host_window: MessageWriter<'w, UpdateHostWindowRequest>,
    events: MessageWriter<'w, ExtensionModelEvent>,
    commands: Commands<'w, 's>,
}

impl VirtualWindows<'_, '_> {
    pub(super) fn focus_popup(&mut self, id: i32) {
        let Some((_, _, snapshot, _, _, _, _)) =
            self.windows.iter().find(|(_, wid, ..)| wid.0 == id)
        else {
            return;
        };
        let changed = self.last_focused.0 != Some(id) || !snapshot.focused;
        for (_, wid, mut window, ..) in &mut self.windows {
            window.focused = wid.0 == id;
        }
        self.last_focused.0 = Some(id);
        if changed {
            self.events
                .write(ExtensionModelEvent::WindowFocusChanged { window_id: id });
        }
    }

    pub(super) fn register_popup(&mut self, url: String, host: &ExtensionWindowSnapshot) -> i32 {
        let id = self.sequence.next();
        for (_, _, mut window, ..) in &mut self.windows {
            window.focused = false;
        }
        self.last_focused.0 = Some(id);
        let mut window = host.clone();
        window.id = id;
        window.window_type = "popup".into();
        window.focused = true;
        self.commands.spawn((
            Name::new("Extension window"),
            VirtualExtensionWindow,
            ExtensionWindowId(id),
            VirtualWindowSnapshot::from_snapshot(&window),
            VirtualWindowHost(host.id),
            VirtualWindowUrls(vec![url.clone()]),
            VirtualPopupUrl(url),
            VirtualWindowTabs::default(),
        ));
        self.events
            .write(ExtensionModelEvent::WindowCreated(window));
        self.events
            .write(ExtensionModelEvent::WindowFocusChanged { window_id: id });
        id
    }

    pub(super) fn dismiss_popup(&mut self, id: i32, model: &ExtensionModel) {
        let Some((entity, _, snapshot, host, ..)) =
            self.windows.iter().find(|(_, wid, ..)| wid.0 == id)
        else {
            return;
        };
        let focused = snapshot.focused;
        let host_window_id = host.map(|host| host.0);
        self.commands.entity(entity).despawn();
        self.events
            .write(ExtensionModelEvent::WindowRemoved { window_id: id });
        if focused {
            let fallback = Self::focus_fallback(host_window_id, model);
            self.last_focused.0 = (fallback >= 0).then_some(fallback);
            self.events.write(ExtensionModelEvent::WindowFocusChanged {
                window_id: fallback,
            });
        }
    }

    pub(super) fn has_popup(&self, id: i32, extension_id: &str) -> bool {
        self.windows
            .iter()
            .find(|(_, wid, ..)| wid.0 == id)
            .is_some_and(|(_, _, snapshot, _, _, popup_url, _)| {
                snapshot.window_type == "popup"
                    && popup_url
                        .is_some_and(|url| popup::OpenRequest::owns_url(extension_id, &url.0))
            })
    }

    pub(super) fn dispatch(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<(Value, bool), ExtensionApiError> {
        match request.method.as_str() {
            "get" => self
                .get(request, model, authorization)
                .map(|value| (value, false)),
            "getCurrent" => self
                .get_current(request, model, authorization)
                .map(|value| (value, false)),
            "getLastFocused" => self
                .get_last_focused(request, model, authorization)
                .map(|value| (value, false)),
            "getAll" => self
                .get_all(request, model, authorization)
                .map(|value| (value, false)),
            "create" => self
                .create(request, model, authorization)
                .map(|value| (value, true)),
            "update" => self
                .update(request, model, authorization)
                .map(|value| (value, false)),
            "remove" => self.remove(request, model).map(|value| (value, true)),
            _ => Err(ExtensionApiError::new(
                "unsupported_api",
                format!("windows.{} is not supported", request.method),
            )),
        }
    }

    fn focused_host_id(model: &ExtensionModel) -> Option<i32> {
        model
            .windows
            .iter()
            .find(|window| window.focused)
            .map(|window| window.id)
    }

    fn focus_fallback(host_window_id: Option<i32>, model: &ExtensionModel) -> i32 {
        if let Some(id) = host_window_id
            && model.windows.iter().any(|window| window.id == id)
        {
            return id;
        }
        Self::focused_host_id(model)
            .or_else(|| model.windows.first().map(|window| window.id))
            .unwrap_or(WINDOW_ID_NONE)
    }

    fn contains_id(&self, id: i32, model: &ExtensionModel) -> bool {
        self.windows.iter().any(|(_, wid, ..)| wid.0 == id)
            || model.windows.iter().any(|window| window.id == id)
    }

    fn current_id(&self, caller: &ExtensionCallerContext, model: &ExtensionModel) -> Option<i32> {
        if let Some(caller_url) = caller.url() {
            let mut matched = Vec::new();
            for (_, wid, _, _, urls, _, _) in &self.windows {
                if urls
                    .0
                    .iter()
                    .any(|url| VirtualWindowUrls::page_matches(url, caller_url))
                {
                    matched.push(wid.0);
                }
            }
            matched.sort_unstable();
            if let Some(id) = matched.first().copied() {
                return Some(id);
            }
        }
        let mut focused = Vec::new();
        for (_, wid, snapshot, ..) in &self.windows {
            if snapshot.focused {
                focused.push(wid.0);
            }
        }
        focused.sort_unstable();
        if let Some(id) = focused.first().copied() {
            return Some(id);
        }
        Self::focused_host_id(model)
            .or_else(|| model.windows.first().map(|window| window.id))
            .or(Some(ExtensionWindowSequence::FALLBACK_HOST_ID))
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
        if id >= ExtensionWindowSequence::FIRST_ID && id < self.sequence.0 {
            return Err(ExtensionApiError::new(
                "window_not_found",
                "extension window is unavailable",
            ));
        }
        Ok(Self::focused_host_id(model)
            .or_else(|| model.windows.first().map(|window| window.id))
            .unwrap_or(id))
    }

    fn get(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let id = request
            .argument(0)
            .and_then(Value::as_i64)
            .and_then(|id| i32::try_from(id).ok())
            .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "windowId is required"))?;
        let options = request.argument(1);
        let id = self.resolve_id(id, &request.caller_context, model)?;
        let id = self.resolve_native_alias(id, model)?;
        self.value_by_id(id, options, model, request, authorization)
    }

    fn get_current(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let id = self
            .current_id(&request.caller_context, model)
            .ok_or_else(|| {
                ExtensionApiError::new("window_not_found", "current window is unavailable")
            })?;
        self.value_by_id(id, request.argument(0), model, request, authorization)
    }

    fn get_last_focused(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let id = self
            .last_focused
            .0
            .filter(|id| self.contains_id(*id, model))
            .or_else(|| Self::focused_host_id(model))
            .or_else(|| model.windows.first().map(|window| window.id))
            .unwrap_or(ExtensionWindowSequence::FALLBACK_HOST_ID);
        self.value_by_id(id, request.argument(0), model, request, authorization)
    }

    fn get_all(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let options = request.argument(0);
        let populate = options
            .and_then(|options| options.get("populate"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut ordered = Vec::new();
        for (entity, wid, ..) in &self.windows {
            ordered.push((wid.0, entity));
        }
        ordered.sort_unstable_by_key(|(id, _)| *id);
        let mut claimed = HashSet::new();
        let mut focused_hosts = HashSet::new();
        let mut virtual_values = Vec::new();
        for (_, entity) in ordered {
            let Ok((_, wid, snapshot, host, urls, popup_url, mut tabs)) =
                self.windows.get_mut(entity)
            else {
                continue;
            };
            tabs.refresh(wid.0, &urls.0, popup_url.is_some(), model, &mut claimed);
            if snapshot.focused
                && let Some(host) = host
            {
                focused_hosts.insert(host.0);
            }
            let materialized = snapshot.to_snapshot(wid.0);
            if VirtualWindowSnapshot::matches_type(&materialized, options) {
                virtual_values.push(VirtualWindowSnapshot::disclosed_value(
                    &materialized,
                    tabs.snapshots(model),
                    populate,
                    request,
                    authorization,
                ));
            }
        }
        let mut values = Vec::new();
        for window in &model.windows {
            if !VirtualWindowSnapshot::matches_type(window, options) {
                continue;
            }
            let mut window = window.clone();
            if focused_hosts.contains(&window.id) {
                window.focused = true;
            }
            let mut tabs = Vec::new();
            for tab in &model.tabs {
                if tab.window_id == window.id && !claimed.contains(&tab.id) {
                    tabs.push(tab.clone());
                }
            }
            values.push(VirtualWindowSnapshot::disclosed_value(
                &window,
                tabs,
                populate,
                request,
                authorization,
            ));
        }
        values.extend(virtual_values);
        Ok(Value::Array(values))
    }

    fn create(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let fields = request
            .argument(0)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let extension_id = request.caller_context.extension_id();
        Self::validate_create_fields(&fields)?;
        let urls = Self::parse_create_urls(&fields, extension_id)?;
        let base = model
            .windows
            .iter()
            .find(|window| window.focused)
            .or_else(|| model.windows.first());
        let focused = fields
            .get("focused")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let window_type = fields
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("normal");
        let state = fields
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("normal");
        let id = self.sequence.next();
        if focused {
            for (_, _, mut entry, ..) in &mut self.windows {
                entry.focused = false;
            }
            self.last_focused.0 = Some(id);
        }
        let window = Self::build_created_snapshot(&fields, base, id, focused, window_type, state);
        let result = VirtualWindowSnapshot::disclosed_value(
            &window,
            Vec::new(),
            true,
            request,
            authorization,
        );
        let mut entity = self.commands.spawn((
            Name::new("Extension window"),
            VirtualExtensionWindow,
            ExtensionWindowId(id),
            VirtualWindowSnapshot::from_snapshot(&window),
            VirtualWindowUrls(urls.clone()),
            VirtualWindowTabs::default(),
        ));
        if cfg!(target_os = "macos") && window_type == "popup" {
            if let Some(host) = base {
                entity.insert(VirtualWindowHost(host.id));
            }
            if let Some(url) = urls
                .first()
                .filter(|url| popup::OpenRequest::owns_url(extension_id, url))
            {
                entity.insert(VirtualPopupUrl(url.clone()));
            }
        }
        self.events
            .write(ExtensionModelEvent::WindowCreated(window));
        if focused {
            self.events
                .write(ExtensionModelEvent::WindowFocusChanged { window_id: id });
        }
        let open_urls = if urls.is_empty() {
            vec![None]
        } else {
            urls.into_iter().map(Some).collect()
        };
        self.open_window.write(OpenExtensionWindowRequest {
            window_id: id,
            host_window_id: base.map(|window| window.id),
            extension_id: extension_id.to_string(),
            urls: open_urls,
            window_type: window_type.into(),
        });
        Ok(result)
    }

    fn build_created_snapshot(
        fields: &Map<String, Value>,
        base: Option<&ExtensionWindowSnapshot>,
        id: i32,
        focused: bool,
        window_type: &str,
        state: &str,
    ) -> ExtensionWindowSnapshot {
        let integer = |key: &str| {
            fields
                .get(key)
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
        };
        let positive = |key: &str| {
            fields
                .get(key)
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .map(|value| value as i32)
        };
        ExtensionWindowSnapshot {
            id,
            focused,
            left: integer("left")
                .or_else(|| base.map(|window| window.left))
                .unwrap_or(0),
            top: integer("top")
                .or_else(|| base.map(|window| window.top))
                .unwrap_or(0),
            width: positive("width")
                .or_else(|| base.map(|window| window.width))
                .unwrap_or(800),
            height: positive("height")
                .or_else(|| base.map(|window| window.height))
                .unwrap_or(600),
            incognito: false,
            window_type: window_type.into(),
            state: state.into(),
            always_on_top: false,
        }
    }

    fn validate_create_fields(fields: &Map<String, Value>) -> Result<(), ExtensionApiError> {
        if fields.get("incognito").and_then(Value::as_bool) == Some(true) {
            return Err(ExtensionApiError::new(
                "unsupported_option",
                "incognito extension windows are unavailable",
            ));
        }
        let state = fields
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("normal");
        UpdateHostWindowRequest::validate_state(state)?;
        if state != "normal"
            && ["left", "top", "width", "height"]
                .iter()
                .any(|key| fields.contains_key(*key))
        {
            return Err(ExtensionApiError::new(
                "invalid_arguments",
                "window bounds cannot be combined with this state",
            ));
        }
        let window_type = fields
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("normal");
        if !matches!(window_type, "normal" | "popup" | "panel") {
            return Err(ExtensionApiError::new(
                "invalid_arguments",
                "extension window type is invalid",
            ));
        }
        if fields.contains_key("tabId") {
            return Err(ExtensionApiError::new(
                "unsupported_option",
                "moving an existing tab into an extension window is unavailable",
            ));
        }
        Ok(())
    }

    fn parse_create_urls(
        fields: &Map<String, Value>,
        extension_id: &str,
    ) -> Result<Vec<String>, ExtensionApiError> {
        match fields.get("url") {
            Some(Value::String(url)) => Ok(vec![Self::resolve_create_url(url, extension_id)?]),
            Some(Value::Array(values)) => {
                let mut urls = Vec::with_capacity(values.len());
                for value in values {
                    let Some(url) = value.as_str() else {
                        return Err(ExtensionApiError::new(
                            "invalid_arguments",
                            "window URL is invalid",
                        ));
                    };
                    urls.push(Self::resolve_create_url(url, extension_id)?);
                }
                Ok(urls)
            }
            Some(_) => Err(ExtensionApiError::new(
                "invalid_arguments",
                "window URL is invalid",
            )),
            None => Ok(Vec::new()),
        }
    }

    fn resolve_create_url(url: &str, extension_id: &str) -> Result<String, ExtensionApiError> {
        let parsed = url::Url::parse(url).or_else(|_| {
            url::Url::parse(&format!("chrome-extension://{extension_id}/"))?.join(url)
        });
        let parsed =
            parsed.map_err(|_| ExtensionApiError::new("invalid_url", "window URL is invalid"))?;
        match parsed.scheme() {
            "http" | "https" => {}
            "chrome-extension" if parsed.host_str() == Some(extension_id) => {}
            _ => {
                return Err(ExtensionApiError::new(
                    "invalid_url",
                    "window URL uses an unsupported scheme",
                ));
            }
        }
        Ok(parsed.to_string())
    }

    fn update(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ExtensionApiError> {
        let requested_id = request
            .argument(0)
            .and_then(Value::as_i64)
            .and_then(|id| i32::try_from(id).ok())
            .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "windowId is required"))?;
        let update_value = request
            .argument(1)
            .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "updateInfo is required"))?;
        let id = self.resolve_id(requested_id, &request.caller_context, model)?;
        let update = UpdateHostWindowRequest::from_value(id, update_value)?;
        if let Some(target) = self
            .windows
            .iter()
            .find_map(|(entity, wid, ..)| (wid.0 == id).then_some(entity))
        {
            return Ok(self.update_virtual(target, id, &update, model, request, authorization));
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
            self.last_focused.0 = Some(id);
        }
        for event in VirtualWindowSnapshot::events_since(&before, &after) {
            self.events.write(event);
        }
        let mut tabs = Vec::new();
        for tab in &model.tabs {
            if tab.window_id == id {
                tabs.push(tab.clone());
            }
        }
        let result =
            VirtualWindowSnapshot::disclosed_value(&after, tabs, true, request, authorization);
        self.update_host_window.write(update);
        Ok(result)
    }

    fn update_virtual(
        &mut self,
        target: Entity,
        id: i32,
        update: &UpdateHostWindowRequest,
        model: &ExtensionModel,
        request: &ApiRequest,
        authorization: &BridgeAuthorization,
    ) -> Value {
        let was_focused = self
            .windows
            .get(target)
            .expect("known extension window")
            .2
            .focused;
        if update.focused == Some(true) {
            for (_, wid, mut entry, ..) in &mut self.windows {
                entry.focused = wid.0 == id;
            }
            self.last_focused.0 = Some(id);
        }
        let (_, _, mut snapshot, _, _, _, tabs) = self
            .windows
            .get_mut(target)
            .expect("known extension window");
        let before = snapshot.to_snapshot(id);
        update.apply_to_virtual(&mut snapshot);
        let mut after = snapshot.to_snapshot(id);
        let mut events = VirtualWindowSnapshot::events_since(&before, &after);
        if update.focused == Some(false) && was_focused {
            snapshot.focused = false;
            after.focused = false;
            let fallback = Self::focused_host_id(model).unwrap_or(WINDOW_ID_NONE);
            self.last_focused.0 = (fallback >= 0).then_some(fallback);
            events.push(ExtensionModelEvent::WindowFocusChanged {
                window_id: fallback,
            });
        }
        let result = VirtualWindowSnapshot::disclosed_value(
            &after,
            tabs.snapshots(model),
            true,
            request,
            authorization,
        );
        for event in events {
            self.events.write(event);
        }
        result
    }

    fn remove(
        &mut self,
        request: &ApiRequest,
        model: &ExtensionModel,
    ) -> Result<Value, ExtensionApiError> {
        let id = request
            .argument(0)
            .and_then(Value::as_i64)
            .and_then(|id| i32::try_from(id).ok())
            .ok_or_else(|| ExtensionApiError::new("invalid_arguments", "windowId is required"))?;
        let Some((entity, _, snapshot, host, urls, popup_url, mut tabs)) =
            self.windows.iter_mut().find(|(_, wid, ..)| wid.0 == id)
        else {
            return Err(ExtensionApiError::new(
                "window_not_found",
                "extension window is unavailable",
            ));
        };
        let has_popup = popup_url.is_some();
        tabs.refresh(id, &urls.0, has_popup, model, &mut HashSet::new());
        let tab_ids = tabs.0.clone();
        let remaining_urls = urls
            .0
            .iter()
            .skip(usize::from(has_popup))
            .cloned()
            .collect();
        let focused = snapshot.focused;
        let host_window_id = host.map(|host| host.0);
        self.commands.entity(entity).despawn();
        self.events
            .write(ExtensionModelEvent::WindowRemoved { window_id: id });
        if focused {
            let fallback = Self::focus_fallback(host_window_id, model);
            self.last_focused.0 = (fallback >= 0).then_some(fallback);
            self.events.write(ExtensionModelEvent::WindowFocusChanged {
                window_id: fallback,
            });
        }
        self.close_window.write(CloseExtensionWindowRequest {
            window_id: Some(id),
            tab_ids,
            urls: remaining_urls,
        });
        Ok(Value::Null)
    }

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
        if let Some((_, _, snapshot, _, urls, popup_url, mut tabs)) =
            self.windows.iter_mut().find(|(_, wid, ..)| wid.0 == id)
        {
            tabs.refresh(id, &urls.0, popup_url.is_some(), model, &mut HashSet::new());
            let materialized = snapshot.to_snapshot(id);
            if !VirtualWindowSnapshot::matches_type(&materialized, options) {
                return Err(ExtensionApiError::new(
                    "window_not_found",
                    "window type is filtered out",
                ));
            }
            return Ok(VirtualWindowSnapshot::disclosed_value(
                &materialized,
                tabs.snapshots(model),
                populate,
                request,
                authorization,
            ));
        }
        let fallback;
        let window = if let Some(window) = model.windows.iter().find(|window| window.id == id) {
            window
        } else {
            fallback = VirtualWindowSnapshot::fallback(id);
            &fallback
        };
        if !VirtualWindowSnapshot::matches_type(window, options) {
            return Err(ExtensionApiError::new(
                "window_not_found",
                "window type is filtered out",
            ));
        }
        let mut tabs = Vec::new();
        for tab in &model.tabs {
            if tab.window_id == id {
                tabs.push(tab.clone());
            }
        }
        Ok(VirtualWindowSnapshot::disclosed_value(
            window,
            tabs,
            populate,
            request,
            authorization,
        ))
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Extension windows"),
        ExtensionWindowSequence::default(),
        LastFocusedExtensionWindow::default(),
    ));
}

fn open(
    mut requests: MessageReader<OpenExtensionWindowRequest>,
    mut popup_requests: MessageWriter<popup::OpenRequest>,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    for request in requests.read() {
        let mut urls = request.urls.iter().cloned();
        if cfg!(target_os = "macos")
            && request.window_type == "popup"
            && let Some(url) = urls.next()
        {
            if let Some(url) = url {
                if popup::OpenRequest::owns_url(&request.extension_id, &url) {
                    popup_requests.write(popup::OpenRequest {
                        extension_id: request.extension_id.clone(),
                        url,
                        window_id: request.window_id,
                        host_window_id: request.host_window_id,
                    });
                } else {
                    stack_requests.write(vmux_layout::stack::OpenRequest {
                        url: Some(url),
                        policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
                    });
                }
            } else {
                stack_requests.write(vmux_layout::stack::OpenRequest {
                    url: None,
                    policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
                });
            }
        }
        for url in urls {
            stack_requests.write(vmux_layout::stack::OpenRequest {
                url,
                policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
            });
        }
    }
}

fn route_close(
    mut requests: MessageReader<CloseExtensionWindowRequest>,
    tab_ids: Query<(Entity, &ExtensionTabId)>,
    stacks: Query<(Entity, &PageMetadata, Option<&LastActivatedAt>), With<Stack>>,
    mut close_requests: MessageWriter<CloseStackRequest>,
    mut popup_requests: MessageWriter<popup::CloseRequest>,
) {
    for request in requests.read() {
        if let Some(window_id) = request.window_id {
            popup_requests.write(popup::CloseRequest { window_id });
        }
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

fn sync(
    model: Single<Ref<ExtensionModel>>,
    mut windows: Query<
        (
            Entity,
            &ExtensionWindowId,
            &VirtualWindowUrls,
            Option<&VirtualPopupUrl>,
            &mut VirtualWindowTabs,
        ),
        With<VirtualExtensionWindow>,
    >,
) {
    if !model.is_changed() {
        return;
    }
    let mut ordered = Vec::new();
    for (entity, wid, ..) in &windows {
        ordered.push((wid.0, entity));
    }
    ordered.sort_unstable_by_key(|(id, _)| *id);
    let mut claimed = HashSet::new();
    for (_, entity) in ordered {
        let Ok((_, wid, urls, popup_url, mut tabs)) = windows.get_mut(entity) else {
            continue;
        };
        tabs.refresh(wid.0, &urls.0, popup_url.is_some(), &model, &mut claimed);
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
        if request.left.is_some() || request.top.is_some() {
            let current = match window.position {
                WindowPosition::At(position) => position,
                _ => IVec2::ZERO,
            };
            window.position = WindowPosition::At(IVec2::new(
                request.left.unwrap_or(current.x),
                request.top.unwrap_or(current.y),
            ));
        }
        if request.width.is_some() || request.height.is_some() {
            let width = request
                .width
                .map_or(window.resolution.width(), |width| width as f32);
            let height = request
                .height
                .map_or(window.resolution.height(), |height| height as f32);
            window.resolution.set(width, height);
        }
        match request.state.as_deref() {
            Some("fullscreen") => {
                window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
            }
            Some("normal") => window.mode = WindowMode::Windowed,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;
    use serde_json::json;

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Fixture {
        app: App,
    }

    impl Fixture {
        fn new(model: ExtensionModel) -> Self {
            let mut app = App::new();
            app.add_plugins(ExtensionWindowsPlugin)
                .add_message::<popup::OpenRequest>()
                .add_message::<popup::CloseRequest>()
                .add_message::<vmux_layout::stack::OpenRequest>()
                .add_message::<CloseStackRequest>()
                .add_message::<ExtensionModelEvent>();
            app.world_mut().spawn(model);
            app.update();
            Self { app }
        }

        fn with_default_model() -> Self {
            Self::new(ExtensionModel {
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
            })
        }

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

        fn dispatch(
            &mut self,
            request: &ApiRequest,
            authorization: &BridgeAuthorization,
        ) -> Result<Value, ExtensionApiError> {
            let model = self
                .app
                .world_mut()
                .query::<&ExtensionModel>()
                .single(self.app.world())
                .unwrap()
                .clone();
            let mut state = SystemState::<VirtualWindows>::new(self.app.world_mut());
            let result = state.get_mut(self.app.world_mut()).unwrap().dispatch(
                request,
                &model,
                authorization,
            );
            state.apply(self.app.world_mut());
            result.map(|(value, _)| value)
        }
    }

    #[test]
    fn creates_queries_updates_and_removes_extension_window() {
        let mut fixture = Fixture::with_default_model();
        let popout_url = format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/fido2");
        let created = fixture
            .dispatch(
                &Fixture::request(
                    "create",
                    json!([{
                        "url": popout_url,
                        "type": "popup",
                        "width": 900,
                        "height": 700
                    }]),
                ),
                &BridgeAuthorization::default(),
            )
            .unwrap();
        let id = created["id"].as_i64().unwrap() as i32;
        assert_eq!(created["type"], "popup");
        assert_eq!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<OpenExtensionWindowRequest>>()
                .drain()
                .count(),
            1
        );
        fixture
            .app
            .world_mut()
            .query::<&mut ExtensionModel>()
            .single_mut(fixture.app.world_mut())
            .unwrap()
            .tabs
            .push(ExtensionTabSnapshot {
                id: 8,
                window_id: if cfg!(target_os = "macos") { id } else { 1 },
                index: 1,
                active: !cfg!(target_os = "macos"),
                highlighted: true,
                pinned: false,
                url: format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/vault"),
                title: "Bitwarden".into(),
                status: "complete".into(),
            });

        let all = fixture
            .dispatch(
                &Fixture::request("getAll", json!([{ "populate": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();
        assert_eq!(all.as_array().unwrap().len(), 2);
        let virtual_window = all
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["id"] == id)
            .unwrap();
        assert_eq!(virtual_window["tabs"][0]["windowId"], id);

        let updated = fixture
            .dispatch(
                &Fixture::request("update", json!([id, { "left": 42, "focused": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();
        assert_eq!(updated["left"], 42);

        let mut events_cursor = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>()
            .get_cursor();
        let _ = events_cursor
            .read(
                fixture
                    .app
                    .world()
                    .resource::<Messages<ExtensionModelEvent>>(),
            )
            .count();
        fixture
            .dispatch(
                &Fixture::request("remove", json!([id])),
                &BridgeAuthorization::default(),
            )
            .unwrap();
        let close = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<CloseExtensionWindowRequest>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(close.tab_ids, vec![8]);
        let events = fixture
            .app
            .world()
            .resource::<Messages<ExtensionModelEvent>>();
        assert!(events_cursor.read(events).any(|event| matches!(
            event,
            ExtensionModelEvent::WindowRemoved { window_id } if *window_id == id
        )));
    }

    #[test]
    fn current_window_resolves_extension_page_url() {
        let mut fixture = Fixture::with_default_model();
        let created = fixture
            .dispatch(
                &Fixture::request(
                    "create",
                    json!([{ "url": format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/fido2") }]),
                ),
                &BridgeAuthorization::default(),
            )
            .unwrap();
        let id = created["id"].as_i64().unwrap();
        let mut current = Fixture::request("getCurrent", json!([]));
        current.caller_context = ExtensionCallerContext::ExtensionPage {
            extension_id: EXTENSION_ID.into(),
            context_id: "document".into(),
            url: format!("chrome-extension://{EXTENSION_ID}/popup/index.html?x=1#/vault"),
            document_id: "document".into(),
        };

        let result = fixture
            .dispatch(&current, &BridgeAuthorization::default())
            .unwrap();

        assert_eq!(result["id"], id);
    }

    #[test]
    fn get_maps_chromium_native_window_id_to_focused_host_window() {
        let mut fixture = Fixture::with_default_model();

        let result = fixture
            .dispatch(
                &Fixture::request("get", json!([1_798_152_106, { "populate": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();

        assert_eq!(result["id"], 1);
        assert_eq!(result["left"], 10);
        assert_eq!(result["tabs"][0]["id"], 7);
    }

    #[test]
    fn get_maps_window_id_none_to_current_window_with_geometry() {
        let mut fixture = Fixture::with_default_model();

        let result = fixture
            .dispatch(
                &Fixture::request("get", json!([WINDOW_ID_NONE, { "populate": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();

        assert_eq!(result["id"], 1);
        assert_eq!(result["left"], 10);
        assert_eq!(result["top"], 20);
        assert_eq!(result["width"], 1200);
        assert_eq!(result["height"], 800);
        assert_eq!(result["tabs"][0]["id"], 7);
    }

    #[test]
    fn window_queries_return_fallback_geometry_before_host_projection() {
        let mut fixture = Fixture::new(ExtensionModel::default());

        let by_id = fixture
            .dispatch(
                &Fixture::request("get", json!([1_798_152_106, { "populate": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();
        let current = fixture
            .dispatch(
                &Fixture::request("getCurrent", json!([{ "populate": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();

        assert_eq!(by_id["id"], 1_798_152_106);
        assert_eq!(by_id["left"], 0);
        assert_eq!(by_id["top"], 0);
        assert_eq!(by_id["width"], 1920);
        assert_eq!(by_id["height"], 1080);
        assert_eq!(by_id["tabs"], json!([]));
        assert_eq!(current["id"], ExtensionWindowSequence::FALLBACK_HOST_ID);
        assert_eq!(current["width"], 1920);
    }

    #[test]
    fn populated_windows_redact_tab_details_without_permission() {
        let mut fixture = Fixture::with_default_model();
        let result = fixture
            .dispatch(
                &Fixture::request("getAll", json!([{ "populate": true }])),
                &BridgeAuthorization::default(),
            )
            .unwrap();

        let tab = &result[0]["tabs"][0];
        assert_eq!(tab["id"], 7);
        assert!(tab.get("url").is_none());
        assert!(tab.get("title").is_none());
    }

    #[test]
    fn populated_windows_disclose_tab_details_with_tabs_permission() {
        let mut fixture = Fixture::with_default_model();
        let authorization = BridgeAuthorization {
            permissions: ["tabs".into()].into_iter().collect(),
            ..Default::default()
        };
        let result = fixture
            .dispatch(
                &Fixture::request("getAll", json!([{ "populate": true }])),
                &authorization,
            )
            .unwrap();

        let tab = &result[0]["tabs"][0];
        assert_eq!(tab["url"], "https://example.com/");
        assert_eq!(tab["title"], "Example");
    }

    #[test]
    fn create_rejects_existing_tab_id() {
        let mut fixture = Fixture::with_default_model();
        let error = fixture
            .dispatch(
                &Fixture::request("create", json!([{ "tabId": 7 }])),
                &BridgeAuthorization::default(),
            )
            .unwrap_err();

        assert_eq!(error.code, "unsupported_option");
    }

    #[test]
    fn close_fallback_selects_most_recent_matching_extension_page() {
        let mut fixture = Fixture::new(ExtensionModel::default());
        let older = fixture
            .app
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
        let popout = fixture
            .app
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
        let mut cursor = fixture
            .app
            .world()
            .resource::<Messages<CloseStackRequest>>()
            .get_cursor();
        fixture
            .app
            .world_mut()
            .write_message(CloseExtensionWindowRequest {
                window_id: None,
                tab_ids: Vec::new(),
                urls: vec![format!(
                    "chrome-extension://{EXTENSION_ID}/popup/index.html?singleActionPopout=fido#/fido2"
                )],
            });

        fixture.app.update();

        let messages = fixture
            .app
            .world()
            .resource::<Messages<CloseStackRequest>>();
        let closed = cursor
            .read(messages)
            .map(|request| request.stack)
            .collect::<Vec<_>>();
        assert_eq!(closed, vec![popout]);
        assert_ne!(closed[0], older);
    }
}
