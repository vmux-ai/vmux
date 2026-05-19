use std::path::{Path, PathBuf};

use bevy::{prelude::*, window::PrimaryWindow};
use bevy_cef::prelude::*;
use moonshine_save::prelude::TriggerLoad;
use vmux_core::PageMetadata;
use vmux_space::{
    event::{SPACES_LIST_EVENT, SPACES_WEBVIEW_URL, SpaceCommandEvent, SpaceRow, SpacesListEvent},
    model::{
        DEFAULT_SPACE_ID, SpaceRecord, SpaceRegistry, default_space_record, registry_path,
        space_layout_path_for, unique_space_id,
    },
};
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppRegistry};

use crate::{command_bar::NewStackContext, profile};
pub(crate) use vmux_layout::spaces::{
    ActiveSpace, SpacesView, active_space_rows, read_space_registry_from,
};
use vmux_layout::stack::Stack;

pub(crate) struct SpacesPlugin;

impl Plugin for SpacesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveSpace>();
        register_spaces_webview_app(
            app.world_mut()
                .resource_mut::<WebviewAppRegistry>()
                .as_mut(),
        );
        app.add_plugins(BinJsEmitEventPlugin::<SpaceCommandEvent>::default())
            .add_observer(on_space_command)
            .add_observer(reset_spaces_sent_marker_on_ui_ready)
            .add_systems(
                Update,
                (apply_pending_space_switch, broadcast_spaces_to_views).chain(),
            );
    }
}

#[derive(Component)]
struct SpacesListSent;

fn reset_spaces_sent_marker_on_ui_ready(
    trigger: On<BinReceive<UiReady>>,
    spaces_views: Query<(), With<SpacesView>>,
    chrome_views: Query<(), With<vmux_layout::LayoutChrome>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if spaces_views.get(entity).is_err() && chrome_views.get(entity).is_err() {
        return;
    }
    commands.entity(entity).remove::<SpacesListSent>();
}

fn register_spaces_webview_app(registry: &mut WebviewAppRegistry) {
    registry.register(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_space"),
        &WebviewAppConfig::with_custom_host("spaces"),
    );
}

fn write_space_registry_to(root: &Path, registry: &SpaceRegistry) {
    let path = registry_path(root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(body) = ron::ser::to_string_pretty(registry, ron::ser::PrettyConfig::default()) {
        let _ = std::fs::write(path, body);
    }
}

fn delete_space_record(registry: &mut SpaceRegistry, id: &str) -> Option<SpaceRecord> {
    if id == DEFAULT_SPACE_ID {
        return None;
    }
    let idx = registry.spaces.iter().position(|space| space.id == id)?;
    Some(registry.spaces.remove(idx))
}

fn delete_space_layout(root: &Path, record: &SpaceRecord) {
    if record.id == DEFAULT_SPACE_ID {
        return;
    }
    let path = space_layout_path_for(root, &record.id, &record.profile);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn space_rows(
    active: &ActiveSpace,
    registry: &SpaceRegistry,
    active_stack_count: usize,
) -> Vec<SpaceRow> {
    registry
        .spaces
        .iter()
        .map(|space| {
            let is_active = space.id == active.record.id;
            SpaceRow {
                id: space.id.clone(),
                name: space.name.clone(),
                profile: space.profile.clone(),
                is_active,
                tab_count: if is_active {
                    active_stack_count as u32
                } else {
                    0
                },
            }
        })
        .collect()
}

#[derive(Resource)]
struct PendingSpaceSwitch {
    from_id: String,
    record: SpaceRecord,
    delay_frames: u8,
}

fn broadcast_spaces_to_views(
    active: Res<ActiveSpace>,
    pending_spaces: Query<Entity, (With<SpacesView>, With<UiReady>, Without<SpacesListSent>)>,
    sent_spaces: Query<Entity, (With<SpacesView>, With<UiReady>, With<SpacesListSent>)>,
    pending_chrome: Query<
        Entity,
        (
            With<vmux_layout::LayoutChrome>,
            With<UiReady>,
            Without<SpacesListSent>,
        ),
    >,
    sent_chrome: Query<
        Entity,
        (
            With<vmux_layout::LayoutChrome>,
            With<UiReady>,
            With<SpacesListSent>,
        ),
    >,
    browsers: NonSend<Browsers>,
    tabs: Query<(), With<Stack>>,
    mut last_body: Local<String>,
    mut commands: Commands,
) {
    let pending_total = pending_spaces.iter().count() + pending_chrome.iter().count();
    let sent_total = sent_spaces.iter().count() + sent_chrome.iter().count();
    if pending_total == 0 && sent_total == 0 {
        return;
    }
    let registry = read_space_registry_from(&profile::shared_data_dir());
    let payload = SpacesListEvent {
        spaces: space_rows(&active, &registry, tabs.iter().count()),
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    let body_changed = body != *last_body;
    for entity in pending_spaces.iter().chain(pending_chrome.iter()) {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            SPACES_LIST_EVENT,
            &payload,
        ));
        commands.entity(entity).insert(SpacesListSent);
    }
    if body_changed {
        for entity in sent_spaces.iter().chain(sent_chrome.iter()) {
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                SPACES_LIST_EVENT,
                &payload,
            ));
        }
        *last_body = body;
    }
}

