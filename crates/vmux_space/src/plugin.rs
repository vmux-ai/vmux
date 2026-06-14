use std::path::{Path, PathBuf};

use bevy::{ecs::message::MessageReader, prelude::*, window::PrimaryWindow};
use bevy_cef::prelude::*;
use moonshine_save::prelude::TriggerLoad;
use vmux_core::page::PageReady;
use vmux_core::profile;
use vmux_core::{
    PageMetadata, PageOpenError, PageOpenHandled, PageOpenRequest, PageOpenSet, PageOpenTarget,
    PageOpenTask,
};
use vmux_layout::stack::Stack;
use vmux_layout::{TabLayoutSpawnContent, TabLayoutSpawnRequest};

use crate::event::{
    SPACES_LIST_EVENT, SPACES_PAGE_URL, SpaceCommandEvent, SpaceRow, SpacesListEvent,
};
use crate::model::{
    SpaceRecord, SpaceRegistry, bootstrap_space_record, registry_path, space_layout_path_for,
    unique_space_id,
};
use crate::spaces::{ActiveSpace, Spaces, read_space_registry_from, space_profile_bundle};

#[derive(Message, Clone)]
pub struct SaveSpaceRequest {
    pub path: PathBuf,
}

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.init_resource::<ActiveSpace>()
            .add_message::<SaveSpaceRequest>()
            .add_systems(Startup, ensure_space_registry)
            .add_systems(
                Startup,
                update_effective_startup_url
                    .after(vmux_setting::SettingsLoadSet)
                    .before(vmux_layout::LayoutStartupSet::Post),
            )
            .add_systems(Update, update_effective_startup_url)
            .add_message::<vmux_core::page::SpacesPageSpawnRequest>()
            .add_systems(
                Update,
                respond_spaces_spawn.in_set(vmux_command::ReadAppCommands),
            )
            .add_systems(
                Update,
                handle_spaces_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_plugins(BinEventEmitterPlugin::<(SpaceCommandEvent,)>::default())
            .add_observer(on_space_command)
            .add_observer(reset_spaces_sent_marker_on_page_ready)
            .add_systems(
                Update,
                handle_open_in_new_space.in_set(vmux_command::ReadAppCommands),
            )
            .add_systems(
                Update,
                (apply_pending_space_switch, broadcast_spaces_to_views).chain(),
            )
            .add_systems(
                Update,
                crate::snapshot_updater::update_spaces_snapshot
                    .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
            )
            .add_systems(
                Startup,
                reconcile_space_overrides.after(vmux_setting::SettingsLoadSet),
            )
            .add_systems(Update, reconcile_space_overrides);
    }
}

