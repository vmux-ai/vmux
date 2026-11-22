use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, PostUpdate};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use vmux_ecs::{EntityTarget, Order, PageMetadata, UiState, UiStatePlugin, UiStateWrite};

use crate::event::{
    ManagerCategorySelected, ManagerDateFilterAdded, ManagerFilterAdded, ManagerFilterRemoved,
    ManagerModeChanged, ManagerQueryChanged, ManagerSelectionChanged, ManagerSelectionCleared,
    ManagerWithinFilterAdded, ProviderSessionTarget, SessionRepositories,
};
use crate::manager::{ManagerPresentation, SessionFilter, StageGroup, UpdatedFilter};
use crate::state::ManagerUiState;
use crate::{
    AgentFamily, ProviderKind, ProviderSessionId, Route, SessionId, SessionSummary,
    SessionSummaryProjection, StageSummary,
};

use super::catalog::SessionManagerView;

pub(super) struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::SessionHostPlugin>() {
            app.add_plugins(crate::SessionHostPlugin);
        }
        app.add_plugins(super::manager_operations::ManagerOperationsPlugin)
            .add_plugins(UiStatePlugin::<ManagerUiState>::default())
            .add_plugins(UiEventPlugin::<(
                ManagerQueryChanged,
                ManagerFilterAdded,
                ManagerFilterRemoved,
                ManagerCategorySelected,
                ManagerModeChanged,
                ManagerSelectionChanged,
                ManagerSelectionCleared,
                ManagerDateFilterAdded,
                ManagerWithinFilterAdded,
            )>::default())
            .add_observer(attach)
            .add_observer(retire)
            .add_observer(query)
            .add_observer(add_filter)
            .add_observer(remove_filter)
            .add_observer(category)
            .add_observer(mode)
            .add_observer(select)
            .add_observer(clear)
            .add_observer(date)
            .add_observer(within)
            .add_observer(reconcile)
            .add_observer(project)
            .add_systems(PostUpdate, sync.after(crate::model::CanonicalizeSet));
    }
}

#[derive(Component, Default)]
struct ManagerQuery(String);

#[derive(Component, Default)]
pub(super) struct ManagerFilters;

#[derive(Component)]
struct Filter;

#[derive(Component)]
struct CurrentFilter;

#[derive(Component)]
struct ArchivedFilter;

#[derive(Component)]
struct StageFilter(crate::StageId);

#[derive(Component)]
struct AgentFilter(String);

#[derive(Component)]
struct ProjectFilter(String);

#[derive(Component, Default)]
struct ManagerDisplayMode(bool);

#[derive(Component)]
struct Membership {
    target: Entity,
}

#[derive(Component)]
struct Selected;

#[derive(EntityEvent)]
pub(super) struct Project {
    pub(super) entity: Entity,
}

#[derive(EntityEvent)]
struct Publish {
    entity: Entity,
}

fn attach(
    event: On<Add, SessionManagerView>,
    pages: Query<&PageMetadata>,
    profile: vmux_ecs::profile::CurrentProfile,
    mut commands: Commands,
) {
    let entity = event.event_target();
    let filters = pages
        .get(entity)
        .map(|page| ManagerFilters::route(&page.url))
        .unwrap_or_default();
    let mut view = commands.entity(entity);
    view.insert((
        ManagerQuery::default(),
        ManagerFilters,
        ManagerDisplayMode::default(),
        crate::provider::NativeContext::capture(
            profile.entity(),
            profile.profile(),
            profile.paths(),
        ),
        UiState::<ManagerUiState>::default(),
    ));
    if let (Some(entity), Some(current)) = (profile.entity(), profile.profile()) {
        view.insert((
            EntityTarget::<vmux_ecs::profile::ActiveProfile>::new(entity),
            vmux_ecs::ProfileId(current.id().to_string()),
        ));
    }
    for filter in filters {
        commands.trigger(UiInput {
            webview: entity,
            payload: ManagerFilterAdded { filter },
        });
    }
    commands.trigger(Project { entity });
}

fn retire(
    event: On<Remove, SessionManagerView>,
    members: Query<(Entity, &ChildOf), Or<(With<Membership>, With<Filter>)>>,
    mut commands: Commands,
) {
    for (entity, parent) in &members {
        if parent.parent() == event.entity {
            commands.entity(entity).despawn();
        }
    }
    commands.entity(event.entity).try_remove::<(
        ManagerQuery,
        ManagerFilters,
        ManagerDisplayMode,
        UiState<ManagerUiState>,
    )>();
}

impl ManagerFilters {
    fn stage(filters: &[SessionFilter]) -> Option<&str> {
        filters.iter().find_map(|filter| match filter {
            SessionFilter::Stage(id) => Some(id.as_str()),
            SessionFilter::Current
            | SessionFilter::Archived
            | SessionFilter::Agent(_)
            | SessionFilter::Project(_)
            | SessionFilter::Updated(_) => None,
        })
    }

    fn archived_selected(filters: &[SessionFilter]) -> bool {
        filters
            .iter()
            .any(|filter| matches!(filter, SessionFilter::Archived))
    }

