use std::path::PathBuf;

use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::ecs::Ready;
use vmux_app::prelude::*;
use vmux_app::project::filesystem::FilesystemPlugin;
use vmux_app::project::{
    Project, ProjectId, ProjectRoute, ProjectSaved, Root, SaveProjectRequest, WriteCompleted,
};

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
struct ProjectRow {
    url: String,
    name: String,
    revision: u64,
}

#[ui_state(Default, Eq)]
struct ProjectsUiState {
    ready: bool,
    status: String,
    projects: Vec<ProjectRow>,
}

#[ui_event(Eq)]
struct RegisterProjectEvent {
    name: String,
}

#[ui_event(Eq)]
struct RenameProjectEvent {
    url: String,
    name: String,
}

#[derive(Resource)]
struct DemoWorkspace {
    _temp: tempfile::TempDir,
    root: PathBuf,
    repo_dir: PathBuf,
    status: String,
    next_index: u64,
}

struct ProjectDemoPlugin;

impl Plugin for ProjectDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiStatePlugin::<ProjectsUiState>::default(),
            UiIpcEventPlugin::<RegisterProjectEvent>::default(),
            UiIpcEventPlugin::<RenameProjectEvent>::default(),
        ))
        .add_systems(Startup, spawn_root)
        .add_observer(handle_register)
        .add_observer(handle_rename)
        .add_systems(Update, (record_write_status, publish_projects).chain());
    }
}

fn spawn_root(workspace: Res<DemoWorkspace>, mut commands: Commands) {
    commands.spawn(Root(workspace.root.clone()));
}

fn handle_register(
    trigger: On<UiIpcInput<RegisterProjectEvent>>,
    mut workspace: ResMut<DemoWorkspace>,
    mut commands: Commands,
) {
    let name = trigger.event().name.trim();
    if name.is_empty() {
        return;
    }
    workspace.next_index += 1;
    let url = format!("vmux://projects/demo-{}", workspace.next_index);
    commands.spawn(SaveProjectRequest {
        root: workspace.root.clone(),
        expected: 0,
        project: ProjectSaved {
            root: workspace.root.clone(),
            id: ProjectId(url),
            directory: workspace.repo_dir.clone(),
            name: name.to_string(),
            repository: Some("https://github.com/vmux-ai/vmux.git".into()),
            revision: 0,
        },
    });
}

fn handle_rename(
    trigger: On<UiIpcInput<RenameProjectEvent>>,
    workspace: Res<DemoWorkspace>,
    projects: Query<(&ProjectId, &Revision), With<Project>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let name = event.name.trim();
    if name.is_empty() {
        return;
    }
    let Some((_, revision)) = projects.iter().find(|(id, _)| id.0 == event.url) else {
        return;
    };
    commands.spawn(SaveProjectRequest {
        root: workspace.root.clone(),
        expected: revision.0,
        project: ProjectSaved {
            root: workspace.root.clone(),
            id: ProjectId(event.url.clone()),
            directory: workspace.repo_dir.clone(),
            name: name.to_string(),
            repository: Some("https://github.com/vmux-ai/vmux.git".into()),
            revision: revision.0,
        },
    });
}

fn record_write_status(
    mut completed: MessageReader<WriteCompleted>,
    mut workspace: ResMut<DemoWorkspace>,
) {
    for event in completed.read() {
        workspace.status = match &event.result {
            Ok(_) => String::from("Saved to .projects.ron"),
            Err(error) => format!("Save error: {error:?}"),
        };
    }
}

fn publish_projects(
    roots: Query<Has<Ready>, With<Root>>,
    projects: Query<(&ProjectId, &Name, &Revision), With<Project>>,
    workspace: Res<DemoWorkspace>,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    mut writer: UiStateWriter<ProjectsUiState>,
) {
    let ready = roots.iter().any(|is_ready| is_ready);
    let mut rows: Vec<ProjectRow> = projects
        .iter()
        .map(|(id, name, revision)| ProjectRow {
            url: id.0.clone(),
            name: name.as_str().to_string(),
            revision: revision.0,
        })
        .collect();
    rows.sort_by(|left, right| left.url.cmp(&right.url));
    for page in &pages {
        writer.push(
            page,
            ProjectsUiState {
                ready,
                status: workspace.status.clone(),
                projects: rows.clone(),
            },
        );
    }
}

#[component]
fn ProjectsPageView() -> Element {
    let binding = use_ui_state::<ProjectsUiState>();
    let state = binding.state.read().clone();
    let mut draft = use_signal(String::new);
    rsx! {
        div { class: "root",
            h1 { "Vmux Projects (vmux_project)" }
            p { class: "status", "Store ready: {state.ready} — {state.status}" }
            div { class: "row",
                input {
                    value: "{draft}",
                    placeholder: "Project name",
                    oninput: move |event| draft.set(event.value()),
                }
                button {
                    onclick: move |_| {
                        let name = draft();
                        if !name.trim().is_empty() {
                            let _ = send_ui_event(&RegisterProjectEvent { name });
                            draft.set(String::new());
                        }
                    },
                    "Register"
                }
            }
            ul {
                for row in state.projects {
                    li {
                        span { "{row.name} ({row.url}) rev {row.revision}" }
                        button {
                            onclick: move |_| {
                                let _ = send_ui_event(&RenameProjectEvent {
                                    url: row.url.clone(),
                                    name: format!("{} ★", row.name),
                                });
                            },
                            "Star"
                        }
                    }
                }
            }
        }
    }
}

static PROJECTS_PAGE: Page = Page {
    permissions: &[
        ProjectsUiState::PERMISSION,
        RegisterProjectEvent::PERMISSION,
        RenameProjectEvent::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://feature-project/", ProjectsPageView).titled("Projects")
};

const CSS: &str = "body{background:#111318;color:#f3f4f6;font-family:system-ui,sans-serif;padding:24px}.row{display:flex;gap:8px;margin:12px 0}input{flex:1;padding:8px 12px;border-radius:8px;border:1px solid #334155;background:#1e293b;color:#fff}button{padding:8px 14px;border-radius:8px;border:1px solid #334155;background:#2563eb;color:#fff;cursor:pointer}ul{list-style:none;padding:0}li{display:flex;justify-content:space-between;align-items:center;padding:8px 0;border-bottom:1px solid #1f2937}.status{color:#94a3b8}";

fn main() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let repo_dir = root.join("demo-repo");
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    let initial_route = ProjectRoute::from_path(&root, &repo_dir).unwrap_or_default();

    let mut config = WindowHostConfig::new("Vmux Projects")
        .size(720, 480)
        .page(&PROJECTS_PAGE)
        .stylesheet(CSS);
    if std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some() {
        config = config.headless();
    }

    let workspace = DemoWorkspace {
        _temp: temp,
        root,
        repo_dir,
        status: format!("Ready (sample route: {initial_route})"),
        next_index: 0,
    };

    App::new()
        .insert_resource(workspace)
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            ProjectPlugin,
            FilesystemPlugin,
            ProjectDemoPlugin,
        ))
        .run();
}
