#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod chrome;
#[cfg(not(target_arch = "wasm32"))]
mod focus_ring;
#[cfg(not(target_arch = "wasm32"))]
mod footer;
#[cfg(not(target_arch = "wasm32"))]
pub mod glass;
#[cfg(not(target_arch = "wasm32"))]
mod header;
#[cfg(not(target_arch = "wasm32"))]
pub mod profile;
#[cfg(not(target_arch = "wasm32"))]
pub mod scene;
#[cfg(not(target_arch = "wasm32"))]
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod tab;
#[cfg(not(target_arch = "wasm32"))]
pub mod unit;
#[cfg(not(target_arch = "wasm32"))]
mod webview_reveal;

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub mod drag;
#[cfg(not(target_arch = "wasm32"))]
pub mod pane;
#[cfg(not(target_arch = "wasm32"))]
pub mod side_sheet;
#[cfg(not(target_arch = "wasm32"))]
pub mod space;
#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub mod swap;
#[cfg(not(target_arch = "wasm32"))]
pub mod window;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
pub use chrome::{
    Browser, LayoutChrome, LayoutChromePlugin, Loading, NavigationState,
    apply_chrome_state_from_cef,
};
#[cfg(not(target_arch = "wasm32"))]
use focus_ring::FocusRingPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use footer::Footer;
#[cfg(not(target_arch = "wasm32"))]
use footer::FooterLayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
use glass::GlassMaterialPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use header::Header;
#[cfg(not(target_arch = "wasm32"))]
use header::HeaderLayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
use moonshine_save::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use pane::PanePlugin;
#[cfg(not(target_arch = "wasm32"))]
use side_sheet::SideSheetLayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
use space::SpacePlugin;
#[cfg(not(target_arch = "wasm32"))]
use tab::TabPlugin;
#[cfg(not(target_arch = "wasm32"))]
use vmux_webview_app::JsEmitUiReadyPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use webview_reveal::PendingWebviewReveal;
#[cfg(not(target_arch = "wasm32"))]
use webview_reveal::WebviewRevealPlugin;
#[cfg(not(target_arch = "wasm32"))]
use window::WindowPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use window::fit_window_to_screen;

#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutStartupSet {
    Window,
    Persistence,
    DefaultSession,
    Post,
}

#[cfg(not(target_arch = "wasm32"))]
/// Marker component indicating a panel (header, side-sheet) is open.
/// Added/removed at runtime; persisted on state entities.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout"]
pub struct Open;

#[cfg(not(target_arch = "wasm32"))]
/// Persisted entity that mirrors header open state.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout"]
#[require(Save)]
pub struct HeaderState;

#[cfg(not(target_arch = "wasm32"))]
/// Persisted entity that mirrors side-sheet open state.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout"]
#[require(Save)]
pub struct SideSheetState;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct NewTabContext {
    pub tab: Option<Entity>,
    pub previous_tab: Option<Entity>,
    pub needs_open: bool,
    pub dismiss_modal: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct CloseRequiresConfirmation;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct SessionFilePresent(pub bool);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message)]
pub enum LayoutSpawnRequest {
    Terminal { tab: Entity },
    ProcessesMonitor { tab: Entity },
}

