use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::name::Name;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query};
use vmux_api::session::{RemoteAgent, RemoteApproval, RemoteSession, RemoteStatus, SessionId};
use vmux_ecs::{CreatedAt, Cwd, Order};
use vmux_session::host::Agents;
use vmux_start::roster::Roster;

use crate::connection::{ConnectionSource, MobileRoot, SourceEpoch, SourceInvalidated};
use crate::{InvalidateSet, MaterializeSet, ProjectSet};

pub(crate) struct MobileCatalogPlugin;

impl BevyPlugin for MobileCatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ReconcileSessions>()
            .add_message::<ReconcileAgents>()
            .add_message::<UpdateSessionItem>()
            .add_systems(PreUpdate, clear.in_set(InvalidateSet))
            .add_systems(
                PreUpdate,
                (reconcile_sessions, reconcile_agents, update_session)
                    .chain()
                    .in_set(MaterializeSet),
            )
            .add_systems(PreUpdate, project.in_set(ProjectSet));
    }
}

#[derive(Component)]
pub(crate) struct RemoteSessionItem;

#[derive(Component, Clone, PartialEq)]
pub(crate) struct RemoteSessionDetails {
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) runtime: String,
    pub(crate) model: Option<String>,
    pub(crate) status: RemoteStatus,
    pub(crate) approval: Option<RemoteApproval>,
}

impl RemoteSessionDetails {
    pub(crate) fn from_wire(session: &RemoteSession) -> Self {
        Self {
            url: session.url.clone(),
            title: session.title.clone(),
            runtime: session.runtime.clone(),
            model: session.model.clone(),
            status: session.status.clone(),
            approval: session.approval.clone(),
        }
    }

    pub(crate) fn to_wire(
        &self,
        id: &SessionId,
        name: &Name,
        cwd: &Cwd,
        created_at: &CreatedAt,
    ) -> RemoteSession {
        RemoteSession {
            id: id.clone(),
            url: self.url.clone(),
            title: self.title.clone(),
            name: name.as_str().to_string(),
            runtime: self.runtime.clone(),
            model: self.model.clone(),
            cwd: cwd.0.to_string_lossy().into_owned(),
            status: self.status.clone(),
            approval: self.approval.clone(),
            created_at_ms: u64::try_from(created_at.0).unwrap_or(0),
        }
    }
}

#[derive(Component)]
pub(crate) struct RemoteAgentItem;

#[derive(Component, Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RemoteAgentId(pub(crate) String);

#[derive(Component, Clone, PartialEq)]
pub(crate) struct RemoteAgentDetails {
    pub(crate) url: String,
    pub(crate) icon: String,
}

#[derive(Component, Default)]
struct CatalogProjectionState {
    last_sessions: Vec<RemoteSession>,
    last_agents: Vec<RemoteAgent>,
    published: bool,
}

#[derive(Message)]
pub(crate) struct ReconcileSessions {
    pub(crate) source: Entity,
    pub(crate) epoch: u64,
    pub(crate) sessions: Vec<RemoteSession>,
}

#[derive(Message)]
pub(crate) struct ReconcileAgents {
    pub(crate) source: Entity,
    pub(crate) epoch: u64,
    pub(crate) agents: Vec<RemoteAgent>,
}

#[derive(Message)]
pub(crate) struct UpdateSessionItem {
    pub(crate) source: Entity,
    pub(crate) epoch: u64,
    pub(crate) session: RemoteSession,
}

fn clear(
    mut invalidated: MessageReader<SourceInvalidated>,
    sessions: Query<(Entity, &ChildOf), With<RemoteSessionItem>>,
    agents: Query<(Entity, &ChildOf), With<RemoteAgentItem>>,
    mut commands: Commands,
) {
    let mut cleared = HashSet::new();
    for event in invalidated.read() {
        cleared.insert(event.source);
    }
    if cleared.is_empty() {
        return;
    }
    for (entity, parent) in &sessions {
        if cleared.contains(&parent.parent()) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, parent) in &agents {
        if cleared.contains(&parent.parent()) {
            commands.entity(entity).despawn();
        }
    }
}

type ExistingSessionsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static SessionId,
        &'static mut Name,
        &'static mut Cwd,
        &'static mut Order,
        &'static mut CreatedAt,
        &'static mut RemoteSessionDetails,
    ),
    With<RemoteSessionItem>,
>;

fn reconcile_sessions(
    mut events: MessageReader<ReconcileSessions>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut existing: ExistingSessionsQuery,
    mut commands: Commands,
) {
    for event in events.read() {
        let Ok(epoch) = sources.get(event.source) else {
            continue;
        };
        if epoch.0 != event.epoch {
            continue;
        }
        let mut by_id = HashMap::new();
        for (entity, parent, id, ..) in existing.iter() {
            if parent.parent() == event.source {
                by_id.insert(id.clone(), entity);
            }
        }
        let mut seen = HashSet::with_capacity(event.sessions.len());
        for (index, session) in event.sessions.iter().enumerate() {
            seen.insert(session.id.clone());
            let details = RemoteSessionDetails::from_wire(session);
            let order = Order(index as u32);
            let cwd_path = std::path::PathBuf::from(&session.cwd);
            let created_ms = i64::try_from(session.created_at_ms).unwrap_or(i64::MAX);
            if let Some(&entity) = by_id.get(&session.id)
                && let Ok((_, _, _, mut name, mut cwd, mut current_order, mut created, mut current)) =
                    existing.get_mut(entity)
            {
                if name.as_str() != session.name {
                    *name = Name::new(session.name.clone());
                }
                if cwd.0 != cwd_path {
                    cwd.0 = cwd_path;
                }
                if *current_order != order {
                    *current_order = order;
                }
                if created.0 != created_ms {
                    created.0 = created_ms;
                }
                if *current != details {
                    *current = details;
                }
            } else {
                commands.spawn((
                    ChildOf(event.source),
                    RemoteSessionItem,
                    session.id.clone(),
                    Name::new(session.name.clone()),
                    Cwd(cwd_path),
                    order,
                    CreatedAt(created_ms),
                    details,
                ));
            }
        }
        for (id, entity) in by_id {
            if !seen.contains(&id) {
                commands.entity(entity).despawn();
            }
        }
    }
}

type ExistingAgentsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static RemoteAgentId,
        &'static mut Name,
        &'static mut Order,
        &'static mut RemoteAgentDetails,
    ),
    With<RemoteAgentItem>,
>;

