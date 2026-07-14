use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{WebviewMaxFrameRate, WebviewSize, WebviewSource};

use super::bridge::ExtensionBridgeServer;
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
    for runtime in &prepared.0 {
        let identity = server
            .identity(&runtime.extension_id)
            .unwrap_or_else(|| panic!("missing bridge identity for {}", runtime.extension_id));
        let mut url = url::Url::parse(&format!(
            "chrome-extension://{}/vmux_bridge.html",
            runtime.extension_id
        ))
        .expect("valid extension bridge URL");
        url.query_pairs_mut()
            .append_pair("endpoint", server.endpoint())
            .append_pair("token", &identity.token)
            .append_pair("extension", &identity.extension_id)
            .append_pair("profile", &identity.profile_id);
        commands.spawn((
            ExtensionBridgeWebview {
                extension_id: runtime.extension_id.clone(),
            },
            WebviewSource::new(url.to_string()),
            WebviewSize(Vec2::ONE),
            WebviewMaxFrameRate(1),
            Visibility::Hidden,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::bridge::ExtensionBridgeServer;
    use crate::extensions::load::PreparedExtensions;
    use crate::extensions::runtime::PreparedRuntime;
    use bevy::window::PrimaryWindow;
    use bevy_cef::prelude::{WebviewMaxFrameRate, WebviewSize, WebviewSource};

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
        app.insert_resource(PreparedExtensions(vec![runtime]))
            .insert_resource(bridge)
            .add_systems(Update, spawn_extension_bridge_pages);
        app.world_mut().spawn(PrimaryWindow);

        app.update();

        let mut query = app.world_mut().query::<(
            Entity,
            &ExtensionBridgeWebview,
            &WebviewSource,
            &WebviewSize,
            &WebviewMaxFrameRate,
            &Visibility,
        )>();
        let (entity, bridge, source, size, frame_rate, visibility) =
            query.single(app.world()).unwrap();
        assert_eq!(bridge.extension_id, EXTENSION_ID);
        assert!(
            matches!(source, WebviewSource::Url(url) if url.starts_with(&format!("chrome-extension://{EXTENSION_ID}/vmux_bridge.html?")))
        );
        assert_eq!(size.0, Vec2::ONE);
        assert_eq!(frame_rate.0, 1);
        assert_eq!(*visibility, Visibility::Hidden);
        assert!(app.world().get::<vmux_layout::Browser>(entity).is_none());
    }
}
