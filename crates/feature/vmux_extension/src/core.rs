use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::SystemParam;
use std::result::Result;
use vmux_api::extension::{ExtPinRequest, ExtToggleRequest, ExtUninstallRequest};
pub use vmux_ecs::store::State;
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget,
    OperationTargetRevision, Revision,
};

use crate::snapshot::{ExtEntry, ExtensionGrants, Index};

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
            .add_systems(Update, step.in_set(ExtensionSet::Step))
            .add_observer(start)
            .add_observer(toggle)
            .add_observer(pin)
            .add_observer(uninstall)
            .add_observer(install)
            .add_observer(grant)
            .add_observer(save)
            .add_observer(admit)
            .add_observer(discard)
            .add_observer(retire);
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

#[derive(Component)]
struct Pending;

impl Pending {
    fn parts(ticket: Ticket, profile: String) -> impl Bundle {
        (Self, Scope(ticket), ProfileId(profile))
    }

    fn bundle(ticket: Ticket, profile: String, kind: &str) -> impl Bundle {
        (
            Operation::bundle(kind).unwrap(),
            OperationTarget::new(ticket.owner),
            Self::parts(ticket, profile),
        )
    }
}

#[derive(Component)]
pub struct Toggle {
    enabled: bool,
    approve_permissions: bool,
}

impl Toggle {
    pub fn bundle(ticket: Ticket, profile: String, request: ExtToggleRequest) -> impl Bundle {
        (
            Pending::bundle(ticket, profile, "extension.toggle"),
            Self {
                enabled: request.enabled,
                approve_permissions: request.approve_permissions,
            },
            Id(request.id),
        )
    }
}

#[derive(Component)]
pub struct Pin(bool);

impl Pin {
    pub fn bundle(ticket: Ticket, profile: String, request: ExtPinRequest) -> impl Bundle {
        (
            Pending::bundle(ticket, profile, "extension.pin"),
            Self(request.pinned),
            Id(request.id),
        )
    }
}

#[derive(Component)]
pub struct Uninstall;

impl Uninstall {
    pub fn bundle(ticket: Ticket, profile: String, request: ExtUninstallRequest) -> impl Bundle {
        (
            Pending::bundle(ticket, profile, "extension.remove"),
            Self,
            Id(request.id),
        )
    }
}

#[derive(Component)]
pub struct Grant;

impl Grant {
    pub fn bundle(
        ticket: Ticket,
        profile: String,
        id: String,
        grants: ExtensionGrants,
    ) -> impl Bundle {
        (
            Pending::bundle(ticket, profile, "extension.grant"),
            Self,
            Id(id),
            Grants(grants),
        )
    }
}

#[derive(Component)]
pub struct Save;

impl Save {
    pub fn bundle(ticket: Ticket) -> impl Bundle {
        (
            Pending::bundle(ticket, String::new(), "extension.save"),
            Self,
        )
    }
}

#[derive(Component)]
pub struct Install;

impl Install {
    pub fn bundle(ticket: Ticket, profile: String, entry: ExtEntry) -> impl Bundle {
        (
            Operation::bundle("extension.install").unwrap(),
            OperationTarget::new(ticket.owner),
            Self::prepared(ticket, profile, entry),
        )
    }

    pub(crate) fn prepared(ticket: Ticket, profile: String, entry: ExtEntry) -> impl Bundle {
        (
            Pending::parts(ticket, profile),
            Self,
            Id(entry.id),
            Name::new(entry.name),
            Version(entry.version),
            Popup(entry.popup),
            Icon(entry.icon),
            Permissions(entry.permissions),
            OptionalPermissions(entry.optional_permissions),
            HostPermissions(entry.host_permissions),
            OptionalHostPermissions(entry.optional_host_permissions),
            SourceHash(entry.source_hash),
            PublicKey(entry.public_key_b64),
        )
    }
}

#[derive(Component, Debug)]
pub struct PreparedEvidence(pub Vec<Evidence>);

#[derive(Debug, Default)]
pub struct Evidence {
    pub id: String,
    pub profile: String,
    pub source_hash: Option<String>,
    pub declared_permissions: Option<Vec<String>>,
    pub optional_permissions: Option<Vec<String>>,
    pub host_permissions: Option<Vec<String>>,
    pub optional_host_permissions: Option<Vec<String>>,
    pub grant_enabled_profiles: bool,
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

