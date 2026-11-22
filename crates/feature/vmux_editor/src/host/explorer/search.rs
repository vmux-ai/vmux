use std::path::PathBuf;

use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::event::{
    ExplorerGoto, ExplorerSearchClear, ExplorerSearchCollapseAll, ExplorerSearchDraftRequest,
    ExplorerSearchEvent, ExplorerSearchFile, ExplorerSearchGroupToggle, ExplorerSearchOpen,
    ExplorerSearchRequest,
};

use super::panel::StackExplorerVisibility;
use super::{ExplorerPanelDefaults, ExplorerPanelSent};
use crate::host::editor::{FileNavigateRequest, FileView};
use crate::host::navigation::PendingGoto;

pub(super) struct SearchPlugin;

impl Plugin for SearchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ExplorerGoto,
            ExplorerSearchOpen,
            ExplorerSearchGroupToggle,
            ExplorerSearchCollapseAll,
            ExplorerSearchClear,
            ExplorerSearchDraftRequest,
        )>::default())
            .add_systems(Update, (queue, apply, emit).chain())
            .add_observer(goto)
            .add_observer(open)
            .add_observer(toggle_group)
            .add_observer(collapse_all)
            .add_observer(clear)
            .add_observer(draft)
            .add_observer(begin)
            .add_observer(adopt_completed)
            .add_observer(navigate);
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchRequest {
    pub target_path: PathBuf,
    pub root: String,
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub files: Vec<ExplorerSearchFile>,
    pub capped: bool,
}

#[derive(EntityEvent)]
pub(crate) struct ExplorerSearchRunRequest {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) generation: u64,
    pub(crate) query: String,
    pub(crate) regex: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
}

#[derive(EntityEvent)]
pub(crate) struct ExplorerSearchCancelRequest {
    #[event_target]
    pub(crate) entity: Entity,
}

#[derive(EntityEvent)]
pub(crate) struct ExplorerSearchCompleted {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) generation: u64,
    pub(crate) root: String,
    pub(crate) query: String,
    pub(crate) regex: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
    pub(crate) files: Vec<ExplorerSearchFile>,
    pub(crate) capped: bool,
}

#[derive(Component, Default, Clone, PartialEq, Eq)]
pub(crate) struct ExplorerSearchQuery {
    pub(crate) query: String,
    pub(crate) regex: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
    pub(crate) generation: u64,
}

impl ExplorerSearchQuery {
    fn matches_form(
        &self,
        query: &str,
        regex: bool,
        case_sensitive: bool,
        whole_word: bool,
    ) -> bool {
        self.query == query
            && self.regex == regex
            && self.case_sensitive == case_sensitive
            && self.whole_word == whole_word
    }

    fn set_form(
        &mut self,
        query: String,
        regex: bool,
        case_sensitive: bool,
        whole_word: bool,
    ) -> u64 {
        self.query = query;
        self.regex = regex;
        self.case_sensitive = case_sensitive;
        self.whole_word = whole_word;
        self.advance()
    }

    fn clear(&mut self) -> u64 {
        self.query.clear();
        self.regex = false;
        self.case_sensitive = false;
        self.whole_word = false;
        self.advance()
    }

    fn advance(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }
}

#[derive(Component, Default, Clone)]
struct ExplorerSearchResults {
    root: String,
    files: Vec<ExplorerSearchFile>,
    capped: bool,
    collapsed: Vec<String>,
    opened: String,
}

#[derive(Component)]
struct GlobalSearchDirty;

#[derive(Component)]
struct PendingGlobalSearch {
    request: GlobalSearchRequest,
    retries_left: u8,
}

impl PendingGlobalSearch {
    const RETRY_LIMIT: u8 = 120;

    fn new(request: GlobalSearchRequest) -> Self {
        Self {
            request,
            retries_left: Self::RETRY_LIMIT,
        }
    }

    fn retry(&mut self) -> bool {
        self.retries_left = self.retries_left.saturating_sub(1);
        self.retries_left > 0
    }
}

type GlobalSearchDirtyReady = (
    With<ExplorerSearchQuery>,
    With<GlobalSearchDirty>,
    With<vmux_ecs::page::PageReady>,
);

fn goto(
    trigger: On<UiInput<ExplorerGoto>>,
    views: Query<&FileView>,
    mut writer: MessageWriter<crate::lsp::manager::LspGoto>,
) {
    let entity = trigger.event().webview;
    let Ok(file_view) = views.get(entity) else {
        return;
    };
    writer.write(crate::lsp::manager::LspGoto {
        entity,
        path: file_view.path.clone(),
        line: trigger.event().payload.line,
        utf16_col: 0,
        origin: None,
    });
}

