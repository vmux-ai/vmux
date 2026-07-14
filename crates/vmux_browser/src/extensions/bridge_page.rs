use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{PrivatePreloadScripts, WebviewMaxFrameRate, WebviewSize, WebviewSource};

use super::bridge::{BridgeIdentity, ExtensionBridgeServer};
use super::load::PreparedExtensions;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBridgeWebview {
    pub extension_id: String,
}

pub fn spawn_extension_bridge_pages(
    mut commands: Commands,
    prepared: Res<PreparedExtensions>,
    server: Res<ExtensionBridgeServer>,
    primary_window: Query<(), With<PrimaryWindow>>,
) {
    if primary_window.is_empty() {
        return;
    }
    let conformance = super::broker::extension_conformance_enabled();
    for runtime in &prepared.0 {
        let identity = server
            .identity(&runtime.extension_id)
            .unwrap_or_else(|| panic!("missing bridge identity for {}", runtime.extension_id));
        let url = format!(
            "chrome-extension://{}/vmux_bridge.html",
            runtime.extension_id
        );
        commands.spawn((
            ExtensionBridgeWebview {
                extension_id: runtime.extension_id.clone(),
            },
            WebviewSource::new(url),
            PrivatePreloadScripts::from([bridge_config_source(&server, identity, conformance)]),
            WebviewSize(Vec2::ONE),
            WebviewMaxFrameRate(1),
            Visibility::Hidden,
        ));
        if conformance {
            commands.spawn((
                ExtensionBridgeWebview {
                    extension_id: runtime.extension_id.clone(),
                },
                WebviewSource::new(format!(
                    "chrome-extension://{}/echo.html",
                    runtime.extension_id
                )),
                WebviewSize(Vec2::ONE),
                WebviewMaxFrameRate(1),
                Visibility::Hidden,
            ));
        }
    }
}

fn bridge_config_source(
    server: &ExtensionBridgeServer,
    identity: &BridgeIdentity,
    conformance: bool,
) -> String {
    let config = serde_json::json!({
        "endpoint": server.endpoint(),
        "extension": identity.extension_id,
        "profile": identity.profile_id,
        "token": identity.token,
        "conformance": conformance,
    });
    format!("globalThis.__vmuxBridgeConfig = {config};\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::bridge::ExtensionBridgeServer;
    use crate::extensions::load::PreparedExtensions;
    use crate::extensions::runtime::PreparedRuntime;
    use bevy::window::PrimaryWindow;
    use bevy_cef::prelude::{
        PrivatePreloadScripts, WebviewMaxFrameRate, WebviewSize, WebviewSource,
    };

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn spawns_hidden_non_layout_bridge_webview() {
        let mut app = App::new();
        let runtime = PreparedRuntime {
            extension_id: EXTENSION_ID.into(),
            dir: std::path::PathBuf::from("runtime"),
            runtime_hash: "runtime-hash".into(),
            source_hash: "source-hash".into(),
        };
        let bridge = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
        let identity = bridge.identity(EXTENSION_ID).unwrap().clone();
        app.insert_resource(PreparedExtensions(vec![runtime]))
            .insert_resource(bridge)
            .add_systems(Update, spawn_extension_bridge_pages);
        app.world_mut().spawn(PrimaryWindow);

        app.update();

        let mut query = app.world_mut().query::<(
            Entity,
            &ExtensionBridgeWebview,
            &WebviewSource,
            &PrivatePreloadScripts,
            &WebviewSize,
            &WebviewMaxFrameRate,
            &Visibility,
        )>();
        let (entity, bridge, source, preload, size, frame_rate, visibility) =
            query.single(app.world()).unwrap();
        assert_eq!(bridge.extension_id, EXTENSION_ID);
        assert!(
            matches!(source, WebviewSource::Url(url) if url == &format!("chrome-extension://{EXTENSION_ID}/vmux_bridge.html"))
        );
        let [config] = preload.0.as_slice() else {
            panic!("expected one bridge preload script");
        };
        let config = config
            .strip_prefix("globalThis.__vmuxBridgeConfig = ")
            .and_then(|source| source.strip_suffix(";\n"))
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(
            config["endpoint"],
            app.world().resource::<ExtensionBridgeServer>().endpoint()
        );
        assert_eq!(config["extension"], identity.extension_id);
        assert_eq!(config["profile"], identity.profile_id);
        assert_eq!(config["token"], identity.token);
        assert_eq!(config["conformance"], false);
        assert_eq!(size.0, Vec2::ONE);
        assert_eq!(frame_rate.0, 1);
        assert_eq!(*visibility, Visibility::Hidden);
        assert!(app.world().get::<vmux_layout::Browser>(entity).is_none());
    }
}