    pub fn project_ui_state(
        index: &Index,
        profile: &str,
        loaded: &[String],
    ) -> vmux_api::extension::ExtensionsUiState {
        let mut extensions = Vec::new();
        let mut enabled_ids = Vec::new();
        for entry in &index.entries {
            if entry.enabled_for(profile) {
                enabled_ids.push(entry.id.clone());
            }
            if !entry.installed_for(profile) {
                continue;
            }
            let enabled = entry.enabled_for(profile);
            extensions.push(vmux_api::extension::ExtRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                icon: entry.icon.clone(),
                popup: entry.popup.clone(),
                enabled,
                pinned: entry.pinned_for(profile),
                needs_approval: !entry
                    .grants_for(profile)
                    .covers(&entry.permissions, &entry.host_permissions),
                required_permissions: entry.permissions.clone(),
                required_host_permissions: entry.host_permissions.clone(),
                status: if enabled {
                    vmux_api::extension::ExtStatus::Installed
                } else {
                    vmux_api::extension::ExtStatus::Disabled
                },
            });
        }
        enabled_ids.sort();
        let mut loaded_ids = loaded.to_vec();
        loaded_ids.sort();
        let visible = extensions.clone();
        vmux_api::extension::ExtensionsUiState {
            loaded: true,
            extensions,
            visible,
            query: String::new(),
            installing: Vec::new(),
            pending: enabled_ids != loaded_ids,
        }
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
enum Repair {
    Required,
    Suspended,
}

#[derive(Component)]
struct Writing {
    ticket: Ticket,
    operation: Entity,
    base: Revision,
    revision: u64,
    snapshot: Index,
    cleanup: Option<(String, String)>,
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
        Has<Repair>,
    )>,
    packages: Query<(Entity, &ChildOf), With<Package>>,
    mut clock: ResMut<Clock>,
    mut loads: MessageWriter<LoadRequest>,
    mut completed: MessageWriter<LoadCompleted>,
    mut commands: Commands,
) {
    for (owner, root, scope, seed, writing, pending_repair) in &roots {
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
        let repair = writing.is_some() || pending_repair;
        if repair {
            state.advance();
        }
        commands
            .entity(owner)
            .remove::<(Loaded, Writing, WriteError, Seed, Repair)>()
            .insert((Scope(ticket.clone()), state, Revision::default()));
        if repair {
            commands.entity(owner).insert(Repair::Required);
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

#[derive(EntityEvent)]
struct Starting {
    entity: Entity,
}

fn begin(
    requests: Query<
        (Entity, &OperationState),
        (With<Operation>, With<InstallRequest>, Without<InstallOwner>),
    >,
    mut commands: Commands,
) {
    for (entity, state) in &requests {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            commands.trigger(Starting { entity });
        }
    }
}

fn start(
    event: On<Starting>,
    requests: Query<(&InstallRequest, &OperationState), (With<Operation>, Without<InstallOwner>)>,
    roots: Query<(&Root, &Scope, Has<Loaded>)>,
    packages: Query<(Entity, &Id, &ChildOf), With<Package>>,
    associations: Query<(Entity, &ProfileId, &ChildOf), With<Association>>,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok((request, state)) = requests.get(operation) else {
        return;
    };
    if !matches!(state, OperationState::Pending | OperationState::Running) {
        return;
    }
    let root = request.ticket.owner;
    if !roots.get(root).is_ok_and(|(root, scope, loaded)| {
        root.0 == request.ticket.directory && scope.0 == request.ticket && loaded
    }) || request.id.len() != 32
        || !request.id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
    {
        commands.trigger(OperationCompleted {
            entity: operation,
            result: Err("invalid extension installation context".into()),
        });
        commands.entity(operation).remove::<InstallRequest>();
        return;
    }
    let mut found = None;
    for (entity, id, parent) in &packages {
        if parent.parent() == root && id.0 == request.id {
            found = Some(entity);
            break;
        }
    }
    let package = match found {
        Some(entity) => entity,
        None => commands
            .spawn((Package, Id(request.id.clone()), ChildOf(root)))
            .id(),
    };
    let mut found = None;
    for (entity, profile, parent) in &associations {
        if parent.parent() == package && profile.0 == request.profile {
            found = Some(entity);
            break;
        }
    }
    let association = match found {
        Some(entity) => entity,
        None => commands
            .spawn((
                Association,
                ProfileId(request.profile.clone()),
                Grants::default(),
                ChildOf(package),
            ))
            .id(),
    };
    commands.entity(package).insert(Installing(operation));
    commands.entity(association).insert(Installing(operation));
    commands.entity(operation).insert(InstallOwner {
        package,
        association,
    });
}

fn cancel(
    mut operations: Query<
        (
            Entity,
            Option<&Scope>,
            Option<&InstallRequest>,
            Option<&InstallOwner>,
            &mut OperationState,
        ),
        (With<Operation>, Or<(With<Pending>, With<InstallRequest>)>),
    >,
    roots: Query<(&Root, &Scope)>,
    installing: Query<&Installing>,
    mut commands: Commands,
) {
    for (operation, request, install, owner, mut state) in &mut operations {
        let ticket = match (request, install) {
            (Some(scope), _) => &scope.0,
            (_, Some(install)) => &install.ticket,
            _ => continue,
        };
        let current = roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.directory && scope.0 == *ticket);
        let owned = owner.is_none_or(|owner| {
            installing
                .get(owner.package)
                .is_ok_and(|epoch| epoch.0 == operation)
                && installing
                    .get(owner.association)
                    .is_ok_and(|epoch| epoch.0 == operation)
        });
        if current && owned {
            continue;
        }
        if matches!(*state, OperationState::Pending | OperationState::Running) {
            *state = OperationState::Canceled;
        }
        commands
            .entity(operation)
            .remove::<(Pending, Queued, InstallRequest, InstallOwner)>();
        if let Some(owner) = owner {
            commands.entity(operation).insert(ReleasedInstall(*owner));
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
                commands
                    .entity(ticket.owner)
                    .remove::<(WriteError, Repair)>();
                effects.changed.write(Changed(ticket.owner));
                if let Some((id, profile)) = &writing.cleanup {
                    effects.cleanup.write(CleanupRequest {
                        ticket: ticket.clone(),
                        id: id.clone(),
                        profile: profile.clone(),
                        package: !installs
                            .iter()
                            .any(|install| install.ticket == *ticket && install.id == *id)
                            && !writing.snapshot.entries.iter().any(|entry| entry.id == *id),
                    });
                }
            }
            Err(error) => {
                if completion.result.is_ok() {
                    state.retry(completion.revision, error.clone());
                    commands.entity(ticket.owner).insert(Repair::Required);
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
            .remove::<(Pending, Queued)>();
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

fn settle(
    operations: Query<(Entity, &OperationState, &InstallOwner), With<InstallRequest>>,
    mut commands: Commands,
) {
    for (operation, state, owner) in &operations {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        commands
            .entity(operation)
            .remove::<(InstallRequest, InstallOwner)>();
        commands.entity(operation).insert(ReleasedInstall(*owner));
    }
}

#[derive(Component)]
struct ReleasedInstall(InstallOwner);

#[derive(EntityEvent)]
struct Releasing {
    entity: Entity,
}

fn release(released: Query<Entity, With<ReleasedInstall>>, mut commands: Commands) {
    for entity in &released {
        commands.trigger(Releasing { entity });
    }
}

fn retire(
    event: On<Releasing>,
    released: Query<&ReleasedInstall>,
    entities: Query<(Option<&Installing>, Has<Installed>, Has<Version>)>,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok(released) = released.get(operation) else {
        return;
    };
    let owner = released.0;
    commands.entity(operation).remove::<ReleasedInstall>();
    if let Ok((Some(epoch), installed, _)) = entities.get(owner.association)
        && epoch.0 == operation
    {
        commands.entity(owner.association).remove::<Installing>();
        if !installed {
            commands.entity(owner.association).despawn();
        }
    }
    let Ok((epoch, _, committed)) = entities.get(owner.package) else {
        return;
    };
    let owned = epoch.is_some_and(|epoch| epoch.0 == operation);
    if owned {
        commands.entity(owner.package).remove::<Installing>();
    }
    if !committed && (epoch.is_none() || owned) {
        commands.entity(owner.package).despawn();
    }
}

fn evidence(
    mut roots: Query<(Entity, &Scope, &mut State, &mut Revision, &PreparedEvidence), With<Loaded>>,
    mut packages: Query<
        (
            Entity,
            &Id,
            &ChildOf,
            &mut SourceHash,
            &mut Permissions,
            &mut OptionalPermissions,
            &mut HostPermissions,
            &mut OptionalHostPermissions,
        ),
        With<Package>,
    >,
    mut associations: Query<
        (Entity, &ProfileId, &ChildOf, Has<Enabled>, &mut Grants),
        With<Association>,
    >,
    mut changed: MessageWriter<Changed>,
    mut commands: Commands,
) {
    for (owner, _, mut state, mut revision, evidence) in &mut roots {
        let mut touched = false;
        for evidence in &evidence.0 {
            for (
                entity,
                id,
                parent,
                mut hash,
                mut permissions,
                mut optional_permissions,
                mut host_permissions,
                mut optional_host_permissions,
            ) in &mut packages
            {
                if parent.parent() != owner || id.0 != evidence.id {
                    continue;
                }
                if let Some(source_hash) = &evidence.source_hash
                    && hash.0.is_empty()
                {
                    hash.0 = source_hash.clone();
                    touched = true;
                }
                if let Some(declared) = &evidence.declared_permissions {
                    permissions.0.clone_from(declared);
                    touched = true;
                }
                if let Some(optional) = &evidence.optional_permissions {
                    optional_permissions.0.clone_from(optional);
                    touched = true;
                }
                if let Some(hosts) = &evidence.host_permissions {
                    host_permissions.0.clone_from(hosts);
                    touched = true;
                }
                if let Some(optional_hosts) = &evidence.optional_host_permissions {
                    optional_host_permissions.0.clone_from(optional_hosts);
                    touched = true;
                }
                if evidence.grant_enabled_profiles {
                    for (_, _, parent, enabled, mut grants) in &mut associations {
                        if parent.parent() == entity && enabled {
                            grants.0 = ExtensionGrants {
                                permissions: permissions.0.clone(),
                                host_permissions: host_permissions.0.clone(),
                            };
                            touched = true;
                        }
                    }
                }
                if let Some(error) = &evidence.error {
                    for (association, profile, parent, _, _) in &associations {
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
        (
            &Scope,
            &State,
            Has<WriteError>,
            Option<&Repair>,
            Has<AutoSave>,
        ),
        (With<Loaded>, Without<Writing>),
    >,
    requests: Query<&Scope, With<Pending>>,
    mut commands: Commands,
) {
    for (scope, state, error, repair, auto) in &roots {
        if (!auto && repair.is_none())
            || !state.is_dirty()
            || (error && !matches!(repair, Some(Repair::Required)))
            || requests.iter().any(|request| request.0 == scope.0)
        {
            continue;
        }
        commands.spawn(Save::bundle(scope.0.clone()));
    }
}

fn queue(
    requests: Query<Entity, (With<Pending>, Without<Queued>)>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    for entity in &requests {
        commands.entity(entity).insert(Queued(clock.order()));
    }
}

#[derive(EntityEvent)]
struct Accepting {
    entity: Entity,
}

fn accept(
    requests: Query<(Entity, &Queued, &OperationState), (With<Pending>, With<Operation>)>,
    mut commands: Commands,
) {
    let mut ordered = Vec::new();
    for (entity, queued, state) in &requests {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            ordered.push((queued.0, entity));
        }
    }
    ordered.sort_unstable_by_key(|request| request.0);
    for (_, entity) in ordered {
        commands.trigger(Accepting { entity });
    }
}

#[derive(SystemParam)]
struct Admission<'w, 's> {
    writing: Query<'w, 's, (), With<Writing>>,
    store: ParamSet<
        'w,
        's,
        (
            Catalog<'w, 's>,
            Query<'w, 's, (&'static Revision, &'static mut State, Has<Repair>)>,
        ),
    >,
}

#[derive(SystemParam)]
struct Provisional<'w, 's> {
    packages: Query<'w, 's, (&'static Id, &'static ChildOf)>,
    associations: Query<
        'w,
        's,
        (Entity, &'static ProfileId, &'static ChildOf),
        (With<Association>, Without<Installed>),
    >,
}

#[derive(SystemParam)]
struct Proposals<'w, 's> {
    requests: Query<
        'w,
        's,
        (&'static Scope, &'static ProfileId, &'static OperationState),
        (With<Pending>, With<Queued>),
    >,
    writing: Query<'w, 's, (), With<Writing>>,
    catalog: Catalog<'w, 's>,
}

impl Proposals<'_, '_> {
    fn read(&self, operation: Entity) -> Option<(Ticket, String, Revision, Result<Index, String>)> {
        let (scope, profile, state) = self.requests.get(operation).ok()?;
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || self.writing.contains(scope.0.owner)
        {
            return None;
        }
        let result = if self.catalog.ready(&scope.0) {
            self.catalog.snapshot(scope.0.owner)
        } else {
            Err("extension store is unavailable".into())
        };
        Some((
            scope.0.clone(),
            profile.0.clone(),
            self.catalog.revision(scope.0.owner).unwrap_or_default(),
            result,
        ))
    }
}

#[derive(EntityEvent)]
struct Prepared {
    entity: Entity,
    base: Revision,
    before: Index,
    result: Result<Index, String>,
}

impl Prepared {
    fn rejected(entity: Entity, base: Revision, error: String) -> Self {
        Self {
            entity,
            base,
            before: Index::default(),
            result: Err(error),
        }
    }
}

fn toggle(
    event: On<Accepting>,
    inputs: Query<(&Toggle, &Id)>,
    proposals: Proposals,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok((request, id)) = inputs.get(operation) else {
        return;
    };
    let Some((_ticket, profile, base, snapshot)) = proposals.read(operation) else {
        return;
    };

    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(Prepared::rejected(operation, base, error));
            return;
        }
    };
    let mut next = snapshot.clone();
    let Some(entry) = next.entries.iter_mut().find(|entry| entry.id == id.0) else {
        commands.trigger(Prepared::rejected(
            operation,
            base,
            "extension association not found".into(),
        ));
        return;
    };
    if !entry.installed_for(&profile) {
        commands.trigger(Prepared::rejected(
            operation,
            base,
            "extension association not found".into(),
        ));
        return;
    }
    if request.enabled
        && !entry
            .grants_for(&profile)
            .covers(&entry.permissions, &entry.host_permissions)
    {
        if !request.approve_permissions {
            commands.trigger(Prepared::rejected(
                operation,
                base,
                "extension permissions require approval".into(),
            ));
            return;
        }
        entry.approved_grants.insert(
            profile.clone(),
            ExtensionGrants {
                permissions: entry.permissions.clone(),
                host_permissions: entry.host_permissions.clone(),
            },
        );
    }
    entry.profile_enabled.insert(profile, request.enabled);
    let result = next.validate().map(|_| next);
    commands.trigger(Prepared {
        entity: operation,
        base,
        before: snapshot,
        result,
    });
}

fn pin(
    event: On<Accepting>,
    inputs: Query<(&Pin, &Id)>,
    proposals: Proposals,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok((request, id)) = inputs.get(operation) else {
        return;
    };
    let Some((_ticket, profile, base, snapshot)) = proposals.read(operation) else {
        return;
    };

    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(Prepared::rejected(operation, base, error));
            return;
        }
    };
    let mut next = snapshot.clone();
    let Some(entry) = next.entries.iter_mut().find(|entry| entry.id == id.0) else {
        commands.trigger(Prepared::rejected(
            operation,
            base,
            "extension package not found".into(),
        ));
        return;
    };
    if !entry.installed_for(&profile) {
        commands.trigger(Prepared::rejected(
            operation,
            base,
            "extension association not found".into(),
        ));
        return;
    }
    if entry.pinned_for(&profile) != request.0 {
        entry.profile_pinned.insert(profile, request.0);
    }
    let result = next.validate().map(|_| next);
    commands.trigger(Prepared {
        entity: operation,
        base,
        before: snapshot,
        result,
    });
}

fn uninstall(
    event: On<Accepting>,
    inputs: Query<&Id, With<Uninstall>>,
    proposals: Proposals,
    provisional: Provisional,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok(id) = inputs.get(operation) else {
        return;
    };
    let Some((ticket, profile, base, snapshot)) = proposals.read(operation) else {
        return;
    };
    for (entity, found, parent) in &provisional.associations {
        if found.0 == profile
            && provisional
                .packages
                .get(parent.parent())
                .is_ok_and(|(found, parent)| found.0 == id.0 && parent.parent() == ticket.owner)
        {
            commands.entity(entity).despawn();
        }
    }
    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(Prepared::rejected(operation, base, error));
            return;
        }
    };
    let mut next = snapshot.clone();
    if let Some(entry) = next.entries.iter_mut().find(|entry| entry.id == id.0) {
        entry.profile_enabled.remove(&profile);
        entry.profile_pinned.remove(&profile);
        entry.approved_grants.remove(&profile);
        if entry.profile_enabled.is_empty() {
            next.entries.retain(|entry| entry.id != id.0);
        }
    }
    let result = next.validate().map(|_| next);
    commands.trigger(Prepared {
        entity: operation,
        base,
        before: snapshot,
        result,
    });
}

fn grant(
    event: On<Accepting>,
    inputs: Query<(&Id, &Grants), With<Grant>>,
    proposals: Proposals,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok((id, grants)) = inputs.get(operation) else {
        return;
    };
    let Some((_ticket, profile, base, snapshot)) = proposals.read(operation) else {
        return;
    };

    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(Prepared::rejected(operation, base, error));
            return;
        }
    };
    let mut next = snapshot.clone();
    let Some(entry) = next.entries.iter_mut().find(|entry| entry.id == id.0) else {
        commands.trigger(Prepared::rejected(
            operation,
            base,
            "extension package not found".into(),
        ));
        return;
    };
    if !entry.installed_for(&profile) {
        commands.trigger(Prepared::rejected(
            operation,
            base,
            "extension association not found".into(),
        ));
        return;
    }
    for permission in &grants.0.permissions {
        if !entry.permissions.contains(permission)
            && !entry.optional_permissions.contains(permission)
        {
            commands.trigger(Prepared::rejected(
                operation,
                base,
                "permission is not declared by the extension".into(),
            ));
            return;
        }
    }
    for permission in &grants.0.host_permissions {
        if !entry.host_permissions.contains(permission)
            && !entry.optional_host_permissions.contains(permission)
        {
            commands.trigger(Prepared::rejected(
                operation,
                base,
                "host permission is not declared by the extension".into(),
            ));
            return;
        }
    }
    entry
        .approved_grants
        .insert(profile.clone(), grants.0.clone());
    if !grants.0.covers(&entry.permissions, &entry.host_permissions) {
        entry.profile_enabled.insert(profile, false);
    }
    let result = next.validate().map(|_| next);
    commands.trigger(Prepared {
        entity: operation,
        base,
        before: snapshot,
        result,
    });
}

fn save(
    event: On<Accepting>,
    inputs: Query<(), With<Save>>,
    proposals: Proposals,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok(()) = inputs.get(operation) else {
        return;
    };
    let Some((_ticket, _profile, base, snapshot)) = proposals.read(operation) else {
        return;
    };

    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(Prepared::rejected(operation, base, error));
            return;
        }
    };
    let next = snapshot.clone();

    let result = next.validate().map(|_| next);
    commands.trigger(Prepared {
        entity: operation,
        base,
        before: snapshot,
        result,
    });
}

