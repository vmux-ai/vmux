use std::collections::{HashMap, HashSet, VecDeque};
use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_cef::prelude::{
    BinHostEmitEvent, BinIpcBlocked, BinIpcEpoch, BinIpcNavigationRevision,
    BinIpcSurfaceGeneration, Browsers,
};
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::{UiEventPermissions, UiState as UiStateContract, UiStateFrame};

pub struct UiStatePlugin<S> {
    post_update: bool,
    state: PhantomData<fn() -> S>,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiStateFlushSet;

impl<S> UiStatePlugin<S> {
    pub fn with_post_update_flush() -> Self {
        Self {
            post_update: true,
            state: PhantomData,
        }
    }
}

impl<S> Default for UiStatePlugin<S> {
    fn default() -> Self {
        Self {
            post_update: false,
            state: PhantomData,
        }
    }
}

impl<S> Plugin for UiStatePlugin<S>
where
    S: UiStateContract,
    UiStateFrame<S, S::Update>:
        for<'a> rkyv::Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<UiStateCache<S>>()
            .init_resource::<UiStateDirty<S>>()
            .add_observer(collect::<S>)
            .add_observer(replay::<S>)
            .add_observer(replay_activated::<S>)
            .add_observer(retry_unblocked::<S>)
            .add_observer(retry_permission::<S>)
            .add_observer(cache_removed::<S>)
            .add_systems(Last, emit::<S>);
        if self.post_update {
            app.add_systems(
                PostUpdate,
                (emit::<S>, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(UiStateFlushSet),
            );
        }
    }
}

#[derive(Resource)]
struct UiStateDirty<S: UiStateContract> {
    entities: HashSet<Entity>,
    state: PhantomData<fn() -> S>,
}

impl<S: UiStateContract> Default for UiStateDirty<S> {
    fn default() -> Self {
        Self {
            entities: HashSet::new(),
            state: PhantomData,
        }
    }
}

impl<S: UiStateContract> UiStateDirty<S> {
    fn mark(&mut self, entity: Entity) {
        self.entities.insert(entity);
    }
}

#[derive(Resource)]
struct UiStateCache<S: UiStateContract> {
    states: HashMap<String, S>,
    order: VecDeque<String>,
}

impl<S: UiStateContract> Default for UiStateCache<S> {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl<S: UiStateContract> UiStateCache<S> {
    const CAPACITY: usize = 8;

    fn key(metadata: &crate::PageMetadata) -> Option<String> {
        if metadata.url.is_empty() {
            return None;
        }
        Some(
            vmux_api::VmuxRoute::canonical(metadata.url.split('#').next().unwrap_or(&metadata.url))
                .unwrap_or_else(|| {
                    metadata
                        .url
                        .split('#')
                        .next()
                        .unwrap_or(&metadata.url)
                        .trim()
                        .to_string()
                }),
        )
    }

    fn get(&mut self, key: &str) -> Option<S> {
        let state = self.states.get(key)?.clone();
        self.touch(key.to_string());
        Some(state)
    }

    fn put(&mut self, key: String, state: S) {
        self.states.insert(key.clone(), state);
        self.touch(key);
        while self.order.len() > Self::CAPACITY {
            if let Some(expired) = self.order.pop_front() {
                self.states.remove(&expired);
            }
        }
    }

    fn touch(&mut self, key: String) {
        self.order.retain(|candidate| candidate != &key);
        self.order.push_back(key);
    }
}

#[derive(Component)]
pub struct UiState<S: UiStateContract> {
    updates: Vec<S::Update>,
    retained: Option<S>,
    pending: Option<BinHostEmitEvent>,
    pending_snapshot: bool,
    replay: bool,
    synced: bool,
    navigation_pending: bool,
    key: Option<String>,
    authority: Option<(u64, u64, u64)>,
    delivery_attempts: u8,
    state: PhantomData<fn() -> S>,
}

impl<S: UiStateContract> Default for UiState<S> {
    fn default() -> Self {
        Self {
            updates: Vec::new(),
            retained: None,
            pending: None,
            pending_snapshot: false,
            replay: false,
            synced: false,
            navigation_pending: false,
            key: None,
            authority: None,
            delivery_attempts: 0,
            state: PhantomData,
        }
    }
}

impl<S: UiStateContract> UiState<S> {
    pub fn current(&self) -> Option<&S> {
        self.retained.as_ref()
    }

    fn restore(&mut self, state: Option<S>) {
        self.updates.clear();
        self.retained = state;
        self.pending = None;
        self.pending_snapshot = false;
        self.replay = true;
        self.synced = false;
        self.navigation_pending = false;
        self.authority = None;
        self.delivery_attempts = 0;
    }

    fn replace(&mut self, key: Option<String>, state: Option<S>) {
        self.key = key;
        self.restore(state);
    }

    fn bind(&mut self, key: Option<String>, cache: &mut UiStateCache<S>) {
        if self.key == key {
            if self.retained.is_none()
                && let Some(key) = key.as_deref()
            {
                self.retained = cache.get(key);
            }
            return;
        }
        let previous_key = self.key.take();
        let mut navigation_seed = None;
        if let Some(previous_key) = previous_key.as_ref() {
            let pending = std::mem::take(&mut self.updates);
            let retained = if pending.is_empty() {
                self.retained.take()
            } else {
                Some(S::from_updates(self.retained.take(), pending))
            };
            if let Some(mut retained) = retained {
                retained.retain();
                navigation_seed = retained.navigation_seed();
                cache.put(previous_key.clone(), retained);
            }
        }
        self.key = key;
        if previous_key.is_some() || self.retained.is_none() {
            self.retained = self.key.as_deref().and_then(|key| cache.get(key));
        }
        self.navigation_pending = false;
        if self.retained.is_none()
            && let Some(seed) = navigation_seed
        {
            self.navigation_pending = !seed.navigation_ready();
            self.retained = Some(seed);
        }
        self.pending = None;
        self.pending_snapshot = false;
        self.synced = false;
        self.authority = None;
        self.delivery_attempts = 0;
    }

