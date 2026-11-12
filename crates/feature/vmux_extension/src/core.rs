use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::{SystemParam, SystemState};
use serde::{Deserialize, Serialize};
use vmux_api::extension::{ExtPinRequest, ExtToggleRequest, ExtUninstallRequest};
pub use vmux_ecs::store::State;
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget,
    OperationTargetRevision, Revision,
};

use crate::snapshot::{EnableForProfileResult, ExtEntry, ExtensionGrants, Index};

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<Clock>()
            .add_message::<LoadRequest>()
            .add_message::<LoadCompleted>()
            .add_message::<WriteTaskRequest>()
            .add_message::<WriteCompleted>()
            .add_message::<Changed>()
            .add_message::<CleanupRequest>()
            .add_message::<Reconcile>()
            .add_schedule(Schedule::new(ExtensionSchedule))
            .configure_sets(
                ExtensionSchedule,
                (ExtensionSet::Resolve, ExtensionSet::Accept).chain(),
            )
            .add_systems(
                ExtensionSchedule,
                (reset, begin, cancel, restore, finish, reconcile, evidence)
                    .chain()
                    .in_set(ExtensionSet::Resolve),
            )
            .add_systems(
                ExtensionSchedule,
                (autosave, queue, accept, settle, release)
                    .chain()
                    .in_set(ExtensionSet::Accept),
            )
            .add_systems(Update, step.in_set(ExtensionSet::Step));
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExtensionSchedule;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtensionSet {
    Step,
    Resolve,
    Accept,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[require(State, Revision)]
pub struct Root(pub PathBuf);

#[derive(Component)]
pub struct Seed(pub Result<Index, String>);

#[derive(Component)]
pub struct Loaded;

#[derive(Component)]
#[require(Revision)]
pub struct Package;

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Id(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Version(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Popup(pub Option<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Icon(pub Option<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Permissions(pub Vec<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct OptionalPermissions(pub Vec<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct HostPermissions(pub Vec<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct OptionalHostPermissions(pub Vec<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SourceHash(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(pub Option<String>);

#[derive(Component)]
#[require(Revision)]
pub struct Association;

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileId(pub String);

#[derive(Component)]
pub struct Enabled;

#[derive(Component)]
pub struct Pinned;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants(pub ExtensionGrants);

#[derive(Component)]
pub struct Installed;

#[derive(Component, Clone, Debug)]
pub struct InstallRequest {
    pub ticket: Ticket,
    pub profile: String,
    pub id: String,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct InstallOwner {
    pub package: Entity,
    pub association: Entity,
}

#[derive(Component)]
pub(crate) struct Installing(pub Entity);

#[derive(Component)]
pub struct AutoSave;

#[derive(Component)]
pub struct WriteError(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub owner: Entity,
    pub directory: PathBuf,
    pub generation: u64,
}

#[derive(Component)]
pub(crate) struct Scope(pub Ticket);

#[derive(Message, Clone)]
pub struct LoadRequest(pub Ticket);

#[derive(Message)]
pub struct LoadCompleted {
    pub ticket: Ticket,
    pub result: Result<Index, String>,
}

#[derive(Message, Clone)]
pub struct WriteTaskRequest {
    pub ticket: Ticket,
    pub operation: Entity,
    pub revision: u64,
    pub snapshot: Index,
}

#[derive(Message)]
pub struct WriteCompleted {
    pub ticket: Ticket,
    pub operation: Entity,
    pub revision: u64,
    pub result: Result<(), String>,
}

#[derive(Message)]
pub struct Changed(pub Entity);

#[derive(Message)]
pub struct CleanupCompleted {
    pub ticket: Ticket,
    pub id: String,
    pub profile: String,
    pub result: Result<(), String>,
}

#[derive(Message, Clone)]
pub struct CleanupRequest {
    pub ticket: Ticket,
    pub id: String,
    pub profile: String,
    pub package: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Mutation {
    Toggle(ExtToggleRequest),
    Pin(ExtPinRequest),
    Remove(ExtUninstallRequest),
    Install(Box<ExtEntry>),
    Grant { id: String, grants: ExtensionGrants },
    Save,
}

#[derive(Component, Clone, Debug)]
pub struct Request {
    pub ticket: Ticket,
    pub profile: String,
    pub mutation: Mutation,
}

impl Request {
    pub fn bundle(self) -> impl Bundle {
        let kind = match self.mutation {
            Mutation::Toggle(_) => "extension.toggle",
            Mutation::Pin(_) => "extension.pin",
            Mutation::Remove(_) => "extension.remove",
            Mutation::Install(_) => "extension.install",
            Mutation::Grant { .. } => "extension.grant",
            Mutation::Save => "extension.save",
        };
        (
            Operation::bundle(kind).unwrap(),
            OperationTarget::new(self.ticket.owner),
            self,
        )
    }

    fn propose(&self, mut snapshot: Index) -> Result<Index, String> {
        match &self.mutation {
            Mutation::Toggle(request) => {
                match snapshot.set_enabled_for(
                    &self.profile,
                    &request.id,
                    request.enabled,
                    request.approve_permissions,
                ) {
                    EnableForProfileResult::Updated => {}
                    EnableForProfileResult::NeedsApproval => {
                        return Err("extension permissions require approval".into());
                    }
                    EnableForProfileResult::NotFound => {
                        return Err("extension association not found".into());
                    }
                }
            }
            Mutation::Pin(request) => {
                let Some(entry) = snapshot.entries.iter().find(|entry| entry.id == request.id)
                else {
                    return Err("extension package not found".into());
                };
                if !entry.installed_for(&self.profile) {
                    return Err("extension association not found".into());
                }
                snapshot.set_pinned_for(&self.profile, &request.id, request.pinned);
            }
            Mutation::Remove(request) => {
                if let Some(entry) = snapshot
                    .entries
                    .iter_mut()
                    .find(|entry| entry.id == request.id)
                {
                    entry.profile_enabled.remove(&self.profile);
                    entry.profile_pinned.remove(&self.profile);
                    entry.approved_grants.remove(&self.profile);
                    if entry.profile_enabled.is_empty() {
                        snapshot.remove(&request.id);
                    }
                }
            }
            Mutation::Install(entry) => {
                let mut next = entry.as_ref().clone();
                if let Some(existing) = snapshot.entries.iter().find(|entry| entry.id == next.id) {
                    next.profile_enabled.clone_from(&existing.profile_enabled);
                    next.profile_pinned.clone_from(&existing.profile_pinned);
                    next.approved_grants.clone_from(&existing.approved_grants);
                } else {
                    next.profile_enabled.clear();
                    next.profile_pinned.clear();
                    next.approved_grants.clear();
                }
                next.enabled = false;
                next.profile_enabled
                    .entry(self.profile.clone())
                    .or_insert(false);
                for (profile, grants) in &mut next.approved_grants {
                    grants.retain_declared(
                        &next.permissions,
                        &next.optional_permissions,
                        &next.host_permissions,
                        &next.optional_host_permissions,
                    );
                    if !grants.covers(&next.permissions, &next.host_permissions) {
                        next.profile_enabled.insert(profile.clone(), false);
                    }
                }
                for (profile, enabled) in &mut next.profile_enabled {
                    if !next
                        .approved_grants
                        .get(profile)
                        .cloned()
                        .unwrap_or_default()
                        .covers(&next.permissions, &next.host_permissions)
                    {
                        *enabled = false;
                    }
                }
                snapshot.upsert(next);
            }
            Mutation::Grant { id, grants } => {
                let Some(entry) = snapshot.entries.iter_mut().find(|entry| entry.id == *id) else {
                    return Err("extension package not found".into());
                };
                if !entry.installed_for(&self.profile) {
                    return Err("extension association not found".into());
                }
                for permission in &grants.permissions {
                    if !entry.permissions.contains(permission)
                        && !entry.optional_permissions.contains(permission)
                    {
                        return Err("permission is not declared by the extension".into());
                    }
                }
                for permission in &grants.host_permissions {
                    if !entry.host_permissions.contains(permission)
                        && !entry.optional_host_permissions.contains(permission)
                    {
                        return Err("host permission is not declared by the extension".into());
                    }
                }
                entry
                    .approved_grants
                    .insert(self.profile.clone(), grants.clone());
                if !grants.covers(&entry.permissions, &entry.host_permissions) {
                    entry.profile_enabled.insert(self.profile.clone(), false);
                }
            }
            Mutation::Save => {}
        }
        snapshot.validate()?;
        Ok(snapshot)
    }
}

#[derive(Component, Debug)]
pub struct PreparedEvidence(pub Vec<Evidence>);

#[derive(Debug)]
pub struct Evidence {
    pub id: String,
    pub profile: String,
    pub source_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(SystemParam)]
pub struct Catalog<'w, 's> {
    roots: Query<
        'w,
        's,
        (
            &'static Root,
            &'static Scope,
            &'static Revision,
            Option<&'static Loaded>,
        ),
    >,
    packages: Query<
        'w,
        's,
        (
            Entity,
            &'static Id,
            &'static Name,
            &'static Version,
            &'static Popup,
            &'static Icon,
            &'static Permissions,
            &'static OptionalPermissions,
            &'static HostPermissions,
            &'static OptionalHostPermissions,
            &'static SourceHash,
            &'static PublicKey,
            &'static ChildOf,
        ),
        With<Package>,
    >,
    associations: Query<
        'w,
        's,
        (
            Entity,
            &'static ProfileId,
            Has<Enabled>,
            Has<Pinned>,
            &'static Grants,
            &'static ChildOf,
        ),
        (With<Association>, With<Installed>),
    >,
}

impl Catalog<'_, '_> {
    pub fn association(&self, owner: Entity, id: &str, profile: &str) -> Option<Entity> {
        let ticket = self.ticket(owner)?;
        if !self.ready(ticket) {
            return None;
        }
        let mut package = None;
        for (entity, package_id, .., parent) in &self.packages {
            if parent.parent() == owner && package_id.0 == id {
                if package.is_some() {
                    return None;
                }
                package = Some(entity);
            }
        }
        let package = package?;
        let mut association = None;
        for (entity, profile_id, .., parent) in &self.associations {
            if parent.parent() == package && profile_id.0 == profile {
                if association.is_some() {
                    return None;
                }
                association = Some(entity);
            }
        }
        association
    }

    pub fn ticket(&self, owner: Entity) -> Option<&Ticket> {
        let (root, scope, _, _) = self.roots.get(owner).ok()?;
        if root.0 != scope.0.directory {
            return None;
        }
        Some(&scope.0)
    }

    pub fn ready(&self, ticket: &Ticket) -> bool {
        self.roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope, _, loaded)| {
                root.0 == ticket.directory && scope.0 == *ticket && loaded.is_some()
            })
    }

    pub fn revision(&self, owner: Entity) -> Option<Revision> {
        self.roots
            .get(owner)
            .ok()
            .map(|(_, _, revision, _)| *revision)
    }

    pub fn snapshot(&self, owner: Entity) -> Result<Index, String> {
        let ticket = self.ticket(owner).ok_or("extension root not found")?;
        if !self.ready(ticket) {
            return Err("extension store is not loaded".into());
        }
        let mut snapshot = Index::default();
        let mut ids = HashSet::new();
        for (
            entity,
            id,
            name,
            version,
            popup,
            icon,
            permissions,
            optional_permissions,
            hosts,
            optional_hosts,
            hash,
            public_key,
            parent,
        ) in &self.packages
        {
            if parent.parent() != owner {
                continue;
            }
            if !ids.insert(id.0.clone()) {
                return Err("duplicate extension package identity".into());
            }
            let mut entry = ExtEntry {
                id: id.0.clone(),
                name: name.as_str().into(),
                version: version.0.clone(),
                popup: popup.0.clone(),
                icon: icon.0.clone(),
                enabled: false,
                profile_enabled: BTreeMap::new(),
                profile_pinned: BTreeMap::new(),
                permissions: permissions.0.clone(),
                optional_permissions: optional_permissions.0.clone(),
                host_permissions: hosts.0.clone(),
                optional_host_permissions: optional_hosts.0.clone(),
                approved_grants: BTreeMap::new(),
                source_hash: hash.0.clone(),
                public_key_b64: public_key.0.clone(),
            };
            for (_, profile, enabled, pinned, grants, parent) in &self.associations {
                if parent.parent() != entity {
                    continue;
                }
                if entry
                    .profile_enabled
                    .insert(profile.0.clone(), enabled)
                    .is_some()
                {
                    return Err("duplicate extension profile association".into());
                }
                entry.profile_pinned.insert(profile.0.clone(), pinned);
                entry
                    .approved_grants
                    .insert(profile.0.clone(), grants.0.clone());
            }
            snapshot.entries.push(entry);
        }
        snapshot.entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(snapshot)
    }
}

fn step(world: &mut World) {
    world.run_schedule(ExtensionSchedule);
}

#[derive(Resource, Default)]
struct Clock {
    generation: u64,
    order: u64,
}

impl Clock {
    fn ticket(&mut self, owner: Entity, directory: &std::path::Path) -> Ticket {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("extension generation exhausted");
        Ticket {
            owner,
            directory: directory.to_path_buf(),
            generation: self.generation,
        }
    }

    fn order(&mut self) -> u64 {
        self.order = self
            .order
            .checked_add(1)
            .expect("extension order exhausted");
        self.order
    }
}

#[derive(Component)]
struct Queued(u64);

#[derive(Component)]
struct Repair;

#[derive(Component)]
struct Writing {
    ticket: Ticket,
    operation: Entity,
    base: Revision,
    revision: u64,
    snapshot: Index,
    request: Request,
    before: State,
}

#[derive(Message)]
struct Reconcile {
    ticket: Ticket,
    snapshot: Index,
    revision: Revision,
}

fn reset(
    roots: Query<(
        Entity,
        Ref<Root>,
        Option<&Scope>,
        Option<&Seed>,
        Option<&Writing>,
    )>,
    packages: Query<(Entity, &ChildOf), With<Package>>,
    mut clock: ResMut<Clock>,
    mut loads: MessageWriter<LoadRequest>,
    mut completed: MessageWriter<LoadCompleted>,
    mut commands: Commands,
) {
    for (owner, root, scope, seed, writing) in &roots {
        if seed.is_none()
            && !root.is_changed()
            && scope.is_some_and(|scope| scope.0.directory == root.0)
        {
            continue;
        }
        for (entity, parent) in &packages {
            if parent.parent() == owner {
                commands.entity(entity).despawn();
            }
        }
        let ticket = clock.ticket(owner, &root.0);
        let mut state = State::default();
        state.materialize();
        let repair = writing.is_some_and(|writing| writing.ticket.directory == root.0);
        if repair {
            state.advance();
        }
        commands
            .entity(owner)
            .remove::<(Loaded, Writing, WriteError, Seed, Repair)>()
            .insert((Scope(ticket.clone()), state, Revision::default()));
        if repair {
            commands.entity(owner).insert(Repair);
        }
        if let Some(seed) = seed {
            completed.write(LoadCompleted {
                ticket,
                result: seed.0.clone(),
            });
        } else {
            loads.write(LoadRequest(ticket));
        }
    }
}

fn begin(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &InstallRequest, &OperationState), (With<Operation>, Without<InstallOwner>)>();
    let mut requests = Vec::new();
    for (entity, request, state) in query.iter(world) {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            requests.push((entity, request.clone()));
        }
    }
    for (operation, request) in requests {
        let root = request.ticket.owner;
        if world
            .get::<Root>(root)
            .is_none_or(|root| root.0 != request.ticket.directory)
            || world
                .get::<Scope>(root)
                .is_none_or(|scope| scope.0 != request.ticket)
            || world.get::<Loaded>(root).is_none()
            || request.id.len() != 32
            || !request.id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
        {
            world.trigger(OperationCompleted {
                entity: operation,
                result: Err("invalid extension installation context".into()),
            });
            world.entity_mut(operation).remove::<InstallRequest>();
            continue;
        }
        let mut packages = world.query_filtered::<(Entity, &Id, &ChildOf), With<Package>>();
        let mut found = None;
        for (entity, id, parent) in packages.iter(world) {
            if parent.parent() == root && id.0 == request.id {
                found = Some(entity);
                break;
            }
        }
        let package = match found {
            Some(entity) => entity,
            None => world
                .spawn((Package, Id(request.id.clone()), ChildOf(root)))
                .id(),
        };
        let mut associations =
            world.query_filtered::<(Entity, &ProfileId, &ChildOf), With<Association>>();
        let mut found = None;
        for (entity, profile, parent) in associations.iter(world) {
            if parent.parent() == package && profile.0 == request.profile {
                found = Some(entity);
                break;
            }
        }
        let association = match found {
            Some(entity) => entity,
            None => world
                .spawn((
                    Association,
                    ProfileId(request.profile.clone()),
                    Grants::default(),
                    ChildOf(package),
                ))
                .id(),
        };
        world.entity_mut(package).insert(Installing(operation));
        world.entity_mut(association).insert(Installing(operation));
        world.entity_mut(operation).insert(InstallOwner {
            package,
            association,
        });
    }
}

fn cancel(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        Option<&Request>,
        Option<&InstallRequest>,
        Option<&InstallOwner>,
        &OperationState,
    ), With<Operation>>();
    let mut stale = Vec::new();
    for (operation, request, install, owner, state) in query.iter(world) {
        let ticket = match (request, install) {
            (Some(request), _) => &request.ticket,
            (_, Some(install)) => &install.ticket,
            _ => continue,
        };
        let current = world
            .get::<Root>(ticket.owner)
            .is_some_and(|root| root.0 == ticket.directory)
            && world
                .get::<Scope>(ticket.owner)
                .is_some_and(|scope| scope.0 == *ticket);
        let owned = owner.is_none_or(|owner| {
            world
                .get::<Installing>(owner.package)
                .is_some_and(|epoch| epoch.0 == operation)
                && world
                    .get::<Installing>(owner.association)
                    .is_some_and(|epoch| epoch.0 == operation)
        });
        if !current || !owned {
            stale.push((
                operation,
                owner.copied(),
                matches!(state, OperationState::Pending | OperationState::Running),
            ));
        }
    }
    for (operation, owner, active) in stale {
        if active {
            world.entity_mut(operation).insert(OperationState::Canceled);
        }
        world
            .entity_mut(operation)
            .remove::<(Request, Queued, InstallRequest, InstallOwner)>();
        if let Some(owner) = owner {
            world.entity_mut(operation).insert(ReleasedInstall(owner));
        }
    }
}

fn restore(
    mut completed: MessageReader<LoadCompleted>,
    mut roots: Query<(&Root, &Scope, &mut State, Has<Loaded>)>,
    mut updates: MessageWriter<Reconcile>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        let ticket = &completion.ticket;
        let Ok((root, scope, mut state, loaded)) = roots.get_mut(ticket.owner) else {
            continue;
        };
        if loaded || root.0 != ticket.directory || scope.0 != *ticket {
            continue;
        }
        let snapshot = match &completion.result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                state.fail(error.clone());
                continue;
            }
        };
        if let Err(error) = snapshot.validate() {
            state.fail(error);
            continue;
        }
        state.ready();
        if snapshot.requires_save() {
            state.advance();
        }
        commands.entity(ticket.owner).insert(Loaded);
        updates.write(Reconcile {
            ticket: ticket.clone(),
            snapshot: snapshot.clone(),
            revision: Revision::default(),
        });
    }
}

#[derive(SystemParam)]
struct WriteEffects<'w> {
    updates: MessageWriter<'w, Reconcile>,
    changed: MessageWriter<'w, Changed>,
    cleanup: MessageWriter<'w, CleanupRequest>,
}

fn finish(
    mut completed: MessageReader<WriteCompleted>,
    mut roots: Query<(&Root, &Scope, &Writing, &mut State, &mut Revision)>,
    states: Query<&OperationState>,
    installs: Query<&InstallRequest>,
    mut effects: WriteEffects,
    mut commands: Commands,
) {
    for completion in completed.read() {
        let ticket = &completion.ticket;
        let Ok((root, scope, writing, mut state, mut revision)) = roots.get_mut(ticket.owner)
        else {
            continue;
        };
        if root.0 != ticket.directory
            || scope.0 != *ticket
            || writing.ticket != *ticket
            || writing.operation != completion.operation
            || writing.revision != completion.revision
        {
            continue;
        }
        let result = if states
            .get(completion.operation)
            .is_ok_and(|state| *state == OperationState::Canceled)
        {
            Err("extension operation canceled".into())
        } else if *revision != writing.base {
            Err("extension revision conflict".into())
        } else {
            completion.result.clone()
        };
        match &result {
            Ok(()) => {
                state.succeed(completion.revision);
                *revision = Revision(completion.revision);
                effects.updates.write(Reconcile {
                    ticket: ticket.clone(),
                    snapshot: writing.snapshot.clone(),
                    revision: *revision,
                });
                commands.entity(ticket.owner).remove::<WriteError>();
                effects.changed.write(Changed(ticket.owner));
                if let Mutation::Remove(request) = &writing.request.mutation {
                    effects.cleanup.write(CleanupRequest {
                        ticket: ticket.clone(),
                        id: request.id.clone(),
                        profile: writing.request.profile.clone(),
                        package: !installs
                            .iter()
                            .any(|install| install.ticket == *ticket && install.id == request.id)
                            && !writing
                                .snapshot
                                .entries
                                .iter()
                                .any(|entry| entry.id == request.id),
                    });
                }
            }
            Err(error) => {
                if completion.result.is_ok() {
                    state.retry(completion.revision, error.clone());
                    commands.entity(ticket.owner).insert(Repair);
                } else if *revision == writing.base {
                    *state = writing.before.clone();
                } else {
                    state.retry(completion.revision, error.clone());
                }
                commands
                    .entity(ticket.owner)
                    .insert(WriteError(error.clone()));
            }
        }
        commands.trigger(OperationCompleted::from_result(
            completion.operation,
            &result,
        ));
        commands
            .entity(completion.operation)
            .remove::<(Request, Queued)>();
        commands.entity(ticket.owner).remove::<Writing>();
    }
}

fn reconcile(
    mut updates: MessageReader<Reconcile>,
    roots: Query<(&Root, &Scope)>,
    catalog: Catalog,
    packages: Query<
        (
            Entity,
            &Id,
            &ChildOf,
            &Revision,
            Has<Version>,
            Option<&Installing>,
        ),
        With<Package>,
    >,
    associations: Query<
        (
            Entity,
            &ProfileId,
            &ChildOf,
            Option<&Revision>,
            Has<Installed>,
        ),
        With<Association>,
    >,
    mut commands: Commands,
) {
    for update in updates.read() {
        if !roots.get(update.ticket.owner).is_ok_and(|(root, scope)| {
            root.0 == update.ticket.directory && scope.0 == update.ticket
        }) {
            continue;
        }
        let before = catalog.snapshot(update.ticket.owner).unwrap_or_default();
        let mut existing = HashMap::new();
        for (entity, id, parent, revision, committed, installing) in &packages {
            if parent.parent() == update.ticket.owner {
                existing.insert(
                    id.0.as_str(),
                    (entity, *revision, committed, installing.is_some()),
                );
            }
        }
        let mut retained = HashSet::new();
        for entry in &update.snapshot.entries {
            let previous = before
                .entries
                .iter()
                .find(|previous| previous.id == entry.id);
            let (entity, revision) = match existing.get(entry.id.as_str()) {
                Some((entity, revision, _, _)) => (
                    *entity,
                    if previous == Some(entry) {
                        *revision
                    } else {
                        update.revision
                    },
                ),
                None => (
                    commands
                        .spawn((Package, Id(entry.id.clone()), ChildOf(update.ticket.owner)))
                        .id(),
                    update.revision,
                ),
            };
            retained.insert(entity);
            commands.entity(entity).insert((
                Name::new(if entry.name.is_empty() {
                    entry.id.clone()
                } else {
                    entry.name.clone()
                }),
                Version(entry.version.clone()),
                Popup(entry.popup.clone()),
                Icon(entry.icon.clone()),
                Permissions(entry.permissions.clone()),
                OptionalPermissions(entry.optional_permissions.clone()),
                HostPermissions(entry.host_permissions.clone()),
                OptionalHostPermissions(entry.optional_host_permissions.clone()),
                SourceHash(entry.source_hash.clone()),
                PublicKey(entry.public_key_b64.clone()),
                revision,
            ));
            let mut profiles = HashMap::new();
            for (association, profile, parent, revision, committed) in &associations {
                if parent.parent() == entity {
                    profiles.insert(
                        profile.0.as_str(),
                        (
                            association,
                            revision.copied().unwrap_or_default(),
                            committed,
                        ),
                    );
                }
            }
            for (profile, enabled) in &entry.profile_enabled {
                let changed = previous.is_none_or(|previous| {
                    !previous.installed_for(profile)
                        || previous.enabled_for(profile) != *enabled
                        || previous.pinned_for(profile) != entry.pinned_for(profile)
                        || previous.grants_for(profile) != entry.grants_for(profile)
                });
                let (association, previous_revision, _) = match profiles.remove(profile.as_str()) {
                    Some(existing) => existing,
                    None => (
                        commands
                            .spawn((Association, ProfileId(profile.clone()), ChildOf(entity)))
                            .id(),
                        update.revision,
                        false,
                    ),
                };
                commands.entity(association).insert((
                    Installed,
                    Grants(entry.grants_for(profile)),
                    if changed {
                        update.revision
                    } else {
                        previous_revision
                    },
                ));
                if *enabled {
                    commands.entity(association).insert(Enabled);
                } else {
                    commands.entity(association).remove::<Enabled>();
                }
                if entry.pinned_for(profile) {
                    commands.entity(association).insert(Pinned);
                } else {
                    commands.entity(association).remove::<Pinned>();
                }
            }
            for (entity, _, committed) in profiles.values() {
                if *committed {
                    commands.entity(*entity).despawn();
                }
            }
        }
        for (entity, _, committed, installing) in existing.values() {
            if !retained.contains(entity) && *committed {
                if *installing {
                    commands.entity(*entity).remove::<(
                        Name,
                        Version,
                        Popup,
                        Icon,
                        Permissions,
                        OptionalPermissions,
                        HostPermissions,
                        OptionalHostPermissions,
                        SourceHash,
                        PublicKey,
                    )>();
                    for (association, _, parent, _, committed) in &associations {
                        if parent.parent() == *entity && committed {
                            commands.entity(association).despawn();
                        }
                    }
                } else {
                    commands.entity(*entity).despawn();
                }
            }
        }
    }
}

fn settle(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &OperationState, &InstallOwner), With<InstallRequest>>();
    let mut completed = Vec::new();
    for (operation, state, owner) in query.iter(world) {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            completed.push((operation, *owner));
        }
    }
    for (operation, owner) in completed {
        world
            .entity_mut(operation)
            .remove::<(InstallRequest, InstallOwner)>();
        world.entity_mut(operation).insert(ReleasedInstall(owner));
    }
}

#[derive(Component)]
struct ReleasedInstall(InstallOwner);

fn release(world: &mut World) {
    let mut query = world.query::<(Entity, &ReleasedInstall)>();
    let mut released = Vec::new();
    for (operation, owner) in query.iter(world) {
        released.push((operation, owner.0));
    }
    for (operation, owner) in released {
        world.entity_mut(operation).remove::<ReleasedInstall>();
        if world
            .get::<Installing>(owner.association)
            .is_some_and(|epoch| epoch.0 == operation)
        {
            world.entity_mut(owner.association).remove::<Installing>();
            if world.get::<Installed>(owner.association).is_none() {
                world.entity_mut(owner.association).despawn();
            }
        }
        if world
            .get::<Installing>(owner.package)
            .is_some_and(|epoch| epoch.0 == operation)
        {
            world.entity_mut(owner.package).remove::<Installing>();
        }
        if world.get::<Version>(owner.package).is_none()
            && world.get::<Installing>(owner.package).is_none()
            && let Ok(entity) = world.get_entity_mut(owner.package)
        {
            entity.despawn();
        }
    }
}

fn evidence(
    mut roots: Query<(Entity, &Scope, &mut State, &mut Revision, &PreparedEvidence), With<Loaded>>,
    mut packages: Query<(Entity, &Id, &ChildOf, &mut SourceHash), With<Package>>,
    associations: Query<(Entity, &ProfileId, &ChildOf), With<Association>>,
    mut changed: MessageWriter<Changed>,
    mut commands: Commands,
) {
    for (owner, _, mut state, mut revision, evidence) in &mut roots {
        let mut touched = false;
        for evidence in &evidence.0 {
            for (entity, id, parent, mut hash) in &mut packages {
                if parent.parent() != owner || id.0 != evidence.id {
                    continue;
                }
                if let Some(source_hash) = &evidence.source_hash
                    && hash.0.is_empty()
                {
                    hash.0 = source_hash.clone();
                    touched = true;
                }
                if let Some(error) = &evidence.error {
                    for (association, profile, parent) in &associations {
                        if parent.parent() == entity && profile.0 == evidence.profile {
                            commands.entity(association).remove::<Enabled>();
                            commands.entity(entity).insert(WriteError(error.clone()));
                            touched = true;
                        }
                    }
                }
            }
        }
        commands.entity(owner).remove::<PreparedEvidence>();
        if touched {
            *revision = revision.saturating_next();
            state.advance();
            changed.write(Changed(owner));
        }
    }
}

fn autosave(
    roots: Query<
        (&Scope, &State, Has<WriteError>, Has<Repair>, Has<AutoSave>),
        (With<Loaded>, Without<Writing>),
    >,
    requests: Query<&Request>,
    mut commands: Commands,
) {
    for (scope, state, error, repair, auto) in &roots {
        if (!auto && !repair)
            || !state.is_dirty()
            || (error && !repair)
            || requests.iter().any(|request| request.ticket == scope.0)
        {
            continue;
        }
        commands.spawn(
            Request {
                ticket: scope.0.clone(),
                profile: String::new(),
                mutation: Mutation::Save,
            }
            .bundle(),
        );
    }
}

fn queue(
    requests: Query<Entity, (With<Request>, Without<Queued>)>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    for entity in &requests {
        commands.entity(entity).insert(Queued(clock.order()));
    }
}

fn accept(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &Request, &Queued, &OperationState), With<Operation>>();
    let mut requests = Vec::new();
    for (entity, request, queued, state) in query.iter(world) {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            requests.push((queued.0, entity, request.clone()));
        }
    }
    requests.sort_by_key(|request| request.0);
    let mut catalog = SystemState::<Catalog>::new(world);
    for (_, operation, request) in requests {
        let owner = request.ticket.owner;
        if world.get::<Writing>(owner).is_some() {
            continue;
        }
        if let Mutation::Remove(remove) = &request.mutation {
            let mut pending = world.query_filtered::<(Entity, &ProfileId, &ChildOf), (With<Association>, Without<Installed>)>();
            let mut removed = Vec::new();
            for (entity, profile, parent) in pending.iter(world) {
                if profile.0 != request.profile {
                    continue;
                }
                let package = parent.parent();
                if world.get::<Id>(package).is_some_and(|id| id.0 == remove.id)
                    && world
                        .get::<ChildOf>(package)
                        .is_some_and(|parent| parent.parent() == owner)
                {
                    removed.push(entity);
                }
            }
            for entity in removed {
                world.entity_mut(entity).despawn();
            }
        }
        let snapshot = {
            let catalog = catalog.get(world).unwrap();
            if !catalog.ready(&request.ticket) {
                Err("extension store is unavailable".into())
            } else {
                catalog.snapshot(owner)
            }
        };
        let proposed = snapshot
            .clone()
            .and_then(|snapshot| request.propose(snapshot));
        let proposed = match proposed {
            Ok(proposed) => proposed,
            Err(error) => {
                world.trigger(OperationCompleted {
                    entity: operation,
                    result: Err(error),
                });
                world.entity_mut(operation).remove::<(Request, Queued)>();
                continue;
            }
        };
        let dirty = world
            .get::<State>(owner)
            .is_some_and(|state| state.is_dirty());
        if !dirty
            && snapshot
                .as_ref()
                .is_ok_and(|snapshot| *snapshot == proposed)
        {
            world.trigger(OperationCompleted {
                entity: operation,
                result: Ok(()),
            });
            world.entity_mut(operation).remove::<(Request, Queued)>();
            continue;
        }
        let base = *world.get::<Revision>(owner).unwrap();
        let before = world.get::<State>(owner).unwrap().clone();
        let revision = {
            let mut state = world.get_mut::<State>(owner).unwrap();
            state.advance();
            state.begin().unwrap()
        };
        world
            .entity_mut(operation)
            .insert((OperationState::Running, OperationTargetRevision(revision)));
        world.entity_mut(owner).remove::<Repair>();
        world.entity_mut(owner).insert(Writing {
            ticket: request.ticket.clone(),
            operation,
            base,
            revision,
            snapshot: proposed.clone(),
            request: request.clone(),
            before,
        });
        world.write_message(WriteTaskRequest {
            ticket: request.ticket,
            operation,
            revision,
            snapshot: proposed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::SystemState;
    use vmux_ecs::OperationState;

    const PACKAGE_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Fixture {
        app: App,
        root: Entity,
        ticket: Ticket,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(CorePlugin);
            let mut index = Index::default();
            let mut entry = ExtEntry {
                id: PACKAGE_ID.into(),
                name: "Example".into(),
                version: "1".into(),
                permissions: vec!["clipboardWrite".into()],
                ..Default::default()
            };
            entry.profile_enabled.insert("personal".into(), false);
            entry.profile_enabled.insert("work".into(), false);
            index.entries.push(entry);
            let root = app
                .world_mut()
                .spawn((Root("/extensions".into()), Seed(Ok(index))))
                .id();
            app.update();
            let mut state = SystemState::<Catalog>::new(app.world_mut());
            let ticket = state
                .get(app.world())
                .unwrap()
                .ticket(root)
                .unwrap()
                .clone();
            Self { app, root, ticket }
        }

        fn submit(&mut self, profile: &str, mutation: Mutation) -> Entity {
            let entity = self
                .app
                .world_mut()
                .spawn(
                    Request {
                        ticket: self.ticket.clone(),
                        profile: profile.into(),
                        mutation,
                    }
                    .bundle(),
                )
                .id();
            self.app.update();
            entity
        }

        fn write(&mut self) -> WriteTaskRequest {
            self.app
                .world_mut()
                .resource_mut::<Messages<WriteTaskRequest>>()
                .drain()
                .next()
                .unwrap()
        }

        fn finish(&mut self, request: &WriteTaskRequest, result: Result<(), String>) {
            self.app.world_mut().write_message(WriteCompleted {
                ticket: request.ticket.clone(),
                operation: request.operation,
                revision: request.revision,
                result,
            });
            self.app.update();
        }

        fn snapshot(&mut self) -> Index {
            let mut state = SystemState::<Catalog>::new(self.app.world_mut());
            state
                .get(self.app.world())
                .unwrap()
                .snapshot(self.root)
                .unwrap()
        }
    }

    #[test]
    fn enabling_requires_approval_and_commits_only_the_selected_profile_after_io() {
        let mut fixture = Fixture::new();
        let refused = fixture.submit(
            "personal",
            Mutation::Toggle(ExtToggleRequest {
                id: PACKAGE_ID.into(),
                enabled: true,
                approve_permissions: false,
            }),
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(refused),
            Some(&OperationState::Failed)
        );
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<WriteTaskRequest>>()
                .drain()
                .next()
                .is_none()
        );
        let accepted = fixture.submit(
            "personal",
            Mutation::Toggle(ExtToggleRequest {
                id: PACKAGE_ID.into(),
                enabled: true,
                approve_permissions: true,
            }),
        );
        let write = fixture.write();
        assert!(!fixture.snapshot().entries[0].enabled_for("personal"));
        fixture.finish(&write, Ok(()));
        let entry = &fixture.snapshot().entries[0];
        assert!(entry.enabled_for("personal"));
        assert!(!entry.enabled_for("work"));
        assert!(
            entry
                .grants_for("personal")
                .covers(&["clipboardWrite".into()], &[])
        );
        assert!(entry.grants_for("work").permissions.is_empty());
        assert_eq!(
            fixture.app.world().get::<OperationState>(accepted),
            Some(&OperationState::Succeeded)
        );
    }

    #[test]
    fn failed_atomic_write_preserves_committed_state_and_a_new_request_can_retry() {
        let mut fixture = Fixture::new();
        let operation = fixture.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Err("disk full".into()));
        assert!(!fixture.snapshot().entries[0].pinned_for("personal"));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<WriteError>(fixture.root)
                .unwrap()
                .0,
            "disk full"
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        fixture.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            }),
        );
        let retry = fixture.write();
        fixture.finish(&write, Ok(()));
        assert!(!fixture.snapshot().entries[0].pinned_for("personal"));
        fixture.finish(&retry, Ok(()));
        assert!(fixture.snapshot().entries[0].pinned_for("personal"));
        assert!(
            fixture
                .app
                .world()
                .get::<WriteError>(fixture.root)
                .is_none()
        );
    }

    #[test]
    fn removing_one_profile_retains_the_shared_package_and_other_associations() {
        let mut fixture = Fixture::new();
        fixture.submit(
            "personal",
            Mutation::Remove(ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.entries[0].installed_for("personal"));
        assert!(snapshot.entries[0].installed_for("work"));
        let cleanup = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .drain()
            .next()
            .unwrap();
        assert!(!cleanup.package);
        fixture.submit(
            "work",
            Mutation::Remove(ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        assert!(fixture.snapshot().entries.is_empty());
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<CleanupRequest>>()
                .drain()
                .next()
                .unwrap()
                .package
        );
    }

    #[test]
    fn package_update_narrows_grants_and_disables_a_profile_missing_new_permissions() {
        let mut fixture = Fixture::new();
        fixture.submit(
            "personal",
            Mutation::Toggle(ExtToggleRequest {
                id: PACKAGE_ID.into(),
                enabled: true,
                approve_permissions: true,
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        let mut update = fixture.snapshot().entries.remove(0);
        update.version = "2".into();
        update.permissions = vec!["tabs".into()];
        fixture.submit("personal", Mutation::Install(Box::new(update)));
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        let entry = &fixture.snapshot().entries[0];
        assert_eq!(entry.version, "2");
        assert!(!entry.enabled_for("personal"));
        assert!(entry.grants_for("personal").permissions.is_empty());
        assert!(entry.installed_for("work"));
    }

    #[test]
    fn replacing_a_root_cancels_pending_work_and_refuses_late_completion() {
        let mut fixture = Fixture::new();
        let operation = fixture.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            }),
        );
        let write = fixture.write();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(Root("/replacement".into()));
        fixture.app.update();
        fixture.finish(&write, Ok(()));
        assert!(fixture.app.world().get::<Loaded>(fixture.root).is_none());
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<Package>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn bootstrap_schedule_applies_runtime_evidence_without_running_application_startup() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(PreparedEvidence(vec![Evidence {
                id: PACKAGE_ID.into(),
                profile: "personal".into(),
                source_hash: Some("validated-source".into()),
                error: None,
            }]));
        fixture.app.world_mut().run_schedule(ExtensionSchedule);
        assert_eq!(
            fixture.snapshot().entries[0].source_hash,
            "validated-source"
        );
        assert!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .is_dirty()
        );
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<WriteTaskRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn a_replacement_install_invalidates_the_older_package_operation() {
        let mut fixture = Fixture::new();
        let older = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                InstallRequest {
                    ticket: fixture.ticket.clone(),
                    profile: "personal".into(),
                    id: PACKAGE_ID.into(),
                },
            ))
            .id();
        fixture.app.update();
        let newer = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                InstallRequest {
                    ticket: fixture.ticket.clone(),
                    profile: "personal".into(),
                    id: PACKAGE_ID.into(),
                },
            ))
            .id();
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(older),
            Some(&OperationState::Canceled)
        );
        assert!(fixture.app.world().get::<InstallOwner>(newer).is_some());
        assert_eq!(
            fixture.app.world().get::<OperationState>(newer),
            Some(&OperationState::Running)
        );
    }

    #[test]
    fn removing_a_provisional_association_cancels_install_without_publishing_metadata() {
        let mut fixture = Fixture::new();
        let id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                InstallRequest {
                    ticket: fixture.ticket.clone(),
                    profile: "personal".into(),
                    id: id.into(),
                },
            ))
            .id();
        fixture.app.update();
        let owner = *fixture.app.world().get::<InstallOwner>(operation).unwrap();
        assert!(
            !fixture
                .snapshot()
                .entries
                .iter()
                .any(|entry| entry.id == id)
        );
        fixture.submit(
            "personal",
            Mutation::Remove(ExtUninstallRequest { id: id.into() }),
        );
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
        assert!(fixture.app.world().get_entity(owner.association).is_err());
        assert!(fixture.app.world().get_entity(owner.package).is_err());
        assert!(
            !fixture
                .snapshot()
                .entries
                .iter()
                .any(|entry| entry.id == id)
        );
    }

    #[test]
    fn unrelated_metadata_commit_keeps_a_provisional_install_alive() {
        let mut fixture = Fixture::new();
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                InstallRequest {
                    ticket: fixture.ticket.clone(),
                    profile: "new-profile".into(),
                    id: PACKAGE_ID.into(),
                },
            ))
            .id();
        fixture.app.update();
        let owner = *fixture.app.world().get::<InstallOwner>(operation).unwrap();
        fixture.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        assert!(fixture.app.world().get_entity(owner.association).is_ok());
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Running)
        );
    }

    #[test]
    fn removing_the_last_committed_profile_preserves_another_profiles_pending_install() {
        let mut fixture = Fixture::new();
        fixture.submit(
            "personal",
            Mutation::Remove(ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        let mut replacement = fixture.snapshot().entries.remove(0);
        replacement.version = "2".into();
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                InstallRequest {
                    ticket: fixture.ticket.clone(),
                    profile: "new-profile".into(),
                    id: PACKAGE_ID.into(),
                },
            ))
            .id();
        fixture.app.update();
        fixture.submit(
            "work",
            Mutation::Remove(ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            }),
        );
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        fixture.app.update();
        assert!(fixture.snapshot().entries.is_empty());
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Running)
        );
        let cleanup = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .drain()
            .next()
            .unwrap();
        assert!(!cleanup.package);
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(Request {
                ticket: fixture.ticket.clone(),
                profile: "new-profile".into(),
                mutation: Mutation::Install(Box::new(replacement)),
            });
        fixture.app.update();
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.entries[0].version, "2");
        assert!(snapshot.entries[0].installed_for("new-profile"));
        assert!(!snapshot.entries[0].installed_for("work"));
        assert!(fixture.app.world().get::<InstallOwner>(operation).is_none());
    }

    #[test]
    fn canceled_successful_io_is_compensated_with_the_canonical_snapshot() {
        let mut fixture = Fixture::new();
        let before = fixture.snapshot();
        let operation = fixture.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            }),
        );
        let write = fixture.write();
        assert!(write.snapshot.entries[0].pinned_for("personal"));
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(OperationState::Canceled);
        fixture.finish(&write, Ok(()));
        assert_eq!(fixture.snapshot(), before);
        let repair = fixture.write();
        assert_ne!(repair.operation, operation);
        assert_eq!(repair.snapshot, before);
        fixture.finish(&repair, Ok(()));
        assert!(
            !fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .is_dirty()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<WriteError>(fixture.root)
                .is_none()
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
    }
}
