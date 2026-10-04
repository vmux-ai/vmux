use crate::Terminal;
use crate::launch::TerminalLaunch;
use bevy::prelude::*;
use vmux_command::{CommandBarTerminalPage, CommandBarWorkDirectory};

pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            sync_work_directories.in_set(vmux_command::WriteCommandBarSnapshots),
        );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Terminal command bar page"),
        CommandBarTerminalPage(crate::TerminalPlugin::URL.to_string()),
    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_page(app: &App) -> &CommandBarTerminalPage {
        app.world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarTerminalPage>())
            .unwrap()
    }

    #[test]
    fn contributes_terminal_page() {
        let mut app = App::new();
        app.add_systems(Startup, spawn);
        app.world_mut().run_schedule(Startup);
        assert_eq!(terminal_page(&app).0, crate::TerminalPlugin::URL);
    }
}