    fn matches(
        filter: &SessionFilter,
        session: &SessionSummary,
        repositories: &SessionRepositories,
    ) -> bool {
        match filter {
            SessionFilter::Current | SessionFilter::Stage(_) | SessionFilter::Archived => true,
            SessionFilter::Agent(agent) => Self::agent(session) == *agent,
            SessionFilter::Project(project) => {
                if project.starts_with('/')
                    && std::path::Path::new(&session.cwd).starts_with(project)
                {
                    return true;
                }
                repositories.get(&session.id).is_some_and(|repository| {
                    repository.project == *project || repository.project_url == *project
                })
            }
            SessionFilter::Updated(updated) => updated.matches(session.last_activated_at),
        }
    }

    fn agent(session: &SessionSummary) -> String {
        if session.agent_name.is_empty() {
            session.agent.clone()
        } else {
            session.agent_name.clone()
        }
    }

    pub(super) fn inbox(groups: &[StageGroup]) -> Vec<StageGroup> {
        groups
            .iter()
            .filter(|group| !group.stage.terminal)
            .cloned()
            .collect()
    }

    pub(crate) fn filtered(
        groups: &[StageGroup],
        filters: &[SessionFilter],
        repositories: &SessionRepositories,
    ) -> Vec<StageGroup> {
        let selected_stage = filters.iter().find_map(|filter| match filter {
            SessionFilter::Stage(id) => Some(id.as_str()),
            SessionFilter::Current
            | SessionFilter::Archived
            | SessionFilter::Agent(_)
            | SessionFilter::Project(_)
            | SessionFilter::Updated(_) => None,
        });
        let mut filtered = Vec::new();
        for group in groups {
            if filters.contains(&SessionFilter::Current)
                && (group.stage.terminal || group.stage.id == "stale")
            {
                continue;
            }
            if selected_stage.is_some_and(|stage| stage != group.stage.id.as_str()) {
                continue;
            }
            let mut group = group.clone();
            group.sessions.retain(|session| {
                filters
                    .iter()
                    .all(|filter| Self::matches(filter, session, repositories))
            });
            filtered.push(group);
        }
        filtered
    }

