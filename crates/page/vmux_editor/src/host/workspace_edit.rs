use bevy::prelude::*;
use bevy_cef::prelude::*;

use super::editing::EditRequest;
use super::editor::{Editor, FileView};
use super::file_lifecycle::{SelfWrites, canon};
use crate::edit::EditCommand;
use crate::lsp::workspace_edit::WorkspaceEditPlan;

pub(super) struct WorkspaceEditPlugin;

impl Plugin for WorkspaceEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::lsp::manager::LspRequestedEdit>()
            .add_systems(
                Update,
                apply_lsp_workspace_edit
                    .in_set(crate::lsp::server_request::ServerRequestSet::Answer),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_lsp_workspace_edit(
    requests: Query<(Entity, &crate::lsp::server_request::AwaitingApplyEdit)>,
    views: Query<(Entity, &FileView, &Editor)>,
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
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                rename.entity,
                &vmux_core::event::FileEditFailure { reason },
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_planned_documents(
    plan: WorkspaceEditPlan,
    views: &Query<(Entity, &FileView, &Editor)>,
    self_writes: &mut SelfWrites,
    manager: &crate::lsp::manager::LspManager,
    commands: &mut Commands,
) -> Option<String> {
    let prepared = match PreparedWorkspaceEdit::new(plan, views, manager) {
        Ok(prepared) => prepared,
        Err(reason) => return Some(reason),
    };
    prepared.apply(self_writes, commands).err()
}

struct PreparedWorkspaceEdit {
    documents: Vec<PreparedDocument>,
}

impl PreparedWorkspaceEdit {
    fn new(
        plan: WorkspaceEditPlan,
        views: &Query<(Entity, &FileView, &Editor)>,
        manager: &crate::lsp::manager::LspManager,
    ) -> Result<Self, String> {
        let mut documents = Vec::with_capacity(plan.documents.len());
        for document in plan.documents {
            documents.push(PreparedDocument::new(document, views, manager)?);
        }
        Ok(Self { documents })
    }

    fn apply(self, self_writes: &mut SelfWrites, commands: &mut Commands) -> Result<(), String> {
        for document in self.documents {
            document.apply(self_writes, commands)?;
        }
        Ok(())
    }
}

struct PreparedDocument {
    path: vmux_path::ScopedPath,
    targets: Vec<Entity>,
    updated: String,
}

impl PreparedDocument {
    fn new(
        document: crate::lsp::workspace_edit::PlannedDocument,
        views: &Query<(Entity, &FileView, &Editor)>,
        manager: &crate::lsp::manager::LspManager,
    ) -> Result<Self, String> {
        if let (Some(expected), Some(actual)) = (
            document.version,
            manager.document_version(document.path.as_path()),
        ) && expected != actual
        {
            return Err(format!(
                "{} changed since the edit was computed",
                document.path.as_path().display()
            ));
        }

        let wanted = canon(document.path.as_path());
        let targets: Vec<Entity> = views
            .iter()
            .filter(|(_, view, ..)| canon(&view.path) == wanted)
            .map(|(entity, ..)| entity)
            .collect();
        let source = if targets.is_empty() {
            std::fs::read_to_string(document.path.as_path())
                .map_err(|_| format!("{} could not be read", document.path.as_path().display()))?
        } else {
            let mut texts = targets
                .iter()
                .filter_map(|entity| views.get(*entity).ok())
                .map(|(_, _, editor)| editor.core.buffer.text());
            let first = texts.next().unwrap_or_default();
            if texts.any(|text| text != first) {
                return Err(format!(
                    "{} is open more than once with different contents",
                    document.path.as_path().display()
                ));
            }
            first
        };
        let buffer = crate::edit::buffer::TextBuffer::from_text(
            document.path.as_path().to_path_buf(),
            String::new(),
            &source,
        );
        let updated = buffer
            .with_lsp_edits(&document.edits)
            .map_err(|error| format!("{}: {error}", document.path.as_path().display()))?;
        Ok(Self {
            path: document.path,
            targets,
            updated,
        })
    }

    fn apply(self, self_writes: &mut SelfWrites, commands: &mut Commands) -> Result<(), String> {
        if self.targets.is_empty() {
            vmux_path::AtomicFile::write(self.path.as_path(), self.updated.as_bytes())
                .map_err(|error| format!("{}: {error}", self.path.as_path().display()))?;
            self_writes
                .0
                .insert(canon(self.path.as_path()), std::time::Instant::now());
            return Ok(());
        }
        for entity in self.targets {
            commands.trigger(EditRequest::new(
                entity,
                vec![EditCommand::ReplaceText(self.updated.clone())],
            ));
        }
        Ok(())
    }
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
            Self::with_workspace_edit(path, panes, Self::renaming(path))
        }

        fn with_workspace_edit(path: &Path, panes: usize, edit: lsp_types::WorkspaceEdit) -> Self {
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
                    params: lsp_types::ApplyWorkspaceEditParams { label: None, edit },
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
                WorkspaceEditPlugin,
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

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn a_later_invalid_document_leaves_earlier_documents_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("a.rs");
        let invalid = temp.path().join("z.rs");
        std::fs::write(&first, ApplyEdit::BEFORE).unwrap();
        std::fs::write(&invalid, ApplyEdit::BEFORE).unwrap();

        let uri = |path: &Path| -> lsp_types::Uri {
            format!("file://{}", path.display()).parse().unwrap()
        };
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            uri(&first),
            vec![lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: 0,
                    },
                    end: lsp_types::Position {
                        line: 0,
                        character: 3,
                    },
                },
                new_text: "1".to_string(),
            }],
        );
        changes.insert(
            uri(&invalid),
            vec![
                lsp_types::TextEdit {
                    range: lsp_types::Range {
                        start: lsp_types::Position {
                            line: 0,
                            character: 0,
                        },
                        end: lsp_types::Position {
                            line: 0,
                            character: 4,
                        },
                    },
                    new_text: "invalid".to_string(),
                },
                lsp_types::TextEdit {
                    range: lsp_types::Range {
                        start: lsp_types::Position {
                            line: 0,
                            character: 2,
                        },
                        end: lsp_types::Position {
                            line: 0,
                            character: 6,
                        },
                    },
                    new_text: "overlap".to_string(),
                },
            ],
        );
        let mut edit = ApplyEdit::with_workspace_edit(
            &first,
            0,
            lsp_types::WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            },
        );

        edit.app.update();

        assert_eq!(std::fs::read_to_string(&first).unwrap(), ApplyEdit::BEFORE);
        assert_eq!(
            std::fs::read_to_string(&invalid).unwrap(),
            ApplyEdit::BEFORE
        );
        let reply = edit.sent.try_recv().unwrap();
        assert_eq!(reply["result"]["applied"], false);
    }
}
