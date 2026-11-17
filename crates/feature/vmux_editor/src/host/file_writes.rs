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
            (receive, expire)
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

fn receive(world: &mut World) {
    let inputs = match world.get_non_send_mut::<SelfWrites>() {
        Some(mut input) => input.0.take(),
        None => return,
    };
    for input in inputs {
        let path = input.path;
        let mut existing = None;
        let mut receipts = world.query::<(Entity, &FileId, &WrittenAt)>();
        for (entity, file, _) in receipts.iter(world) {
            if file.0 == path {
                existing = Some(entity);
                break;
            }
        }
        match existing {
            Some(entity) => {
                let mut at = world.get_mut::<WrittenAt>(entity).unwrap();
                at.0 = at.0.max(input.at);
            }
            None => {
                world.spawn((
                    Name::new("Editor file write receipt"),
                    FileId(path),
                    WrittenAt(input.at),
                ));
            }
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
    use super::*;
    use bevy_cef::prelude::Browsers;

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
    fn completed_writes_reuse_one_receipt_and_expired_receipts_retire() {
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
        app.update();
        let receipts: Vec<_> = app
            .world_mut()
            .query::<(Entity, &FileId, &WrittenAt)>()
            .iter(app.world())
            .map(|(entity, file, _)| (entity, file.0.clone()))
            .collect();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].1, path.canonicalize().unwrap());
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
        app.world_mut()
            .trigger(EditRequest::new(view, vec![EditCommand::Save]));
        app.update();
        app.world_mut().write_message(Observed(Ok(
            notify::Event::new(notify::EventKind::Any).add_path(path.clone())
        )));
        app.update();
        assert!(app.world().get::<FileLoadTask>(view).is_none());
        assert!(app.world().get::<Editor>(view).is_some());
        let receipts: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<WrittenAt>>()
            .iter(app.world())
            .collect();
        assert_eq!(receipts.len(), 1);
        app.world_mut().get_mut::<WrittenAt>(receipts[0]).unwrap().0 =
            Instant::now() - Duration::from_secs(3);
        std::fs::write(&path, "external\n").unwrap();
        app.world_mut().write_message(Observed(Ok(
            notify::Event::new(notify::EventKind::Any).add_path(path)
        )));
        FileLoadTask::settle(&mut app, view);
        assert_eq!(
            app.world().get::<Editor>(view).unwrap().core.buffer.text(),
            "external\n"
        );
    }
}
