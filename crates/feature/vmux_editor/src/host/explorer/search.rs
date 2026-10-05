use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
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
            .add_observer(update_form)
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

#[derive(Component, Clone)]
struct GlobalSearchState(ExplorerSearchEvent);

#[derive(Component)]
struct GlobalSearchDirty;

#[derive(EntityEvent)]
struct SearchFormChanged {
    #[event_target]
    entity: Entity,
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
}

#[derive(Component)]
struct PendingGlobalSearch {
    request: GlobalSearchRequest,
    retries_left: u8,
}

impl PendingGlobalSearch {
    fn new(request: GlobalSearchRequest) -> Self {
        Self {
            request,
            retries_left: GLOBAL_SEARCH_RETRY_LIMIT,
        }
    }

    fn retry(&mut self) -> bool {
        self.retries_left = self.retries_left.saturating_sub(1);
        self.retries_left > 0
    }
}

const GLOBAL_SEARCH_RETRY_LIMIT: u8 = 120;

type GlobalSearchDirtyReady = (
    With<GlobalSearchState>,
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
    searches: Query<&GlobalSearchState>,
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
        if let Ok(search) = searches.get(entity)
            && (search.0.query != request.query
                || search.0.regex != request.regex
                || search.0.case_sensitive != request.case_sensitive
                || search.0.whole_word != request.whole_word)
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
        commands.entity(entity).insert((
            GlobalSearchState(ExplorerSearchEvent {
                root: request.root,
                query: request.query,
                regex: request.regex,
                case_sensitive: request.case_sensitive,
                whole_word: request.whole_word,
                files: request.files,
                capped: request.capped,
                collapsed: Vec::new(),
                opened: String::new(),
            }),
            GlobalSearchDirty,
        ));
        commands.entity(pending_entity).despawn();
    }
}

fn emit(
    query: Query<(Entity, &GlobalSearchState), GlobalSearchDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, search) in &query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &search.0));
        commands.entity(entity).remove::<GlobalSearchDirty>();
    }
}

fn draft(trigger: On<UiInput<ExplorerSearchDraftRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(SearchFormChanged {
        entity: trigger.event().webview,
        query: request.query.clone(),
        regex: request.regex,
        case_sensitive: request.case_sensitive,
        whole_word: request.whole_word,
    });
}

fn begin(trigger: On<UiInput<ExplorerSearchRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(SearchFormChanged {
        entity: trigger.event().webview,
        query: request.query.clone(),
        regex: request.regex,
        case_sensitive: request.case_sensitive,
        whole_word: request.whole_word,
    });
}

fn update_form(
    trigger: On<SearchFormChanged>,
    mut searches: Query<&mut GlobalSearchState>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if let Ok(mut search) = searches.get_mut(event.entity) {
        if search.0.query == event.query
            && search.0.regex == event.regex
            && search.0.case_sensitive == event.case_sensitive
            && search.0.whole_word == event.whole_word
        {
            return;
        }
        search.0.root.clear();
        search.0.query.clone_from(&event.query);
        search.0.regex = event.regex;
        search.0.case_sensitive = event.case_sensitive;
        search.0.whole_word = event.whole_word;
        search.0.files.clear();
        search.0.capped = false;
        search.0.collapsed.clear();
        search.0.opened.clear();
    } else {
        commands
            .entity(event.entity)
            .insert(GlobalSearchState(ExplorerSearchEvent {
                query: event.query.clone(),
                regex: event.regex,
                case_sensitive: event.case_sensitive,
                whole_word: event.whole_word,
                ..Default::default()
            }));
    }
    commands.entity(event.entity).insert(GlobalSearchDirty);
}

fn open(
    trigger: On<UiInput<ExplorerSearchOpen>>,
    mut searches: Query<&mut GlobalSearchState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    if let Ok(mut search) = searches.get_mut(entity) {
        search.0.opened =
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
    mut searches: Query<&mut GlobalSearchState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut search) = searches.get_mut(entity) else {
        return;
    };
    let path = &trigger.event().payload.path;
    if let Some(index) = search.0.collapsed.iter().position(|entry| entry == path) {
        search.0.collapsed.remove(index);
    } else {
        search.0.collapsed.push(path.clone());
    }
    commands.entity(entity).insert(GlobalSearchDirty);
}

fn collapse_all(
    trigger: On<UiInput<ExplorerSearchCollapseAll>>,
    mut searches: Query<&mut GlobalSearchState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut search) = searches.get_mut(entity) else {
        return;
    };
    let mut collapsed = Vec::with_capacity(search.0.files.len());
    for file in &search.0.files {
        collapsed.push(file.path.clone());
    }
    search.0.collapsed = collapsed;
    commands.entity(entity).insert(GlobalSearchDirty);
}

fn clear(
    trigger: On<UiInput<ExplorerSearchClear>>,
    mut searches: Query<&mut GlobalSearchState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if let Ok(mut search) = searches.get_mut(entity) {
        search.0 = ExplorerSearchEvent::default();
    } else {
        commands
            .entity(entity)
            .insert(GlobalSearchState(ExplorerSearchEvent::default()));
    }
    commands.entity(entity).insert(GlobalSearchDirty);
}

fn navigate(
    trigger: On<FileNavigateRequest>,
    mut searches: Query<&mut GlobalSearchState>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(mut search) = searches.get_mut(entity) else {
        return;
    };
    let path = trigger.event().path.to_string_lossy();
    let prefix = format!("{path}:");
    if search.0.opened.starts_with(&prefix) {
        return;
    }
    search.0.opened.clear();
    commands.entity(entity).insert(GlobalSearchDirty);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ContractPlugin;
    use bevy::ecs::message::Messages;

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
        assert!(app.world().get::<GlobalSearchState>(first).is_some());
        assert!(app.world().get::<GlobalSearchState>(second).is_none());
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

        assert!(app.world().get::<GlobalSearchState>(view).is_some());
        assert_eq!(
            app.world_mut()
                .query::<&PendingGlobalSearch>()
                .iter(app.world())
                .count(),
            0
        );
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
