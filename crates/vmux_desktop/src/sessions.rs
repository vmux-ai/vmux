use std::path::{Path, PathBuf};

use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode, window::PrimaryWindow};
use bevy_cef::prelude::*;
use moonshine_save::prelude::TriggerLoad;
use vmux_core::PageMetadata;
use vmux_sessions::{
    event::{
        SESSIONS_LIST_EVENT, SESSIONS_WEBVIEW_URL, SessionCommandEvent, SessionRow,
        SessionsListEvent,
    },
    model::{
        DEFAULT_SESSION_ID, SessionRecord, SessionRegistry, default_session_record, registry_path,
        session_layout_path_for, unique_session_id,
    },
};
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppRegistry};

use crate::{
    browser::Browser,
    command_bar::NewTabContext,
    layout::{tab::Tab, window::WEBVIEW_MESH_DEPTH_BIAS},
    profile,
    settings::AppSettings,
};

#[derive(Resource, Clone, Debug)]
pub(crate) struct ActiveSession {
    pub record: SessionRecord,
}

impl Default for ActiveSession {
    fn default() -> Self {
        let registry = read_session_registry_from(&profile::shared_data_dir());
        let record = registry
            .sessions
            .iter()
            .find(|session| session.id == vmux_sessions::model::DEFAULT_SESSION_ID)
            .cloned()
            .or_else(|| registry.sessions.first().cloned())
            .unwrap_or_else(default_session_record);
        Self { record }
    }
}

impl ActiveSession {
    pub(crate) fn layout_path(&self) -> PathBuf {
        session_layout_path_for(
            &profile::shared_data_dir(),
            &self.record.id,
            &self.record.profile,
        )
    }
}

#[derive(Component)]
pub(crate) struct SessionsView;

impl SessionsView {
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SESSIONS_WEBVIEW_URL),
                ResolvedWebviewUri(SESSIONS_WEBVIEW_URL.to_string()),
                PageMetadata {
                    title: "Sessions".to_string(),
                    url: SESSIONS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Inherited,
                Pickable::default(),
            ),
        )
    }
}

pub(crate) struct SessionsPlugin;

impl Plugin for SessionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveSession>();
        register_sessions_webview_app(
            app.world_mut()
                .resource_mut::<WebviewAppRegistry>()
                .as_mut(),
        );
        app.add_plugins(JsEmitEventPlugin::<SessionCommandEvent>::default())
            .add_observer(on_session_command)
            .add_systems(Update, broadcast_sessions_to_views);
    }
}

fn register_sessions_webview_app(registry: &mut WebviewAppRegistry) {
    registry.register(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_sessions"),
        &WebviewAppConfig::with_custom_host("sessions"),
    );
}

fn read_session_registry_from(root: &Path) -> SessionRegistry {
    let mut registry = std::fs::read_to_string(registry_path(root))
        .ok()
        .and_then(|body| ron::de::from_str::<SessionRegistry>(&body).ok())
        .unwrap_or_default();
    if registry.sessions.is_empty() {
        registry.sessions.push(default_session_record());
    }
    if !registry
        .sessions
        .iter()
        .any(|session| session.id == vmux_sessions::model::DEFAULT_SESSION_ID)
    {
        registry.sessions.insert(0, default_session_record());
    }
    registry
}

