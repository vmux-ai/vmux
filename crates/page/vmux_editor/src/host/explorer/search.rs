use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::event::{ExplorerGoto, ExplorerSearchEvent, ExplorerSearchFile, ExplorerSearchOpen};

use super::panel::{ExplorerPanelDefaults, ExplorerPanelSent, StackExplorerVisibility};
use crate::host::plugin::{FileView, PendingGoto};
use crate::host::viewport::FileViewport;

pub(in crate::host) struct ExplorerSearchPlugin;

impl Plugin for ExplorerSearchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingGlobalSearch>()
            .add_plugins(UiEventPlugin::<(ExplorerGoto, ExplorerSearchOpen)>::default())
            .add_systems(
                Update,
                (
                    apply_global_search_requests,
                    emit_global_search.after(apply_global_search_requests),
                ),
            )
            .add_observer(on_explorer_goto)
            .add_observer(on_explorer_search_open);
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchRequest {
    pub target_path: PathBuf,
    pub root: String,
    pub query: String,
    pub files: Vec<ExplorerSearchFile>,
    pub capped: bool,
}

#[derive(Component, Clone)]
struct GlobalSearchState(ExplorerSearchEvent);

#[derive(Component)]
struct GlobalSearchDirty;

#[derive(Resource, Default)]
struct PendingGlobalSearch(Vec<PendingGlobalSearchRequest>);

struct PendingGlobalSearchRequest {
    request: GlobalSearchRequest,
    retries_left: u8,
}

const GLOBAL_SEARCH_RETRY_LIMIT: u8 = 120;

type GlobalSearchDirtyReady = (
    With<GlobalSearchState>,
    With<GlobalSearchDirty>,
    With<vmux_core::page::PageReady>,
);
type NavigableFileView = (
    &'static mut FileView,
    &'static mut FileViewport,
    &'static mut PageMetadata,
);

fn on_explorer_goto(
    trigger: On<BinReceive<ExplorerGoto>>,
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

fn apply_global_search_requests(
    mut reader: MessageReader<GlobalSearchRequest>,
    views: Query<(Entity, &FileView, Option<&ChildOf>)>,
    visibility: Query<&StackExplorerVisibility>,
    mut pending: ResMut<PendingGlobalSearch>,
    panel: Res<ExplorerPanelDefaults>,
    mut commands: Commands,
) {
    for request in reader.read() {
        pending.0.push(PendingGlobalSearchRequest {
            request: request.clone(),
            retries_left: GLOBAL_SEARCH_RETRY_LIMIT,
        });
    }
    let mut remaining = Vec::new();
    for mut pending_request in pending.0.drain(..) {
        let request = &pending_request.request;
        let Some((entity, _, parent)) = views
            .iter()
            .find(|(_, view, _)| view.path == request.target_path)
        else {
            pending_request.retries_left = pending_request.retries_left.saturating_sub(1);
            if pending_request.retries_left > 0 {
                remaining.push(pending_request);
            }
            continue;
        };
        let scope = parent.map(ChildOf::parent).unwrap_or(entity);
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
        let request = pending_request.request;
        commands.entity(entity).insert((
            GlobalSearchState(ExplorerSearchEvent {
                root: request.root,
                query: request.query,
                files: request.files,
                capped: request.capped,
            }),
            GlobalSearchDirty,
        ));
    }
    pending.0 = remaining;
}

fn emit_global_search(
    query: Query<(Entity, &GlobalSearchState), GlobalSearchDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, search) in &query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_event(entity, &search.0));
        commands.entity(entity).remove::<GlobalSearchDirty>();
    }
}

fn on_explorer_search_open(
    trigger: On<BinReceive<ExplorerSearchOpen>>,
    mut views: Query<NavigableFileView>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut view, mut viewport, mut metadata)) = views.get_mut(entity) else {
        return;
    };
    view.navigate(
        entity,
        PathBuf::from(&request.path),
        request.line.saturating_sub(1),
        &mut viewport,
        &mut metadata,
        &mut manager,
        &mut commands,
    );
    commands.entity(entity).insert(PendingGoto::selection(
        request.line.saturating_sub(1),
        request.col,
        request.end_col,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::EditorContractPlugin;
    use bevy::ecs::message::Messages;

    #[test]
    fn global_search_opens_only_the_target_stack_explorer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, EditorContractPlugin, ExplorerSearchPlugin))
            .init_resource::<BinIpcEventRawBuffer>()
            .insert_resource(ExplorerPanelDefaults {
                default_visible: false,
                width: 240,
            });
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
    }

    #[test]
    fn explorer_goto_writes_lsp_goto_message() {
        use crate::lsp::manager::LspGoto;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ExplorerSearchPlugin))
            .add_message::<LspGoto>();
        let entity = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/x.rs"),
            })
            .id();
        app.world_mut().trigger(BinReceive {
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
