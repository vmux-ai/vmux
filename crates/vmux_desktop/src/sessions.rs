use std::path::{Path, PathBuf};

use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode, window::PrimaryWindow};
use bevy_cef::prelude::*;
use moonshine_save::prelude::TriggerLoad;
use vmux_core::PageMetadata;
use vmux_session::{
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
            .find(|session| session.id == vmux_session::model::DEFAULT_SESSION_ID)
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
        app.add_plugins(BinJsEmitEventPlugin::<SessionCommandEvent>::default())
            .add_observer(on_session_command)
            .add_systems(
                Update,
                (apply_pending_session_switch, broadcast_sessions_to_views).chain(),
            );
    }
}

fn register_sessions_webview_app(registry: &mut WebviewAppRegistry) {
    registry.register(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_session"),
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
        .any(|session| session.id == vmux_session::model::DEFAULT_SESSION_ID)
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
                tab_count: if is_active {
                    active_tab_count as u32
                } else {
                    0
                },
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

#[derive(Resource)]
struct PendingSessionSwitch {
    from_id: String,
    record: SessionRecord,
    delay_frames: u8,
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
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            SESSIONS_LIST_EVENT,
            &payload,
        ));
    }
}

fn apply_pending_session_switch(
    pending: Option<ResMut<PendingSessionSwitch>>,
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
    focus: Option<ResMut<crate::layout::tab::FocusedTab>>,
    mut commands: Commands,
) {
    let Some(mut pending) = pending else {
        return;
    };
    if pending.delay_frames > 0 {
        pending.delay_frames -= 1;
        return;
    }
    let record = pending.record.clone();
    let from_id = pending.from_id.clone();
    commands.remove_resource::<PendingSessionSwitch>();
    if from_id != active.record.id {
        return;
    }
    active.record = record.clone();
    let target_path =
        session_layout_path_for(&profile::shared_data_dir(), &record.id, &record.profile);
    if target_path.exists() {
        commands.trigger_load(moonshine_save::prelude::LoadWorld::default_from_file(
            target_path,
        ));
    } else {
        for entity in &session_entities {
            commands.entity(entity).try_despawn();
        }
        let Ok(main) = main_q.single() else { return };
        let spawned = crate::layout::window::spawn_default_session_layout(
            main,
            *primary_window,
            &settings.layout,
            &mut new_tab_ctx,
            &mut commands,
        );
        if let Some(mut focus) = focus {
            focus.space = Some(spawned.space);
            focus.pane = Some(spawned.pane);
            focus.tab = Some(spawned.tab);
        }
    }
}

fn spawn_sessions_page_layout(
    main: Entity,
    primary_window: Entity,
    settings: &AppSettings,
    new_tab_ctx: &mut NewTabContext,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    focus: Option<&mut crate::layout::tab::FocusedTab>,
    commands: &mut Commands,
) {
    let spawned = crate::layout::window::spawn_default_session_layout(
        main,
        primary_window,
        &settings.layout,
        new_tab_ctx,
        commands,
    );
    if let Some(focus) = focus {
        focus.space = Some(spawned.space);
        focus.pane = Some(spawned.pane);
        focus.tab = Some(spawned.tab);
    }
    let Some(tab) = new_tab_ctx.tab.take() else {
        return;
    };
    new_tab_ctx.previous_tab = None;
    new_tab_ctx.needs_open = false;
    new_tab_ctx.dismiss_modal = false;
    commands.entity(tab).insert(PageMetadata {
        title: "Sessions".to_string(),
        url: SESSIONS_WEBVIEW_URL.to_string(),
        favicon_url: String::new(),
    });
    commands.spawn((SessionsView::new(meshes, webview_mt), ChildOf(tab)));
}