fn queue(mut reader: MessageReader<GlobalSearchRequest>, mut commands: Commands) {
    for request in reader.read() {
        commands.spawn(PendingGlobalSearch::new(request.clone()));
    }
}

fn apply(
    mut pending: Query<(Entity, &mut PendingGlobalSearch)>,
    views: Query<(Entity, &FileView, Option<&ChildOf>)>,
    mut queries: Query<&mut ExplorerSearchQuery>,
    visibility: Query<&StackExplorerVisibility>,
    panel: Single<&ExplorerPanelDefaults>,
    mut commands: Commands,
) {
    for (pending_entity, mut pending_request) in &mut pending {
        let request = &pending_request.request;
        let Some((entity, _, parent)) = views
            .iter()
            .find(|(_, view, _)| view.path == request.target_path)
        else {
            if !pending_request.retry() {
                commands.entity(pending_entity).despawn();
            }
            continue;
        };
        let scope = parent.map(ChildOf::parent).unwrap_or(entity);
        if let Ok(search) = queries.get(entity)
            && !search.matches_form(
                &request.query,
                request.regex,
                request.case_sensitive,
                request.whole_word,
            )
        {
            commands.entity(pending_entity).despawn();
            continue;
        }
        let explorer_visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        if !explorer_visible {
            commands
                .entity(scope)
                .insert(StackExplorerVisibility { visible: true });
            for (view, _, parent) in &views {
                let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
                if view_scope == scope {
                    commands.entity(view).remove::<ExplorerPanelSent>();
                }
            }
        }
        let request = pending_request.request.clone();
        if let Ok(mut query) = queries.get_mut(entity) {
            query.query = request.query;
            query.regex = request.regex;
            query.case_sensitive = request.case_sensitive;
            query.whole_word = request.whole_word;
        } else {
            commands.entity(entity).insert(ExplorerSearchQuery {
                query: request.query,
                regex: request.regex,
                case_sensitive: request.case_sensitive,
                whole_word: request.whole_word,
                generation: 1,
            });
        }
        commands.entity(entity).insert((
            ExplorerSearchResults {
                root: request.root,
                files: request.files,
                capped: request.capped,
                collapsed: Vec::new(),
                opened: String::new(),
            },
            GlobalSearchDirty,
        ));
        commands.entity(pending_entity).despawn();
    }
}

fn adopt_completed(
    trigger: On<ExplorerSearchCompleted>,
    views: Query<(Entity, Option<&ChildOf>), With<FileView>>,
    queries: Query<&ExplorerSearchQuery>,
    visibility: Query<&StackExplorerVisibility>,
    panel: Option<Single<&ExplorerPanelDefaults>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(query) = queries.get(entity) else {
        return;
    };
    if query.generation != trigger.generation
        || !query.matches_form(
            &trigger.query,
            trigger.regex,
            trigger.case_sensitive,
            trigger.whole_word,
        )
    {
        return;
    }
    if let Ok((_, parent)) = views.get(entity) {
        let scope = parent.map(ChildOf::parent).unwrap_or(entity);
        let default_visible = panel.is_some_and(|panel| panel.default_visible);
        let explorer_visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(default_visible);
        if !explorer_visible {
            commands
                .entity(scope)
                .insert(StackExplorerVisibility { visible: true });
            for (view, parent) in &views {
                let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
                if view_scope == scope {
                    commands.entity(view).remove::<ExplorerPanelSent>();
                }
            }
        }
    }
    commands.entity(entity).insert((
        ExplorerSearchResults {
            root: trigger.root.clone(),
            files: trigger.files.clone(),
            capped: trigger.capped,
            collapsed: Vec::new(),
            opened: String::new(),
        },
        GlobalSearchDirty,
    ));
}

fn emit(
    query: Query<
        (Entity, &ExplorerSearchQuery, Option<&ExplorerSearchResults>),
        GlobalSearchDirtyReady,
    >,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    if !browsers.available() {
        return;
    }
    for (entity, form, results) in &query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let event = match results {
            Some(results) => ExplorerSearchEvent {
                root: results.root.clone(),
                query: form.query.clone(),
                regex: form.regex,
                case_sensitive: form.case_sensitive,
                whole_word: form.whole_word,
                files: results.files.clone(),
                capped: results.capped,
                collapsed: results.collapsed.clone(),
                opened: results.opened.clone(),
            },
            None => ExplorerSearchEvent {
                query: form.query.clone(),
                regex: form.regex,
                case_sensitive: form.case_sensitive,
                whole_word: form.whole_word,
                ..Default::default()
            },
        };
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &event));
        commands.entity(entity).remove::<GlobalSearchDirty>();
    }
}

