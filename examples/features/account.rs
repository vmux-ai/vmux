use std::path::PathBuf;

use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;
use vmux_app::account::local::LocalPlugin;
use vmux_app::account::{
    AccountPlugin, CreateRequest, MutationCompleted, Profile, RenameRequest, Root,
};
use vmux_app::ecs::{ProfileId, Ready, Uuid as RecordId};
use vmux_app::prelude::*;

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
struct ProfileRow {
    id: String,
    directory: String,
    name: String,
}

#[ui_state(Default, Eq)]
struct AccountsUiState {
    ready: bool,
    status: String,
    profiles: Vec<ProfileRow>,
}

#[ui_event(Eq)]
struct CreateProfileEvent {
    name: String,
}

#[ui_event(Eq)]
struct RenameProfileEvent {
    id: String,
    name: String,
}

#[derive(Resource)]
struct AccountDemoState {
    _temp: tempfile::TempDir,
    data_dir: PathBuf,
    status: String,
}

struct AccountDemoPlugin;

impl Plugin for AccountDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiStatePlugin::<AccountsUiState>::default(),
            UiIpcEventPlugin::<CreateProfileEvent>::default(),
            UiIpcEventPlugin::<RenameProfileEvent>::default(),
        ))
        .add_systems(Startup, spawn_account_root)
        .add_observer(handle_create)
        .add_observer(handle_rename)
        .add_systems(Update, (record_mutation, publish_accounts).chain());
    }
}

fn spawn_account_root(state: Res<AccountDemoState>, mut commands: Commands) {
    commands.spawn(Root::at(&state.data_dir, "personal"));
}

fn handle_create(
    trigger: On<UiIpcInput<CreateProfileEvent>>,
    roots: Query<Entity, With<Root>>,
    mut requests: MessageWriter<CreateRequest>,
) {
    let name = trigger.event().name.trim();
    if name.is_empty() {
        return;
    }
    let Some(root) = roots.iter().next() else {
        return;
    };
    requests.write(CreateRequest {
        root,
        operation_id: Uuid::new_v4(),
        name: name.to_string(),
    });
}

fn handle_rename(
    trigger: On<UiIpcInput<RenameProfileEvent>>,
    roots: Query<Entity, With<Root>>,
    mut requests: MessageWriter<RenameRequest>,
) {
    let event = trigger.event();
    let name = event.name.trim();
    if name.is_empty() {
        return;
    }
    let Ok(profile_id) = event.id.parse::<Uuid>() else {
        return;
    };
    let Some(root) = roots.iter().next() else {
        return;
    };
    requests.write(RenameRequest {
        root,
        operation_id: Uuid::new_v4(),
        profile_id,
        name: name.to_string(),
    });
}

fn record_mutation(
    mut completed: MessageReader<MutationCompleted>,
    mut state: ResMut<AccountDemoState>,
) {
    for event in completed.read() {
        state.status = match &event.result {
            Ok(_) => String::from("Committed to local SQLite store"),
            Err(error) => format!("Mutation failed: {error:?}"),
        };
    }
}

fn publish_accounts(
    roots: Query<Has<Ready>, With<Root>>,
    profiles: Query<(&RecordId, &ProfileId, &Name), With<Profile>>,
    state: Res<AccountDemoState>,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    mut writer: UiStateWriter<AccountsUiState>,
) {
    let ready = roots.iter().any(|is_ready| is_ready);
    let mut rows: Vec<ProfileRow> = profiles
        .iter()
        .map(|(id, dir, name)| ProfileRow {
            id: id.0.clone(),
            directory: dir.0.clone(),
            name: name.as_str().to_string(),
        })
        .collect();
    rows.sort_by(|left, right| left.directory.cmp(&right.directory));
    for page in &pages {
        writer.push(
            page,
            AccountsUiState {
                ready,
                status: state.status.clone(),
                profiles: rows.clone(),
            },
        );
    }
}

#[component]
fn AccountsPageView() -> Element {
    let binding = use_ui_state::<AccountsUiState>();
    let state = binding.state.read().clone();
    let mut draft = use_signal(String::new);
    rsx! {
        div { class: "root",
            h1 { "Vmux Accounts & Profiles (vmux_account)" }
            p { class: "status", "SQLite ready: {state.ready} — {state.status}" }
            div { class: "row",
                input {
                    value: "{draft}",
                    placeholder: "New profile name (e.g. Work)",
                    oninput: move |event| draft.set(event.value()),
                }
                button {
                    onclick: move |_| {
                        let name = draft();
                        if !name.trim().is_empty() {
                            let _ = send_ui_event(&CreateProfileEvent { name });
                            draft.set(String::new());
                        }
                    },
                    "Create Profile"
                }
            }
            ul {
                for row in state.profiles {
                    li {
                        span { "{row.name} (dir: {row.directory})" }
                        button {
                            onclick: move |_| {
                                let _ = send_ui_event(&RenameProfileEvent {
                                    id: row.id.clone(),
                                    name: format!("{} ✓", row.name),
                                });
                            },
                            "Rename"
                        }
                    }
                }
            }
        }
    }
}

static ACCOUNTS_PAGE: Page = Page {
    permissions: &[
        AccountsUiState::PERMISSION,
        CreateProfileEvent::PERMISSION,
        RenameProfileEvent::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://feature-account/", AccountsPageView).titled("Accounts")
};

const CSS: &str = "body{background:#111318;color:#f3f4f6;font-family:system-ui,sans-serif;padding:24px}.row{display:flex;gap:8px;margin:12px 0}input{flex:1;padding:8px 12px;border-radius:8px;border:1px solid #334155;background:#1e293b;color:#fff}button{padding:8px 14px;border-radius:8px;border:1px solid #334155;background:#2563eb;color:#fff;cursor:pointer}ul{list-style:none;padding:0}li{display:flex;justify-content:space-between;align-items:center;padding:8px 0;border-bottom:1px solid #1f2937}.status{color:#94a3b8}";

fn main() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().to_path_buf();

    let mut config = WindowHostConfig::new("Vmux Accounts")
        .size(720, 480)
        .page(&ACCOUNTS_PAGE)
        .stylesheet(CSS);
    if std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some() {
        config = config.headless();
    }

    App::new()
        .insert_resource(AccountDemoState {
            _temp: temp,
            data_dir,
            status: String::from("Opening local SQLite store"),
        })
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            AccountPlugin,
            LocalPlugin,
            AccountDemoPlugin,
        ))
        .run();
}
