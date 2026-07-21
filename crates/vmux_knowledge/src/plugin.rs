use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::knowledge::{KNOWLEDGE_TREE_EVENT, KnowledgeTreeEvent};
use vmux_core::page::PageReady;
use vmux_layout::LayoutCef;

use crate::store::{build_tree, ensure_vault, vault_dir};

pub struct KnowledgePlugin;

#[derive(Resource)]
struct KnowledgeState {
    dirty: bool,
    generation: u64,
    revision: u64,
    loaded: bool,
    tree: KnowledgeTreeEvent,
}

impl Default for KnowledgeState {
    fn default() -> Self {
        Self {
            dirty: true,
            generation: 1,
            revision: 0,
            loaded: false,
            tree: KnowledgeTreeEvent::default(),
        }
    }
}

struct KnowledgeWatch {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
}

#[derive(Component)]
struct KnowledgeTreeTask {
    generation: u64,
    task: Task<Result<KnowledgeTreeEvent, String>>,
}

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        let vault = vault_dir();
        if ensure_vault(&vault).is_ok() {
            if let Err(error) = vmux_core::knowledge::sync_external_agent_configs() {
                bevy::log::warn!("external agent Knowledge sync failed: {error}");
            }
            let (tx, rx) = mpsc::channel();
            match notify::recommended_watcher(move |result| {
                let _ = tx.send(result);
            }) {
                Ok(mut watcher) => {
                    if watcher.watch(&vault, RecursiveMode::Recursive).is_ok() {
                        app.insert_non_send(KnowledgeWatch {
                            _watcher: watcher,
                            rx,
                        });
                    }
                }
                Err(error) => bevy::log::warn!("knowledge watcher init failed: {error}"),
            }
        }
        app.init_resource::<KnowledgeState>().add_systems(
            Update,
            (
                drain_knowledge_watch,
                start_knowledge_tree_scan,
                drain_knowledge_tree_scan,
                emit_knowledge_tree,
            )
                .chain(),
        );
    }
}

fn drain_knowledge_watch(
    watch: Option<NonSendMut<KnowledgeWatch>>,
    mut state: ResMut<KnowledgeState>,
) {
    let Some(watch) = watch else {
        return;
    };
    let mut changed = false;
    for result in watch.rx.try_iter() {
        changed |= result.is_ok();
    }
    if changed {
        state.dirty = true;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn start_knowledge_tree_scan(
    mut state: ResMut<KnowledgeState>,
    pending: Query<(), With<KnowledgeTreeTask>>,
    mut commands: Commands,
) {
    if !state.dirty || !pending.is_empty() {
        return;
    }
    let generation = state.generation;
    let task = IoTaskPool::get()
        .spawn(async move { build_tree(&vault_dir()).map_err(|error| error.to_string()) });
    state.dirty = false;
    commands.spawn(KnowledgeTreeTask { generation, task });
}

fn drain_knowledge_tree_scan(
    mut tasks: Query<(Entity, &mut KnowledgeTreeTask)>,
    mut state: ResMut<KnowledgeState>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.tree = match result {
            Ok(tree) => tree,
            Err(error) => KnowledgeTreeEvent {
                root: vault_dir().to_string_lossy().into_owned(),
                entries: Vec::new(),
                error,
            },
        };
        state.loaded = true;
        state.revision = state.revision.wrapping_add(1);
    }
}

fn emit_knowledge_tree(
    state: Res<KnowledgeState>,
    browsers: NonSend<Browsers>,
    layout: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    mut last_revision: Local<u64>,
    mut commands: Commands,
) {
    if !state.loaded {
        return;
    }
    let Ok((entity, page_ready)) = layout.single() else {
        return;
    };
    if state.revision == *last_revision && !page_ready.is_changed() {
        return;
    }
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        KNOWLEDGE_TREE_EVENT,
        &state.tree,
    ));
    *last_revision = state.revision;
}
