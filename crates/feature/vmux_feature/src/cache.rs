use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
#[cfg(feature = "http")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use vmux_ecs::{Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet, Operation, Revision};

use crate::cache_document::{ClearJournal, Envelope, OffRecord, SavedSnapshot};
use crate::cache_io::{Cleanup, Confirm, Open, Stage, Write, Writer, Written};
use crate::flag::{
    Assignment, Configuration, Context, OfflinePolicy, Retention, Snapshot, Source, SubjectKind,
    Value,
};
use crate::manifest::FeatureManifest;
use crate::runtime::{Lease, Now};
use crate::{Definition, Generation, Provider, Receive, Set};

pub struct CachePlugin;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Bind;

impl bevy_app::Plugin for CachePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::FeaturePlugin>() {
            app.add_plugins(crate::FeaturePlugin);
        }
        app.add_plugins((
            crate::experiment::ExperimentPlugin,
            JobPlugin::<Open>::default(),
            JobPlugin::<Write>::default(),
            JobPlugin::<Confirm>::default(),
            JobPlugin::<Cleanup>::default(),
        ))
        .add_message::<SaveSnapshot>()
        .add_message::<SaveAssignment>()
        .add_message::<RetireAssignments>()
        .init_resource::<CacheLimits>()
        .add_message::<Accepted>()
        .add_message::<RetentionAccepted>()
        .add_message::<RetentionCleared>()
        .add_systems(
            Update,
            (open, reset)
                .chain()
                .in_set(Bind)
                .after(Set::Clock)
                .before(Set::Prepare),
        )
        .add_systems(
            Update,
            (opened, restore, retention)
                .chain()
                .after(JobSet::Apply)
                .before(Set::Accept),
        )
        .add_systems(
            Update,
            (
                observe,
                detect,
                memory,
                receive,
                receive_assignment,
                receive_retirement,
                prepare_disable,
                prepare_retirement,
                prepare_assignment,
                prepare,
                retry,
                send,
                dispatch::<Open>,
                dispatch::<Write>,
                dispatch::<Confirm>,
                dispatch::<Cleanup>,
            )
                .chain()
                .after(Set::Accept)
                .before(Set::Evaluate),
        )
        .add_systems(
            Update,
            (
                adopt_write,
                written,
                confirmed,
                cleaned,
                apply_disable,
                apply_assignment,
                apply_retirement,
                apply,
            )
                .chain()
                .after(JobSet::Apply)
                .before(Set::Accept),
        );
        #[cfg(feature = "http")]
        app.add_systems(
            Update,
            (
                propose_clear,
                invalidate_clear,
                admit_clear,
                apply_clear,
                prepare_clear,
            )
                .chain()
                .after(prepare_disable)
                .before(prepare_retirement),
        );
    }
}

#[derive(Component)]
#[require(Revision)]
pub struct Store;

#[derive(Component, Clone)]
#[component(immutable)]
pub struct Location(pub PathBuf);

#[derive(Component, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct Binding {
    pub store: Entity,
    pub identity: String,
    pub installation: String,
}

#[derive(Component)]
pub struct StoreFailure(pub String);

#[derive(Component)]
struct Opening;

#[derive(Component)]
struct Ready;

#[derive(Component)]
struct Handle(Arc<Writer>);

#[derive(Component)]
struct LastOperation(Option<Uuid>);

#[derive(Component)]
struct Artifact {
    identity: String,
    bytes: Vec<u8>,
}

#[derive(Component)]
struct Retained;

#[derive(Component)]
struct Environment(String);

#[derive(Component)]
struct Installation(String);

#[derive(Component)]
struct DurableRevision(u64);

#[derive(Component)]
struct Restored {
    generation: u64,
}

#[derive(Message)]
pub struct SaveSnapshot {
    pub store: Entity,
    pub identity: String,
    pub document: crate::snapshot::Document,
}

#[derive(Message)]
pub struct SaveAssignment {
    pub store: Entity,
    pub assignment: Assignment,
}

#[derive(Resource)]
pub struct CacheLimits {
    pub assignment_count: usize,
    pub assignment_bytes: usize,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            assignment_count: 4096,
            assignment_bytes: 2 * 1024 * 1024,
        }
    }
}

#[derive(Component)]
pub struct AssignmentCapacity;

#[derive(Message, Clone, Component)]
#[component(immutable)]
pub struct RetireAssignments {
    pub store: Entity,
    pub environment: String,
    pub flag: String,
    pub generation: String,
}

#[derive(Component)]
struct Assign(Assignment);

#[derive(Message)]
pub struct Accepted {
    pub store: Entity,
    pub operation: Uuid,
    pub sequence: u64,
}

#[derive(Component)]
struct Save {
    identity: String,
    document: crate::snapshot::Document,
}

#[derive(Component)]
struct Disable {
    evaluation: Entity,
    record: OffRecord,
}

#[derive(Message)]
pub struct RetentionAccepted {
    pub evaluation: Entity,
    pub operation: Uuid,
    pub revision: u64,
}

#[derive(Message)]
pub struct RetentionCleared {
    pub evaluation: Entity,
    pub operation: Uuid,
    pub revision: u64,
}

#[cfg(feature = "http")]
#[derive(Component)]
#[component(immutable)]
struct Clear {
    evaluation: Entity,
    record: Entity,
    retained_revision: u64,
    approval: ClearApproval,
    permit: Arc<AtomicBool>,
}

#[cfg(feature = "http")]
#[derive(PartialEq)]
struct ClearApproval {
    provider: Entity,
    generation: u64,
    credential: Entity,
    credential_revision: u64,
    declaration: Entity,
    index: usize,
    scope: Entity,
    context: Context,
    binding: Binding,
    document: crate::snapshot::Document,
    flag: String,
}

#[derive(Component)]
struct Abandoned;

#[cfg(feature = "http")]
#[derive(Component)]
struct ClearCommitted;

#[cfg(feature = "http")]
#[derive(SystemParam)]
struct ClearAuthority<'w, 's> {
    data: RetentionData<'w, 's>,
    definitions: Query<'w, 's, (&'static Definition, &'static ChildOf)>,
    authorizations: Query<
        'w,
        's,
        (
            &'static crate::http::CredentialSource,
            &'static crate::http::Authentication,
        ),
    >,
    credentials: Query<'w, 's, (&'static crate::http::Credential, &'static Revision)>,
}

#[cfg(feature = "http")]
impl ClearAuthority<'_, '_> {
    fn approval(&self, evaluation: Entity, now: u64) -> Option<ClearApproval> {
        let (definition, owner) = self.definitions.get(evaluation).ok()?;
        let provider = owner.parent();
        let (environment, revision, generation, lease, binding) =
            self.data.providers.get(provider).ok()?;
        let (source, proof) = self.authorizations.get(provider).ok()?;
        let (credential, credential_revision) = self.credentials.get(source.0).ok()?;
        let context = self.data.contexts.get(definition.scope).ok()?;
        if !lease.available
            || lease.source != Source::Cloud
            || lease.generation != generation.0
            || now >= lease.valid_until
            || credential.expires_at() <= now
            || credential.environment() != environment.as_str()
            || proof.source != source.0
            || proof.source_revision != credential_revision.0
            || proof.generation != generation.0
            || proof.revision != revision.0
            || !proof.permits(definition.scope, context)
            || context.subject.kind != SubjectKind::Installation
            || context.subject.key != binding.installation
        {
            return None;
        }
        if let Some(Value::String(account)) =
            context.attributes.get(&crate::flag::Attribute::Account)
            && *account != credential.account().to_string()
        {
            return None;
        }
        let source = self.data.sources.get(definition.source).ok()?;
        let declaration = source.flags.get(definition.index)?;
        if declaration.offline != OfflinePolicy::RetainOff {
            return None;
        }
        let mut flags = std::collections::BTreeMap::new();
        for (key, parent, configuration) in &self.data.configurations {
            if parent.parent() == provider {
                flags.insert(key.as_str().to_string(), configuration.clone());
            }
        }
        let document = crate::snapshot::Document {
            schema_version: 2,
            environment: environment.as_str().into(),
            revision: revision.0,
            issued_at: lease.issued_at,
            valid_until: lease.valid_until,
            flags,
        };
        if !document.is_valid(environment.as_str(), now)
            || document.fingerprint() != lease.fingerprint
        {
            return None;
        }
        let configuration = document.flags.get(&declaration.key)?;
        let snapshot = Snapshot {
            environment: environment.as_str().into(),
            revision: revision.0,
            issued_at: lease.issued_at,
            valid_until: lease.valid_until,
            source: Source::Cloud,
        };
        let decision = declaration
            .decide(
                context,
                Some(configuration),
                Some(&snapshot),
                None,
                Retention::absent(),
                now,
            )
            .ok()?;
        if decision.value != Value::Boolean(true) || decision.details.source != Source::Cloud {
            return None;
        }
        Some(ClearApproval {
            provider,
            generation: generation.0,
            credential: proof.source,
            credential_revision: credential_revision.0,
            declaration: definition.source,
            index: definition.index,
            scope: definition.scope,
            context: context.clone(),
            binding: binding.clone(),
            document,
            flag: declaration.key.clone(),
        })
    }
}

#[cfg(feature = "http")]
type ClearRecords<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        &'static Environment,
        &'static Installation,
        &'static Revision,
        &'static DurableRevision,
        &'static ChildOf,
    ),
    (With<Retained>, Without<Provider>),
>;

