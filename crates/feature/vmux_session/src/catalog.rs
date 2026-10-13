#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate};
#[cfg(not(host))]
use bevy_ecs::prelude::Resource;
#[cfg(host)]
use bevy_ecs::prelude::*;
#[cfg(host)]
use bevy_ecs::system::SystemParam;
#[cfg(host)]
use vmux_api::conversation::{AssistantBlock, Message};
#[cfg(host)]
use vmux_ecs::{CreatedAt, Cwd, Description, LastActivatedAt, Order, Terminal};

#[cfg(host)]
use crate::{
    AgentConversationTitle, AgentId, ConversationEvent, MessageContent, RunState, Session,
    SessionId, Stage, StageChangedAt, StageDefinition, StageId, Transcripts,
};

#[vmux_api::contract(Default, Eq)]
#[derive(Resource)]
pub struct CatalogSnapshot {
    pub stages: Vec<StageSummary>,
    pub sessions: Vec<SessionSummary>,
}

#[vmux_api::contract(Default, Eq)]
pub struct StageSummary {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub terminal: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cwd: String,
    pub stage: String,
    pub created_at: i64,
    pub last_activated_at: i64,
    pub stage_changed_at: i64,
    pub agent: String,
    pub agent_name: String,
    pub agent_icon: String,
    pub agent_color: String,
    pub runtime: String,
    pub conversation_title: String,
    pub last_message: String,
}

#[cfg(host)]
pub(crate) struct CatalogPlugin;

#[cfg(host)]
impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CatalogSnapshot>()
            .add_systems(PostUpdate, project);
    }
}

#[cfg(host)]
type SessionCatalog<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SessionId,
        &'static Name,
        &'static Description,
        &'static Cwd,
        &'static Stage,
        &'static CreatedAt,
        &'static LastActivatedAt,
        &'static StageChangedAt,
        Option<&'static AgentId>,
        Option<&'static vmux_ecs::team::Profile>,
        Option<&'static RunState>,
        Option<&'static AgentConversationTitle>,
    ),
    With<Session>,
>;

#[cfg(host)]
type ChangedSessions<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Session>,
        Or<(
            Changed<SessionId>,
            Changed<Name>,
            Changed<Description>,
            Changed<Cwd>,
            Changed<Stage>,
            Changed<CreatedAt>,
            Changed<LastActivatedAt>,
            Changed<StageChangedAt>,
            Changed<AgentId>,
            Changed<vmux_ecs::team::Profile>,
            Changed<RunState>,
            Changed<AgentConversationTitle>,
        )>,
    ),
>;

#[cfg(host)]
type ChangedStages<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<StageDefinition>,
        Or<(
            Changed<StageId>,
            Changed<Name>,
            Changed<Order>,
            Changed<Terminal>,
        )>,
    ),
>;

#[cfg(host)]
#[derive(SystemParam)]
struct CatalogChanges<'w, 's> {
    sessions: ChangedSessions<'w, 's>,
    stages: ChangedStages<'w, 's>,
    events: Query<
        'w,
        's,
        (),
        (
            With<ConversationEvent>,
            Or<(Added<ConversationEvent>, Changed<MessageContent>)>,
        ),
    >,
    removed_sessions: RemovedComponents<'w, 's, Session>,
    removed_ids: RemovedComponents<'w, 's, SessionId>,
    removed_names: RemovedComponents<'w, 's, Name>,
    removed_descriptions: RemovedComponents<'w, 's, Description>,
    removed_cwds: RemovedComponents<'w, 's, Cwd>,
    removed_stages: RemovedComponents<'w, 's, Stage>,
    removed_created_at: RemovedComponents<'w, 's, CreatedAt>,
    removed_activated_at: RemovedComponents<'w, 's, LastActivatedAt>,
    removed_stage_changed_at: RemovedComponents<'w, 's, StageChangedAt>,
    removed_stage_definitions: RemovedComponents<'w, 's, StageDefinition>,
    removed_stage_ids: RemovedComponents<'w, 's, StageId>,
    removed_orders: RemovedComponents<'w, 's, Order>,
    removed_terminal: RemovedComponents<'w, 's, Terminal>,
    removed_agents: RemovedComponents<'w, 's, AgentId>,
    removed_profiles: RemovedComponents<'w, 's, vmux_ecs::team::Profile>,
    removed_states: RemovedComponents<'w, 's, RunState>,
    removed_titles: RemovedComponents<'w, 's, AgentConversationTitle>,
    removed_events: RemovedComponents<'w, 's, ConversationEvent>,
    removed_messages: RemovedComponents<'w, 's, MessageContent>,
}

