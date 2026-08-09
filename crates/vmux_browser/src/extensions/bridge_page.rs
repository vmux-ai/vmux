use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{
    CefShutdownState, PrivatePreloadScripts, WebviewMaxFrameRate, WebviewSize, WebviewSource,
};
use std::collections::{HashMap, HashSet};

use super::bridge::{BridgeIdentity, ExtensionBridgeServer};
use super::load::PreparedExtensions;

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

#[derive(Resource, Default)]
pub(crate) struct ExtensionBridgeLifecycle {
    stopping: bool,
    spawned: HashMap<(String, ExtensionBridgeRole), Entity>,
}

#[derive(Resource, Default)]
pub(crate) struct ExtensionInfrastructureEntities(HashSet<Entity>);

impl ExtensionInfrastructureEntities {
    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.0.contains(&entity)
    }

    pub(crate) fn insert(&mut self, entity: Entity) {
        self.0.insert(entity);
    }
}

pub fn stop_extension_bridge_pages(
    mut exits: MessageReader<AppExit>,
    mut lifecycle: ResMut<ExtensionBridgeLifecycle>,
    pages: Query<Entity, With<ExtensionBridgeWebview>>,
    mut commands: Commands,
) {
    if exits.read().count() == 0 {
        return;
    }
    lifecycle.stopping = true;
    for entity in &pages {
        commands.entity(entity).despawn();
    }
}

pub fn spawn_extension_bridge_pages(
    mut commands: Commands,
    prepared: Res<PreparedExtensions>,
    server: Res<ExtensionBridgeServer>,
    primary_window: Query<(), With<PrimaryWindow>>,
    added_primary_window: Query<(), Added<PrimaryWindow>>,
    pages: Query<Entity, With<ExtensionBridgeWebview>>,
    mut removed_pages: RemovedComponents<ExtensionBridgeWebview>,
    shutdown: Option<Res<CefShutdownState>>,
    mut lifecycle: ResMut<ExtensionBridgeLifecycle>,
    mut infrastructure: ResMut<ExtensionInfrastructureEntities>,
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
    if lifecycle.stopping
        || shutdown.is_some_and(|state| state.started())
        || primary_window.is_empty()
    {
        return;
    }
    let conformance = super::broker::extension_conformance_enabled();
    let desired = prepared
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
    lifecycle.spawned.retain(|key, entity| {
        if desired.contains(key) && pages.contains(*entity) {
            return true;
        }
        if pages.contains(*entity) {
            commands.entity(*entity).despawn();
        }
        false
    });
    for runtime in &prepared.0 {
        let identity = server
            .identity(&runtime.extension_id)
            .unwrap_or_else(|| panic!("missing bridge identity for {}", runtime.extension_id));
        let transport = (runtime.extension_id.clone(), ExtensionBridgeRole::Transport);
        if !lifecycle
            .spawned
            .get(&transport)
            .is_some_and(|entity| pages.contains(*entity))
        {
            let entity = commands
                .spawn((
                    ExtensionBridgeWebview {
                        extension_id: runtime.extension_id.clone(),
                        role: ExtensionBridgeRole::Transport,
                    },
                    WebviewSource::new(format!(
                        "chrome-extension://{}/vmux_bridge.html",
                        runtime.extension_id
                    )),
                    PrivatePreloadScripts::from([bridge_config_source(
                        &server,
                        identity,
                        conformance,
                    )]),
                    WebviewSize(Vec2::ONE),
                    WebviewMaxFrameRate(1),
                    Visibility::Hidden,
                ))
                .id();
            infrastructure.insert(entity);
            lifecycle.spawned.insert(transport, entity);
        }
        let echo = (
            runtime.extension_id.clone(),
            ExtensionBridgeRole::ConformanceEcho,
        );
        if conformance
            && !lifecycle
                .spawned
                .get(&echo)
                .is_some_and(|entity| pages.contains(*entity))
        {
            let entity = commands
                .spawn((
                    ExtensionBridgeWebview {
                        extension_id: runtime.extension_id.clone(),
                        role: ExtensionBridgeRole::ConformanceEcho,
                    },
                    WebviewSource::new(format!(
                        "chrome-extension://{}/echo.html",
                        runtime.extension_id
                    )),
                    WebviewSize(Vec2::ONE),
                    WebviewMaxFrameRate(1),
                    Visibility::Hidden,
                ))
                .id();
            infrastructure.insert(entity);
            lifecycle.spawned.insert(echo, entity);
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
}
