use std::path::PathBuf;

use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::prelude::*;
use vmux_app::session::{
    AgentId, CreateRequest, PromptQueue, QueuePlugin, QueuedPromptReads, QueuedPrompts, Session,
    SessionDomainPlugin, SessionId, Stage, StageChangeRequest, StageId,
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
struct SessionRow {
    id: String,
    name: String,
    stage: String,
    queued: Vec<String>,
}

#[ui_state(Default, Eq)]
struct SessionsUiState {
    sessions: Vec<SessionRow>,
}

#[ui_event(Eq)]
struct CreateSessionEvent {
    name: String,
}

#[ui_event(Eq)]
struct AdvanceStageEvent {
    id: String,
    stage: String,
}

#[ui_event(Eq)]
struct EnqueuePromptEvent {
    id: String,
    prompt: String,
}

#[ui_event(Eq)]
struct PopPromptEvent {
    id: String,
}

struct SessionDemoPlugin;

impl Plugin for SessionDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiStatePlugin::<SessionsUiState>::default(),
            UiIpcEventPlugin::<CreateSessionEvent>::default(),
            UiIpcEventPlugin::<AdvanceStageEvent>::default(),
            UiIpcEventPlugin::<EnqueuePromptEvent>::default(),
            UiIpcEventPlugin::<PopPromptEvent>::default(),
        ))
        .add_systems(Startup, seed_session)
        .add_observer(handle_create)
        .add_observer(handle_stage)
        .add_observer(handle_enqueue)
        .add_observer(handle_pop)
        .add_systems(Update, (attach_queues, publish_sessions).chain());
    }
}

fn seed_session(mut create: MessageWriter<CreateRequest>) {
    create.write(CreateRequest::new(
        "Decouple Feature Crates",
        "Standalone vmux_session example",
        PathBuf::from("/workspace/vmux"),
        Some(AgentId("codex".into())),
    ));
}

fn attach_queues(
    new_sessions: Query<Entity, (With<Session>, Without<PromptQueue>)>,
    mut commands: Commands,
) {
    for entity in &new_sessions {
        commands.entity(entity).insert(PromptQueue::default());
    }
}

fn handle_create(
    trigger: On<UiIpcInput<CreateSessionEvent>>,
    mut create: MessageWriter<CreateRequest>,
) {
    let name = trigger.event().name.trim();
    if name.is_empty() {
        return;
    }
    create.write(CreateRequest::new(
        name,
        "Interactive session",
        PathBuf::from("/workspace/vmux"),
        Some(AgentId("codex".into())),
    ));
}

fn handle_stage(
    trigger: On<UiIpcInput<AdvanceStageEvent>>,
    mut stage_changes: MessageWriter<StageChangeRequest>,
) {
    let event = trigger.event();
    stage_changes.write(StageChangeRequest {
        id: SessionId(event.id.clone()),
        stage: StageId(event.stage.clone()),
    });
}

fn handle_enqueue(
    trigger: On<UiIpcInput<EnqueuePromptEvent>>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    mut queue: QueuedPrompts,
) {
    let event = trigger.event();
    let prompt = event.prompt.trim();
    if prompt.is_empty() {
        return;
    }
    let Some((entity, _)) = sessions.iter().find(|(_, id)| id.0 == event.id) else {
        return;
    };
    queue.enqueue(entity, prompt.to_string(), Vec::new(), false);
}

fn handle_pop(
    trigger: On<UiIpcInput<PopPromptEvent>>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    mut queue: QueuedPrompts,
) {
    let Some((entity, _)) = sessions.iter().find(|(_, id)| id.0 == trigger.event().id) else {
        return;
    };
    let _ = queue.take_next(entity);
}

fn publish_sessions(
    sessions: Query<(Entity, &SessionId, &Name, &Stage), With<Session>>,
    queues: QueuedPromptReads,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    mut writer: UiStateWriter<SessionsUiState>,
) {
    let mut rows: Vec<SessionRow> = sessions
        .iter()
        .map(|(entity, id, name, stage)| SessionRow {
            id: id.0.clone(),
            name: name.as_str().to_string(),
            stage: stage.0.0.clone(),
            queued: queues
                .visible(entity)
                .into_iter()
                .map(|(_, text, _)| text.to_string())
                .collect(),
        })
        .collect();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    for page in &pages {
        writer.push(
            page,
            SessionsUiState {
                sessions: rows.clone(),
            },
        );
    }
}

