use bevy::prelude::*;
use bevy_cef::prelude::*;

use super::editing::EditRequest;
use super::editor::{Editor, FileView};
use super::file_lifecycle::{SelfWrites, canon};
use crate::edit::EditCommand;
use crate::lsp::workspace_edit::WorkspaceEditPlan;

pub(super) struct EditorWorkspaceEditPlugin;

impl Plugin for EditorWorkspaceEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::lsp::manager::LspRequestedEdit>()
            .add_systems(
                Update,
                apply_lsp_workspace_edit
                    .in_set(crate::lsp::server_request::ServerRequestSet::Answer),
            );
    }
}

type EditableFileViews = (Entity, &'static FileView, &'static Editor);

#[allow(clippy::too_many_arguments)]
fn apply_lsp_workspace_edit(
    requests: Query<(Entity, &crate::lsp::server_request::AwaitingApplyEdit)>,
    views: Query<EditableFileViews>,
    mut self_writes: NonSendMut<SelfWrites>,
    manager: Res<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut replies: MessageWriter<crate::lsp::server_request::ServerReply>,
    mut renames: MessageReader<crate::lsp::manager::LspRequestedEdit>,
    mut commands: Commands,
) {
    for (request, awaiting) in &requests {
        let refusal = match WorkspaceEditPlan::within(&awaiting.root, &awaiting.params.edit) {
            Ok(plan) => {
                apply_planned_documents(plan, &views, &mut self_writes, &manager, &mut commands)
            }
            Err(refusal) => Some(refusal.to_string()),
        };
        replies.write(crate::lsp::server_request::ServerReply {
            request,
            result: match &refusal {
                None => serde_json::json!({ "applied": true }),
                Some(reason) => {
                    serde_json::json!({ "applied": false, "failureReason": reason })
                }
            },
        });
    }

    for rename in renames.read() {
        let refusal = match &rename.result {
            Err(reason) => Some(reason.clone()),
            Ok(edit) => match WorkspaceEditPlan::within(&rename.root, edit) {
                Ok(plan) => {
                    apply_planned_documents(plan, &views, &mut self_writes, &manager, &mut commands)
                }
                Err(refusal) => Some(refusal.to_string()),
            },
        };
        let Some(reason) = refusal else {
            continue;
        };
        if browsers.can_emit_to(&rename.entity) {
            commands.trigger(BinHostEmitEvent::from_event(
                rename.entity,
                &vmux_core::event::FileEditFailedEvent { reason },
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_planned_documents(
    plan: WorkspaceEditPlan,
    views: &Query<EditableFileViews>,
    self_writes: &mut SelfWrites,
    manager: &crate::lsp::manager::LspManager,
    commands: &mut Commands,
) -> Option<String> {
    for document in plan.documents {
        let wanted = canon(document.path.as_path());
        if let (Some(expected), Some(actual)) = (
            document.version,
            manager.document_version(document.path.as_path()),
        ) && expected != actual
        {
            return Some(format!(
                "{} changed since the edit was computed",
                document.path.as_path().display()
            ));
        }

        let open: Vec<Entity> = views
            .iter()
            .filter(|(_, view, ..)| canon(&view.path) == wanted)
            .map(|(entity, ..)| entity)
            .collect();

        if open.is_empty() {
            if let Err(reason) = edit_closed_file(&document, self_writes) {
                return Some(reason);
            }
            continue;
        }

        let mut texts = open
            .iter()
            .filter_map(|entity| views.get(*entity).ok())
            .map(|(_, _, edit, ..)| edit.core.buffer.text());
        let first = texts.next().unwrap_or_default();
        if texts.any(|text| text != first) {
            return Some(format!(
                "{} is open more than once with different contents",
                document.path.as_path().display()
            ));
        }

        for entity in open {
            let Ok((_, _, edit)) = views.get(entity) else {
                continue;
            };
            let updated = match edit.core.buffer.with_lsp_edits(&document.edits) {
                Ok(updated) => updated,
                Err(error) => {
                    return Some(format!("{}: {error}", document.path.as_path().display()));
                }
            };
            commands.trigger(EditRequest::new(
                entity,
                vec![EditCommand::ReplaceText(updated)],
            ));
        }
    }
    None
}

fn edit_closed_file(
    document: &crate::lsp::workspace_edit::PlannedDocument,
    self_writes: &mut SelfWrites,
) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(document.path.as_path()) else {
        return Err(format!(
            "{} could not be read",
            document.path.as_path().display()
        ));
    };
    let buffer = crate::edit::buffer::TextBuffer::from_text(
        document.path.as_path().to_path_buf(),
        String::new(),
        &text,
    );
    let updated = match buffer.with_lsp_edits(&document.edits) {
        Ok(updated) => updated,
        Err(error) => return Err(format!("{}: {error}", document.path.as_path().display())),
    };
    self_writes
        .0
        .insert(canon(document.path.as_path()), std::time::Instant::now());
    vmux_path::AtomicFile::write(document.path.as_path(), updated.as_bytes())
        .map_err(|error| format!("{}: {error}", document.path.as_path().display()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::edit::highlight_cache::HighlightCache;
    use crate::edit::{EditCore, EditMode};
    use crate::host::editing::ClipboardHandle;
    use crate::host::keymap::EditorKeymap;
    use crate::host::viewport::FileViewport;
    use crate::keymap::KeymapKindExt;

    struct ApplyEdit {
        app: App,
        views: Vec<Entity>,
        sent: std::sync::mpsc::Receiver<serde_json::Value>,
    }

    impl ApplyEdit {
        const BEFORE: &'static str = "one two three\n";

        fn renamed(path: &Path, panes: usize) -> Self {
            let (mut app, views) = Self::bare(path, panes);
            app.world_mut()
                .write_message(crate::lsp::manager::LspRequestedEdit {
                    entity: views[0],
                    root: path.parent().unwrap_or(path).to_path_buf(),
                    result: Ok(Self::renaming(path)),
                });
            let (_outgoing, sent) = std::sync::mpsc::channel();
            Self { app, views, sent }
        }

        fn with_edit(path: &Path, panes: usize) -> Self {
            let (app, views) = Self::bare(path, panes);
            let (outgoing, sent) = std::sync::mpsc::channel();
            let events = app
                .world()
                .resource::<crate::lsp::server_request::ServerEvents>()
                .sender();
            events
                .send(crate::lsp::server_request::ServerEvent::ApplyEdit {
                    reply: crate::lsp::server_request::ReplyHandle::new(
                        crate::lsp::wire::RequestId::Number(1000),
                        outgoing,
                    ),
                    root: path.parent().unwrap_or(path).to_path_buf(),
                    params: lsp_types::ApplyWorkspaceEditParams {
                        label: None,
                        edit: Self::renaming(path),
                    },
                })
                .unwrap();
            Self { app, views, sent }
        }

        fn bare(path: &Path, panes: usize) -> (App, Vec<Entity>) {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                crate::lsp::server_request::ServerRequestPlugin,
                super::super::editing::EditExecutionPlugin,
                EditorWorkspaceEditPlugin,
            ));
            app.world_mut().insert_non_send(ClipboardHandle(None));
            app.world_mut().insert_non_send(SelfWrites::default());
            app.world_mut().insert_non_send(Browsers::default());
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));

            let mut views = Vec::new();
            for _ in 0..panes {
                let core = EditCore::new(
                    path.to_path_buf(),
                    "Rust".into(),
                    Self::BEFORE,
                    EditMode::Normal,
                );
                views.push(
                    app.world_mut()
                        .spawn((
                            FileView {
                                path: path.to_path_buf(),
                            },
                            Editor::new(
                                core,
                                HighlightCache::new(path),
                                crate::fold::FoldState::default(),
                            ),
                            EditorKeymap(vmux_core::editor::KeymapKind::Vscode.make(&[], "\\")),
                            FileViewport {
                                top_row: 0,
                                rows: 0,
                                wrap_columns: 0,
                                word_wrap: vmux_core::editor::WordWrap::default(),
                                word_wrap_column: 80,
                            },
                            vmux_git::GitDiffSource {
                                content: Self::BEFORE.to_string(),
                                dirty: false,
                            },
                        ))
                        .id(),
                );
            }

            (app, views)
        }

        #[allow(clippy::mutable_key_type)]
        fn renaming(path: &Path) -> lsp_types::WorkspaceEdit {
            let edit = |start: u32, end: u32, text: &str| lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: start,
                    },
                    end: lsp_types::Position {
                        line: 0,
                        character: end,
                    },
                },
                new_text: text.to_string(),
            };
            let uri: lsp_types::Uri = format!("file://{}", path.display()).parse().unwrap();
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri, vec![edit(8, 13, "3"), edit(0, 3, "1")]);
            lsp_types::WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }
        }

        fn text(&self, entity: Entity) -> String {
            self.app
                .world()
                .get::<Editor>(entity)
                .unwrap()
                .core
                .buffer
                .text()
        }

        fn undo(&mut self, entity: Entity) {
            self.app
                .world_mut()
                .get_mut::<Editor>(entity)
                .unwrap()
                .core
                .apply(EditCommand::Undo);
        }
    }

    #[test]
    fn a_rename_reply_edits_the_panes_the_way_an_apply_edit_request_does() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut edit = ApplyEdit::renamed(&path, 2);
        edit.app.update();

        for view in edit.views.clone() {
            assert_eq!(edit.text(view), "1 two 3\n");
        }
    }

    #[test]
    fn apply_edit_reaches_every_pane_showing_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut edit = ApplyEdit::with_edit(&path, 2);
        edit.app.update();

        for view in edit.views.clone() {
            assert_eq!(edit.text(view), "1 two 3\n");
            assert!(
                edit.app.world().get::<Editor>(view).unwrap().core.dirty,
                "an applied edit leaves the buffer dirty for the user to save"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            ApplyEdit::BEFORE,
            "an open document is edited in the buffer, not written behind the user"
        );
    }

    #[test]
    fn the_whole_edit_undoes_in_one_step() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut edit = ApplyEdit::with_edit(&path, 1);
        edit.app.update();
        let view = edit.views[0];
        assert_eq!(edit.text(view), "1 two 3\n");

        edit.undo(view);
        assert_eq!(edit.text(view), ApplyEdit::BEFORE);
    }

    #[test]
    fn panes_that_have_drifted_apart_are_refused_rather_than_corrupted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut edit = ApplyEdit::with_edit(&path, 2);
        let second = edit.views[1];
        edit.app
            .world_mut()
            .get_mut::<Editor>(second)
            .unwrap()
            .core
            .apply(EditCommand::InsertText("MINE ".to_string()));
        edit.app.update();

        assert_eq!(
            edit.text(edit.views[0]),
            ApplyEdit::BEFORE,
            "left untouched"
        );
        assert_eq!(edit.text(second), "MINE one two three\n", "left untouched");

        let reply = edit.sent.try_recv().expect("the server must be answered");
        assert_eq!(reply["result"]["applied"], false);
        assert!(
            reply["result"]["failureReason"]
                .as_str()
                .is_some_and(|reason| reason.contains("different contents")),
            "the server is told why: {reply}"
        );
    }

    #[test]
    fn the_server_is_told_the_edit_applied() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut edit = ApplyEdit::with_edit(&path, 1);
        edit.app.update();

        let reply = edit.sent.try_recv().expect("the server must be answered");
        assert_eq!(reply["id"], 1000);
        assert_eq!(reply["result"]["applied"], true);
    }

    #[test]
    fn a_document_no_pane_shows_is_edited_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("closed.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut edit = ApplyEdit::with_edit(&path, 0);
        edit.app.update();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1 two 3\n");
        assert_eq!(edit.sent.try_recv().unwrap()["result"]["applied"], true);
    }
}
