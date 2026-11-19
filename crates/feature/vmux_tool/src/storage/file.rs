use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use std::path::PathBuf;

use super::{Busy, Failed, IoTask};

pub(super) struct FileReadPlugin;

impl Plugin for FileReadPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

#[derive(Component)]
pub(crate) struct FileReadRequest(pub(crate) PathBuf);

#[derive(Component)]
pub(crate) struct FileReadResult(pub(crate) Result<String, String>);

#[derive(Component)]
pub(super) struct FileBytes(pub(super) String);

fn read(
    requests: Query<
        (
            Entity,
            &FileReadRequest,
            Option<&FileBytes>,
            Option<&Failed<FileBytes>>,
        ),
        Without<Busy>,
    >,
    mut commands: Commands,
) {
    for (entity, request, bytes, error) in &requests {
        if let Some(bytes) = bytes {
            commands
                .entity(entity)
                .insert(FileReadResult(Ok(bytes.0.clone())))
                .remove::<(FileReadRequest, FileBytes)>();
        } else if let Some(error) = error {
            commands
                .entity(entity)
                .insert(FileReadResult(Err(error.0.clone())))
                .remove::<(FileReadRequest, Failed<FileBytes>)>();
        } else {
            let path = request.0.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(IoTaskPool::get().spawn(async move {
                    std::fs::read_to_string(path)
                        .map(FileBytes)
                        .map_err(|error| error.to_string())
                })),
            ));
        }
    }
}
