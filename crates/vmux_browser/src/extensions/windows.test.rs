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
fn get_maps_window_id_none_to_current_window_with_geometry() {
    let result = dispatch(
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
    let model = ChromeModel {
        windows: Vec::new(),
        tabs: Vec::new(),
    };
    let mut windows = ExtensionWindows::default();

    let by_id = dispatch(
        &request("get", json!([1_798_152_106, { "populate": true }])),
        &model,
        &mut windows,
        &BridgeAuthorization::default(),
    )
    .unwrap();
    let current = dispatch(
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
