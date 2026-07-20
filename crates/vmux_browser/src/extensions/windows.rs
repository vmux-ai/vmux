use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowPosition};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use vmux_core::PageMetadata;
use vmux_core::extension::protocol::{ApiRequest, ChromeError, ExtensionCallerContext};
use vmux_history::LastActivatedAt;
use vmux_layout::stack::{CloseStackRequest, Stack};

use super::bridge::BridgeAuthorization;
use super::model::{ChromeModel, ChromeModelEvent, ChromeStableIds, ChromeTab, ChromeWindow};

pub const WINDOW_ID_NONE: i32 = -1;
pub const WINDOW_ID_CURRENT: i32 = -2;
const FIRST_EXTENSION_WINDOW_ID: i32 = 1_000_000_000;

#[derive(Clone, Debug)]
struct ExtensionWindow {
    window: ChromeWindow,
    urls: Vec<String>,
    tab_ids: Vec<i32>,
}

#[derive(Resource)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowEffect {
    Open(Vec<Option<String>>),
    Close {
        tab_ids: Vec<i32>,
        urls: Vec<String>,
    },
    UpdateHost {
        window_id: i32,
        update: HostWindowUpdate,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDispatch {
    pub result: Value,
    pub effects: Vec<WindowEffect>,
    pub events: Vec<ChromeModelEvent>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct CloseExtensionWindowRequest {
    pub tab_ids: Vec<i32>,
    pub urls: Vec<String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct UpdateHostWindowRequest {
    pub window_id: i32,
    pub update: HostWindowUpdate,
}

pub fn dispatch(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    match request.method.as_str() {
        "get" => get(request, model, windows, authorization),
        "getCurrent" => get_current(request, model, windows, authorization),
        "getLastFocused" => get_last_focused(request, model, windows, authorization),
        "getAll" => get_all(request, model, windows, authorization),
        "create" => create(request, model, windows, authorization),
        "update" => update(request, model, windows, authorization),
        "remove" => remove(request, model, windows),
        _ => Err(ChromeError::new(
            "unsupported_api",
            format!("windows.{} is not supported", request.method),
        )),
    }
}

pub fn route_close_extension_windows(
    mut requests: MessageReader<CloseExtensionWindowRequest>,
    stable_ids: Res<ChromeStableIds>,
    stacks: Query<(Entity, &PageMetadata, Option<&LastActivatedAt>), With<Stack>>,
    mut close_requests: MessageWriter<CloseStackRequest>,
) {
    for request in requests.read() {
        let mut targets = HashSet::new();
        for tab_id in &request.tab_ids {
            if let Some(entity) = stable_ids.tab_entity(*tab_id)
                && stacks.contains(entity)
            {
                targets.insert(entity);
            }
        }
        if targets.is_empty() {
            for (entity, metadata, _) in &stacks {
                if request
                    .urls
                    .iter()
                    .any(|url| same_document(url, &metadata.url))
                {
                    targets.insert(entity);
                }
            }
        }
        if targets.is_empty()
            && let Some(entity) = stacks
                .iter()
                .filter(|(_, metadata, _)| {
                    request
                        .urls
                        .iter()
                        .any(|url| extension_page_matches(url, &metadata.url))
                })
                .max_by_key(|(_, _, activated)| activated.map_or(0, |activated| activated.0))
                .map(|(entity, _, _)| entity)
        {
            targets.insert(entity);
        }
        for stack in targets {
            close_requests.write(CloseStackRequest { stack });
        }
    }
}

pub fn sync_extension_windows(model: Res<ChromeModel>, mut windows: ResMut<ExtensionWindows>) {
    if !model.is_changed() {
        return;
    }
    let mut claimed = HashSet::new();
    for window in windows.windows.values_mut() {
        refresh_tab_ids(window, &model, &mut claimed);
    }
}

pub fn apply_host_window_updates(
    mut requests: MessageReader<UpdateHostWindowRequest>,
    stable_ids: Res<ChromeStableIds>,
    mut native_windows: Query<&mut Window>,
) {
    for request in requests.read() {
        let Some(entity) = stable_ids.window_entity(request.window_id) else {
            continue;
        };
        let Ok(mut window) = native_windows.get_mut(entity) else {
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
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    let id = argument(request, 0)
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ChromeError::new("invalid_arguments", "windowId is required"))?;
    let options = argument(request, 1);
    let id = resolve_window_id(id, &request.caller_context, model, windows)?;
    let id = resolve_native_window_alias(id, model, windows)?;
    let result = window_by_id(id, options, model, windows, request, authorization)?;
    Ok(success(result))
}

fn get_current(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    let id = current_window_id(&request.caller_context, model, windows)
        .ok_or_else(|| ChromeError::new("window_not_found", "current window is unavailable"))?;
    let result = window_by_id(
        id,
        argument(request, 0),
        model,
        windows,
        request,
        authorization,
    )?;
    Ok(success(result))
}

fn get_last_focused(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    let id = windows
        .last_focused
        .filter(|id| window_exists(*id, model, windows))
        .or_else(|| focused_host_window(model))
        .or_else(|| model.windows.first().map(|window| window.id))
        .ok_or_else(|| ChromeError::new("window_not_found", "focused window is unavailable"))?;
    let result = window_by_id(
        id,
        argument(request, 0),
        model,
        windows,
        request,
        authorization,
    )?;
    Ok(success(result))
}

fn get_all(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    let options = argument(request, 0);
    let populate = populate(options);
    let mut claimed = HashSet::new();
    let ids = windows.windows.keys().copied().collect::<Vec<_>>();
    let mut virtual_values = Vec::new();
    for id in ids {
        let extension_window = windows
            .windows
            .get_mut(&id)
            .expect("known extension window");
        refresh_tab_ids(extension_window, model, &mut claimed);
        if type_matches(&extension_window.window, options) {
            virtual_values.push(window_value(
                &extension_window.window,
                virtual_tabs(extension_window, model),
                populate,
                request,
                authorization,
            ));
        }
    }
    let mut values = model
        .windows
        .iter()
        .filter(|window| type_matches(window, options))
        .map(|window| {
            window_value(
                window,
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
    Ok(success(Value::Array(values)))
}

fn create(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    let data = argument(request, 0).and_then(Value::as_object);
    if data
        .and_then(|data| data.get("incognito"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(ChromeError::new(
            "unsupported_option",
            "incognito extension windows are unavailable",
        ));
    }
    validate_state_and_bounds(data)?;
    let urls = create_urls(data, request)?;
    let base = model
        .windows
        .iter()
        .find(|window| window.focused)
        .or_else(|| model.windows.first());
    let focused = data
        .and_then(|data| data.get("focused"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let window_type = data
        .and_then(|data| data.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("normal");
    if !matches!(window_type, "normal" | "popup" | "panel") {
        return Err(ChromeError::new(
            "invalid_arguments",
            "extension window type is invalid",
        ));
    }
    let state = data
        .and_then(|data| data.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("normal");
    validate_state(state)?;
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
    let window = ChromeWindow {
        id,
        focused,
        left: integer(data, "left")
            .or_else(|| base.map(|window| window.left))
            .unwrap_or(0),
        top: integer(data, "top")
            .or_else(|| base.map(|window| window.top))
            .unwrap_or(0),
        width: positive_integer(data, "width")
            .map(|value| value as i32)
            .or_else(|| base.map(|window| window.width))
            .unwrap_or(800),
        height: positive_integer(data, "height")
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
    let result = window_value(&window, Vec::new(), true, request, authorization);
    windows.windows.insert(id, extension_window);
    let mut events = vec![ChromeModelEvent::WindowCreated(window)];
    if focused {
        events.push(ChromeModelEvent::WindowFocusChanged { window_id: id });
    }
    let open = if urls.is_empty() {
        vec![None]
    } else {
        urls.into_iter().map(Some).collect()
    };
    Ok(WindowDispatch {
        result,
        effects: vec![WindowEffect::Open(open)],
        events,
    })
}

fn update(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
) -> Result<WindowDispatch, ChromeError> {
    let requested_id = argument(request, 0)
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ChromeError::new("invalid_arguments", "windowId is required"))?;
    let update_value = argument(request, 1)
        .cloned()
        .ok_or_else(|| ChromeError::new("invalid_arguments", "updateInfo is required"))?;
    let update: HostWindowUpdate = serde_json::from_value(update_value)
        .map_err(|_| ChromeError::new("invalid_arguments", "updateInfo is invalid"))?;
    validate_update(&update)?;
    let id = resolve_window_id(requested_id, &request.caller_context, model, windows)?;
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
        apply_update(&mut entry.window, &update);
        let mut events = window_update_events(&before, &entry.window);
        if update.focused == Some(false) && was_focused {
            entry.window.focused = false;
            let fallback = focused_host_window(model).unwrap_or(WINDOW_ID_NONE);
            windows.last_focused = (fallback >= 0).then_some(fallback);
            events.push(ChromeModelEvent::WindowFocusChanged {
                window_id: fallback,
            });
        }
        return Ok(WindowDispatch {
            result: window_value(
                &entry.window,
                virtual_tabs(entry, model),
                true,
                request,
                authorization,
            ),
            effects: Vec::new(),
            events,
        });
    }
    let before = model
        .windows
        .iter()
        .find(|window| window.id == id)
        .cloned()
        .ok_or_else(|| ChromeError::new("window_not_found", "window is unavailable"))?;
    let mut after = before.clone();
    apply_update(&mut after, &update);
    if update.focused == Some(true) {
        windows.last_focused = Some(id);
    }
    Ok(WindowDispatch {
        result: window_value(
            &after,
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
        effects: vec![WindowEffect::UpdateHost {
            window_id: id,
            update,
        }],
        events: window_update_events(&before, &after),
    })
}

fn remove(
    request: &ApiRequest,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
) -> Result<WindowDispatch, ChromeError> {
    let id = argument(request, 0)
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ChromeError::new("invalid_arguments", "windowId is required"))?;
    let mut entry = windows
        .windows
        .remove(&id)
        .ok_or_else(|| ChromeError::new("window_not_found", "extension window is unavailable"))?;
    refresh_tab_ids(&mut entry, model, &mut HashSet::new());
    let mut events = vec![ChromeModelEvent::WindowRemoved { window_id: id }];
    if entry.window.focused {
        let fallback = focused_host_window(model).unwrap_or(WINDOW_ID_NONE);
        windows.last_focused = (fallback >= 0).then_some(fallback);
        events.push(ChromeModelEvent::WindowFocusChanged {
            window_id: fallback,
        });
    }
    Ok(WindowDispatch {
        result: Value::Null,
        effects: vec![WindowEffect::Close {
            tab_ids: entry.tab_ids,
            urls: entry.urls,
        }],
        events,
    })
}

fn success(result: Value) -> WindowDispatch {
    WindowDispatch {
        result,
        effects: Vec::new(),
        events: Vec::new(),
    }
}

fn argument(request: &ApiRequest, index: usize) -> Option<&Value> {
    match &request.arguments {
        Value::Array(arguments) => arguments.get(index),
        value if index == 0 => Some(value),
        _ => None,
    }
}

fn resolve_window_id(
    id: i32,
    caller: &ExtensionCallerContext,
    model: &ChromeModel,
    windows: &ExtensionWindows,
) -> Result<i32, ChromeError> {
    if id == WINDOW_ID_CURRENT {
        return current_window_id(caller, model, windows)
            .ok_or_else(|| ChromeError::new("window_not_found", "current window is unavailable"));
    }
    if id < 0 {
        return Err(ChromeError::new("invalid_arguments", "windowId is invalid"));
    }
    Ok(id)
}

fn current_window_id(
    caller: &ExtensionCallerContext,
    model: &ChromeModel,
    windows: &ExtensionWindows,
) -> Option<i32> {
    if let Some(caller_url) = caller.url()
        && let Some(id) = windows.windows.iter().find_map(|(id, window)| {
            window
                .urls
                .iter()
                .any(|url| extension_page_matches(url, caller_url))
                .then_some(*id)
        })
    {
        return Some(id);
    }
    windows
        .windows
        .values()
        .find(|window| window.window.focused)
        .map(|window| window.window.id)
        .or_else(|| focused_host_window(model))
        .or_else(|| model.windows.first().map(|window| window.id))
}

fn focused_host_window(model: &ChromeModel) -> Option<i32> {
    model
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| window.id)
}

fn window_exists(id: i32, model: &ChromeModel, windows: &ExtensionWindows) -> bool {
    windows.windows.contains_key(&id) || model.windows.iter().any(|window| window.id == id)
}

fn resolve_native_window_alias(
    id: i32,
    model: &ChromeModel,
    windows: &ExtensionWindows,
) -> Result<i32, ChromeError> {
    if window_exists(id, model, windows) {
        return Ok(id);
    }
    if id >= FIRST_EXTENSION_WINDOW_ID && id < windows.next_id {
        return Err(ChromeError::new(
            "window_not_found",
            "extension window is unavailable",
        ));
    }
    focused_host_window(model)
        .or_else(|| model.windows.first().map(|window| window.id))
        .ok_or_else(|| ChromeError::new("window_not_found", "window is unavailable"))
}

fn window_by_id(
    id: i32,
    options: Option<&Value>,
    model: &ChromeModel,
    windows: &mut ExtensionWindows,
    request: &ApiRequest,
    authorization: &BridgeAuthorization,
) -> Result<Value, ChromeError> {
    if let Some(window) = windows.windows.get_mut(&id) {
        refresh_tab_ids(window, model, &mut HashSet::new());
        if !type_matches(&window.window, options) {
            return Err(ChromeError::new(
                "window_not_found",
                "window type is filtered out",
            ));
        }
        return Ok(window_value(
            &window.window,
            virtual_tabs(window, model),
            populate(options),
            request,
            authorization,
        ));
    }
    let window = model
        .windows
        .iter()
        .find(|window| window.id == id)
        .ok_or_else(|| ChromeError::new("window_not_found", "window is unavailable"))?;
    if !type_matches(window, options) {
        return Err(ChromeError::new(
            "window_not_found",
            "window type is filtered out",
        ));
    }
    Ok(window_value(
        window,
        model
            .tabs
            .iter()
            .filter(|tab| tab.window_id == id)
            .cloned()
            .collect(),
        populate(options),
        request,
        authorization,
    ))
}

fn window_value(
    window: &ChromeWindow,
    tabs: Vec<ChromeTab>,
    populate: bool,
    request: &ApiRequest,
    authorization: &BridgeAuthorization,
) -> Value {
    let mut value = window_base_value(window);
    if populate {
        value.as_object_mut().expect("window object").insert(
            "tabs".into(),
            Value::Array(
                tabs.into_iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        tab_value(tab, window.id, index as u32, request, authorization)
                    })
                    .collect(),
            ),
        );
    }
    value
}

fn window_base_value(window: &ChromeWindow) -> Value {
    serde_json::to_value(window).expect("ChromeWindow serializes")
}

fn tab_value(
    mut tab: ChromeTab,
    window_id: i32,
    index: u32,
    request: &ApiRequest,
    authorization: &BridgeAuthorization,
) -> Value {
    tab.window_id = window_id;
    tab.index = index;
    let disclose = authorization.permissions.contains("tabs")
        || url::Url::parse(&tab.url).is_ok_and(|url| {
            (url.scheme() == "chrome-extension"
                && url.host_str() == Some(request.caller_context.extension_id()))
                || authorization
                    .host_permissions
                    .iter()
                    .any(|pattern| pattern.matches(&url))
        });
    let mut value = serde_json::to_value(tab).expect("ChromeTab serializes");
    if !disclose {
        let object = value.as_object_mut().expect("tab object");
        object.remove("url");
        object.remove("title");
    }
    value
}

fn virtual_tabs(window: &ExtensionWindow, model: &ChromeModel) -> Vec<ChromeTab> {
    window
        .tab_ids
        .iter()
        .filter_map(|id| model.tabs.iter().find(|tab| tab.id == *id).cloned())
        .collect()
}

fn refresh_tab_ids(window: &mut ExtensionWindow, model: &ChromeModel, claimed: &mut HashSet<i32>) {
    window
        .tab_ids
        .retain(|id| model.tabs.iter().any(|tab| tab.id == *id));
    claimed.extend(window.tab_ids.iter().copied());
    for url in &window.urls {
        let exact = model
            .tabs
            .iter()
            .find(|tab| !claimed.contains(&tab.id) && same_document(url, &tab.url));
        if let Some(tab) = exact {
            window.tab_ids.push(tab.id);
            claimed.insert(tab.id);
        }
    }
    window.tab_ids.sort_unstable();
    window.tab_ids.dedup();
}

fn same_document(expected: &str, actual: &str) -> bool {
    let (Ok(mut expected), Ok(mut actual)) = (url::Url::parse(expected), url::Url::parse(actual))
    else {
        return expected == actual;
    };
    expected.set_fragment(None);
    actual.set_fragment(None);
    expected == actual
}

fn extension_page_matches(expected: &str, actual: &str) -> bool {
    if same_document(expected, actual) {
        return true;
    }
    let (Ok(expected), Ok(actual)) = (url::Url::parse(expected), url::Url::parse(actual)) else {
        return false;
    };
    expected.scheme() == "chrome-extension"
        && expected.scheme() == actual.scheme()
        && expected.host_str() == actual.host_str()
        && expected.path() == actual.path()
}

fn populate(options: Option<&Value>) -> bool {
    options
        .and_then(|options| options.get("populate"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn type_matches(window: &ChromeWindow, options: Option<&Value>) -> bool {
    options
        .and_then(|options| options.get("windowTypes"))
        .and_then(Value::as_array)
        .is_none_or(|types| {
            types
                .iter()
                .filter_map(Value::as_str)
                .any(|window_type| window_type == window.window_type)
        })
}

fn create_urls(
    data: Option<&serde_json::Map<String, Value>>,
    request: &ApiRequest,
) -> Result<Vec<String>, ChromeError> {
    let urls = match data.and_then(|data| data.get("url")) {
        Some(Value::String(url)) => vec![resolve_url(url, request)?],
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| ChromeError::new("invalid_arguments", "window URL is invalid"))
                    .and_then(|url| resolve_url(url, request))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(ChromeError::new(
                "invalid_arguments",
                "window URL is invalid",
            ));
        }
        None => Vec::new(),
    };
    if data.is_some_and(|data| data.contains_key("tabId")) {
        return Err(ChromeError::new(
            "unsupported_option",
            "moving an existing tab into an extension window is unavailable",
        ));
    }
    Ok(urls)
}

fn resolve_url(url: &str, request: &ApiRequest) -> Result<String, ChromeError> {
    let parsed = url::Url::parse(url).or_else(|_| {
        url::Url::parse(&format!(
            "chrome-extension://{}/",
            request.caller_context.extension_id()
        ))?
        .join(url)
    });
    let parsed = parsed.map_err(|_| ChromeError::new("invalid_url", "window URL is invalid"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        "chrome-extension" if parsed.host_str() == Some(request.caller_context.extension_id()) => {}
        _ => {
            return Err(ChromeError::new(
                "invalid_url",
                "window URL uses an unsupported scheme",
            ));
        }
    }
    Ok(parsed.to_string())
}

fn integer(data: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<i32> {
    data.and_then(|data| data.get(key))
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn positive_integer(data: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<u32> {
    data.and_then(|data| data.get(key))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn validate_state_and_bounds(
    data: Option<&serde_json::Map<String, Value>>,
) -> Result<(), ChromeError> {
    let state = data
        .and_then(|data| data.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("normal");
    validate_state(state)?;
    if state != "normal"
        && data.is_some_and(|data| {
            ["left", "top", "width", "height"]
                .iter()
                .any(|key| data.contains_key(*key))
        })
    {
        return Err(ChromeError::new(
            "invalid_arguments",
            "window bounds cannot be combined with this state",
        ));
    }
    Ok(())
}

fn validate_update(update: &HostWindowUpdate) -> Result<(), ChromeError> {
    if let Some(state) = update.state.as_deref() {
        validate_state(state)?;
        if state != "normal"
            && (update.left.is_some()
                || update.top.is_some()
                || update.width.is_some()
                || update.height.is_some())
        {
            return Err(ChromeError::new(
                "invalid_arguments",
                "window bounds cannot be combined with this state",
            ));
        }
        if state == "minimized" && update.focused == Some(true) {
            return Err(ChromeError::new(
                "invalid_arguments",
                "a minimized window cannot be focused",
            ));
        }
        if matches!(state, "fullscreen" | "maximized") && update.focused == Some(false) {
            return Err(ChromeError::new(
                "invalid_arguments",
                "a fullscreen or maximized window cannot be unfocused",
            ));
        }
    }
    Ok(())
}

fn validate_state(state: &str) -> Result<(), ChromeError> {
    if matches!(state, "normal" | "minimized" | "maximized" | "fullscreen") {
        Ok(())
    } else {
        Err(ChromeError::new(
            "invalid_arguments",
            "window state is invalid",
        ))
    }
}

fn apply_update(window: &mut ChromeWindow, update: &HostWindowUpdate) {
    if let Some(left) = update.left {
        window.left = left;
    }
    if let Some(top) = update.top {
        window.top = top;
    }
    if let Some(width) = update.width {
        window.width = width as i32;
    }
    if let Some(height) = update.height {
        window.height = height as i32;
    }
    if let Some(focused) = update.focused {
        window.focused = focused;
    }
    if let Some(state) = &update.state {
        window.state.clone_from(state);
    }
}

fn window_update_events(before: &ChromeWindow, after: &ChromeWindow) -> Vec<ChromeModelEvent> {
    let mut events = Vec::new();
    if before.left != after.left
        || before.top != after.top
        || before.width != after.width
        || before.height != after.height
    {
        events.push(ChromeModelEvent::WindowBoundsChanged(after.clone()));
    }
    if before.focused != after.focused {
        events.push(ChromeModelEvent::WindowFocusChanged {
            window_id: if after.focused {
                after.id
            } else {
                WINDOW_ID_NONE
            },
        });
    }
    events
}

pub fn event_payload(event: &ChromeModelEvent) -> Option<(&'static str, Value)> {
    match event {
        ChromeModelEvent::WindowCreated(window) => {
            Some(("onCreated", json!([window_base_value(window)])))
        }
        ChromeModelEvent::WindowRemoved { window_id } => Some(("onRemoved", json!([window_id]))),
        ChromeModelEvent::WindowFocusChanged { window_id } => {
            Some(("onFocusChanged", json!([window_id])))
        }
        ChromeModelEvent::WindowBoundsChanged(window) => {
            Some(("onBoundsChanged", json!([window_base_value(window)])))
        }
        _ => None,
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

    fn model() -> ChromeModel {
        ChromeModel {
            windows: vec![ChromeWindow {
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
            tabs: vec![ChromeTab {
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
        let created = dispatch(
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
        assert_eq!(created.effects.len(), 1);
        model.tabs.push(ChromeTab {
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

        let all = dispatch(
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

        let updated = dispatch(
            &request("update", json!([id, { "left": 42, "focused": true }])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        assert_eq!(updated.result["left"], 42);

        let removed = dispatch(
            &request("remove", json!([id])),
            &model,
            &mut windows,
            &BridgeAuthorization::default(),
        )
        .unwrap();
        assert!(matches!(
            &removed.effects[0],
            WindowEffect::Close { tab_ids, .. } if tab_ids == &vec![8]
        ));
        assert!(matches!(
            removed.events[0],
            ChromeModelEvent::WindowRemoved { window_id } if window_id == id
        ));
    }

    #[test]
    fn current_window_resolves_extension_page_url() {
        let model = model();
        let mut windows = ExtensionWindows::default();
        let created = dispatch(
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

        let result = dispatch(
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

        let result = dispatch(
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
    fn populated_windows_redact_tab_details_without_permission() {
        let result = dispatch(
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
        let result = dispatch(
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
        let error = dispatch(
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
        app.init_resource::<ChromeStableIds>()
            .add_message::<CloseExtensionWindowRequest>()
            .add_message::<CloseStackRequest>()
            .add_systems(Update, route_close_extension_windows);
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
