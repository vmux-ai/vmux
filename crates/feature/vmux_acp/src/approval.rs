use bevy_app::{App, Plugin as BevyPlugin, PreStartup, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::SystemParam;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::result::Result;
use vmux_ecs::{
    Active, BaseRevision, Cwd, EntityTarget, Operation, OperationError, OperationState, Revision,
};
use vmux_session::{AgentId, Session};

pub(crate) struct ApprovalPlugin;

impl BevyPlugin for ApprovalPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_message::<StoreRequested>()
            .add_message::<Remember>()
            .add_message::<Scoped>()
            .add_message::<Loaded>()
            .add_message::<Saved>()
            .add_message::<PreparedGrant>()
            .add_systems(PreStartup, spawn_default_file)
            .add_systems(
                Reconcile,
                (initialize, remember, receive, materialize, receive_saves).chain(),
            )
            .configure_sets(Update, (Prepare, Resolve).chain())
            .add_systems(
                Update,
                (initialize, remember, prepare).chain().in_set(Prepare),
            )
            .add_systems(
                Update,
                (receive, materialize, receive_saves, settle)
                    .chain()
                    .in_set(Resolve),
            )
            .add_systems(
                Update,
                counts
                    .after(Resolve)
                    .in_set(vmux_session::ApprovalProjectionSet),
            )
            .add_observer(remove)
            .add_observer(load)
            .add_observer(scope)
            .add_observer(apply)
            .add_observer(saved)
            .init_schedule(Reconcile);
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Reconcile;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Prepare;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Resolve;

fn spawn_default_file(mut commands: Commands) {
    commands.spawn(File::bundle(None));
}

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

#[derive(Message, Event)]
pub(crate) struct Scoped {
    pub(crate) operation: Entity,
    pub(crate) repository: Option<PathBuf>,
    pub(crate) directory: Option<PathBuf>,
}

#[derive(Message, Event)]
pub(crate) struct Loaded {
    pub(crate) file: Entity,
    pub(crate) operation: Entity,
    pub(crate) fingerprint: Option<u64>,
    pub(crate) result: Result<Vec<(String, String, String)>, String>,
}

#[derive(Message, Event)]
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

#[derive(Message, Event)]
struct PreparedGrant {
    file: Entity,
    actor: String,
    directory: String,
    transient: Option<Entity>,
    tool: Option<String>,
    binding: Option<(Entity, String, PathBuf)>,
    explicit: bool,
}

fn receive(
    mut loaded: ResMut<Messages<Loaded>>,
    mut scoped: ResMut<Messages<Scoped>>,
    mut commands: Commands,
) {
    for receipt in loaded.drain() {
        commands.trigger(receipt);
    }
    for receipt in scoped.drain() {
        commands.trigger(receipt);
    }
}

fn materialize(mut prepared: ResMut<Messages<PreparedGrant>>, mut commands: Commands) {
    for grant in prepared.drain() {
        commands.trigger(grant);
    }
}

fn receive_saves(mut saved: ResMut<Messages<Saved>>, mut commands: Commands) {
    for receipt in saved.drain() {
        commands.trigger(receipt);
    }
}

fn load(
    mut receipt: On<Loaded>,
    mut files: Query<&mut File>,
    targets: Query<&EntityTarget<File>>,
    grants: Query<(Entity, &ChildOf, Has<Explicit>), With<Grant>>,
    parents: Query<&ChildOf>,
    mut prepared: MessageWriter<PreparedGrant>,
    mut commands: Commands,
) {
    let receipt = receipt.event_mut();
    if !targets
        .get(receipt.operation)
        .is_ok_and(|target| target.entity() == receipt.file)
    {
        return;
    }
    let Ok(mut file) = files.get_mut(receipt.file) else {
        return;
    };
    if file.read != Some(receipt.operation) {
        return;
    }
    file.read = None;
    let result = std::mem::replace(&mut receipt.result, Ok(Vec::new()));
    let records = match result {
        Ok(records) => records,
        Err(error) => {
            file.loaded = false;
            file.read_failed = true;
            commands.entity(receipt.operation).insert((
                OperationState::Failed,
                OperationError::new(error.clone()).unwrap(),
            ));
            commands
                .entity(receipt.file)
                .insert(OperationError::new(error).unwrap());
            return;
        }
    };
    file.loaded = true;
    file.read_failed = false;
    file.fingerprint = receipt.fingerprint;
    commands.entity(receipt.file).remove::<OperationError>();
    commands
        .entity(receipt.operation)
        .insert(OperationState::Succeeded);
    for (entity, parent, explicit) in &grants {
        if explicit {
            continue;
        }
        let Ok(scope) = parents.get(parent.parent()) else {
            continue;
        };
        let Ok(agent) = parents.get(scope.parent()) else {
            continue;
        };
        if agent.parent() == receipt.file {
            commands.entity(entity).despawn();
        }
    }
    for (actor, directory, tool) in records {
        if actor.is_empty()
            || actor != actor.trim().to_ascii_lowercase()
            || tool.is_empty()
            || tool != Grant::key(&tool)
        {
            continue;
        }
        prepared.write(PreparedGrant {
            file: receipt.file,
            actor,
            directory,
            transient: None,
            tool: Some(tool),
            binding: None,
            explicit: false,
        });
    }
}

fn scope(
    mut receipt: On<Scoped>,
    requests: Query<(
        &ScopeRequest,
        &ChildOf,
        &AgentId,
        &Cwd,
        Option<&OperationState>,
        Option<&Name>,
    )>,
    files: Query<Has<Active>, With<File>>,
    sessions: Query<(&AgentId, &Cwd, Option<&Pending>), With<Session>>,
    mut prepared: MessageWriter<PreparedGrant>,
    mut commands: Commands,
) {
    let receipt = receipt.event_mut();
    let Ok((request, parent, agent, cwd, state, name)) = requests.get(receipt.operation) else {
        return;
    };
    let owner = if request.consent {
        request.file
    } else {
        request.session
    };
    if parent.parent() != owner
        || !files.contains(request.file)
        || matches!(
            state,
            Some(OperationState::Canceled | OperationState::Failed)
        )
    {
        return;
    }
    let binding = match sessions.get(request.session) {
        Ok((found, directory, _))
            if found == agent && directory == cwd && files.get(request.file) == Ok(true) =>
        {
            Some((request.session, agent.0.clone(), cwd.0.clone()))
        }
        _ => None,
    };
    if !request.consent && binding.is_none() {
        commands
            .entity(receipt.operation)
            .remove::<ScopeRequest>()
            .insert(OperationState::Canceled);
        return;
    }
    let path = receipt
        .repository
        .take()
        .or_else(|| receipt.directory.take());
    let transient = if path.is_none() {
        Some(request.session)
    } else {
        None
    };
    if transient.is_some() && !sessions.contains(request.session) {
        commands
            .entity(receipt.operation)
            .remove::<ScopeRequest>()
            .insert((
                OperationState::Failed,
                OperationError::new("approval scope is unavailable").unwrap(),
            ));
        return;
    }
    let directory = match path {
        Some(path) => Scope::key(&path),
        None => Scope::key(&cwd.0),
    };
    let tool = if request.consent {
        name.map(|name| name.as_str().to_string())
    } else {
        None
    };
    prepared.write(PreparedGrant {
        file: request.file,
        actor: agent.0.trim().to_ascii_lowercase(),
        directory,
        transient,
        tool,
        binding,
        explicit: request.consent,
    });
    commands
        .entity(receipt.operation)
        .remove::<ScopeRequest>()
        .insert(OperationState::Succeeded);
    if sessions
        .get(request.session)
        .is_ok_and(|(_, _, pending)| pending.is_some_and(|pending| pending.0 == receipt.operation))
    {
        commands.entity(request.session).remove::<Pending>();
    }
}

#[derive(SystemParam)]
struct GrantCatalog<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    agents: Query<'w, 's, (Entity, &'static ChildOf, &'static AgentId), With<Agent>>,
    scopes: Query<'w, 's, (Entity, &'static ChildOf, &'static Scope, &'static Name)>,
    rules: Query<'w, 's, (Entity, &'static ChildOf, &'static Name), With<Grant>>,
}

impl GrantCatalog<'_, '_> {
    fn agent(&self, file: Entity, actor: &str) -> Option<Entity> {
        let children = self.children.get(file).ok()?;
        let mut found = None;
        for child in children.iter() {
            let Ok((entity, parent, agent)) = self.agents.get(child) else {
                continue;
            };
            if parent.parent() == file && agent.0 == actor {
                found = Some(entity);
            }
        }
        found
    }

    fn scope(&self, agent: Entity, directory: &str, transient: Option<Entity>) -> Option<Entity> {
        let children = self.children.get(agent).ok()?;
        let mut found = None;
        for child in children.iter() {
            let Ok((entity, parent, scope, name)) = self.scopes.get(child) else {
                continue;
            };
            if parent.parent() == agent && name.as_str() == directory && scope.session == transient
            {
                found = Some(entity);
            }
        }
        found
    }

    fn rule(&self, scope: Entity, tool: &str) -> Option<Entity> {
        let children = self.children.get(scope).ok()?;
        let mut found = None;
        for child in children.iter() {
            let Ok((entity, parent, name)) = self.rules.get(child) else {
                continue;
            };
            if parent.parent() == scope && name.as_str() == tool {
                found = Some(entity);
            }
        }
        found
    }
}

fn apply(
    grant: On<PreparedGrant>,
    catalog: GrantCatalog,
    sessions: Query<Option<&Pending>, With<Session>>,
    mut revisions: Query<&mut Revision, With<File>>,
    mut commands: Commands,
) {
    if !revisions.contains(grant.file) {
        return;
    }
    let bind = grant
        .binding
        .as_ref()
        .is_some_and(|(session, _, _)| sessions.contains(*session));
    if grant.binding.is_some() && !bind && (!grant.explicit || grant.transient.is_some()) {
        return;
    }
    if grant
        .transient
        .is_some_and(|session| !sessions.contains(session))
    {
        return;
    }
    let agent = match catalog.agent(grant.file, &grant.actor) {
        Some(agent) => agent,
        None => commands
            .spawn((Agent, AgentId(grant.actor.clone()), ChildOf(grant.file)))
            .id(),
    };
    let scope = match catalog.scope(agent, &grant.directory, grant.transient) {
        Some(scope) => scope,
        None => commands
            .spawn((
                Scope {
                    session: grant.transient,
                },
                Name::new(grant.directory.clone()),
                ChildOf(agent),
            ))
            .id(),
    };
    if bind && let Some((session, actor, cwd)) = &grant.binding {
        if let Ok(Some(pending)) = sessions.get(*session) {
            commands.entity(pending.0).try_despawn();
            commands.entity(*session).remove::<Pending>();
        }
        commands.entity(*session).insert(Binding {
            scope,
            file: grant.file,
            agent: actor.clone(),
            cwd: cwd.clone(),
        });
    }
    let Some(tool) = &grant.tool else {
        return;
    };
    let existing = catalog.rule(scope, tool);
    if existing.is_some() && !grant.explicit {
        return;
    }
    let revision = if grant.explicit && grant.transient.is_none() {
        let mut revision = revisions.get_mut(grant.file).unwrap();
        revision.0 = revision
            .0
            .checked_add(1)
            .expect("approval revision exhausted");
        revision.0
    } else {
        0
    };
    let entity = match existing {
        Some(entity) => entity,
        None => commands
            .spawn((
                Grant,
                Name::new(tool.clone()),
                Revision(revision),
                ChildOf(scope),
            ))
            .id(),
    };
    if grant.explicit {
        commands
            .entity(entity)
            .insert((Explicit, Revision(revision)));
    }
}

#[derive(SystemParam)]
struct Acknowledgments<'w, 's> {
    revisions: Query<'w, 's, &'static Revision>,
    versions: ParamSet<
        'w,
        's,
        (
            Query<'w, 's, (&'static EntityTarget<File>, &'static BaseRevision)>,
            Query<'w, 's, &'static mut BaseRevision>,
        ),
    >,
}

impl Acknowledgments<'_, '_> {
    fn accepts(&mut self, operation: Entity, file: Entity, revision: u64) -> bool {
        self.versions
            .p0()
            .get(operation)
            .is_ok_and(|(target, base)| target.entity() == file && base.0 == revision)
            && self
                .revisions
                .get(file)
                .is_ok_and(|current| revision <= current.0)
    }
}

fn saved(
    mut receipt: On<Saved>,
    mut files: Query<&mut File>,
    mut acknowledgments: Acknowledgments,
    rules: Query<(Entity, &ChildOf, &Revision), (With<Grant>, With<Explicit>)>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let receipt = receipt.event_mut();
    if !acknowledgments.accepts(receipt.operation, receipt.file, receipt.revision) {
        return;
    }
    let Ok(mut file) = files.get_mut(receipt.file) else {
        return;
    };
    if file.write != Some(receipt.operation) {
        return;
    }
    file.write = None;
    match std::mem::replace(&mut receipt.result, Ok(())) {
        Ok(()) => {
            file.fingerprint = receipt.fingerprint;
            file.write_failed = false;
            acknowledgments
                .versions
                .p1()
                .get_mut(receipt.file)
                .unwrap()
                .0 = receipt.revision;
            commands
                .entity(receipt.operation)
                .insert(OperationState::Succeeded);
            for (entity, parent, revision) in &rules {
                let Ok(scope) = parents.get(parent.parent()) else {
                    continue;
                };
                let Ok(agent) = parents.get(scope.parent()) else {
                    continue;
                };
                if agent.parent() == receipt.file && revision.0 <= receipt.revision {
                    commands.entity(entity).remove::<Explicit>();
                }
            }
        }
        Err(error) => {
            file.loaded = false;
            file.read_failed = true;
            file.write_failed = true;
            commands.entity(receipt.operation).insert((
                OperationState::Failed,
                OperationError::new(error.clone()).unwrap(),
            ));
            commands
                .entity(receipt.file)
                .insert(OperationError::new(error).unwrap());
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
            app.add_plugins(ApprovalPlugin);
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
    fn requester_removal_between_receipt_validation_and_materialization_keeps_only_durable_grants()
    {
        for repository in [Some("/repo"), None] {
            let mut fixture = Fixture::new();
            let session = fixture.session("claude", "/repo/project");
            fixture.remember(session, "bash");
            fixture.app.add_systems(
                Update,
                (move |mut commands: Commands| {
                    commands.entity(session).try_despawn();
                })
                .after(receive)
                .before(materialize),
            );
            fixture.resolve(session, repository, None);
            assert!(fixture.app.world().get_entity(session).is_err());
            assert_eq!(fixture.count(), usize::from(repository.is_some()));
            let scopes: Vec<_> = fixture
                .app
                .world_mut()
                .query::<&Scope>()
                .iter(fixture.app.world())
                .map(|scope| scope.session)
                .collect();
            assert!(scopes.iter().all(Option::is_none));
            if repository.is_some() {
                let survivor = fixture.session("claude", "/repo/other");
                fixture.resolve(survivor, repository, None);
                assert!(fixture.allows(survivor, "bash"));
            }
        }
    }

    #[test]
    fn consent_receipt_cannot_retire_a_later_scope_receipt_in_the_same_batch() {
        let mut fixture = Fixture::new();
        let session = fixture.session("claude", "/repo/project");
        let pending = fixture.app.world().get::<Pending>(session).unwrap().0;
        fixture.remember(session, "bash");
        let consent = fixture
            .app
            .world_mut()
            .query::<(Entity, &ScopeRequest)>()
            .iter(fixture.app.world())
            .find_map(|(entity, request)| {
                (request.consent && request.session == session).then_some(entity)
            })
            .unwrap();
        for (operation, directory) in [(consent, "/repo/consented"), (pending, "/repo/current")] {
            fixture.app.world_mut().write_message(Scoped {
                operation,
                repository: Some(directory.into()),
                directory: None,
            });
        }
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(pending),
            Some(&OperationState::Succeeded)
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(consent),
            Some(&OperationState::Succeeded)
        );
        assert!(!fixture.allows(session, "bash"));
        let matching = fixture.session("claude", "/repo/consented/subdir");
        fixture.resolve(matching, Some("/repo/consented"), None);
        assert!(fixture.allows(matching, "bash"));
    }

    #[test]
    fn save_acknowledgment_preserves_grants_created_after_its_captured_revision() {
        let mut fixture = Fixture::new();
        let session = fixture.session("claude", "/repo/project");
        fixture.resolve(session, Some("/repo"), None);
        fixture.remember(session, "bash");
        fixture.resolve(session, Some("/repo"), None);
        let revision = fixture.app.world().get::<Revision>(fixture.file).unwrap().0;
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("approval.write").unwrap().running(),
                EntityTarget::<File>::new(fixture.file),
                BaseRevision(revision),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .get_mut::<File>(fixture.file)
            .unwrap()
            .write = Some(operation);
        fixture.remember(session, "read");
        fixture.resolve(session, Some("/repo"), None);
        fixture.app.world_mut().write_message(Saved {
            file: fixture.file,
            operation,
            revision,
            fingerprint: Some(7),
            result: Ok(()),
        });
        fixture.app.world_mut().run_schedule(Reconcile);
        assert!(fixture.allows(session, "bash"));
        assert!(fixture.allows(session, "read"));
        let rules: Vec<_> = fixture
            .app
            .world_mut()
            .query_filtered::<(&Name, Has<Explicit>), With<Grant>>()
            .iter(fixture.app.world())
            .map(|(name, explicit)| (name.as_str().to_string(), explicit))
            .collect();
        assert_eq!(rules.len(), 2);
        assert!(rules.contains(&("bash".into(), false)));
        assert!(rules.contains(&("read".into(), true)));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<BaseRevision>(fixture.file)
                .unwrap()
                .0,
            revision
        );
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
