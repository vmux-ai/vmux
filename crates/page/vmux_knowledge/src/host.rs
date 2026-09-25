use std::path::Path;
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::knowledge::KnowledgeIndex;

use crate::store::{ensure_vault, ensure_vault_repository, vault_dir};

pub struct KnowledgePlugin;

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .spawn((PAGE_MANIFEST, KnowledgeIndexRuntime::default()));
        app.init_resource::<KnowledgeIndex>()
            .register_type::<ExpandedKnowledgeDirs>()
            .add_systems(
                Update,
                (
                    drain_knowledge_watch,
                    start_knowledge_index,
                    finish_knowledge_index,
                )
                    .chain(),
            );

        let vault = vault_dir();
        if let Err(error) = ensure_vault(&vault) {
            warn!("knowledge vault initialization failed: {error}");
            return;
        }
        if let Err(error) = ensure_vault_repository(&vault) {
            warn!("knowledge Git initialization failed: {error}");
        }
        if let Err(error) = vmux_core::knowledge::sync_external_agent_configs() {
            warn!("external agent Knowledge sync failed: {error}");
        }
        let wake = app
            .world()
            .get_resource::<EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        match KnowledgeWatch::watching(&vault, wake) {
            Ok(watch) => {
                app.insert_non_send(watch);
            }
            Err(error) => warn!("knowledge watcher init failed: {error}"),
        }
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "knowledge",
    title: "Knowledge",
    title_message_id: Some("layout-knowledge"),
    replaces_command: None,
    keywords: &["knowledge", "notes", "markdown"],
    icon: Some(vmux_core::BuiltinIcon::Brain),
    command_bar: true,
};

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::knowledge"]
#[require(moonshine_save::prelude::Save)]
pub struct ExpandedKnowledgeDirs(Vec<String>);

struct KnowledgeWatch {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<notify::Event>>,
}

impl KnowledgeWatch {
    fn watching(root: &Path, wake: Option<EventLoopProxy<WinitUserEvent>>) -> notify::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |result| {
            if sender.send(result).is_ok()
                && let Some(wake) = wake.as_ref()
            {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            receiver,
        })
    }
}

fn drain_knowledge_watch(
    watch: Option<NonSend<KnowledgeWatch>>,
    mut runtime: Single<&mut KnowledgeIndexRuntime>,
) {
    let Some(watch) = watch else {
        return;
    };
    if watch
        .receiver
        .try_iter()
        .any(|result| result.is_ok_and(|event| !matches!(event.kind, EventKind::Access(_))))
    {
        runtime.invalidate();
    }
}

#[derive(Component)]
struct KnowledgeIndexRuntime {
    dirty: bool,
    generation: u64,
}

impl Default for KnowledgeIndexRuntime {
    fn default() -> Self {
        Self {
            dirty: true,
            generation: 1,
        }
    }
}

impl KnowledgeIndexRuntime {
    fn invalidate(&mut self) {
        self.dirty = true;
        self.generation = self.generation.wrapping_add(1);
    }
}

fn start_knowledge_index(
    mut runtime: Single<&mut KnowledgeIndexRuntime>,
    pending: Query<(), With<KnowledgeIndexTask>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !runtime.dirty || !pending.is_empty() {
        return;
    }
    let generation = runtime.generation;
    let wake = wake.map(|wrapper| (**wrapper).clone());
    let task = IoTaskPool::get().spawn(async move {
        let result = KnowledgeIndex::build(&vault_dir()).map_err(|error| error.to_string());
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    runtime.dirty = false;
    commands.spawn(KnowledgeIndexTask { generation, task });
}

#[derive(Component)]
struct KnowledgeIndexTask {
    generation: u64,
    task: Task<Result<KnowledgeIndex, String>>,
}

fn finish_knowledge_index(
    mut tasks: Query<(Entity, &mut KnowledgeIndexTask)>,
    mut runtime: Single<&mut KnowledgeIndexRuntime>,
    mut index: ResMut<KnowledgeIndex>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.generation != runtime.generation {
            runtime.dirty = true;
            continue;
        }
        match result {
            Ok(next) => *index = next,
            Err(error) => warn!("knowledge index refresh failed: {error}"),
        }
    }
}
