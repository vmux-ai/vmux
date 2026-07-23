use std::path::PathBuf;

use bevy::{ecs::message::MessageReader, prelude::*, window::PrimaryWindow};
use bevy_cef::prelude::*;
use vmux_core::page::PageReady;
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_layout::stack::Stack;
use vmux_layout::warm_page::WarmPagePlugin;
use vmux_layout::{TabLayoutSpawnContent, TabLayoutSpawnRequest};

use crate::event::{
    SPACES_LIST_EVENT, SPACES_PAGE_URL, SpaceCommandEvent, SpaceRow, SpacesListEvent,
};
use crate::spaces::{ActiveSpace, Spaces};

#[derive(Message, Clone)]
pub struct SaveSpaceRequest {
    pub path: PathBuf,
}

/// A space CRUD request from a non-web source (e.g. the agent/MCP). Relayed into
/// the same `SpaceCommandEvent` flow the web spaces page uses.
#[derive(Message, Clone)]
pub struct SpaceCommandRequest {
    pub command: String,
    pub space_id: Option<String>,
    pub name: Option<String>,
}

/// Wires the spaces domain: space commands, active-space syncing, configured startup
/// resolution, and the spaces list webview.
pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, "spaces");
        app.init_resource::<ActiveSpace>()
            .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
            .add_message::<SaveSpaceRequest>()
            .add_message::<SpaceCommandRequest>()
            .add_systems(Update, relay_space_command_requests)
            .add_systems(
                Update,
                (sync_active_space_record, update_effective_startup_url).chain(),
            )
            .add_systems(
                Update,
                update_effective_startup_dir
                    .in_set(vmux_layout::settings::EffectiveStartupDirSet)
                    .before(vmux_command::ReadAppCommands),
            )
            .add_systems(Update, sync_space_name_to_id)
            .add_systems(
                Startup,
                update_effective_startup_url
                    .after(vmux_setting::SettingsLoadSet)
                    .before(vmux_layout::LayoutStartupSet::Post),
            )
            .add_systems(
                Startup,
                update_effective_startup_dir
                    .after(vmux_setting::SettingsLoadSet)
                    .after(vmux_layout::LayoutStartupSet::Persistence)
                    .before(vmux_layout::LayoutStartupSet::DefaultTab),
            )
            .add_message::<vmux_core::page::SpacesPageSpawnRequest>()
            .add_systems(
                Update,
                respond_spaces_spawn.in_set(vmux_command::ReadAppCommands),
            )
            .add_plugins(WarmPagePlugin::<Spaces>::default())
            .add_plugins(BinEventEmitterPlugin::<(SpaceCommandEvent,)>::for_hosts(&[
                "spaces", "layout",
            ]))
            .add_observer(on_space_command)
            .add_observer(reset_spaces_sent_marker_on_page_ready)
            .add_systems(
                Update,
                handle_open_in_new_space.in_set(vmux_command::ReadAppCommands),
            )
            .add_systems(Update, broadcast_spaces_to_views)
            .add_systems(
                Update,
                crate::snapshot_updater::update_spaces_snapshot
                    .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
            );
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

fn update_effective_startup_dir(
    settings: Option<Res<vmux_setting::AppSettings>>,
    spaces: Query<
        (
            Entity,
            Ref<vmux_layout::space::SpaceId>,
            Has<vmux_core::Active>,
        ),
        With<vmux_layout::space::Space>,
    >,
    mut effective: ResMut<vmux_layout::settings::EffectiveStartupDir>,
) {
    let mut fallback = None;
    let mut selected = None;
    for (entity, id, is_active) in &spaces {
        fallback.get_or_insert((entity, id));
        if is_active {
            selected = Some((entity, id));
            break;
        }
    }
    let Some((entity, id)) = selected.or(fallback) else {
        if effective.0.is_some() {
            effective.0 = None;
        }
        return;
    };
    if !settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed())
        && !id.is_changed()
        && effective.0.as_ref().map(|(current, _)| *current) == Some(entity)
        && effective
            .0
            .as_ref()
            .is_some_and(|(_, current)| current.as_ref().is_none_or(|path| path.is_dir()))
    {
        return;
    }
    let path = settings
        .as_deref()
        .and_then(|settings| vmux_setting::resolve_startup_dir(settings, &id.0));
    let next = (entity, path);
    if effective.0.as_ref() != Some(&next) {
        effective.0 = Some(next);
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

fn sync_active_space_record(
    tagged: Query<
        (&vmux_layout::space::SpaceId, &Name),
        (With<vmux_layout::space::Space>, With<vmux_core::Active>),
    >,
    mut active: ResMut<ActiveSpace>,
) {
    if let Some((id, name)) = tagged.iter().next()
        && (active.record.id != id.0 || active.record.name != name.as_str())
    {
        active.record.id = id.0.clone();
        active.record.name = name.to_string();
    }
}

type SpaceListQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static vmux_layout::space::SpaceId,
        &'static Name,
        Has<vmux_core::Active>,
        Option<&'static vmux_core::Order>,
        Option<&'static Children>,
    ),
    With<vmux_layout::space::Space>,
