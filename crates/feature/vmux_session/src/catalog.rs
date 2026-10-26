#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate};
#[cfg(not(host))]
use bevy_ecs::prelude::Resource;
#[cfg(host)]
use bevy_ecs::prelude::*;
#[cfg(host)]
use bevy_ecs::system::SystemParam;
#[cfg(host)]
use std::collections::{HashMap, HashSet};
#[cfg(host)]
use vmux_api::Id;
#[cfg(host)]
use vmux_api::session::{AssistantBlock, Message};
#[cfg(host)]
use vmux_connector::{
    ExternalId, ExternalLink, ExternalLinkIdentity, ExternalScope, ExternalScopeRef, ProviderKey,
    ResourceKind,
};
#[cfg(host)]
use vmux_ecs::{
    ArchivedAt, CreatedAt, Cwd, Description, LastActivatedAt, Order, RemoteUpdatedAt, Terminal,
};

#[cfg(host)]
use crate::model::{ProviderArchiveOperations, ProviderSessionTombstones};
#[cfg(host)]
use crate::{
    AgentConversationTitle, AgentFamily, AgentId, ConversationEvent, MessageContent,
    ProviderArchive, ProviderStage, RunState, Session, SessionDuplicate, Stage, StageChangedAt,
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
    pub archive: Option<ArchiveState>,
}

impl ProviderSession {
    pub fn catalog_id(&self) -> String {
        format!("{}:{}", self.kind, self.sid)
    }

    pub fn source_updated_at(&self) -> i64 {
        self.archive
            .map(ArchiveState::updated_at)
            .unwrap_or_default()
            .max(self.updated_at)
    }