fn apply_pending_space_switch(
    pending: Option<ResMut<PendingSpaceSwitch>>,
    mut active: ResMut<ActiveSpace>,
    space_entities: Query<
        Entity,
        Or<(
            With<crate::profile::Profile>,
            With<vmux_layout::space::Space>,
            With<vmux_history::Visit>,
        )>,
    >,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    focus: Option<ResMut<vmux_layout::stack::FocusedStack>>,
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
    commands.remove_resource::<PendingSpaceSwitch>();
    if from_id != active.record.id {
        return;
    }
    active.record = record.clone();
    let target_path =
        space_layout_path_for(&profile::shared_data_dir(), &record.id, &record.profile);
    if target_path.exists() {
        commands.trigger_load(moonshine_save::prelude::LoadWorld::default_from_file(
            target_path,
        ));
    } else {
        for entity in &space_entities {
            commands.entity(entity).try_despawn();
        }
        let Ok(main) = main_q.single() else { return };
        let spawned = vmux_layout::window::spawn_default_space_layout(
            main,
            *primary_window,
            &mut new_stack_ctx,
            &mut commands,
        );
        if let Some(mut focus) = focus {
            focus.tab = Some(spawned.tab);
            focus.pane = Some(spawned.pane);
            focus.stack = Some(spawned.stack);
        }
    }
}

fn spawn_spaces_page_layout(
    main: Entity,
    primary_window: Entity,
    new_stack_ctx: &mut NewStackContext,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    focus: Option<&mut vmux_layout::stack::FocusedStack>,
    commands: &mut Commands,
) {
    let spawned = vmux_layout::window::spawn_default_space_layout(
        main,
        primary_window,
        new_stack_ctx,
        commands,
    );
    if let Some(focus) = focus {
        focus.tab = Some(spawned.tab);
        focus.pane = Some(spawned.pane);
        focus.stack = Some(spawned.stack);
    }
    let Some(tab) = new_stack_ctx.stack.take() else {
        return;
    };
    new_stack_ctx.previous_stack = None;
    new_stack_ctx.needs_open = false;
    new_stack_ctx.dismiss_modal = false;
    commands.entity(tab).insert(PageMetadata {
        title: "Spaces".to_string(),
        url: SPACES_WEBVIEW_URL.to_string(),
        favicon_url: String::new(),
        bg_color: None,
    });
    commands.spawn((SpacesView::new(meshes, webview_mt), ChildOf(tab)));
}