>;

fn display_dir(path: &std::path::Path) -> String {
    if let Some(home) = std::env::home_dir()
        && let Ok(rel) = path.strip_prefix(&home)
    {
        return format!("~/{}", rel.to_string_lossy());
    }
    path.to_string_lossy().to_string()
}

fn space_rows_from_world(
    spaces: &SpaceListQuery,
    tab_q: &Query<(), With<vmux_layout::tab::Tab>>,
    settings: Option<&vmux_setting::AppSettings>,
) -> Vec<SpaceRow> {
    let mut rows: Vec<(u32, SpaceRow)> = spaces
        .iter()
        .map(|(sid, name, is_active, order, children)| {
            let tab_count = children
                .map(|c| c.iter().filter(|e| tab_q.contains(*e)).count())
                .unwrap_or(0) as u32;
            let startup_dir = settings
                .and_then(|s| vmux_setting::resolve_startup_dir(s, &sid.0))
                .map(|path| display_dir(&path))
                .unwrap_or_default();
            (
                order.map(|o| o.0).unwrap_or(u32::MAX),
                SpaceRow {
                    id: sid.0.clone(),
                    name: name.to_string(),
                    profile: crate::model::bootstrap_profile_name(),
                    is_active,
                    tab_count,
                    startup_dir,
                },
            )
        })
        .collect();
    rows.sort_by_key(|(order, _)| *order);
    rows.into_iter().map(|(_, row)| row).collect()
}

fn broadcast_spaces_to_views(
    spaces: SpaceListQuery,
    tab_q: Query<(), With<vmux_layout::tab::Tab>>,
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
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut last_body: Local<String>,
    mut commands: Commands,
) {
    let pending_total = pending_spaces.iter().count() + pending_cef.iter().count();
    let sent_total = sent_spaces.iter().count() + sent_cef.iter().count();
    if pending_total == 0 && sent_total == 0 {
        return;
    }
    let payload = SpacesListEvent {
        spaces: space_rows_from_world(&spaces, &tab_q, settings.as_deref()),
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

fn relay_space_command_requests(
    mut reader: MessageReader<SpaceCommandRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        commands.trigger(BinReceive {
            webview: Entity::PLACEHOLDER,
            payload: SpaceCommandEvent {
                command: request.command.clone(),
                space_id: request.space_id.clone(),
                name: request.name.clone(),
            },
        });
    }
}

type SpaceQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static vmux_layout::space::SpaceId,
        Has<vmux_core::Active>,
        Option<&'static vmux_core::Order>,
    ),
    With<vmux_layout::space::Space>,
>;

type SpaceTabQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static vmux_layout::space::SpaceId,
        &'static vmux_history::LastActivatedAt,
    ),
    With<vmux_layout::tab::Tab>,
>;

fn bump_space_tab(tabs: &SpaceTabQuery, space_id: &str, commands: &mut Commands) {
    if let Some((tab, _, _)) = tabs
        .iter()
        .filter(|(_, sid, _)| sid.0 == space_id)
        .max_by_key(|(_, _, ts)| ts.0)
    {
        commands
            .entity(tab)
            .insert(vmux_history::LastActivatedAt::now());
    }
}

fn deactivate_all_spaces(spaces: &SpaceQuery, commands: &mut Commands) {
    for (entity, _, is_active, _) in spaces.iter() {
        if is_active {
            commands.entity(entity).remove::<vmux_core::Active>();
        }
    }
}

