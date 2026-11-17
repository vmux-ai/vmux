use std::collections::BTreeMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::Commands;
use bevy::prelude::*;
use bevy_app::{App, Plugin, Update};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet, Operation, PageMetadata, Revision,
    UiStatePlugin, UiStateWrite,
};

use crate::administration::{Capabilities, Publication, Snapshot};
use crate::native::{self, Endpoint};
use crate::state::*;

pub(crate) struct FeatureHostPlugin;

impl Plugin for FeatureHostPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<native::Inspect>::default(),
            JobPlugin::<native::Load>::default(),
            JobPlugin::<native::Save>::default(),
            JobPlugin::<native::Review>::default(),
            JobPlugin::<native::Publish>::default(),
            JobPlugin::<native::Discard>::default(),
            UiStatePlugin::<FeaturesUiState>::default(),
            UiEventPlugin::<(
                FeaturesOpenRequest,
                FeaturesFilterRequest,
                FeatureSelectRequest,
                DraftEditRequest,
                DraftSaveRequest,
            )>::default(),
            UiEventPlugin::<(
                DraftReviewRequest,
                DraftPublishRequest,
                DraftDiscardRequest,
                FlagRollbackRequest,
            )>::default(),
        ))
        .add_message::<Received>()
        .add_message::<ReadRequested>()
        .add_systems(Update, load.before(JobSet::Submit))
        .add_observer(page_ready)
        .add_observer(open)
        .add_observer(filter)
        .add_observer(select)
        .add_observer(edit)
        .add_observer(save)
        .add_observer(review)
        .add_observer(publish)
        .add_observer(discard)
        .add_observer(rollback)
        .add_systems(
            Update,
            (
                inspect,
                receive::<native::Load>,
                receive::<native::Save>,
                receive::<native::Review>,
                receive::<native::Publish>,
                receive::<native::Discard>,
                adopt,
                emit,
            )
                .chain()
                .after(JobSet::Apply),
        );
    }
}

#[derive(Component, Default)]
struct Context {
    endpoint: Option<Entity>,
    generation: u64,
    configuration: [u8; 32],
    environment: String,
    revision: u64,
    loaded: bool,
    capabilities: Capabilities,
    pending: Option<Entity>,
    error: String,
}

#[derive(Component, Default)]
struct Selection {
    query: String,
    key: String,
    tab: String,
}

#[derive(Component)]
struct Flag {
    published: Option<bool>,
    desired: bool,
    saved: bool,
    persisted: bool,
    staged: bool,
}

#[derive(Component)]
struct Draft {
    base: u64,
    reviewed: Option<u64>,
}

impl Draft {
    fn local(page: Entity, base: u64) -> impl Bundle {
        (
            Name::new(uuid::Uuid::new_v4().to_string()),
            Revision(0),
            ChildOf(page),
            Self {
                base,
                reviewed: None,
            },
        )
    }
}

impl crate::administration::Save {
    fn matches(&self, snapshot: &Snapshot) -> bool {
        snapshot.revision == self.base_revision
            && snapshot.draft.as_ref().is_some_and(|draft| {
                draft.id == self.draft_id
                    && draft.base_revision == self.base_revision
                    && draft.revision == self.expected_revision.saturating_add(1)
                    && self
                        .changes
                        .iter()
                        .all(|(key, value)| draft.flags.get(key) == Some(value))
            })
    }
}

impl crate::administration::Review {
    fn matches(&self, snapshot: &Snapshot) -> bool {
        snapshot.revision == self.base_revision
            && snapshot.draft.as_ref().is_some_and(|draft| {
                draft.id == self.draft_id
                    && draft.base_revision == self.base_revision
                    && draft.revision == self.expected_revision
                    && draft.reviewed_revision == Some(self.expected_revision)
            })
    }
}

impl crate::administration::Publish {
    fn matches(&self, snapshot: &Snapshot) -> bool {
        snapshot.history.iter().any(|publication| {
            publication.operation_id == self.operation_id
                && publication.draft_id == self.draft_id
                && publication.draft_revision == self.expected_revision
                && publication.revision == self.base_revision.saturating_add(1)
        })
    }
}

impl crate::administration::Discard {
    fn matches(&self, snapshot: &Snapshot) -> bool {
        snapshot.draft.is_none()
    }
}

#[derive(Component)]
struct History(Publication);

#[derive(Component)]
struct Pending {
    endpoint: Entity,
    generation: u64,
    configuration: [u8; 32],
    preserve: bool,
    read_only: bool,
}

impl Pending {
    fn read(page: Entity, endpoint: Entity, context: &Context) -> impl Bundle {
        (
            Operation::bundle("feature.inspect").unwrap(),
            ChildOf(page),
            Self {
                endpoint,
                generation: context.generation,
                configuration: context.configuration,
                preserve: true,
                read_only: true,
            },
        )
    }

    fn write(page: Entity, context: &Context) -> impl Bundle {
        (
            Operation::bundle("feature.admin").unwrap(),
            ChildOf(page),
            Self {
                endpoint: context.endpoint.unwrap(),
                generation: context.generation,
                configuration: context.configuration,
                preserve: false,
                read_only: false,
            },
        )
    }
}