fn on_session_command(
    trigger: On<BinReceive<SessionCommandEvent>>,
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
    mut focus: Option<ResMut<crate::layout::tab::FocusedTab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
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
        let from_id = active.record.id.clone();
        commands.insert_resource(PendingSessionSwitch {
            from_id,
            record: target,
            delay_frames: 1,
        });
        return;
    }

    let (target, open_sessions_page) = match evt.command.as_str() {
        "attach" => {
            let Some(id) = evt.session_id.as_deref() else {
                return;
            };
            let Some(record) = registry.sessions.iter().find(|session| session.id == id) else {
                return;
            };
            (record.clone(), false)
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
            (record, true)
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
            commands.entity(entity).try_despawn();
        }
        let Ok(main) = main_q.single() else { return };
        if open_sessions_page {
            spawn_sessions_page_layout(
                main,
                *primary_window,
                &settings,
                &mut new_tab_ctx,
                &mut meshes,
                &mut webview_mt,
                focus.as_deref_mut(),
                &mut commands,
            );
        } else {
            let spawned = crate::layout::window::spawn_default_session_layout(
                main,
                *primary_window,
                &settings.layout,
                &mut new_tab_ctx,
                &mut commands,
            );
            if let Some(mut focus) = focus {
                focus.space = Some(spawned.space);
                focus.pane = Some(spawned.pane);
                focus.tab = Some(spawned.tab);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        layout::{pane::Pane, space::Space, tab::Tab, window::Main},
        settings::{
            AppSettings, BrowserSettings, FocusRingSettings, LayoutSettings, PaneSettings,
            ShortcutSettings, SideSheetSettings, WindowSettings,
        },
    };
    use vmux_history::LastActivatedAt;
    use vmux_session::model::DEFAULT_PROFILE_ID;
    use vmux_webview_app::WebviewAppRegistry;

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = profile::HOME_ENV_LOCK
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let old_home = std::env::var_os("HOME");
            let home =
                std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).expect("create temp home");
            unsafe {
                std::env::set_var("HOME", &home);
            }
            Self {
                _guard: guard,
                old_home,
            }
        }
    }

    impl Drop for HomeEnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(home) = &self.old_home {
                    std::env::set_var("HOME", home);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
        }
    }

    fn work_session_record() -> SessionRecord {
        SessionRecord {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: DEFAULT_PROFILE_ID.to_string(),
        }
    }

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

    #[test]
    fn delete_active_session_defers_current_layout_teardown() {
        let _home = HomeEnvGuard::use_temp_home("delete-active-session-defers-layout");
        let active_record = work_session_record();
        write_session_registry_to(
            &profile::shared_data_dir(),
            &SessionRegistry {
                sessions: vec![default_session_record(), active_record.clone()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_session_command);
        app.insert_resource(ActiveSession {
            record: active_record,
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewTabContext>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((Space::default(), ChildOf(main)))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                PageMetadata {
                    title: "Sessions".to_string(),
                    url: SESSIONS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SessionsView, ChildOf(tab))).id();

        app.world_mut()
            .entity_mut(webview)
            .trigger(|webview| BinReceive {
                webview,
                payload: SessionCommandEvent {
                    command: "delete".to_string(),
                    session_id: Some("work".to_string()),
                    name: None,
                },
            });
        app.update();

        assert!(app.world().get_entity(space).is_ok());
        assert_eq!(app.world().resource::<ActiveSession>().record.id, "work");
    }

    #[test]
    fn deferred_active_session_delete_spawns_target_layout() {
        let _home = HomeEnvGuard::use_temp_home("deferred-active-session-delete-switches");
        let active_record = work_session_record();
        write_session_registry_to(
            &profile::shared_data_dir(),
            &SessionRegistry {
                sessions: vec![default_session_record(), active_record.clone()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_session_command);
        app.add_systems(Update, apply_pending_session_switch);
        app.insert_resource(ActiveSession {
            record: active_record,
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewTabContext>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((Space::default(), ChildOf(main)))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                PageMetadata {
                    title: "Sessions".to_string(),
                    url: SESSIONS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SessionsView, ChildOf(tab))).id();

        app.world_mut()
            .entity_mut(webview)
            .trigger(|webview| BinReceive {
                webview,
                payload: SessionCommandEvent {
                    command: "delete".to_string(),
                    session_id: Some("work".to_string()),
                    name: None,
                },
            });
        app.update();
        assert!(app.world().get_entity(space).is_ok());

        for _ in 0..3 {
            app.update();
            if app.world().get_entity(space).is_err() {
                break;
            }
        }
        assert!(app.world().get_entity(space).is_err());
        assert_eq!(
            app.world().resource::<ActiveSession>().record,
            default_session_record()
        );
        let mut spaces = app.world_mut().query::<&Space>();
        assert_eq!(spaces.iter(app.world()).count(), 1);
    }

    #[test]
    fn new_session_spawns_sessions_page_layout() {
        let _home = HomeEnvGuard::use_temp_home("new-session-spawns-sessions-page");
        write_session_registry_to(
            &profile::shared_data_dir(),
            &SessionRegistry {
                sessions: vec![default_session_record()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_session_command);
        app.insert_resource(crate::layout::tab::FocusedTab::default());
        app.insert_resource(ActiveSession {
            record: default_session_record(),
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewTabContext>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let old_space = app
            .world_mut()
            .spawn((Space::default(), ChildOf(main)))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(old_space))).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                PageMetadata {
                    title: "Sessions".to_string(),
                    url: SESSIONS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SessionsView, ChildOf(tab))).id();
        *app.world_mut()
            .resource_mut::<crate::layout::tab::FocusedTab>() = crate::layout::tab::FocusedTab {
            space: Some(old_space),
            pane: Some(pane),
            tab: Some(tab),
        };

        app.world_mut()
            .entity_mut(webview)
            .trigger(|webview| BinReceive {
                webview,
                payload: SessionCommandEvent {
                    command: "new".to_string(),
                    session_id: None,
                    name: Some("Client A".to_string()),
                },
            });
        app.update();

        assert!(app.world().get_entity(old_space).is_err());
        assert_eq!(
            app.world().resource::<ActiveSession>().record.id,
            "client-a"
        );
        assert!(!app.world().resource::<NewTabContext>().needs_open);
        assert!(app.world().resource::<NewTabContext>().tab.is_none());

        let mut spaces = app.world_mut().query::<&Space>();
        assert_eq!(spaces.iter(app.world()).count(), 1);

        let tabs = {
            let mut tab_q = app
                .world_mut()
                .query_filtered::<(Entity, &PageMetadata, &Children), With<Tab>>();
            tab_q
                .iter(app.world())
                .map(|(entity, meta, children)| {
                    let has_sessions_view = children
                        .iter()
                        .any(|child| app.world().get::<SessionsView>(child).is_some());
                    (entity, meta.url.clone(), has_sessions_view)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].1, SESSIONS_WEBVIEW_URL);
        assert!(tabs[0].2);
        let focus = app.world().resource::<crate::layout::tab::FocusedTab>();
        assert_ne!(focus.space, Some(old_space));
        assert!(focus.space.is_some());
        assert!(focus.pane.is_some());
        assert!(focus.tab.is_some());
    }

    #[test]
    fn ipc_new_session_focuses_sessions_page_layout_in_same_update() {
        let _home = HomeEnvGuard::use_temp_home("ipc-new-session-focuses-layout");
        write_session_registry_to(
            &profile::shared_data_dir(),
            &SessionRegistry {
                sessions: vec![default_session_record()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(bevy_cef::prelude::JsEmitEventPlugin::<SessionCommandEvent>::default());
        app.add_plugins(crate::layout::tab::TabPlugin);
        app.add_message::<vmux_layout::LayoutSpawnRequest>();
        app.add_observer(on_session_command);
        app.init_resource::<crate::layout::pane::PendingCursorWarp>();
        app.init_resource::<bevy_cef::prelude::IpcEventRawBuffer>();
        app.insert_resource(ActiveSession {
            record: default_session_record(),
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewTabContext>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let old_space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt::now(), ChildOf(main)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(old_space)))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                LastActivatedAt::now(),
                PageMetadata {
                    title: "Sessions".to_string(),
                    url: SESSIONS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SessionsView, ChildOf(tab))).id();
        let payload = serde_json::to_string(&SessionCommandEvent {
            command: "new".to_string(),
            session_id: None,
            name: Some("Client A".to_string()),
        })
        .unwrap();
        app.world_mut()
            .resource_mut::<bevy_cef::prelude::IpcEventRawBuffer>()
            .0
            .push(bevy_cef_core::prelude::IpcEventRaw { webview, payload });

        app.update();

        let focus = app.world().resource::<crate::layout::tab::FocusedTab>();
        assert!(focus.space.is_some());
        assert!(focus.pane.is_some());
        assert!(focus.tab.is_some());
    }
}
