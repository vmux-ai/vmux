use crate::Terminal;
use crate::launch::TerminalLaunch;
use bevy::prelude::*;
use vmux_command::CommandBarWorkDirectory;

pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_work_directories.in_set(vmux_command::WriteCommandBarSnapshots),
        );
    }
}

fn sync_work_directories(
    changed: Query<(Entity, &TerminalLaunch), (With<Terminal>, Changed<TerminalLaunch>)>,
    mut removed: RemovedComponents<TerminalLaunch>,
    mut commands: Commands,
) {
    for (entity, launch) in &changed {
        commands
            .entity(entity)
            .insert(CommandBarWorkDirectory(launch.cwd.clone()));
    }
    for entity in removed.read() {
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.remove::<CommandBarWorkDirectory>();
        }
    }
}
