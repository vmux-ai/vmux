use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use moonshine_save::prelude::*;
use vmux_command::open::OpenCommand;
use vmux_command::{AppCommand, BrowserCommand, ReadAppCommands};

pub use vmux_core::profile::{
    active_profile_name, cef_cache_path, profile_dir, session_path, shared_data_dir,
};

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Profile>()
            .add_systems(Update, handle_open_in_new_space.in_set(ReadAppCommands));
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::profile"]
#[require(Save)]
pub struct Profile {
    pub name: String,
    pub color: [f32; 4],
    pub icon: Option<String>,
}

impl Profile {
    pub fn default_profile() -> Self {
        Self {
            name: "default".to_string(),
            color: [0.4, 0.6, 1.0, 1.0],
            icon: None,
        }
    }
}

fn handle_open_in_new_space(
    mut reader: MessageReader<AppCommand>,
    profiles: Query<Entity, With<Profile>>,
    main_q: Query<Entity, With<crate::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<crate::NewStackContext>,
    mut spawn_requests: MessageWriter<crate::LayoutSpawnRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewSpace { url })) = cmd else {
            continue;
        };

        let Ok(main) = main_q.single() else { continue };

        let count = profiles.iter().count();
        let name = format!("Space {}", count + 1);
        let profile = Profile {
            name,
            color: [0.4, 0.6, 1.0, 1.0],
            icon: None,
        };

        let layout =
            crate::window::spawn_space_layout(main, *primary_window, profile, &mut commands);

        let startup = effective_startup_url.as_deref().map(|u| u.0.as_str());
        let resolved = vmux_command::open::handler::resolve_url(url.as_deref(), startup);

        if let Some(old_stack) = new_stack_ctx.stack.take() {
            commands.entity(old_stack).despawn();
        }
        new_stack_ctx.previous_stack = None;
        new_stack_ctx.dismiss_modal = false;

        spawn_requests.write(crate::LayoutSpawnRequest::OpenUrl {
            stack: layout.stack,
            url: resolved,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::EffectiveStartupUrl;
    use crate::window::Main as MainNode;
    use vmux_command::CommandPlugin;

    #[derive(Resource, Default)]
    struct CollectedSpawns(Vec<crate::LayoutSpawnRequest>);

    fn collect_spawn_requests(
        mut reader: MessageReader<crate::LayoutSpawnRequest>,
        mut collected: ResMut<CollectedSpawns>,
    ) {
        for req in reader.read() {
            collected.0.push(req.clone());
        }
    }

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin));
        app.add_message::<crate::LayoutSpawnRequest>();
        app.init_resource::<crate::NewStackContext>();
        app.init_resource::<CollectedSpawns>();
        app.add_systems(
            Update,
            (
                handle_open_in_new_space.in_set(ReadAppCommands),
                collect_spawn_requests.after(handle_open_in_new_space),
            ),
        );
        app
    }

    fn spawn_main_and_profile(app: &mut App) -> Entity {
        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let main = app.world_mut().spawn(MainNode).id();
        app.world_mut().spawn(Profile::default_profile());
        main
    }

    #[test]
    fn open_in_new_space_explicit_url_spawns_new_profile_with_url() {
        let mut app = build_app();
        spawn_main_and_profile(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewSpace {
                    url: Some("https://example.com".into()),
                },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        match &collected.0[0] {
            crate::LayoutSpawnRequest::OpenUrl { url, .. } => {
                assert_eq!(url, "https://example.com");
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }

        let profile_count = app
            .world_mut()
            .query::<&Profile>()
            .iter(app.world())
            .count();
        assert_eq!(profile_count, 2, "expected two profiles after InNewSpace");

        let names: Vec<String> = app
            .world_mut()
            .query::<&Profile>()
            .iter(app.world())
            .map(|p| p.name.clone())
            .collect();
        assert!(names.contains(&"default".to_string()));
        assert!(names.contains(&"Space 2".to_string()));
    }

    #[test]
    fn open_in_new_space_none_url_falls_back_to_startup() {
        let mut app = build_app();
        app.insert_resource(EffectiveStartupUrl("https://startup.test".into()));
        spawn_main_and_profile(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewSpace { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        match &collected.0[0] {
            crate::LayoutSpawnRequest::OpenUrl { url, .. } => {
                assert_eq!(url, "https://startup.test");
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }
    }

    #[test]
    fn open_in_new_space_none_url_no_startup_falls_back_to_default() {
        let mut app = build_app();
        spawn_main_and_profile(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewSpace { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        match &collected.0[0] {
            crate::LayoutSpawnRequest::OpenUrl { url, .. } => {
                assert_eq!(url, vmux_command::open::handler::DEFAULT_NEW_PAGE_URL);
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }
    }
}