fn draft(
    trigger: On<UiInput<ExplorerSearchDraftRequest>>,
    mut queries: Query<&mut ExplorerSearchQuery>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    if let Ok(mut query) = queries.get_mut(entity) {
        if query.matches_form(
            &request.query,
            request.regex,
            request.case_sensitive,
            request.whole_word,
        ) {
            return;
        }
        query.set_form(
            request.query.clone(),
            request.regex,
            request.case_sensitive,
            request.whole_word,
        );
    } else {
        commands.entity(entity).insert(ExplorerSearchQuery {
            query: request.query.clone(),
            regex: request.regex,
            case_sensitive: request.case_sensitive,
            whole_word: request.whole_word,
            generation: 1,
        });
    }
    commands.trigger(ExplorerSearchCancelRequest { entity });
    commands
        .entity(entity)
        .remove::<ExplorerSearchResults>()
        .insert(GlobalSearchDirty);
}

fn begin(
    trigger: On<UiInput<ExplorerSearchRequest>>,
    mut queries: Query<&mut ExplorerSearchQuery>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let generation = if let Ok(mut query) = queries.get_mut(entity) {
        query.set_form(
            request.query.clone(),
            request.regex,
            request.case_sensitive,
            request.whole_word,
        )
    } else {
        commands.entity(entity).insert(ExplorerSearchQuery {
            query: request.query.clone(),
            regex: request.regex,
            case_sensitive: request.case_sensitive,
            whole_word: request.whole_word,
            generation: 1,
        });
        1
    };
    commands.trigger(ExplorerSearchCancelRequest { entity });
    commands
        .entity(entity)
        .remove::<ExplorerSearchResults>()
        .insert(GlobalSearchDirty);
    commands.trigger(ExplorerSearchRunRequest {
        entity,
        generation,
        query: request.query.clone(),
        regex: request.regex,
        case_sensitive: request.case_sensitive,
        whole_word: request.whole_word,
    });
}

fn open(
    trigger: On<UiInput<ExplorerSearchOpen>>,
    mut results: Query<&mut ExplorerSearchResults>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    if let Ok(mut search) = results.get_mut(entity) {
        search.opened =
            vmux_ecs::event::ExplorerSearchMatch::key_at(&request.path, request.line, request.col);
        commands.entity(entity).insert(GlobalSearchDirty);
    }
    commands.trigger(FileNavigateRequest::new(
        entity,
        PathBuf::from(&request.path),
        request.line.saturating_sub(1),
    ));
    commands.entity(entity).insert(PendingGoto::selection(
        request.line.saturating_sub(1),
        request.col,
        request.end_col,
    ));
}

fn toggle_group(
    trigger: On<UiInput<ExplorerSearchGroupToggle>>,
    mut results: Query<&mut ExplorerSearchResults>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut search) = results.get_mut(entity) else {
        return;
    };
    let path = &trigger.event().payload.path;
    if let Some(index) = search.collapsed.iter().position(|entry| entry == path) {
        search.collapsed.remove(index);
    } else {
        search.collapsed.push(path.clone());
    }
    commands.entity(entity).insert(GlobalSearchDirty);
}

fn collapse_all(
    trigger: On<UiInput<ExplorerSearchCollapseAll>>,
    mut results: Query<&mut ExplorerSearchResults>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut search) = results.get_mut(entity) else {
        return;
    };
    let mut collapsed = Vec::with_capacity(search.files.len());
    for file in &search.files {
        collapsed.push(file.path.clone());
    }
    search.collapsed = collapsed;
    commands.entity(entity).insert(GlobalSearchDirty);
}

fn clear(
    trigger: On<UiInput<ExplorerSearchClear>>,
    mut queries: Query<&mut ExplorerSearchQuery>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if let Ok(mut query) = queries.get_mut(entity) {
        query.clear();
    } else {
        commands.entity(entity).insert(ExplorerSearchQuery {
            generation: 1,
            ..Default::default()
        });
    }
    commands.trigger(ExplorerSearchCancelRequest { entity });
    commands
        .entity(entity)
        .remove::<ExplorerSearchResults>()
        .insert(GlobalSearchDirty);
}