#[cfg(feature = "http")]
fn propose_clear(
    evaluations: Query<Entity, With<Definition>>,
    authority: ClearAuthority,
    records: ClearRecords,
    pending: Query<&Clear>,
    stores: Query<(), (With<Ready>, Without<StoreFailure>)>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for evaluation in &evaluations {
        let Some(approval) = authority.approval(evaluation, now.0) else {
            continue;
        };
        if !stores.contains(approval.binding.store) {
            continue;
        }
        for (record, key, environment, installation, revision, durable, owner) in &records {
            if owner.parent() != approval.binding.store
                || key.as_str() != approval.flag
                || environment.0 != approval.document.environment
                || installation.0 != approval.binding.installation
                || revision.0 == 0
                || durable.0 != revision.0
                || approval.document.revision <= revision.0
                || pending.iter().any(|clear| clear.record == record)
            {
                continue;
            }
            commands.spawn((
                Operation::bundle("flags.cache.clear").expect("retention clear kind"),
                ChildOf(approval.binding.store),
                Clear {
                    evaluation,
                    record,
                    retained_revision: revision.0,
                    approval,
                    permit: Arc::new(AtomicBool::new(true)),
                },
            ));
            break;
        }
    }
}

#[cfg(feature = "http")]
type CancelableClears<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Clear, Has<Plan>),
    (Without<Abandoned>, Without<ClearCommitted>),
>;

