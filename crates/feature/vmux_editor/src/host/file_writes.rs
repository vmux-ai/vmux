use std::path::Path;
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::file_lifecycle::FileChangesSet;
use super::file_write_io::SelfWrites;
use super::fold_preferences::FileId;

pub(super) struct FileWritesPlugin;

impl Plugin for FileWritesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (receive, ApplyDeferred, expire)
                .chain()
                .after(FileWriteSet)
                .before(FileChangesSet),
        )
        .insert_non_send(SelfWrites::default());
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FileWriteSet;

#[derive(Component)]
struct WrittenAt(Instant);

#[derive(SystemParam)]
pub(super) struct WriteReceipts<'w, 's> {
    written: Query<'w, 's, (&'static FileId, &'static WrittenAt)>,
}

impl WriteReceipts<'_, '_> {
    pub(super) fn recent(&self, path: &Path) -> bool {
        for (file, at) in &self.written {
            if file.0 == path && at.0.elapsed() < Duration::from_secs(2) {
                return true;
            }
        }
        false
    }
}

fn receive(
    input: Option<NonSendMut<SelfWrites>>,
    mut receipts: Query<(&FileId, &mut WrittenAt)>,
    mut commands: Commands,
) {
    let mut inputs = match input {
        Some(mut input) => input.0.take(),
        None => return,
    };
    inputs.sort_by(|left, right| left.path.cmp(&right.path));
    let mut inputs = inputs.into_iter().peekable();
    while let Some(mut input) = inputs.next() {
        while inputs.peek().is_some_and(|next| next.path == input.path) {
            input.at = input.at.max(inputs.next().unwrap().at);
        }
        let mut existing = false;
        for (file, mut at) in &mut receipts {
            if file.0 == input.path {
                at.0 = at.0.max(input.at);
                existing = true;
                break;
            }
        }
        if !existing {
            commands.spawn((
                Name::new("Editor file write receipt"),
                FileId(input.path),
                WrittenAt(input.at),
            ));
        }
    }
}