    fn push(&mut self, key: Option<String>, update: S::Update, cache: &mut UiStateCache<S>) {
        self.bind(key, cache);
        self.updates.push(update);
        self.delivery_attempts = 0;
    }

    fn finish_delivery(&mut self, authority: (u64, u64, u64), delivered: bool) -> bool {
        if self.authority != Some(authority) {
            return false;
        }
        if delivered {
            self.pending = None;
            if self.pending_snapshot {
                self.replay = false;
                self.synced = true;
            }
            self.pending_snapshot = false;
            self.delivery_attempts = 0;
            return !self.updates.is_empty();
        }
        self.delivery_attempts = self.delivery_attempts.saturating_add(1);
        self.delivery_attempts < 3
    }
}

struct UiStateDelivery<S> {
    entity: Entity,
    authority: (u64, u64, u64),
    event: BinHostEmitEvent,
    state: PhantomData<fn() -> S>,
}

impl<S: UiStateContract> Command for UiStateDelivery<S> {
    type Out = ();

    fn apply(self, world: &mut World) {
        let receipt = self.event.receipt();
        world.trigger(self.event);
        let retry = world
            .get_mut::<UiState<S>>(self.entity)
            .is_some_and(|mut state| state.finish_delivery(self.authority, receipt.delivered()));
        if retry {
            world.resource_mut::<UiStateDirty<S>>().mark(self.entity);
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct UiStateWriter<'w, 's, S: UiStateContract> {
    metadata: Query<'w, 's, &'static crate::PageMetadata>,
    updates: Query<'w, 's, &'static mut UiState<S>>,
    cache: ResMut<'w, UiStateCache<S>>,
    dirty: ResMut<'w, UiStateDirty<S>>,
    commands: Commands<'w, 's>,
}

impl<S: UiStateContract> UiStateWriter<'_, '_, S> {
    pub fn push<T>(&mut self, webview: Entity, update: T)
    where
        T: Into<S::Update>,
    {
        let key = self
            .metadata
            .get(webview)
            .ok()
            .and_then(UiStateCache::<S>::key);
        let update = update.into();
        match self.updates.get_mut(webview) {
            Ok(mut updates) => {
                updates.push(key, update, &mut self.cache);
                self.dirty.mark(webview);
            }
            Err(_) => {
                self.commands.queue(move |world: &mut World| {
                    if world.get_entity(webview).is_err() {
                        return;
                    }
                    world.resource_scope(|world, mut cache: Mut<UiStateCache<S>>| {
                        if let Some(mut updates) = world.get_mut::<UiState<S>>(webview) {
                            updates.push(key, update, &mut cache);
                            return;
                        }
                        let mut updates = UiState::<S>::default();
                        updates.push(key, update, &mut cache);
                        world.entity_mut(webview).insert(updates);
                    });
                    world.resource_mut::<UiStateDirty<S>>().mark(webview);
                });
            }
        }
    }

    pub fn restore(
        &mut self,
        webview: Entity,
        metadata: Option<&crate::PageMetadata>,
        state: Option<S>,
    ) {
        let key = metadata.and_then(UiStateCache::<S>::key);
        match self.updates.get_mut(webview) {
            Ok(mut updates) => {
                updates.replace(key, state);
                self.dirty.mark(webview);
            }
            Err(_) => {
                self.commands.queue(move |world: &mut World| {
                    if world.get_entity(webview).is_err() {
                        return;
                    }
                    if let Some(mut updates) = world.get_mut::<UiState<S>>(webview) {
                        updates.replace(key, state);
                    } else {
                        let mut updates = UiState::<S>::default();
                        updates.replace(key, state);
                        world.entity_mut(webview).insert(updates);
                    }
                    world.resource_mut::<UiStateDirty<S>>().mark(webview);
                });
            }
        }
    }
}

fn collect<S: UiStateContract>(trigger: On<UiStateWrite<S>>, mut writer: UiStateWriter<S>) {
    writer.push(trigger.event().webview, trigger.event().update.clone());
}

fn replay<S: UiStateContract>(
    trigger: On<bevy_cef::prelude::UiInput<vmux_api::PageReady>>,
    metadata: Query<&crate::PageMetadata>,
    mut cache: ResMut<UiStateCache<S>>,
    mut dirty: ResMut<UiStateDirty<S>>,
    mut updates: Query<&mut UiState<S>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;

    let key = metadata.get(entity).ok().and_then(UiStateCache::<S>::key);
    if let Ok(mut updates) = updates.get_mut(entity) {
        updates.bind(key, &mut cache);
        if !updates.pending_snapshot {
            updates.pending = None;
        }
        updates.replay = true;
        updates.synced = false;
        dirty.mark(entity);
        return;
    }
    let Some(key) = key else {
        return;
    };
    let Some(state) = cache.get(&key) else {
        return;
    };
    let mut updates = UiState::<S> {
        key: Some(key),
        ..Default::default()
    };
    updates.restore(Some(state));
    commands.entity(entity).try_insert(updates);
    dirty.mark(entity);
}

fn replay_activated<S: UiStateContract>(
    trigger: On<crate::page::PageActivated>,
    metadata: Query<&crate::PageMetadata>,
    mut cache: ResMut<UiStateCache<S>>,
    mut dirty: ResMut<UiStateDirty<S>>,
    mut updates: Query<&mut UiState<S>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview();
    let key = metadata.get(entity).ok().and_then(UiStateCache::<S>::key);
    if let Ok(mut updates) = updates.get_mut(entity) {
        updates.bind(key, &mut cache);
        if !updates.synced {
            updates.replay = true;
            dirty.mark(entity);
        }
        return;
    }
    let Some(key) = key else {
        return;
    };
    let Some(state) = cache.get(&key) else {
        return;
    };
    let mut updates = UiState::<S> {
        key: Some(key),
        ..Default::default()
    };
    updates.restore(Some(state));
    commands.entity(entity).try_insert(updates);
    dirty.mark(entity);
}

fn retry_unblocked<S: UiStateContract>(
    trigger: On<Remove, BinIpcBlocked>,
    states: Query<(), With<UiState<S>>>,
    mut dirty: ResMut<UiStateDirty<S>>,
) {
    let entity = trigger.event_target();
    if states.contains(entity) {
        dirty.mark(entity);
    }
}

fn retry_permission<S: UiStateContract>(
    trigger: On<Add, UiEventPermissions>,
    permissions: Query<&UiEventPermissions>,
    states: Query<(Entity, &crate::PageMetadata), With<UiState<S>>>,
    mut dirty: ResMut<UiStateDirty<S>>,
) {
    let Ok(permission) = permissions.get(trigger.event_target()) else {
        return;
    };
    for (entity, metadata) in &states {
        if UiEventPermissions::allows_page(
            std::iter::once(permission),
            &metadata.url,
            S::PERMISSION,
        ) {
            dirty.mark(entity);
        }
    }
}

fn cache_removed<S: UiStateContract>(
    trigger: On<Remove, UiState<S>>,
    mut states: Query<&mut UiState<S>>,
    mut cache: ResMut<UiStateCache<S>>,
) {
    let Ok(mut state) = states.get_mut(trigger.event_target()) else {
        return;
    };
    let Some(key) = state.key.take() else {
        return;
    };
    let pending = std::mem::take(&mut state.updates);
    let retained = if pending.is_empty() {
        state.retained.take()
    } else {
        Some(S::from_updates(state.retained.take(), pending))
    };
    let Some(mut retained) = retained else {
        return;
    };
    retained.retain();
    cache.put(key, retained);
}

#[derive(bevy::ecs::system::SystemParam)]
struct UiStateStore<'w, S: UiStateContract> {
    cache: ResMut<'w, UiStateCache<S>>,
    dirty: ResMut<'w, UiStateDirty<S>>,
}

fn emit<S>(
    mut updates: Query<&mut UiState<S>>,
    authority: Query<(
        &BinIpcSurfaceGeneration,
        &BinIpcNavigationRevision,
        &BinIpcEpoch,
        &crate::PageMetadata,
    )>,
    blocked: Query<(), With<BinIpcBlocked>>,
    permissions: Query<&UiEventPermissions>,
    mut store: UiStateStore<S>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) where
    S: UiStateContract,
    UiStateFrame<S, S::Update>:
        for<'a> rkyv::Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    let Some(browsers) = browsers else {
        return;
    };
    let entities = std::mem::take(&mut store.dirty.entities);
    for entity in entities {
        let Ok(mut updates) = updates.get_mut(entity) else {
            continue;
        };
        let Ok((surface_generation, navigation_revision, ipc_epoch, metadata)) =
            authority.get(entity)
        else {
            continue;
        };
        let updates = updates.bypass_change_detection();
        updates.bind(UiStateCache::<S>::key(metadata), &mut store.cache);

        if surface_generation.0 == 0
            || navigation_revision.0 == 0
            || ipc_epoch.0 == 0
            || !UiEventPermissions::allows_page(permissions.iter(), &metadata.url, S::PERMISSION)
        {
            continue;
        }
        let authority = (surface_generation.0, navigation_revision.0, ipc_epoch.0);
        if updates.authority != Some(authority) {
            updates.pending = None;
            updates.pending_snapshot = false;
            updates.replay = updates.retained.is_some();
            updates.synced = false;
            updates.authority = Some(authority);
            updates.delivery_attempts = 0;
        }
        if updates.pending.is_none() {
            let pending = std::mem::take(&mut updates.updates);
            let send_snapshot = updates.replay || !updates.synced;
            if pending.is_empty() {
                if !send_snapshot {
                    continue;
                }
                let Some(retained) = updates.retained.as_ref() else {
                    continue;
                };
                if updates.navigation_pending && !retained.navigation_ready() {
                    continue;
                }
                let frame = UiStateFrame::<S, S::Update>::Snapshot(retained.clone());
                updates.navigation_pending = false;
                updates.pending = Some(BinHostEmitEvent::from_payload_at::<S, _>(
                    entity, authority, &frame,
                ));
                updates.pending_snapshot = true;
            } else if send_snapshot {
                let mut state = S::from_updates(updates.retained.take(), pending);
                if updates.navigation_pending && !state.navigation_ready() {
                    state.retain();
                    updates.retained = Some(state);
                    continue;
                }
                updates.navigation_pending = false;
                let frame = UiStateFrame::<S, S::Update>::Snapshot(state.clone());
                updates.pending = Some(BinHostEmitEvent::from_payload_at::<S, _>(
                    entity, authority, &frame,
                ));
                updates.pending_snapshot = true;
                state.retain();
                updates.retained = Some(state);
            } else {
                let frame = UiStateFrame::<S, S::Update>::Updates(pending);
                updates.pending = Some(BinHostEmitEvent::from_payload_at::<S, _>(
                    entity, authority, &frame,
                ));
                let UiStateFrame::Updates(pending) = frame else {
                    unreachable!();
                };
                let mut state = S::from_updates(updates.retained.take(), pending);
                state.retain();
                updates.retained = Some(state);
            }
        }
        if blocked.contains(entity) || !browsers.can_emit_to(&entity) {
            continue;
        }
        if let Some(event) = updates.pending.clone() {
            commands.queue(UiStateDelivery::<S> {
                entity,
                authority,
                event,
                state: PhantomData,
            });
        }
    }
}

#[derive(Clone, EntityEvent)]
pub struct UiStateWrite<S: UiStateContract> {
    #[event_target]
    webview: Entity,
    update: S::Update,
}

impl<S: UiStateContract> UiStateWrite<S> {
    pub fn from_event<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<S::Update>,
    {
        Self {
            webview,
            update: event.clone().into(),
        }
    }