#[cfg(feature = "http")]
fn invalidate_clear(
    operations: CancelableClears,
    authority: ClearAuthority,
    records: Query<(&Revision, &DurableRevision), With<Retained>>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (entity, clear, planned) in &operations {
        let valid = records.get(clear.record).is_ok_and(|(revision, durable)| {
            revision.0 == clear.retained_revision && durable.0 == clear.retained_revision
        }) && authority
            .approval(clear.evaluation, now.0)
            .is_some_and(|current| current == clear.approval);
        if !valid {
            clear.permit.store(false, Ordering::Release);
            if planned {
                commands.entity(entity).insert(Abandoned);
            } else {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[cfg(feature = "http")]
type PendingClears<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf, &'static Clear), (Without<Plan>, Without<Abandoned>)>;

#[cfg(feature = "http")]
fn prepare_clear(
    operations: PendingClears,
    stores: WritableStores,
    stored: Stored,
    mut commands: Commands,
) {
    for (entity, owner, clear) in &operations {
        let store = owner.parent();
        if !stores.contains(store) {
            continue;
        }
        let mut envelope = match stored.envelope(store) {
            Ok(envelope) => envelope,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        envelope.clear = Some(ClearJournal {
            operation: Uuid::nil(),
            previous: OffRecord {
                environment: clear.approval.document.environment.clone(),
                installation: clear.approval.binding.installation.clone(),
                flag: clear.approval.flag.clone(),
                revision: clear.retained_revision,
            },
            revision: clear.approval.document.revision,
            admitted: false,
        });
        envelope.snapshots.retain(|snapshot| {
            snapshot.identity != clear.approval.binding.identity
                || snapshot.document.environment != clear.approval.document.environment
        });
        envelope.snapshots.push(SavedSnapshot {
            identity: clear.approval.binding.identity.clone(),
            document: clear.approval.document.clone(),
        });
        match Plan::capture(envelope) {
            Ok(plan) => {
                commands.entity(entity).insert(plan);
                commands.entity(store).insert(Writing(entity));
                break;
            }
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
            }
        }
    }
}

#[cfg(feature = "http")]
type CompletedClears<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Plan,
        &'static Phase,
        &'static Clear,
        Has<Abandoned>,
    ),
    With<ClearCommitted>,
>;

#[cfg(feature = "http")]
type ClearProposals<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Plan,
        &'static Phase,
        &'static Clear,
        Has<Abandoned>,
    ),
    Without<ClearCommitted>,
>;

#[cfg(feature = "http")]
fn admit_clear(
    proposals: ClearProposals,
    committed: ClearCommit,
    authority: ClearAuthority,
    handles: Query<&Handle>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (entity, parent, proposal, phase, clear, abandoned) in &proposals {
        let store = parent.parent();
        if *phase != Phase::Accepted || !proposal.correlated() || !committed.owns(store, entity) {
            continue;
        }
        let fresh = now.fresh();
        if abandoned
            || !committed.current(clear.record, clear.retained_revision)
            || authority
                .approval(clear.evaluation, fresh)
                .is_none_or(|current| current != clear.approval)
        {
            commands
                .entity(store)
                .insert((
                    Revision(proposal.sequence),
                    LastOperation(Some(proposal.operation)),
                ))
                .remove::<Writing>();
            commands.entity(entity).despawn();
            continue;
        }
        let Ok(handle) = handles.get(store) else {
            continue;
        };
        let mut envelope = proposal.envelope.clone();
        envelope.retained.retain(|record| {
            record.environment != clear.approval.document.environment
                || record.installation != clear.approval.binding.installation
                || record.flag != clear.approval.flag
        });
        let Some(journal) = &mut envelope.clear else {
            commands
                .entity(store)
                .insert(StoreFailure("clear proposal has no journal".into()));
            continue;
        };
        journal.admitted = true;
        let final_plan = match Plan::capture(envelope) {
            Ok(plan) => plan,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        let result = Write {
            writer: handle.0.clone(),
            operation: final_plan.operation,
            bytes: final_plan.bytes.clone(),
            permit: None,
            #[cfg(test)]
            fail_directory_sync: false,
            #[cfg(test)]
            fail_before_replacement: false,
        }
        .run();
        match result.stage {
            Stage::Durable => {
                commands
                    .entity(entity)
                    .insert((final_plan, Phase::Accepted, ClearCommitted));
            }
            Stage::Replaced => {
                commands.entity(entity).insert((
                    final_plan,
                    Phase::Confirm,
                    ClearCommitted,
                    WriteFailure(
                        result
                            .error
                            .unwrap_or_else(|| "clear durability is uncertain".into()),
                    ),
                ));
            }
            Stage::NotInstalled => {
                if result.temporary.is_some()
                    && let Err(error) = (Cleanup {
                        writer: handle.0.clone(),
                        operation: final_plan.operation,
                    })
                    .run()
                {
                    commands.entity(store).insert(StoreFailure(error));
                    continue;
                }
                commands.entity(entity).insert((
                    Phase::Write,
                    RetryAt(fresh.saturating_add(1)),
                    WriteFailure(
                        result
                            .error
                            .unwrap_or_else(|| "clear commit was not installed".into()),
                    ),
                ));
            }
        }
    }
}

#[cfg(feature = "http")]
#[derive(SystemParam)]
struct ClearCommit<'w, 's> {
    stores: Query<'w, 's, &'static Writing, With<Store>>,
    records: ClearRecords<'w, 's>,
}

#[cfg(feature = "http")]
impl ClearCommit<'_, '_> {
    fn owns(&self, store: Entity, operation: Entity) -> bool {
        self.stores
            .get(store)
            .is_ok_and(|writing| writing.0 == operation)
    }

    fn current(&self, record: Entity, revision: u64) -> bool {
        self.records
            .get(record)
            .is_ok_and(|(_, _, _, _, current, durable, _)| {
                current.0 == revision && durable.0 == revision
            })
    }

    fn revision(&self, record: Entity, minimum: u64) -> u64 {
        match self.records.get(record) {
            Ok((_, _, _, _, revision, _, _)) => revision.0.max(minimum),
            Err(_) => minimum,
        }
    }
}

#[cfg(feature = "http")]
fn apply_clear(
    operations: CompletedClears,
    committed: ClearCommit,
    committed_clears: Query<(), With<ClearCommitted>>,
    artifacts: Query<(Entity, &Name, &ChildOf, &Artifact)>,
    mut accepted: MessageWriter<RetentionCleared>,
    mut commands: Commands,
) {
    for (entity, owner, plan, phase, clear, abandoned) in &operations {
        let store = owner.parent();
        if *phase != Phase::Accepted || !plan.correlated() || !committed.owns(store, entity) {
            continue;
        }
        let valid = !abandoned
            && committed_clears.contains(entity)
            && committed.current(clear.record, clear.retained_revision);
        if valid {
            let document = &clear.approval.document;
            let identity = &clear.approval.binding.identity;
            let bytes = serde_json::to_vec(document).expect("accepted clear snapshot");
            let mut found = false;
            for (entity, environment, owner, artifact) in &artifacts {
                if owner.parent() == store
                    && environment.as_str() == document.environment
                    && artifact.identity == *identity
                {
                    commands.entity(entity).insert(Artifact {
                        identity: identity.clone(),
                        bytes: bytes.clone(),
                    });
                    found = true;
                    break;
                }
            }
            if !found {
                commands.spawn((
                    Name::new(document.environment.clone()),
                    ChildOf(store),
                    Artifact {
                        identity: identity.clone(),
                        bytes,
                    },
                ));
            }
            commands.entity(clear.record).despawn();
            commands
                .entity(clear.evaluation)
                .insert(Retention::absent());
            accepted.write(RetentionCleared {
                evaluation: clear.evaluation,
                operation: plan
                    .envelope
                    .clear
                    .as_ref()
                    .map_or(plan.operation, |journal| journal.operation),
                revision: clear.approval.document.revision,
            });
        } else {
            let revision = committed.revision(clear.record, clear.retained_revision);
            commands.spawn((
                Operation::bundle("flags.cache.disable").expect("restore retained off kind"),
                ChildOf(store),
                Disable {
                    evaluation: clear.evaluation,
                    record: OffRecord {
                        environment: clear.approval.document.environment.clone(),
                        installation: clear.approval.binding.installation.clone(),
                        flag: clear.approval.flag.clone(),
                        revision,
                    },
                },
            ));
        }
        commands
            .entity(store)
            .insert((Revision(plan.sequence), LastOperation(Some(plan.operation))))
            .remove::<Writing>();
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct RetentionData<'w, 's> {
    providers: Query<
        'w,
        's,
        (
            &'static Name,
            &'static Revision,
            &'static Generation,
            &'static Lease,
            &'static Binding,
        ),
        With<Provider>,
    >,
    sources: Query<'w, 's, &'static FeatureManifest>,
    contexts: Query<'w, 's, &'static Context>,
    configurations: Query<'w, 's, (&'static Name, &'static ChildOf, &'static Configuration)>,
}

type MemoryRecords<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        &'static Environment,
        &'static Installation,
        &'static mut Revision,
        &'static DurableRevision,
        &'static ChildOf,
    ),
    (With<Retained>, Without<Provider>),
>;

fn detect(
    definitions: Query<(Entity, &ChildOf, &Definition)>,
    data: RetentionData,
    stores: Query<(), (With<Ready>, Without<StoreFailure>)>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (evaluation, owner, definition) in &definitions {
        let Ok((environment, revision, generation, lease, binding)) =
            data.providers.get(owner.parent())
        else {
            continue;
        };
        if !stores.contains(binding.store)
            || !lease.available
            || generation.0 != lease.generation
            || now.0 >= lease.valid_until
        {
            continue;
        }
        let Ok(source) = data.sources.get(definition.source) else {
            continue;
        };
        let Some(declaration) = source.flags.get(definition.index) else {
            continue;
        };
        if declaration.offline != OfflinePolicy::RetainOff {
            continue;
        }
        let Ok(context) = data.contexts.get(definition.scope) else {
            continue;
        };
        if context.subject.kind != SubjectKind::Installation
            || context.subject.key != binding.installation
        {
            continue;
        }
        let mut configuration = None;
        for (key, provider, value) in &data.configurations {
            if provider.parent() == owner.parent() && key.as_str() == declaration.key {
                configuration = Some(value);
                break;
            }
        }
        let Some(configuration) = configuration else {
            continue;
        };
        let snapshot = Snapshot {
            environment: environment.as_str().into(),
            revision: revision.0,
            issued_at: lease.issued_at,
            valid_until: lease.valid_until,
            source: lease.source,
        };
        let Ok(decision) = declaration.decide(
            context,
            Some(configuration),
            Some(&snapshot),
            None,
            Retention::absent(),
            now.0,
        ) else {
            continue;
        };
        if decision.value != Value::Boolean(false)
            || !matches!(decision.details.source, Source::Cloud | Source::Cache)
        {
            continue;
        }
        commands.entity(evaluation).insert(ObservedOff {
            store: binding.store,
            record: OffRecord {
                environment: environment.as_str().into(),
                installation: binding.installation.clone(),
                flag: declaration.key.clone(),
                revision: revision.0,
            },
        });
    }
}

#[derive(Component)]
#[component(immutable)]
struct ObservedOff {
    store: Entity,
    record: OffRecord,
}

fn memory(
    observations: Query<(Entity, &ObservedOff)>,
    mut records: MemoryRecords,
    pending: Query<(&ChildOf, &Disable)>,
    mut commands: Commands,
) {
    for (evaluation, observed) in &observations {
        commands.entity(evaluation).remove::<ObservedOff>();
        let mut durable = 0;
        let mut effective_revision = observed.record.revision;
        let mut found = false;
        for (entity, key, namespace, installation, mut current, saved, parent) in &mut records {
            if parent.parent() == observed.store
                && key.as_str() == observed.record.flag
                && namespace.0 == observed.record.environment
                && installation.0 == observed.record.installation
            {
                current.0 = current.0.max(observed.record.revision);
                effective_revision = current.0;
                durable = saved.0;
                found = true;
                let _ = entity;
                break;
            }
        }
        if !found {
            commands.spawn((
                Name::new(observed.record.flag.clone()),
                ChildOf(observed.store),
                Retained,
                Environment(observed.record.environment.clone()),
                Installation(observed.record.installation.clone()),
                Revision(observed.record.revision),
                DurableRevision(0),
            ));
        }
        commands
            .entity(evaluation)
            .insert(Retention::off(effective_revision));
        if durable >= observed.record.revision {
            continue;
        }
        if pending.iter().any(|(owner, pending)| {
            owner.parent() == observed.store
                && pending.record.flag == observed.record.flag
                && pending.record.environment == observed.record.environment
                && pending.record.installation == observed.record.installation
                && pending.record.revision >= observed.record.revision
        }) {
            continue;
        }
        commands.spawn((
            Operation::bundle("flags.cache.disable").expect("durable disable kind"),
            ChildOf(observed.store),
            Disable {
                evaluation,
                record: OffRecord {
                    environment: observed.record.environment.clone(),
                    installation: observed.record.installation.clone(),
                    flag: observed.record.flag.clone(),
                    revision: observed.record.revision,
                },
            },
        ));
    }
}

type WritableStores<'w, 's> =
    Query<'w, 's, (), (With<Ready>, Without<Writing>, Without<StoreFailure>)>;

fn prepare_disable(
    operations: Query<(Entity, &ChildOf, &Disable), Without<Plan>>,
    stores: WritableStores,
    stored: Stored,
    mut commands: Commands,
) {
    for (entity, owner, disable) in &operations {
        let store = owner.parent();
        if !stores.contains(store) {
            continue;
        }
        let mut envelope = match stored.envelope(store) {
            Ok(envelope) => envelope,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        let Some(sequence) = envelope.sequence.checked_add(1) else {
            commands
                .entity(store)
                .insert(StoreFailure("cache sequence is exhausted".into()));
            continue;
        };
        let operation = Uuid::new_v4();
        envelope.sequence = sequence;
        envelope.operation = Some(operation);
        envelope.retained.retain(|record| {
            record.environment != disable.record.environment
                || record.installation != disable.record.installation
                || record.flag != disable.record.flag
        });
        envelope.retained.push(disable.record.clone());
        let bytes = match envelope.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        let fingerprint = Sha256::digest(&bytes).into();
        commands.entity(entity).insert((
            Plan {
                operation,
                sequence,
                bytes: Arc::from(bytes),
                fingerprint,
                envelope,
            },
            Phase::Write,
        ));
        commands.entity(store).insert(Writing(entity));
        break;
    }
}

type SavedDisables<'w, 's> = Query<
    'w,
    's,
    (
        &'static Name,
        &'static Environment,
        &'static Installation,
        &'static mut DurableRevision,
        &'static ChildOf,
    ),
    With<Retained>,
>;

fn apply_disable(
    operations: Query<(Entity, &ChildOf, &Plan, &Phase, &Disable)>,
    stores: Query<&Writing, With<Store>>,
    mut records: SavedDisables,
    mut accepted: MessageWriter<RetentionAccepted>,
    mut commands: Commands,
) {
    for (entity, owner, plan, phase, disable) in &operations {
        if *phase != Phase::Accepted || !plan.correlated() {
            continue;
        }
        let store = owner.parent();
        if !stores.get(store).is_ok_and(|writing| writing.0 == entity) {
            continue;
        }
        for (key, environment, installation, mut durable, owner) in &mut records {
            if owner.parent() == store
                && key.as_str() == disable.record.flag
                && environment.0 == disable.record.environment
                && installation.0 == disable.record.installation
            {
                durable.0 = durable.0.max(disable.record.revision);
                break;
            }
        }
        commands
            .entity(store)
            .insert((Revision(plan.sequence), LastOperation(Some(plan.operation))))
            .remove::<Writing>();
        accepted.write(RetentionAccepted {
            evaluation: disable.evaluation,
            operation: plan.operation,
            revision: disable.record.revision,
        });
        commands.entity(entity).despawn();
    }
}

#[derive(Component)]
struct Writing(Entity);

#[derive(Component)]
#[require(Phase = Phase::Write)]
struct Plan {
    operation: Uuid,
    sequence: u64,
    bytes: Arc<[u8]>,
    fingerprint: [u8; 32],
    envelope: Envelope,
}

impl Plan {
    fn capture(mut envelope: Envelope) -> Result<Self, String> {
        envelope.sequence = envelope
            .sequence
            .checked_add(1)
            .ok_or("cache sequence is exhausted")?;
        let operation = Uuid::new_v4();
        envelope.operation = Some(operation);
        if let Some(journal) = &mut envelope.clear
            && journal.operation.is_nil()
        {
            journal.operation = operation;
        }
        let bytes = envelope.encode()?;
        Ok(Self {
            operation,
            sequence: envelope.sequence,
            fingerprint: Sha256::digest(&bytes).into(),
            bytes: Arc::from(bytes),
            envelope,
        })
    }

    fn correlated(&self) -> bool {
        self.envelope.sequence == self.sequence && self.envelope.operation == Some(self.operation)
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Write,
    Confirm,
    Cleanup,
    Accepted,
}

#[derive(Component)]
struct Waiting;

#[derive(Component)]
struct RetryAt(u64);

#[derive(Component)]
struct RetryWake(async_io::Timer);

impl RetryWake {
    fn new(delay: Duration, wake: &std::task::Waker) -> Self {
        let mut timer = Self(async_io::Timer::after(delay));
        timer.register(wake);
        timer
    }

    fn register(&mut self, wake: &std::task::Waker) {
        let mut context = std::task::Context::from_waker(wake);
        let _ = Pin::new(&mut self.0).poll(&mut context);
    }
}

fn retry(
    mut operations: Query<(Entity, &RetryAt, Option<&mut RetryWake>)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    now: Res<Now>,
    mut commands: Commands,
) {
    let Some(wake) = wake else {
        return;
    };
    for (entity, deadline, timer) in &mut operations {
        if deadline.0 <= now.0 {
            continue;
        }
        match timer {
            Some(mut timer) => timer.register(&wake.0),
            None => {
                commands.entity(entity).insert(RetryWake::new(
                    Duration::from_secs(deadline.0 - now.0),
                    &wake.0,
                ));
            }
        }
    }
}

#[derive(Component)]
pub struct WriteFailure(pub String);

#[derive(Component)]
struct Temporary(PathBuf);

fn observe(
    mut input: MessageReader<crate::SnapshotAccepted>,
    providers: Query<(&Revision, &Generation, &Lease, &Binding), With<Provider>>,
    mut requests: MessageWriter<SaveSnapshot>,
) {
    for received in input.read() {
        if received.source != Source::Cloud {
            continue;
        }
        let Ok((revision, generation, lease, binding)) = providers.get(received.provider) else {
            continue;
        };
        if generation.0 != received.generation
            || revision.0 != received.document.revision
            || !lease.available
            || lease.fingerprint != received.document.fingerprint()
        {
            continue;
        }
        requests.write(SaveSnapshot {
            store: binding.store,
            identity: binding.identity.clone(),
            document: received.document.clone(),
        });
    }
}

fn receive(
    mut requests: MessageReader<SaveSnapshot>,
    stores: Query<Has<StoreFailure>, With<Store>>,
    existing: Query<(Entity, &ChildOf, &Save), Without<Plan>>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !stores.get(request.store).is_ok_and(|failed| !failed)
            || request.identity.is_empty()
            || !request
                .document
                .is_valid(&request.document.environment, now.0)
        {
            continue;
        }
        let mut pending = None;
        for (entity, owner, save) in &existing {
            if owner.parent() == request.store
                && save.identity == request.identity
                && save.document.environment == request.document.environment
            {
                pending = Some((entity, save));
                break;
            }
        }
        if let Some((entity, save)) = pending {
            if request.document.revision < save.document.revision
                || request.document.revision == save.document.revision
                    && (request.document.fingerprint() != save.document.fingerprint()
                        || request.document.issued_at < save.document.issued_at)
            {
                continue;
            }
            commands.entity(entity).insert(Save {
                identity: request.identity.clone(),
                document: request.document.clone(),
            });
            continue;
        }
        commands.spawn((
            Operation::bundle("flags.cache.snapshot").expect("snapshot cache operation"),
            ChildOf(request.store),
            Save {
                identity: request.identity.clone(),
                document: request.document.clone(),
            },
        ));
    }
}

fn receive_assignment(
    mut requests: MessageReader<SaveAssignment>,
    stores: Query<(), (With<Store>, Without<StoreFailure>)>,
    pending: Query<(&ChildOf, &Assign)>,
    assignments: Query<(&ChildOf, &Assignment)>,
    mut commands: Commands,
) {
    let mut batch = Vec::<(Entity, Assignment)>::new();
    for request in requests.read() {
        if !stores.contains(request.store) || request.assignment.validate().is_err() {
            continue;
        }
        let assignment = &request.assignment;
        let matches = |other: &Assignment| {
            other.matches(
                &assignment.environment,
                &assignment.generation,
                &assignment.flag,
                &assignment.subject,
            )
        };
        if pending
            .iter()
            .any(|(owner, pending)| owner.parent() == request.store && matches(&pending.0))
            || assignments
                .iter()
                .any(|(owner, saved)| owner.parent() == request.store && matches(saved))
            || batch
                .iter()
                .any(|(store, saved)| *store == request.store && matches(saved))
        {
            continue;
        }
        batch.push((request.store, assignment.clone()));
        commands.spawn((
            Operation::bundle("flags.cache.assignment").expect("experiment assignment operation"),
            ChildOf(request.store),
            Assign(assignment.clone()),
        ));
    }
}

fn prepare_assignment(
    operations: Query<(Entity, &ChildOf, &Assign), Without<Plan>>,
    stores: WritableStores,
    stored: Stored,
    limits: Res<CacheLimits>,
    mut commands: Commands,
) {
    for (entity, owner, assign) in &operations {
        let store = owner.parent();
        if !stores.contains(store) {
            continue;
        }
        let mut envelope = match stored.envelope(store) {
            Ok(envelope) => envelope,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        if envelope.assignments.iter().any(|other| {
            other.matches(
                &assign.0.environment,
                &assign.0.generation,
                &assign.0.flag,
                &assign.0.subject,
            )
        }) {
            commands.entity(entity).despawn();
            continue;
        }
        let mut bytes = serde_json::to_vec(&assign.0).map_or(usize::MAX, |bytes| bytes.len());
        for assignment in &envelope.assignments {
            bytes = bytes.saturating_add(
                serde_json::to_vec(assignment).map_or(usize::MAX, |bytes| bytes.len()),
            );
            if bytes > limits.assignment_bytes {
                break;
            }
        }
        if envelope.assignments.len() >= limits.assignment_count.min(4096)
            || bytes > limits.assignment_bytes
        {
            commands.entity(store).insert(AssignmentCapacity);
            commands.entity(entity).despawn();
            continue;
        }
        envelope.assignments.push(assign.0.clone());
        let plan = match Plan::capture(envelope) {
            Ok(plan) => plan,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        commands.entity(entity).insert((plan, Phase::Write));
        commands.entity(store).insert(Writing(entity));
        break;
    }
}

fn apply_assignment(
    operations: Query<(Entity, &ChildOf, &Plan, &Phase, &Assign)>,
    stores: Query<&Writing, With<Store>>,
    mut commands: Commands,
) {
    for (entity, owner, plan, phase, assign) in &operations {
        if *phase != Phase::Accepted || !plan.correlated() {
            continue;
        }
        let store = owner.parent();
        if !stores.get(store).is_ok_and(|writing| writing.0 == entity) {
            continue;
        }
        commands.spawn((
            Name::new(assign.0.flag.clone()),
            ChildOf(store),
            assign.0.clone(),
        ));
        commands
            .entity(store)
            .insert((Revision(plan.sequence), LastOperation(Some(plan.operation))))
            .remove::<Writing>();
        commands.entity(entity).despawn();
    }
}

fn receive_retirement(
    mut requests: MessageReader<RetireAssignments>,
    providers: Query<(&Name, &Binding)>,
    configurations: Query<(&Name, &ChildOf, &Configuration)>,
    stores: Query<(), (With<Store>, Without<StoreFailure>)>,
    pending: Query<&RetireAssignments>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !stores.contains(request.store)
            || request.environment.is_empty()
            || request.flag.is_empty()
            || request.generation.is_empty()
        {
            continue;
        }
        let mut active = false;
        for (key, parent, configuration) in &configurations {
            if key.as_str() != request.flag
                || configuration.experiment_generation.as_deref()
                    != Some(request.generation.as_str())
            {
                continue;
            }
            if providers
                .get(parent.parent())
                .is_ok_and(|(environment, binding)| {
                    environment.as_str() == request.environment && binding.store == request.store
                })
            {
                active = true;
                break;
            }
        }
        if active
            || pending.iter().any(|pending| {
                pending.store == request.store
                    && pending.environment == request.environment
                    && pending.flag == request.flag
                    && pending.generation == request.generation
            })
        {
            continue;
        }
        commands.spawn((
            Operation::bundle("flags.cache.retire_assignments")
                .expect("assignment retirement operation"),
            ChildOf(request.store),
            request.clone(),
        ));
    }
}

fn prepare_retirement(
    operations: Query<(Entity, &RetireAssignments), Without<Plan>>,
    stores: WritableStores,
    stored: Stored,
    mut commands: Commands,
) {
    for (entity, request) in &operations {
        if !stores.contains(request.store) {
            continue;
        }
        let mut envelope = match stored.envelope(request.store) {
            Ok(envelope) => envelope,
            Err(error) => {
                commands.entity(request.store).insert(StoreFailure(error));
                continue;
            }
        };
        envelope.assignments.retain(|assignment| {
            assignment.environment != request.environment
                || assignment.flag != request.flag
                || assignment.generation != request.generation
        });
        let plan = match Plan::capture(envelope) {
            Ok(plan) => plan,
            Err(error) => {
                commands.entity(request.store).insert(StoreFailure(error));
                continue;
            }
        };
        commands.entity(entity).insert((plan, Phase::Write));
        commands.entity(request.store).insert(Writing(entity));
        break;
    }
}

fn apply_retirement(
    operations: Query<(Entity, &RetireAssignments, &Plan, &Phase)>,
    stores: Query<&Writing>,
    assignments: Query<(Entity, &ChildOf, &Assignment)>,
    mut commands: Commands,
) {
    for (entity, request, plan, phase) in &operations {
        if *phase != Phase::Accepted
            || !plan.correlated()
            || !stores
                .get(request.store)
                .is_ok_and(|writing| writing.0 == entity)
        {
            continue;
        }
        for (assignment_entity, parent, assignment) in &assignments {
            if parent.parent() == request.store
                && assignment.environment == request.environment
                && assignment.flag == request.flag
                && assignment.generation == request.generation
            {
                commands.entity(assignment_entity).despawn()
            }
        }
        commands
            .entity(request.store)
            .insert((Revision(plan.sequence), LastOperation(Some(plan.operation))))
            .remove::<(Writing, AssignmentCapacity)>();
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct Stored<'w, 's> {
    headers: Query<'w, 's, (&'static Revision, &'static LastOperation), With<Store>>,
    artifacts: Query<'w, 's, (&'static ChildOf, &'static Artifact)>,
    assignments: Query<'w, 's, (&'static ChildOf, &'static Assignment)>,
    retained: Query<
        'w,
        's,
        (
            &'static Name,
            &'static Environment,
            &'static Installation,
            &'static DurableRevision,
            &'static ChildOf,
        ),
        With<Retained>,
    >,
}

impl Stored<'_, '_> {
    fn envelope(&self, store: Entity) -> Result<Envelope, String> {
        let mut envelope = Envelope::empty();
        let (sequence, operation) = self
            .headers
            .get(store)
            .map_err(|_| "cache Store is missing")?;
        envelope.sequence = sequence.0;
        envelope.operation = operation.0;
        for (owner, assignment) in &self.assignments {
            if owner.parent() == store {
                envelope.assignments.push(assignment.clone());
            }
        }
        for (owner, artifact) in &self.artifacts {
            if owner.parent() == store {
                envelope.snapshots.push(SavedSnapshot {
                    identity: artifact.identity.clone(),
                    document: crate::snapshot::Document::decode(&artifact.bytes)?,
                });
            }
        }
        for (flag, environment, installation, revision, owner) in &self.retained {
            if owner.parent() == store && revision.0 > 0 {
                envelope.retained.push(OffRecord {
                    environment: environment.0.clone(),
                    installation: installation.0.clone(),
                    flag: flag.as_str().into(),
                    revision: revision.0,
                });
            }
        }
        Ok(envelope)
    }
}

fn prepare(
    operations: Query<(Entity, &ChildOf, &Save), Without<Plan>>,
    stores: WritableStores,
    stored: Stored,
    mut commands: Commands,
) {
    for (entity, owner, save) in &operations {
        let store = owner.parent();
        if !stores.contains(store) {
            continue;
        }
        let mut envelope = match stored.envelope(store) {
            Ok(envelope) => envelope,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        if envelope.snapshots.iter().any(|snapshot| {
            snapshot.identity == save.identity
                && snapshot.document.environment == save.document.environment
                && (snapshot.document.revision > save.document.revision
                    || snapshot.document.revision == save.document.revision
                        && (snapshot.document.fingerprint() != save.document.fingerprint()
                            || snapshot.document.issued_at > save.document.issued_at))
        }) {
            commands.entity(entity).despawn();
            continue;
        }
        let Some(sequence) = envelope.sequence.checked_add(1) else {
            commands
                .entity(store)
                .insert(StoreFailure("cache sequence is exhausted".into()));
            continue;
        };
        let operation = Uuid::new_v4();
        envelope.sequence = sequence;
        envelope.operation = Some(operation);
        envelope.snapshots.retain(|snapshot| {
            snapshot.identity != save.identity
                || snapshot.document.environment != save.document.environment
        });
        envelope.snapshots.push(SavedSnapshot {
            identity: save.identity.clone(),
            document: save.document.clone(),
        });
        let bytes = match envelope.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        let fingerprint = Sha256::digest(&bytes).into();
        commands.entity(entity).insert((
            Plan {
                operation,
                sequence,
                bytes: Arc::from(bytes),
                fingerprint,
                envelope,
            },
            Phase::Write,
        ));
        commands.entity(store).insert(Writing(entity));
        break;
    }
}

type PendingWrites<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Plan,
        &'static Phase,
        Option<&'static Temporary>,
        Option<&'static RetryAt>,
        Has<Abandoned>,
    ),
    Without<Waiting>,
>;

fn send(
    operations: PendingWrites,
    stores: Query<&Handle>,
    now: Res<Now>,
    #[cfg(feature = "http")] clears: Query<&Clear>,
    mut commands: Commands,
) {
    for (entity, owner, plan, phase, temporary, retry, abandoned) in &operations {
        if abandoned && *phase == Phase::Write {
            if temporary.is_some() {
                commands.entity(entity).insert(Phase::Cleanup);
            } else {
                commands.entity(owner.parent()).remove::<Writing>();
                commands.entity(entity).despawn();
            }
            continue;
        }
        if retry.is_some_and(|retry| retry.0 > now.0) || *phase == Phase::Accepted {
            continue;
        }
        let Ok(handle) = stores.get(owner.parent()) else {
            continue;
        };
        let mut io = commands.spawn((
            Operation::bundle("flags.cache.io").expect("cache IO operation"),
            ChildOf(entity),
        ));
        match phase {
            Phase::Write => {
                #[cfg(feature = "http")]
                let permit = clears.get(entity).ok().map(|clear| clear.permit.clone());
                #[cfg(not(feature = "http"))]
                let permit = None;
                io.insert(Write {
                    writer: handle.0.clone(),
                    operation: plan.operation,
                    bytes: plan.bytes.clone(),
                    permit,
                    #[cfg(test)]
                    fail_directory_sync: false,
                    #[cfg(test)]
                    fail_before_replacement: false,
                });
            }
            Phase::Confirm => {
                io.insert(Confirm {
                    writer: handle.0.clone(),
                    fingerprint: plan.fingerprint,
                });
            }
            Phase::Cleanup => {
                let Some(temporary) = temporary else {
                    io.despawn();
                    continue;
                };
                if temporary.0 != handle.0.temporary(plan.operation) {
                    io.despawn();
                    continue;
                }
                io.insert(Cleanup {
                    writer: handle.0.clone(),
                    operation: plan.operation,
                });
            }
            Phase::Accepted => {}
        }
        commands
            .entity(entity)
            .insert(Waiting)
            .remove::<(RetryAt, RetryWake)>();
    }
}

type Undispatched<'w, 's, J> =
    Query<'w, 's, (Entity, &'static J), (Without<JobRequest<J>>, Without<JobResult<J>>)>;

fn dispatch<J: Job + Component>(
    requests: Undispatched<J>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands.entity(entity).insert(JobRequest(request.clone()));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type Writes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<Write>,
        &'static mut JobResult<Write>,
    ),
>;

#[derive(Component)]
struct WriteReceipt {
    stage: Option<Stage>,
    temporary: Option<PathBuf>,
    error: Option<String>,
}

impl WriteReceipt {
    fn from_native(result: Result<Written, String>) -> Self {
        match result {
            Ok(written) => Self {
                stage: Some(written.stage),
                temporary: written.temporary,
                error: written.error,
            },
            Err(error) => Self {
                stage: None,
                temporary: None,
                error: Some(error),
            },
        }
    }
}

fn adopt_write(mut inputs: Writes, mut commands: Commands) {
    for (entity, owner, accepted, mut result) in &mut inputs {
        if !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        let outcome = std::mem::replace(&mut result.0, Err("consumed cache write".into()));
        commands
            .entity(entity)
            .insert(WriteReceipt::from_native(outcome))
            .remove::<JobResult<Write>>()
            .remove::<JobRequest<Write>>();
    }
}

type WriteReceipts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<Write>,
        &'static mut WriteReceipt,
    ),
>;

fn written(
    mut inputs: WriteReceipts,
    operations: Query<(), With<Plan>>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (entity, owner, accepted, mut result) in &mut inputs {
        let operation = owner.parent();
        if !operations.contains(operation) || !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        commands.entity(entity).despawn();
        let mut target = commands.entity(operation);
        target.remove::<Waiting>();
        match result.stage {
            Some(Stage::Durable) => {
                target.insert(Phase::Accepted).remove::<WriteFailure>();
            }
            stage => {
                target.insert(WriteFailure(
                    result
                        .error
                        .take()
                        .unwrap_or_else(|| "cache write was not acknowledged".into()),
                ));
                match stage {
                    Some(Stage::Replaced) => {
                        target.insert(Phase::Confirm);
                    }
                    Some(Stage::NotInstalled) => {
                        if let Some(path) = result.temporary.take() {
                            target.insert((
                                Phase::Cleanup,
                                Temporary(path),
                                RetryAt(now.0.saturating_add(1)),
                            ));
                        } else {
                            target.insert((Phase::Write, RetryAt(now.0.saturating_add(1))));
                        }
                    }
                    None => {
                        target.insert((Phase::Confirm, RetryAt(now.0.saturating_add(1))));
                    }
                    Some(Stage::Durable) => unreachable!(),
                }
            }
        }
    }
}

type Confirms<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<Confirm>,
        &'static mut JobResult<Confirm>,
    ),
>;

fn confirmed(
    mut inputs: Confirms,
    operations: Query<(), With<Plan>>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (entity, owner, accepted, mut result) in &mut inputs {
        let operation = owner.parent();
        if !operations.contains(operation) || !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        let outcome =
            match std::mem::replace(&mut result.0, Err("consumed cache confirmation".into())) {
                Ok(value) => value,
                Err(error) => Err(error),
            };
        commands.entity(entity).despawn();
        let mut target = commands.entity(operation);
        target.remove::<Waiting>();
        match outcome {
            Ok(()) => {
                target.insert(Phase::Accepted).remove::<WriteFailure>();
            }
            Err(error) => {
                target.insert((WriteFailure(error), RetryAt(now.0.saturating_add(1))));
            }
        }
    }
}

type Cleanups<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<Cleanup>,
        &'static mut JobResult<Cleanup>,
    ),
>;

fn cleaned(
    mut inputs: Cleanups,
    operations: Query<(), With<Plan>>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (entity, owner, accepted, mut result) in &mut inputs {
        let operation = owner.parent();
        if !operations.contains(operation) || !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        let outcome = match std::mem::replace(&mut result.0, Err("consumed cache cleanup".into())) {
            Ok(value) => value,
            Err(error) => Err(error),
        };
        commands.entity(entity).despawn();
        let mut target = commands.entity(operation);
        target.remove::<Waiting>();
        match outcome {
            Ok(()) => {
                target.insert(Phase::Write).remove::<Temporary>();
            }
            Err(error) => {
                target.insert((WriteFailure(error), RetryAt(now.0.saturating_add(1))));
            }
        }
    }
}

fn apply(
    operations: Query<(Entity, &ChildOf, &Plan, &Phase, &Save)>,
    stores: Query<&Writing, With<Store>>,
    artifacts: Query<(Entity, &Name, &ChildOf, &Artifact)>,
    mut accepted: MessageWriter<Accepted>,
    mut commands: Commands,
) {
    for (entity, owner, plan, phase, save) in &operations {
        if *phase != Phase::Accepted || !plan.correlated() {
            continue;
        }
        let store = owner.parent();
        if !stores.get(store).is_ok_and(|writing| writing.0 == entity) {
            continue;
        }
        let bytes = serde_json::to_vec(&save.document).expect("accepted snapshot artifact");
        let mut found = false;
        for (snapshot, environment, parent, artifact) in &artifacts {
            if parent.parent() == store
                && environment.as_str() == save.document.environment
                && artifact.identity == save.identity
            {
                commands.entity(snapshot).insert(Artifact {
                    identity: save.identity.clone(),
                    bytes: bytes.clone(),
                });
                found = true;
                break;
            }
        }
        if !found {
            commands.spawn((
                Name::new(save.document.environment.clone()),
                ChildOf(store),
                Artifact {
                    identity: save.identity.clone(),
                    bytes,
                },
            ));
        }
        commands
            .entity(store)
            .insert((Revision(plan.sequence), LastOperation(Some(plan.operation))))
            .remove::<Writing>();
        accepted.write(Accepted {
            store,
            operation: plan.operation,
            sequence: plan.sequence,
        });
        commands.entity(entity).despawn();
    }
}

type UnopenedStores<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Location),
    (
        With<Store>,
        Without<Opening>,
        Without<Ready>,
        Without<StoreFailure>,
    ),
>;

fn open(stores: UnopenedStores, mut commands: Commands) {
    for (store, location) in &stores {
        commands.entity(store).insert(Opening);
        commands.spawn((
            Operation::bundle("flags.cache.open").expect("cache open kind"),
            ChildOf(store),
            Open {
                path: location.0.clone(),
            },
        ));
    }
}

type ReboundProviders<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Generation, &'static mut Lease),
    (With<Provider>, Changed<Binding>),
>;

fn reset(mut providers: ReboundProviders, mut commands: Commands) {
    for (entity, mut generation, mut lease) in &mut providers {
        generation.0 = generation.0.saturating_add(1);
        lease.available = false;
        commands.entity(entity).remove::<Restored>();
    }
}

type Opens<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<Open>,
        &'static mut JobResult<Open>,
    ),
>;

fn opened(mut inputs: Opens, stores: Query<(), With<Store>>, mut commands: Commands) {
    for (entity, owner, accepted, mut result) in &mut inputs {
        let store = owner.parent();
        if !stores.contains(store) || !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        let outcome = match std::mem::replace(&mut result.0, Err("consumed cache open".into())) {
            Ok(value) => value,
            Err(error) => Err(error),
        };
        commands.entity(entity).despawn();
        commands.entity(store).remove::<Opening>();
        let opened = match outcome {
            Ok(opened) => opened,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        commands.entity(store).insert(Handle(opened.writer));
        let envelope = match opened.bytes {
            Some(bytes) => Envelope::decode(&bytes),
            None => Ok(Envelope::empty()),
        };
        let envelope = match envelope {
            Ok(envelope) => envelope,
            Err(error) => {
                commands.entity(store).insert(StoreFailure(error));
                continue;
            }
        };
        for snapshot in envelope.snapshots {
            let bytes = serde_json::to_vec(&snapshot.document).expect("validated cached snapshot");
            commands.spawn((
                Name::new(snapshot.document.environment),
                ChildOf(store),
                Artifact {
                    identity: snapshot.identity,
                    bytes,
                },
            ));
        }
        for record in envelope.retained {
            commands.spawn((
                Name::new(record.flag),
                ChildOf(store),
                Retained,
                Environment(record.environment),
                Installation(record.installation),
                Revision(record.revision),
                DurableRevision(record.revision),
            ));
        }
        for assignment in envelope.assignments {
            commands.spawn((
                Name::new(assignment.flag.clone()),
                ChildOf(store),
                assignment,
            ));
        }
        commands.entity(store).insert((
            Revision(envelope.sequence),
            LastOperation(envelope.operation),
            Ready,
        ));
    }
}

type Providers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        &'static Generation,
        &'static Binding,
        Option<&'static Restored>,
    ),
    With<Provider>,
>;

fn restore(
    providers: Providers,
    stores: Query<(), With<Ready>>,
    artifacts: Query<(&Name, &ChildOf, &Artifact)>,
    now: Res<Now>,
    mut received: MessageWriter<Receive>,
    mut commands: Commands,
) {
    for (provider, environment, generation, binding, restored) in &providers {
        if !stores.contains(binding.store)
            || restored.is_some_and(|restored| restored.generation == generation.0)
        {
            continue;
        }
        for (name, owner, artifact) in &artifacts {
            if owner.parent() != binding.store
                || name.as_str() != environment.as_str()
                || artifact.identity != binding.identity
            {
                continue;
            }
            if let Ok(document) = crate::snapshot::Document::decode(&artifact.bytes)
                && document.is_valid(environment.as_str(), now.0)
            {
                received.write(Receive {
                    receipt: None,
                    provider,
                    generation: generation.0,
                    document,
                    source: Source::Cache,
                });
            }
        }
        commands.entity(provider).insert(Restored {
            generation: generation.0,
        });
    }
}

type Retaining<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Definition,
        Option<&'static Retention>,
    ),
>;

fn retention(
    definitions: Retaining,
    providers: Query<(&Name, &Binding), With<Provider>>,
    stores: Query<(Has<Ready>, Has<StoreFailure>), With<Store>>,
    sources: Query<&FeatureManifest>,
    scopes: Query<&Context>,
    records: Query<(&Name, &Environment, &Installation, &Revision, &ChildOf), With<Retained>>,
    mut commands: Commands,
) {
    for (entity, owner, definition, existing) in &definitions {
        let Ok((environment, binding)) = providers.get(owner.parent()) else {
            continue;
        };
        let Ok(source) = sources.get(definition.source) else {
            continue;
        };
        let Some(declaration) = source.flags.get(definition.index) else {
            continue;
        };
        if declaration.offline != OfflinePolicy::RetainOff {
            continue;
        }
        let status = match stores.get(binding.store) {
            Ok((true, false)) => {
                let context = scopes.get(definition.scope);
                if !context.is_ok_and(|context| {
                    context.subject.kind == SubjectKind::Installation
                        && context.subject.key == binding.installation
                        && context.validate().is_ok()
                }) {
                    Retention::failed()
                } else {
                    let mut status = Retention::absent();
                    for (flag, namespace, installation, revision, owner) in &records {
                        if owner.parent() == binding.store
                            && flag.as_str() == declaration.key
                            && namespace.0 == environment.as_str()
                            && installation.0 == binding.installation
                        {
                            status = Retention::off(revision.0);
                            break;
                        }
                    }
                    status
                }
            }
            Ok((_, true)) | Err(_) => Retention::failed(),
            _ => Retention::restoring(),
        };
        if existing != Some(&status) {
            commands.entity(entity).insert(status);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::flag::{Configuration, Decision};
    #[cfg(feature = "http")]
    use crate::http::{Credential, CredentialSource, Endpoint, HttpPlugin, RequireAuthorization};
    use crate::manifest::{FeatureManifestOwner, FeatureManifestSource};
    use crate::snapshot::Document;
    use crate::{Current, Tick};

    struct FixtureFeature;

    impl FeatureManifestSource for FixtureFeature {
        const SOURCE: &'static str = r#"(flags:[
            (key:"normal",type:Boolean,class:Release,default:Boolean(false)),
            (key:"remote.live",type:Boolean,class:Release,default:Boolean(true),offline:RetainOff),
            (key:"retained.default-off",type:Boolean,class:Operational,default:Boolean(false),offline:RetainOff),
        ])"#;
    }

    struct Fixture {
        app: App,
        store: Entity,
        provider: Entity,
        normal: Entity,
        live: Entity,
    }

    #[cfg(feature = "http")]
    struct Notify(std::sync::mpsc::SyncSender<()>);

    #[cfg(feature = "http")]
    impl std::task::Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }
        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    impl Fixture {
        #[cfg(feature = "http")]
        fn wakes(&mut self) -> std::sync::mpsc::Receiver<()> {
            let (send, receive) = std::sync::mpsc::sync_channel(16);
            self.app
                .insert_resource(vmux_ecs::JobWake(std::task::Waker::from(Arc::new(Notify(
                    send,
                )))));
            receive
        }

        fn idle(&mut self) {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let mut saves = self
                    .app
                    .world_mut()
                    .query_filtered::<Entity, Or<(With<Save>, With<Disable>)>>();
                #[cfg(feature = "http")]
                let clearing = self
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<Clear>>()
                    .iter(self.app.world())
                    .next()
                    .is_some();
                #[cfg(not(feature = "http"))]
                let clearing = false;
                if !clearing
                    && self.app.world().get::<Writing>(self.store).is_none()
                    && saves.iter(self.app.world()).next().is_none()
                {
                    return;
                }
                assert!(Instant::now() < until, "cache transactions did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        fn new(path: &std::path::Path, identity: &str) -> Self {
            let mut app = App::new();
            app.add_plugins((
                CachePlugin,
                crate::manifest::FeaturePlugin::<FixtureFeature>::default(),
            ));
            let source = app
                .world_mut()
                .query_filtered::<Entity, With<FeatureManifestOwner<FixtureFeature>>>()
                .single(app.world())
                .unwrap();
            let scope = app
                .world_mut()
                .spawn((
                    Name::new("Installation flags"),
                    Context::installation("device-a"),
                ))
                .id();
            let store = app
                .world_mut()
                .spawn((
                    Name::new("Feature cache"),
                    Store,
                    Location(path.to_path_buf()),
                ))
                .id();
            let provider = app
                .world_mut()
                .spawn((
                    Provider::bundle("production"),
                    Binding {
                        store,
                        identity: identity.into(),
                        installation: "device-a".into(),
                    },
                ))
                .id();
            let declarations = &FeatureManifest::of::<FixtureFeature>().flags;
            let normal = app
                .world_mut()
                .spawn(Definition::bundle(
                    provider,
                    source,
                    0,
                    scope,
                    &declarations[0],
                ))
                .id();
            let live = app
                .world_mut()
                .spawn(Definition::bundle(
                    provider,
                    source,
                    1,
                    scope,
                    &declarations[1],
                ))
                .id();
            app.world_mut().write_message(Tick(1000));
            Self {
                app,
                store,
                provider,
                normal,
                live,
            }
        }

        fn ready(&mut self) -> Result<(), String> {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if self.app.world().get::<Ready>(self.store).is_some() {
                    return Ok(());
                }
                if let Some(failure) = self.app.world().get::<StoreFailure>(self.store) {
                    return Err(failure.0.clone());
                }
                assert!(Instant::now() < until, "cache restoration did not finish");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn save(&mut self, identity: &str, revision: u64, enabled: bool) -> Accepted {
            self.app.world_mut().write_message(SaveSnapshot {
                store: self.store,
                identity: identity.into(),
                document: Self::document(revision, enabled),
            });
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(accepted) = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<Accepted>>()
                    .drain()
                    .next()
                {
                    return accepted;
                }
                assert!(Instant::now() < until, "cache write was not acknowledged");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn document(revision: u64, enabled: bool) -> Document {
            Document {
                schema_version: 1,
                environment: "production".into(),
                revision,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("normal".into(), Configuration::boolean(enabled))]),
            }
        }

        #[cfg(feature = "http")]
        fn disable(&mut self, revision: u64) {
            let generation = self.app.world().get::<Generation>(self.provider).unwrap().0;
            self.app.world_mut().write_message(Receive {
                receipt: None,
                provider: self.provider,
                generation,
                source: Source::Cloud,
                document: Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(false))]),
                },
            });
            self.idle();
            assert!(!self.app.world().get::<Current>(self.live).unwrap().0);
        }

        #[cfg(feature = "http")]
        fn authenticate(&mut self, revision: u64) -> crate::http::tests::Server {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let body = serde_json::to_string(&Document {
                schema_version: 1,
                environment: "production".into(),
                revision,
                issued_at: now,
                valid_until: now + 600,
                flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(true))]),
            })
            .unwrap();
            let server = crate::http::tests::Server::new(vec![crate::http::tests::Server::reply(
                200,
                &body,
                "\"clear\"",
            )]);
            let credential = self
                .app
                .world_mut()
                .spawn((
                    Name::new("Control credential"),
                    Credential::new(
                        "cache-test-credential".into(),
                        Uuid::new_v4(),
                        Uuid::new_v4(),
                        Uuid::new_v4(),
                        "production".into(),
                        now + 600,
                    )
                    .unwrap(),
                ))
                .id();
            self.app.add_plugins(HttpPlugin);
            self.app.world_mut().entity_mut(self.provider).insert((
                Endpoint::new(&server.url).unwrap(),
                CredentialSource(credential),
                RequireAuthorization,
            ));
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if self
                    .app
                    .world()
                    .get::<crate::http::Authentication>(self.provider)
                    .is_some()
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "authenticated HTTP snapshot did not arrive"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            let request = server
                .requests
                .recv_timeout(Duration::from_secs(1))
                .unwrap();
            assert!(
                request
                    .to_lowercase()
                    .contains("authorization: bearer cache-test-credential\r\n")
            );
            server
        }

        #[cfg(feature = "http")]
        fn planned_clear(&mut self) {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let world = self.app.world_mut();
                if world
                    .query_filtered::<Entity, (With<Clear>, With<Plan>)>()
                    .iter(world)
                    .next()
                    .is_some()
                {
                    return;
                }
                assert!(Instant::now() < until, "authorized clear was not prepared");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[cfg(feature = "http")]
    #[test]
    fn authenticated_newer_on_is_durable_before_reopening_and_survives_offline_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        {
            let mut fixture = Fixture::new(&path, "account-a");
            fixture.ready().unwrap();
            fixture.disable(6);
            let _server = fixture.authenticate(7);
            assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            assert!(
                fixture
                    .app
                    .world()
                    .resource::<Messages<RetentionCleared>>()
                    .is_empty()
            );
            fixture.idle();
            assert!(fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            let receipt = fixture
                .app
                .world_mut()
                .resource_mut::<Messages<RetentionCleared>>()
                .drain()
                .next()
                .unwrap();
            assert_eq!(receipt.revision, 7);
            let persisted = Envelope::decode(&std::fs::read(&path).unwrap()).unwrap();
            assert!(persisted.retained.is_empty());
            assert_eq!(persisted.snapshots[0].document.revision, 7);
        }
        let mut restored = Fixture::new(&path, "account-b");
        restored.app.world_mut().write_message(Tick(u64::MAX - 100));
        restored.ready().unwrap();
        assert!(
            restored
                .app
                .world()
                .get::<Current>(restored.live)
                .unwrap()
                .0
        );
    }

    #[cfg(feature = "http")]
    #[test]
    fn changed_context_cancels_a_queued_clear_without_replacing_the_retained_disable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut fixture = Fixture::new(&path, "account-a");
        fixture.ready().unwrap();
        fixture.disable(6);
        let _server = fixture.authenticate(7);
        fixture.planned_clear();
        let writer = fixture
            .app
            .world()
            .get::<Handle>(fixture.store)
            .unwrap()
            .0
            .clone();
        let pause = writer.pause();
        fixture.app.update();
        let scope = fixture
            .app
            .world()
            .get::<Definition>(fixture.live)
            .unwrap()
            .scope;
        fixture
            .app
            .world_mut()
            .get_mut::<Context>(scope)
            .unwrap()
            .generation += 1;
        fixture.app.update();
        drop(pause);
        fixture.idle();
        assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<RetentionCleared>>()
                .is_empty()
        );
        let persisted = Envelope::decode(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted.retained[0].revision, 6);
    }

    #[cfg(feature = "http")]
    #[test]
    fn pending_clear_restart_keeps_off_before_authority_loss_can_be_repaired() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let interrupted = directory.path().join("interrupted.json");
        let mut fixture = Fixture::new(&path, "account-a");
        fixture.ready().unwrap();
        fixture.disable(6);
        let _server = fixture.authenticate(7);
        fixture.planned_clear();
        let world = fixture.app.world_mut();
        let (io, request, owner) = world
            .query::<(Entity, &Write, Option<&JobOwner<Write>>)>()
            .single(world)
            .unwrap();
        assert!(
            owner.is_none(),
            "crash fixture must precede native job adoption"
        );
        let request = request.clone();
        world.entity_mut(io).remove::<JobRequest<Write>>();
        let result = request.run();
        assert!(result.error.is_none(), "real pending-journal write failed");
        let bytes = std::fs::read(&path).unwrap();
        let persisted = Envelope::decode(&bytes).unwrap();
        assert!(!persisted.clear.as_ref().unwrap().admitted);
        assert_eq!(persisted.retained[0].revision, 6);
        std::fs::write(&interrupted, bytes).unwrap();
        drop(fixture);
        let mut restored = Fixture::new(&interrupted, "account-b");
        restored.app.world_mut().write_message(Tick(u64::MAX - 100));
        restored.ready().unwrap();
        restored
            .app
            .insert_resource(crate::OverridePolicy::development());
        restored
            .app
            .world_mut()
            .entity_mut(restored.live)
            .insert(crate::DevelopmentOverride(Value::Boolean(true)));
        restored.app.update();
        assert!(
            !restored
                .app
                .world()
                .get::<Current>(restored.live)
                .unwrap()
                .0
        );
    }

    #[cfg(feature = "http")]
    #[test]
    fn a_newer_off_during_clear_stays_closed_and_keeps_the_newest_durable_disable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut fixture = Fixture::new(&path, "account-a");
        fixture.ready().unwrap();
        fixture.disable(6);
        let _server = fixture.authenticate(7);
        fixture.planned_clear();
        let writer = fixture
            .app
            .world()
            .get::<Handle>(fixture.store)
            .unwrap()
            .0
            .clone();
        let pause = writer.pause();
        fixture.app.update();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let generation = fixture
            .app
            .world()
            .get::<Generation>(fixture.provider)
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation,
            source: Source::Cloud,
            document: Document {
                schema_version: 1,
                environment: "production".into(),
                revision: 8,
                issued_at: now,
                valid_until: now + 600,
                flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(false))]),
            },
        });
        fixture.app.update();
        assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
        drop(pause);
        fixture.idle();
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<RetentionCleared>>()
                .is_empty()
        );
        assert_eq!(
            Envelope::decode(&std::fs::read(&path).unwrap())
                .unwrap()
                .retained[0]
                .revision,
            8
        );
    }

    #[cfg(feature = "http")]
    #[test]
    fn failed_clear_wakes_for_same_operation_retry_and_never_acknowledges_uncertain_bytes() {
        for uncertain in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("flags.json");
            let mut fixture = Fixture::new(&path, "account-a");
            fixture.ready().unwrap();
            fixture.disable(6);
            let _server = fixture.authenticate(7);
            fixture.planned_clear();
            let wake = fixture.wakes();
            let world = fixture.app.world_mut();
            let (io, request, owner) = world
                .query::<(Entity, &Write, &ChildOf)>()
                .single(world)
                .unwrap();
            let operation = world.get::<Plan>(owner.parent()).unwrap().operation;
            let mut request = request.clone();
            request.fail_directory_sync = uncertain;
            request.fail_before_replacement = !uncertain;
            world
                .entity_mut(io)
                .insert((request.clone(), JobRequest(request)));
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                fixture.app.update();
                let world = fixture.app.world_mut();
                if world
                    .query_filtered::<Entity, With<WriteFailure>>()
                    .iter(world)
                    .next()
                    .is_some()
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "injected native write failure did not arrive"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            assert!(
                fixture
                    .app
                    .world()
                    .resource::<Messages<RetentionCleared>>()
                    .is_empty()
            );
            assert_eq!(
                Envelope::decode(&std::fs::read(&path).unwrap())
                    .unwrap()
                    .retained
                    .is_empty(),
                false
            );
            if !uncertain {
                while wake.try_recv().is_ok() {}
            }
            wake.recv_timeout(Duration::from_secs(2))
                .expect("native confirmation or retry must wake an idle host");
            fixture.idle();
            let receipt = fixture
                .app
                .world_mut()
                .resource_mut::<Messages<RetentionCleared>>()
                .drain()
                .next()
                .unwrap();
            assert_eq!(receipt.operation, operation);
            assert!(fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            assert!(
                Envelope::decode(&std::fs::read(path).unwrap())
                    .unwrap()
                    .retained
                    .is_empty()
            );
        }
    }

    #[cfg(feature = "http")]
    #[test]
    fn an_authenticated_snapshot_for_another_account_cannot_clear_retention() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut fixture = Fixture::new(&path, "account-a");
        fixture.ready().unwrap();
        fixture.disable(6);
        let scope = fixture
            .app
            .world()
            .get::<Definition>(fixture.live)
            .unwrap()
            .scope;
        fixture
            .app
            .world_mut()
            .get_mut::<Context>(scope)
            .unwrap()
            .attributes
            .insert(
                crate::flag::Attribute::Account,
                Value::String(Uuid::from_u128(1).to_string()),
            );
        let _server = fixture.authenticate(7);
        fixture.idle();
        assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<RetentionCleared>>()
                .is_empty()
        );
        assert_eq!(
            Envelope::decode(&std::fs::read(path).unwrap())
                .unwrap()
                .retained[0]
                .revision,
            6
        );
    }

    #[test]
    fn snapshot_restart_restores_only_the_selected_identity_and_preserves_the_lease() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        {
            let mut writer = Fixture::new(&path, "account-a");
            writer.ready().unwrap();
            let receipt = writer.save("account-a", 7, true);
            let envelope = Envelope::decode(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(envelope.operation, Some(receipt.operation));
            assert_eq!(envelope.sequence, receipt.sequence);
        }
        {
            let mut other = Fixture::new(&path, "account-b");
            other.ready().unwrap();
            assert!(!other.app.world().get::<Current>(other.normal).unwrap().0);
        }
        let mut restored = Fixture::new(&path, "account-a");
        restored.ready().unwrap();
        assert!(
            restored
                .app
                .world()
                .get::<Current>(restored.normal)
                .unwrap()
                .0
        );
        assert_eq!(
            restored
                .app
                .world()
                .get::<Decision<bool>>(restored.normal)
                .unwrap()
                .details
                .source,
            Source::Cache
        );
        restored.app.world_mut().write_message(Tick(1100));
        restored.app.update();
        assert!(
            !restored
                .app
                .world()
                .get::<Current>(restored.normal)
                .unwrap()
                .0
        );
    }

    #[test]
    fn expired_snapshot_and_changed_identity_do_not_clear_a_durable_disable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut document = Fixture::document(3, true);
        document.issued_at = 900;
        document.valid_until = 950;
        let envelope = Envelope {
            schema_version: 1,
            sequence: 1,
            operation: Some(Uuid::new_v4()),
            snapshots: vec![SavedSnapshot {
                identity: "account-a".into(),
                document,
            }],
            retained: vec![OffRecord {
                environment: "production".into(),
                installation: "device-a".into(),
                flag: "remote.live".into(),
                revision: 4,
            }],
            assignments: Vec::new(),
            clear: None,
        };
        std::fs::write(&path, envelope.encode().unwrap()).unwrap();
        let mut fixture = Fixture::new(&path, "account-b");
        fixture.ready().unwrap();
        assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
        let generation = fixture
            .app
            .world()
            .get::<Generation>(fixture.provider)
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation,
            source: Source::Cloud,
            document: Document {
                schema_version: 1,
                environment: "production".into(),
                revision: 9,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(true))]),
            },
        });
        fixture.app.update();
        assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
        fixture.app.world_mut().write_message(Tick(1200));
        fixture.app.update();
        assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
    }

    #[test]
    fn corrupt_or_newer_cache_is_preserved_and_retention_fails_closed() {
        for bytes in [b"corrupt cache".to_vec(), b"{\"schema_version\":3,\"sequence\":0,\"operation\":null,\"snapshots\":[],\"retained\":[]}".to_vec()] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("flags.json");
            std::fs::write(&path, &bytes).unwrap();
            let mut fixture = Fixture::new(&path, "anonymous");
            assert!(fixture.ready().is_err());
            fixture.app.update();
            assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn a_competing_cache_writer_cannot_replace_a_live_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut first = Fixture::new(&path, "anonymous");
        first.ready().unwrap();
        let mut second = Fixture::new(&path, "anonymous");
        assert!(second.ready().is_err());
        first.save("anonymous", 1, false);
        assert_eq!(
            Envelope::decode(&std::fs::read(path).unwrap())
                .unwrap()
                .sequence,
            1
        );
    }

    #[test]
    fn a_type_error_default_false_cannot_be_saved_as_a_cloud_disable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut fixture = Fixture::new(&path, "account-a");
        let source = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<FeatureManifestOwner<FixtureFeature>>>()
            .single(fixture.app.world())
            .unwrap();
        let scope = fixture
            .app
            .world()
            .get::<Definition>(fixture.live)
            .unwrap()
            .scope;
        let declaration = &FeatureManifest::of::<FixtureFeature>().flags[2];
        let evaluation = fixture
            .app
            .world_mut()
            .spawn(Definition::bundle(
                fixture.provider,
                source,
                2,
                scope,
                declaration,
            ))
            .id();
        fixture.ready().unwrap();
        let generation = fixture
            .app
            .world()
            .get::<Generation>(fixture.provider)
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation,
            source: Source::Cloud,
            document: Document {
                schema_version: 2,
                environment: "production".into(),
                revision: 1,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([(
                    "retained.default-off".into(),
                    Configuration::constant(crate::flag::Value::String("malformed boolean".into())),
                )]),
            },
        });
        fixture.idle();
        let decision = fixture
            .app
            .world()
            .get::<Decision<crate::flag::Value>>(evaluation)
            .unwrap();
        assert_eq!(decision.value, crate::flag::Value::Boolean(false));
        assert_eq!(decision.details.source, Source::Default);
        let mut records = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Retained>>();
        assert_eq!(records.iter(fixture.app.world()).count(), 0);
        let envelope = Envelope::decode(&std::fs::read(&path).unwrap()).unwrap();
        assert!(envelope.retained.is_empty());
    }

    #[test]
    fn accepted_cloud_off_closes_immediately_and_survives_expired_offline_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        {
            let mut fixture = Fixture::new(&path, "account-a");
            fixture.ready().unwrap();
            assert!(fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            let generation = fixture
                .app
                .world()
                .get::<Generation>(fixture.provider)
                .unwrap()
                .0;
            fixture.app.world_mut().write_message(Receive {
                receipt: None,
                provider: fixture.provider,
                generation,
                source: Source::Cloud,
                document: Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision: 6,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(false))]),
                },
            });
            fixture.app.update();
            assert!(!fixture.app.world().get::<Current>(fixture.live).unwrap().0);
            assert!(
                fixture
                    .app
                    .world()
                    .resource::<Messages<RetentionAccepted>>()
                    .is_empty()
            );
            let until = Instant::now() + Duration::from_secs(5);
            let receipt = loop {
                fixture.app.update();
                if let Some(receipt) = fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<RetentionAccepted>>()
                    .drain()
                    .next()
                {
                    break receipt;
                }
                assert!(
                    Instant::now() < until,
                    "durable disable was not acknowledged"
                );
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(receipt.evaluation, fixture.live);
            let persisted = Envelope::decode(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(persisted.retained[0].revision, 6);
            fixture.idle();
        }
        let mut restored = Fixture::new(&path, "account-b");
        restored.app.world_mut().write_message(Tick(1200));
        restored.ready().unwrap();
        assert!(
            !restored
                .app
                .world()
                .get::<Current>(restored.live)
                .unwrap()
                .0
        );
        let generation = restored
            .app
            .world()
            .get::<Generation>(restored.provider)
            .unwrap()
            .0;
        restored.app.world_mut().write_message(Receive {
            receipt: None,
            provider: restored.provider,
            generation,
            source: Source::Cloud,
            document: Document {
                schema_version: 1,
                environment: "production".into(),
                revision: 7,
                issued_at: 1200,
                valid_until: 1300,
                flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(true))]),
            },
        });
        restored.app.update();
        assert!(
            !restored
                .app
                .world()
                .get::<Current>(restored.live)
                .unwrap()
                .0
        );
    }
}