fn expire(receipts: Query<(Entity, &WrittenAt)>, mut commands: Commands) {
    for (entity, at) in &receipts {
        if at.0.elapsed() >= Duration::from_secs(2) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::edit::EditCommand;
    use super::super::editing::{ClipboardHandle, EditExecutionPlugin, EditRequest};
    use super::super::editor::{Editor, FileView};
    use super::super::file_lifecycle::{FileLifecyclePlugin, FileLoadTask};
    use super::super::file_watch::Observed;
    use super::super::viewport::FileViewport;
    use super::super::workspace_edit::WorkspaceEditPlugin;
    use super::*;
    use bevy_cef::prelude::Browsers;

    #[test]
    fn lsp_edits_commit_with_the_file_watch_and_document_pipeline_installed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file.txt");
        std::fs::write(&path, "one two three\n").unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            FileLifecyclePlugin,
            EditExecutionPlugin,
            WorkspaceEditPlugin,
            crate::lsp::server_request::ServerRequestPlugin,
            vmux_lsp::LspPlugin,
            crate::lsp::manager::ManagerPlugin,
        ))
        .insert_resource(vmux_setting::AppSettings::default())
        .insert_non_send(Browsers::default())
        .insert_non_send(ClipboardHandle(None))
        .add_message::<crate::lsp::manager_page::PackageInstallRequest>();
        app.add_plugins(crate::lsp::registry::RegistryPlugin);
        app.update();

        let sender = app
            .world_mut()
            .query::<&crate::lsp::server_request::ServerInputSender>()
            .single(app.world())
            .unwrap()
            .clone();
        let (outgoing, reply) = std::sync::mpsc::channel();
        sender
            .apply_edits
            .send(crate::lsp::server_request::ApplyEditInput {
                reply: crate::lsp::server_request::ReplyHandle::new(
                    crate::lsp::wire::RequestId::Number(1000),
                    outgoing,
                ),
                root: directory.path().to_path_buf(),
                server: None,
                transport: None,
                params: lsp_types::ApplyWorkspaceEditParams {
                    label: None,
                    edit: lsp_types::WorkspaceEdit {
                        document_changes: Some(lsp_types::DocumentChanges::Edits(vec![
                            lsp_types::TextDocumentEdit {
                                text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                                    uri: url::Url::from_file_path(&path)
                                        .unwrap()
                                        .as_str()
                                        .parse()
                                        .unwrap(),
                                    version: None,
                                },
                                edits: vec![lsp_types::OneOf::Left(lsp_types::TextEdit {
                                    range: lsp_types::Range {
                                        start: lsp_types::Position::new(0, 0),
                                        end: lsp_types::Position::new(0, 3),
                                    },
                                    new_text: "replacement".into(),
                                })],
                            },
                        ])),
                        ..Default::default()
                    },
                },
            })
            .unwrap();
        app.update();
        assert_eq!(reply.try_recv().unwrap()["result"]["applied"], true);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "replacement two three\n"
        );
        app.update();
        let mut receipts = bevy::ecs::system::SystemState::<WriteReceipts>::new(app.world_mut());
        assert!(
            receipts
                .get(app.world())
                .unwrap()
                .recent(&path.canonicalize().unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn completed_write_keeps_its_identity_when_the_path_becomes_a_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let written = directory.path().join("written.txt");
        let target = directory.path().join("target.txt");
        std::fs::write(&written, "saved").unwrap();
        std::fs::write(&target, "external").unwrap();
        let identity = written.canonicalize().unwrap();
        let mut app = App::new();
        app.add_plugins(FileWritesPlugin);
        app.world_mut()
            .non_send_mut::<SelfWrites>()
            .0
            .insert(identity.clone(), Instant::now());
        std::fs::remove_file(&written).unwrap();
        std::os::unix::fs::symlink(&target, &written).unwrap();
        app.update();
        let mut receipts = app.world_mut().query::<&FileId>();
        let paths: Vec<_> = receipts
            .iter(app.world())
            .map(|file| file.0.clone())
            .collect();
        assert_eq!(paths, [identity]);
        assert_ne!(paths[0], target.canonicalize().unwrap());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "external");
    }

    #[test]
    fn out_of_order_writes_reuse_a_recent_receipt_until_it_expires() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file.txt");
        let mut app = App::new();
        app.add_plugins(FileWritesPlugin);
        for text in ["first", "latest"] {
            std::fs::write(&path, text).unwrap();
            app.world_mut()
                .non_send_mut::<SelfWrites>()
                .0
                .insert(path.canonicalize().unwrap(), Instant::now());
        }
        app.world_mut().non_send_mut::<SelfWrites>().0.insert(
            path.canonicalize().unwrap(),
            Instant::now() - Duration::from_secs(3),
        );
        app.update();
        let receipts: Vec<_> = app
            .world_mut()
            .query::<(Entity, &FileId, &WrittenAt)>()
            .iter(app.world())
            .map(|(entity, file, _)| (entity, file.0.clone()))
            .collect();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].1, path.canonicalize().unwrap());
        app.world_mut().non_send_mut::<SelfWrites>().0.insert(
            path.canonicalize().unwrap(),
            Instant::now() - Duration::from_secs(3),
        );
        app.update();
        assert!(app.world().get_entity(receipts[0].0).is_ok());
        app.world_mut()
            .get_mut::<WrittenAt>(receipts[0].0)
            .unwrap()
            .0 = Instant::now() - Duration::from_secs(3);
        app.update();
        assert!(app.world().get_entity(receipts[0].0).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "latest");
    }

    #[test]
    fn editor_save_suppresses_its_callback_and_expiry_allows_an_external_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file.txt");
        std::fs::write(&path, "original\n").unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, FileLifecyclePlugin, EditExecutionPlugin))
            .insert_non_send(Browsers::default())
            .insert_non_send(ClipboardHandle(None));
        let view = app
            .world_mut()
            .spawn((
                FileView { path: path.clone() },
                FileViewport {
                    top_row: 0,
                    rows: 0,
                    wrap_columns: 0,
                    word_wrap: vmux_api::editor::WordWrap::default(),
                    word_wrap_column: 80,
                    scroll_revision: 0,
                },
            ))
            .id();
        FileLoadTask::settle(&mut app, view);
        EditRequest::new(view, vec![EditCommand::Save]).emit_world(app.world_mut());
        app.world_mut()
            .write_message(Observed::from_notify(Ok(notify::Event::new(
                notify::EventKind::Any,
            )
            .add_path(path.clone()))));
        app.update();
        assert!(app.world().get::<FileLoadTask>(view).is_none());
        assert!(Editor::read(app.world(), view).is_some());
        let receipts: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<WrittenAt>>()
            .iter(app.world())
            .collect();
        assert_eq!(receipts.len(), 1);
        app.world_mut().get_mut::<WrittenAt>(receipts[0]).unwrap().0 =
            Instant::now() - Duration::from_secs(3);
        std::fs::write(&path, "external\n").unwrap();
        app.world_mut()
            .write_message(Observed::from_notify(Ok(notify::Event::new(
                notify::EventKind::Any,
            )
            .add_path(path))));
        FileLoadTask::settle(&mut app, view);
        assert_eq!(
            Editor::read(app.world(), view).unwrap().core.buffer.text(),
            "external\n"
        );
    }
}