    #[cfg(host)]
    pub fn identity(&self) -> Option<ExternalLinkIdentity> {
        let provider = self
            .kind
            .split_once(':')
            .map(|(provider, _)| provider)
            .unwrap_or(self.kind.as_str());
        Some(ExternalLinkIdentity::new(
            ProviderKey::new(provider).ok()?,
            Id::<ExternalScope>::new(self.kind.clone()),
            ResourceKind::session(),
            ExternalId::new(self.sid.clone()).ok()?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveState {
    Active(i64),
    Archived(i64),
}

impl ArchiveState {
    pub fn updated_at(self) -> i64 {
        match self {
            Self::Active(at) | Self::Archived(at) => at,
        }
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
    pub provider_kind: String,
    pub provider_sid: String,
    pub managed: bool,
    pub name: String,
    pub description: String,
    pub cwd: String,
    pub stage: String,
    pub created_at: i64,
    pub last_activated_at: i64,
    pub stage_changed_at: i64,
    pub archived_at: i64,
    pub agent: String,
    pub agent_name: String,
    pub agent_icon: String,
    pub agent_color: String,
    pub runtime: String,
    pub conversation_title: String,
    pub last_prompt: String,
    pub last_message: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionAge(pub u64);

impl SessionAge {
    pub const MINUTE: u64 = 60;
    pub const HOUR: u64 = 60 * Self::MINUTE;
    pub const DAY: u64 = 24 * Self::HOUR;
    pub const WEEK: u64 = 7 * Self::DAY;
    pub const MONTH: u64 = 30 * Self::DAY;
    pub const YEAR: u64 = 365 * Self::DAY;

    pub fn from_millis(timestamp: i64, now: i64) -> Self {
        if timestamp <= 0 {
            return Self(0);
        }
        Self(now.saturating_sub(timestamp).max(0) as u64 / 1_000)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSummaryProjection {
    pub title: String,
    pub preview: String,
    pub age: SessionAge,
    pub terminal: bool,
    pub archived: bool,
    pub stale: bool,
}

impl SessionSummaryProjection {
    pub const STALE_AFTER: u64 = 3 * SessionAge::DAY;

    pub fn new(session: &SessionSummary, stages: &[StageSummary], now: i64) -> Self {
        let title = Self::title(session);
        let stage = Self::stage(session, stages);
        let terminal = stage.is_some_and(|stage| stage.terminal);
        let archived = session.archived_at > 0;
        let age = SessionAge::from_millis(session.last_activated_at, now);
        let stale = Self::stale(session, terminal, age);
        Self {
            preview: Self::preview(session, &title),
            title,
            age,
            terminal,
            archived,
            stale,
        }
    }

    pub fn is_stale(session: &SessionSummary, stages: &[StageSummary], now: i64) -> bool {
        let terminal = Self::stage(session, stages).is_some_and(|stage| stage.terminal);
        let age = SessionAge::from_millis(session.last_activated_at, now);
        Self::stale(session, terminal, age)
    }

    fn stage<'a>(session: &SessionSummary, stages: &'a [StageSummary]) -> Option<&'a StageSummary> {
        stages.iter().find(|stage| stage.id == session.stage)
    }

    fn stale(session: &SessionSummary, terminal: bool, age: SessionAge) -> bool {
        let active = matches!(
            session.runtime.as_str(),
            "streaming" | "awaiting" | "installing" | "errored"
        );
        !terminal && !active && age.0 >= Self::STALE_AFTER
    }

    pub fn matches(session: &SessionSummary, query: &str) -> bool {
        query.is_empty()
            || session.name.to_ascii_lowercase().contains(query)
            || session.description.to_ascii_lowercase().contains(query)
            || session.cwd.to_ascii_lowercase().contains(query)
            || session.agent.to_ascii_lowercase().contains(query)
            || session.agent_name.to_ascii_lowercase().contains(query)
            || session
                .conversation_title
                .to_ascii_lowercase()
                .contains(query)
            || session.last_prompt.to_ascii_lowercase().contains(query)
            || session.last_message.to_ascii_lowercase().contains(query)
    }

    fn title(session: &SessionSummary) -> String {
        for title in [
            &session.conversation_title,
            &session.name,
            &session.last_prompt,
            &session.sid,
        ] {
            if !title.trim().is_empty() {
                return title.clone();
            }
        }
        String::new()
    }

    fn preview(session: &SessionSummary, title: &str) -> String {
        for preview in [
            &session.last_prompt,
            &session.last_message,
            &session.description,
        ] {
            if !preview.trim().is_empty() && preview != title {
                return preview.clone();
            }
        }
        String::new()
    }
}

#[cfg(host)]
pub(crate) struct CatalogPlugin;

#[cfg(host)]
impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CatalogSnapshot>()
            .init_resource::<ProviderCatalog>()
            .add_systems(PostUpdate, project.after(crate::model::CanonicalizeSet));
    }
}

#[cfg(host)]
type SessionCatalog<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Session>,
        &'static Name,
        &'static Description,
        &'static Cwd,
        &'static Stage,
        &'static CreatedAt,
        &'static LastActivatedAt,
        &'static StageChangedAt,
        Option<&'static ArchivedAt>,
        Option<&'static AgentId>,
        Option<&'static vmux_ecs::team::Profile>,
        Option<&'static RunState>,
        Option<&'static AgentConversationTitle>,
    ),
    (With<Session>, Without<SessionDuplicate>),
>;

#[cfg(host)]
type LinkedSessions<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static ProviderKey,
        &'static ExternalScopeRef,
        &'static ResourceKind,
        &'static ExternalId,
    ),
    With<ExternalLink>,
>;

#[cfg(host)]
type StageCatalog<'w, 's> = Query<
    'w,
    's,
    (
        &'static StageId,
        &'static Name,
        &'static Order,
        Has<Terminal>,
    ),
    With<StageDefinition>,
>;

#[cfg(host)]
type ProviderStages<'w, 's> = Query<
    'w,
    's,
    (
        &'static ProviderKey,
        &'static ExternalScopeRef,
        &'static ResourceKind,
        &'static ExternalId,
        &'static Stage,
        &'static StageChangedAt,
        &'static RemoteUpdatedAt,
    ),
    With<ProviderStage>,
>;

#[cfg(host)]
type ProviderArchives<'w, 's> = Query<
    'w,
    's,
    (
        &'static ProviderKey,
        &'static ExternalScopeRef,
        &'static ResourceKind,
        &'static ExternalId,
        &'static RemoteUpdatedAt,
        Option<&'static ArchivedAt>,
    ),
    With<ProviderArchive>,
>;

#[cfg(host)]
#[derive(SystemParam)]
struct CatalogSources<'w, 's> {
    sessions: SessionCatalog<'w, 's>,
    linked_sessions: LinkedSessions<'w, 's>,
    stages: StageCatalog<'w, 's>,
    providers: Res<'w, ProviderCatalog>,
    operations: ProviderArchiveOperations<'w, 's>,
    provider_stages: ProviderStages<'w, 's>,
    provider_archives: ProviderArchives<'w, 's>,
    provider_tombstones: Res<'w, ProviderSessionTombstones>,
    transcripts: Transcripts<'w, 's>,
}

#[cfg(host)]
type ChangedSessions<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Session>,
        Or<(
            Changed<Id<Session>>,
            Changed<Name>,
            Changed<Description>,
            Changed<Cwd>,
            Changed<Stage>,
            Changed<CreatedAt>,
            Changed<LastActivatedAt>,
            Changed<StageChangedAt>,
            Changed<ArchivedAt>,
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
    links: Query<
        'w,
        's,
        (),
        (
            With<ExternalLink>,
            Or<(
                Added<ExternalLink>,
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
                Changed<ChildOf>,
            )>,
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
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
                Changed<Stage>,
                Changed<StageChangedAt>,
                Changed<RemoteUpdatedAt>,
            )>,
        ),
    >,
    provider_archives: Query<
        'w,
        's,
        (),
        (
            With<ProviderArchive>,
            Or<(
                Added<ProviderArchive>,
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
                Changed<RemoteUpdatedAt>,
                Changed<ArchivedAt>,
            )>,
        ),
    >,
    duplicates: Query<'w, 's, (), Or<(Added<SessionDuplicate>, Changed<SessionDuplicate>)>>,
    removed_sessions: RemovedComponents<'w, 's, Session>,
    removed_ids: RemovedComponents<'w, 's, Id<Session>>,
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
    removed_archived_at: RemovedComponents<'w, 's, ArchivedAt>,
    removed_agents: RemovedComponents<'w, 's, AgentId>,
    removed_profiles: RemovedComponents<'w, 's, vmux_ecs::team::Profile>,
    removed_states: RemovedComponents<'w, 's, RunState>,
    removed_titles: RemovedComponents<'w, 's, AgentConversationTitle>,
    removed_events: RemovedComponents<'w, 's, ConversationEvent>,
    removed_messages: RemovedComponents<'w, 's, MessageContent>,
    removed_provider_stages: RemovedComponents<'w, 's, ProviderStage>,
    removed_provider_archives: RemovedComponents<'w, 's, ProviderArchive>,
    removed_duplicates: RemovedComponents<'w, 's, SessionDuplicate>,
    removed_links: RemovedComponents<'w, 's, ExternalLink>,
    removed_providers: RemovedComponents<'w, 's, ProviderKey>,
    removed_scopes: RemovedComponents<'w, 's, ExternalScopeRef>,
    removed_resource_kinds: RemovedComponents<'w, 's, ResourceKind>,
    removed_external_ids: RemovedComponents<'w, 's, ExternalId>,
    removed_owners: RemovedComponents<'w, 's, ChildOf>,
}

#[cfg(host)]
impl CatalogChanges<'_, '_> {
    fn any(&mut self) -> bool {
        let mut any = !self.sessions.is_empty()
            || !self.stages.is_empty()
            || !self.events.is_empty()
            || !self.links.is_empty()
            || !self.provider_stages.is_empty()
            || !self.provider_archives.is_empty()
            || !self.duplicates.is_empty();
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
        any |= self.removed_archived_at.read().count() > 0;
        any |= self.removed_agents.read().count() > 0;
        any |= self.removed_profiles.read().count() > 0;
        any |= self.removed_states.read().count() > 0;
        any |= self.removed_titles.read().count() > 0;
        any |= self.removed_events.read().count() > 0;
        any |= self.removed_messages.read().count() > 0;
        any |= self.removed_provider_stages.read().count() > 0;
        any |= self.removed_provider_archives.read().count() > 0;
        any |= self.removed_duplicates.read().count() > 0;
        any |= self.removed_links.read().count() > 0;
        any |= self.removed_providers.read().count() > 0;
        any |= self.removed_scopes.read().count() > 0;
        any |= self.removed_resource_kinds.read().count() > 0;
        any |= self.removed_external_ids.read().count() > 0;
        any |= self.removed_owners.read().count() > 0;
        any
    }
}

#[cfg(host)]
fn project(
    sources: CatalogSources,
    mut changes: CatalogChanges,
    mut catalog: ResMut<CatalogSnapshot>,
) {
    let CatalogSources {
        sessions,
        linked_sessions,
        stages,
        providers,
        operations,
        provider_stages,
        provider_archives,
        provider_tombstones,
        transcripts,
    } = sources;
    if !catalog.is_added()
        && !providers.is_changed()
        && !operations.is_changed()
        && !provider_tombstones.is_changed()
        && !changes.any()
    {
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
    let linked_sessions = linked_sessions
        .iter()
        .filter(|(_, _, _, kind, _)| kind.as_str() == "session")
        .map(|(owner, provider, scope, kind, id)| {
            (
                owner.parent(),
                ExternalLinkIdentity::new(
                    provider.clone(),
                    scope.id().clone(),
                    kind.clone(),
                    id.clone(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
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
        archived_at,
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
        let archived_at = linked_sessions
            .get(&entity)
            .and_then(|identity| operations.archived_at(identity))
            .unwrap_or_else(|| {
                archived_at
                    .map(|archived_at| archived_at.0)
                    .unwrap_or_default()
            });
        next.sessions.push(SessionSummary {
            id: id.as_str().to_string(),
            kind: "session".to_string(),
            sid: id.as_str().to_string(),
            provider_kind: linked_sessions
                .get(&entity)
                .map(|identity| identity.scope().as_str().to_string())
                .unwrap_or_default(),
            provider_sid: linked_sessions
                .get(&entity)
                .map(|identity| identity.external_id().as_str().to_string())
                .unwrap_or_default(),
            managed: true,
            name: name.as_str().to_string(),
            description: description.0.clone(),
            cwd: cwd.0.to_string_lossy().into_owned(),
            stage: stage.0.0.clone(),
            created_at: created_at.0,
            last_activated_at: last_activated_at.0,
            stage_changed_at: stage_changed_at.0,
            archived_at,
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
    let linked_sessions = linked_sessions.values().cloned().collect::<HashSet<_>>();
    let provider_stages = provider_stages
        .iter()
        .map(
            |(provider, scope, kind, id, stage, changed_at, source_updated_at)| {
                (
                    ExternalLinkIdentity::new(
                        provider.clone(),
                        scope.id().clone(),
                        kind.clone(),
                        id.clone(),
                    ),
                    (stage.0.0.clone(), changed_at.0, source_updated_at.0.0),
                )
            },
        )
        .collect::<HashMap<_, _>>();
    let provider_archives = provider_archives
        .iter()
        .map(
            |(provider, scope, kind, id, source_updated_at, archived_at)| {
                (
                    ExternalLinkIdentity::new(
                        provider.clone(),
                        scope.id().clone(),
                        kind.clone(),
                        id.clone(),
                    ),
                    (
                        source_updated_at.0.0,
                        archived_at.map(|archived_at| archived_at.0),
                    ),
                )
            },
        )
        .collect::<HashMap<_, _>>();
    for session in &providers.sessions {
        let Some(key) = session.identity() else {
            continue;
        };
        if linked_sessions.contains(&key) || provider_tombstones.contains(&key) {
            continue;
        }
        let provider_updated_at = session.source_updated_at();
        let stage_override =
            provider_stages
                .get(&key)
                .filter(|(_, changed_at, source_updated_at)| {
                    let source_updated_at = if *source_updated_at == 0 {
                        *changed_at
                    } else {
                        *source_updated_at
                    };
                    source_updated_at >= provider_updated_at
                });
        let stage = stage_override
            .map(|(stage, _, _)| stage.clone())
            .unwrap_or_else(|| default_stage.clone());
        let stage_changed_at = stage_override
            .map(|(_, changed_at, _)| *changed_at)
            .unwrap_or(provider_updated_at);
        let archive_override = provider_archives
            .get(&key)
            .filter(|(source_updated_at, _)| {
                session.archive.is_none()
                    || *source_updated_at
                        >= session
                            .archive
                            .map(ArchiveState::updated_at)
                            .unwrap_or_default()
            });
        let archived_at = if let Some(pending) = operations.archived_at(&key) {
            pending
        } else if let Some((_, archived_at)) = archive_override {
            archived_at.unwrap_or_default()
        } else {
            match session.archive {
                Some(ArchiveState::Archived(at)) if at > 0 => at,
                Some(ArchiveState::Archived(_)) => session.updated_at,
                Some(ArchiveState::Active(_)) | None => 0,
            }
        };
        next.sessions.push(SessionSummary {
            id: session.catalog_id(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            provider_kind: session.kind.clone(),
            provider_sid: session.sid.clone(),
            managed: false,
            name: session.name.clone(),
            description: session.description.clone(),
            cwd: session.cwd.clone(),
            stage,
            created_at: session.created_at,
            last_activated_at: session.updated_at,
            stage_changed_at,
            archived_at,
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

    struct ProviderFixture;

    impl ProviderFixture {
        fn identity(kind: &str, id: &str) -> ExternalLinkIdentity {
            ProviderSession {
                kind: kind.to_string(),
                sid: id.to_string(),
                ..Default::default()
            }
            .identity()
            .unwrap()
        }

        fn components(
            kind: &str,
            id: &str,
        ) -> (ProviderKey, ExternalScopeRef, ResourceKind, ExternalId) {
            let identity = Self::identity(kind, id);
            (
                identity.provider().clone(),
                ExternalScopeRef::new(identity.scope().clone()),
                identity.kind().clone(),
                identity.external_id().clone(),
            )
        }
    }

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
        assert_eq!(catalog.sessions[0].id, id.as_str());
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
                identity: ProviderFixture::identity("codex", "provider-session"),
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
    fn provider_removal_hides_the_session_across_source_updates() {
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
                updated_at: 10,
                ..Default::default()
            });
        app.update();
        assert_eq!(app.world().resource::<CatalogSnapshot>().sessions.len(), 1);

        let identity = app.world().resource::<ProviderCatalog>().sessions[0]
            .identity()
            .unwrap();
        app.world_mut()
            .resource_mut::<Messages<crate::CleanupRequest>>()
            .write(crate::CleanupRequest {
                targets: vec![crate::CleanupTarget::Provider(identity)],
            });
        app.update();
        assert!(
            app.world()
                .resource::<CatalogSnapshot>()
                .sessions
                .is_empty()
        );

        app.world_mut().resource_mut::<ProviderCatalog>().sessions[0].updated_at = 20;
        app.update();
        assert!(
            app.world()
                .resource::<CatalogSnapshot>()
                .sessions
                .is_empty()
        );
        let removals = app
            .world_mut()
            .query_filtered::<Entity, With<crate::ProviderSessionTombstone>>()
            .iter(app.world())
            .count();
        assert_eq!(removals, 1);
    }

    #[test]
    fn provider_archive_state_is_independent_from_stage_state() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        app.update();
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
                archive: Some(ArchiveState::Archived(20)),
                ..Default::default()
            });
        app.world_mut()
            .resource_mut::<Messages<crate::ProviderStageChangeRequest>>()
            .write(crate::ProviderStageChangeRequest {
                identity: ProviderFixture::identity("acp:codex", "provider-session"),
                stage: StageId("in_progress".into()),
                source_updated_at: 100,
            });
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "in_progress");
        assert_eq!(session.archived_at, 20);

        app.world_mut().resource_mut::<ProviderCatalog>().sessions[0].archive =
            Some(ArchiveState::Active(30));
        app.world_mut().resource_mut::<ProviderCatalog>().sessions[0].updated_at = 30;
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "in_progress");
        assert_eq!(session.archived_at, 0);
        assert!(session.stage_changed_at > 30);

        app.world_mut().resource_mut::<ProviderCatalog>().sessions[0].archive =
            Some(ArchiveState::Active(101));
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "in_progress");
        assert_eq!(session.archived_at, 0);
        assert_eq!(session.stage_changed_at, 101);
    }

    #[test]
    fn provider_archive_override_is_fallback_until_native_state_catches_up() {
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
            .resource_mut::<Messages<crate::ProviderArchiveRequest>>()
            .write(crate::ProviderArchiveRequest {
                identity: ProviderFixture::identity("acp:codex", "provider-session"),
                archived: true,
                source_updated_at: 10,
                native: false,
            });
        app.update();

        let archived = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert!(archived.archived_at > 0);
        assert_eq!(archived.stage, "in_progress");

        let native_updated_at = archived.archived_at + 1;
        let provider = &mut app.world_mut().resource_mut::<ProviderCatalog>().sessions[0];
        provider.archive = Some(ArchiveState::Active(native_updated_at));
        app.update();

        let active = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(active.archived_at, 0);
        assert_eq!(active.stage, "in_progress");
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
                identity: ProviderFixture::identity("acp:codex", "provider-session"),
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
                identity: ProviderFixture::identity("acp:codex", "provider-session"),
                stage: StageId("done".into()),
                source_updated_at: archived_at + 1,
            });
        app.update();

        let session = &app.world().resource::<CatalogSnapshot>().sessions[0];
        assert_eq!(session.stage, "done");
        assert!(session.stage_changed_at > archived_at);
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
        let identity = ProviderFixture::components("acp:codex", "provider-session");
        app.world_mut().spawn((
            ProviderStage,
            identity,
            Stage(StageId("done".into())),
            StageChangedAt(200),
            RemoteUpdatedAt(vmux_ecs::UnixMillis(100)),
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
        "vmux_api::Id<vmux_api::Session>": (value: "session-1"),
        "vmux_history::CreatedAt": (1),
        "vmux_history::LastActivatedAt": (2),
        "vmux_api::Session": (),
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