fn navigate(
    trigger: On<FileNavigateRequest>,
    mut results: Query<&mut ExplorerSearchResults>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(mut search) = results.get_mut(entity) else {
        return;
    };
    let path = trigger.event().path.to_string_lossy();
    let prefix = format!("{path}:");
    if search.opened.starts_with(&prefix) {
        return;
    }
    search.opened.clear();
    commands.entity(entity).insert(GlobalSearchDirty);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ContractPlugin;
    use bevy::ecs::message::Messages;
    use bevy_cef::prelude::{BinIpcEventRawBuffer, Browsers};

    use crate::lsp::manager::LspGoto;

    #[test]
    fn global_search_opens_only_the_target_stack_explorer() {
        let mut app = App::new();
        app.world_mut().spawn(ExplorerPanelDefaults {
            default_visible: false,
            width: 240,
            loaded: true,
        });
        app.add_plugins((MinimalPlugins, ContractPlugin, SearchPlugin))
            .init_resource::<BinIpcEventRawBuffer>();
        app.world_mut().insert_non_send(Browsers::default());
        let first_stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let second_stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let target = PathBuf::from("/project/a.rs");
        let first = app
            .world_mut()
            .spawn((
                FileView {
                    path: target.clone(),
                },
                ChildOf(first_stack),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::from("/project/b.rs"),
                },
                ChildOf(second_stack),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<GlobalSearchRequest>>()
            .write(GlobalSearchRequest {
                target_path: target,
                root: "/project".to_string(),
                query: "needle".to_string(),
                regex: false,
                case_sensitive: false,
                whole_word: false,
                files: Vec::new(),
                capped: false,
            });
        app.update();

        assert!(
            app.world()
                .get::<StackExplorerVisibility>(first_stack)
                .unwrap()
                .visible
        );
        assert!(
            !app.world()
                .get::<StackExplorerVisibility>(second_stack)
                .unwrap()
                .visible
        );
        assert!(app.world().get::<ExplorerSearchResults>(first).is_some());
        assert!(app.world().get::<ExplorerSearchResults>(second).is_none());
        assert_eq!(
            app.world_mut()
                .query::<&PendingGlobalSearch>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn global_search_waits_as_a_request_entity_for_its_target() {
        let mut app = App::new();
        app.world_mut().spawn(ExplorerPanelDefaults {
            default_visible: false,
            width: 240,
            loaded: true,
        });
        app.add_plugins((MinimalPlugins, ContractPlugin, SearchPlugin))
            .init_resource::<BinIpcEventRawBuffer>();
        app.world_mut().insert_non_send(Browsers::default());
        let target = PathBuf::from("/project/later.rs");
        app.world_mut()
            .resource_mut::<Messages<GlobalSearchRequest>>()
            .write(GlobalSearchRequest {
                target_path: target.clone(),
                root: "/project".to_string(),
                query: "needle".to_string(),
                regex: false,
                case_sensitive: false,
                whole_word: false,
                files: Vec::new(),
                capped: false,
            });
        app.update();

        assert_eq!(
            app.world_mut()
                .query::<&PendingGlobalSearch>()
                .iter(app.world())
                .count(),
            1
        );

        let stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let view = app
            .world_mut()
            .spawn((FileView { path: target }, ChildOf(stack)))
            .id();
        app.update();

        assert!(app.world().get::<ExplorerSearchResults>(view).is_some());
        assert_eq!(
            app.world_mut()
                .query::<&PendingGlobalSearch>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn stale_completed_search_generation_is_refused() {
        let mut app = App::new();
        app.world_mut().spawn(ExplorerPanelDefaults {
            default_visible: false,
            width: 240,
            loaded: true,
        });
        app.add_plugins((MinimalPlugins, ContractPlugin, SearchPlugin))
            .init_resource::<BinIpcEventRawBuffer>();
        app.world_mut().insert_non_send(Browsers::default());
        let view = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/project/a.rs"),
            })
            .id();
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: ExplorerSearchRequest {
                query: "first".to_string(),
                regex: false,
                case_sensitive: false,
                whole_word: false,
            },
        });
        app.world_mut().flush();
        let first_generation = app
            .world()
            .get::<ExplorerSearchQuery>(view)
            .unwrap()
            .generation;
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: ExplorerSearchClear,
        });
        app.world_mut().trigger(ExplorerSearchCompleted {
            entity: view,
            generation: first_generation,
            root: "/project".to_string(),
            query: "first".to_string(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
            files: vec![ExplorerSearchFile {
                path: "/project/a.rs".to_string(),
                matches: Vec::new(),
                capped: false,
            }],
            capped: false,
        });
        assert!(app.world().get::<ExplorerSearchResults>(view).is_none());
    }

    #[test]
    fn explorer_goto_writes_lsp_goto_message() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SearchPlugin))
            .add_message::<LspGoto>();
        let entity = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/x.rs"),
            })
            .id();
        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: ExplorerGoto {
                path: "/x.rs".to_string(),
                line: 12,
            },
        });
        let mut messages = app.world_mut().resource_mut::<Messages<LspGoto>>();
        let received: Vec<_> = messages.drain().collect();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].line, 12);
        assert_eq!(received[0].path, PathBuf::from("/x.rs"));
    }
}
