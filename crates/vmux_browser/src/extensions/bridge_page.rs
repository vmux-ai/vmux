use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{
    CefShutdownState, PrivatePreloadScripts, WebviewMaxFrameRate, WebviewSize, WebviewSource,
};
use std::collections::HashSet;
use vmux_flex::prelude::*;

use super::bridge::{BridgeIdentity, ExtensionBridgeServer};
use super::load::PreparedExtensions;

pub(crate) struct ExtensionBridgePagePlugin;

impl Plugin for ExtensionBridgePagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (stop_extension_bridge_pages, spawn_extension_bridge_pages)
                .chain()
                .before(bevy_cef::prelude::CefSystems::CreateAndResize),
        );
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBridgeWebview {
    pub extension_id: String,
    pub role: ExtensionBridgeRole,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ExtensionBridgeRole {
    Transport,
    ConformanceEcho,
}

#[derive(Component)]
pub(crate) struct ExtensionInfrastructureWebview;

#[derive(Component)]
pub(crate) struct RetiredExtensionInfrastructureWebview(Entity);

impl RetiredExtensionInfrastructureWebview {
    pub(crate) fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.0 == entity
    }
}

#[derive(Component)]
struct ExtensionBridgeStopping;

fn stop_extension_bridge_pages(
    mut exits: MessageReader<AppExit>,
    pages: Query<Entity, With<ExtensionBridgeWebview>>,
    stopping: Query<(), With<ExtensionBridgeStopping>>,
    mut commands: Commands,
) {
    if exits.read().count() == 0 {
        return;
    }
    if stopping.is_empty() {
        commands.spawn(ExtensionBridgeStopping);
    }
    for entity in &pages {
        commands.spawn(RetiredExtensionInfrastructureWebview::new(entity));
        commands.entity(entity).despawn();
    }
}

fn spawn_extension_bridge_pages(
    mut commands: Commands,
    prepared: Res<PreparedExtensions>,
    server: Res<ExtensionBridgeServer>,
    primary_window: Query<(), With<PrimaryWindow>>,
    added_primary_window: Query<(), Added<PrimaryWindow>>,
    pages: Query<(Entity, &ExtensionBridgeWebview)>,
    mut removed_pages: RemovedComponents<ExtensionBridgeWebview>,
    shutdown: Option<Res<CefShutdownState>>,
    stopping: Query<(), With<ExtensionBridgeStopping>>,
    mut initialized: Local<bool>,
) {
    let should_reconcile = !*initialized
        || prepared.is_changed()
        || !added_primary_window.is_empty()
        || removed_pages.read().count() > 0
        || shutdown.as_ref().is_some_and(|state| state.is_changed());
    *initialized = true;
    if !should_reconcile {
        return;
    }
    if !stopping.is_empty()
        || shutdown.is_some_and(|state| state.started())
        || primary_window.is_empty()
    {
        return;
    }
    let conformance = super::broker::extension_conformance_enabled();
    let mut desired = prepared
        .0
        .iter()
        .flat_map(|runtime| {
            let mut roles = vec![(runtime.extension_id.clone(), ExtensionBridgeRole::Transport)];
            if conformance {
                roles.push((
                    runtime.extension_id.clone(),
                    ExtensionBridgeRole::ConformanceEcho,
                ));
            }
            roles
        })
        .collect::<HashSet<_>>();
    for (entity, page) in &pages {
        let key = (page.extension_id.clone(), page.role);
        if desired.remove(&key) {
            continue;
        }
        commands.spawn(RetiredExtensionInfrastructureWebview::new(entity));
        commands.entity(entity).despawn();
    }
    for runtime in &prepared.0 {
        let identity = server
            .identity(&runtime.extension_id)
            .unwrap_or_else(|| panic!("missing bridge identity for {}", runtime.extension_id));
        for role in [
            ExtensionBridgeRole::Transport,
            ExtensionBridgeRole::ConformanceEcho,
        ] {
            if role == ExtensionBridgeRole::ConformanceEcho && !conformance {
                continue;
            }
            let key = (runtime.extension_id.clone(), role);
            if !desired.remove(&key) {
                continue;
            }
            let mut entity = commands.spawn((
                ExtensionBridgeWebview {
                    extension_id: runtime.extension_id.clone(),
                    role,
                },
                ExtensionInfrastructureWebview,
                WebviewSize(Vec2::ONE),
                WebviewMaxFrameRate(1),
                Visibility::Hidden,
            ));
            match role {
                ExtensionBridgeRole::Transport => {
                    entity.insert((
                        WebviewSource::new(format!(
                            "chrome-extension://{}/vmux_bridge.html",
                            runtime.extension_id
                        )),
                        PrivatePreloadScripts::from([bridge_config_source(
                            &server,
                            identity,
                            conformance,
                        )]),
                    ));
                }
                ExtensionBridgeRole::ConformanceEcho => {
                    entity.insert(WebviewSource::new(format!(
                        "chrome-extension://{}/echo.html",
                        runtime.extension_id
                    )));
                }
            }
        }
    }
}

fn bridge_config_source(
    server: &ExtensionBridgeServer,
    identity: &BridgeIdentity,
    conformance: bool,
) -> String {
    super::runtime::bridge_source(&super::runtime::BridgeConfig {
        endpoint: server.endpoint(),
        extension: &identity.extension_id,
        profile: &identity.profile_id,
        token: &identity.token,
        conformance,
    })
    .expect("valid embedded extension bridge template")
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
            matches!(source, WebviewSource(url) if url == &format!("chrome-extension://{EXTENSION_ID}/vmux_bridge.html"))
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
                .get::<ExtensionInfrastructureWebview>(entity)
                .is_some()
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
        assert!(
            app.world_mut()
                .query::<&RetiredExtensionInfrastructureWebview>()
                .iter(app.world())
                .any(|retired| retired.contains(entity))
        );
    }
}
