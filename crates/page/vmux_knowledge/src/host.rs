use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::knowledge::{
    KNOWLEDGE_CREATE_RESULT_EVENT, KNOWLEDGE_SEARCH_EVENT, KNOWLEDGE_TREE_EVENT,
    KnowledgeCreateRequest, KnowledgeCreateResult, KnowledgeIndex, KnowledgeSearchEvent,
    KnowledgeSearchMatch, KnowledgeSearchRequest, KnowledgeTreeEvent,
};
use vmux_core::page::PageReady;
use vmux_layout::LayoutCef;

use crate::store::{build_tree, create_entry, ensure_vault, ensure_vault_repository, vault_dir};

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        let vault = vault_dir();
        if ensure_vault(&vault).is_ok() {
            if let Err(error) = ensure_vault_repository(&vault) {
                bevy::log::warn!("knowledge Git initialization failed: {error}");
            }
            if let Err(error) = vmux_core::knowledge::sync_external_agent_configs() {
                bevy::log::warn!("external agent Knowledge sync failed: {error}");
            }
            let (tx, rx) = mpsc::channel();
            let watch_wake = app
                .world()
                .get_resource::<EventLoopProxyWrapper>()
                .map(|wrapper| (**wrapper).clone());
            match notify::recommended_watcher(move |result| {
                if tx.send(result).is_ok()
                    && let Some(wake) = watch_wake.as_ref()
                {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
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
        app.init_resource::<KnowledgeState>()
            .init_resource::<KnowledgeIndex>()
            .add_plugins(BinEventEmitterPlugin::<(
                KnowledgeSearchRequest,
                KnowledgeCreateRequest,
            )>::default())
            .add_systems(
                Update,
                (
                    drain_knowledge_watch,
                    start_knowledge_tree_scan,
                    drain_knowledge_tree_scan,
                    emit_knowledge_tree,
                )
                    .chain(),
            )
            .add_observer(on_knowledge_search)
            .add_observer(on_knowledge_create);
    }
}

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
    task: Task<Result<(KnowledgeTreeEvent, KnowledgeIndex), String>>,
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
        changed |= result.is_ok_and(|event| !matches!(event.kind, EventKind::Access(_)));
    }
    if changed {
        state.dirty = true;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn start_knowledge_tree_scan(
    mut state: ResMut<KnowledgeState>,
    pending: Query<(), With<KnowledgeTreeTask>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !state.dirty || !pending.is_empty() {
        return;
    }
    let generation = state.generation;
    let wake = wake.map(|wrapper| (**wrapper).clone());
    let task = IoTaskPool::get().spawn(async move {
        let result = (|| {
            let root = vault_dir();
            let tree = build_tree(&root).map_err(|error| error.to_string())?;
            let index = KnowledgeIndex::build(&root).map_err(|error| error.to_string())?;
            Ok((tree, index))
        })();
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    state.dirty = false;
    commands.spawn(KnowledgeTreeTask { generation, task });
}

fn drain_knowledge_tree_scan(
    mut tasks: Query<(Entity, &mut KnowledgeTreeTask)>,
    mut state: ResMut<KnowledgeState>,
    mut index: ResMut<KnowledgeIndex>,
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
            Ok((tree, next_index)) => {
                *index = next_index;
                tree
            }
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
    if !browsers.can_emit_to(&entity) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        KNOWLEDGE_TREE_EVENT,
        &state.tree,
    ));
    *last_revision = state.revision;
}

fn on_knowledge_search(
    trigger: On<BinReceive<KnowledgeSearchRequest>>,
    index: Res<KnowledgeIndex>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !browsers.can_emit_to(&webview) {
        return;
    }
    let query = trigger.event().payload.query.trim().to_string();
    let matches = index
        .search(&query, 64)
        .into_iter()
        .map(|item| KnowledgeSearchMatch {
            title: item.title,
            path: item.path.to_string_lossy().into_owned(),
            line: item.line + 1,
            preview: item.preview,
        })
        .collect();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        KNOWLEDGE_SEARCH_EVENT,
        &KnowledgeSearchEvent { query, matches },
    ));
}

fn on_knowledge_create(
    trigger: On<BinReceive<KnowledgeCreateRequest>>,
    browsers: NonSend<Browsers>,
    mut state: ResMut<KnowledgeState>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !browsers.can_emit_to(&webview) {
        return;
    }
    let request = &trigger.event().payload;
    let result = create_entry(
        &vault_dir(),
        std::path::Path::new(&request.parent),
        &request.name,
        request.is_directory,
    );
    let payload = match result {
        Ok(path) => {
            state.dirty = true;
            state.generation = state.generation.wrapping_add(1);
            KnowledgeCreateResult {
                ok: true,
                path: path.to_string_lossy().into_owned(),
                error: String::new(),
                is_directory: request.is_directory,
            }
        }
        Err(error) => KnowledgeCreateResult {
            ok: false,
            path: String::new(),
            error,
            is_directory: request.is_directory,
        },
    };
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        KNOWLEDGE_CREATE_RESULT_EVENT,
        &payload,
    ));
}