fn install(
    event: On<Accepting>,
    inputs: Query<
        (
            &Id,
            &Name,
            &Version,
            &Popup,
            &Icon,
            &Permissions,
            &OptionalPermissions,
            &HostPermissions,
            &OptionalHostPermissions,
            &SourceHash,
            &PublicKey,
        ),
        With<Install>,
    >,
    proposals: Proposals,
    mut commands: Commands,
) {
    let operation = event.entity;
    let Ok((
        id,
        name,
        version,
        popup,
        icon,
        permissions,
        optional,
        hosts,
        optional_hosts,
        hash,
        public_key,
    )) = inputs.get(operation)
    else {
        return;
    };
    let Some((_ticket, profile, base, snapshot)) = proposals.read(operation) else {
        return;
    };

    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(Prepared::rejected(operation, base, error));
            return;
        }
    };
    let mut next = snapshot.clone();
    let mut entry = ExtEntry {
        id: id.0.clone(),
        name: name.as_str().into(),
        version: version.0.clone(),
        popup: popup.0.clone(),
        icon: icon.0.clone(),
        permissions: permissions.0.clone(),
        optional_permissions: optional.0.clone(),
        host_permissions: hosts.0.clone(),
        optional_host_permissions: optional_hosts.0.clone(),
        source_hash: hash.0.clone(),
        public_key_b64: public_key.0.clone(),
        ..Default::default()
    };
    if let Some(previous) = next.entries.iter().find(|previous| previous.id == id.0) {
        entry.profile_enabled.clone_from(&previous.profile_enabled);
        entry.profile_pinned.clone_from(&previous.profile_pinned);
        entry.approved_grants.clone_from(&previous.approved_grants);
    }
    entry.profile_enabled.entry(profile).or_insert(false);
    for (profile, grants) in &mut entry.approved_grants {
        grants.retain_declared(
            &entry.permissions,
            &entry.optional_permissions,
            &entry.host_permissions,
            &entry.optional_host_permissions,
        );
        if !grants.covers(&entry.permissions, &entry.host_permissions) {
            entry.profile_enabled.insert(profile.clone(), false);
        }
    }
    for (profile, enabled) in &mut entry.profile_enabled {
        if !entry
            .approved_grants
            .get(profile)
            .cloned()
            .unwrap_or_default()
            .covers(&entry.permissions, &entry.host_permissions)
        {
            *enabled = false;
        }
    }
    if let Some(previous) = next.entries.iter_mut().find(|previous| previous.id == id.0) {
        *previous = entry;
    } else {
        next.entries.push(entry);
    }
    let result = next.validate().map(|_| next);
    commands.trigger(Prepared {
        entity: operation,
        base,
        before: snapshot,
        result,
    });
}

