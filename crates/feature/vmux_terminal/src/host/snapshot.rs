use crate::Terminal;
use crate::launch::TerminalLaunch;
use bevy::prelude::*;
use vmux_command::snapshot::{CommandBarProjection, CommandBarWorkDirectory};

pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (sync_work_directories, project)
                .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
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

fn project(mut state: Single<&mut CommandBarProjection>) {
    if !state.terminals.terminal_page_url.is_empty() {
        return;
    }
    state.terminals.terminal_page_url = crate::TerminalPlugin::URL.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(app: &App) -> &CommandBarProjection {
        app.world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarProjection>())
            .unwrap()
    }

    #[test]
    fn writes_url_and_no_running_terminals() {
        let mut app = App::new();
        app.add_systems(Update, project);
        app.world_mut().spawn(CommandBarProjection::default());
        app.update();
        let snap = &projection(&app).terminals;
        assert_eq!(snap.terminal_page_url, crate::TerminalPlugin::URL);
    }
}
