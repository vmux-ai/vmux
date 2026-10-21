#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate};
#[cfg(not(host))]
use bevy_ecs::prelude::Resource;
#[cfg(host)]
use bevy_ecs::prelude::*;
#[cfg(host)]
use bevy_ecs::system::SystemParam;
#[cfg(host)]
use vmux_api::session::{AssistantBlock, Message};
#[cfg(host)]
use vmux_ecs::{CreatedAt, Cwd, Description, LastActivatedAt, Order, Terminal};

#[cfg(host)]
use crate::model::SourceUpdatedAt;
#[cfg(host)]
use crate::{
    AgentConversationTitle, AgentFamily, AgentId, ConversationEvent, MessageContent, ProviderKind,
    ProviderSessionId, ProviderStage, RunState, Session, SessionId, Stage, StageChangedAt,
    StageDefinition, StageId, Transcripts,
};

#[vmux_api::contract(Default, Eq)]
#[derive(Resource)]
pub struct CatalogSnapshot {
    pub stages: Vec<StageSummary>,
    pub sessions: Vec<SessionSummary>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderCatalog {
    pub sessions: Vec<ProviderSession>,
}

impl ProviderCatalog {
    pub fn get(&self, kind: &str, sid: &str) -> Option<&ProviderSession> {
        self.sessions
            .iter()
            .find(|session| session.kind == kind && session.sid == sid)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderSession {
    pub kind: String,
    pub sid: String,
    pub agent: String,
    pub agent_name: String,
    pub name: String,
    pub description: String,
    pub cwd: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub conversation_title: String,
    pub last_prompt: String,
}

impl ProviderSession {
    pub fn catalog_id(&self) -> String {
        format!("{}:{}", self.kind, self.sid)
    }
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
    pub kind: String,
    pub sid: String,
    pub managed: bool,
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
    pub last_prompt: String,
    pub last_message: String,
}

#[cfg(host)]
pub(crate) struct CatalogPlugin;

#[cfg(host)]
impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CatalogSnapshot>()
            .init_resource::<ProviderCatalog>()
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
    provider_stages: Query<
        'w,
        's,
        (),
        (
            With<ProviderStage>,
            Or<(
                Added<ProviderStage>,
                Changed<ProviderKind>,
                Changed<ProviderSessionId>,
                Changed<Stage>,
                Changed<StageChangedAt>,
                Changed<SourceUpdatedAt>,
            )>,
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
    removed_provider_stages: RemovedComponents<'w, 's, ProviderStage>,
    removed_provider_kinds: RemovedComponents<'w, 's, ProviderKind>,
    removed_provider_ids: RemovedComponents<'w, 's, ProviderSessionId>,
}

#[cfg(host)]
impl CatalogChanges<'_, '_> {
    fn any(&mut self) -> bool {
        let mut any = !self.sessions.is_empty()
            || !self.stages.is_empty()
            || !self.events.is_empty()
            || !self.provider_stages.is_empty();
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
        any |= self.removed_provider_stages.read().count() > 0;
        any |= self.removed_provider_kinds.read().count() > 0;
        any |= self.removed_provider_ids.read().count() > 0;
        any
    }
}

#[cfg(host)]
fn project(
    sessions: SessionCatalog,
    stages: Query<(&StageId, &Name, &Order, Has<Terminal>), With<StageDefinition>>,
    providers: Res<ProviderCatalog>,
    provider_stages: Query<
        (
            &ProviderKind,
            &ProviderSessionId,
            &Stage,
            &StageChangedAt,
            &SourceUpdatedAt,
        ),
        With<ProviderStage>,
    >,
    transcripts: Transcripts,
    mut changes: CatalogChanges,
    mut catalog: ResMut<CatalogSnapshot>,
) {
    if !catalog.is_added() && !providers.is_changed() && !changes.any() {
        return;
    }
    let mut next = CatalogSnapshot::default();
    for (id, name, order, terminal) in &stages {
        next.stages.push(StageSummary {
            id: id.0.clone(),
            name: name.as_str().to_string(),
            order: order.0,
            terminal,
        });
    }
    next.stages.sort_by_key(|stage| stage.order);
    let default_stage = next
        .stages
        .iter()
        .find(|stage| !stage.terminal)
        .map(|stage| stage.id.clone())
        .unwrap_or_default();
    for (
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
    ) in &sessions
    {
        let conversation =
            ConversationPreview::from(conversation_title, &transcripts.get(entity).messages);
        let agent_name = agent
            .map(|agent| {
                let fallback = profile
                    .map(|profile| profile.name.as_str())
                    .unwrap_or(agent.0.as_str());
                AgentFamily::of(&agent.0).name(fallback)
            })
            .unwrap_or_default();
        next.sessions.push(SessionSummary {
            id: id.0.clone(),
            kind: "session".to_string(),
            sid: id.0.clone(),
            managed: true,
            name: name.as_str().to_string(),
            description: description.0.clone(),
            cwd: cwd.0.to_string_lossy().into_owned(),
            stage: stage.0.0.clone(),
            created_at: created_at.0,
            last_activated_at: last_activated_at.0,
            stage_changed_at: stage_changed_at.0,
            agent: agent.map(|agent| agent.0.clone()).unwrap_or_default(),
            agent_name,
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
            last_prompt: conversation.last_prompt,
            last_message: conversation.last_message,
        });
    }
    for session in &providers.sessions {
        let stage_override =
            provider_stages
                .iter()
                .find(|(kind, id, _, changed_at, source_updated_at)| {
                    let source_updated_at = if source_updated_at.0 == 0 {
                        changed_at.0
                    } else {
                        source_updated_at.0
                    };
                    kind.0 == session.kind
                        && id.0 == session.sid
                        && source_updated_at >= session.updated_at
                });
        let stage = stage_override
            .map(|(_, _, stage, _, _)| stage.0.0.clone())
            .unwrap_or_else(|| default_stage.clone());
        let stage_changed_at = stage_override
            .map(|(_, _, _, changed_at, _)| changed_at.0)
            .unwrap_or(session.updated_at);
        next.sessions.push(SessionSummary {
            id: session.catalog_id(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            managed: false,
            name: session.name.clone(),
            description: session.description.clone(),
            cwd: session.cwd.clone(),
            stage,
            created_at: session.created_at,
            last_activated_at: session.updated_at,
            stage_changed_at,
            agent: session.agent.clone(),
            agent_name: AgentFamily::of(&session.agent).name(&session.agent_name),
            runtime: "inactive".to_string(),
            conversation_title: session.conversation_title.clone(),
            last_prompt: session.last_prompt.clone(),
            ..Default::default()
        });
    }
    next.sessions
        .sort_by_key(|session| std::cmp::Reverse(session.last_activated_at));
    if *catalog != next {
        *catalog = next;
    }
}

#[cfg(host)]
struct ConversationPreview {
    title: String,
    last_prompt: String,
    last_message: String,
}

#[cfg(host)]
impl ConversationPreview {
    fn from(title: Option<&AgentConversationTitle>, messages: &[Message]) -> Self {
        let title = title
            .map(|title| Self::compact(&title.0))
            .unwrap_or_default();
        let last_prompt = messages
            .iter()
            .rev()
            .find_map(Self::prompt)
            .unwrap_or_default();
        let last_message = messages
            .iter()
            .rev()
            .find_map(Self::message)
            .unwrap_or_default();
        Self {
            title,
            last_prompt,
            last_message,
        }
    }

    fn prompt(message: &Message) -> Option<String> {
        let Message::User { text, .. } = message else {
            return None;
        };
        let text = Self::compact(text);
        (!text.is_empty()).then_some(text)
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
                    Message::user("Make it feel more like Linear"),
                    Message::Assistant {
                        blocks: vec![AssistantBlock::Text(
                            "Aligned the session list with Linear".into(),
                        )],
                    },
                ],
            });
        app.update();

        let summary = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(summary.conversation_title, "Fix session manager");
        assert_eq!(summary.last_prompt, "Make it feel more like Linear");
        assert_eq!(summary.last_message, "Aligned the session list with Linear");

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
        assert_eq!(
            app.world().resource::<CatalogSnapshot>().sessions[0].conversation_title,
            "Renamed"
        );
    }

