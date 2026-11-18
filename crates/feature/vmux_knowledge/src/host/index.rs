use std::path::Path;
use std::sync::mpsc;

use crate::{
    KnowledgeGraphPlugin, KnowledgeIndex, KnowledgeVault, LiveKnowledge, MaterializeKnowledge,
    RawSnapshot,
};
use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::result::Result;

pub(super) struct KnowledgeIndexPlugin;

impl Plugin for KnowledgeIndexPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<KnowledgeGraphPlugin>() {
            app.add_plugins(KnowledgeGraphPlugin);
        }
        app.insert_non_send(KnowledgeWatch::default());
        app.add_systems(Startup, initialize)
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
        LiveKnowledge,
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
        let result = RawSnapshot::capture(&root).map_err(|error| error.to_string());
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    commands.entity(entity).remove::<KnowledgeIndexDirty>();
    commands.spawn(KnowledgeIndexTask {
        owner: entity,
        generation,
        task,
    });
}

#[derive(Component)]
struct KnowledgeIndexTask {
    owner: Entity,
    generation: u64,
    task: Task<Result<RawSnapshot, String>>,
}

fn finish(
    mut tasks: Query<(Entity, &mut KnowledgeIndexTask)>,
    indexes: Query<&KnowledgeIndexGeneration, With<KnowledgeIndex>>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let Ok(generation) = indexes.get(task.owner) else {
            continue;
        };
        if task.generation != generation.0 {
            commands.entity(task.owner).insert(KnowledgeIndexDirty);
            continue;
        }
        match result {
            Ok(snapshot) => {
                commands.trigger(MaterializeKnowledge {
                    entity: task.owner,
                    snapshot,
                });
            }
            Err(error) => warn!("knowledge index refresh failed: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RawNote;
    use crate::graph::KnowledgeNote;
    use std::time::{Duration, Instant};

    struct IndexFixture {
        directory: tempfile::TempDir,
        app: App,
        owner: Entity,
    }

    impl IndexFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(KnowledgeIndexPlugin);
            let owner = app
                .world_mut()
                .spawn((
                    KnowledgeIndex::default(),
                    KnowledgeIndexGeneration(2),
                    LiveKnowledge,
                ))
                .id();
            app.world_mut().flush();
            app.world_mut().trigger(MaterializeKnowledge {
                entity: owner,
                snapshot: RawSnapshot {
                    root: root.clone(),
                    files: vec![RawNote {
                        path: root.join("note.md"),
                        text: "# Current\n".to_string(),
                    }],
                },
            });
            app.world_mut().flush();
            Self {
                directory,
                app,
                owner,
            }
        }

        fn task(&mut self, generation: u64) -> Entity {
            let root = self.directory.path().canonicalize().unwrap();
            let snapshot = RawSnapshot {
                root: root.clone(),
                files: vec![RawNote {
                    path: root.join("note.md"),
                    text: "# Stale\n".to_string(),
                }],
            };
            let task = IoTaskPool::get().spawn(async move { Ok(snapshot) });
            self.app
                .world_mut()
                .spawn(KnowledgeIndexTask {
                    owner: self.owner,
                    generation,
                    task,
                })
                .id()
        }

        fn drain(&mut self, task: Entity) {
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.app.world().get_entity(task).is_ok() && Instant::now() < deadline {
                self.app.world_mut().run_schedule(Update);
                std::thread::yield_now();
            }
            assert!(
                self.app.world().get_entity(task).is_err(),
                "captured IO must drain"
            );
        }
    }

    #[test]
    fn obsolete_snapshot_is_drained_without_replacing_the_current_catalog() {
        let mut fixture = IndexFixture::new();
        let task = fixture.task(1);
        fixture.drain(task);
        let names = fixture
            .app
            .world_mut()
            .query::<(&KnowledgeNote, &Name)>()
            .iter(fixture.app.world())
            .map(|(_, name)| name.as_str().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, ["Current"]);
        assert!(
            fixture
                .app
                .world()
                .get::<KnowledgeIndexDirty>(fixture.owner)
                .is_some()
        );
    }

    #[test]
    fn captured_io_drains_even_after_its_catalog_owner_is_removed() {
        let mut fixture = IndexFixture::new();
        let task = fixture.task(2);
        fixture.app.world_mut().despawn(fixture.owner);
        fixture.drain(task);
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&KnowledgeIndex>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&KnowledgeNote>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }
}