#[derive(Message)]
struct ReadRequested {
    page: Entity,
    endpoint: Entity,
    generation: u64,
}

fn load(
    mut requests: MessageReader<ReadRequested>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    owned: OwnedPageValues,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(mut context) = contexts.get_mut(request.page) else {
            continue;
        };
        if context.pending.is_some()
            || context.endpoint != Some(request.endpoint)
            || context.generation != request.generation
        {
            continue;
        }
        let Ok(endpoint) = endpoints.get(request.endpoint) else {
            continue;
        };
        if !context.loaded || context.configuration != endpoint.fingerprint() {
            for (entity, owner) in &owned {
                if owner.parent() == request.page {
                    commands.entity(entity).despawn();
                }
            }
            context.loaded = false;
            context.revision = 0;
            context.capabilities = Capabilities::default();
        }
        context.configuration = endpoint.fingerprint();
        context.environment = endpoint.environment.clone();
        let operation = commands
            .spawn(Pending::read(request.page, request.endpoint, &context))
            .id();
        if endpoint.writable() {
            commands.entity(operation).insert(JobRequest(native::Load {
                endpoint: endpoint.clone(),
            }));
        } else {
            commands
                .entity(operation)
                .insert(JobRequest(native::Inspect {
                    endpoint: endpoint.clone(),
                }));
        }
        context.pending = Some(operation);
        context.error.clear();
    }
}

#[derive(Component)]
struct PublicationAttempt(crate::administration::Publish);

impl PublicationAttempt {
    fn settled_by(&self, snapshot: &Snapshot) -> bool {
        snapshot
            .history
            .iter()
            .any(|publication| publication.operation_id == self.0.operation_id)
            || snapshot.revision != self.0.base_revision
            || snapshot.draft.as_ref().is_none_or(|draft| {
                draft.id != self.0.draft_id
                    || draft.revision != self.0.expected_revision
                    || draft.base_revision != self.0.base_revision
            })
    }
}

#[derive(SystemParam)]
struct Requested<'w, 's> {
    requests: RequestData<'w, 's>,
}

type RequestData<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static crate::administration::Save>,
        Option<&'static crate::administration::Review>,
        Option<&'static crate::administration::Publish>,
        Option<&'static crate::administration::Discard>,
    ),
>;
type Completions<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Pending,
        &'static JobOwner<J>,
        &'static mut JobResult<J>,
    ),
>;
type OwnedPageValues<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf), Or<(With<Flag>, With<Draft>, With<History>)>>;

impl Requested<'_, '_> {
    fn matches(&self, operation: Entity, snapshot: &Snapshot) -> bool {
        let Ok((save, review, publish, discard)) = self.requests.get(operation) else {
            return false;
        };
        if let Some(request) = save {
            return request.matches(snapshot);
        }
        if let Some(request) = review {
            return request.matches(snapshot);
        }
        if let Some(request) = publish {
            return request.matches(snapshot);
        }
        if let Some(request) = discard {
            return request.matches(snapshot);
        }
        false
    }
}

#[derive(Message)]
struct Received {
    page: Entity,
    endpoint: Entity,
    generation: u64,
    configuration: [u8; 32],
    preserve: bool,
    snapshot: Snapshot,
}