#[cfg(host)]
impl CatalogChanges<'_, '_> {
    fn any(&mut self) -> bool {
        let mut any =
            !self.sessions.is_empty() || !self.stages.is_empty() || !self.events.is_empty();
        any |= self.removed_sessions.read().count() > 0;
        any |= self.removed_ids.read().count() > 0;
        any |= self.removed_names.read().count() > 0;
        any |= self.removed_descriptions.read().count() > 0;
        any |= self.removed_cwds.read().count() > 0;
        any |= self.removed_stages.read().count() > 0;
        any |= self.removed_created_at.read().count() > 0;
        any |= self.removed_activated_at.read().count() > 0;
        any |= self.removed_stage_changed_at.read().count() > 0;
        any |= self.removed_stage_definitions.read().count() > 0;
        any |= self.removed_stage_ids.read().count() > 0;
        any |= self.removed_orders.read().count() > 0;
        any |= self.removed_terminal.read().count() > 0;
        any |= self.removed_agents.read().count() > 0;
        any |= self.removed_profiles.read().count() > 0;
        any |= self.removed_states.read().count() > 0;
        any |= self.removed_titles.read().count() > 0;
        any |= self.removed_events.read().count() > 0;
        any |= self.removed_messages.read().count() > 0;
        any
    }
}

#[cfg(host)]
fn project(
    sessions: SessionCatalog,
    stages: Query<(&StageId, &Name, &Order, Has<Terminal>), With<StageDefinition>>,
    transcripts: Transcripts,
    mut changes: CatalogChanges,
    mut catalog: ResMut<CatalogSnapshot>,
) {
    if !catalog.is_added() && !changes.any() {
        return;
    }
    let mut next = CatalogSnapshot {
        stages: stages
            .iter()
            .map(|(id, name, order, terminal)| StageSummary {
                id: id.0.clone(),
                name: name.as_str().to_string(),
                order: order.0,
                terminal,
            })
            .collect(),
        sessions: sessions
            .iter()
            .map(
                |(
                    entity,
                    id,
                    name,
                    description,
                    cwd,
                    stage,
                    created_at,
                    last_activated_at,
                    stage_changed_at,
                    agent,
                    profile,
                    runtime,
                    conversation_title,
                )| {
                    let conversation = ConversationPreview::from(
                        conversation_title,
                        &transcripts.get(entity).messages,
                    );
                    SessionSummary {
                        id: id.0.clone(),
                        name: name.as_str().to_string(),
                        description: description.0.clone(),
                        cwd: cwd.0.to_string_lossy().into_owned(),
                        stage: stage.0.0.clone(),
                        created_at: created_at.0,
                        last_activated_at: last_activated_at.0,
                        stage_changed_at: stage_changed_at.0,
                        agent: agent.map(|agent| agent.0.clone()).unwrap_or_default(),
                        agent_name: profile
                            .map(|profile| profile.name.clone())
                            .unwrap_or_default(),
                        agent_icon: profile
                            .and_then(|profile| profile.icon.clone())
                            .unwrap_or_default(),
                        agent_color: profile
                            .map(|profile| profile.avatar.color.clone())
                            .unwrap_or_default(),
                        runtime: runtime
                            .map(RunState::status)
                            .unwrap_or("inactive")
                            .to_string(),
                        conversation_title: conversation.title,
                        last_message: conversation.last_message,
                    }
                },
            )
            .collect(),
    };
    next.stages.sort_by_key(|stage| stage.order);
    next.sessions
        .sort_by_key(|session| std::cmp::Reverse(session.last_activated_at));
    if *catalog != next {
        *catalog = next;
    }
}

#[cfg(host)]
struct ConversationPreview {
    title: String,
    last_message: String,
}

