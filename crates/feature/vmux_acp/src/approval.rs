use bevy_app::{App, Plugin as BevyPlugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::SystemParam;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use vmux_ecs::{
    Active, BaseRevision, Cwd, EntityTarget, Operation, OperationError, OperationState, Revision,
};
use vmux_session::{AgentId, Session};

pub(crate) struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.world_mut().spawn(File::bundle(None));
        app.add_message::<StoreRequested>()
            .add_message::<Remember>()
            .add_message::<Scoped>()
            .add_message::<Loaded>()
            .add_message::<Saved>()
            .add_systems(Reconcile, (initialize, remember, resolve).chain())
            .configure_sets(Update, (Prepare, Resolve).chain())
            .add_systems(
                Update,
                (initialize, remember, prepare).chain().in_set(Prepare),
            )
            .add_systems(Update, (resolve, settle).chain().in_set(Resolve))
            .add_systems(
                Update,
                counts
                    .after(Resolve)
                    .in_set(vmux_session::ApprovalProjectionSet),
            )
            .add_observer(remove)
            .init_schedule(Reconcile);
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Reconcile;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Prepare;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Resolve;

#[derive(Component)]
pub(crate) struct File {
    pub(crate) path: Option<PathBuf>,
    pub(crate) loaded: bool,
    pub(crate) read: Option<Entity>,
    pub(crate) write: Option<Entity>,
    pub(crate) read_failed: bool,
    pub(crate) write_failed: bool,
    pub(crate) fingerprint: Option<u64>,
}

impl File {
    fn bundle(path: Option<PathBuf>) -> impl Bundle {
        (
            Name::new("Agent approvals"),
            Active,
            Revision(0),
            BaseRevision(0),
            Self {
                loaded: path.is_none(),
                path,
                read: None,
                write: None,
                read_failed: false,
                write_failed: false,
                fingerprint: None,
            },
        )
    }
}

#[derive(Component)]
pub(crate) struct Agent;

#[derive(Component)]
pub(crate) struct Scope {
    session: Option<Entity>,
}

impl Scope {
    pub(crate) fn key(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[derive(Component)]
pub(crate) struct Grant;

impl Grant {
    pub(crate) fn key(tool: &str) -> String {
        let mut key = String::new();
        for part in tool.trim().to_ascii_lowercase().split(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '-' | '.' | ':' | '_')
        }) {
            if part.is_empty() {
                continue;
            }
            if !key.is_empty() {
                key.push('_');
            }
            key.push_str(part);
        }
        key
    }
}

#[derive(Component)]
pub(crate) struct Explicit;

#[derive(Component)]
pub(crate) struct Binding {
    scope: Entity,
    file: Entity,
    agent: String,
    cwd: PathBuf,
}

#[derive(Component)]
struct Pending(Entity);

#[derive(Component, Clone, Copy)]
pub(crate) struct ScopeRequest {
    pub(crate) file: Entity,
    pub(crate) session: Entity,
    pub(crate) consent: bool,
}

#[derive(Message)]
pub(crate) struct StoreRequested(pub PathBuf);

#[derive(Message)]
pub(crate) struct Remember {
    pub(crate) file: Entity,
    pub(crate) session: Entity,
    pub(crate) agent: AgentId,
    pub(crate) cwd: Cwd,
    pub(crate) tool: String,
}

#[derive(Message)]
pub(crate) struct Scoped {
    pub(crate) operation: Entity,
    pub(crate) repository: Option<PathBuf>,
    pub(crate) directory: Option<PathBuf>,
}

#[derive(Message)]
pub(crate) struct Loaded {
    pub(crate) file: Entity,
    pub(crate) operation: Entity,
    pub(crate) fingerprint: Option<u64>,
    pub(crate) result: Result<Vec<(String, String, String)>, String>,
}

#[derive(Message)]
pub(crate) struct Saved {
    pub(crate) file: Entity,
    pub(crate) operation: Entity,
    pub(crate) revision: u64,
    pub(crate) fingerprint: Option<u64>,
    pub(crate) result: Result<(), String>,
}

#[derive(SystemParam)]
pub(crate) struct Grants<'w, 's> {
    files: Query<'w, 's, (Entity, &'static File, Has<Active>)>,
    records: Records<'w, 's>,
    sessions:
        Query<'w, 's, (&'static AgentId, &'static Cwd, Option<&'static Binding>), With<Session>>,
    requests: Query<
        'w,
        's,
        (
            &'static ScopeRequest,
            &'static AgentId,
            &'static Cwd,
            &'static Name,
            &'static OperationState,
        ),
    >,
}

impl Grants<'_, '_> {
    pub(crate) fn active(&self) -> Option<Entity> {
        for (entity, _, active) in &self.files {
            if active {
                return Some(entity);
            }
        }
        None
    }

    pub(crate) fn allows(&self, session: Entity, tool: &str) -> bool {
        let key = Grant::key(tool);
        !key.is_empty() && self.keys(session).contains(key.as_str())
    }

    pub(crate) fn count(&self, session: Entity) -> u32 {
        u32::try_from(self.keys(session).len()).unwrap_or(u32::MAX)
    }

    fn keys(&self, session: Entity) -> BTreeSet<&str> {
        let mut keys = BTreeSet::new();
        let Ok((agent, cwd, binding)) = self.sessions.get(session) else {
            return keys;
        };
        if agent.0.trim().is_empty() {
            return keys;
        }
        let Some(file) = self.active() else {
            return keys;
        };
        for (request, actor, directory, name, state) in &self.requests {
            if request.consent
                && request.file == file
                && request.session == session
                && actor == agent
                && directory == cwd
                && !matches!(state, OperationState::Failed | OperationState::Canceled)
            {
                keys.insert(name.as_str());
            }
        }
        let Some(binding) = binding else {
            return keys;
        };
        if binding.file != file || binding.agent != agent.0 || binding.cwd != cwd.0 {
            return keys;
        }
        let Ok((parent, scope, _)) = self.records.scopes.get(binding.scope) else {
            return keys;
        };
        if scope.session.is_some_and(|owner| owner != session) {
            return keys;
        }
        let Ok((root, actor)) = self.records.agents.get(parent.parent()) else {
            return keys;
        };
        if root.parent() != file || actor.0 != agent.0.trim().to_ascii_lowercase() {
            return keys;
        }
        let Ok((_, store, _)) = self.files.get(file) else {
            return keys;
        };
        for (parent, name, explicit) in &self.records.rules {
            if parent.parent() == binding.scope
                && (store.loaded || explicit || scope.session.is_some())
            {
                keys.insert(name.as_str());
            }
        }
        keys
    }
}

#[derive(SystemParam)]
pub(crate) struct Records<'w, 's> {
    agents: Query<'w, 's, (&'static ChildOf, &'static AgentId), With<Agent>>,
    scopes: Query<'w, 's, (&'static ChildOf, &'static Scope, &'static Name)>,
    rules: Query<'w, 's, (&'static ChildOf, &'static Name, Has<Explicit>), With<Grant>>,
}

impl Records<'_, '_> {
    pub(crate) fn records(&self, file: Entity) -> Vec<(&str, &str, &str)> {
        let mut records = Vec::new();
        for (parent, name, _) in &self.rules {
            let Ok((parent, scope, directory)) = self.scopes.get(parent.parent()) else {
                continue;
            };
            if scope.session.is_some() {
                continue;
            }
            let Ok((parent, agent)) = self.agents.get(parent.parent()) else {
                continue;
            };
            if parent.parent() == file {
                records.push((agent.0.as_str(), directory.as_str(), name.as_str()));
            }
        }
        records
    }
}

fn initialize(
    mut requests: MessageReader<StoreRequested>,
    mut files: Query<(Entity, &File)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let mut selected = None;
        for (entity, file) in &mut files {
            commands.entity(entity).remove::<Active>();
            if file.path.as_ref() == Some(&request.0) {
                selected = Some(entity);
            }
        }
        if let Some(file) = selected {
            commands.entity(file).insert(Active);
        } else {
            commands.spawn(File::bundle(Some(request.0.clone())));
        }
    }
}

fn remember(
    mut requests: MessageReader<Remember>,
    mut files: Query<&mut File>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let key = Grant::key(&request.tool);
        if key.is_empty() || request.agent.0.trim().is_empty() {
            continue;
        }
        let Ok(mut file) = files.get_mut(request.file) else {
            continue;
        };
        file.read_failed = false;
        file.write_failed = false;
        commands.spawn((
            Operation::bundle("approval.remember").unwrap().running(),
            ScopeRequest {
                file: request.file,
                session: request.session,
                consent: true,
            },
            request.agent.clone(),
            request.cwd.clone(),
            Name::new(key),
            ChildOf(request.file),
        ));
    }
}

fn prepare(
    grants: Grants,
    sessions: Query<(Entity, &AgentId, &Cwd, Option<&Binding>, Option<&Pending>), With<Session>>,
    requests: Query<(&ScopeRequest, &AgentId, &Cwd)>,
    mut commands: Commands,
) {
    let Some(file) = grants.active() else {
        return;
    };
    for (session, agent, cwd, binding, pending) in &sessions {
        if binding.is_some_and(|binding| {
            binding.file == file && binding.agent == agent.0 && binding.cwd == cwd.0
        }) {
            continue;
        }
        commands.entity(session).remove::<Binding>();
        if let Some(pending) = pending {
            if requests
                .get(pending.0)
                .is_ok_and(|(request, actor, directory)| {
                    request.file == file && actor == agent && directory == cwd
                })
            {
                continue;
            }
            commands.entity(pending.0).try_despawn();
        }
        let operation = commands
            .spawn((
                Operation::bundle("approval.scope").unwrap().running(),
                ScopeRequest {
                    file,
                    session,
                    consent: false,
                },
                agent.clone(),
                cwd.clone(),
                Name::new("Approval scope"),
                ChildOf(session),
            ))
            .id();
        commands.entity(session).insert(Pending(operation));
    }
}

fn resolve(world: &mut World) {
    let mut incoming = Vec::new();
    let loaded: Vec<_> = world.resource_mut::<Messages<Loaded>>().drain().collect();
    for result in loaded {
        if world
            .get::<EntityTarget<File>>(result.operation)
            .is_none_or(|target| target.entity() != result.file)
        {
            continue;
        }
        let Some(mut file) = world.get_mut::<File>(result.file) else {
            continue;
        };
        if file.read != Some(result.operation) {
            continue;
        }
        file.read = None;
        match result.result {
            Ok(records) => {
                file.loaded = true;
                file.read_failed = false;
                file.fingerprint = result.fingerprint;
                world.entity_mut(result.file).remove::<OperationError>();
                world
                    .entity_mut(result.operation)
                    .insert(OperationState::Succeeded);
                let mut stale = Vec::new();
                for (entity, parent, explicit) in world
                    .query_filtered::<(Entity, &ChildOf, Has<Explicit>), With<Grant>>()
                    .iter(world)
                {
                    if explicit {
                        continue;
                    }
                    let Some(scope) = world.get::<ChildOf>(parent.parent()) else {
                        continue;
                    };
                    let Some(agent) = world.get::<ChildOf>(scope.parent()) else {
                        continue;
                    };
                    if agent.parent() == result.file {
                        stale.push(entity);
                    }
                }
                for entity in stale {
                    world.despawn(entity);
                }
                for (agent, directory, tool) in records {
                    if agent.is_empty()
                        || agent != agent.trim().to_ascii_lowercase()
                        || tool.is_empty()
                        || tool != Grant::key(&tool)
                    {
                        continue;
                    }
                    incoming.push((result.file, agent, directory, None, Some(tool), None, false));
                }
            }
            Err(error) => {
                file.loaded = false;
                file.read_failed = true;
                world.entity_mut(result.operation).insert((
                    OperationState::Failed,
                    OperationError::new(error.clone()).unwrap(),
                ));
                world
                    .entity_mut(result.file)
                    .insert(OperationError::new(error).unwrap());
            }
        }
    }
    let scoped: Vec<_> = world.resource_mut::<Messages<Scoped>>().drain().collect();
    for result in scoped {
        let Some(request) = world.get::<ScopeRequest>(result.operation).copied() else {
            continue;
        };
        let owner = if request.consent {
            request.file
        } else {
            request.session
        };
        if world
            .get::<ChildOf>(result.operation)
            .is_none_or(|parent| parent.parent() != owner)
        {
            continue;
        }
        let Some(agent) = world.get::<AgentId>(result.operation).cloned() else {
            continue;
        };
        let Some(cwd) = world.get::<Cwd>(result.operation).cloned() else {
            continue;
        };
        if world.get::<File>(request.file).is_none()
            || matches!(
                world.get::<OperationState>(result.operation),
                Some(OperationState::Canceled | OperationState::Failed)
            )
        {
            continue;
        }
        let mut binding = None;
        if world.get::<AgentId>(request.session) == Some(&agent)
            && world.get::<Cwd>(request.session) == Some(&cwd)
            && world.get::<Session>(request.session).is_some()
            && world.get::<Active>(request.file).is_some()
        {
            binding = Some((request.session, agent.0.clone(), cwd.0.clone()));
        }
        if !request.consent && binding.is_none() {
            world
                .entity_mut(result.operation)
                .remove::<ScopeRequest>()
                .insert(OperationState::Canceled);
            continue;
        }
        let path = result.repository.or(result.directory);
        let transient = if path.is_none() {
            Some(request.session)
        } else {
            None
        };
        if transient.is_some() && world.get::<Session>(request.session).is_none() {
            world
                .entity_mut(result.operation)
                .remove::<ScopeRequest>()
                .insert((
                    OperationState::Failed,
                    OperationError::new("approval scope is unavailable").unwrap(),
                ));
            continue;
        }
        let key = if let Some(path) = path {
            Scope::key(&path)
        } else {
            Scope::key(&cwd.0)
        };
        let tool = if request.consent {
            world
                .get::<Name>(result.operation)
                .map(|name| name.as_str().to_string())
        } else {
            None
        };
        incoming.push((
            request.file,
            agent.0.trim().to_ascii_lowercase(),
            key,
            transient,
            tool,
            binding,
            request.consent,
        ));
        world
            .entity_mut(result.operation)
            .remove::<ScopeRequest>()
            .insert(OperationState::Succeeded);
        if world
            .get::<Pending>(request.session)
            .is_some_and(|pending| pending.0 == result.operation)
        {
            world.entity_mut(request.session).remove::<Pending>();
        }
    }
    let mut agents = HashMap::new();
    for (entity, parent, agent) in world
        .query_filtered::<(Entity, &ChildOf, &AgentId), With<Agent>>()
        .iter(world)
    {
        agents.insert((parent.parent(), agent.0.clone()), entity);
    }
    let mut scopes = HashMap::new();
    for (entity, parent, scope, name) in world
        .query::<(Entity, &ChildOf, &Scope, &Name)>()
        .iter(world)
    {
        scopes.insert(
            (parent.parent(), name.as_str().to_string(), scope.session),
            entity,
        );
    }
    let mut rules = HashMap::new();
    for (entity, parent, name) in world
        .query_filtered::<(Entity, &ChildOf, &Name), With<Grant>>()
        .iter(world)
    {
        rules.insert((parent.parent(), name.as_str().to_string()), entity);
    }
    for (file, actor, directory, transient, tool, binding, explicit) in incoming {
        let agent_key = (file, actor.clone());
        let agent = if let Some(agent) = agents.get(&agent_key) {
            *agent
        } else {
            let agent = world.spawn((Agent, AgentId(actor), ChildOf(file))).id();
            agents.insert(agent_key, agent);
            agent
        };
        let scope_key = (agent, directory.clone(), transient);
        let scope = if let Some(scope) = scopes.get(&scope_key) {
            *scope
        } else {
            let scope = world
                .spawn((
                    Scope { session: transient },
                    Name::new(directory),
                    ChildOf(agent),
                ))
                .id();
            scopes.insert(scope_key, scope);
            scope
        };
        if let Some((session, agent, cwd)) = binding {
            if let Some(pending) = world.get::<Pending>(session) {
                let operation = pending.0;
                world.despawn(operation);
                world.entity_mut(session).remove::<Pending>();
            }
            world.entity_mut(session).insert(Binding {
                scope,
                file,
                agent,
                cwd,
            });
        }
        let Some(tool) = tool else {
            continue;
        };
        if let Some(entity) = rules.get(&(scope, tool.clone())) {
            if explicit {
                let revision = if transient.is_none() {
                    let mut revision = world.get_mut::<Revision>(file).unwrap();
                    revision.0 = revision
                        .0
                        .checked_add(1)
                        .expect("approval revision exhausted");
                    revision.0
                } else {
                    0
                };
                world
                    .entity_mut(*entity)
                    .insert((Explicit, Revision(revision)));
            }
            continue;
        }
        let revision = if explicit && transient.is_none() {
            let mut revision = world.get_mut::<Revision>(file).unwrap();
            revision.0 = revision
                .0
                .checked_add(1)
                .expect("approval revision exhausted");
            revision.0
        } else {
            0
        };
        let entity = world
            .spawn((
                Grant,
                Name::new(tool.clone()),
                Revision(revision),
                ChildOf(scope),
            ))
            .id();
        if explicit {
            world.entity_mut(entity).insert(Explicit);
        }
        rules.insert((scope, tool), entity);
    }
    let saved: Vec<_> = world.resource_mut::<Messages<Saved>>().drain().collect();
    for result in saved {
        if world
            .get::<EntityTarget<File>>(result.operation)
            .is_none_or(|target| target.entity() != result.file)
            || world
                .get::<BaseRevision>(result.operation)
                .map(|captured| captured.0)
                != Some(result.revision)
            || world
                .get::<Revision>(result.file)
                .is_none_or(|current| result.revision > current.0)
        {
            continue;
        }
        let Some(mut file) = world.get_mut::<File>(result.file) else {
            continue;
        };
        if file.write != Some(result.operation) {
            continue;
        }
        file.write = None;
        match result.result {
            Ok(()) => {
                file.fingerprint = result.fingerprint;
                file.write_failed = false;
                world.get_mut::<BaseRevision>(result.file).unwrap().0 = result.revision;
                world
                    .entity_mut(result.operation)
                    .insert(OperationState::Succeeded);
                let mut acknowledged = Vec::new();
                for (entity, parent, revision) in world
                    .query_filtered::<(Entity, &ChildOf, &Revision), (With<Grant>, With<Explicit>)>(
                    )
                    .iter(world)
                {
                    let Some(scope) = world.get::<ChildOf>(parent.parent()) else {
                        continue;
                    };
                    let Some(agent) = world.get::<ChildOf>(scope.parent()) else {
                        continue;
                    };
                    if agent.parent() == result.file && revision.0 <= result.revision {
                        acknowledged.push(entity);
                    }
                }
                for entity in acknowledged {
                    world.entity_mut(entity).remove::<Explicit>();
                }
            }
            Err(error) => {
                file.loaded = false;
                file.read_failed = true;
                file.write_failed = true;
                world.entity_mut(result.operation).insert((
                    OperationState::Failed,
                    OperationError::new(error.clone()).unwrap(),
                ));
                world
                    .entity_mut(result.file)
                    .insert(OperationError::new(error).unwrap());
            }
        }
    }
}

fn settle(
    mut files: Query<(Entity, &mut File)>,
    operations: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, mut file) in &mut files {
        if let Some(operation) = file.read {
            if operations.get(operation).is_ok_and(|state| {
                matches!(state, OperationState::Pending | OperationState::Running)
            }) {
                continue;
            }
            file.read = None;
            file.loaded = false;
            file.read_failed = true;
            commands
                .entity(entity)
                .insert(OperationError::new("approval initial read was canceled").unwrap());
        }
        if let Some(operation) = file.write {
            if operations.get(operation).is_ok_and(|state| {
                matches!(state, OperationState::Pending | OperationState::Running)
            }) {
                continue;
            }
            file.write = None;
            file.loaded = false;
            file.read_failed = true;
            file.write_failed = true;
            commands
                .entity(entity)
                .insert(OperationError::new("approval replacement write was canceled").unwrap());
        }
    }
}

fn remove(event: On<Remove, Session>, scopes: Query<(Entity, &Scope)>, mut commands: Commands) {
    for (entity, scope) in &scopes {
        if scope.session == Some(event.entity) {
            commands.entity(entity).despawn();
        }
    }
}

fn counts(
    grants: Grants,
    sessions: Query<(Entity, Option<&vmux_session::ApprovalCount>), With<Session>>,
    mut commands: Commands,
) {
    for (session, previous) in &sessions {
        let count = grants.count(session);
        if previous.is_none_or(|previous| previous.0 != count) {
            commands
                .entity(session)
                .insert(vmux_session::ApprovalCount(count));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    struct Fixture {
        app: App,
        file: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(Plugin);
            app.update();
            let file = app
                .world_mut()
                .query_filtered::<Entity, With<File>>()
                .single(app.world())
                .unwrap();
            Self { app, file }
        }

        fn session(&mut self, agent: &str, cwd: &str) -> Entity {
            let session = self
                .app
                .world_mut()
                .spawn((Session, AgentId(agent.into()), Cwd(cwd.into())))
                .id();
            self.app.update();
            session
        }

        fn remember(&mut self, session: Entity, tool: &str) {
            let agent = self.app.world().get::<AgentId>(session).unwrap().clone();
            let cwd = self.app.world().get::<Cwd>(session).unwrap().clone();
            self.app.world_mut().write_message(Remember {
                file: self.file,
                session,
                agent,
                cwd,
                tool: tool.into(),
            });
            self.app.update();
        }

        fn resolve(&mut self, session: Entity, repository: Option<&str>, directory: Option<&str>) {
            let operations: Vec<_> = self
                .app
                .world_mut()
                .query::<(Entity, &ScopeRequest)>()
                .iter(self.app.world())
                .filter(|(_, request)| request.session == session)
                .map(|(entity, _)| entity)
                .collect();
            for operation in operations {
                self.app.world_mut().write_message(Scoped {
                    operation,
                    repository: repository.map(PathBuf::from),
                    directory: directory.map(PathBuf::from),
                });
            }
            self.app.update();
        }

        fn allows(&mut self, session: Entity, tool: &str) -> bool {
            let tool = tool.to_string();
            self.app
                .world_mut()
                .run_system_once(move |grants: Grants| grants.allows(session, &tool))
                .unwrap()
        }

        fn count(&mut self) -> usize {
            self.app
                .world_mut()
                .query_filtered::<Entity, With<Grant>>()
                .iter(self.app.world())
                .count()
        }
    }

    #[test]
    fn shared_repository_grants_keep_agent_aliases_and_other_scopes_isolated() {
        let mut fixture = Fixture::new();
        let first = fixture.session("codex-acp", "/project-a");
        fixture.remember(first, "mcp__vmux__run");
        fixture.resolve(first, Some("/repository-a"), Some("/project-a"));
        let sibling = fixture.session("CODEX-ACP", "/project-b");
        fixture.resolve(sibling, Some("/repository-a"), Some("/project-b"));
        let alias = fixture.session("codex", "/project-a");
        fixture.resolve(alias, Some("/repository-a"), Some("/project-a"));
        let other = fixture.session("codex-acp", "/project-c");
        fixture.resolve(other, Some("/repository-b"), Some("/project-c"));
        assert!(fixture.allows(first, "mcp.vmux.run"));
        assert!(fixture.allows(sibling, "mcp-vmux-run"));
        assert!(!fixture.allows(alias, "mcp.vmux.run"));
        assert!(!fixture.allows(other, "mcp.vmux.run"));
        assert!(!fixture.allows(first, "mcp.vmux.open_file"));
        assert_eq!(fixture.count(), 1);
    }

    #[test]
    fn accepted_permission_is_immediate_only_for_its_captured_session_context() {
        let mut fixture = Fixture::new();
        let session = fixture.session("one", "/project-a");
        let other = fixture.session("one", "/project-a");
        fixture.remember(session, "execute command");
        assert!(fixture.allows(session, "execute_command"));
        assert!(!fixture.allows(other, "execute_command"));
        fixture
            .app
            .world_mut()
            .entity_mut(session)
            .insert(AgentId("two".into()));
        assert!(!fixture.allows(session, "execute_command"));
        fixture.resolve(session, Some("/repository-a"), Some("/project-a"));
        assert!(!fixture.allows(session, "execute_command"));
        fixture.resolve(other, Some("/repository-a"), Some("/project-a"));
        assert!(fixture.allows(other, "execute_command"));
    }

    #[test]
    fn accepted_grant_survives_requester_removal_and_coalesces_duplicates() {
        let mut fixture = Fixture::new();
        let session = fixture.session("one", "/project-a");
        fixture.remember(session, "edit");
        fixture.remember(session, "edit");
        fixture.app.world_mut().despawn(session);
        fixture.resolve(session, Some("/repository-a"), Some("/project-a"));
        let next = fixture.session("one", "/project-b");
        fixture.resolve(next, Some("/repository-a"), Some("/project-b"));
        assert!(fixture.allows(next, "edit"));
        assert_eq!(fixture.count(), 1);
    }

    #[test]
    fn unavailable_filesystem_scope_remains_transient_and_ends_with_its_session() {
        let mut fixture = Fixture::new();
        let session = fixture.session("one", "/missing");
        fixture.remember(session, "edit");
        fixture.resolve(session, None, None);
        let other = fixture.session("one", "/missing");
        fixture.resolve(other, None, None);
        assert!(fixture.allows(session, "edit"));
        assert!(!fixture.allows(other, "edit"));
        let file = fixture.file;
        let records = fixture
            .app
            .world_mut()
            .run_system_once(move |grants: Records| grants.records(file).len())
            .unwrap();
        assert_eq!(records, 0);
        fixture.app.world_mut().despawn(session);
        fixture.app.world_mut().flush();
        assert_eq!(fixture.count(), 0);
    }

    #[test]
    fn changing_directory_invalidates_the_binding_before_the_next_update() {
        let mut fixture = Fixture::new();
        let session = fixture.session("one", "/project-a");
        fixture.remember(session, "edit");
        fixture.resolve(session, None, Some("/project-a"));
        assert!(fixture.allows(session, "edit"));
        fixture
            .app
            .world_mut()
            .entity_mut(session)
            .insert(Cwd("/project-b".into()));
        assert!(!fixture.allows(session, "edit"));
        fixture.app.update();
        fixture.resolve(session, None, Some("/project-b"));
        assert!(!fixture.allows(session, "edit"));
    }
}