#[cfg(not(target_arch = "wasm32"))]
pub struct LayoutPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Open>()
            .register_type::<HeaderState>()
            .register_type::<SideSheetState>();
        app.init_resource::<NewTabContext>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .add_message::<LayoutSpawnRequest>()
            .configure_sets(
                Startup,
                (
                    LayoutStartupSet::Window,
                    LayoutStartupSet::Persistence,
                    LayoutStartupSet::DefaultSession,
                    LayoutStartupSet::Post,
                )
                    .chain(),
            );
        app.add_plugins((
            JsEmitUiReadyPlugin,
            WindowPlugin,
            SpacePlugin,
            PanePlugin,
            TabPlugin,
            FocusRingPlugin,
            GlassMaterialPlugin,
            SideSheetLayoutPlugin,
            HeaderLayoutPlugin,
            FooterLayoutPlugin,
            WebviewRevealPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::entity::EntityHashMap;
    use bevy::reflect::TypePath;
    use bevy::scene::serde::SceneDeserializer;
    use serde::de::DeserializeSeed;

    use super::*;

    #[test]
    fn persisted_type_paths_match_legacy_desktop_sessions() {
        assert_eq!(
            profile::Profile::type_path(),
            "vmux_desktop::profile::Profile"
        );
        assert_eq!(Open::type_path(), "vmux_desktop::layout::Open");
        assert_eq!(
            HeaderState::type_path(),
            "vmux_desktop::layout::HeaderState"
        );
        assert_eq!(
            SideSheetState::type_path(),
            "vmux_desktop::layout::SideSheetState"
        );
        assert_eq!(pane::Pane::type_path(), "vmux_desktop::layout::pane::Pane");
        assert_eq!(
            pane::PaneSplit::type_path(),
            "vmux_desktop::layout::pane::PaneSplit"
        );
        assert_eq!(
            pane::PaneSplitDirection::type_path(),
            "vmux_desktop::layout::pane::PaneSplitDirection"
        );
        assert_eq!(
            pane::PaneSize::type_path(),
            "vmux_desktop::layout::pane::PaneSize"
        );
        assert_eq!(
            space::Space::type_path(),
            "vmux_desktop::layout::space::Space"
        );
        assert_eq!(tab::Tab::type_path(), "vmux_desktop::layout::tab::Tab");
    }

    #[test]
    fn legacy_desktop_session_component_names_deserialize() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .register_type::<profile::Profile>()
            .register_type::<Open>()
            .register_type::<HeaderState>()
            .register_type::<SideSheetState>()
            .register_type::<pane::Pane>()
            .register_type::<pane::PaneSplit>()
            .register_type::<pane::PaneSplitDirection>()
            .register_type::<pane::PaneSize>()
            .register_type::<space::Space>()
            .register_type::<tab::Tab>();

        let data = r#"(
            resources: {},
            entities: {
                4294967293: (
                    components: {
                        "vmux_desktop::profile::Profile": (
                            name: "default",
                            color: (0.4, 0.6, 1.0, 1.0),
                            icon: None,
                        ),
                        "vmux_desktop::layout::HeaderState": (),
                        "vmux_desktop::layout::SideSheetState": (),
                        "vmux_desktop::layout::Open": (),
                        "vmux_desktop::layout::pane::Pane": (),
                        "vmux_desktop::layout::pane::PaneSize": (
                            flex_grow: 1.0,
                        ),
                        "vmux_desktop::layout::pane::PaneSplit": (
                            direction: Row,
                        ),
                        "vmux_desktop::layout::space::Space": (
                            name: "Space 1",
                        ),
                        "vmux_desktop::layout::tab::Tab": (
                            scroll_x: 0.0,
                            scroll_y: 0.0,
                        ),
                    },
                ),
            },
        )"#;

        let mut deserializer = ron::Deserializer::from_str(data).unwrap();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let scene = SceneDeserializer {
            type_registry: &registry,
        }
        .deserialize(&mut deserializer)
        .unwrap();
        drop(registry);

        scene
            .write_to_world(app.world_mut(), &mut EntityHashMap::default())
            .unwrap();

        let world = app.world_mut();
        assert_eq!(world.query::<&profile::Profile>().iter(world).count(), 1);
        assert_eq!(
            world
                .query_filtered::<(), With<HeaderState>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<(), With<SideSheetState>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<(), With<pane::Pane>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(world.query::<&pane::PaneSplit>().iter(world).count(), 1);
        assert_eq!(world.query::<&pane::PaneSize>().iter(world).count(), 1);
        assert_eq!(world.query::<&space::Space>().iter(world).count(), 1);
        assert_eq!(world.query::<&tab::Tab>().iter(world).count(), 1);
    }
}
