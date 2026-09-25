use std::path::PathBuf;

use bevy::{ecs::message::MessageReader, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::page::PageReady;
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_layout::native_open::HostedPagePlugin;
use vmux_layout::stack::Stack;
use vmux_layout::{LayoutUiStateUpdates, TabLayoutSpawnContent, TabLayoutSpawnRequest};

use super::SpacesUiStateUpdates;
use crate::event::{
    ProjectActivateRequest, ProjectForgetRequest, SPACES_PAGE_URL, SpaceAttachRequest,
    SpaceCreateRequest, SpaceDeleteRequest, SpaceOpenPageRequest, SpaceRenameRequest, SpaceRow,
    SpacesListEvent, SpacesUiState,
};
use crate::spaces::{ActiveSpace, SpaceSelection, Spaces, SpacesPageSnapshot};

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::SpacesPage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_plugins(vmux_command::CommandTypePlugin::<OpenRequest>::default())
            .add_plugins(super::SpaceToolPlugin)
            .add_plugins(vmux_layout::LayoutContractPlugin)
            .add_plugins(vmux_core::host::UiStatePlugin::<SpacesUiState>::default())
            .init_resource::<ActiveSpace>()
            .init_resource::<vmux_layout::window::FocusedWindow>()
            .add_message::<SaveSpaceRequest>()
            .add_message::<SpaceAttachRequest>()
            .add_message::<SpaceCreateRequest>()
            .add_message::<SpaceDeleteRequest>()
            .add_message::<SpaceOpenPageRequest>()
            .add_message::<SpaceRenameRequest>()
            .add_systems(
                Update,
                (
                    relay_space_requests::<SpaceAttachRequest>,
                    relay_space_requests::<SpaceCreateRequest>,
                    relay_space_requests::<SpaceDeleteRequest>,
                    relay_space_requests::<SpaceOpenPageRequest>,
                    relay_space_requests::<SpaceRenameRequest>,
                ),
            )
            .add_systems(
                Update,
                (sync_active_space_record, update_effective_startup_url)
                    .chain()
                    .after(vmux_layout::space::CurrentSpaceSet),
            )
            .add_systems(
                Update,
                update_effective_startup_dir
                    .in_set(vmux_layout::settings::EffectiveStartupDirSet)
                    .before(vmux_command::ReadCommandRequests),
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
                respond_spaces_spawn.in_set(vmux_command::ReadCommandRequests),
            )
            .add_plugins((
                HostedPagePlugin::<Spaces>::default(),
                super::key::SpaceKeyPlugin,
                super::project::SpaceProjectPlugin,
                crate::snapshot_updater::SnapshotPlugin,
                UiEventPlugin::<(
                    SpaceAttachRequest,
                    SpaceCreateRequest,
                    SpaceDeleteRequest,
                    SpaceOpenPageRequest,
                    SpaceRenameRequest,
                    ProjectActivateRequest,
                    ProjectForgetRequest,
                    vmux_core::event::ProjectTreeToggle,
                )>::default(),
            ))
            .add_observer(on_space_attach)
            .add_observer(on_space_create)
            .add_observer(on_space_delete)
            .add_observer(on_space_open_page)
            .add_observer(on_space_rename)
            .add_observer(on_project_activate)
            .add_observer(on_project_forget)
            .add_observer(reset_spaces_sent_marker_on_page_ready)
            .add_systems(
                Update,
                handle_open_in_new_space.in_set(vmux_command::ReadCommandRequests),
            )
            .add_systems(Update, broadcast_spaces_to_views);
    }
}

#[derive(Message, Clone)]
pub struct SaveSpaceRequest {
    pub path: PathBuf,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub url: Option<String>,
}

impl vmux_command::CommandRequest for OpenRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        vec![
            vmux_command::CommandDefinition::new(
                "open_in_new_space",
                "Open in New Space",
                "Browser > Open",
            )
            .accelerator("super+shift+n")
            .mcp(vmux_command::CommandMcp::new(
                "Open a page in a brand-new Space (top-level profile). Spaces are the highest-level container and each carries its own profile (cookies, identity, theme). Use only when the user explicitly asks for a new profile, a separate identity, or a top-level workspace switch.",
                vmux_command::InputSchema::object().optional(
                    "url",
                    vmux_command::InputSchema::string().description(
                        "Absolute URL to open in the new Space. If omitted, opens the startup URL.",
                    ),
                ),
            )),
        ]
    }
}

