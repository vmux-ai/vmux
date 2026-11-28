use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_api::extension::{ExtPinRequest, ExtToggleRequest};
use vmux_app::extension::snapshot::{ExtEntry, Index};
use vmux_app::extension::{
    Catalog, ExtensionPlugin, Pin, Root, Seed, Toggle, WriteCompleted, WriteTaskRequest,
};
use vmux_app::prelude::*;

const FIRST_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct ExtensionRow {
    id: String,
    name: String,
    version: String,
    enabled: bool,
    pinned: bool,
}

#[ui_state(Default, Eq)]
struct ExtensionsUiState {
    extensions: Vec<ExtensionRow>,
}

#[ui_event(Eq)]
struct ToggleExtensionEvent {
    id: String,
    enabled: bool,
    approve_permissions: bool,
}

#[ui_event(Eq)]
struct PinExtensionEvent {
    id: String,
    pinned: bool,
}

struct ExtensionDemoPlugin;

impl Plugin for ExtensionDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiStatePlugin::<ExtensionsUiState>::default(),
            UiIpcEventPlugin::<ToggleExtensionEvent>::default(),
            UiIpcEventPlugin::<PinExtensionEvent>::default(),
        ))
        .add_systems(Startup, seed_catalog)
        .add_observer(handle_toggle)
        .add_observer(handle_pin)
        .add_systems(Update, (commit_writes, publish_catalog).chain());
    }
}

fn seed_catalog(mut commands: Commands) {
    let mut index = Index::default();
    let mut first = ExtEntry {
        id: FIRST_ID.into(),
        name: "Clipboard Helper".into(),
        version: "1.0.0".into(),
        permissions: vec!["clipboardWrite".into()],
        ..Default::default()
    };
    first.profile_enabled.insert("personal".into(), false);
    let mut second = ExtEntry {
        id: SECOND_ID.into(),
        name: "Reader Mode".into(),
        version: "2.1.0".into(),
        permissions: vec!["activeTab".into()],
        ..Default::default()
    };
    second.profile_enabled.insert("personal".into(), true);
    index.entries.push(first);
    index.entries.push(second);
    commands.spawn((Root("/extensions".into()), Seed(Ok(index))));
}

fn handle_toggle(
    trigger: On<UiIpcInput<ToggleExtensionEvent>>,
    roots: Query<Entity, With<Root>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    let Some(ticket) = catalog.ticket(root).cloned() else {
        return;
    };
    let event = trigger.event();
    commands.spawn(Toggle::bundle(
        ticket,
        "personal".into(),
        ExtToggleRequest {
            id: event.id.clone(),
            enabled: event.enabled,
            approve_permissions: event.approve_permissions,
        },
    ));
}

fn handle_pin(
    trigger: On<UiIpcInput<PinExtensionEvent>>,
    roots: Query<Entity, With<Root>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    let Some(ticket) = catalog.ticket(root).cloned() else {
        return;
    };
    let event = trigger.event();
    commands.spawn(Pin::bundle(
        ticket,
        "personal".into(),
        ExtPinRequest {
            id: event.id.clone(),
            pinned: event.pinned,
        },
    ));
}

fn commit_writes(
    mut requests: MessageReader<WriteTaskRequest>,
    mut completed: MessageWriter<WriteCompleted>,
) {
    for request in requests.read() {
        completed.write(WriteCompleted {
            ticket: request.ticket.clone(),
            operation: request.operation,
            revision: request.revision,
            result: Ok(()),
        });
    }
}

fn publish_catalog(
    roots: Query<Entity, With<Root>>,
    catalog: Catalog,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    mut writer: UiStateWriter<ExtensionsUiState>,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    let Ok(snapshot) = catalog.snapshot(root) else {
        return;
    };
    let rows: Vec<ExtensionRow> = snapshot
        .entries
        .iter()
        .map(|entry| ExtensionRow {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: entry.version.clone(),
            enabled: entry.enabled_for("personal"),
            pinned: entry.pinned_for("personal"),
        })
        .collect();
    for page in &pages {
        writer.push(
            page,
            ExtensionsUiState {
                extensions: rows.clone(),
            },
        );
    }
}

#[component]
fn ExtensionsPageView() -> Element {
    let binding = use_ui_state::<ExtensionsUiState>();
    let state = binding.state.read().clone();
    rsx! {
        div { class: "root",
            h1 { "Vmux Extensions (vmux_extension)" }
            ul {
                for ext in state.extensions {
                    li {
                        span { "{ext.name} v{ext.version}" }
                        div { class: "actions",
                            button {
                                onclick: {
                                    let id = ext.id.clone();
                                    let enabled = !ext.enabled;
                                    move |_| {
                                        let _ = send_ui_event(&ToggleExtensionEvent {
                                            id: id.clone(),
                                            enabled,
                                            approve_permissions: true,
                                        });
                                    }
                                },
                                if ext.enabled { "Disable" } else { "Approve & Enable" }
                            }
                            button {
                                onclick: {
                                    let id = ext.id.clone();
                                    let pinned = !ext.pinned;
                                    move |_| {
                                        let _ = send_ui_event(&PinExtensionEvent {
                                            id: id.clone(),
                                            pinned,
                                        });
                                    }
                                },
                                if ext.pinned { "Unpin" } else { "Pin" }
                            }
                        }
                    }
                }
            }
        }
    }
}

static EXTENSIONS_PAGE: Page = Page {
    permissions: &[
        ExtensionsUiState::PERMISSION,
        ToggleExtensionEvent::PERMISSION,
        PinExtensionEvent::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://feature-extension/", ExtensionsPageView).titled("Extensions")
};

const CSS: &str = "body{background:#111318;color:#f3f4f6;font-family:system-ui,sans-serif;padding:24px}ul{list-style:none;padding:0}li{display:flex;justify-content:space-between;align-items:center;padding:10px 0;border-bottom:1px solid #1f2937}.actions{display:flex;gap:8px}button{padding:6px 12px;border-radius:8px;border:1px solid #334155;background:#2563eb;color:#fff;cursor:pointer}";

fn main() {
    let mut config = WindowHostConfig::new("Vmux Extensions")
        .size(720, 440)
        .page(&EXTENSIONS_PAGE)
        .stylesheet(CSS);
    if std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some() {
        config = config.headless();
    }

    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            ExtensionPlugin,
            ExtensionDemoPlugin,
        ))
        .run();
}