#[derive(SystemParam)]
struct Declarations<'w, 's> {
    endpoints: Query<'w, 's, &'static Endpoint>,
    definitions: Query<'w, 's, (&'static Name, &'static crate::Definition, &'static ChildOf)>,
    providers: Query<'w, 's, &'static Name, With<crate::Provider>>,
}

impl Declarations<'_, '_> {
    fn defaults(&self, environment: &str) -> BTreeMap<String, bool> {
        let mut values = BTreeMap::new();
        for (name, definition, owner) in &self.definitions {
            if self
                .providers
                .get(owner.parent())
                .is_ok_and(|name| name.as_str() == environment)
            {
                values.insert(name.as_str().to_string(), definition.default);
            }
        }
        values
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&PageMetadata>,
    contexts: Query<&Context>,
    endpoints: Query<(Entity, &Endpoint)>,
    declarations: Declarations,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(metadata) = pages.get(page) else {
        return;
    };
    if !metadata
        .url
        .starts_with(crate::registration::FeaturePagePlugin::URL)
        || contexts.contains(page)
    {
        return;
    }
    let endpoint = endpoints
        .iter()
        .find(|(_, endpoint)| endpoint.environment == "development")
        .or_else(|| endpoints.iter().next());
    let mut context = Context::default();
    if let Some((entity, endpoint)) = endpoint {
        context.endpoint = Some(entity);
        commands.write_message(ReadRequested {
            page,
            endpoint: entity,
            generation: context.generation,
        });
        for (name, value) in declarations.defaults(&endpoint.environment) {
            commands.spawn((
                Name::new(name),
                ChildOf(page),
                Flag {
                    published: None,
                    desired: value,
                    saved: value,
                    persisted: false,
                    staged: false,
                },
            ));
        }
    } else {
        context.loaded = true;
    }
    commands.entity(page).insert((
        context,
        Selection {
            tab: "overview".into(),
            ..Default::default()
        },
    ));
}

impl Context {
    fn matches(&self, endpoint: &Endpoint) -> bool {
        self.configuration == endpoint.fingerprint()
    }
}

fn open(
    trigger: On<UiInput<FeaturesOpenRequest>>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<(Entity, &Endpoint)>,
    flags: Query<(&ChildOf, &Flag)>,
    attempts: Query<&PublicationAttempt>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(mut context) = contexts.get_mut(page) else {
        return;
    };
    if context.pending.is_some() {
        return;
    }
    let wanted = &trigger.event().payload.environment;
    let next = endpoints.iter().find(|(entity, endpoint)| {
        if wanted.is_empty() {
            Some(*entity) == context.endpoint
        } else {
            endpoint.environment == *wanted
        }
    });
    let Some((entity, endpoint)) = next else {
        return;
    };
    if Some(entity) != context.endpoint || !context.matches(endpoint) {
        if attempts.contains(page) {
            context.error = "features-publication-pending".into();
            return;
        }
        if flags
            .iter()
            .any(|(owner, flag)| owner.parent() == page && flag.unsaved())
        {
            context.error = "features-unsaved".into();
            return;
        }
        context.generation = context.generation.wrapping_add(1);
        context.endpoint = Some(entity);
        context.loaded = false;
        context.capabilities = Capabilities::default();
    }
    commands.write_message(ReadRequested {
        page,
        endpoint: entity,
        generation: context.generation,
    });
}

fn filter(trigger: On<UiInput<FeaturesFilterRequest>>, mut selections: Query<&mut Selection>) {
    if let Ok(mut selection) = selections.get_mut(trigger.event().webview) {
        selection.query = trigger.event().payload.query.chars().take(128).collect();
    }
}

fn select(trigger: On<UiInput<FeatureSelectRequest>>, mut selections: Query<&mut Selection>) {
    if let Ok(mut selection) = selections.get_mut(trigger.event().webview) {
        let request = &trigger.event().payload;
        if matches!(request.tab.as_str(), "overview" | "changes" | "history") {
            selection.tab = request.tab.clone();
        }
        selection.key = request.key.clone();
    }
}

fn edit(
    trigger: On<UiInput<DraftEditRequest>>,
    mut contexts: Query<&mut Context>,
    attempts: Query<&PublicationAttempt>,
    mut flags: Query<(&Name, &ChildOf, &mut Flag)>,
    drafts: Query<&ChildOf, With<Draft>>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(context) = contexts.get_mut(page) else {
        return;
    };
    if !context.capabilities.edit || context.pending.is_some() || attempts.contains(page) {
        return;
    }
    let request = &trigger.event().payload;
    for (name, owner, mut flag) in &mut flags {
        if owner.parent() == page && name.as_str() == request.key {
            flag.desired = request.enabled;
            flag.staged = true;
            if !drafts.iter().any(|owner| owner.parent() == page) {
                commands.spawn(Draft::local(page, context.revision));
            }
            break;
        }
    }
}

fn save(
    trigger: On<UiInput<DraftSaveRequest>>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    drafts: Query<(&Name, &Revision, &ChildOf, &Draft)>,
    flags: Query<(&Name, &ChildOf, &Flag)>,
    attempts: Query<&PublicationAttempt>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(mut context) = contexts.get_mut(page) else {
        return;
    };
    if !context.capabilities.edit || context.pending.is_some() || attempts.contains(page) {
        return;
    }
    let Some((name, revision, _, draft)) = drafts
        .iter()
        .find(|(_, _, owner, _)| owner.parent() == page)
    else {
        return;
    };
    if revision.0 != trigger.event().payload.expected_revision {
        return;
    }
    let Some(endpoint) = context
        .endpoint
        .and_then(|entity| endpoints.get(entity).ok())
    else {
        return;
    };
    if !context.matches(endpoint) {
        return;
    }
    let mut changes = BTreeMap::new();
    for (name, owner, flag) in &flags {
        if owner.parent() == page && flag.unsaved() {
            changes.insert(name.as_str().to_string(), flag.desired);
        }
    }
    if changes.is_empty() {
        return;
    }
    let request = crate::administration::Save {
        draft_id: name.as_str().into(),
        expected_revision: revision.0,
        base_revision: draft.base,
        changes,
    };
    let operation = commands
        .spawn((
            Pending::write(page, &context),
            JobRequest(native::Save {
                endpoint: endpoint.clone(),
                request: request.clone(),
            }),
        ))
        .id();
    context.pending = Some(operation);
    context.error.clear();
    commands.entity(operation).insert(request);
}

fn review(
    trigger: On<UiInput<DraftReviewRequest>>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    drafts: Query<(&Name, &Revision, &ChildOf, &Draft)>,
    flags: Query<(&ChildOf, &Flag)>,
    attempts: Query<&PublicationAttempt>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(mut context) = contexts.get_mut(page) else {
        return;
    };
    if !context.capabilities.review
        || context.pending.is_some()
        || attempts.contains(page)
        || flags
            .iter()
            .any(|(owner, flag)| owner.parent() == page && flag.unsaved())
    {
        return;
    }
    let request = &trigger.event().payload;
    let Some((name, revision, _, draft)) = drafts
        .iter()
        .find(|(_, _, owner, _)| owner.parent() == page)
    else {
        return;
    };
    if name.as_str() != request.draft_id
        || revision.0 != request.expected_revision
        || draft.base != request.base_revision
    {
        return;
    }
    let Some(endpoint) = context
        .endpoint
        .and_then(|entity| endpoints.get(entity).ok())
    else {
        return;
    };
    if !context.matches(endpoint) {
        return;
    }
    let request = crate::administration::Review {
        draft_id: request.draft_id.clone(),
        expected_revision: request.expected_revision,
        base_revision: request.base_revision,
    };
    let operation = commands
        .spawn((
            Pending::write(page, &context),
            JobRequest(native::Review {
                endpoint: endpoint.clone(),
                request: request.clone(),
            }),
        ))
        .id();
    context.pending = Some(operation);
    context.error.clear();
    commands.entity(operation).insert(request);
}

fn publish(
    trigger: On<UiInput<DraftPublishRequest>>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    drafts: Query<(&Name, &Revision, &ChildOf, &Draft)>,
    flags: Query<(&ChildOf, &Flag)>,
    attempts: Query<&PublicationAttempt>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(mut context) = contexts.get_mut(page) else {
        return;
    };
    if !context.capabilities.publish
        || context.pending.is_some()
        || flags
            .iter()
            .any(|(owner, flag)| owner.parent() == page && flag.unsaved())
    {
        return;
    }
    let request = &trigger.event().payload;
    let Some((name, revision, _, draft)) = drafts
        .iter()
        .find(|(_, _, owner, _)| owner.parent() == page)
    else {
        return;
    };
    if name.as_str() != request.draft_id
        || revision.0 != request.expected_revision
        || draft.base != request.base_revision
        || draft.reviewed != Some(revision.0)
        || context.revision != draft.base
    {
        return;
    }
    let Some(endpoint) = context
        .endpoint
        .and_then(|entity| endpoints.get(entity).ok())
    else {
        return;
    };
    if !context.matches(endpoint) {
        return;
    }
    let publication = match attempts.get(page) {
        Ok(attempt)
            if attempt.0.draft_id == request.draft_id
                && attempt.0.expected_revision == request.expected_revision
                && attempt.0.base_revision == request.base_revision =>
        {
            attempt.0.clone()
        }
        Ok(_) => {
            context.error = "features-publication-pending".into();
            return;
        }
        Err(_) => crate::administration::Publish {
            operation_id: uuid::Uuid::new_v4().to_string(),
            draft_id: request.draft_id.clone(),
            expected_revision: request.expected_revision,
            base_revision: request.base_revision,
        },
    };
    commands
        .entity(page)
        .insert(PublicationAttempt(publication.clone()));
    let operation = commands
        .spawn((
            Pending::write(page, &context),
            JobRequest(native::Publish {
                endpoint: endpoint.clone(),
                request: publication.clone(),
            }),
        ))
        .id();
    context.pending = Some(operation);
    context.error.clear();
    commands.entity(operation).insert(publication);
}

fn discard(
    trigger: On<UiInput<DraftDiscardRequest>>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    drafts: Query<(Entity, &Name, &Revision, &ChildOf), With<Draft>>,
    mut flags: Query<(&ChildOf, &mut Flag)>,
    attempts: Query<&PublicationAttempt>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(mut context) = contexts.get_mut(page) else {
        return;
    };
    if !context.capabilities.edit || context.pending.is_some() || attempts.contains(page) {
        return;
    }
    let request = &trigger.event().payload;
    let Some((entity, name, revision, _)) = drafts
        .iter()
        .find(|(_, _, _, owner)| owner.parent() == page)
    else {
        return;
    };
    if name.as_str() != request.draft_id || revision.0 != request.expected_revision {
        return;
    }
    if revision.0 == 0 {
        for (owner, mut flag) in &mut flags {
            if owner.parent() == page {
                flag.desired = flag.saved;
                flag.staged = false;
            }
        }
        commands.entity(entity).despawn();
        return;
    }
    let Some(endpoint) = context
        .endpoint
        .and_then(|entity| endpoints.get(entity).ok())
    else {
        return;
    };
    if !context.matches(endpoint) {
        return;
    }
    let request = crate::administration::Discard {
        draft_id: request.draft_id.clone(),
        expected_revision: request.expected_revision,
    };
    let operation = commands
        .spawn((
            Pending::write(page, &context),
            JobRequest(native::Discard {
                endpoint: endpoint.clone(),
                request: request.clone(),
            }),
        ))
        .id();
    context.pending = Some(operation);
    context.error.clear();
    commands.entity(operation).insert(request);
}

fn rollback(
    trigger: On<UiInput<FlagRollbackRequest>>,
    contexts: Query<&Context>,
    history: Query<(&ChildOf, &History)>,
    mut flags: Query<(&Name, &ChildOf, &mut Flag)>,
    drafts: Query<&ChildOf, With<Draft>>,
    attempts: Query<&PublicationAttempt>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok(context) = contexts.get(page) else {
        return;
    };
    if !context.capabilities.edit
        || context.pending.is_some()
        || attempts.contains(page)
        || drafts.iter().any(|owner| owner.parent() == page)
    {
        return;
    }
    let Some((_, history)) = history.iter().find(|(owner, history)| {
        owner.parent() == page && history.0.revision == trigger.event().payload.revision
    }) else {
        return;
    };
    for (name, owner, mut flag) in &mut flags {
        if owner.parent() == page
            && let Some(value) = history.0.flags.get(name.as_str())
        {
            flag.desired = *value;
            flag.staged = true;
        }
    }
    commands.spawn(Draft::local(page, context.revision));
}

fn receive<J: Job<Output = Result<Snapshot, String>>>(
    mut results: Completions<'_, '_, J>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    requested: Requested,
    mut received: MessageWriter<Received>,
    mut commands: Commands,
) {
    for (operation, owner, pending, job_owner, mut result) in &mut results {
        let page = owner.parent();
        let Ok(mut context) = contexts.get_mut(page) else {
            commands.entity(operation).despawn();
            continue;
        };
        if !job_owner.accepts(Some(owner))
            || context.pending != Some(operation)
            || context.generation != pending.generation
            || context.endpoint != Some(pending.endpoint)
        {
            commands.entity(operation).despawn();
            continue;
        }
        context.pending = None;
        if !endpoints
            .get(pending.endpoint)
            .is_ok_and(|endpoint| endpoint.fingerprint() == pending.configuration)
        {
            context.capabilities = Capabilities::default();
            context.error = "features-invalid-snapshot".into();
            context.loaded = true;
            commands.entity(operation).despawn();
            continue;
        }
        match std::mem::replace(&mut result.0, Err("receipt consumed".into())) {
            Ok(Ok(snapshot)) if pending.read_only || requested.matches(operation, &snapshot) => {
                received.write(Received {
                    page,
                    endpoint: pending.endpoint,
                    generation: pending.generation,
                    configuration: pending.configuration,
                    preserve: pending.preserve,
                    snapshot,
                });
            }
            Ok(Ok(_)) => {
                context.error = "features-invalid-snapshot".into();
                context.loaded = true;
            }
            Ok(Err(error)) | Err(error) => {
                context.error = error;
                context.loaded = true;
            }
        }
        commands.entity(operation).despawn();
    }
}

fn inspect(
    mut results: Completions<'_, '_, native::Inspect>,
    mut contexts: Query<&mut Context>,
    endpoints: Query<&Endpoint>,
    mut received: MessageWriter<Received>,
    mut commands: Commands,
) {
    for (operation, owner, pending, job_owner, mut result) in &mut results {
        let page = owner.parent();
        let Ok(mut context) = contexts.get_mut(page) else {
            commands.entity(operation).despawn();
            continue;
        };
        if !job_owner.accepts(Some(owner))
            || context.pending != Some(operation)
            || context.generation != pending.generation
            || context.endpoint != Some(pending.endpoint)
        {
            commands.entity(operation).despawn();
            continue;
        }
        context.pending = None;
        if !endpoints
            .get(pending.endpoint)
            .is_ok_and(|endpoint| endpoint.fingerprint() == pending.configuration)
        {
            context.capabilities = Capabilities::default();
            context.error = "features-invalid-snapshot".into();
            context.loaded = true;
            commands.entity(operation).despawn();
            continue;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match std::mem::replace(&mut result.0, Err("receipt consumed".into())) {
            Ok(Ok(document))
                if endpoints
                    .get(pending.endpoint)
                    .is_ok_and(|endpoint| document.is_valid(&endpoint.environment, now)) =>
            {
                received.write(Received {
                    page,
                    endpoint: pending.endpoint,
                    generation: pending.generation,
                    configuration: pending.configuration,
                    preserve: pending.preserve,
                    snapshot: Snapshot {
                        schema_version: 1,
                        environment: document.environment,
                        revision: document.revision,
                        capabilities: Capabilities::default(),
                        flags: document.flags,
                        draft: None,
                        history: Vec::new(),
                    },
                });
            }
            Ok(Err(error)) | Err(error) => {
                context.error = error;
                context.loaded = true;
            }
            _ => {
                context.error = "features-invalid-snapshot".into();
                context.loaded = true;
            }
        }
        commands.entity(operation).despawn();
    }
}

fn adopt(
    mut received: MessageReader<Received>,
    mut contexts: Query<&mut Context>,
    owned: OwnedPageValues<'_, '_>,
    flags: Query<(&Name, &ChildOf, &Flag)>,
    attempts: Query<&PublicationAttempt>,
    declarations: Declarations,
    mut commands: Commands,
) {
    for received in received.read() {
        let Ok(mut context) = contexts.get_mut(received.page) else {
            continue;
        };
        if context.endpoint != Some(received.endpoint) || context.generation != received.generation
        {
            continue;
        }
        if !declarations
            .endpoints
            .get(received.endpoint)
            .is_ok_and(|endpoint| endpoint.fingerprint() == received.configuration)
        {
            continue;
        }
        let snapshot = &received.snapshot;
        let mut local = BTreeMap::new();
        if received.preserve && context.loaded {
            for (name, owner, flag) in &flags {
                if owner.parent() == received.page && flag.unsaved() {
                    local.insert(name.as_str().to_string(), flag.desired);
                }
            }
        }
        for (entity, owner) in &owned {
            if owner.parent() == received.page {
                commands.entity(entity).despawn();
            }
        }
        context.revision = snapshot.revision;
        context.loaded = true;
        context.capabilities = snapshot.capabilities.clone();
        context.error.clear();
        let mut values = snapshot.flags.clone();
        if let Some(draft) = &snapshot.draft {
            values.extend(draft.flags.clone());
        }
        for (name, value) in declarations.defaults(&snapshot.environment) {
            values.entry(name).or_insert(value);
        }
        for (key, saved) in values {
            let desired = local.get(&key).copied().unwrap_or(saved);
            let published = snapshot.flags.get(&key).copied();
            let persisted = published.is_some()
                || snapshot
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.flags.contains_key(&key));
            let staged = local.contains_key(&key)
                || snapshot
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.flags.get(&key).copied() != published);
            commands.spawn((
                Name::new(key),
                ChildOf(received.page),
                Flag {
                    published,
                    saved,
                    desired,
                    persisted,
                    staged,
                },
            ));
        }
        if let Some(draft) = &snapshot.draft {
            commands.spawn((
                Name::new(draft.id.clone()),
                Revision(draft.revision),
                ChildOf(received.page),
                Draft {
                    base: draft.base_revision,
                    reviewed: draft.reviewed_revision,
                },
            ));
        } else if !local.is_empty() {
            commands.spawn(Draft::local(received.page, snapshot.revision));
        }
        for publication in &snapshot.history {
            commands.spawn((
                Name::new(publication.operation_id.clone()),
                ChildOf(received.page),
                History(publication.clone()),
            ));
        }
        if let Ok(attempt) = attempts.get(received.page)
            && attempt.settled_by(snapshot)
        {
            commands
                .entity(received.page)
                .remove::<PublicationAttempt>();
        }
    }
}

fn emit(
    contexts: Query<(Entity, &Context, &Selection)>,
    flags: Query<(&Name, &ChildOf, &Flag)>,
    drafts: Query<(&Name, &Revision, &ChildOf, &Draft)>,
    history: Query<(&ChildOf, &History)>,
    declarations: Declarations,
    mut previous: Local<BTreeMap<Entity, FeaturesUiState>>,
    mut commands: Commands,
) {
    previous.retain(|entity, _| contexts.contains(*entity));
    for (page, context, selection) in &contexts {
        let environment = context.environment.clone();
        let valid = context
            .endpoint
            .and_then(|entity| declarations.endpoints.get(entity).ok())
            .is_some_and(|endpoint| context.matches(endpoint));
        let defaults = declarations.defaults(&environment);
        let mut state = FeaturesUiState {
            environment: environment.clone(),
            environments: declarations
                .endpoints
                .iter()
                .map(|endpoint| endpoint.environment.clone())
                .collect(),
            loaded: context.loaded,
            busy: context.pending.is_some(),
            can_edit: valid && context.capabilities.edit,
            can_review: valid && context.capabilities.review,
            can_publish: valid && context.capabilities.publish,
            revision: context.revision,
            query: selection.query.clone(),
            selected: selection.key.clone(),
            tab: selection.tab.clone(),
            error: context.error.clone(),
            ..Default::default()
        };
        for (name, owner, flag) in &flags {
            if owner.parent() != page {
                continue;
            }
            let default = defaults.get(name.as_str()).copied();
            state.unsaved |= flag.unsaved();
            state.flags.push(FlagRow {
                key: name.as_str().into(),
                published: flag.published,
                proposed: flag.desired,
                default,
                source: if flag.published.is_some() {
                    "published"
                } else {
                    "default"
                }
                .into(),
                changed: flag.staged && Some(flag.desired) != flag.published,
            });
        }
        state.flags.sort_by(|left, right| left.key.cmp(&right.key));
        if let Some((name, revision, _, draft)) = drafts
            .iter()
            .find(|(_, _, owner, _)| owner.parent() == page)
        {
            state.draft_id = name.as_str().into();
            state.draft_revision = revision.0;
            state.base_revision = draft.base;
            state.reviewed = draft.reviewed == Some(revision.0)
                && !state.unsaved
                && context.revision == draft.base;
        }
        let mut entries: Vec<_> = history
            .iter()
            .filter(|(owner, _)| owner.parent() == page)
            .map(|(_, history)| &history.0)
            .collect();
        entries.sort_by_key(|entry| entry.revision);
        let mut prior: BTreeMap<String, bool> = BTreeMap::new();
        for entry in entries {
            let mut changes = Vec::new();
            for (key, value) in &entry.flags {
                let before = prior.get(key).copied();
                if before != Some(*value) {
                    changes.push(ChangeRow {
                        key: key.clone(),
                        before,
                        after: *value,
                    });
                }
            }
            state.history.push(HistoryRow {
                revision: entry.revision,
                actor: entry.actor.clone(),
                created_at: entry.created_at,
                changes,
            });
            prior = entry.flags.clone();
        }
        if previous.get(&page) != Some(&state) {
            commands.trigger(UiStateWrite::<FeaturesUiState>::from_event(page, &state));
            previous.insert(page, state);
        }
    }
}

impl Flag {
    fn unsaved(&self) -> bool {
        self.desired != self.saved || (!self.persisted && self.staged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    struct Server {
        url: String,
        started: mpsc::Receiver<()>,
        release: mpsc::Sender<()>,
        worker: Option<JoinHandle<()>>,
    }

    impl Server {
        fn new() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let (send, started) = mpsc::channel();
            let (release, receive) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                    assert!(header.len() < 8192);
                }
                let header_text = String::from_utf8(header).unwrap();
                let length = header_text
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                    .unwrap_or(0);
                assert!(length <= 65536);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                send.send(()).unwrap();
                receive.recv_timeout(Duration::from_secs(5)).unwrap();
                let body=br#"{"schema_version":1,"environment":"development","revision":1,"capabilities":{"edit":true,"review":true,"publish":true},"flags":{"published.flag":true},"draft":null,"history":[]}"#;
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(body).unwrap();
            });
            Self {
                url,
                started,
                release,
                worker: Some(worker),
            }
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.release.send(());
            if let Some(worker) = self.worker.take() {
                worker.join().unwrap();
            }
        }
    }

    struct Fixture {
        app: App,
        page: Entity,
        endpoint: Entity,
    }

    impl Fixture {
        fn new(server: &Server) -> Self {
            let mut app = App::new();
            app.add_plugins(FeatureHostPlugin);
            let endpoint = Endpoint::new(
                &server.url,
                "development",
                Some("fixture-credential".into()),
            )
            .unwrap();
            let owner = app
                .world_mut()
                .spawn((Name::new("development"), crate::Provider, endpoint.clone()))
                .id();
            let page = app
                .world_mut()
                .spawn((
                    Name::new("Flags test page"),
                    Context::default(),
                    Selection::default(),
                ))
                .id();
            let mut commands = app.world_mut().commands();
            let mut context = Context {
                endpoint: Some(owner),
                ..Default::default()
            };
            context.configuration = endpoint.fingerprint();
            context.environment = endpoint.environment.clone();
            let operation = commands
                .spawn((
                    Pending::read(page, owner, &context),
                    JobRequest(native::Load {
                        endpoint: endpoint.clone(),
                    }),
                ))
                .id();
            context.pending = Some(operation);
            commands.entity(page).insert(context);
            app.world_mut().flush();
            Self {
                app,
                page,
                endpoint: owner,
            }
        }

        fn wait_for_request(&mut self, server: &Server) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if server.started.try_recv().is_ok() {
                    return;
                }
                assert!(Instant::now() < deadline, "native request did not start");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn drain(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self
                .app
                .world()
                .get::<Context>(self.page)
                .unwrap()
                .pending
                .is_some()
            {
                self.app.update();
                assert!(Instant::now() < deadline, "native receipt did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn flags(&mut self) -> Vec<String> {
            let mut flags = self.app.world_mut().query::<(&Name, &ChildOf, &Flag)>();
            let mut names = Vec::new();
            for (name, owner, _) in flags.iter(self.app.world()) {
                if owner.parent() == self.page {
                    names.push(name.as_str().to_string());
                }
            }
            names.sort();
            names
        }
    }

    #[test]
    fn changed_credentials_and_namespace_reject_a_late_administration_response() {
        let server = Server::new();
        let mut fixture = Fixture::new(&server);
        fixture.wait_for_request(&server);
        fixture.app.world_mut().entity_mut(fixture.endpoint).insert(
            Endpoint::new(&server.url, "other", Some("replacement-credential".into())).unwrap(),
        );
        server.release.send(()).unwrap();
        fixture.drain();
        assert!(fixture.flags().is_empty());
        assert!(
            !fixture
                .app
                .world()
                .get::<Context>(fixture.page)
                .unwrap()
                .capabilities
                .publish
        );
    }

    #[test]
    fn inspector_includes_published_and_declared_flags_without_staging_defaults() {
        let server = Server::new();
        let mut fixture = Fixture::new(&server);
        fixture.app.world_mut().spawn(crate::Definition::bundle(
            fixture.endpoint,
            "declared.flag",
            false,
        ));
        fixture.wait_for_request(&server);
        server.release.send(()).unwrap();
        fixture.drain();
        assert_eq!(fixture.flags(), ["declared.flag", "published.flag"]);
        let mut flags = fixture.app.world_mut().query::<(&Name, &Flag)>();
        let declared = flags
            .iter(fixture.app.world())
            .find(|(name, _)| name.as_str() == "declared.flag")
            .unwrap()
            .1;
        assert!(!declared.unsaved());
    }

    #[test]
    fn failed_environment_switch_cannot_display_the_previous_environment_values() {
        let server = Server::new();
        let mut fixture = Fixture::new(&server);
        fixture.wait_for_request(&server);
        server.release.send(()).unwrap();
        fixture.drain();
        assert_eq!(fixture.flags(), ["published.flag"]);
        let other = Server::new();
        fixture
            .app
            .world_mut()
            .spawn(Endpoint::new(&other.url, "other", Some("other-credential".into())).unwrap());
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.page,
            payload: FeaturesOpenRequest {
                environment: "other".into(),
            },
        });
        fixture.wait_for_request(&other);
        other.release.send(()).unwrap();
        fixture.drain();
        let context = fixture.app.world().get::<Context>(fixture.page).unwrap();
        assert_eq!(context.environment, "other");
        assert!(!context.error.is_empty());
        assert!(!context.capabilities.publish);
        assert!(fixture.flags().is_empty());
    }

    #[test]
    fn publication_refresh_unblocks_edits_only_after_the_reviewed_draft_is_retired() {
        for (draft_id, revision, editable) in [
            (Some("reviewed"), 2, false),
            (Some("reviewed"), 3, true),
            (Some("replacement"), 2, true),
            (None, 0, true),
        ] {
            let server = Server::new();
            let mut fixture = Fixture::new(&server);
            fixture.wait_for_request(&server);
            server.release.send(()).unwrap();
            fixture.drain();
            let operation_id = uuid::Uuid::new_v4().to_string();
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.page)
                .insert(PublicationAttempt(crate::administration::Publish {
                    operation_id: operation_id.clone(),
                    draft_id: "reviewed".into(),
                    expected_revision: 2,
                    base_revision: 1,
                }));
            let configuration = fixture
                .app
                .world()
                .get::<Context>(fixture.page)
                .unwrap()
                .configuration;
            fixture.app.world_mut().write_message(Received {
                page: fixture.page,
                endpoint: fixture.endpoint,
                generation: 0,
                configuration,
                preserve: true,
                snapshot: Snapshot {
                    schema_version: 1,
                    environment: "development".into(),
                    revision: 1,
                    capabilities: Capabilities {
                        edit: true,
                        review: true,
                        publish: true,
                    },
                    flags: BTreeMap::from([("published.flag".into(), true)]),
                    draft: draft_id.map(|id| crate::administration::Draft {
                        id: id.into(),
                        revision,
                        base_revision: 1,
                        reviewed_revision: None,
                        flags: BTreeMap::from([("published.flag".into(), true)]),
                    }),
                    history: Vec::new(),
                },
            });
            fixture.app.update();
            fixture.app.world_mut().trigger(UiInput {
                webview: fixture.page,
                payload: DraftEditRequest {
                    key: "published.flag".into(),
                    enabled: false,
                },
            });
            let world = fixture.app.world_mut();
            let flag = world.query::<&Flag>().single(world).unwrap();
            assert_eq!(flag.unsaved(), editable);
            if !editable {
                assert_eq!(
                    world
                        .get::<PublicationAttempt>(fixture.page)
                        .unwrap()
                        .0
                        .operation_id,
                    operation_id
                );
            }
        }
    }

    #[test]
    fn an_unrelated_snapshot_cannot_acknowledge_a_saved_draft() {
        let server = Server::new();
        let mut fixture = Fixture::new(&server);
        let old = fixture
            .app
            .world()
            .get::<Context>(fixture.page)
            .unwrap()
            .pending
            .unwrap();
        fixture.app.world_mut().despawn(old);
        let endpoint = fixture
            .app
            .world()
            .get::<Endpoint>(fixture.endpoint)
            .unwrap()
            .clone();
        let request = crate::administration::Save {
            draft_id: "intended-draft".into(),
            expected_revision: 0,
            base_revision: 1,
            changes: BTreeMap::from([("published.flag".into(), false)]),
        };
        let mut context = fixture
            .app
            .world_mut()
            .entity_mut(fixture.page)
            .take::<Context>()
            .unwrap();
        let mut commands = fixture.app.world_mut().commands();
        let operation = commands
            .spawn((
                Pending::write(fixture.page, &context),
                JobRequest(native::Save {
                    endpoint,
                    request: request.clone(),
                }),
            ))
            .id();
        context.pending = Some(operation);
        commands.entity(operation).insert(request);
        commands.entity(fixture.page).insert(context);
        fixture.app.world_mut().flush();
        fixture.wait_for_request(&server);
        server.release.send(()).unwrap();
        fixture.drain();
        assert!(fixture.flags().is_empty());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Context>(fixture.page)
                .unwrap()
                .error,
            "features-invalid-snapshot"
        );
    }
}