    pub fn webview(&self) -> Entity {
        self.webview
    }

    pub fn update(&self) -> &S::Update {
        &self.update
    }
}

#[cfg(test)]
mod tests {
    use crate::event::{
        FileDocumentKind, FileMetaEvent, FileThemeEvent, UiFilePatch, FileViewportPatch,
    };

    use super::*;
    use crate::event::{FileDirtyEvent, UiFile};
    use bevy_cef::prelude::UiInput;
    use vmux_api::git::FileGitState;
    use vmux_api::{BinEvent, UiEventPermissions};

    type FileFrame = UiStateFrame<UiFile, <UiFile as UiStateContract>::Update>;

    struct ReadyPage;

    impl ReadyPage {
        fn identify(app: &mut App, entity: Entity, url: &'static str) {
            app.world_mut().entity_mut(entity).insert((
                BinIpcSurfaceGeneration(1),
                BinIpcNavigationRevision(1),
                BinIpcEpoch(1),
            ));
            if app.world().get::<crate::PageMetadata>(entity).is_none() {
                app.world_mut()
                    .entity_mut(entity)
                    .insert(crate::PageMetadata {
                        url: url.to_string(),
                        ..Default::default()
                    });
            }
        }

        fn allow<S: UiStateContract>(app: &mut App, url: &'static str) {
            app.world_mut().spawn(UiEventPermissions {
                url,
                owns_subtree: false,
                permissions: &[S::PERMISSION],
            });
        }

