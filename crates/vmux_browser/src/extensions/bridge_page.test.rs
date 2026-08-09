use super::*;
use crate::extensions::bridge::ExtensionBridgeServer;
use crate::extensions::load::PreparedExtensions;
use crate::extensions::runtime::PreparedRuntime;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{PrivatePreloadScripts, WebviewMaxFrameRate, WebviewSize, WebviewSource};

const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn spawns_hidden_non_layout_bridge_webview() {
    let mut app = App::new();
    let runtime = PreparedRuntime {
        extension_id: EXTENSION_ID.into(),
        dir: std::path::PathBuf::from("runtime"),
        runtime_hash: "runtime-hash".into(),
        source_hash: "source-hash".into(),
        permissions: Vec::new(),
        optional_permissions: Vec::new(),
        host_permissions: Vec::new(),
        optional_host_permissions: Vec::new(),
        granted_permissions: Vec::new(),
        granted_host_permissions: Vec::new(),
    };
    let bridge = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
    let identity = bridge.identity(EXTENSION_ID).unwrap().clone();
    app.insert_resource(PreparedExtensions(vec![runtime]))
        .insert_resource(bridge)
        .init_resource::<ExtensionBridgeLifecycle>()
        .init_resource::<ExtensionInfrastructureEntities>()
        .add_message::<AppExit>()
        .add_systems(
            Update,
            (stop_extension_bridge_pages, spawn_extension_bridge_pages).chain(),
        );

    app.update();
    assert!(
        app.world_mut()
            .query::<&ExtensionBridgeWebview>()
            .iter(app.world())
            .next()
            .is_none()
    );

    app.world_mut().spawn(PrimaryWindow);
    app.update();
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
    assert_eq!(bridge.role, ExtensionBridgeRole::Transport);
    assert!(
        matches!(source, WebviewSource::Url(url) if url == &format!("chrome-extension://{EXTENSION_ID}/vmux_bridge.html"))
    );
    let [config] = preload.0.as_slice() else {
        panic!("expected one bridge preload script");
    };
    assert!(!config.contains("globalThis.__vmuxBridgeConfig"));
    assert!(config.contains(app.world().resource::<ExtensionBridgeServer>().endpoint()));
    assert!(config.contains(&identity.extension_id));
    assert!(config.contains(&identity.profile_id));
    assert!(config.contains(&identity.token));
    assert_eq!(size.0, Vec2::ONE);
    assert_eq!(frame_rate.0, 1);
    assert_eq!(*visibility, Visibility::Hidden);
    assert!(app.world().get::<vmux_layout::Browser>(entity).is_none());
    assert!(
        app.world()
            .resource::<ExtensionInfrastructureEntities>()
            .contains(entity)
    );
    assert_eq!(
        app.world_mut()
            .query::<&ExtensionBridgeWebview>()
            .iter(app.world())
            .count(),
        1
    );
    app.world_mut()
        .resource_mut::<Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    app.update();

    assert_eq!(
        app.world_mut()
            .query::<&ExtensionBridgeWebview>()
            .iter(app.world())
            .count(),
        0
    );
}