fn on_space_command(
    trigger: On<BinReceive<SpaceCommandEvent>>,
    mut active: ResMut<ActiveSpace>,
    space_entities: Query<
        Entity,
        Or<(
            With<crate::profile::Profile>,
            With<vmux_layout::space::Space>,
            With<vmux_history::Visit>,
        )>,
    >,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut focus: Option<ResMut<vmux_layout::stack::FocusedStack>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut spawn_requests: Option<MessageWriter<vmux_layout::LayoutSpawnRequest>>,
    stack_q: Query<(Entity, &PageMetadata), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
) {
    let root = profile::shared_data_dir();
    let mut registry = read_space_registry_from(&root);
    let evt = &trigger.event().payload;
    if evt.command == "open_page" {
        if let Some((existing, _)) = stack_q
            .iter()
            .find(|(_, meta)| meta.url == SPACES_WEBVIEW_URL)
        {
            vmux_terminal::pid::focus_pane_entity(existing, &mut commands, &child_of_q);
            return;
        }
        let Some(focus_res) = focus.as_deref() else {
            return;
        };
        let Some(pane) = focus_res.pane else {
            return;
        };
        let Some(spawn_requests) = spawn_requests.as_mut() else {
            return;
        };
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                vmux_history::LastActivatedAt::now(),
                ChildOf(pane),
            ))
            .id();
        spawn_requests.write(vmux_layout::LayoutSpawnRequest::OpenUrl {
            stack,
            url: SPACES_WEBVIEW_URL.to_string(),
        });
        return;
    }
    if evt.command == "delete" {
        let Some(id) = evt.space_id.as_deref() else {
            return;
        };
        let Some(deleted) = delete_space_record(&mut registry, id) else {
            return;
        };
        let deleted_active = deleted.id == active.record.id;
        let target = registry
            .spaces
            .first()
            .cloned()
            .unwrap_or_else(default_space_record);
        write_space_registry_to(&root, &registry);
        delete_space_layout(&root, &deleted);
        if !deleted_active {
            return;
        }
        let from_id = active.record.id.clone();
        commands.insert_resource(PendingSpaceSwitch {
            from_id,
            record: target,
            delay_frames: 1,
        });
        return;
    }

    let (target, open_spaces_page) = match evt.command.as_str() {
        "attach" => {
            let Some(id) = evt.space_id.as_deref() else {
                return;
            };
            let Some(record) = registry.spaces.iter().find(|space| space.id == id) else {
                return;
            };
            (record.clone(), false)
        }
        "new" => {
            let name = evt
                .name
                .clone()
                .unwrap_or_else(|| format!("Space {}", registry.spaces.len() + 1));
            let id = unique_space_id(&registry.spaces, &name);
            let record = SpaceRecord {
                id,
                name,
                profile: active.record.profile.clone(),
            };
            registry.spaces.push(record.clone());
            write_space_registry_to(&root, &registry);
            (record, true)
        }
        _ => return,
    };

    if target.id == active.record.id {
        return;
    }

    let current_path = active.layout_path();
    crate::persistence::save_space_to_path(&mut commands, current_path);
    active.record = target;
    let target_path = active.layout_path();
    if target_path.exists() {
        commands.trigger_load(moonshine_save::prelude::LoadWorld::default_from_file(
            target_path,
        ));
    } else {
        for entity in &space_entities {
            commands.entity(entity).try_despawn();
        }
        let Ok(main) = main_q.single() else { return };
        if open_spaces_page {
            spawn_spaces_page_layout(
                main,
                *primary_window,
                &mut new_stack_ctx,
                &mut meshes,
                &mut webview_mt,
                focus.as_deref_mut(),
                &mut commands,
            );
        } else {
            let spawned = vmux_layout::window::spawn_default_space_layout(
                main,
                *primary_window,
                &mut new_stack_ctx,
                &mut commands,
            );
            if let Some(mut focus) = focus {
                focus.tab = Some(spawned.tab);
                focus.pane = Some(spawned.pane);
                focus.stack = Some(spawned.stack);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_history::LastActivatedAt;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_layout::{pane::Pane, space::Space, stack::Stack, window::Main};
    use vmux_settings::{AppSettings, BrowserSettings, ShortcutSettings};
    use vmux_space::model::DEFAULT_PROFILE_ID;
    use vmux_webview_app::WebviewAppRegistry;

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = profile::ENV_LOCK
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
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
            agent: vmux_settings::AgentSettings::default(),
        }
    }

    fn work_space_record() -> SpaceRecord {
        SpaceRecord {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: DEFAULT_PROFILE_ID.to_string(),
        }
    }

    #[test]
    fn rows_mark_active_space_and_profile() {
        let active = ActiveSpace {
            record: SpaceRecord {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: DEFAULT_PROFILE_ID.to_string(),
            },
        };
        let registry = SpaceRegistry {
            spaces: vec![default_space_record(), active.record.clone()],
        };
        let rows = space_rows(&active, &registry, 4);
        assert!(!rows[0].is_active);
        assert!(rows[1].is_active);
        assert_eq!(rows[1].profile, DEFAULT_PROFILE_ID);
        assert_eq!(rows[1].tab_count, 4);
    }

    #[test]
    fn registers_spaces_host_before_cef_embedded_hosts_are_read() {
        let mut registry = WebviewAppRegistry::default();
        register_spaces_webview_app(&mut registry);

        let hosts = registry.embedded_hosts();
        let entry = hosts.entry_for_host("spaces").unwrap();
        assert_eq!(entry.default_document, "spaces/index.html");
    }

    #[test]
    fn delete_space_removes_named_space_from_registry() {
        let mut registry = SpaceRegistry {
            spaces: vec![
                default_space_record(),
                SpaceRecord {
                    id: "work".to_string(),
                    name: "Work".to_string(),
                    profile: DEFAULT_PROFILE_ID.to_string(),
                },
            ],
        };

        let deleted = delete_space_record(&mut registry, "work").unwrap();

        assert_eq!(deleted.id, "work");
        assert_eq!(registry.spaces, vec![default_space_record()]);
    }

    #[test]
    fn delete_space_keeps_default_space() {
        let mut registry = SpaceRegistry {
            spaces: vec![default_space_record()],
        };

        let deleted = delete_space_record(&mut registry, "default");

        assert!(deleted.is_none());
        assert_eq!(registry.spaces, vec![default_space_record()]);
    }

    #[test]
    fn delete_active_space_defers_current_layout_teardown() {
        let _home = HomeEnvGuard::use_temp_home("delete-active-space-defers-layout");
        let active_record = work_space_record();
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![default_space_record(), active_record.clone()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_space_command);
        app.insert_resource(ActiveSpace {
            record: active_record,
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewStackContext>();
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
                Stack::default(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SpacesView, ChildOf(tab))).id();

        app.world_mut()
            .entity_mut(webview)
            .trigger(|webview| BinReceive {
                webview,
                payload: SpaceCommandEvent {
                    command: "delete".to_string(),
                    space_id: Some("work".to_string()),
                    name: None,
                },
            });
        app.update();

        assert!(app.world().get_entity(space).is_ok());
        assert_eq!(app.world().resource::<ActiveSpace>().record.id, "work");
    }

    #[test]
    fn deferred_active_space_delete_spawns_target_layout() {
        let _home = HomeEnvGuard::use_temp_home("deferred-active-space-delete-switches");
        let active_record = work_space_record();
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![default_space_record(), active_record.clone()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_space_command);
        app.add_systems(Update, apply_pending_space_switch);
        app.insert_resource(ActiveSpace {
            record: active_record,
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewStackContext>();
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
                Stack::default(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SpacesView, ChildOf(tab))).id();

        app.world_mut()
            .entity_mut(webview)
            .trigger(|webview| BinReceive {
                webview,
                payload: SpaceCommandEvent {
                    command: "delete".to_string(),
                    space_id: Some("work".to_string()),
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
            app.world().resource::<ActiveSpace>().record,
            default_space_record()
        );
        let mut tab_query = app.world_mut().query::<&Space>();
        assert_eq!(tab_query.iter(app.world()).count(), 1);
    }

    #[test]
    fn new_space_spawns_spaces_page_layout() {
        let _home = HomeEnvGuard::use_temp_home("new-space-spawns-spaces-page");
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![default_space_record()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_space_command);
        app.insert_resource(vmux_layout::stack::FocusedStack::default());
        app.insert_resource(ActiveSpace {
            record: default_space_record(),
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewStackContext>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let old_tab = app
            .world_mut()
            .spawn((Space::default(), ChildOf(main)))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(old_tab))).id();
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SpacesView, ChildOf(tab))).id();
        *app.world_mut()
            .resource_mut::<vmux_layout::stack::FocusedStack>() =
            vmux_layout::stack::FocusedStack {
                tab: Some(old_tab),
                pane: Some(pane),
                stack: Some(tab),
            };

        app.world_mut()
            .entity_mut(webview)
            .trigger(|webview| BinReceive {
                webview,
                payload: SpaceCommandEvent {
                    command: "new".to_string(),
                    space_id: None,
                    name: Some("Client A".to_string()),
                },
            });
        app.update();

        assert!(app.world().get_entity(old_tab).is_err());
        assert_eq!(app.world().resource::<ActiveSpace>().record.id, "client-a");
        assert!(!app.world().resource::<NewStackContext>().needs_open);
        assert!(app.world().resource::<NewStackContext>().stack.is_none());

        let mut tab_query = app.world_mut().query::<&Space>();
        assert_eq!(tab_query.iter(app.world()).count(), 1);

        let tabs = {
            let mut tab_q = app
                .world_mut()
                .query_filtered::<(Entity, &PageMetadata, &Children), With<Stack>>();
            tab_q
                .iter(app.world())
                .map(|(entity, meta, children)| {
                    let has_spaces_view = children
                        .iter()
                        .any(|child| app.world().get::<SpacesView>(child).is_some());
                    (entity, meta.url.clone(), has_spaces_view)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].1, SPACES_WEBVIEW_URL);
        assert!(tabs[0].2);
        let focus = app.world().resource::<vmux_layout::stack::FocusedStack>();
        assert_ne!(focus.tab, Some(old_tab));
        assert!(focus.tab.is_some());
        assert!(focus.pane.is_some());
        assert!(focus.stack.is_some());
    }

    #[test]
    fn ipc_new_space_focuses_spaces_page_layout_in_same_update() {
        let _home = HomeEnvGuard::use_temp_home("ipc-new-space-focuses-layout");
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![default_space_record()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(bevy_cef::prelude::JsEmitEventPlugin::<SpaceCommandEvent>::default());
        app.add_plugins(vmux_layout::stack::StackPlugin);
        app.add_message::<vmux_layout::LayoutSpawnRequest>();
        app.add_observer(on_space_command);
        app.init_resource::<vmux_layout::pane::PendingCursorWarp>();
        app.init_resource::<bevy_cef::prelude::IpcEventRawBuffer>();
        app.insert_resource(ActiveSpace {
            record: default_space_record(),
        });
        app.insert_resource(test_settings());
        app.init_resource::<NewStackContext>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let old_tab = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt::now(), ChildOf(main)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(old_tab)))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::now(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((SpacesView, ChildOf(tab))).id();
        let payload = serde_json::to_string(&SpaceCommandEvent {
            command: "new".to_string(),
            space_id: None,
            name: Some("Client A".to_string()),
        })
        .unwrap();
        app.world_mut()
            .resource_mut::<bevy_cef::prelude::IpcEventRawBuffer>()
            .0
            .push(bevy_cef_core::prelude::IpcEventRaw { webview, payload });

        app.update();

        let focus = app.world().resource::<vmux_layout::stack::FocusedStack>();
        assert!(focus.tab.is_some());
        assert!(focus.pane.is_some());
        assert!(focus.stack.is_some());
    }
}
