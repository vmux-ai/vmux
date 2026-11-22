use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_ecs::UiStateWrite;

use crate::state::ManagerUiState;

pub(crate) struct ManagerFixture {
    pub app: App,
    pub view: Entity,
    pub directory: tempfile::TempDir,
}

impl ManagerFixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ManagerFixturePlugin));
        let profile = vmux_ecs::profile::Profile::named("manager-fixture");
        app.world_mut().spawn((
            vmux_ecs::profile::ActiveProfile(profile.clone()),
            Name::new("Fixture profile"),
            vmux_ecs::profile::ProfileDirectories(vmux_ecs::profile::ProfilePaths::at(
                profile,
                directory.path().to_path_buf(),
            )),
        ));
        app.update();
        let view = app
            .world_mut()
            .spawn((
                super::catalog::SessionManagerView,
                CapturedProjection::default(),
            ))
            .id();
        app.world_mut().flush();
        Self {
            app,
            view,
            directory,
        }
    }

    pub fn session(&mut self, id: &str, name: &str, activated: i64) -> Entity {
        self.app
            .world_mut()
            .spawn((
                crate::Session,
                crate::SessionId(id.into()),
                Name::new(name.to_string()),
                vmux_ecs::Description::default(),
                vmux_ecs::Cwd(self.directory.path().to_path_buf()),
                crate::Stage(crate::StageId("in_progress".into())),
                vmux_ecs::CreatedAt(1),
                vmux_ecs::LastActivatedAt(activated),
                crate::StageChangedAt(activated),
                crate::RunState::Idle,
            ))
            .id()
    }

    pub fn provider(&mut self, id: &str, name: &str) -> Entity {
        let source = self
            .app
            .world_mut()
            .spawn((
                crate::provider::Source,
                crate::provider::Origin::Native,
                crate::provider::Capability::Supported,
                crate::provider::Generation(0),
                crate::AgentId("codex".into()),
                Name::new("Codex"),
            ))
            .id();
        self.app
            .world_mut()
            .spawn((
                crate::provider::Entry,
                ChildOf(source),
                crate::ProviderKind("acp:codex".into()),
                crate::ProviderSessionId(id.into()),
                crate::AgentId("codex".into()),
                Name::new(name.to_string()),
                vmux_ecs::Description::default(),
                vmux_ecs::Cwd(self.directory.path().to_path_buf()),
                vmux_ecs::CreatedAt(1),
                crate::SourceUpdatedAt(vmux_ecs::UnixMillis::now().0),
                crate::provider::ArchiveObservedAt(0),
                crate::provider::ConversationTitle::default(),
                crate::provider::LastPrompt::default(),
            ))
            .id()
    }

    pub fn input<T: vmux_api::BinEvent + Send + Sync>(&mut self, payload: T) {
        self.app.world_mut().trigger(UiInput {
            webview: self.view,
            payload,
        });
        self.app.world_mut().flush();
    }

    pub fn state(&self) -> ManagerUiState {
        self.app
            .world()
            .get::<CapturedProjection>(self.view)
            .unwrap()
            .0
            .clone()
    }

    pub fn update(&mut self) -> ManagerUiState {
        self.app.update();
        self.state()
    }
}

struct ManagerFixturePlugin;

impl Plugin for ManagerFixturePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(super::catalog::CatalogPlugin)
            .add_observer(capture);
    }
}

#[derive(Component, Default)]
struct CapturedProjection(ManagerUiState);

fn capture(event: On<UiStateWrite<ManagerUiState>>, mut views: Query<&mut CapturedProjection>) {
    if let Ok(mut view) = views.get_mut(event.webview()) {
        view.0 = event.update().clone();
    }
}