#[component]
fn SessionsPageView() -> Element {
    let binding = use_ui_state::<SessionsUiState>();
    let state = binding.state.read().clone();
    let mut session_name = use_signal(String::new);
    let mut prompt_draft = use_signal(String::new);
    rsx! {
        div { class: "root",
            h1 { "Vmux Sessions & Queue (vmux_session)" }
            div { class: "row",
                input {
                    value: "{session_name}",
                    placeholder: "New session title",
                    oninput: move |event| session_name.set(event.value()),
                }
                button {
                    onclick: move |_| {
                        let name = session_name();
                        if !name.trim().is_empty() {
                            let _ = send_ui_event(&CreateSessionEvent { name });
                            session_name.set(String::new());
                        }
                    },
                    "New Session"
                }
            }
            div { class: "row",
                input {
                    value: "{prompt_draft}",
                    placeholder: "Prompt to enqueue",
                    oninput: move |event| prompt_draft.set(event.value()),
                }
            }
            for session in state.sessions {
                div { class: "card",
                    div { class: "header",
                        strong { "{session.name}" }
                        span { class: "badge", "{session.stage}" }
                    }
                    div { class: "actions",
                        button {
                            onclick: {
                                let id = session.id.clone();
                                let next = match session.stage.as_str() {
                                    "in_progress" => "in_review",
                                    "in_review" => "done",
                                    _ => "in_progress",
                                }
                                .to_string();
                                move |_| {
                                    let _ = send_ui_event(&AdvanceStageEvent {
                                        id: id.clone(),
                                        stage: next.clone(),
                                    });
                                }
                            },
                            "Next Stage"
                        }
                        button {
                            onclick: {
                                let id = session.id.clone();
                                move |_| {
                                    let prompt = prompt_draft();
                                    if !prompt.trim().is_empty() {
                                        let _ = send_ui_event(&EnqueuePromptEvent {
                                            id: id.clone(),
                                            prompt,
                                        });
                                        prompt_draft.set(String::new());
                                    }
                                }
                            },
                            "Enqueue Prompt"
                        }
                        button {
                            onclick: {
                                let id = session.id.clone();
                                move |_| {
                                    let _ = send_ui_event(&PopPromptEvent { id: id.clone() });
                                }
                            },
                            "Pop Prompt"
                        }
                    }
                    ul {
                        for item in session.queued {
                            li { "{item}" }
                        }
                    }
                }
            }
        }
    }
}

static SESSIONS_PAGE: Page = Page {
    permissions: &[
        SessionsUiState::PERMISSION,
        CreateSessionEvent::PERMISSION,
        AdvanceStageEvent::PERMISSION,
        EnqueuePromptEvent::PERMISSION,
        PopPromptEvent::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://feature-session/", SessionsPageView).titled("Sessions")
};

const CSS: &str = "body{background:#111318;color:#f3f4f6;font-family:system-ui,sans-serif;padding:24px}.row{display:flex;gap:8px;margin:10px 0}input{flex:1;padding:8px 12px;border-radius:8px;border:1px solid #334155;background:#1e293b;color:#fff}.card{border:1px solid #1f2937;border-radius:10px;padding:12px;margin:10px 0;background:#181b22}.header{display:flex;justify-content:space-between;margin-bottom:8px}.badge{padding:2px 8px;border-radius:999px;background:#1e293b;color:#93c5fd;font-size:12px}.actions{display:flex;gap:8px}button{padding:6px 12px;border-radius:8px;border:1px solid #334155;background:#2563eb;color:#fff;cursor:pointer}";

fn main() {
    let mut config = WindowHostConfig::new("Vmux Sessions")
        .size(760, 520)
        .page(&SESSIONS_PAGE)
        .stylesheet(CSS);
    if std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some() {
        config = config.headless();
    }

    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            SessionDomainPlugin,
            QueuePlugin,
            SessionDemoPlugin,
        ))
        .run();
}