fn admit(
    mut event: On<Prepared>,
    requests: Query<
        (
            &Scope,
            &ProfileId,
            &OperationState,
            Option<&Id>,
            Has<Uninstall>,
        ),
        (With<Pending>, With<Queued>),
    >,
    mut admission: Admission,
    mut writes: MessageWriter<WriteTaskRequest>,
    mut commands: Commands,
) {
    let event = event.event_mut();
    let operation = event.entity;
    let Ok((scope, profile, status, id, remove)) = requests.get(operation) else {
        return;
    };
    let owner = scope.0.owner;
    if !matches!(status, OperationState::Pending | OperationState::Running)
        || admission.writing.contains(owner)
    {
        return;
    }
    let proposed =
        match std::mem::replace(&mut event.result, Err("extension proposal consumed".into())) {
            Ok(proposed) => proposed,
            Err(error) => {
                commands.trigger(OperationCompleted {
                    entity: operation,
                    result: Err(error),
                });
                commands.entity(operation).remove::<(Pending, Queued)>();
                return;
            }
        };
    let catalog = admission.store.p0();
    if !catalog.ready(&scope.0) || catalog.revision(owner) != Some(event.base) {
        commands.trigger(OperationCompleted {
            entity: operation,
            result: Err("extension revision conflict".into()),
        });
        commands.entity(operation).remove::<(Pending, Queued)>();
        return;
    }
    let mut states = admission.store.p1();
    let (base, mut state, repair) = states.get_mut(owner).unwrap();
    if !state.is_dirty() && event.before == proposed {
        commands.trigger(OperationCompleted {
            entity: operation,
            result: Ok(()),
        });
        commands.entity(operation).remove::<(Pending, Queued)>();
        return;
    }
    let before = state.clone();
    state.advance();
    let revision = state.begin().unwrap();
    commands
        .entity(operation)
        .insert((OperationState::Running, OperationTargetRevision(revision)));
    if repair {
        commands.entity(owner).insert(Repair::Suspended);
    }
    let cleanup = if remove {
        id.map(|id| (id.0.clone(), profile.0.clone()))
    } else {
        None
    };
    commands.entity(owner).insert(Writing {
        ticket: scope.0.clone(),
        operation,
        base: *base,
        revision,
        snapshot: proposed.clone(),
        cleanup,
        before,
    });
    writes.write(WriteTaskRequest {
        ticket: scope.0.clone(),
        operation,
        revision,
        snapshot: proposed,
    });
}