impl TryFrom<&vmux_command::CommandInvocation> for OpenRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "open_in_new_space")
            .then(|| Self {
                url: invocation.argument("url"),
            })
            .ok_or(())
    }
}

fn update_effective_startup_url(
    settings: Option<Res<vmux_setting::AppSettings>>,
    active: Option<Res<ActiveSpace>>,
    mut effective: ResMut<vmux_core::EffectiveStartupUrl>,
) {
    let (Some(settings), Some(active)) = (settings, active) else {
        return;
    };
    if settings.is_changed() || active.is_changed() || effective.0.is_empty() {
        effective.0 = settings.startup_url(&active.record.id);
    }
}

fn update_effective_startup_dir(
    settings: Option<Res<vmux_setting::AppSettings>>,
    current: Query<
        (Entity, Ref<vmux_layout::space::SpaceId>),
        (
            With<vmux_layout::space::Space>,
            With<vmux_layout::space::CurrentSpace>,
        ),
    >,
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
    let selected = current.iter().next().or_else(|| {
        spaces
            .iter()
            .find(|(_, _, is_active)| *is_active)
            .map(|(entity, id, _)| (entity, id))
    });
    let fallback = spaces.iter().next().map(|(entity, id, _)| (entity, id));
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
        .and_then(|settings| settings.startup_dir(&id.0));
    let next = (entity, path);
    if effective.0.as_ref() != Some(&next) {
        effective.0 = Some(next);
    }
}

#[derive(Component)]
struct SpacesListSent;

fn reset_spaces_sent_marker_on_page_ready(
    trigger: On<UiInput<PageReady>>,
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
    current: Query<
        (&vmux_layout::space::SpaceId, &Name),
        (
            With<vmux_layout::space::Space>,
            With<vmux_layout::space::CurrentSpace>,
        ),
    >,
    spaces: Query<
        (&vmux_layout::space::SpaceId, &Name, Has<vmux_core::Active>),
        With<vmux_layout::space::Space>,
    >,
    mut active: ResMut<ActiveSpace>,
) {
    let selected = current.iter().next().or_else(|| {
        spaces
            .iter()
            .find(|(_, _, is_active)| *is_active)
            .map(|(id, name, _)| (id, name))
    });
    if let Some((id, name)) = selected
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
        Entity,
        &'static vmux_layout::space::SpaceId,
        &'static Name,
        Has<vmux_core::Active>,
        Option<&'static vmux_core::Order>,
        Option<&'static Children>,
        &'static ChildOf,
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
    main: Option<Entity>,
) -> Vec<SpaceRow> {
    let profile = crate::model::bootstrap_profile_name();
    let mut rows: Vec<(u32, SpaceRow)> = Vec::new();
    for (_, sid, name, is_active, order, children, parent) in spaces.iter() {
        let local = main.is_none_or(|main| parent.parent() == main);
        let local_tab_count = children
            .map(|c| c.iter().filter(|e| tab_q.contains(*e)).count())
            .unwrap_or(0) as u32;
        if let Some((existing_order, row)) =
            rows.iter_mut().find(|(_, existing)| existing.id == sid.0)
        {
            *existing_order = (*existing_order).min(order.map(|o| o.0).unwrap_or(u32::MAX));
            if local {
                row.is_active = is_active;
                row.tab_count = local_tab_count;
            }
            continue;
        }
        let startup_dir = settings
            .and_then(|s| s.startup_dir(&sid.0))
            .map(|path| display_dir(&path))
            .unwrap_or_default();
        rows.push((
            order.map(|o| o.0).unwrap_or(u32::MAX),
            SpaceRow {
                id: sid.0.clone(),
                name: name.to_string(),
                profile: profile.clone(),
                is_active: local && is_active,
                tab_count: if local { local_tab_count } else { 0 },
                startup_dir,
            },
        ));
    }
    rows.sort_by_key(|(order, _)| *order);
    rows.into_iter().map(|(_, row)| row).collect()
}

