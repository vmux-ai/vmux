use std::collections::HashSet;

use bevy_app::{App, AppExit, Plugin, Startup, Update};
use bevy_ecs::prelude::{
    ChildOf, Commands, Component, Entity, IntoScheduleConfigs, Message, MessageReader,
    MessageWriter, Query, Res, ResMut, Resource, With,
};
use bevy_ecs::schedule::ApplyDeferred;
use bevy_window::{
    CompositeAlphaMode, ExitCondition, MonitorSelection, PrimaryWindow, Window, WindowPlugin,
    WindowPosition, WindowResolution,
};
use vmux_ecs::{FilePath, ShutdownPlugin, ShutdownRequest};
use vmux_page::{HostedPage, Page, PageAppearance, PageHostPlugin, PageRoot, PageStylesheet};

pub use display::DisplayRelocationPlugin;

mod display;

#[derive(Resource, Clone)]
pub struct WindowHostConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    pub ime_enabled: bool,
    pub exit_on_last_window_close: bool,
    pub startup_page: Option<&'static Page>,
    pub appearance: PageAppearance,
    pub stylesheet: Option<String>,
    pub asset_directory: Option<FilePath>,
    pub headless: bool,
}

impl Default for WindowHostConfig {
    fn default() -> Self {
        Self {
            title: String::from("Vmux App"),
            width: 1280,
            height: 800,
            transparent: false,
            ime_enabled: true,
            exit_on_last_window_close: true,
            startup_page: None,
            appearance: PageAppearance::Dark,
            stylesheet: None,
            asset_directory: None,
            headless: false,
        }
    }
}