fn update_effective_startup_url(
    settings: Option<Res<vmux_setting::AppSettings>>,
    active: Option<Res<ActiveSpace>>,
    mut effective: ResMut<vmux_layout::settings::EffectiveStartupUrl>,
) {
    let (Some(settings), Some(active)) = (settings, active) else {
        return;
    };
    if settings.is_changed() || active.is_changed() || effective.0.is_empty() {
        effective.0 = vmux_setting::resolve_startup_url(&settings, &active.record.id);
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_spaces_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if task.url != SPACES_PAGE_URL {
            continue;
        }
        clear_stack_children(task.stack, &children_q, &mut commands);
        commands.entity(task.stack).insert(PageMetadata {
            title: "Spaces".to_string(),
            url: SPACES_PAGE_URL.to_string(),
            favicon_url: String::new(),
            bg_color: None,
        });
        commands.spawn((
            Spaces::new(&mut meshes, &mut webview_mt),
            ChildOf(task.stack),
        ));
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

#[derive(Component)]
struct SpacesListSent;

fn reset_spaces_sent_marker_on_page_ready(
    trigger: On<BinReceive<PageReady>>,
    spaces_views: Query<(), With<Spaces>>,
    cef_views: Query<(), With<vmux_layout::LayoutCef>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if spaces_views.get(entity).is_err() && cef_views.get(entity).is_err() {
        return;
    }
    commands.entity(entity).remove::<SpacesListSent>();
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

fn ensure_space_registry() {
    let root = profile::shared_data_dir();
    let path = registry_path(&root);
    if path.exists() {
        return;
    }
    write_space_registry_to(&root, &read_space_registry_from(&root));
}

fn delete_space_record(registry: &mut SpaceRegistry, id: &str) -> Option<SpaceRecord> {
    if registry.spaces.len() <= 1 {
        return None;
    }
    let idx = registry.spaces.iter().position(|space| space.id == id)?;
    Some(registry.spaces.remove(idx))
}

fn delete_space_layout(root: &Path, record: &SpaceRecord) {
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
    pending_spaces: Query<Entity, (With<Spaces>, With<PageReady>, Without<SpacesListSent>)>,
    sent_spaces: Query<Entity, (With<Spaces>, With<PageReady>, With<SpacesListSent>)>,
    pending_cef: Query<
        Entity,
        (
            With<vmux_layout::LayoutCef>,
            With<PageReady>,
            Without<SpacesListSent>,
        ),
    >,
    sent_cef: Query<
        Entity,
        (
            With<vmux_layout::LayoutCef>,
            With<PageReady>,
            With<SpacesListSent>,
        ),
    >,
    browsers: NonSend<Browsers>,
    tabs: Query<(), With<Stack>>,
    mut last_body: Local<String>,
    mut commands: Commands,
) {
    let pending_total = pending_spaces.iter().count() + pending_cef.iter().count();
    let sent_total = sent_spaces.iter().count() + sent_cef.iter().count();
    if pending_total == 0 && sent_total == 0 {
        return;
    }
    let registry = read_space_registry_from(&profile::shared_data_dir());
    let payload = SpacesListEvent {
        spaces: space_rows(&active, &registry, tabs.iter().count()),
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    let body_changed = body != *last_body;
    for entity in pending_spaces.iter().chain(pending_cef.iter()) {
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
        for entity in sent_spaces.iter().chain(sent_cef.iter()) {
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
            With<vmux_layout::space::Space>,
            With<vmux_layout::tab::Tab>,
            With<vmux_history::Visit>,
        )>,
    >,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
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
        commands.spawn(space_profile_bundle(&record));
        let Ok(main) = main_q.single() else { return };
        layout_requests.write(TabLayoutSpawnRequest {
            main,
            primary_window: *primary_window,
            name: None,
            content: TabLayoutSpawnContent::StartupUrlOrPrompt,
            clear_pending_stack: false,
            focus: true,
        });
    }
}

fn on_space_command(
    trigger: On<BinReceive<SpaceCommandEvent>>,
    mut active: ResMut<ActiveSpace>,
    space_entities: Query<
        Entity,
        Or<(
            With<vmux_layout::space::Space>,
            With<vmux_layout::tab::Tab>,
            With<vmux_history::Visit>,
        )>,
    >,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    focus: Option<ResMut<vmux_layout::stack::FocusedStack>>,
    mut spawn_requests: Option<MessageWriter<PageOpenRequest>>,
    mut save_requests: MessageWriter<SaveSpaceRequest>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    stack_q: Query<(Entity, &PageMetadata), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
) {
    let root = profile::shared_data_dir();
    let mut registry = read_space_registry_from(&root);
    let evt = &trigger.event().payload;
    if evt.command == "open_page" {
        if let Some((existing, _)) = stack_q.iter().find(|(_, meta)| meta.url == SPACES_PAGE_URL) {
            vmux_core::focus_pane_entity(existing, &mut commands, &child_of_q);
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
        spawn_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: SPACES_PAGE_URL.to_string(),
            request_id: None,
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
            .unwrap_or_else(bootstrap_space_record);
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
    save_requests.write(SaveSpaceRequest { path: current_path });
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
        commands.spawn(space_profile_bundle(&active.record));
        let Ok(main) = main_q.single() else { return };
        layout_requests.write(TabLayoutSpawnRequest {
            main,
            primary_window: *primary_window,
            name: None,
            content: if open_spaces_page {
                TabLayoutSpawnContent::Url(SPACES_PAGE_URL.to_string())
            } else {
                TabLayoutSpawnContent::StartupUrlOrPrompt
            },
            clear_pending_stack: true,
            focus: true,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_open_in_new_space(
    mut reader: MessageReader<vmux_command::AppCommand>,
    mut active: ResMut<ActiveSpace>,
    space_entities: Query<
        Entity,
        Or<(
            With<vmux_layout::space::Space>,
            With<vmux_layout::tab::Tab>,
            With<vmux_history::Visit>,
        )>,
    >,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    effective_startup_url: Option<Res<vmux_layout::settings::EffectiveStartupUrl>>,
    mut save_requests: MessageWriter<SaveSpaceRequest>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let vmux_command::AppCommand::Browser(vmux_command::BrowserCommand::Open(
            vmux_command::open::OpenCommand::InNewSpace { url },
        )) = cmd
        else {
            continue;
        };

        let root = profile::shared_data_dir();
        let mut registry = read_space_registry_from(&root);
        let name = format!("Space {}", registry.spaces.len() + 1);
        let record = SpaceRecord {
            id: unique_space_id(&registry.spaces, &name),
            name,
            profile: active.record.profile.clone(),
        };
        registry.spaces.push(record.clone());
        write_space_registry_to(&root, &registry);
        save_requests.write(SaveSpaceRequest {
            path: active.layout_path(),
        });
        active.record = record;
        for entity in &space_entities {
            commands.entity(entity).try_despawn();
        }
        let Ok(main) = main_q.single() else { continue };
        commands.spawn(space_profile_bundle(&active.record));
        let content = url
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| TabLayoutSpawnContent::Url(url.to_string()))
            .or_else(|| {
                effective_startup_url
                    .as_deref()
                    .map(|startup| startup.0.as_str())
                    .filter(|startup| !startup.is_empty())
                    .map(|startup| TabLayoutSpawnContent::Url(startup.to_string()))
            })
            .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt);
        layout_requests.write(TabLayoutSpawnRequest {
            main,
            primary_window: *primary_window,
            name: None,
            content,
            clear_pending_stack: true,
            focus: true,
        });
    }
}

fn respond_spaces_spawn(
    mut reader: MessageReader<vmux_core::page::SpacesPageSpawnRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for req in reader.read() {
        let entity = commands
            .spawn(crate::spaces::Spaces::new(&mut meshes, &mut webview_mt))
            .id();
        commands.entity(entity).insert(ChildOf(req.target_stack));
    }
}

pub(crate) fn seed_missing_overrides(
    spaces: &mut std::collections::BTreeMap<String, vmux_setting::SpaceOverrides>,
    registry: &SpaceRegistry,
) -> bool {
    let mut added = false;
    for space in &registry.spaces {
        if !spaces.contains_key(&space.id) {
            spaces.insert(space.id.clone(), vmux_setting::SpaceOverrides::default());
            added = true;
        }
    }
    added
}

fn reconcile_space_overrides(
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    active: Option<Res<ActiveSpace>>,
    mut writes: MessageWriter<vmux_setting::SettingsWriteRequest>,
) {
    let (Some(mut settings), Some(active)) = (settings, active) else {
        return;
    };
    if !(settings.is_changed() || active.is_changed()) {
        return;
    }
    let registry = read_space_registry_from(&profile::shared_data_dir());
    if seed_missing_overrides(&mut settings.spaces, &registry) {
        match vmux_setting::serialize_settings_to_ron(&settings) {
            Ok(ron_bytes) => {
                writes.write(vmux_setting::SettingsWriteRequest { ron_bytes });
            }
            Err(e) => bevy::log::warn!("reconcile_space_overrides: serialize failed: {e}"),
        }
    }
}

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BOOTSTRAP_PROFILE_NAME;
    use vmux_history::LastActivatedAt;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_layout::{NewStackContext, pane::Pane, stack::Stack, tab::Tab, window::Main};
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = super::ENV_LOCK
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
                startup_dir: None,
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
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
        }
    }

    fn work_space_record() -> SpaceRecord {
        SpaceRecord {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: BOOTSTRAP_PROFILE_NAME.to_string(),
        }
    }

    fn resolve_stack_page_open_requests(
        mut reader: MessageReader<PageOpenRequest>,
        mut commands: Commands,
    ) {
        for request in reader.read() {
            if let PageOpenTarget::Stack(stack) = request.target {
                commands.spawn(PageOpenTask {
                    id: vmux_core::PageOpenId::new(),
                    stack,
                    url: request.url.clone(),
                    request_id: request.request_id,
                });
            }
        }
    }

    #[test]
    fn rows_mark_active_space_and_profile() {
        let active = ActiveSpace {
            record: SpaceRecord {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: BOOTSTRAP_PROFILE_NAME.to_string(),
            },
        };
        let registry = SpaceRegistry {
            spaces: vec![bootstrap_space_record(), active.record.clone()],
        };
        let rows = space_rows(&active, &registry, 4);
        assert!(!rows[0].is_active);
        assert!(rows[1].is_active);
        assert_eq!(rows[1].profile, BOOTSTRAP_PROFILE_NAME);
        assert_eq!(rows[1].tab_count, 4);
    }

    #[test]
    fn registers_spaces_host_before_cef_embedded_hosts_are_read() {
        let mut app = App::new();
        app.add_plugins(SpacePlugin);
        let mut query = app.world_mut().query::<&vmux_core::page::PageManifest>();
        let hosts = bevy_cef_core::prelude::CefEmbeddedHosts(
            query
                .iter(app.world())
                .map(vmux_core::page::PageManifest::embedded_host)
                .collect(),
        );

        let entry = hosts.entry_for_host("spaces").unwrap();
        assert_eq!(entry.default_document, "spaces/index.html");
    }

    #[test]
    fn delete_space_removes_named_space_from_registry() {
        let mut registry = SpaceRegistry {
            spaces: vec![
                bootstrap_space_record(),
                SpaceRecord {
                    id: "work".to_string(),
                    name: "Work".to_string(),
                    profile: BOOTSTRAP_PROFILE_NAME.to_string(),
                },
            ],
        };

        let deleted = delete_space_record(&mut registry, "work").unwrap();

        assert_eq!(deleted.id, "work");
        assert_eq!(registry.spaces, vec![bootstrap_space_record()]);
    }

    #[test]
    fn delete_space_keeps_last_space() {
        let mut registry = SpaceRegistry {
            spaces: vec![bootstrap_space_record()],
        };

        let deleted = delete_space_record(&mut registry, "space-1");

        assert!(deleted.is_none());
        assert_eq!(registry.spaces, vec![bootstrap_space_record()]);
    }

    #[test]
    fn delete_active_space_defers_current_layout_teardown() {
        let _home = HomeEnvGuard::use_temp_home("delete-active-space-defers-layout");
        let active_record = work_space_record();
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![bootstrap_space_record(), active_record.clone()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SaveSpaceRequest>()
            .add_message::<vmux_layout::TabLayoutSpawnRequest>()
            .add_observer(on_space_command)
            .insert_resource(ActiveSpace {
                record: active_record,
            })
            .insert_resource(test_settings())
            .insert_resource(test_settings().layout)
            .init_resource::<NewStackContext>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_PAGE_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((Spaces, ChildOf(tab))).id();

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
                spaces: vec![bootstrap_space_record(), active_record.clone()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SaveSpaceRequest>()
            .add_message::<vmux_layout::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(on_space_command)
            .add_systems(
                Update,
                (
                    apply_pending_space_switch,
                    vmux_layout::window::spawn_requested_tab_layouts,
                )
                    .chain(),
            )
            .insert_resource(ActiveSpace {
                record: active_record,
            })
            .insert_resource(test_settings())
            .insert_resource(test_settings().layout)
            .init_resource::<NewStackContext>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_PAGE_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((Spaces, ChildOf(tab))).id();

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
            bootstrap_space_record()
        );
        let mut tab_query = app.world_mut().query::<&Tab>();
        assert_eq!(tab_query.iter(app.world()).count(), 1);
    }

    #[test]
    fn new_space_spawns_spaces_page_layout() {
        let _home = HomeEnvGuard::use_temp_home("new-space-spawns-spaces-page");
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![bootstrap_space_record()],
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SaveSpaceRequest>()
            .add_message::<vmux_layout::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(on_space_command)
            .add_systems(
                Update,
                (
                    vmux_layout::window::spawn_requested_tab_layouts,
                    resolve_stack_page_open_requests,
                    handle_spaces_page_open,
                )
                    .chain(),
            )
            .insert_resource(vmux_layout::stack::FocusedStack::default())
            .insert_resource(ActiveSpace {
                record: bootstrap_space_record(),
            })
            .insert_resource(test_settings())
            .insert_resource(test_settings().layout)
            .init_resource::<NewStackContext>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let old_tab = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(old_tab))).id();
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_PAGE_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((Spaces, ChildOf(tab))).id();
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

        let mut tab_query = app.world_mut().query::<&Tab>();
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
                        .any(|child| app.world().get::<Spaces>(child).is_some());
                    (entity, meta.url.clone(), has_spaces_view)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].1, SPACES_PAGE_URL);
        assert!(tabs[0].2);
        let focus = app.world().resource::<vmux_layout::stack::FocusedStack>();
        assert_ne!(focus.tab, Some(old_tab));
        assert!(focus.tab.is_some());
        assert!(focus.pane.is_some());
        assert!(focus.stack.is_some());
    }

    #[test]
    fn effective_startup_url_reflects_active_space_override() {
        let mut settings = test_settings();
        settings.browser.startup_url = "https://global.example".into();
        settings.spaces.insert(
            "work".into(),
            vmux_setting::SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: None,
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
            .insert_resource(ActiveSpace {
                record: work_space_record(),
            })
            .add_systems(Update, update_effective_startup_url);

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_layout::settings::EffectiveStartupUrl>()
                .0,
            "https://work.example"
        );
    }

    #[test]
    fn ipc_new_space_focuses_spaces_page_layout_in_same_update() {
        let _home = HomeEnvGuard::use_temp_home("ipc-new-space-focuses-layout");
        write_space_registry_to(
            &profile::shared_data_dir(),
            &SpaceRegistry {
                spaces: vec![bootstrap_space_record()],
            },
        );
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            bevy_cef::prelude::JsEmitEventPlugin::<SpaceCommandEvent>::default(),
            vmux_layout::stack::StackPlugin,
        ))
        .add_message::<vmux_layout::LayoutSpawnRequest>()
        .add_message::<vmux_layout::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<SaveSpaceRequest>()
        .add_observer(on_space_command)
        .add_systems(Update, vmux_layout::window::spawn_requested_tab_layouts)
        .init_resource::<vmux_layout::pane::PendingCursorWarp>()
        .init_resource::<bevy_cef::prelude::IpcEventRawBuffer>()
        .insert_resource(ActiveSpace {
            record: bootstrap_space_record(),
        })
        .insert_resource(test_settings())
        .insert_resource(test_settings().layout)
        .init_resource::<NewStackContext>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let old_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(main)))
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
                    url: SPACES_PAGE_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();
        let webview = app.world_mut().spawn((Spaces, ChildOf(tab))).id();
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

    #[test]
    fn seed_adds_missing_and_reports_change() {
        let mut spaces = std::collections::BTreeMap::new();
        let registry = crate::model::SpaceRegistry {
            spaces: vec![
                crate::model::SpaceRecord {
                    id: "a".into(),
                    name: "A".into(),
                    profile: "P".into(),
                },
                crate::model::SpaceRecord {
                    id: "b".into(),
                    name: "B".into(),
                    profile: "P".into(),
                },
            ],
        };
        assert!(seed_missing_overrides(&mut spaces, &registry));
        assert_eq!(spaces.len(), 2);
        assert!(spaces.contains_key("a") && spaces.contains_key("b"));
    }

    #[test]
    fn seed_preserves_existing_and_reports_no_change() {
        let mut spaces = std::collections::BTreeMap::new();
        spaces.insert(
            "a".into(),
            vmux_setting::SpaceOverrides {
                startup_url: Some("x".into()),
                startup_dir: None,
            },
        );
        let registry = crate::model::SpaceRegistry {
            spaces: vec![crate::model::SpaceRecord {
                id: "a".into(),
                name: "A".into(),
                profile: "P".into(),
            }],
        };
        assert!(!seed_missing_overrides(&mut spaces, &registry));
        assert_eq!(spaces["a"].startup_url.as_deref(), Some("x"));
    }
}