    pub(crate) fn from_catalog(
        stages: &[StageSummary],
        sessions: &[SessionSummary],
        search: &str,
    ) -> Vec<StageGroup> {
        let now = vmux_ecs::UnixMillis::now().0;
        let mut groups = stages
            .iter()
            .cloned()
            .map(|stage| StageGroup {
                stage,
                sessions: Vec::new(),
            })
            .collect::<Vec<_>>();
        for session in sessions {
            if !SessionSummaryProjection::matches(session, search) {
                continue;
            }
            if SessionSummaryProjection::is_stale(session, stages, now) {
                let stale_index = match groups.iter().position(|group| group.stage.id == "stale") {
                    Some(index) => index,
                    None => {
                        let index = groups
                            .iter()
                            .position(|group| !group.stage.terminal)
                            .map(|index| index + 1)
                            .unwrap_or(groups.len());
                        groups.insert(
                            index,
                            StageGroup {
                                stage: StageSummary {
                                    id: "stale".into(),
                                    name: "sessions-stale".into(),
                                    order: 0,
                                    terminal: false,
                                },
                                sessions: Vec::new(),
                            },
                        );
                        index
                    }
                };
                groups[stale_index].sessions.push(session.clone());
                continue;
            }
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group.stage.id == session.stage)
            {
                group.sessions.push(session.clone());
                continue;
            }
            groups.push(StageGroup {
                stage: StageSummary {
                    id: session.stage.clone(),
                    name: session.stage.clone(),
                    order: u32::MAX,
                    terminal: false,
                },
                sessions: vec![session.clone()],
            });
        }
        groups
    }

    pub(super) fn archived(sessions: &[SessionSummary], search: &str) -> Vec<StageGroup> {
        vec![StageGroup {
            stage: StageSummary {
                id: "archived".into(),
                name: "common-archived".into(),
                order: u32::MAX,
                terminal: true,
            },
            sessions: sessions
                .iter()
                .filter(|session| SessionSummaryProjection::matches(session, search))
                .cloned()
                .collect(),
        }]
    }

    fn agents(sessions: &[SessionSummary]) -> Vec<(String, usize)> {
        let mut agents = HashMap::<String, usize>::new();
        for session in sessions {
            let agent = if session.agent_name.is_empty() {
                session.agent.clone()
            } else {
                session.agent_name.clone()
            };
            if !agent.is_empty() {
                *agents.entry(agent).or_default() += 1;
            }
        }
        let mut agents = agents.into_iter().collect::<Vec<_>>();
        agents.sort_by_key(|left| left.0.to_lowercase());
        agents
    }

    fn projects(
        sessions: &[SessionSummary],
        repositories: &SessionRepositories,
    ) -> Vec<(String, usize)> {
        let mut projects = HashMap::<String, usize>::new();
        for session in sessions {
            if let Some(repository) = repositories.get(&session.id)
                && !repository.project.is_empty()
            {
                *projects.entry(repository.project).or_default() += 1;
            }
        }
        let mut projects = projects.into_iter().collect::<Vec<_>>();
        projects.sort_by_key(|left| left.0.to_lowercase());
        projects
    }

    fn route(route: &str) -> Vec<SessionFilter> {
        let mut filters = Vec::new();
        if let Some(project) = Route::manager_filter(route, "project") {
            filters.push(SessionFilter::Project(project));
        }
        let filter = match Route::manager_filter(route, "status").as_deref() {
            Some("current") => SessionFilter::Current,
            Some("stale") => SessionFilter::Stage("stale".into()),
            Some("archived") => SessionFilter::Archived,
            _ => return filters,
        };
        filters.push(filter);
        filters
    }

    fn project(
        &self,
        filters: &[SessionFilter],
        snapshot: crate::CatalogSnapshot,
        repositories: SessionRepositories,
        query: &ManagerQuery,
        grid: bool,
        leaving: &HashSet<String>,
    ) -> ManagerPresentation {
        let selected_archived = Self::archived_selected(filters);
        let filter_requested = query.0.starts_with('#');
        let filter_needle = query
            .0
            .strip_prefix('#')
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let search = if filter_requested {
            String::new()
        } else {
            query.0.trim().to_ascii_lowercase()
        };
        let mut active = Vec::new();
        let mut archived = Vec::new();
        for session in &snapshot.sessions {
            if session.archived_at > 0 {
                archived.push(session.clone());
                if !selected_archived && leaving.contains(&session.id) {
                    active.push(session.clone());
                }
            } else {
                active.push(session.clone());
                if selected_archived && leaving.contains(&session.id) {
                    archived.push(session.clone());
                }
            }
        }
        let category_groups = Self::from_catalog(&snapshot.stages, &active, &search);
        let mut groups = if selected_archived {
            Self::filtered(&Self::archived(&archived, &search), filters, &repositories)
        } else {
            let groups = Self::filtered(&category_groups, filters, &repositories);
            if Self::stage(filters).is_none() {
                Self::inbox(&groups)
            } else {
                groups
            }
        };
        if grid {
            groups.sort_by_key(|group| match group.stage.id.as_str() {
                "stale" => (0, 0),
                _ => (1, group.stage.order),
            });
        }
        let mut visible_ids = HashSet::new();
        for group in &groups {
            for session in &group.sessions {
                visible_ids.insert(session.id.clone());
            }
        }
        ManagerPresentation {
            query: query.0.clone(),
            filters: filters.to_vec(),
            grid,
            selected_stage: Self::stage(filters).map(str::to_string),
            selected_archived,
            archived_count: archived.len(),
            filter_requested,
            filter_needle,
            visible_sessions: !visible_ids.is_empty(),
            visible_ids,
            agent_options: Self::agents(&snapshot.sessions),
            project_options: Self::projects(&snapshot.sessions, &repositories),
            snapshot,
            repositories,
            category_groups,
            groups,
            ..Default::default()
        }
    }
}