impl WindowHostConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn exit_on_last_window_close(mut self, exit: bool) -> Self {
        self.exit_on_last_window_close = exit;
        self
    }

    pub fn page(mut self, page: &'static Page) -> Self {
        self.startup_page = Some(page);
        self
    }

    pub fn appearance(mut self, appearance: PageAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn stylesheet(mut self, css: impl Into<String>) -> Self {
        self.stylesheet = Some(css.into());
        self
    }

    pub fn asset_directory(mut self, directory: impl Into<FilePath>) -> Self {
        self.asset_directory = Some(directory.into());
        self
    }

    pub fn headless(mut self) -> Self {
        self.headless = true;
        self
    }

    pub fn primary_window(&self) -> Window {
        self.window_with_title(&self.title, false)
    }

    pub fn window_with_title(&self, title: &str, secondary: bool) -> Window {
        Window {
            title: title.to_string(),
            transparent: self.transparent,
            composite_alpha_mode: if self.transparent {
                CompositeAlphaMode::PostMultiplied
            } else {
                CompositeAlphaMode::Auto
            },
            decorations: true,
            resizable: true,
            ime_enabled: self.ime_enabled,
            position: if secondary {
                WindowPosition::Automatic
            } else {
                WindowPosition::Centered(MonitorSelection::Primary)
            },
            resolution: WindowResolution::new(self.width, self.height),
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub struct WindowHostPlugin {
    config: WindowHostConfig,
}

impl WindowHostPlugin {
    pub fn new(config: WindowHostConfig) -> Self {
        Self { config }
    }

    pub fn headless(title: impl Into<String>) -> Self {
        Self {
            config: WindowHostConfig::new(title).headless(),
        }
    }

    fn window_plugin(&self) -> WindowPlugin {
        WindowPlugin {
            primary_window: Some(self.config.primary_window()),
            close_when_requested: true,
            exit_condition: if self.config.exit_on_last_window_close {
                ExitCondition::OnAllClosed
            } else {
                ExitCondition::DontExit
            },
            ..Default::default()
        }
    }
}

impl Plugin for WindowHostPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ShutdownPlugin>() {
            app.add_plugins(ShutdownPlugin);
        }
        if !app.is_plugin_added::<WindowPlugin>() {
            app.add_plugins(self.window_plugin());
        }
        #[cfg(all(feature = "native", not(target_os = "ios")))]
        if !self.config.headless {
            app.insert_resource(bevy_winit::WinitSettings::desktop_app());
            if !app.is_plugin_added::<bevy_input::InputPlugin>() {
                app.add_plugins(bevy_input::InputPlugin);
            }
            if !app.is_plugin_added::<bevy_winit::WinitPlugin>() {
                app.add_plugins(bevy_winit::WinitPlugin::default());
            }
            app.add_plugins(vmux_runner::WinitWakePlugin);
        }
        let page_host = if self.config.headless {
            PageHostPlugin::headless()
        } else {
            PageHostPlugin::default()
        };
        app.insert_resource(self.config.clone())
            .init_resource::<ClosedWindowsThisFrame>()
            .add_plugins((page_host, DisplayRelocationPlugin))
            .add_message::<OpenWindowRequest>()
            .add_message::<CloseWindowRequest>()
            .add_message::<HideAllWindowsRequest>()
            .add_systems(Startup, attach_startup_page)
            .add_systems(
                Update,
                (open, close, ApplyDeferred, check_exit, hide_all).chain(),
            );
        #[cfg(feature = "menu")]
        if !app.is_plugin_added::<vmux_menu::MenuPlugin>() {
            app.add_plugins(vmux_menu::MenuPlugin);
        }
        #[cfg(feature = "menu-native")]
        if !self.config.headless && !app.is_plugin_added::<vmux_menu::NativeMenuPlugin>() {
            app.add_plugins(vmux_menu::NativeMenuPlugin);
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostWindow;

#[derive(Message, Clone)]
pub struct OpenWindowRequest {
    pub title: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub page: Option<&'static Page>,
}

impl OpenWindowRequest {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            width: None,
            height: None,
            page: None,
        }
    }

    pub fn with_page(mut self, page: &'static Page) -> Self {
        self.page = Some(page);
        self
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct CloseWindowRequest(pub Entity);

#[derive(Message, Clone, Copy, Debug)]
pub struct HideAllWindowsRequest;

fn attach_startup_page(
    config: Res<WindowHostConfig>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok(window) = primary.single() else {
        return;
    };
    commands.entity(window).insert(HostWindow);
    let Some(page) = config.startup_page else {
        return;
    };
    let mut page_entity = commands.spawn((
        HostedPage::new(page),
        PageRoot,
        config.appearance,
        ChildOf(window),
    ));
    if let Some(css) = &config.stylesheet {
        page_entity.insert(PageStylesheet(css.clone()));
    }
    if let Some(directory) = &config.asset_directory {
        page_entity.insert(directory.clone());
    }
}

#[derive(Resource, Default)]
struct ClosedWindowsThisFrame(bool);

fn open(
    mut requests: MessageReader<OpenWindowRequest>,
    config: Res<WindowHostConfig>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let mut window = config.window_with_title(&request.title, true);
        if request.width.is_some() || request.height.is_some() {
            let width = request.width.unwrap_or(config.width);
            let height = request.height.unwrap_or(config.height);
            window.resolution = WindowResolution::new(width, height);
        }
        let entity = commands.spawn((window, HostWindow)).id();
        if let Some(page) = request.page {
            let mut page_entity = commands.spawn((
                HostedPage::new(page),
                PageRoot,
                config.appearance,
                ChildOf(entity),
            ));
            if let Some(css) = &config.stylesheet {
                page_entity.insert(PageStylesheet(css.clone()));
            }
            if let Some(directory) = &config.asset_directory {
                page_entity.insert(directory.clone());
            }
        }
    }
}

fn close(
    mut requests: MessageReader<CloseWindowRequest>,
    windows: Query<Entity, With<Window>>,
    mut closed_this_frame: ResMut<ClosedWindowsThisFrame>,
    mut commands: Commands,
) {
    let mut targets = HashSet::new();
    for request in requests.read() {
        if windows.contains(request.0) && targets.insert(request.0) {
            commands.entity(request.0).despawn();
        }
    }
    if !targets.is_empty() {
        closed_this_frame.0 = true;
    }
}

fn check_exit(
    mut closed_this_frame: ResMut<ClosedWindowsThisFrame>,
    windows: Query<Entity, With<Window>>,
    config: Res<WindowHostConfig>,
    mut shutdown: MessageWriter<ShutdownRequest>,
    mut exits: MessageWriter<AppExit>,
) {
    if !closed_this_frame.0 {
        return;
    }
    closed_this_frame.0 = false;
    if config.exit_on_last_window_close && windows.is_empty() {
        shutdown.write(ShutdownRequest {
            exit: AppExit::Success,
        });
        exits.write(AppExit::Success);
    }
}

fn hide_all(mut requests: MessageReader<HideAllWindowsRequest>, mut windows: Query<&mut Window>) {
    if requests.read().next().is_none() {
        return;
    }
    for mut window in &mut windows {
        window.visible = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::message::Messages;
    use dioxus::prelude::*;
    use vmux_ecs::UiIpcReady;
    use vmux_page::PageBounds;

    #[component]
    fn DemoView() -> Element {
        rsx! { div { "demo" } }
    }

    static DEMO_PAGE: Page = Page::pane("app://demo/", DemoView).titled("Demo");

    #[test]
    fn startup_page_mounts_as_child_of_primary_window_with_bounds() {
        let mut app = App::new();
        app.add_plugins(WindowHostPlugin::new(
            WindowHostConfig::new("Demo App")
                .size(1024, 720)
                .page(&DEMO_PAGE)
                .headless(),
        ));
        app.update();

        let world = app.world_mut();
        let (page, bounds, parent) = world
            .query_filtered::<(Entity, &PageBounds, &ChildOf), With<UiIpcReady>>()
            .single(world)
            .unwrap();
        assert_eq!(bounds.width, 1024.0);
        assert_eq!(bounds.height, 720.0);
        assert!(world.get::<PrimaryWindow>(parent.parent()).is_some());
        assert!(world.get::<HostWindow>(parent.parent()).is_some());
        assert!(world.get::<HostedPage>(page).is_some());
    }

    #[test]
    fn open_close_and_hide_window_requests_manage_windows() {
        let mut app = App::new();
        app.add_plugins(WindowHostPlugin::headless("Main"));
        app.update();

        app.world_mut()
            .write_message(OpenWindowRequest::new("Secondary").with_page(&DEMO_PAGE));
        app.update();
        app.update();

        let world = app.world_mut();
        assert_eq!(world.query::<&Window>().iter(world).count(), 2);
        assert_eq!(world.query::<&HostedPage>().iter(world).count(), 1);

        app.world_mut().write_message(HideAllWindowsRequest);
        app.update();
        let world = app.world_mut();
        assert!(world.query::<&Window>().iter(world).all(|w| !w.visible));

        let primary = world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(world)
            .unwrap();
        app.world_mut().write_message(CloseWindowRequest(primary));
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Window>().iter(world).count(), 1);
    }

    #[test]
    fn open_applies_optional_dimensions_independently() {
        let mut app = App::new();
        app.add_plugins(WindowHostPlugin::new(
            WindowHostConfig::new("Main").size(1280, 800).headless(),
        ));
        app.update();

        app.world_mut().write_message(OpenWindowRequest {
            title: "Narrow".into(),
            width: Some(640),
            height: None,
            page: None,
        });
        app.world_mut().write_message(OpenWindowRequest {
            title: "Tall".into(),
            width: None,
            height: Some(900),
            page: None,
        });
        app.update();

        let world = app.world_mut();
        let mut sizes: Vec<(String, u32, u32)> = world
            .query::<&Window>()
            .iter(world)
            .map(|w| {
                (
                    w.title.clone(),
                    w.resolution.physical_width(),
                    w.resolution.physical_height(),
                )
            })
            .collect();
        sizes.sort();
        assert!(sizes.contains(&("Narrow".into(), 640, 800)));
        assert!(sizes.contains(&("Tall".into(), 1280, 900)));
    }

    #[test]
    fn duplicate_close_and_simultaneous_open_do_not_exit_prematurely() {
        let mut app = App::new();
        app.add_plugins(WindowHostPlugin::headless("First"));
        app.update();

        let first = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .write_message(OpenWindowRequest::new("Second"));
        app.world_mut().write_message(CloseWindowRequest(first));
        app.update();

        assert_eq!(
            app.world_mut().query::<&Window>().iter(app.world()).count(),
            1
        );
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());

        app.world_mut()
            .write_message(OpenWindowRequest::new("Third"));
        app.update();
        let remaining: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<Window>>()
            .iter(app.world())
            .collect();
        assert_eq!(remaining.len(), 2);

        app.world_mut()
            .write_message(CloseWindowRequest(remaining[0]));
        app.world_mut()
            .write_message(CloseWindowRequest(remaining[0]));
        app.update();

        assert_eq!(
            app.world_mut().query::<&Window>().iter(app.world()).count(),
            1
        );
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());

        app.world_mut()
            .write_message(CloseWindowRequest(remaining[1]));
        app.update();
        assert_eq!(
            app.world_mut().query::<&Window>().iter(app.world()).count(),
            0
        );
        assert!(!app.world().resource::<Messages<AppExit>>().is_empty());
    }

    #[cfg(all(feature = "native", not(target_os = "ios")))]
    #[test]
    fn window_host_never_uses_continuous_update_mode() {
        let settings = bevy_winit::WinitSettings::desktop_app();
        assert!(!matches!(
            settings.focused_mode,
            bevy_winit::UpdateMode::Continuous
        ));
        assert!(!matches!(
            settings.unfocused_mode,
            bevy_winit::UpdateMode::Continuous
        ));
    }
}