        fn authorize<S: UiStateContract>(app: &mut App, entity: Entity, url: &'static str) {
            Self::identify(app, entity, url);
            Self::allow::<S>(app, url);
        }
    }

    #[derive(Resource, Default)]
    struct Emitted {
        frames: Vec<FileFrame>,
        state: UiFile,
    }

    #[derive(Resource)]
    struct Targets {
        file: Entity,
        direct: Entity,
    }

    #[derive(Resource, Default)]
    struct Delivered {
        writes: Vec<Entity>,
    }

    #[vmux_api::ui_state(Default, Eq)]
    struct SnapshotState {
        value: u32,
    }

    #[test]
    fn fragment_changes_keep_the_retained_page_snapshot() {
        let mut cache = UiStateCache::<SnapshotState>::default();
        let mut metadata = crate::PageMetadata {
            url: "git://repository#diff-first".to_string(),
            ..Default::default()
        };
        let mut state = UiState::<SnapshotState>::default();
        state.replace(
            UiStateCache::<SnapshotState>::key(&metadata),
            Some(SnapshotState { value: 42 }),
        );
        metadata.url = "git://repository#diff-second".to_string();
        state.bind(UiStateCache::<SnapshotState>::key(&metadata), &mut cache);
        assert_eq!(state.current(), Some(&SnapshotState { value: 42 }));
    }

    #[derive(Resource, Default)]
    struct SnapshotEmitted {
        frames: Vec<UiStateFrame<SnapshotState, SnapshotState>>,
        state: SnapshotState,
    }

    #[vmux_api::ui_state_patch(Default)]
    struct PatchStatePatch {
        first: Option<u32>,
        second: Option<String>,
    }

    #[vmux_api::ui_state(Default, Eq, patch = PatchStatePatch)]
    struct PatchState {
        first: u32,
        second: String,
    }

    impl vmux_api::UiStateProjection<PatchStatePatch> for PatchState {
        fn apply(&mut self, patch: PatchStatePatch) {
            if let Some(first) = patch.first {
                self.first = first;
            }
            if let Some(second) = patch.second {
                self.second = second;
            }
        }
    }

    #[vmux_api::ui_state_patch(Default)]
    struct TransientStatePatch {
        value: Option<u32>,
        reveal: Option<String>,
    }

    #[vmux_api::ui_state(Default, Eq, patch = TransientStatePatch)]
    struct TransientState {
        value: u32,
        reveal: Option<String>,
    }

    impl vmux_api::UiStateProjection<TransientStatePatch> for TransientState {
        fn apply(&mut self, patch: TransientStatePatch) {
            if let Some(value) = patch.value {
                self.value = value;
            }
            if let Some(reveal) = patch.reveal {
                self.reveal = Some(reveal);
            }
        }

        fn retain(&mut self) {
            self.reveal = None;
        }
    }

    #[derive(Resource, Default)]
    struct TransientDelivery {
        accept: bool,
        frames: Vec<UiStateFrame<TransientState, TransientStatePatch>>,
    }

    #[derive(Resource, Default)]
    struct PatchEmitted(Vec<PatchState>);

    #[derive(Resource, Default)]
    struct RejectedEmitted(usize);

