#[cfg(any(
    feature = "ecs",
    feature = "window",
    feature = "http-server",
    feature = "quic-server"
))]
use bevy_app::{PluginGroup, PluginGroupBuilder};

#[cfg(feature = "ecs")]
pub struct MinimalPlugins;

#[cfg(feature = "ecs")]
impl PluginGroup for MinimalPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(vmux_ecs::time::TimePlugin)
            .add(vmux_ecs::PrimitivesPlugin)
            .add(vmux_ecs::OperationPlugin)
            .add(vmux_ecs::ShutdownPlugin)
    }
}

#[cfg(feature = "window")]
pub struct DefaultPlugins;

#[cfg(feature = "window")]
impl PluginGroup for DefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::start::<Self>()
            .add(vmux_ecs::time::TimePlugin)
            .add(vmux_ecs::PrimitivesPlugin)
            .add(vmux_ecs::OperationPlugin)
            .add(vmux_ecs::ShutdownPlugin)
            .add(vmux_flex::FlexPlugin);
        #[cfg(feature = "layout")]
        let builder = builder.add(vmux_layout::LayoutPlugin);
        #[cfg(feature = "menu")]
        let builder = builder.add(vmux_menu::MenuPlugin);
        builder.add(vmux_window::WindowHostPlugin::default())
    }
}

#[cfg(any(feature = "http-server", feature = "quic-server"))]
pub struct ServerPlugins;

#[cfg(any(feature = "http-server", feature = "quic-server"))]
impl PluginGroup for ServerPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::start::<Self>()
            .add(vmux_ecs::time::TimePlugin)
            .add(vmux_ecs::PrimitivesPlugin)
            .add(vmux_ecs::OperationPlugin)
            .add(vmux_ecs::ShutdownPlugin);
        #[cfg(feature = "http-server")]
        let builder = builder.add(vmux_http::HttpServerPlugin);
        #[cfg(feature = "quic-server")]
        let builder = builder.add(vmux_quic::QuicServerPlugin);
        builder
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "ecs")]
    use bevy_app::App;
    #[cfg(any(feature = "ecs", feature = "window"))]
    use bevy_app::PluginGroup;
    #[cfg(feature = "window")]
    use vmux_window::{WindowHostConfig, WindowHostPlugin};

    #[cfg(feature = "window")]
    use super::DefaultPlugins;
    #[cfg(feature = "ecs")]
    use super::MinimalPlugins;
    #[cfg(any(feature = "http-server", feature = "quic-server"))]
    use super::ServerPlugins;

    #[cfg(feature = "ecs")]
    #[test]
    fn minimal_plugins_bootstrap_core_ecs_and_support_disable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.update();
        assert!(app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>());
        assert!(app.is_plugin_added::<vmux_ecs::time::TimePlugin>());
        assert!(app.is_plugin_added::<vmux_ecs::OperationPlugin>());
        assert!(app.is_plugin_added::<vmux_ecs::ShutdownPlugin>());

        let mut trimmed = App::new();
        trimmed.add_plugins(
            MinimalPlugins
                .build()
                .disable::<vmux_ecs::OperationPlugin>(),
        );
        trimmed.update();
        assert!(trimmed.is_plugin_added::<vmux_ecs::PrimitivesPlugin>());
        assert!(trimmed.is_plugin_added::<vmux_ecs::time::TimePlugin>());
        assert!(!trimmed.is_plugin_added::<vmux_ecs::OperationPlugin>());
    }

    #[cfg(feature = "window")]
    #[test]
    fn default_plugins_support_headless_window_override_and_disable() {
        let mut app = App::new();
        app.add_plugins(DefaultPlugins.set(WindowHostPlugin::headless("Bootstrap Test")));
        app.update();
        assert!(app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>());
        assert!(app.is_plugin_added::<vmux_flex::FlexPlugin>());
        assert!(app.is_plugin_added::<WindowHostPlugin>());
        assert_eq!(
            app.world().resource::<WindowHostConfig>().title,
            "Bootstrap Test"
        );
        assert!(app.world().resource::<WindowHostConfig>().headless);

        let mut no_window = App::new();
        no_window.add_plugins(DefaultPlugins.build().disable::<WindowHostPlugin>());
        no_window.update();
        assert!(no_window.is_plugin_added::<vmux_ecs::PrimitivesPlugin>());
        assert!(!no_window.is_plugin_added::<WindowHostPlugin>());
    }

    #[cfg(any(feature = "http-server", feature = "quic-server"))]
    #[test]
    fn server_plugins_bootstrap_network_servers_and_shutdown() {
        let mut app = App::new();
        app.add_plugins(ServerPlugins);
        app.update();
        assert!(app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>());
        assert!(app.is_plugin_added::<vmux_ecs::ShutdownPlugin>());
        #[cfg(feature = "http-server")]
        assert!(app.is_plugin_added::<vmux_http::HttpServerPlugin>());
        #[cfg(feature = "quic-server")]
        assert!(app.is_plugin_added::<vmux_quic::QuicServerPlugin>());
    }
}
