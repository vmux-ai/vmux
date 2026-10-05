use std::path::Path;
use std::sync::mpsc;

use crate::{KnowledgeIndex, KnowledgeVault};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub(super) struct KnowledgeIndexPlugin;

impl Plugin for KnowledgeIndexPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(KnowledgeWatch::default())
            .add_systems(Startup, initialize)
            .add_systems(Update, (drain_watch, start, finish).chain());
    }
}

fn initialize(
    mut commands: Commands,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut watcher: NonSendMut<KnowledgeWatch>,
) {
    let vault = KnowledgeVault::user();
    commands.spawn((
        Name::new("Knowledge index"),
        KnowledgeIndex::default(),
        KnowledgeIndexGeneration(1),
        KnowledgeIndexDirty,
        vault.clone(),
    ));
    if let Err(error) = vault.ensure() {
        warn!("knowledge vault initialization failed: {error}");
        return;
    }
    if let Err(error) = vault.ensure_repository() {
        warn!("knowledge Git initialization failed: {error}");
    }
    if let Err(error) = vault.sync_agent_configs() {
        warn!("external agent Knowledge sync failed: {error}");
    }
    let wake = wake.map(|wrapper| (**wrapper).clone());
    match watch(vault.root(), wake) {
        Ok(knowledge_watcher) => watcher.0 = Some(knowledge_watcher),
        Err(error) => warn!("knowledge watcher init failed: {error}"),
    }
}

#[derive(Component)]
struct KnowledgeIndexGeneration(u64);

#[derive(Component)]
struct KnowledgeIndexDirty;

#[derive(Default)]
struct KnowledgeWatch(Option<KnowledgeWatcher>);

struct KnowledgeWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<notify::Event>>,
}

fn watch(
    root: &Path,
    wake: Option<EventLoopProxy<WinitUserEvent>>,
) -> notify::Result<KnowledgeWatcher> {
    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        if sender.send(result).is_ok()
            && let Some(wake) = wake.as_ref()
        {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(KnowledgeWatcher {
        _watcher: watcher,
        receiver,
    })
}

fn drain_watch(
    watch: NonSend<KnowledgeWatch>,
    mut indexes: Query<(Entity, &mut KnowledgeIndexGeneration), With<KnowledgeIndex>>,
    mut commands: Commands,
) {
    let Some(watch) = watch.0.as_ref() else {
        return;
    };
    if watch
        .receiver
        .try_iter()
        .any(|result| result.is_ok_and(|event| !matches!(event.kind, EventKind::Access(_))))
    {
        let Ok((entity, mut generation)) = indexes.single_mut() else {
            return;
        };
        generation.0 = generation.0.wrapping_add(1);
        commands.entity(entity).insert(KnowledgeIndexDirty);
    }
}

type DirtyIndexes<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static KnowledgeIndexGeneration),
    (With<KnowledgeIndex>, With<KnowledgeIndexDirty>),
>;

fn start(
    indexes: DirtyIndexes,
    vault: Single<&KnowledgeVault>,
    pending: Query<(), With<KnowledgeIndexTask>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !pending.is_empty() {
        return;
    }
    let Ok((entity, generation)) = indexes.single() else {
        return;
    };
    let generation = generation.0;
    let root = vault.root().to_path_buf();
    let wake = wake.map(|wrapper| (**wrapper).clone());
    let task = IoTaskPool::get().spawn(async move {
        let result = KnowledgeIndex::build(&root).map_err(|error| error.to_string());
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    commands.entity(entity).remove::<KnowledgeIndexDirty>();
    commands.spawn(KnowledgeIndexTask { generation, task });
}

#[derive(Component)]
struct KnowledgeIndexTask {
    generation: u64,
    task: Task<Result<KnowledgeIndex, String>>,
}

fn finish(
    mut tasks: Query<(Entity, &mut KnowledgeIndexTask)>,
    mut indexes: Query<(Entity, &mut KnowledgeIndex, &KnowledgeIndexGeneration)>,
    mut commands: Commands,
) {
    let Ok((index_entity, mut index, generation)) = indexes.single_mut() else {
        return;
    };
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.generation != generation.0 {
            commands.entity(index_entity).insert(KnowledgeIndexDirty);
            continue;
        }
        match result {
            Ok(next) => *index = next,
            Err(error) => warn!("knowledge index refresh failed: {error}"),
        }
    }
}