fn discard(event: On<Remove, Pending>, mut commands: Commands) {
    commands.entity(event.entity).remove::<(
        Scope,
        ProfileId,
        Toggle,
        Pin,
        Uninstall,
        Install,
        Grant,
        Save,
        Id,
    )>();
    commands.entity(event.entity).remove::<(
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

        fn submit<B: Bundle>(&mut self, request: B) -> Entity {
            let entity = self.app.world_mut().spawn(request).id();
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
        let refused = fixture.submit(Toggle::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtToggleRequest {
                id: PACKAGE_ID.into(),
                enabled: true,
                approve_permissions: false,
            },
        ));
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
        let accepted = fixture.submit(Toggle::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtToggleRequest {
                id: PACKAGE_ID.into(),
                enabled: true,
                approve_permissions: true,
            },
        ));
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
        let operation = fixture.submit(Pin::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            },
        ));
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
        fixture.submit(Pin::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            },
        ));
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
        fixture.submit(Uninstall::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            },
        ));
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
        fixture.submit(Uninstall::bundle(
            fixture.ticket.clone(),
            "work".into(),
            ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            },
        ));
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
        fixture.submit(Toggle::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtToggleRequest {
                id: PACKAGE_ID.into(),
                enabled: true,
                approve_permissions: true,
            },
        ));
        let write = fixture.write();
        fixture.finish(&write, Ok(()));
        let mut update = fixture.snapshot().entries.remove(0);
        update.version = "2".into();
        update.permissions = vec!["tabs".into()];
        fixture.submit(Install::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            update,
        ));
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
        let operation = fixture.submit(Pin::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            },
        ));
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
                ..Default::default()
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
    fn queued_mutations_wait_for_commit_without_blocking_other_roots() {
        let mut fixture = Fixture::new();
        let index = fixture.snapshot();
        let other = fixture
            .app
            .world_mut()
            .spawn((Root("/other-extensions".into()), Seed(Ok(index))))
            .id();
        fixture.app.update();
        let mut catalog = SystemState::<Catalog>::new(fixture.app.world_mut());
        let other_ticket = catalog
            .get(fixture.app.world())
            .unwrap()
            .ticket(other)
            .unwrap()
            .clone();
        let first = fixture
            .app
            .world_mut()
            .spawn(Pin::bundle(
                fixture.ticket.clone(),
                "personal".into(),
                ExtPinRequest {
                    id: PACKAGE_ID.into(),
                    pinned: true,
                },
            ))
            .id();
        let second = fixture
            .app
            .world_mut()
            .spawn(Toggle::bundle(
                fixture.ticket.clone(),
                "personal".into(),
                ExtToggleRequest {
                    id: PACKAGE_ID.into(),
                    enabled: true,
                    approve_permissions: true,
                },
            ))
            .id();
        let independent = fixture
            .app
            .world_mut()
            .spawn(Pin::bundle(
                other_ticket,
                "personal".into(),
                ExtPinRequest {
                    id: PACKAGE_ID.into(),
                    pinned: true,
                },
            ))
            .id();
        let operations = [first, second, independent];
        fixture.app.update();
        let writes: Vec<_> = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<WriteTaskRequest>>()
            .drain()
            .collect();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].operation, operations[0]);
        assert_eq!(writes[1].operation, operations[2]);
        assert!(!fixture.snapshot().entries[0].pinned_for("personal"));
        assert!(!fixture.snapshot().entries[0].enabled_for("personal"));
        fixture.finish(&writes[0], Ok(()));
        let next = fixture.write();
        assert_eq!(next.operation, operations[1]);
        assert!(fixture.snapshot().entries[0].pinned_for("personal"));
        assert!(!fixture.snapshot().entries[0].enabled_for("personal"));
        fixture.finish(&next, Ok(()));
        let committed = fixture.snapshot();
        assert!(committed.entries[0].pinned_for("personal"));
        assert!(committed.entries[0].enabled_for("personal"));
    }

    #[test]
    fn simultaneous_provisional_installs_share_one_package_and_release_old_epochs() {
        let mut fixture = Fixture::new();
        let id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let mut operations = Vec::new();
        for profile in ["personal", "work"] {
            operations.push(
                fixture
                    .app
                    .world_mut()
                    .spawn((
                        Operation::bundle("extension.install").unwrap().running(),
                        InstallRequest {
                            ticket: fixture.ticket.clone(),
                            profile: profile.into(),
                            id: id.into(),
                        },
                    ))
                    .id(),
            );
        }
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(operations[0]),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(operations[1]),
            Some(&OperationState::Running)
        );
        let packages: Vec<_> = fixture
            .app
            .world_mut()
            .query_filtered::<(Entity, &Id), With<Package>>()
            .iter(fixture.app.world())
            .filter_map(|(entity, found)| (found.0 == id).then_some(entity))
            .collect();
        assert_eq!(packages.len(), 1);
        let package = packages[0];
        let profiles: Vec<_> = fixture
            .app
            .world_mut()
            .query_filtered::<(&ProfileId, &ChildOf), With<Association>>()
            .iter(fixture.app.world())
            .filter_map(|(profile, parent)| {
                (parent.parent() == package).then_some(profile.0.clone())
            })
            .collect();
        assert_eq!(profiles, ["work"]);
        assert!(
            fixture
                .snapshot()
                .entries
                .iter()
                .all(|entry| entry.id != id)
        );
        fixture
            .app
            .world_mut()
            .entity_mut(operations[1])
            .insert(OperationState::Canceled);
        fixture.app.update();
        assert!(fixture.app.world().get_entity(package).is_err());
        assert!(
            fixture
                .snapshot()
                .entries
                .iter()
                .all(|entry| entry.id != id)
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
        fixture.submit(Uninstall::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtUninstallRequest { id: id.into() },
        ));
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
        fixture.submit(Pin::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            },
        ));
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
        fixture.submit(Uninstall::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            },
        ));
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
        fixture.submit(Uninstall::bundle(
            fixture.ticket.clone(),
            "work".into(),
            ExtUninstallRequest {
                id: PACKAGE_ID.into(),
            },
        ));
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
            .insert(Install::prepared(
                fixture.ticket.clone(),
                "new-profile".into(),
                replacement,
            ));
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
        let operation = fixture.submit(Pin::bundle(
            fixture.ticket.clone(),
            "personal".into(),
            ExtPinRequest {
                id: PACKAGE_ID.into(),
                pinned: true,
            },
        ));
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