fn query(
    event: On<UiInput<ManagerQueryChanged>>,
    mut queries: Query<&mut ManagerQuery, With<SessionManagerView>>,
    mut commands: Commands,
) {
    let Ok(mut query) = queries.get_mut(event.webview) else {
        return;
    };
    query.0.clone_from(&event.payload.query);
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn add_filter(
    event: On<UiInput<ManagerFilterAdded>>,
    views: Query<(), With<SessionManagerView>>,
    records: FilterReads,
    mut commands: Commands,
) {
    if !views.contains(event.webview) || matches!(event.payload.filter, SessionFilter::Updated(_)) {
        return;
    }
    let previous = records.matching(event.webview, &event.payload.filter);
    let order = previous
        .first()
        .map(|(_, order)| *order)
        .unwrap_or_else(|| records.next_order(event.webview));
    for (entity, _) in previous {
        commands.entity(entity).despawn();
    }
    let mut filter = commands.spawn((Filter, ChildOf(event.webview), Order(order)));
    match &event.payload.filter {
        SessionFilter::Current => {
            filter.insert(CurrentFilter);
        }
        SessionFilter::Archived => {
            filter.insert(ArchivedFilter);
        }
        SessionFilter::Stage(stage) => {
            filter.insert(StageFilter(crate::StageId(stage.clone())));
        }
        SessionFilter::Agent(agent) => {
            filter.insert(AgentFilter(agent.clone()));
        }
        SessionFilter::Project(project) => {
            filter.insert(ProjectFilter(project.clone()));
        }
        SessionFilter::Updated(_) => return,
    }
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn remove_filter(
    event: On<UiInput<ManagerFilterRemoved>>,
    records: FilterReads,
    mut commands: Commands,
) {
    let Some(entity) = Entity::try_from_bits(event.payload.id) else {
        return;
    };
    let Ok((_, parent, ..)) = records.records.get(entity) else {
        return;
    };
    if parent.parent() != event.webview {
        return;
    }
    commands.entity(entity).despawn();
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn category(
    event: On<UiInput<ManagerCategorySelected>>,
    mut queries: Query<&mut ManagerQuery, With<SessionManagerView>>,
    records: FilterReads,
    mut commands: Commands,
) {
    let Ok(mut query) = queries.get_mut(event.webview) else {
        return;
    };
    query.0.clear();
    for (entity, _) in records.matching(event.webview, &SessionFilter::Current) {
        commands.entity(entity).despawn();
    }
    if event.payload.archived {
        commands.spawn((Filter, ChildOf(event.webview), Order(0), ArchivedFilter));
    } else if let Some(stage) = &event.payload.stage {
        commands.spawn((
            Filter,
            ChildOf(event.webview),
            Order(0),
            StageFilter(crate::StageId(stage.clone())),
        ));
    }
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn mode(
    event: On<UiInput<ManagerModeChanged>>,
    mut modes: Query<&mut ManagerDisplayMode, With<SessionManagerView>>,
    mut commands: Commands,
) {
    let Ok(mut mode) = modes.get_mut(event.webview) else {
        return;
    };
    mode.0 = event.payload.grid;
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn select(
    event: On<UiInput<ManagerSelectionChanged>>,
    members: Query<(Entity, &ChildOf, &Membership), With<Membership>>,
    sources: crate::catalog::CatalogSources,
    mut commands: Commands,
) {
    for (entity, parent, member) in &members {
        if parent.parent() != event.webview
            || sources.id(member.target).as_deref() != Some(event.payload.id.as_str())
        {
            continue;
        }
        if event.payload.selected {
            commands.entity(entity).insert(Selected);
        } else {
            commands.entity(entity).remove::<Selected>();
        }
    }
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn clear(
    event: On<UiInput<ManagerSelectionCleared>>,
    members: Query<(Entity, &ChildOf), (With<Membership>, With<Selected>)>,
    mut commands: Commands,
) {
    for (entity, parent) in &members {
        if parent.parent() == event.webview {
            commands.entity(entity).remove::<Selected>();
        }
    }
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn date(
    event: On<UiInput<ManagerDateFilterAdded>>,
    views: Query<(), With<SessionManagerView>>,
    records: FilterReads,
    mut commands: Commands,
) {
    if !views.contains(event.webview) {
        return;
    }
    let Some(filter) = UpdatedFilter::from_date(event.payload.comparison, &event.payload.value)
    else {
        return;
    };
    let previous = records.matching(event.webview, &SessionFilter::Updated(filter.clone()));
    let order = previous
        .first()
        .map(|(_, order)| *order)
        .unwrap_or_else(|| records.next_order(event.webview));
    for (entity, _) in previous {
        commands.entity(entity).despawn();
    }
    commands.spawn((Filter, ChildOf(event.webview), Order(order), filter));
    commands.trigger(Project {
        entity: event.webview,
    });
}

fn within(
    event: On<UiInput<ManagerWithinFilterAdded>>,
    views: Query<(), With<SessionManagerView>>,
    records: FilterReads,
    mut commands: Commands,
) {
    if !views.contains(event.webview) {
        return;
    }
    let Some(filter) = UpdatedFilter::within(event.payload.days) else {
        return;
    };
    let previous = records.matching(event.webview, &SessionFilter::Updated(filter.clone()));
    let order = previous
        .first()
        .map(|(_, order)| *order)
        .unwrap_or_else(|| records.next_order(event.webview));
    for (entity, _) in previous {
        commands.entity(entity).despawn();
    }
    commands.spawn((Filter, ChildOf(event.webview), Order(order), filter));
    commands.trigger(Project {
        entity: event.webview,
    });
}

#[derive(bevy_ecs::system::SystemParam)]
struct FilterReads<'w, 's> {
    records: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Order,
            Has<CurrentFilter>,
            Has<ArchivedFilter>,
            Option<&'static StageFilter>,
            Option<&'static AgentFilter>,
            Option<&'static ProjectFilter>,
            Option<&'static UpdatedFilter>,
        ),
        With<Filter>,
    >,
}

impl FilterReads<'_, '_> {
    fn values(&self, owner: Entity) -> Vec<(Entity, SessionFilter)> {
        let mut values = Vec::new();
        for (entity, parent, order, current, archived, stage, agent, project, updated) in
            &self.records
        {
            if parent.parent() != owner {
                continue;
            }
            let value = if current {
                SessionFilter::Current
            } else if archived {
                SessionFilter::Archived
            } else if let Some(stage) = stage {
                SessionFilter::Stage(stage.0.0.clone())
            } else if let Some(agent) = agent {
                SessionFilter::Agent(agent.0.clone())
            } else if let Some(project) = project {
                SessionFilter::Project(project.0.clone())
            } else if let Some(updated) = updated {
                SessionFilter::Updated(updated.clone())
            } else {
                continue;
            };
            values.push((order.0, entity, value));
        }
        values.sort_by_key(|(order, entity, _)| (*order, *entity));
        values
            .into_iter()
            .map(|(_, entity, value)| (entity, value))
            .collect()
    }

    fn matching(&self, owner: Entity, requested: &SessionFilter) -> Vec<(Entity, u32)> {
        let mut matched = Vec::new();
        for (entity, parent, order, current, archived, stage, agent, project, updated) in
            &self.records
        {
            if parent.parent() != owner {
                continue;
            }
            let same = match requested {
                SessionFilter::Current | SessionFilter::Archived | SessionFilter::Stage(_) => {
                    current || archived || stage.is_some()
                }
                SessionFilter::Agent(_) => agent.is_some(),
                SessionFilter::Project(_) => project.is_some(),
                SessionFilter::Updated(_) => updated.is_some(),
            };
            if same {
                matched.push((entity, order.0));
            }
        }
        matched.sort_by_key(|(entity, order)| (*order, *entity));
        matched
    }

    fn next_order(&self, owner: Entity) -> u32 {
        let mut order = 0;
        for (_, parent, current, ..) in &self.records {
            if parent.parent() == owner {
                order = order.max(current.0.saturating_add(1));
            }
        }
        order
    }
}

impl Membership {
    fn identity(
        kind: Option<&ProviderKind>,
        provider: Option<&ProviderSessionId>,
        local: Option<&SessionId>,
    ) -> String {
        if let (Some(kind), Some(provider)) = (kind, provider) {
            return format!(
                "{:?}:{}",
                AgentFamily::of(kind.0.trim_start_matches("acp:")),
                provider.0
            );
        }
        format!("local:{}", local.map_or("", |id| id.0.as_str()))
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct Members<'w, 's> {
    records: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Order,
            &'static Membership,
            Option<&'static ProviderKind>,
            Option<&'static ProviderSessionId>,
            Option<&'static SessionId>,
            Has<Selected>,
        ),
        With<Membership>,
    >,
}

impl Members<'_, '_> {
    fn retained(&self, owner: Entity) -> HashSet<Entity> {
        let mut retained = HashSet::new();
        for (_, parent, _, member, ..) in &self.records {
            if parent.parent() == owner {
                retained.insert(member.target);
            }
        }
        retained
    }

    fn positions(
        &self,
        owner: Entity,
        sources: &crate::catalog::CatalogSources,
    ) -> (HashMap<String, u32>, HashSet<String>) {
        let mut positions = HashMap::new();
        let mut selected = HashSet::new();
        for (_, parent, order, member, _, _, _, is_selected) in &self.records {
            if parent.parent() != owner {
                continue;
            }
            if let Some(id) = sources.id(member.target) {
                positions.insert(id.clone(), order.0);
                if is_selected {
                    selected.insert(id);
                }
            }
        }
        (positions, selected)
    }
}

fn reconcile(
    event: On<Project>,
    views: Query<(), With<SessionManagerView>>,
    sources: crate::catalog::CatalogSources,
    members: Members,
    mut commands: Commands,
) {
    if !views.contains(event.entity) {
        return;
    }
    let retained = members.retained(event.entity);
    let snapshot = sources.view_snapshot(&retained);
    let mut existing = HashMap::new();
    let mut order = 0;
    for (entity, parent, position, target, kind, provider, local, selected) in &members.records {
        if parent.parent() != event.entity {
            continue;
        }
        order = order.max(position.0.saturating_add(1));
        existing.insert(
            Membership::identity(kind, provider, local),
            (entity, position.0, target.target, selected),
        );
    }
    for session in &snapshot.sessions {
        let kind = (!session.provider_kind.is_empty())
            .then(|| ProviderKind(session.provider_kind.clone()));
        let provider = (!session.provider_sid.is_empty())
            .then(|| ProviderSessionId(session.provider_sid.clone()));
        let local = SessionId(session.id.clone());
        let identity = Membership::identity(kind.as_ref(), provider.as_ref(), Some(&local));
        let Some(target) = sources.display_target(
            &session.id,
            existing.get(&identity).map(|(_, _, target, _)| *target),
        ) else {
            continue;
        };
        let (entity, _position, previous, _was_selected) = match existing.remove(&identity) {
            Some(member) => member,
            None => {
                let entity = commands
                    .spawn((ChildOf(event.entity), Order(order), Membership { target }))
                    .id();
                if let (Some(kind), Some(provider)) = (kind, provider) {
                    commands.entity(entity).insert((kind, provider));
                } else {
                    commands.entity(entity).insert(local);
                }
                order = order.saturating_add(1);
                (entity, order - 1, target, false)
            }
        };
        if previous != target {
            commands.entity(entity).insert(Membership { target });
        }
    }
    for (entity, ..) in existing.into_values() {
        commands.entity(entity).despawn();
    }

    commands.trigger(Publish {
        entity: event.entity,
    });
}

#[derive(bevy_ecs::system::SystemParam)]
struct ProjectionReads<'w, 's> {
    repositories: Res<'w, SessionRepositories>,
    filters: FilterReads<'w, 's>,
    members: Members<'w, 's>,
    tombstones: Query<
        'w,
        's,
        (&'static ProviderKind, &'static ProviderSessionId),
        With<crate::ProviderSessionTombstone>,
    >,
}

impl ProjectionReads<'_, '_> {
    fn restorable(&self) -> Vec<ProviderSessionTarget> {
        let mut targets = Vec::new();
        for (kind, id) in &self.tombstones {
            targets.push(ProviderSessionTarget {
                kind: kind.0.clone(),
                sid: id.0.clone(),
            });
        }
        targets.sort_by(|a, b| (&a.kind, &a.sid).cmp(&(&b.kind, &b.sid)));
        targets
    }
}

fn project(
    event: On<Publish>,
    views: Query<
        (
            &ManagerQuery,
            &ManagerFilters,
            &ManagerDisplayMode,
            &crate::provider::NativeContext,
        ),
        With<SessionManagerView>,
    >,
    profile: vmux_ecs::profile::CurrentProfile,
    sources: crate::catalog::CatalogSources,
    effects: super::manager_operations::MutationViews,
    projection: ProjectionReads,
    mut commands: Commands,
) {
    let Ok((query, filters, mode, context)) = views.get(event.entity) else {
        return;
    };
    if !context.matches(profile.entity(), profile.profile(), profile.paths()) {
        commands.trigger(UiStateWrite::<ManagerUiState>::from_event(
            event.entity,
            &ManagerUiState {
                query: query.0.clone(),
                grid: mode.0,
                ..Default::default()
            },
        ));
        return;
    }
    let retained = projection.members.retained(event.entity);
    let mut snapshot = sources.view_snapshot(&retained);
    let mut leaving = HashSet::new();
    for row in &mut snapshot.sessions {
        let fallback = retained
            .iter()
            .find(|target| sources.id(**target).as_deref() == Some(row.id.as_str()))
            .copied();
        let Some(target) = sources.display_target(&row.id, fallback) else {
            continue;
        };
        if let Some(stage) = effects.stage(target) {
            row.stage.clone_from(&stage.0);
        }
        if let Some(archive) = effects.archive(target) {
            row.archived_at = if archive.archived {
                archive.accepted_at.max(1)
            } else {
                0
            };
        }
        if effects.leaving(target, event.entity) {
            leaving.insert(row.id.clone());
        }
    }
    let (positions, mut selected) = projection.members.positions(event.entity, &sources);
    snapshot
        .sessions
        .sort_by_key(|row| positions.get(&row.id).copied().unwrap_or(u32::MAX));
    let values = projection.filters.values(event.entity);
    let filter_values = values
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let mut presentation = filters.project(
        &filter_values,
        snapshot,
        projection.repositories.clone(),
        query,
        mode.0,
        &leaving,
    );
    presentation.loading = sources.loading() && presentation.snapshot.sessions.is_empty();
    for (entity, parent, _, target, ..) in &projection.members.records {
        if parent.parent() != event.entity {
            continue;
        }
        let visible = presentation.snapshot.sessions.iter().any(|row| {
            sources.id(target.target).as_deref() == Some(row.id.as_str())
                && presentation.visible_ids.contains(&row.id)
        });
        if !visible {
            commands.entity(entity).try_remove::<Selected>();
        }
    }
    selected.retain(|id| presentation.visible_ids.contains(id));
    presentation.selected_ids = selected;
    presentation.filter_ids = values.iter().map(|(entity, _)| entity.to_bits()).collect();
    presentation.leaving_ids = leaving;
    presentation.restorable = projection.restorable();
    commands.trigger(UiStateWrite::<ManagerUiState>::from_event(
        event.entity,
        &presentation,
    ));
}

fn sync(views: Query<Entity, With<SessionManagerView>>, mut commands: Commands) {
    for entity in &views {
        commands.trigger(Project { entity });
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager_fixture::ManagerFixture;
    use super::*;
    use crate::event::SessionRepository;
    use crate::manager::UpdatedComparison;

    #[test]
    fn project_activity_filters_include_nested_work_and_exclude_stale_terminal_and_sibling_projects()
     {
        let now = vmux_ecs::UnixMillis::now().0;
        let stages = vec![
            StageSummary {
                id: "active".to_string(),
                ..Default::default()
            },
            StageSummary {
                id: "queued".to_string(),
                ..Default::default()
            },
            StageSummary {
                id: "done".to_string(),
                terminal: true,
                ..Default::default()
            },
        ];
        let mut sessions = Vec::new();
        for (id, stage, cwd, activated) in [
            ("active", "active", "/work/repo/src", now),
            ("queued", "queued", "/work/repo/.worktrees/task", now),
            (
                "stale",
                "active",
                "/work/repo",
                now - 4 * 24 * 60 * 60 * 1_000,
            ),
            ("done", "done", "/work/repo", now),
            ("sibling", "active", "/work/repo-other", now),
        ] {
            sessions.push(SessionSummary {
                id: id.to_string(),
                stage: stage.to_string(),
                cwd: cwd.to_string(),
                runtime: "idle".to_string(),
                last_activated_at: activated,
                ..Default::default()
            });
        }
        let groups = ManagerFilters::from_catalog(&stages, &sessions, "");
        let filters = [
            SessionFilter::Project("/work/repo".to_string()),
            SessionFilter::Current,
        ];
        let filtered = ManagerFilters::filtered(&groups, &filters, &SessionRepositories::default());
        let ids = filtered
            .iter()
            .flat_map(|group| group.sessions.iter().map(|session| session.id.as_str()))
            .collect::<HashSet<_>>();
        assert_eq!(ids, HashSet::from(["active", "queued"]));
    }

    #[test]
    fn session_without_stage_definition_remains_visible() {
        let session = SessionSummary {
            id: "session-1".into(),
            name: "Task".into(),
            stage: "active".into(),
            ..Default::default()
        };

        let groups = ManagerFilters::from_catalog(&[], std::slice::from_ref(&session), "");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].stage.id, "active");
        assert_eq!(groups[0].sessions, vec![session]);
    }

    #[test]
    fn stale_session_moves_out_of_its_persisted_stage() {
        let stages = vec![StageSummary {
            id: "in_progress".into(),
            name: "session-stage-active".into(),
            order: 10,
            terminal: false,
        }];
        let session = SessionSummary {
            id: "session-1".into(),
            stage: "in_progress".into(),
            runtime: "idle".into(),
            last_activated_at: vmux_ecs::UnixMillis::now().0
                - (SessionSummaryProjection::STALE_AFTER as i64 * 1_000),
            ..Default::default()
        };

        let groups = ManagerFilters::from_catalog(&stages, std::slice::from_ref(&session), "");
        let active = groups
            .iter()
            .find(|group| group.stage.id == "in_progress")
            .unwrap();
        let stale = groups
            .iter()
            .find(|group| group.stage.id == "stale")
            .unwrap();

        assert!(active.sessions.is_empty());
        assert_eq!(stale.sessions, vec![session]);
    }

    #[test]
    fn stage_and_updated_filters_compose() {
        let recent = SessionSummary {
            id: "recent".into(),
            stage: "in_progress".into(),
            last_activated_at: 200,
            ..Default::default()
        };
        let old = SessionSummary {
            id: "old".into(),
            stage: "in_progress".into(),
            last_activated_at: 50,
            ..Default::default()
        };
        let done = SessionSummary {
            id: "done".into(),
            stage: "done".into(),
            last_activated_at: 200,
            ..Default::default()
        };
        let groups = vec![
            StageGroup {
                stage: StageSummary {
                    id: "in_progress".into(),
                    ..Default::default()
                },
                sessions: vec![recent.clone(), old],
            },
            StageGroup {
                stage: StageSummary {
                    id: "done".into(),
                    ..Default::default()
                },
                sessions: vec![done],
            },
        ];
        let filters = vec![
            SessionFilter::Stage("in_progress".into()),
            SessionFilter::Updated(UpdatedFilter {
                comparison: UpdatedComparison::OnOrAfter,
                threshold: 100,
                value_label: String::new(),
            }),
        ];

        let filtered = ManagerFilters::filtered(&groups, &filters, &SessionRepositories::default());

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].sessions, vec![recent]);
    }

    #[test]
    fn agent_and_project_filters_compose() {
        let codex = SessionSummary {
            id: "codex".into(),
            stage: "in_progress".into(),
            agent_name: "Codex".into(),
            ..Default::default()
        };
        let claude = SessionSummary {
            id: "claude".into(),
            stage: "in_progress".into(),
            agent_name: "Claude".into(),
            ..Default::default()
        };
        let groups = vec![StageGroup {
            stage: StageSummary {
                id: "in_progress".into(),
                ..Default::default()
            },
            sessions: vec![codex.clone(), claude],
        }];
        let filters = vec![
            SessionFilter::Agent("Codex".into()),
            SessionFilter::Project("vmux".into()),
        ];
        let repositories = SessionRepositories {
            ready: true,
            sessions: vec![SessionRepository {
                id: "codex".into(),
                project: "vmux".into(),
                ..Default::default()
            }],
        };

        let filtered = ManagerFilters::filtered(&groups, &filters, &repositories);

        assert_eq!(filtered[0].sessions, vec![codex]);
    }

    #[test]
    fn inbox_excludes_terminal_stages() {
        let active = StageGroup {
            stage: StageSummary {
                id: "in_progress".into(),
                terminal: false,
                ..Default::default()
            },
            sessions: vec![SessionSummary {
                id: "active".into(),
                ..Default::default()
            }],
        };
        let completed = StageGroup {
            stage: StageSummary {
                id: "done".into(),
                terminal: true,
                ..Default::default()
            },
            sessions: vec![SessionSummary {
                id: "completed".into(),
                ..Default::default()
            }],
        };

        let inbox = ManagerFilters::inbox(&[active.clone(), completed]);

        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].stage.id, active.stage.id);
        assert_eq!(inbox[0].sessions, active.sessions);
    }

    #[test]
    fn provider_probe_keeps_live_native_rows_without_replaying_catalog_snapshots() {
        let mut fixture = ManagerFixture::new();
        let now = vmux_ecs::UnixMillis::now().0;
        let source = fixture
            .app
            .world_mut()
            .spawn((
                crate::provider::Source,
                crate::provider::Origin::Native,
                crate::provider::Capability::Supported,
                crate::provider::Generation(0),
                crate::AgentId("codex".into()),
                Name::new("Codex"),
            ))
            .id();
        let entry = fixture
            .app
            .world_mut()
            .spawn((
                crate::provider::Entry,
                ChildOf(source),
                crate::ProviderKind("native:codex".into()),
                crate::ProviderSessionId("p".into()),
                crate::AgentId("codex".into()),
                Name::new("Original"),
                vmux_ecs::Description::default(),
                vmux_ecs::Cwd(fixture.directory.path().to_path_buf()),
                vmux_ecs::CreatedAt(1),
                crate::SourceUpdatedAt(now),
                crate::provider::ConversationTitle::default(),
                crate::provider::LastPrompt::default(),
            ))
            .id();
        fixture.update();
        fixture.app.world_mut().spawn((
            crate::provider::Source,
            crate::provider::Origin::Acp,
            crate::provider::Capability::Unprobed,
            crate::provider::Generation(0),
            crate::AgentId("codex".into()),
            Name::new("Codex ACP"),
        ));
        fixture
            .app
            .world_mut()
            .entity_mut(entry)
            .insert(Name::new("Fresh native title"));
        let state = fixture.update();
        assert!(!state.loading);
        assert_eq!(
            state
                .snapshot
                .sessions
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Fresh native title"]
        );
        fixture.input(ManagerSelectionChanged {
            id: "native:codex:p".into(),
            selected: true,
        });
        assert_eq!(
            fixture.state().selected_ids,
            HashSet::from(["native:codex:p".to_string()])
        );
        fixture.app.world_mut().entity_mut(source).despawn();
        assert!(fixture.update().snapshot.sessions.is_empty());
    }

    #[test]
    fn filter_and_selection_project_in_the_input_frame_without_page_readiness() {
        let mut fixture = ManagerFixture::new();
        fixture.session("first", "Alpha task", vmux_ecs::UnixMillis::now().0);
        fixture.session("second", "Beta task", vmux_ecs::UnixMillis::now().0);
        fixture.update();
        fixture.input(ManagerSelectionChanged {
            id: "first".into(),
            selected: true,
        });
        assert_eq!(
            fixture.state().selected_ids,
            HashSet::from(["first".to_string()])
        );
        fixture.input(ManagerQueryChanged {
            query: "Beta".into(),
        });
        let state = fixture.state();
        assert_eq!(state.visible_ids, HashSet::from(["second".to_string()]));
        assert!(state.selected_ids.is_empty());
    }

    #[test]
    fn retained_catalog_output_cannot_change_manager_membership() {
        let mut fixture = ManagerFixture::new();
        fixture.session("actual", "Canonical task", vmux_ecs::UnixMillis::now().0);
        fixture.update();
        fixture
            .app
            .world_mut()
            .resource_mut::<crate::CatalogSnapshot>()
            .sessions = vec![crate::SessionSummary {
            id: "foreign".into(),
            name: "Injected projection".into(),
            ..Default::default()
        }];
        fixture.input(ManagerQueryChanged {
            query: String::new(),
        });
        assert_eq!(
            fixture.state().visible_ids,
            HashSet::from(["actual".to_string()])
        );
    }

    #[test]
    fn provider_promotion_preserves_order_and_logical_selection() {
        let mut fixture = ManagerFixture::new();
        let now = vmux_ecs::UnixMillis::now().0;
        fixture.session("other", "Other", now);
        let source = fixture
            .app
            .world_mut()
            .spawn((
                crate::provider::Source,
                crate::provider::Origin::Native,
                crate::provider::Capability::Supported,
                crate::provider::Generation(0),
                crate::AgentId("codex".into()),
                Name::new("Codex"),
            ))
            .id();
        fixture.app.world_mut().spawn((
            crate::provider::Entry,
            ChildOf(source),
            crate::ProviderKind("native:codex".into()),
            crate::ProviderSessionId("p".into()),
            crate::AgentId("codex".into()),
            Name::new("Provider task"),
            vmux_ecs::Description::default(),
            vmux_ecs::Cwd(fixture.directory.path().to_path_buf()),
            vmux_ecs::CreatedAt(1),
            crate::SourceUpdatedAt(now - 1),
            crate::provider::ConversationTitle::default(),
            crate::provider::LastPrompt::default(),
        ));
        fixture.update();
        fixture.input(ManagerSelectionChanged {
            id: "native:codex:p".into(),
            selected: true,
        });
        let managed = fixture.session("managed", "Imported task", now + 1);
        fixture.app.world_mut().entity_mut(managed).insert((
            crate::ProviderKind("acp:codex".into()),
            crate::ProviderSessionId("p".into()),
        ));
        let state = fixture.update();
        assert_eq!(
            state
                .snapshot
                .sessions
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["other", "managed"]
        );
        assert_eq!(state.selected_ids, HashSet::from(["managed".to_string()]));
    }
}