    #[derive(Resource)]
    struct PatchTarget(Entity);

    #[derive(Resource)]
    struct RestoreTarget(Entity);

    fn record_file_state(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Emitted>) {
        if trigger.event().id() != UiFile::id() {
            return;
        }
        trigger.event().mark_delivered();
        let frame =
            rkyv::from_bytes::<FileFrame, rkyv::rancor::Error>(trigger.event().payload()).unwrap();
        match &frame {
            UiStateFrame::Snapshot(state) => emitted.state = state.clone(),
            UiStateFrame::Updates(updates) => {
                emitted.state = UiFile::from_updates(
                    Some(std::mem::take(&mut emitted.state)),
                    updates.clone(),
                );
            }
        }
        emitted.frames.push(frame);
    }

    fn record_write(trigger: On<UiStateWrite<UiFile>>, mut delivered: ResMut<Delivered>) {
        delivered.writes.push(trigger.event().webview());
    }

    fn record_snapshot(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<SnapshotEmitted>) {
        if trigger.event().id() != SnapshotState::id() {
            return;
        }
        trigger.event().mark_delivered();
        let frame = rkyv::from_bytes::<
            UiStateFrame<SnapshotState, SnapshotState>,
            rkyv::rancor::Error,
        >(trigger.event().payload())
        .unwrap();
        match &frame {
            UiStateFrame::Snapshot(state) => emitted.state = state.clone(),
            UiStateFrame::Updates(updates) => {
                emitted.state = SnapshotState::from_updates(
                    Some(std::mem::take(&mut emitted.state)),
                    updates.clone(),
                );
            }
        }
        emitted.frames.push(frame);
    }

    fn record_patch_state(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<PatchEmitted>) {
        if trigger.event().id() != PatchState::id() {
            return;
        }
        trigger.event().mark_delivered();
        let frame = rkyv::from_bytes::<
            UiStateFrame<PatchState, PatchStatePatch>,
            rkyv::rancor::Error,
        >(trigger.event().payload())
        .unwrap();
        let previous = emitted.0.pop();
        let state = match frame {
            UiStateFrame::Snapshot(state) => state,
            UiStateFrame::Updates(updates) => PatchState::from_updates(previous, updates),
        };
        emitted.0.push(state);
    }

    fn reject_snapshot(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<RejectedEmitted>) {
        if trigger.event().id() == SnapshotState::id() {
            emitted.0 += 1;
        }
    }

    fn record_transient(trigger: On<BinHostEmitEvent>, mut delivery: ResMut<TransientDelivery>) {
        if trigger.event().id() != TransientState::id() {
            return;
        }
        let frame = rkyv::from_bytes::<
            UiStateFrame<TransientState, TransientStatePatch>,
            rkyv::rancor::Error,
        >(trigger.event().payload())
        .unwrap();
        if delivery.accept {
            trigger.event().mark_delivered();
        }
        delivery.frames.push(frame);
    }

    fn push_patch_state(target: Res<PatchTarget>, mut writer: UiStateWriter<PatchState>) {
        writer.push(target.0, 7u32);
        writer.push(target.0, String::from("ready"));
    }

    fn restore_patch_state(target: Res<RestoreTarget>, mut writer: UiStateWriter<PatchState>) {
        writer.restore(
            target.0,
            Some(&crate::PageMetadata {
                url: "vmux://previous/".into(),
                ..Default::default()
            }),
            Some(PatchState {
                first: 7,
                second: "ready".into(),
            }),
        );
    }

    fn deliver(
        targets: Res<Targets>,
        pages: Query<(), With<UiState<UiFile>>>,
        mut commands: Commands,
    ) {
        for target in [targets.file, targets.direct] {
            if pages.contains(target) {
                commands.trigger(UiStateWrite::<UiFile>::from_event(
                    target,
                    &FileGitState::default(),
                ));
            }
        }
    }