fn reconcile_agents(
    mut events: MessageReader<ReconcileAgents>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut existing: ExistingAgentsQuery,
    mut commands: Commands,
) {
    for event in events.read() {
        let Ok(epoch) = sources.get(event.source) else {
            continue;
        };
        if epoch.0 != event.epoch {
            continue;
        }
        let mut by_id = HashMap::new();
        for (entity, parent, id, ..) in existing.iter() {
            if parent.parent() == event.source {
                by_id.insert(id.0.clone(), entity);
            }
        }
        let mut seen = HashSet::with_capacity(event.agents.len());
        for (index, agent) in event.agents.iter().enumerate() {
            seen.insert(agent.id.clone());
            let details = RemoteAgentDetails {
                url: agent.url.clone(),
                icon: agent.icon.clone(),
            };
            let order = Order(index as u32);
            if let Some(&entity) = by_id.get(&agent.id)
                && let Ok((_, _, _, mut name, mut current_order, mut current)) =
                    existing.get_mut(entity)
            {
                if name.as_str() != agent.name {
                    *name = Name::new(agent.name.clone());
                }
                if *current_order != order {
                    *current_order = order;
                }
                if *current != details {
                    *current = details;
                }
            } else {
                commands.spawn((
                    ChildOf(event.source),
                    RemoteAgentItem,
                    RemoteAgentId(agent.id.clone()),
                    Name::new(agent.name.clone()),
                    order,
                    details,
                ));
            }
        }
        for (id, entity) in by_id {
            if !seen.contains(&id) {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn update_session(
    mut events: MessageReader<UpdateSessionItem>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut existing: ExistingSessionsQuery,
) {
    for event in events.read() {
        let Ok(epoch) = sources.get(event.source) else {
            continue;
        };
        if epoch.0 != event.epoch {
            continue;
        }
        for (_, parent, id, mut name, mut cwd, _, mut created, mut details) in &mut existing {
            if parent.parent() != event.source || *id != event.session.id {
                continue;
            }
            if name.as_str() != event.session.name {
                *name = Name::new(event.session.name.clone());
            }
            let cwd_path = std::path::PathBuf::from(&event.session.cwd);
            if cwd.0 != cwd_path {
                cwd.0 = cwd_path;
            }
            let created_ms = i64::try_from(event.session.created_at_ms).unwrap_or(i64::MAX);
            if created.0 != created_ms {
                created.0 = created_ms;
            }
            let updated = RemoteSessionDetails::from_wire(&event.session);
            if *details != updated {
                *details = updated;
            }
        }
    }
}

type ProjectSessionsQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Order,
        &'static SessionId,
        &'static Name,
        &'static Cwd,
        &'static CreatedAt,
        &'static RemoteSessionDetails,
    ),
    With<RemoteSessionItem>,
>;

type ProjectAgentsQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Order,
        &'static RemoteAgentId,
        &'static Name,
        &'static RemoteAgentDetails,
    ),
    With<RemoteAgentItem>,
>;

fn project(
    sources: Query<Entity, With<ConnectionSource>>,
    mut roots: Query<(Entity, Option<&mut CatalogProjectionState>), With<MobileRoot>>,
    sessions_query: ProjectSessionsQuery,
    agents_query: ProjectAgentsQuery,
    mut rosters: MessageWriter<Roster>,
    mut agent_messages: MessageWriter<Agents>,
    mut commands: Commands,
) {
    let Ok(source) = sources.single() else {
        return;
    };
    let Ok((root, projection)) = roots.single_mut() else {
        return;
    };
    let mut ordered_sessions = Vec::new();
    for (parent, order, id, name, cwd, created, details) in &sessions_query {
        if parent.parent() == source {
            ordered_sessions.push((order.0, details.to_wire(id, name, cwd, created)));
        }
    }
    ordered_sessions.sort_unstable_by_key(|(order, _)| *order);
    let mut sessions = Vec::with_capacity(ordered_sessions.len());
    for (_, session) in ordered_sessions {
        sessions.push(session);
    }

    let mut ordered_agents = Vec::new();
    for (parent, order, id, name, details) in &agents_query {
        if parent.parent() == source {
            ordered_agents.push((
                order.0,
                RemoteAgent {
                    id: id.0.clone(),
                    name: name.as_str().to_string(),
                    url: details.url.clone(),
                    icon: details.icon.clone(),
                },
            ));
        }
    }
    ordered_agents.sort_unstable_by_key(|(order, _)| *order);
    let mut agents = Vec::with_capacity(ordered_agents.len());
    for (_, agent) in ordered_agents {
        agents.push(agent);
    }

    let Some(mut state) = projection else {
        rosters.write(Roster {
            sessions: sessions.clone(),
            agents: agents.clone(),
        });
        agent_messages.write(Agents(agents.clone()));
        commands.entity(root).insert(CatalogProjectionState {
            last_sessions: sessions,
            last_agents: agents,
            published: true,
        });
        return;
    };
    if !state.published || state.last_sessions != sessions || state.last_agents != agents {
        rosters.write(Roster {
            sessions: sessions.clone(),
            agents: agents.clone(),
        });
        if !state.published || state.last_agents != agents {
            agent_messages.write(Agents(agents.clone()));
        }
        state.last_sessions = sessions;
        state.last_agents = agents;
        state.published = true;
    }
}