fn write_session_registry_to(root: &Path, registry: &SessionRegistry) {
    let path = registry_path(root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(body) = ron::ser::to_string_pretty(registry, ron::ser::PrettyConfig::default()) {
        let _ = std::fs::write(path, body);
    }
}

fn delete_session_record(registry: &mut SessionRegistry, id: &str) -> Option<SessionRecord> {
    if id == DEFAULT_SESSION_ID {
        return None;
    }
    let idx = registry
        .sessions
        .iter()
        .position(|session| session.id == id)?;
    Some(registry.sessions.remove(idx))
}

fn delete_session_layout(root: &Path, record: &SessionRecord) {
    if record.id == DEFAULT_SESSION_ID {
        return;
    }
    let path = session_layout_path_for(root, &record.id, &record.profile);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn session_rows(
    active: &ActiveSession,
    registry: &SessionRegistry,
    active_tab_count: usize,
) -> Vec<SessionRow> {
    registry
        .sessions
        .iter()
        .map(|session| {
            let is_active = session.id == active.record.id;
            SessionRow {
                id: session.id.clone(),
                name: session.name.clone(),
                profile: session.profile.clone(),
                is_active,
                tab_count: if is_active { active_tab_count } else { 0 },
            }
        })
        .collect()
}

pub(crate) fn active_session_rows(
    active: &ActiveSession,
    active_tab_count: usize,
) -> Vec<SessionRow> {
    let registry = read_session_registry_from(&profile::shared_data_dir());
    session_rows(active, &registry, active_tab_count)
}

#[derive(Default)]
struct SessionBroadcastCache {
    body: String,
    sent: std::collections::HashSet<Entity>,
}

fn session_emit_targets(
    ready_views: &[Entity],
    body: &str,
    cache: &mut SessionBroadcastCache,
) -> Vec<Entity> {
    if body != cache.body {
        cache.body = body.to_string();
        cache.sent.clear();
    }
    ready_views
        .iter()
        .copied()
        .filter(|entity| cache.sent.insert(*entity))
        .collect()
}

fn broadcast_sessions_to_views(
    active: Res<ActiveSession>,
    sessions_views: Query<Entity, (With<SessionsView>, With<UiReady>)>,
    browsers: NonSend<Browsers>,
    tabs: Query<(), With<Tab>>,
    mut cache: Local<SessionBroadcastCache>,
    mut commands: Commands,
) {
    if sessions_views.is_empty() {
        return;
    }
    let registry = read_session_registry_from(&profile::shared_data_dir());
    let payload = SessionsListEvent {
        sessions: session_rows(&active, &registry, tabs.iter().count()),
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    let mut ready = Vec::new();
    for entity in &sessions_views {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            ready.push(entity);
        }
    }
    for entity in session_emit_targets(&ready, &body, &mut cache) {
        commands.trigger(HostEmitEvent::new(entity, SESSIONS_LIST_EVENT, &payload));
    }
}

fn on_session_command(
    trigger: On<Receive<SessionCommandEvent>>,
    mut active: ResMut<ActiveSession>,
    session_entities: Query<
        Entity,
        Or<(
            With<crate::profile::Profile>,
            With<crate::layout::space::Space>,
            With<crate::layout::HeaderState>,
            With<crate::layout::SideSheetState>,
            With<vmux_history::Visit>,
        )>,
    >,
    main_q: Query<Entity, With<crate::layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut commands: Commands,
) {
    let root = profile::shared_data_dir();
    let mut registry = read_session_registry_from(&root);
    let evt = &trigger.event().payload;
    if evt.command == "delete" {
        let Some(id) = evt.session_id.as_deref() else {
            return;
        };
        let Some(deleted) = delete_session_record(&mut registry, id) else {
            return;
        };
        let deleted_active = deleted.id == active.record.id;
        let target = registry
            .sessions
            .first()
            .cloned()
            .unwrap_or_else(default_session_record);
        write_session_registry_to(&root, &registry);
        delete_session_layout(&root, &deleted);
        if !deleted_active {
            return;
        }
        active.record = target;
        let target_path = active.layout_path();
        if target_path.exists() {
            commands.trigger_load(moonshine_save::prelude::LoadWorld::default_from_file(
                target_path,
            ));
        } else {
            for entity in &session_entities {
                commands.entity(entity).despawn();
            }
            let Ok(main) = main_q.single() else { return };
            crate::layout::window::spawn_default_session_layout(
                main,
                *primary_window,
                &settings.layout,
                &mut new_tab_ctx,
                &mut commands,
            );
        }
        return;
    }

    let target = match evt.command.as_str() {
        "attach" => {
            let Some(id) = evt.session_id.as_deref() else {
                return;
            };
            let Some(record) = registry.sessions.iter().find(|session| session.id == id) else {
                return;
            };
            record.clone()
        }
        "new" => {
            let name = evt
                .name
                .clone()
                .unwrap_or_else(|| format!("Session {}", registry.sessions.len() + 1));
            let id = unique_session_id(&registry.sessions, &name);
            let record = SessionRecord {
                id,
                name,
                profile: active.record.profile.clone(),
            };
            registry.sessions.push(record.clone());
            write_session_registry_to(&root, &registry);
            record
        }
        _ => return,
    };

    if target.id == active.record.id {
        return;
    }

    let current_path = active.layout_path();
    crate::persistence::save_session_to_path(&mut commands, current_path);
    active.record = target;
    let target_path = active.layout_path();
    if target_path.exists() {
        commands.trigger_load(moonshine_save::prelude::LoadWorld::default_from_file(
            target_path,
        ));
    } else {
        for entity in &session_entities {
            commands.entity(entity).despawn();
        }
        let Ok(main) = main_q.single() else { return };
        crate::layout::window::spawn_default_session_layout(
            main,
            *primary_window,
            &settings.layout,
            &mut new_tab_ctx,
            &mut commands,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_sessions::model::DEFAULT_PROFILE_ID;
    use vmux_webview_app::WebviewAppRegistry;

    #[test]
    fn rows_mark_active_session_and_profile() {
        let active = ActiveSession {
            record: SessionRecord {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: DEFAULT_PROFILE_ID.to_string(),
            },
        };
        let registry = SessionRegistry {
            sessions: vec![default_session_record(), active.record.clone()],
        };
        let rows = session_rows(&active, &registry, 4);
        assert!(!rows[0].is_active);
        assert!(rows[1].is_active);
        assert_eq!(rows[1].profile, DEFAULT_PROFILE_ID);
        assert_eq!(rows[1].tab_count, 4);
    }

    #[test]
    fn registers_sessions_host_before_cef_embedded_hosts_are_read() {
        let mut registry = WebviewAppRegistry::default();
        register_sessions_webview_app(&mut registry);

        let hosts = registry.embedded_hosts();
        let entry = hosts.entry_for_host("sessions").unwrap();
        assert_eq!(entry.default_document, "sessions/index.html");
    }

    #[test]
    fn unchanged_payload_is_sent_to_new_sessions_view() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut cache = SessionBroadcastCache::default();

        assert_eq!(
            session_emit_targets(&[first], "same", &mut cache),
            vec![first]
        );
        assert_eq!(
            session_emit_targets(&[first, second], "same", &mut cache),
            vec![second]
        );
    }

    #[test]
    fn delete_session_removes_named_session_from_registry() {
        let mut registry = SessionRegistry {
            sessions: vec![
                default_session_record(),
                SessionRecord {
                    id: "work".to_string(),
                    name: "Work".to_string(),
                    profile: DEFAULT_PROFILE_ID.to_string(),
                },
            ],
        };

        let deleted = delete_session_record(&mut registry, "work").unwrap();

        assert_eq!(deleted.id, "work");
        assert_eq!(registry.sessions, vec![default_session_record()]);
    }

    #[test]
    fn delete_session_keeps_default_session() {
        let mut registry = SessionRegistry {
            sessions: vec![default_session_record()],
        };

        let deleted = delete_session_record(&mut registry, "default");

        assert!(deleted.is_none());
        assert_eq!(registry.sessions, vec![default_session_record()]);
    }
}