    #[test]
    fn catalog_applies_provider_stage_override_without_importing_the_session() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "codex".into(),
                sid: "provider-session".into(),
                agent: "codex-cli".into(),
                name: "External task".into(),
                cwd: "/tmp/project".into(),
                updated_at: 10,
                ..Default::default()
            });
        app.update();

        app.world_mut()
            .resource_mut::<Messages<crate::ProviderStageChangeRequest>>()
            .write(crate::ProviderStageChangeRequest {
                kind: ProviderKind("codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId("done".into()),
                source_updated_at: 10,
            });
        app.update();

        let catalog = app.world().resource::<CatalogSnapshot>();
        assert_eq!(catalog.sessions.len(), 1);
        assert_eq!(catalog.sessions[0].id, "codex:provider-session");
        assert!(!catalog.sessions[0].managed);
        assert_eq!(catalog.sessions[0].stage, "done");
        assert!(catalog.sessions[0].stage_changed_at > 10);
        let imported = app
            .world_mut()
            .query_filtered::<Entity, With<Session>>()
            .iter(app.world())
            .count();
        assert_eq!(imported, 0);
    }

    #[test]
    fn provider_activity_after_archive_returns_to_the_default_stage() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                name: "External task".into(),
                cwd: "/tmp/project".into(),
                updated_at: 10,
                ..Default::default()
            });
        app.update();
        app.world_mut()
            .resource_mut::<Messages<crate::ProviderStageChangeRequest>>()
            .write(crate::ProviderStageChangeRequest {
                kind: ProviderKind("acp:codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId("done".into()),
                source_updated_at: 10,
            });
        app.update();
        let archived_at = app.world().resource::<CatalogSnapshot>().sessions[0].stage_changed_at;
        app.world_mut().resource_mut::<ProviderCatalog>().sessions[0].updated_at = archived_at + 1;
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "in_progress");
        assert_eq!(session.stage_changed_at, archived_at + 1);

        app.world_mut()
            .resource_mut::<Messages<crate::ProviderStageChangeRequest>>()
            .write(crate::ProviderStageChangeRequest {
                kind: ProviderKind("acp:codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId("done".into()),
                source_updated_at: archived_at + 1,
            });
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "done");
        assert!(session.stage_changed_at > archived_at);
    }

    #[test]
    fn legacy_provider_stage_uses_transition_time_as_source_fallback() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                name: "External task".into(),
                cwd: "/tmp/project".into(),
                updated_at: 10,
                ..Default::default()
            });
        app.world_mut().spawn((
            ProviderStage,
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
            Stage(StageId("done".into())),
            StageChangedAt(20),
            SourceUpdatedAt(0),
        ));
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "done");
        assert_eq!(session.stage_changed_at, 20);

        app.world_mut().resource_mut::<ProviderCatalog>().sessions[0].updated_at = 21;
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "in_progress");
        assert_eq!(session.stage_changed_at, 21);
    }

    #[test]
    fn provider_source_timestamp_remains_authoritative() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                name: "External task".into(),
                cwd: "/tmp/project".into(),
                updated_at: 150,
                ..Default::default()
            });
        app.world_mut().spawn((
            ProviderStage,
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
            Stage(StageId("done".into())),
            StageChangedAt(200),
            SourceUpdatedAt(100),
        ));
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "in_progress");
        assert_eq!(session.stage_changed_at, 150);
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