fn broadcast_spaces_to_views(
    spaces: SpaceListQuery,
    tab_q: Query<(), With<vmux_layout::tab::Tab>>,
    mut views: Query<
        (
            Entity,
            Option<&mut SpaceSelection>,
            Option<&SpacesPageSnapshot>,
            Has<SpacesListSent>,
        ),
        (
            With<PageReady>,
            Or<(With<Spaces>, With<vmux_layout::LayoutCef>)>,
        ),
    >,
    browsers: NonSend<Browsers>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mains: Query<Entity, With<vmux_layout::window::Main>>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    spaces_ui: Query<(), With<SpacesUiStateUpdates>>,
    mut commands: Commands,
) {
    for (entity, selection, snapshot, sent) in &mut views {
        let host = vmux_layout::window::host_window_of(entity, &child_of, &host_windows);
        let main = host.and_then(|host| main_for_window(host, &mains, &child_of, &host_windows));
        let rows = space_rows_from_world(&spaces, &tab_q, settings.as_deref(), main);
        let active = rows
            .iter()
            .position(|space| space.is_active)
            .unwrap_or_default();
        let selected = if let Some(mut selection) = selection {
            if !sent {
                selection.0 = active;
            } else {
                selection.0 = selection.0.min(rows.len().saturating_sub(1));
            }
            selection.0
        } else {
            active
        };
        let payload = SpacesListEvent {
            spaces: rows,
            selected: selected as u32,
        };
        if sent && snapshot.is_some_and(|snapshot| snapshot.0 == payload) {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        if layout_ui.contains(entity) {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_layout::state::LayoutUiState,
            >::from_event(entity, &payload));
        }
        if spaces_ui.contains(entity) {
            commands.trigger(vmux_core::host::UiStateWrite::<SpacesUiState>::from_event(
                entity, &payload,
            ));
        }
        commands
            .entity(entity)
            .insert((SpacesListSent, SpacesPageSnapshot(payload)));
    }
}

fn on_project_activate(
    trigger: On<UiInput<ProjectActivateRequest>>,
    active: Option<Res<ActiveSpace>>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    mut saves: MessageWriter<vmux_setting::SettingsSaveRequest>,
) {
    let (Some(active), Some(mut settings)) = (active, settings) else {
        return;
    };
    let space_id = active.record.id.clone();
    let changed = settings
        .bypass_change_detection()
        .activate_space_project(&space_id, trigger.event().payload.path.trim());
    if changed {
        settings.set_changed();
        saves.write(vmux_setting::SettingsSaveRequest);
    }
}

fn on_project_forget(
    trigger: On<UiInput<ProjectForgetRequest>>,
    active: Option<Res<ActiveSpace>>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    mut saves: MessageWriter<vmux_setting::SettingsSaveRequest>,
) {
    let (Some(active), Some(mut settings)) = (active, settings) else {
        return;
    };
    let space_id = active.record.id.clone();
    let changed = settings
        .bypass_change_detection()
        .forget_space_project(&space_id, trigger.event().payload.path.trim());
    if changed {
        settings.set_changed();
        saves.write(vmux_setting::SettingsSaveRequest);
    }
}