fn sync_space_name_to_id(
    mut spaces: Query<
        (&vmux_layout::space::SpaceId, &mut Name),
        (
            With<vmux_layout::space::Space>,
            Changed<vmux_layout::space::SpaceId>,
        ),
    >,
) {
    for (id, mut name) in &mut spaces {
        if name.as_str() != id.0 {
            *name = Name::new(id.0.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_space_command(
    trigger: On<BinReceive<SpaceCommandEvent>>,
    spaces: SpaceQuery,
    tabs: SpaceTabQuery,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    focus: Option<Res<vmux_layout::stack::FocusedStack>>,
    mut spawn_requests: Option<MessageWriter<PageOpenRequest>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut active_id: ResMut<vmux_layout::space::ActiveSpaceId>,
    stack_q: Query<(Entity, &PageMetadata), With<Stack>>,
    child_of_q: Query<&ChildOf>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
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

    if evt.command == "rename" {
        let Some(id) = evt.space_id.as_deref() else {
            return;
        };
        let Some(name) = evt.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else {
            return;
        };
        let Some((entity, _, is_active, _)) = spaces.iter().find(|(_, sid, _, _)| sid.0 == id)
        else {
            return;
        };
        let existing: std::collections::HashSet<String> = spaces
            .iter()
            .filter(|(_, sid, _, _)| sid.0 != id)
            .map(|(_, sid, _, _)| sid.0.clone())
            .collect();
        let new_id = crate::model::unique_space_id_among(&existing, name);
        commands.entity(entity).insert(Name::new(new_id.clone()));
        if new_id != id {
            commands
                .entity(entity)
                .insert(vmux_layout::space::SpaceId(new_id.clone()));
            for (tab, sid, _) in tabs.iter() {
                if sid.0 == id {
                    commands
                        .entity(tab)
                        .insert(vmux_layout::space::SpaceId(new_id.clone()));
                }
            }
            if is_active {
                active_id.0 = Some(new_id.clone());
            }
        }
        return;
    }

    if evt.command == "delete" {
        let Some(id) = evt.space_id.as_deref() else {
            return;
        };
        if spaces.iter().count() <= 1 {
            return;
        }
        let Some((entity, _, was_active, _)) = spaces.iter().find(|(_, sid, _, _)| sid.0 == id)
        else {
            return;
        };
        commands.entity(entity).despawn();
        for (tab, sid, _) in tabs.iter() {
            if sid.0 == id {
                commands.entity(tab).despawn();
            }
        }
        if was_active
            && let Some((target_entity, target_id)) = spaces
                .iter()
                .filter(|(_, sid, _, _)| sid.0 != id)
                .min_by_key(|(_, _, _, order)| order.map(|o| o.0).unwrap_or(u32::MAX))
                .map(|(entity, sid, _, _)| (entity, sid.0.clone()))
        {
            commands
                .entity(target_entity)
                .insert((vmux_core::Active, vmux_history::LastActivatedAt::now()));
            active_id.0 = Some(target_id.clone());
            bump_space_tab(&tabs, &target_id, &mut commands);
        }
        return;
    }

    match evt.command.as_str() {
        "attach" => {
            let Some(id) = evt.space_id.as_deref() else {
                return;
            };
            let Some((entity, _, is_active, _)) = spaces.iter().find(|(_, sid, _, _)| sid.0 == id)
            else {
                return;
            };
            if is_active {
                return;
            }
            deactivate_all_spaces(&spaces, &mut commands);
            commands
                .entity(entity)
                .insert((vmux_core::Active, vmux_history::LastActivatedAt::now()));
            active_id.0 = Some(id.to_string());
            bump_space_tab(&tabs, id, &mut commands);
        }
        "new" => {
            let count = spaces.iter().count();
            let name = evt
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| format!("Space {}", count + 1));
            let existing: std::collections::HashSet<String> =
                spaces.iter().map(|(_, sid, _, _)| sid.0.clone()).collect();
            let id = crate::model::unique_space_id_among(&existing, &name);
            let order = spaces
                .iter()
                .filter_map(|(_, _, _, order)| order.map(|o| o.0))
                .max()
                .map(|max| max + 1)
                .unwrap_or(0);
            let Ok(main) = main_q.single() else { return };
            deactivate_all_spaces(&spaces, &mut commands);
            let space = commands
                .spawn((
                    vmux_layout::space::Space,
                    vmux_layout::space::SpaceId(id.clone()),
                    Name::new(id.clone()),
                    vmux_core::Order(order),
                    vmux_core::Active,
                    vmux_history::LastActivatedAt::now(),
                    vmux_layout::space::space_view_bundle(),
                    ChildOf(main),
                ))
                .id();
            active_id.0 = Some(id.clone());
            let startup_dir = settings
                .as_deref()
                .and_then(|settings| vmux_setting::resolve_startup_dir(settings, &id));
            layout_requests.write(TabLayoutSpawnRequest {
                space,
                primary_window: *primary_window,
                name: None,
                startup_dir,
                content: TabLayoutSpawnContent::Url(SPACES_PAGE_URL.to_string()),
                clear_pending_stack: true,
                focus: true,
                replaces: None,
            });
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_open_in_new_space(
    mut reader: MessageReader<vmux_command::AppCommand>,
    spaces: SpaceQuery,
    main_q: Query<Entity, With<vmux_layout::window::Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    effective_startup_url: Option<Res<vmux_layout::settings::EffectiveStartupUrl>>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut active_id: ResMut<vmux_layout::space::ActiveSpaceId>,
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

        let count = spaces.iter().count();
        let name = format!("Space {}", count + 1);
        let existing: std::collections::HashSet<String> =
            spaces.iter().map(|(_, sid, _, _)| sid.0.clone()).collect();
        let id = crate::model::unique_space_id_among(&existing, &name);
        let order = spaces
            .iter()
            .filter_map(|(_, _, _, order)| order.map(|o| o.0))
            .max()
            .map(|max| max + 1)
            .unwrap_or(0);
        let Ok(main) = main_q.single() else { continue };
        deactivate_all_spaces(&spaces, &mut commands);
        let space = commands
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId(id.clone()),
                Name::new(id.clone()),
                vmux_core::Order(order),
                vmux_core::Active,
                vmux_history::LastActivatedAt::now(),
                vmux_layout::space::space_view_bundle(),
                ChildOf(main),
            ))
            .id();
        active_id.0 = Some(id.clone());
        let startup_dir = settings
            .as_deref()
            .and_then(|settings| vmux_setting::resolve_startup_dir(settings, &id));
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
            space,
            primary_window: *primary_window,
            name: None,
            startup_dir,
            content,
            clear_pending_stack: true,
            focus: true,
            replaces: None,
        });
    }
}

fn respond_spaces_spawn(
    mut reader: MessageReader<vmux_core::page::SpacesPageSpawnRequest>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    for req in reader.read() {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(req.target_stack),
            url: SPACES_PAGE_URL.to_string(),
            request_id: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SpaceRecord, bootstrap_profile_name};
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    fn work_space_record() -> SpaceRecord {
        SpaceRecord {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: bootstrap_profile_name(),
        }
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
    fn legacy_tab_without_startup_dir_is_not_migrated() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "work".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(first.path().to_string_lossy().into_owned()),
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(ActiveSpace {
                record: work_space_record(),
            });
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("work".into()),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((vmux_layout::tab::Tab::default(), ChildOf(space)))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_layout::tab::Tab>(tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            None
        );
        app.world_mut()
            .resource_mut::<vmux_setting::AppSettings>()
            .spaces
            .get_mut("work")
            .unwrap()
            .startup_dir = Some(second.path().to_string_lossy().into_owned());

        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_layout::tab::Tab>(tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            None
        );
    }

    #[test]
    fn effective_startup_dir_captures_active_space_entity_and_path() {
        let active_dir = tempfile::tempdir().unwrap();
        let inactive_dir = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "active".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(active_dir.path().to_string_lossy().into_owned()),
            },
        );
        settings.spaces.insert(
            "inactive".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(inactive_dir.path().to_string_lossy().into_owned()),
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
            .add_systems(Update, update_effective_startup_dir);
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("inactive".into()),
        ));
        let active = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("active".into()),
                vmux_core::Active,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_layout::settings::EffectiveStartupDir>()
                .0,
            Some((active, Some(active_dir.path().to_path_buf())))
        );
    }

    #[test]
    fn missing_startup_dir_remains_unset() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
            .add_systems(Update, update_effective_startup_dir);
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("work".into()),
                vmux_core::Active,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_layout::settings::EffectiveStartupDir>()
                .0,
            Some((space, None))
        );
    }

    #[test]
    fn unset_startup_dir_is_unchanged_without_relevant_updates() {
        #[derive(Resource, Default)]
        struct ChangeCount(u32);

        fn count_changes(
            effective: Res<vmux_layout::settings::EffectiveStartupDir>,
            mut count: ResMut<ChangeCount>,
        ) {
            if effective.is_changed() {
                count.0 += 1;
            }
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
            .init_resource::<ChangeCount>()
            .add_systems(
                Update,
                (
                    update_effective_startup_dir,
                    count_changes.after(update_effective_startup_dir),
                ),
            );
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("work".into()),
            vmux_core::Active,
        ));

        app.update();
        app.update();

        assert_eq!(app.world().resource::<ChangeCount>().0, 1);
    }

    #[test]
    fn effective_startup_dir_is_unchanged_without_relevant_updates() {
        #[derive(Resource, Default)]
        struct ChangeCount(u32);

        fn count_changes(
            effective: Res<vmux_layout::settings::EffectiveStartupDir>,
            mut count: ResMut<ChangeCount>,
        ) {
            if effective.is_changed() {
                count.0 += 1;
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "work".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.path().to_string_lossy().into_owned()),
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
            .init_resource::<ChangeCount>()
            .add_systems(
                Update,
                (
                    update_effective_startup_dir,
                    count_changes.after(update_effective_startup_dir),
                ),
            );
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("work".into()),
            vmux_core::Active,
        ));

        app.update();
        app.update();

        assert_eq!(app.world().resource::<ChangeCount>().0, 1);
    }

    #[test]
    fn effective_startup_dir_re_resolves_when_current_directory_disappears() {
        let primary = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let primary_path = primary.path().to_path_buf();
        let mut settings = test_settings();
        settings.terminal = Some(vmux_setting::TerminalSettings {
            startup_dir: Some(fallback.path().to_string_lossy().into_owned()),
            ..Default::default()
        });
        settings.spaces.insert(
            "work".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(primary_path.to_string_lossy().into_owned()),
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
            .add_systems(Update, update_effective_startup_dir);
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("work".into()),
                vmux_core::Active,
            ))
            .id();

        app.update();
        assert_eq!(
            app.world()
                .resource::<vmux_layout::settings::EffectiveStartupDir>()
                .0,
            Some((space, Some(primary_path)))
        );

        primary.close().unwrap();
        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_layout::settings::EffectiveStartupDir>()
                .0,
            Some((space, Some(fallback.path().to_path_buf())))
        );
    }

    #[test]
    fn rename_reslugs_space_id_and_retags_tabs() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<TabLayoutSpawnRequest>()
            .init_resource::<vmux_layout::space::ActiveSpaceId>()
            .add_observer(on_space_command);
        app.world_mut().spawn(bevy::window::PrimaryWindow);
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("rename-src-test".to_string()),
                Name::new("rename-src-test"),
                vmux_core::Active,
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                vmux_layout::tab::Tab::default(),
                vmux_layout::space::SpaceId("rename-src-test".to_string()),
                vmux_history::LastActivatedAt::now(),
            ))
            .id();

        app.world_mut().trigger(BinReceive {
            webview: Entity::PLACEHOLDER,
            payload: SpaceCommandEvent {
                command: "rename".to_string(),
                space_id: Some("rename-src-test".to_string()),
                name: Some("Vmux Ai/Vmux".to_string()),
            },
        });
        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_layout::space::SpaceId>(space)
                .map(|s| s.0.clone()),
            Some("vmux-ai/vmux".to_string())
        );
        assert_eq!(
            app.world().get::<Name>(space).map(|n| n.to_string()),
            Some("vmux-ai/vmux".to_string())
        );
        assert_eq!(
            app.world()
                .get::<vmux_layout::space::SpaceId>(tab)
                .map(|s| s.0.clone()),
            Some("vmux-ai/vmux".to_string())
        );
        assert_eq!(
            app.world()
                .resource::<vmux_layout::space::ActiveSpaceId>()
                .0
                .as_deref(),
            Some("vmux-ai/vmux")
        );
    }
}
