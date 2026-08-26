use bevy::prelude::*;

pub struct SpaceProjectPlugin;

impl Plugin for SpaceProjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            remember_space_project.before(vmux_layout::settings::EffectiveStartupDirSet),
        );
    }
}

fn remember_space_project(
    bound: Query<
        (Entity, &vmux_layout::tab::TabWorkspace),
        Changed<vmux_layout::tab::TabWorkspace>,
    >,
    child_of: Query<&ChildOf>,
    spaces: Query<(), With<vmux_layout::space::Space>>,
    ids: Query<&vmux_layout::space::SpaceId>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    mut saves: MessageWriter<vmux_setting::SettingsSaveRequest>,
) {
    if bound.is_empty() {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    for (tab, workspace) in &bound {
        let dir = workspace.project_dir.trim();
        if dir.is_empty() {
            continue;
        }
        let Some(space_id) = vmux_layout::space::space_id_of(tab, &child_of, &spaces, &ids) else {
            continue;
        };
        if settings
            .bypass_change_detection()
            .remember_space_startup_dir(&space_id, dir)
        {
            settings.set_changed();
            saves.write(vmux_setting::SettingsSaveRequest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: App,
        tab: Entity,
    }

    impl Fixture {
        fn start(space_id: &str) -> Self {
            let mut app = App::new();
            app.add_plugins(SpaceProjectPlugin)
                .add_message::<vmux_setting::SettingsSaveRequest>()
                .insert_resource(vmux_setting::AppSettings::embedded());
            let space = app
                .world_mut()
                .spawn((
                    vmux_layout::space::Space,
                    vmux_layout::space::SpaceId(space_id.to_string()),
                ))
                .id();
            let tab = app.world_mut().spawn(ChildOf(space)).id();
            Self { app, tab }
        }

        fn select(&mut self, project_dir: &str) {
            self.app
                .world_mut()
                .entity_mut(self.tab)
                .insert(vmux_layout::tab::TabWorkspace {
                    project_dir: project_dir.to_string(),
                });
            self.app.update();
        }

        fn remembered(&self, space_id: &str) -> Option<String> {
            self.app
                .world()
                .resource::<vmux_setting::AppSettings>()
                .spaces
                .get(space_id)?
                .startup_dir
                .clone()
        }

        fn drain_saves(&mut self) -> usize {
            self.app
                .world_mut()
                .resource_mut::<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>()
                .drain()
                .count()
        }
    }

    #[test]
    fn selecting_a_project_records_it_against_the_space() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        assert_eq!(fixture.remembered("work").as_deref(), Some("/tmp/alpha"));
        assert_eq!(fixture.drain_saves(), 1);
    }

    #[test]
    fn reselecting_the_same_project_does_not_ask_for_another_save() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        assert_eq!(fixture.drain_saves(), 1);
        fixture.select("/tmp/alpha");
        assert_eq!(fixture.drain_saves(), 0);
    }

    #[test]
    fn switching_project_replaces_what_the_space_remembers() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        fixture.select("/tmp/beta");
        assert_eq!(fixture.remembered("work").as_deref(), Some("/tmp/beta"));
    }
}