    #[test]
    fn cold_file_navigation_keeps_the_old_document_until_content_and_preserves_the_editor_theme() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<UiFile>::default()))
            .init_resource::<Emitted>()
            .add_observer(record_file_state);
        let entity = app
            .world_mut()
            .spawn((
                UiState::<UiFile>::default(),
                crate::PageMetadata {
                    url: "file:///first.rs".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<UiFile>(&mut app, entity, "file:///first.rs");
        ReadyPage::allow::<UiFile>(&mut app, "file:///second.rs");
        let meta = |path: &str, revision| FileMetaEvent {
            revision,
            path: path.to_string(),
            abs_path: path.to_string(),
            kind: FileDocumentKind::Text,
            language: "Rust".to_string(),
            total_lines: 1,
            indent: Default::default(),
            line_ending: Default::default(),
            encoding: Default::default(),
        };
        let viewport = || FileViewportPatch {
            first_row: 0,
            total_rows: 1,
            total_lines: 1,
            wrap_columns: 0,
            layouts: Vec::new(),
            lines: Vec::new(),
            sticky: Vec::new(),
        };
        app.world_mut()
            .trigger(UiStateWrite::<UiFile>::from_event(
                entity,
                &UiFilePatch {
                    meta: Some(meta("/first.rs", 1)),
                    viewport: Some(viewport()),
                    theme: Some(FileThemeEvent {
                        font_family: "Mono".to_string(),
                        font_size: 12.0,
                        line_height: 1.0,
                    }),
                    ..Default::default()
                },
            ));
        app.update();
        assert_eq!(app.world().resource::<Emitted>().frames.len(), 1);
        app.world_mut()
            .get_mut::<crate::PageMetadata>(entity)
            .unwrap()
            .url = "file:///second.rs".to_string();
        app.world_mut()
            .trigger(UiStateWrite::<UiFile>::from_event(
                entity,
                &FileDirtyEvent { dirty: false },
            ));
        app.update();
        let emitted = app.world().resource::<Emitted>();
        assert_eq!(emitted.frames.len(), 1);
        assert_eq!(
            emitted.state.document.meta.as_ref().unwrap().abs_path,
            "/first.rs"
        );
        app.world_mut()
            .trigger(UiStateWrite::<UiFile>::from_event(
                entity,
                &UiFilePatch {
                    meta: Some(meta("/second.rs", 2)),
                    viewport: Some(viewport()),
                    ..Default::default()
                },
            ));
        app.update();
        let emitted = app.world().resource::<Emitted>();
        assert_eq!(emitted.frames.len(), 2);
        assert_eq!(
            emitted.state.document.meta.as_ref().unwrap().abs_path,
            "/second.rs"
        );
        assert_eq!(
            emitted.state.document.theme.as_ref().unwrap().font_family,
            "Mono"
        );
    }

    #[derive(Resource, Default)]
    struct PresentedValue(u32);

    fn present_snapshot(emitted: Res<SnapshotEmitted>, mut presented: ResMut<PresentedValue>) {
        presented.0 = emitted.state.value;
    }

    #[test]
    fn early_state_flush_delivers_before_presentation_in_the_same_frame() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            UiStatePlugin::<SnapshotState>::with_post_update_flush(),
        ))
        .init_resource::<SnapshotEmitted>()
        .init_resource::<PresentedValue>()
        .add_observer(record_snapshot)
        .add_systems(PostUpdate, present_snapshot.after(UiStateFlushSet));
        let entity = app
            .world_mut()
            .spawn(UiState::<SnapshotState>::default())
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, entity, "vmux://snapshot/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 42 },
            ));
        app.update();
        assert_eq!(app.world().resource::<PresentedValue>().0, 42);
        assert_eq!(app.world().resource::<SnapshotEmitted>().frames.len(), 1);
    }

    #[test]
    fn same_frame_updates_emit_one_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<UiFile>::default()))
            .init_resource::<Emitted>()
            .add_observer(record_file_state);
        let entity = app.world_mut().spawn(UiState::<UiFile>::default()).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<UiFile>(&mut app, entity, "vmux://file/");

        app.world_mut().trigger(UiStateWrite::<UiFile>::from_event(
            entity,
            &FileDirtyEvent { dirty: true },
        ));
        app.world_mut().trigger(UiStateWrite::<UiFile>::from_event(
            entity,
            &FileDirtyEvent { dirty: false },
        ));
        app.update();

        let emitted = app.world().resource::<Emitted>();
        assert_eq!(emitted.frames.len(), 1);
        assert!(matches!(emitted.frames[0], UiStateFrame::Snapshot(_)));
        assert!(!emitted.state.document.dirty.as_ref().unwrap().dirty);
    }

    #[test]
    fn snapshot_updates_retry_and_replay_the_latest_value() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let entity = app
            .world_mut()
            .spawn(crate::PageMetadata {
                url: "vmux://snapshot/".to_string(),
                ..Default::default()
            })
            .id();

        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));
        app.update();
        assert!(app.world().resource::<SnapshotEmitted>().frames.is_empty());

        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, entity, "vmux://snapshot/");
        app.update();

        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: vmux_api::PageReady {},
        });
        app.update();

        let emitted = app.world().resource::<SnapshotEmitted>();
        assert_eq!(emitted.frames.len(), 2);
        assert_eq!(emitted.state.value, 7);
    }

    #[test]
    fn activation_does_not_replay_an_already_synced_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let entity = app.world_mut().spawn_empty().id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, entity, "vmux://snapshot/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));
        app.update();

        app.world_mut()
            .trigger(crate::page::PageActivated::new(entity));
        app.update();

        let emitted = app.world().resource::<SnapshotEmitted>();
        assert_eq!(emitted.frames.len(), 1);
        assert_eq!(emitted.state.value, 7);
    }

    #[test]
    fn a_reopened_route_receives_its_cached_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let first = app
            .world_mut()
            .spawn(crate::PageMetadata {
                url: "vmux://cached/".into(),
                ..Default::default()
            })
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(first);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, first, "vmux://cached/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                first,
                &SnapshotState { value: 7 },
            ));
        app.update();
        app.world_mut().entity_mut(first).despawn();

        let reopened = app
            .world_mut()
            .spawn(crate::PageMetadata {
                url: "vmux://cached/".into(),
                ..Default::default()
            })
            .id();
        app.world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(reopened);
        ReadyPage::authorize::<SnapshotState>(&mut app, reopened, "vmux://cached/");
        app.world_mut().trigger(UiInput {
            webview: reopened,
            payload: vmux_api::PageReady {},
        });
        app.update();
        app.update();

        let emitted = app.world().resource::<SnapshotEmitted>();
        assert_eq!(emitted.state.value, 7);
    }

    #[test]
    fn closing_before_emit_caches_pending_updates() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let first = app
            .world_mut()
            .spawn((
                crate::PageMetadata {
                    url: "vmux://pending-cache/".into(),
                    ..Default::default()
                },
                UiState::<SnapshotState>::default(),
            ))
            .id();
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                first,
                &SnapshotState { value: 11 },
            ));
        app.world_mut().entity_mut(first).despawn();

        let reopened = app
            .world_mut()
            .spawn(crate::PageMetadata {
                url: "vmux://pending-cache/".into(),
                ..Default::default()
            })
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(reopened);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, reopened, "vmux://pending-cache/");
        app.world_mut().trigger(UiInput {
            webview: reopened,
            payload: vmux_api::PageReady {},
        });
        app.update();
        app.update();

        assert_eq!(app.world().resource::<SnapshotEmitted>().state.value, 11);
    }

    #[test]
    fn transition_keeps_synced_state_across_activation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let entity = app.world_mut().spawn(crate::PageTransition).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, entity, "vmux://snapshot/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));

        app.update();

        assert_eq!(app.world().resource::<SnapshotEmitted>().frames.len(), 1);

        app.world_mut()
            .entity_mut(entity)
            .remove::<crate::PageTransition>();
        app.world_mut()
            .trigger(crate::page::PageActivated::new(entity));
        app.update();

        let emitted = app.world().resource::<SnapshotEmitted>();
        assert_eq!(emitted.frames.len(), 1);
        assert_eq!(emitted.state.value, 7);
    }

    #[test]
    fn blocked_page_retries_its_latest_snapshot_after_restore() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let entity = app.world_mut().spawn(BinIpcBlocked).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, entity, "vmux://snapshot/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));

        app.update();

        assert!(app.world().resource::<SnapshotEmitted>().frames.is_empty());

        app.world_mut().entity_mut(entity).remove::<BinIpcBlocked>();
        app.update();

        let emitted = app.world().resource::<SnapshotEmitted>();
        assert_eq!(emitted.frames.len(), 1);
        assert_eq!(emitted.state.value, 7);
    }

    #[test]
    fn denied_page_retries_its_latest_snapshot_after_permission_arrives() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(record_snapshot);
        let entity = app.world_mut().spawn_empty().id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::identify(&mut app, entity, "vmux://snapshot/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));

        app.update();

        assert!(app.world().resource::<SnapshotEmitted>().frames.is_empty());
        assert!(
            app.world()
                .resource::<UiStateDirty<SnapshotState>>()
                .entities
                .is_empty()
        );

        ReadyPage::allow::<SnapshotState>(&mut app, "vmux://snapshot/");
        app.update();

        let emitted = app.world().resource::<SnapshotEmitted>();
        assert_eq!(emitted.frames.len(), 1);
        assert_eq!(emitted.state.value, 7);
    }

    #[test]
    fn reopened_patch_state_seeds_from_the_cached_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<PatchState>::default()))
            .init_resource::<PatchEmitted>()
            .add_observer(record_patch_state);
        let first = app
            .world_mut()
            .spawn(crate::PageMetadata {
                url: "vmux://cached-patch/".into(),
                ..Default::default()
            })
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(first);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<PatchState>(&mut app, first, "vmux://cached-patch/");
        app.world_mut()
            .trigger(UiStateWrite::<PatchState>::from_event(first, &7u32));
        app.update();
        app.world_mut().entity_mut(first).despawn();

        let reopened = app
            .world_mut()
            .spawn(crate::PageMetadata {
                url: "vmux://cached-patch/".into(),
                ..Default::default()
            })
            .id();
        app.world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(reopened);
        ReadyPage::authorize::<PatchState>(&mut app, reopened, "vmux://cached-patch/");
        app.world_mut()
            .trigger(UiStateWrite::<PatchState>::from_event(
                reopened,
                &String::from("ready"),
            ));
        app.update();

        let state = &app.world().resource::<PatchEmitted>().0[0];
        assert_eq!(state.first, 7);
        assert_eq!(state.second, "ready");
    }

    #[test]
    fn direct_writer_keeps_same_frame_updates_when_state_is_missing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<PatchState>::default()))
            .init_resource::<PatchEmitted>()
            .add_observer(record_patch_state)
            .add_systems(Update, push_patch_state);
        let entity = app.world_mut().spawn_empty().id();
        app.insert_resource(PatchTarget(entity));
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<PatchState>(&mut app, entity, "vmux://patch/");

        app.update();

        let emitted = app.world().resource::<PatchEmitted>();
        assert_eq!(emitted.0.len(), 1);
        assert_eq!(emitted.0[0].first, 7);
        assert_eq!(emitted.0[0].second, "ready");
    }

    #[test]
    fn route_change_keeps_pending_updates_with_the_previous_route() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<PatchState>::default()));
        let entity = app
            .world_mut()
            .spawn((
                crate::PageMetadata {
                    url: "vmux://previous/".into(),
                    ..Default::default()
                },
                UiState::<PatchState>::default(),
            ))
            .id();

        app.world_mut()
            .trigger(UiStateWrite::<PatchState>::from_event(entity, &7u32));
        app.world_mut()
            .get_mut::<crate::PageMetadata>(entity)
            .unwrap()
            .url = "vmux://next/".into();
        app.world_mut()
            .trigger(UiStateWrite::<PatchState>::from_event(
                entity,
                &String::from("ready"),
            ));

        let previous = app
            .world_mut()
            .resource_mut::<UiStateCache<PatchState>>()
            .get("vmux://previous/")
            .unwrap();
        assert_eq!(previous.first, 7);
        assert!(previous.second.is_empty());
        let current = app.world().get::<UiState<PatchState>>(entity).unwrap();
        assert_eq!(current.updates.len(), 1);
        let current = PatchState::from_updates(current.retained.clone(), current.updates.clone());
        assert_eq!(current.first, 0);
        assert_eq!(current.second, "ready");
    }

    #[test]
    fn route_change_never_emits_the_previous_route_updates() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<PatchState>::default()))
            .init_resource::<PatchEmitted>()
            .add_observer(record_patch_state);
        let entity = app
            .world_mut()
            .spawn((
                crate::PageMetadata {
                    url: "vmux://previous/".into(),
                    ..Default::default()
                },
                UiState::<PatchState>::default(),
            ))
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<PatchState>(&mut app, entity, "vmux://previous/");
        ReadyPage::allow::<PatchState>(&mut app, "vmux://next/");

        app.world_mut()
            .trigger(UiStateWrite::<PatchState>::from_event(entity, &7u32));
        app.world_mut()
            .get_mut::<crate::PageMetadata>(entity)
            .unwrap()
            .url = "vmux://next/".into();
        app.update();

        assert!(app.world().resource::<PatchEmitted>().0.is_empty());
        let previous = app
            .world_mut()
            .resource_mut::<UiStateCache<PatchState>>()
            .get("vmux://previous/")
            .unwrap();
        assert_eq!(previous.first, 7);
    }

    #[test]
    fn rejected_delivery_retries_with_a_bounded_snapshot_replay() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<RejectedEmitted>()
            .add_observer(reject_snapshot);
        let entity = app.world_mut().spawn_empty().id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<SnapshotState>(&mut app, entity, "vmux://snapshot/");
        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));

        for _ in 0..4 {
            app.update();
        }

        assert_eq!(app.world().resource::<RejectedEmitted>().0, 3);
        assert!(
            app.world()
                .resource::<UiStateDirty<SnapshotState>>()
                .entities
                .is_empty()
        );
        let state = app.world().get::<UiState<SnapshotState>>(entity).unwrap();
        assert!(!state.synced);
        assert!(state.pending_snapshot);
        assert!(state.pending.is_some());
    }

    #[test]
    fn rejected_delivery_retries_the_same_transient_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<TransientState>::default()))
            .init_resource::<TransientDelivery>()
            .add_observer(record_transient);
        let entity = app.world_mut().spawn_empty().id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<TransientState>(&mut app, entity, "vmux://transient/");
        app.world_mut()
            .trigger(UiStateWrite::<TransientState>::from_event(
                entity,
                &TransientStatePatch {
                    reveal: Some("target".into()),
                    ..Default::default()
                },
            ));

        app.update();
        app.world_mut().resource_mut::<TransientDelivery>().accept = true;
        app.update();

        let delivery = app.world().resource::<TransientDelivery>();
        assert_eq!(delivery.frames.len(), 2);
        for frame in &delivery.frames {
            let UiStateFrame::Snapshot(state) = frame else {
                panic!("expected snapshot");
            };
            assert_eq!(state.reveal.as_deref(), Some("target"));
        }
        let state = app.world().get::<UiState<TransientState>>(entity).unwrap();
        assert_eq!(state.current().unwrap().reveal, None);
        assert!(state.synced);
    }

    #[test]
    fn blocked_delivery_keeps_transient_updates_until_unblocked() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<TransientState>::default()))
            .init_resource::<TransientDelivery>()
            .add_observer(record_transient);
        app.world_mut().resource_mut::<TransientDelivery>().accept = true;
        let entity = app.world_mut().spawn(BinIpcBlocked).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        ReadyPage::authorize::<TransientState>(&mut app, entity, "vmux://transient/");
        app.world_mut()
            .trigger(UiStateWrite::<TransientState>::from_event(
                entity,
                &TransientStatePatch {
                    reveal: Some("target".into()),
                    ..Default::default()
                },
            ));

        app.update();
        assert!(
            app.world()
                .resource::<TransientDelivery>()
                .frames
                .is_empty()
        );

        app.world_mut().entity_mut(entity).remove::<BinIpcBlocked>();
        app.update();

        let delivery = app.world().resource::<TransientDelivery>();
        assert_eq!(delivery.frames.len(), 1);
        let UiStateFrame::Snapshot(state) = &delivery.frames[0] else {
            panic!("expected snapshot");
        };
        assert_eq!(state.reveal.as_deref(), Some("target"));
    }

    #[test]
    fn restore_discards_the_interrupted_route_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<PatchState>::default()))
            .add_systems(Update, restore_patch_state);
        let entity = app
            .world_mut()
            .spawn((
                crate::PageMetadata {
                    url: "vmux://interrupted/".into(),
                    ..Default::default()
                },
                UiState::<PatchState>::default(),
            ))
            .id();
        app.world_mut()
            .trigger(UiStateWrite::<PatchState>::from_event(entity, &11u32));
        app.insert_resource(RestoreTarget(entity));

        app.update();

        assert!(
            app.world_mut()
                .resource_mut::<UiStateCache<PatchState>>()
                .get("vmux://interrupted/")
                .is_none()
        );
        let state = app.world().get::<UiState<PatchState>>(entity).unwrap();
        assert_eq!(state.key.as_deref(), Some("vmux://previous/"));
        assert_eq!(state.retained.as_ref().unwrap().first, 7);
        assert_eq!(state.retained.as_ref().unwrap().second, "ready");
        assert!(state.updates.is_empty());
    }

    #[test]
    fn writes_to_closed_pages_are_ignored() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()));
        let entity = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(entity);

        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));
        app.update();

        assert!(app.world().get_entity(entity).is_err());
    }

    #[test]
    fn delivery_only_targets_pages_with_the_requested_ui_state() {
        let mut app = App::new();
        let file = app.world_mut().spawn(UiState::<UiFile>::default()).id();
        let direct = app.world_mut().spawn_empty().id();
        app.insert_resource(Targets { file, direct })
            .init_resource::<Delivered>()
            .add_observer(record_write)
            .add_systems(Update, deliver);

        app.update();

        let delivered = app.world().resource::<Delivered>();
        assert_eq!(delivered.writes, vec![file]);
    }
}