#[cfg(host)]
impl ConversationPreview {
    fn from(title: Option<&AgentConversationTitle>, messages: &[Message]) -> Self {
        let title = title
            .map(|title| Self::compact(&title.0))
            .unwrap_or_default();
        let last_message = messages
            .iter()
            .rev()
            .find_map(Self::message)
            .unwrap_or_default();
        Self {
            title,
            last_message,
        }
    }

    fn message(message: &Message) -> Option<String> {
        let text = match message {
            Message::User { text, .. } => text,
            Message::Assistant { blocks } => blocks.iter().rev().find_map(|block| match block {
                AssistantBlock::Text(text) => Some(text),
                _ => None,
            })?,
            Message::ToolResult { .. } => return None,
        };
        let text = Self::compact(text);
        (!text.is_empty()).then_some(text)
    }

    fn compact(value: &str) -> String {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.chars().count() <= 160 {
            return normalized;
        }
        let mut compact = normalized.chars().take(159).collect::<String>();
        compact.push('…');
        compact
    }
}

#[cfg(all(test, host))]
mod tests {
    use super::*;
    use crate::{CreateRequest, DomainPlugin, RenameRequest, SnapshotReceived};
    #[cfg(feature = "app")]
    use bevy::MinimalPlugins;
    #[cfg(feature = "app")]
    use bevy::asset::AssetPlugin;
    #[cfg(feature = "app")]
    use moonshine_save::load::TriggerLoad;
    #[cfg(feature = "app")]
    use moonshine_save::prelude::{LoadWorld, load_on_default_event};

    #[test]
    fn catalog_tracks_session_metadata() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        let request = CreateRequest::new("Task", "Description", "/tmp/project".into(), None);
        let id = request.id().clone();
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);
        app.update();

        let catalog = app.world().resource::<CatalogSnapshot>();
        assert_eq!(catalog.sessions.len(), 1);
        assert_eq!(catalog.sessions[0].id, id.0);
        assert_eq!(catalog.sessions[0].name, "Task");
        assert_eq!(catalog.sessions[0].description, "Description");
        assert_eq!(catalog.sessions[0].cwd, "/tmp/project");

        let session = app
            .world_mut()
            .query_filtered::<Entity, With<Session>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .entity_mut(session)
            .insert(AgentConversationTitle("Fix session manager".into()));
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: id.clone(),
                messages: vec![
                    Message::user("Make the session manager useful"),
                    Message::Assistant {
                        blocks: vec![AssistantBlock::Text(
                            "Added conversation previews and metadata pills".into(),
                        )],
                    },
                ],
            });
        app.update();

        let summary = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(summary.conversation_title, "Fix session manager");
        assert_eq!(
            summary.last_message,
            "Added conversation previews and metadata pills"
        );

        app.world_mut()
            .resource_mut::<Messages<RenameRequest>>()
            .write(RenameRequest {
                id,
                name: "Renamed".into(),
            });
        app.update();

        assert_eq!(
            app.world().resource::<CatalogSnapshot>().sessions[0].name,
            "Renamed"
        );
    }

    #[test]
    #[cfg(feature = "app")]
    fn legacy_session_without_task_components_remains_in_catalog() {
        let store = r#"(
  resources: {},
  entities: {
    1: (
      components: {
        "bevy_ecs::name::Name": "Task",
        "vmux_ecs::SessionId": ("session-1"),
        "vmux_history::CreatedAt": (1),
        "vmux_history::LastActivatedAt": (2),
        "vmux_session::Session": (),
        "vmux_session::Stage": (("in_progress")),
        "vmux_session::StageChangedAt": (1),
      },
    ),
  },
)"#;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins((vmux_ecs::PrimitivesPlugin, DomainPlugin))
            .add_observer(load_on_default_event);
        app.update();
        app.world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_stream(std::io::Cursor::new(
                store.as_bytes().to_vec(),
            )));

        app.update();

        let catalog = app.world().resource::<CatalogSnapshot>();
        assert_eq!(catalog.sessions.len(), 1);
        assert_eq!(catalog.sessions[0].id, "session-1");
        assert_eq!(catalog.sessions[0].cwd, "");
        assert_eq!(catalog.sessions[0].description, "");
    }
}