fn relay_space_requests<T: Message + Clone>(mut reader: MessageReader<T>, mut commands: Commands) {
    for request in reader.read() {
        commands.trigger(UiInput {
            webview: Entity::PLACEHOLDER,
            payload: request.clone(),
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
        &'static ChildOf,
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
        &'static ChildOf,
    ),
    With<vmux_layout::tab::Tab>,
>;

fn bump_space_tab(tabs: &SpaceTabQuery, space: Entity, commands: &mut Commands) {
    if let Some((tab, _, _, _)) = tabs
        .iter()
        .filter(|(_, _, _, parent)| parent.parent() == space)
        .max_by_key(|(_, _, ts, _)| ts.0)
    {
        commands
            .entity(tab)
            .insert(vmux_history::LastActivatedAt::now());
    }
}

fn deactivate_spaces_in_main(spaces: &SpaceQuery, main: Entity, commands: &mut Commands) {
    for (entity, _, is_active, _, parent) in spaces.iter() {
        if parent.parent() == main && is_active {
            commands.entity(entity).remove::<vmux_core::Active>();
        }
    }
}

fn main_for_window(
    window: Entity,
    mains: &Query<Entity, With<vmux_layout::window::Main>>,
    child_of: &Query<&ChildOf>,
    host_windows: &Query<&HostWindow>,
) -> Option<Entity> {
    mains.iter().find(|main| {
        vmux_layout::window::host_window_of(*main, child_of, host_windows) == Some(window)
    })
}

#[derive(Clone)]
struct SpaceViewTemplate {
    id: String,
    name: String,
    order: u32,
}

impl SpaceViewTemplate {
    fn find(spaces: &SpaceListQuery, id: &str) -> Option<Self> {
        spaces
            .iter()
            .filter(|(_, candidate, _, _, _, _, _)| candidate.0 == id)
            .min_by_key(|(_, _, _, _, order, _, _)| order.map(|order| order.0).unwrap_or(u32::MAX))
            .map(|(_, id, name, _, order, _, _)| Self {
                id: id.0.clone(),
                name: name.to_string(),
                order: order.map(|order| order.0).unwrap_or(u32::MAX),
            })
    }

    fn first_other(spaces: &SpaceListQuery, excluded: &str) -> Option<Self> {
        spaces
            .iter()
            .filter(|(_, candidate, _, _, _, _, _)| candidate.0 != excluded)
            .min_by_key(|(_, _, _, _, order, _, _)| order.map(|order| order.0).unwrap_or(u32::MAX))
            .map(|(_, id, name, _, order, _, _)| Self {
                id: id.0.clone(),
                name: name.to_string(),
                order: order.map(|order| order.0).unwrap_or(u32::MAX),
            })
    }

    fn bundle(&self, main: Entity) -> impl Bundle {
        (
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId(self.id.clone()),
            Name::new(self.name.clone()),
            vmux_core::Order(self.order),
            vmux_core::Active,
            vmux_history::LastActivatedAt::now(),
            vmux_layout::space::space_view_bundle(),
            ChildOf(main),
        )
    }

    fn layout_request(
        &self,
        space: Entity,
        window: Entity,
        settings: Option<&vmux_setting::AppSettings>,
    ) -> TabLayoutSpawnRequest {
        let startup_dir = settings.and_then(|settings| settings.startup_dir(&self.id));
        let content = settings
            .map(|settings| settings.startup_url(&self.id))
            .filter(|url| !url.is_empty())
            .map(|url| TabLayoutSpawnContent::Url {
                url,
                pending_prompt: None,
            })
            .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt);
        TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: None,
            startup_dir,
            content,
            clear_pending_stack: true,
            focus: true,
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

fn on_space_rename(
    trigger: On<UiInput<SpaceRenameRequest>>,
    spaces: SpaceQuery,
    tabs: SpaceTabQuery,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let name = request.name.trim();
    if name.is_empty() {
        return;
    }
    if !spaces
        .iter()
        .any(|(_, id, _, _, _)| id.0 == request.space_id)
    {
        return;
    }
    let existing: std::collections::HashSet<String> = spaces
        .iter()
        .filter(|(_, id, _, _, _)| id.0 != request.space_id)
        .map(|(_, id, _, _, _)| id.0.clone())
        .collect();
    let new_id = crate::model::unique_space_id_among(&existing, name);
    for (entity, id, _, _, _) in spaces.iter() {
        if id.0 != request.space_id {
            continue;
        }
        commands.entity(entity).insert((
            Name::new(new_id.clone()),
            vmux_layout::space::SpaceId(new_id.clone()),
        ));
    }
    if new_id == request.space_id {
        return;
    }
    for (tab, id, _, _) in tabs.iter() {
        if id.0 == request.space_id {
            commands
                .entity(tab)
                .insert(vmux_layout::space::SpaceId(new_id.clone()));
        }
    }
}

fn on_space_open_page(
    trigger: On<UiInput<SpaceOpenPageRequest>>,
    mains: Query<Entity, With<vmux_layout::window::Main>>,
    host_windows: Query<&HostWindow>,
    focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
    focus: Option<Res<vmux_layout::stack::FocusedStack>>,
    mut spawn_requests: Option<MessageWriter<PageOpenRequest>>,
    stacks: Query<(Entity, &PageMetadata), With<Stack>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let window = host_windows
        .get(trigger.event().webview)
        .ok()
        .map(|host| host.0)
        .or_else(|| focused_window.as_deref().and_then(|focused| focused.0));
    let Some(window) = window else { return };
    let Some(_) = main_for_window(window, &mains, &child_of, &host_windows) else {
        return;
    };
    if let Some((existing, _)) = stacks.iter().find(|(stack, metadata)| {
        metadata.url == SPACES_PAGE_URL
            && vmux_layout::window::host_window_of(*stack, &child_of, &host_windows) == Some(window)
    }) {
        vmux_core::focus_pane_entity(existing, &mut commands, &child_of);
        return;
    }
    let Some(pane) = focus.as_deref().and_then(|focus| focus.pane) else {
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
}

#[allow(clippy::too_many_arguments)]
fn on_space_delete(
    trigger: On<UiInput<SpaceDeleteRequest>>,
    spaces: SpaceQuery,
    space_list: SpaceListQuery,
    tabs: SpaceTabQuery,
    mains: Query<Entity, With<vmux_layout::window::Main>>,
    host_windows: Query<&HostWindow>,
    focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    child_of: Query<&ChildOf>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
    let window = host_windows
        .get(trigger.event().webview)
        .ok()
        .map(|host| host.0)
        .or_else(|| focused_window.as_deref().and_then(|focused| focused.0));
    let Some(window) = window else { return };
    let Some(_) = main_for_window(window, &mains, &child_of, &host_windows) else {
        return;
    };
    let id = trigger.event().payload.space_id.as_str();
    let logical_ids: std::collections::HashSet<&str> = spaces
        .iter()
        .map(|(_, id, _, _, _)| id.0.as_str())
        .collect();
    if logical_ids.len() <= 1 {
        return;
    }
    let Some(fallback) = SpaceViewTemplate::first_other(&space_list, id) else {
        return;
    };
    let mut affected_mains = Vec::new();
    for (entity, space_id, _, _, parent) in spaces.iter() {
        if space_id.0 != id {
            continue;
        }
        if !affected_mains.contains(&parent.parent()) {
            affected_mains.push(parent.parent());
        }
        commands.entity(entity).despawn();
    }
    if affected_mains.is_empty() {
        return;
    }
    for affected_main in affected_mains {
        deactivate_spaces_in_main(&spaces, affected_main, &mut commands);
        if let Some((target_entity, _, _, _, _)) = spaces
            .iter()
            .filter(|(_, space_id, _, _, parent)| {
                space_id.0 != id && parent.parent() == affected_main
            })
            .min_by_key(|(_, _, _, order, _)| order.map(|order| order.0).unwrap_or(u32::MAX))
        {
            commands
                .entity(target_entity)
                .insert((vmux_core::Active, vmux_history::LastActivatedAt::now()));
            bump_space_tab(&tabs, target_entity, &mut commands);
            continue;
        }
        let Some(affected_window) =
            vmux_layout::window::host_window_of(affected_main, &child_of, &host_windows)
        else {
            continue;
        };
        let space = commands.spawn(fallback.bundle(affected_main)).id();
        layout_requests.write(fallback.layout_request(space, affected_window, settings.as_deref()));
    }
}

#[allow(clippy::too_many_arguments)]
fn on_space_attach(
    trigger: On<UiInput<SpaceAttachRequest>>,
    spaces: SpaceQuery,
    space_list: SpaceListQuery,
    tabs: SpaceTabQuery,
    mains: Query<Entity, With<vmux_layout::window::Main>>,
    host_windows: Query<&HostWindow>,
    focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    child_of: Query<&ChildOf>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
    let window = host_windows
        .get(trigger.event().webview)
        .ok()
        .map(|host| host.0)
        .or_else(|| focused_window.as_deref().and_then(|focused| focused.0));
    let Some(window) = window else { return };
    let Some(main) = main_for_window(window, &mains, &child_of, &host_windows) else {
        return;
    };
    let id = trigger.event().payload.space_id.as_str();
    let local = spaces
        .iter()
        .find(|(_, space_id, _, _, parent)| space_id.0 == id && parent.parent() == main);
    let Some((entity, _, is_active, _, _)) = local else {
        let Some(template) = SpaceViewTemplate::find(&space_list, id) else {
            return;
        };
        deactivate_spaces_in_main(&spaces, main, &mut commands);
        let space = commands.spawn(template.bundle(main)).id();
        layout_requests.write(template.layout_request(space, window, settings.as_deref()));
        return;
    };
    if is_active {
        return;
    }
    deactivate_spaces_in_main(&spaces, main, &mut commands);
    commands
        .entity(entity)
        .insert((vmux_core::Active, vmux_history::LastActivatedAt::now()));
    bump_space_tab(&tabs, entity, &mut commands);
}

#[allow(clippy::too_many_arguments)]
fn on_space_create(
    trigger: On<UiInput<SpaceCreateRequest>>,
    spaces: SpaceQuery,
    mains: Query<Entity, With<vmux_layout::window::Main>>,
    host_windows: Query<&HostWindow>,
    focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    child_of: Query<&ChildOf>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
    let window = host_windows
        .get(trigger.event().webview)
        .ok()
        .map(|host| host.0)
        .or_else(|| focused_window.as_deref().and_then(|focused| focused.0));
    let Some(window) = window else { return };
    let Some(main) = main_for_window(window, &mains, &child_of, &host_windows) else {
        return;
    };
    let count = spaces
        .iter()
        .filter(|(_, _, _, _, parent)| parent.parent() == main)
        .count();
    let requested_name = trigger.event().payload.name.trim();
    let name = if requested_name.is_empty() {
        format!("Space {}", count + 1)
    } else {
        requested_name.to_string()
    };
    let existing: std::collections::HashSet<String> =
        spaces.iter().map(|(_, id, _, _, _)| id.0.clone()).collect();
    let id = crate::model::unique_space_id_among(&existing, &name);
    let order = spaces
        .iter()
        .filter(|(_, _, _, _, parent)| parent.parent() == main)
        .filter_map(|(_, _, _, order, _)| order.map(|order| order.0))
        .max()
        .map(|max| max + 1)
        .unwrap_or(0);
    deactivate_spaces_in_main(&spaces, main, &mut commands);
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
    let startup_dir = settings
        .as_deref()
        .and_then(|settings| settings.startup_dir(&id));
    layout_requests.write(TabLayoutSpawnRequest {
        space,
        primary_window: window,
        name: None,
        startup_dir,
        content: TabLayoutSpawnContent::Url {
            url: SPACES_PAGE_URL.to_string(),
            pending_prompt: None,
        },
        clear_pending_stack: true,
        focus: true,
    });
}

#[allow(clippy::too_many_arguments)]
fn handle_open_in_new_space(
    mut reader: MessageReader<OpenRequest>,
    spaces: SpaceQuery,
    mains: Query<Entity, With<vmux_layout::window::Main>>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let Some(window) = focused_window.0 else {
            continue;
        };
        let Some(main) = main_for_window(window, &mains, &child_of, &host_windows) else {
            continue;
        };

        let count = spaces
            .iter()
            .filter(|(_, _, _, _, parent)| parent.parent() == main)
            .count();
        let name = format!("Space {}", count + 1);
        let existing: std::collections::HashSet<String> = spaces
            .iter()
            .map(|(_, sid, _, _, _)| sid.0.clone())
            .collect();
        let id = crate::model::unique_space_id_among(&existing, &name);
        let order = spaces
            .iter()
            .filter(|(_, _, _, _, parent)| parent.parent() == main)
            .filter_map(|(_, _, _, order, _)| order.map(|o| o.0))
            .max()
            .map(|max| max + 1)
            .unwrap_or(0);
        deactivate_spaces_in_main(&spaces, main, &mut commands);
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
        let startup_dir = settings
            .as_deref()
            .and_then(|settings| settings.startup_dir(&id));
        let content = request
            .url
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| TabLayoutSpawnContent::Url {
                url: url.to_string(),
                pending_prompt: None,
            })
            .or_else(|| {
                effective_startup_url
                    .as_deref()
                    .map(|startup| startup.0.as_str())
                    .filter(|startup| !startup.is_empty())
                    .map(|startup| TabLayoutSpawnContent::Url {
                        url: startup.to_string(),
                        pending_prompt: None,
                    })
            })
            .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt);
        layout_requests.write(TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: None,
            startup_dir,
            content,
            clear_pending_stack: true,
            focus: true,
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
    use bevy::ecs::system::RunSystemOnce;
    use vmux_command::CommandRequest;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    #[test]
    fn space_mcp_definition_dispatches_to_the_typed_request() {
        let definitions = OpenRequest::definitions();
        let tools = definitions
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["open_in_new_space"],
        );
        let invocation =
            vmux_command::CommandInvocation::new(Entity::PLACEHOLDER, "open_in_new_space")
                .with_arguments(serde_json::json!({"url": "https://vmux.ai"}));
        assert!(OpenRequest::try_from(&invocation).is_ok());
    }

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
            update_channel: Default::default(),
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            projects: Default::default(),
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
    fn main_for_window_walks_through_the_main_column() {
        let mut app = App::new();
        let window = app.world_mut().spawn_empty().id();
        let root = app.world_mut().spawn(HostWindow(window)).id();
        let column = app.world_mut().spawn(ChildOf(root)).id();
        let main = app
            .world_mut()
            .spawn((vmux_layout::window::Main, ChildOf(column)))
            .id();

        let resolved = app
            .world_mut()
            .run_system_once(
                move |mains: Query<Entity, With<vmux_layout::window::Main>>,
                      child_of: Query<&ChildOf>,
                      host_windows: Query<&HostWindow>| {
                    main_for_window(window, &mains, &child_of, &host_windows)
                },
            )
            .unwrap();

        assert_eq!(resolved, Some(main));
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
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .insert_resource(settings)
            .insert_resource(ActiveSpace {
                record: work_space_record(),
            })
            .add_systems(Update, update_effective_startup_url);

        app.update();

        assert_eq!(
            app.world().resource::<vmux_core::EffectiveStartupUrl>().0,
            "https://work.example"
        );
    }

    #[test]
    fn attaching_a_shared_space_creates_a_window_local_view() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(on_space_attach);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.insert_resource(vmux_layout::window::FocusedWindow(Some(second_window)));
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_column = app.world_mut().spawn(ChildOf(first_root)).id();
        let second_column = app.world_mut().spawn(ChildOf(second_root)).id();
        let first_main = app
            .world_mut()
            .spawn((vmux_layout::window::Main, ChildOf(first_column)))
            .id();
        let second_main = app
            .world_mut()
            .spawn((vmux_layout::window::Main, ChildOf(second_column)))
            .id();
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("shared".to_string()),
            Name::new("shared"),
            vmux_core::Order(0),
            vmux_core::Active,
            vmux_layout::space::space_view_bundle(),
            ChildOf(first_main),
        ));
        let previous = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("second".to_string()),
                Name::new("second"),
                vmux_core::Order(0),
                vmux_core::Active,
                vmux_layout::space::space_view_bundle(),
                ChildOf(second_main),
            ))
            .id();
        let webview = app.world_mut().spawn(HostWindow(second_window)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SpaceAttachRequest {
                space_id: "shared".to_string(),
            },
        });
        app.update();

        let shared_views: Vec<(Entity, Entity, bool)> = app
            .world_mut()
            .query_filtered::<
                (Entity, &ChildOf, Has<vmux_core::Active>),
                With<vmux_layout::space::Space>,
            >()
            .iter(app.world())
            .filter_map(|(entity, parent, active)| {
                let id = app
                    .world()
                    .get::<vmux_layout::space::SpaceId>(entity)?;
                (id.0 == "shared").then_some((entity, parent.parent(), active))
            })
            .collect();
        assert_eq!(shared_views.len(), 2);
        assert!(
            shared_views
                .iter()
                .any(|(_, parent, active)| *parent == first_main && *active)
        );
        assert!(
            shared_views
                .iter()
                .any(|(_, parent, active)| *parent == second_main && *active)
        );
        assert!(!app.world().entity(previous).contains::<vmux_core::Active>());
    }

    #[test]
    fn deleting_a_shared_space_removes_every_view_and_activates_local_fallbacks() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(on_space_delete);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.insert_resource(vmux_layout::window::FocusedWindow(Some(second_window)));
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_main = app
            .world_mut()
            .spawn((vmux_layout::window::Main, ChildOf(first_root)))
            .id();
        let second_main = app
            .world_mut()
            .spawn((vmux_layout::window::Main, ChildOf(second_root)))
            .id();
        for main in [first_main, second_main] {
            app.world_mut().spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("shared".to_string()),
                Name::new("shared"),
                vmux_core::Order(0),
                vmux_core::Active,
                vmux_layout::space::space_view_bundle(),
                ChildOf(main),
            ));
        }
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("fallback".to_string()),
            Name::new("fallback"),
            vmux_core::Order(1),
            vmux_layout::space::space_view_bundle(),
            ChildOf(first_main),
        ));
        let webview = app.world_mut().spawn(HostWindow(second_window)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SpaceDeleteRequest {
                space_id: "shared".to_string(),
            },
        });
        app.update();

        let views: Vec<(String, Entity, bool)> = app
            .world_mut()
            .query_filtered::<(
                &vmux_layout::space::SpaceId,
                &ChildOf,
                Has<vmux_core::Active>,
            ), With<vmux_layout::space::Space>>()
            .iter(app.world())
            .map(|(id, parent, active)| (id.0.clone(), parent.parent(), active))
            .collect();
        assert!(views.iter().all(|(id, _, _)| id != "shared"));
        assert_eq!(
            views
                .iter()
                .filter(|(id, _, active)| id == "fallback" && *active)
                .count(),
            2
        );
        assert!(
            views
                .iter()
                .any(|(id, parent, _)| id == "fallback" && *parent == first_main)
        );
        assert!(
            views
                .iter()
                .any(|(id, parent, _)| id == "fallback" && *parent == second_main)
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
                ..Default::default()
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
                ..Default::default()
            },
        );
        settings.spaces.insert(
            "inactive".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(inactive_dir.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .insert_resource(settings)
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

    fn project_request_app(active: &str, projects: Vec<vmux_setting::SpaceProject>) -> App {
        let mut settings = test_settings();
        settings.spaces.insert(
            "work".into(),
            vmux_setting::SpaceOverrides {
                projects,
                active_project: Some(active.to_string()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_setting::SettingsSaveRequest>()
            .insert_resource(settings)
            .insert_resource(ActiveSpace {
                record: SpaceRecord {
                    id: "work".into(),
                    name: "Work".into(),
                    profile: bootstrap_profile_name(),
                },
            })
            .add_observer(on_project_activate)
            .add_observer(on_project_forget);
        app
    }

    fn run_project_request<T: Send + Sync + 'static>(app: &mut App, request: T) {
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(UiInput {
            webview,
            payload: request,
        });
        app.update();
    }

    fn space_state(app: &App) -> (Vec<String>, Option<String>) {
        let space = app
            .world()
            .resource::<AppSettings>()
            .space("work")
            .expect("space");
        (
            space.projects.iter().map(|p| p.path.clone()).collect(),
            space.active_project.clone(),
        )
    }

    #[test]
    fn activating_a_project_moves_only_the_space_default() {
        let mut app = project_request_app(
            "/repo/alpha",
            vec![
                vmux_setting::SpaceProject::at("/repo/alpha"),
                vmux_setting::SpaceProject::at("/repo/beta"),
            ],
        );
        let tabs: Vec<Entity> = ["/repo/alpha", "/repo/beta"]
            .iter()
            .map(|dir| {
                app.world_mut()
                    .spawn(vmux_layout::tab::Tab {
                        startup_dir: Some((*dir).to_string()),
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        let before: Vec<Option<String>> = tabs
            .iter()
            .map(|tab| {
                app.world()
                    .get::<vmux_layout::tab::Tab>(*tab)
                    .and_then(|t| t.startup_dir.clone())
            })
            .collect();

        run_project_request(
            &mut app,
            ProjectActivateRequest {
                path: "/repo/beta".into(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        );

        assert_eq!(space_state(&app).1.as_deref(), Some("/repo/beta"));
        let after: Vec<Option<String>> = tabs
            .iter()
            .map(|tab| {
                app.world()
                    .get::<vmux_layout::tab::Tab>(*tab)
                    .and_then(|t| t.startup_dir.clone())
            })
            .collect();
        assert_eq!(
            before, after,
            "choosing the space's default must leave every open tab where it was"
        );
    }

    #[test]
    fn forgetting_the_active_project_falls_back_to_one_that_remains() {
        let mut app = project_request_app(
            "/repo/beta",
            vec![
                vmux_setting::SpaceProject::at("/repo/alpha"),
                vmux_setting::SpaceProject::at("/repo/beta"),
            ],
        );

        run_project_request(
            &mut app,
            ProjectForgetRequest {
                path: "/repo/beta".into(),
            },
        );

        assert_eq!(
            space_state(&app),
            (vec!["/repo/alpha".to_string()], Some("/repo/alpha".into()))
        );
    }

    #[test]
    fn activating_a_project_the_space_does_not_hold_is_ignored() {
        let mut app = project_request_app(
            "/repo/alpha",
            vec![vmux_setting::SpaceProject::at("/repo/alpha")],
        );

        run_project_request(
            &mut app,
            ProjectActivateRequest {
                path: "/repo/elsewhere".into(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        );

        assert_eq!(space_state(&app).1.as_deref(), Some("/repo/alpha"));
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
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .insert_resource(settings)
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
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .insert_resource(settings)
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
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(on_space_rename);
        app.world_mut().spawn(bevy::window::PrimaryWindow);
        let main = app.world_mut().spawn(vmux_layout::window::Main).id();
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::SpaceId("rename-src-test".to_string()),
                Name::new("rename-src-test"),
                vmux_core::Active,
                vmux_layout::space::CurrentSpace,
                ChildOf(main),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                vmux_layout::tab::Tab::default(),
                vmux_layout::space::SpaceId("rename-src-test".to_string()),
                vmux_history::LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: Entity::PLACEHOLDER,
            payload: SpaceRenameRequest {
                space_id: "rename-src-test".to_string(),
                name: "Vmux Ai/Vmux".to_string(),
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
        assert!(
            app.world()
                .get::<vmux_layout::space::CurrentSpace>(space)
                .is_some()
        );
    }
}
