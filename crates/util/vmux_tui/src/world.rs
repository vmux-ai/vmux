use std::collections::HashMap;
use std::env;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};

use bevy::asset::AssetPlugin;
use bevy::input::ButtonState;
use bevy::input::InputPlugin as BevyInputPlugin;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::{ExitCondition, PrimaryWindow, WindowPlugin, WindowResolution};
use bevy_cef::prelude::{
    Browsers, UiInput, WebviewCommittedNavigationEvent, WebviewLoadingStateEvent,
};
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_api::space::{ProjectActivateRequest, ProjectPickerRequest, SpaceAttachRequest};
use vmux_app::VmuxPlugin;
use vmux_command::{CommandBarOpenRequest, CommandInvocation, ReadCommandRequests};
use vmux_ecs::event::{TermResizeEvent, TermScrollEvent, TerminalUiState, TerminalViewportState};
use vmux_ecs::page::PageReady;
use vmux_ecs::profile::{Profile, ProfileBootstrapSpec, ProfilePaths, ProjectsRoot};
use vmux_ecs::service::{ServiceRequest, ServiceUnavailable};
use vmux_ecs::terminal::TerminalLaunch;
use vmux_ecs::ui::{UiEmitEvent, UiIpcEpoch, UiIpcGeneration, UiIpcNavigationRevision, UiIpcReady};
use vmux_ecs::{
    Active, Order, PageIdentity, PageMetadata, PageNavigation, PageOpenError, PageOpenHandled,
    PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask, UiState,
};
use vmux_flex::Node;
use vmux_history::LastActivatedAt;
use vmux_layout::event::{SideSheetStackActivateRequest, TabActivateRequest, TabCreateRequest};
use vmux_layout::pane::{
    CloseRequest as PaneCloseRequest, Pane, PaneSplit, PaneSplitDirection, Zoomed,
};
use vmux_layout::presentation::Retiring;
use vmux_layout::settings::{ConfirmCloseSettings, LayoutSettings};
use vmux_layout::space::{FocusedSpace, Space, SpaceId};
use vmux_layout::stack::{
    ActiveTabParam, CloseRequest as StackCloseRequest, CloseStackRequest, LayoutFocus, Stack,
};
use vmux_layout::state::LayoutUiState;
use vmux_layout::tab::{CloseRequest as TabCloseRequest, Tab};
use vmux_layout::{LayoutCef, LayoutRestoreRequest, LayoutStartupSet, SelectRequest};
#[cfg(test)]
use vmux_setting::TerminalSettings;
use vmux_setting::{
    AppSettings, ResolvedColorScheme, ResolvedScheme, SettingsLoadSet, SystemAppearance,
    TerminalTheme,
};
use vmux_shortcut::PendingPrefix;
use vmux_space::Spaces;
use vmux_space::model::BOOTSTRAP_SPACE_ID;
use vmux_terminal::{ProcessExited, Terminal, TerminalPlugin};

use crate::input::DecodedKey;
use crate::io::TuiFs;
use crate::service::ServiceBridgePlugin;
use crate::theme::{Palette, Rgb, ThemePreset};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn contains(self, col: u16, row: u16) -> bool {
        col >= self.x
            && col < self.x.saturating_add(self.w)
            && row >= self.y
            && row < self.y.saturating_add(self.h)
    }

    pub fn pty_size(self, pane_gap: bool) -> (u16, u16) {
        if pane_gap {
            (
                self.w.saturating_sub(2).max(2),
                self.h.saturating_sub(2).max(1),
            )
        } else {
            (self.w.max(2), self.h.max(1))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Divider {
    pub axis: SplitAxis,
    pub x: u16,
    pub y: u16,
    pub len: u16,
}

#[derive(Clone)]
pub struct PaneView {
    pub pane: Entity,
    pub terminal: Option<Entity>,
    pub title: String,
    pub url: String,
    pub rect: Rect,
    pub focused: bool,
    pub viewport: Option<TerminalViewportState>,
    pub scroll_top: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedSpace {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub last_activated: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    Consumed,
    UnmatchedLeader,
    Forward,
}

#[derive(Clone)]
pub struct DesktopConfig {
    pub profile: Profile,
    pub settings: Option<AppSettings>,
    pub shell_override: Option<String>,
    pub spaces: Vec<SeedSpace>,
    pub cwd: PathBuf,
    pub isolated_config: bool,
}

#[derive(Resource, Clone)]
struct BootConfig {
    settings: Option<AppSettings>,
    shell_override: Option<String>,
    cwd: PathBuf,
    isolated_config: bool,
}

impl DesktopConfig {
    pub fn load(cwd: &Path) -> Self {
        let profile = Profile::current();
        let desktop_paths = ProfilePaths::current();
        let spaces = Self::load_store_spaces(&desktop_paths.store().join("store.ron"));
        Self {
            profile,
            settings: None,
            shell_override: None,
            spaces,
            cwd: cwd.to_path_buf(),
            isolated_config: false,
        }
    }

    #[cfg(test)]
    pub fn for_test(cwd: &Path) -> Self {
        let mut settings = AppSettings::default();
        settings.browser.startup_url = "vmux://terminal/".into();
        settings.terminal = Some(TerminalSettings {
            shell: Some("/bin/sh".into()),
            confirm_close: false,
            ..Default::default()
        });
        let cwd_text = cwd.to_string_lossy();
        if !cwd_text.is_empty() {
            settings.activate_space_project(BOOTSTRAP_SPACE_ID, &cwd_text);
        }
        Self {
            profile: Profile::named("tui-test"),
            settings: Some(settings),
            shell_override: Some("/bin/sh".into()),
            spaces: Vec::new(),
            cwd: cwd.to_path_buf(),
            isolated_config: true,
        }
    }

    fn resolve_shell_override(settings: &AppSettings) -> Option<String> {
        if let Ok(override_shell) = env::var("VMUX_SHELL")
            && !override_shell.trim().is_empty()
        {
            return Some(override_shell);
        }
        let default_env_shell = TerminalTheme::default_shell();
        let has_explicit_shell = settings.terminal.as_ref().is_some_and(|terminal| {
            terminal
                .shell
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || terminal.themes.iter().any(|entry| {
                    entry.name == terminal.default_theme
                        && !entry.shell.trim().is_empty()
                        && entry.shell != default_env_shell
                })
        });
        if has_explicit_shell {
            None
        } else {
            Self::login_shell()
        }
    }

    fn load_store_spaces(path: &Path) -> Vec<SeedSpace> {
        let Some(contents) = TuiFs::read_to_string(path) else {
            return Vec::new();
        };
        Self::parse_store_spaces(&contents)
    }

    fn parse_store_spaces(source: &str) -> Vec<SeedSpace> {
        let mut spaces = Vec::new();
        let mut in_components = false;
        let mut is_space = false;
        let mut space_id = None::<String>;
        let mut name = None::<String>;
        let mut order = None::<u32>;
        let mut last_activated = 0i64;
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed == "components: {" {
                in_components = true;
                is_space = false;
                space_id = None;
                name = None;
                order = None;
                last_activated = 0;
                continue;
            }
            if !in_components {
                continue;
            }
            if trimmed == "}," {
                if is_space && let Some(id) = space_id.take() {
                    let display = name.take().unwrap_or_else(|| id.clone());
                    spaces.push(SeedSpace {
                        id,
                        name: display,
                        order: order.unwrap_or(spaces.len() as u32),
                        last_activated,
                    });
                }
                in_components = false;
                continue;
            }
            if trimmed.starts_with("\"vmux_desktop::space::Space\":") {
                is_space = true;
            } else if let Some(rest) = trimmed.strip_prefix("\"vmux_desktop::space::SpaceId\": (\"")
                && let Some((value, _)) = rest.split_once('"')
            {
                space_id = Some(value.to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("\"bevy_ecs::name::Name\": \"")
                && let Some((value, _)) = rest.split_once('"')
            {
                name = Some(value.to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("\"vmux_ecs::Order\": (")
                && let Some((value, _)) = rest.split_once(')')
            {
                order = value.trim().parse().ok();
            } else if let Some(rest) = trimmed.strip_prefix("\"vmux_history::LastActivatedAt\": (")
                && let Some((value, _)) = rest.split_once(')')
            {
                last_activated = value.trim().parse().unwrap_or(0);
            }
        }
        spaces.sort_by_key(|space| space.order);
        spaces
    }

    fn login_shell() -> Option<String> {
        let pwd = unsafe { libc::getpwuid(libc::getuid()) };
        if pwd.is_null() {
            return None;
        }
        let raw = unsafe { (*pwd).pw_shell };
        if raw.is_null() {
            return None;
        }
        let text = unsafe { std::ffi::CStr::from_ptr(raw) }
            .to_str()
            .ok()?
            .trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_owned())
        }
    }
}

#[derive(Resource, Default)]
struct CommandTracker {
    dispatched: bool,
    space_picker_requested: bool,
    exit_requested: bool,
    prev_pages: usize,
}

#[derive(Component)]
struct SurfaceReadySent;

struct HeadlessSurfacePlugin;

impl Plugin for HeadlessSurfacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandTracker>()
            .add_message::<PageOpenRequest>()
            .configure_sets(
                Update,
                (
                    PageOpenSet::ResolveTarget,
                    PageOpenSet::HandleKnownPages,
                    PageOpenSet::Fallback,
                    PageOpenSet::Respond,
                )
                    .chain()
                    .after(ReadCommandRequests),
            )
            .add_systems(
                Startup,
                apply_config
                    .after(SettingsLoadSet)
                    .before(LayoutStartupSet::DefaultTab),
            )
            .add_observer(apply_login_shell)
            .add_observer(accept_surface_emit)
            .add_systems(PreUpdate, mark_ready)
            .add_systems(
                Update,
                (
                    mark_ready,
                    bind_projects_root.before(vmux_project::StoreSet::Load),
                    keep_confirm_disabled,
                    watch_last_pane_exit,
                    track_commands.in_set(ReadCommandRequests),
                    track_command_bar.after(ReadCommandRequests),
                    resolve_page_opens.in_set(PageOpenSet::ResolveTarget),
                    finish_page_opens.in_set(PageOpenSet::Respond),
                ),
            );
    }
}

fn bind_projects_root(
    profiles: Query<(Entity, &ProjectsRoot, Option<&vmux_project::Root>)>,
    mut commands: Commands,
) {
    for (entity, directory, current) in &profiles {
        if current.is_none_or(|root| root.0 != directory.0.path()) {
            commands
                .entity(entity)
                .insert(vmux_project::Root(directory.0.path().to_path_buf()));
        }
    }
}

fn keep_confirm_disabled(mut confirm: ResMut<ConfirmCloseSettings>) {
    if confirm.enabled {
        confirm.enabled = false;
    }
}

type ActiveTabFilter = (With<Tab>, Without<Retiring>);
type ActiveStackFilter = (With<Stack>, Without<Retiring>);

#[allow(clippy::too_many_arguments)]
fn watch_last_pane_exit(
    focused_space: FocusedSpace,
    tabs: Query<(Entity, &ChildOf), ActiveTabFilter>,
    focus: LayoutFocus,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<(), ActiveStackFilter>,
    exited_terminals: Query<(), (With<Terminal>, Added<ProcessExited>)>,
    mut stack_closes: MessageReader<StackCloseRequest>,
    mut direct_stack_closes: MessageReader<CloseStackRequest>,
    mut pane_closes: MessageReader<PaneCloseRequest>,
    mut tab_closes: MessageReader<TabCloseRequest>,
    mut tracker: ResMut<CommandTracker>,
) {
    let close_requested = stack_closes.read().count() > 0
        || direct_stack_closes.read().count() > 0
        || pane_closes.read().count() > 0
        || tab_closes.read().count() > 0
        || !exited_terminals.is_empty();
    let space = focused_space.entity();
    let mut active_pages = 0usize;
    for (tab, parent) in &tabs {
        if space.is_some_and(|space| parent.parent() != space) {
            continue;
        }
        for leaf in focus.leaves(tab) {
            if let Ok(children) = pane_children.get(leaf) {
                active_pages += children
                    .iter()
                    .filter(|&child| stacks.contains(child))
                    .count();
            }
        }
    }
    if close_requested && tracker.prev_pages == 1 {
        tracker.exit_requested = true;
    }
    tracker.prev_pages = active_pages;
}

fn apply_login_shell(
    trigger: On<Add, TerminalLaunch>,
    config: Res<BootConfig>,
    mut launches: Query<&mut TerminalLaunch>,
) {
    let Some(shell) = config.shell_override.as_deref() else {
        return;
    };
    if let Ok(mut launch) = launches.get_mut(trigger.entity)
        && launch.args.is_empty()
    {
        launch.command = shell.to_owned();
    }
}

fn resolve_page_opens(
    mut requests: MessageReader<PageOpenRequest>,
    active_tab: ActiveTabParam,
    focus: LayoutFocus,
    mut commands: Commands,
) {
    for request in requests.read() {
        let stack = match request.target {
            PageOpenTarget::Stack(stack) => Some(stack),
            PageOpenTarget::NewStackInPane(pane) => Some(
                commands
                    .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id(),
            ),
            PageOpenTarget::ActiveStackInPane(pane) => focus.stack(pane),
            PageOpenTarget::ActiveStack => active_tab
                .get()
                .and_then(|tab| focus.pane(tab))
                .and_then(|pane| focus.stack(pane)),
            PageOpenTarget::NewStack => {
                active_tab
                    .get()
                    .and_then(|tab| focus.pane(tab))
                    .map(|pane| {
                        commands
                            .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                            .id()
                    })
            }
            PageOpenTarget::ContainingStack(_) => None,
        };
        let Some(stack) = stack else {
            continue;
        };
        let id = PageOpenId::new();
        let url = if request.url.starts_with(TerminalPlugin::url()) {
            request.url.clone()
        } else {
            TerminalPlugin::url().to_string()
        };
        commands.entity(stack).insert(PageNavigation::new(id));
        commands.spawn(PageOpenTask {
            id,
            stack,
            url,
            request_id: request.request_id,
        });
    }
}

type FinishedPageOpens = (
    With<PageOpenTask>,
    Or<(With<PageOpenHandled>, With<PageOpenError>)>,
);

type PendingHeadlessSurfaces = (
    Without<SurfaceReadySent>,
    Or<(With<LayoutCef>, With<Terminal>, With<Spaces>)>,
);

fn finish_page_opens(done: Query<Entity, FinishedPageOpens>, mut commands: Commands) {
    for entity in &done {
        commands.entity(entity).despawn();
    }
}

fn apply_config(
    mut config: ResMut<BootConfig>,
    mut settings: ResMut<AppSettings>,
    mut layout_settings: ResMut<LayoutSettings>,
    mut confirm: ResMut<ConfirmCloseSettings>,
    mut appearance: Query<&mut SystemAppearance>,
    existing_spaces: Query<&SpaceId, With<Space>>,
    mut commands: Commands,
) {
    if config.isolated_config
        && let Some(test_settings) = &config.settings
    {
        *settings = test_settings.clone();
        *layout_settings = test_settings.layout.clone();
    }
    if config.shell_override.is_none() {
        config.shell_override = DesktopConfig::resolve_shell_override(&settings);
    }
    if !config.isolated_config {
        let known: Vec<String> = existing_spaces.iter().map(|id| id.0.clone()).collect();
        let mut next_order = known.len() as u32;
        for space_id in settings.spaces.keys() {
            if !known.iter().any(|existing| existing == space_id) {
                commands.spawn((
                    Space,
                    SpaceId(space_id.clone()),
                    Name::new(space_id.clone()),
                    Order(next_order),
                    LastActivatedAt(0),
                    vmux_layout::profile::Profile {
                        name: Profile::current().id().to_owned(),
                    },
                ));
                next_order += 1;
            }
        }
        let primary = known
            .first()
            .map(String::as_str)
            .unwrap_or(BOOTSTRAP_SPACE_ID);
        let has_project = settings
            .space(primary)
            .is_some_and(|space| space.active_dir().is_some());
        if !has_project && !config.cwd.as_os_str().is_empty() {
            settings.activate_space_project(primary, &config.cwd.to_string_lossy());
        }
    }
    confirm.enabled = false;
    let detected = detect_system_scheme();
    for mut system in &mut appearance {
        system.0 = Some(detected);
    }
}

fn detect_system_scheme() -> ResolvedScheme {
    if TuiFs::is_macos_dark_mode() {
        ResolvedScheme::Dark
    } else {
        ResolvedScheme::Light
    }
}

fn accept_surface_emit(event: On<UiEmitEvent>) {
    event.receipt.mark_delivered();
}

fn mark_ready(surfaces: Query<Entity, PendingHeadlessSurfaces>, mut commands: Commands) {
    for entity in &surfaces {
        commands.entity(entity).insert((
            SurfaceReadySent,
            UiIpcReady,
            UiIpcGeneration(1),
            UiIpcNavigationRevision(1),
            UiIpcEpoch(1),
        ));
        commands.trigger(UiInput {
            webview: entity,
            payload: PageReady {},
        });
    }
}

fn track_commands(
    mut invocations: MessageReader<CommandInvocation>,
    mut tracker: ResMut<CommandTracker>,
) {
    for invocation in invocations.read() {
        tracker.dispatched = true;
        if invocation.id == "quit" {
            tracker.exit_requested = true;
        }
    }
}

fn track_command_bar(
    mut requests: MessageReader<CommandBarOpenRequest>,
    mut tracker: ResMut<CommandTracker>,
) {
    for _ in requests.read() {
        tracker.space_picker_requested = true;
    }
}

pub struct TuiWorld {
    pub app: App,
    pub window: Entity,
    last_sizes: HashMap<Entity, (u16, u16)>,
    scroll_top: HashMap<Entity, u32>,
}

impl TuiWorld {
    pub fn boot(
        cols: u16,
        rows: u16,
        store_root: PathBuf,
        config: DesktopConfig,
        prefer_daemon: bool,
    ) -> Self {
        let boot = BootConfig {
            settings: config.settings.clone(),
            shell_override: config.shell_override.clone(),
            cwd: config.cwd.clone(),
            isolated_config: config.isolated_config,
        };
        let mut app = App::new();
        app.insert_resource(boot)
            .add_plugins((
                MinimalPlugins,
                AssetPlugin::default(),
                BevyInputPlugin,
                WindowPlugin {
                    primary_window: None,
                    close_when_requested: false,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                },
                VmuxPlugin::builder()
                    .ecs(true)
                    .jobs(true)
                    .flex(true)
                    .runtime(true)
                    .setting(true)
                    .command(true)
                    .history(true)
                    .layout(true)
                    .layout_runtime(true)
                    .space(true)
                    .shortcut(true)
                    .build(),
                ServiceBridgePlugin { prefer_daemon },
                vmux_project::filesystem::FilesystemPlugin,
                TerminalPlugin,
                vmux_input::InputPlugin,
                HeadlessSurfacePlugin,
            ))
            .insert_non_send(Browsers::default())
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadingStateEvent>()
            .insert_resource(config.settings.unwrap_or_default());

        let profile = config.profile.clone();
        let profile_label = profile.id().to_owned();
        let spaces = config.spaces.clone();
        let projects_root = if config.isolated_config {
            store_root.join("projects")
        } else {
            ProfilePaths::current().projects()
        };
        let paths = ProfilePaths::at(profile.clone(), store_root);
        app.world_mut().spawn(ProfileBootstrapSpec {
            profile,
            paths,
            test: config.isolated_config,
            projects_root,
        });
        for space in spaces {
            app.world_mut().spawn((
                Space,
                SpaceId(space.id),
                Name::new(space.name),
                Order(space.order),
                LastActivatedAt(space.last_activated),
                vmux_layout::profile::Profile {
                    name: profile_label.clone(),
                },
            ));
        }
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: WindowResolution::new(u32::from(cols) * 8, u32::from(rows) * 16),
                    ..default()
                },
                PrimaryWindow,
                Active,
            ))
            .id();
        app.finish();
        app.cleanup();
        app.world_mut().write_message(LayoutRestoreRequest);
        for _ in 0..6 {
            app.update();
        }
        Self {
            app,
            window,
            last_sizes: HashMap::new(),
            scroll_top: HashMap::new(),
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if let Some(mut window) = self.app.world_mut().get_mut::<Window>(self.window) {
            window
                .resolution
                .set_physical_resolution(u32::from(cols) * 8, u32::from(rows) * 16);
        }
    }

    pub fn tick(&mut self) {
        self.app.update();
    }

    pub fn press_key(&mut self, decoded: DecodedKey) -> KeyOutcome {
        let was_pending = self.leader_active();
        self.app
            .world_mut()
            .resource_mut::<CommandTracker>()
            .dispatched = false;
        decoded.write_events(
            ButtonState::Pressed,
            self.window,
            &mut self
                .app
                .world_mut()
                .resource_mut::<Messages<KeyboardInput>>(),
        );
        self.app.update();
        decoded.write_events(
            ButtonState::Released,
            self.window,
            &mut self
                .app
                .world_mut()
                .resource_mut::<Messages<KeyboardInput>>(),
        );
        self.app.update();
        let dispatched = self.app.world().resource::<CommandTracker>().dispatched;
        let now_pending = self.leader_active();
        if dispatched || now_pending {
            KeyOutcome::Consumed
        } else if was_pending {
            KeyOutcome::UnmatchedLeader
        } else {
            KeyOutcome::Forward
        }
    }

    pub fn leader_active(&mut self) -> bool {
        let mut query = self.app.world_mut().query::<&PendingPrefix>();
        query.iter(self.app.world()).any(PendingPrefix::is_active)
    }

    pub fn take_space_picker_requested(&mut self) -> bool {
        let mut tracker = self.app.world_mut().resource_mut::<CommandTracker>();
        let requested = tracker.space_picker_requested;
        tracker.space_picker_requested = false;
        requested
    }

    pub fn exit_requested(&self) -> bool {
        self.app.world().resource::<CommandTracker>().exit_requested
    }

    pub fn service_unavailable(&mut self) -> Option<String> {
        let mut query = self.app.world_mut().query::<&ServiceUnavailable>();
        query.iter(self.app.world()).next().map(|u| u.0.clone())
    }

    pub fn is_light_mode(&mut self) -> bool {
        let mut query = self.app.world_mut().query::<&ResolvedColorScheme>();
        query
            .iter(self.app.world())
            .next()
            .is_some_and(|scheme| scheme.0 == ResolvedScheme::Light)
    }

    pub fn focus_follows_mouse(&self) -> bool {
        self.app
            .world()
            .resource::<AppSettings>()
            .layout
            .pane
            .focus_follows_mouse
    }

    pub fn configured_theme(&self) -> Option<ThemePreset> {
        let terminal = self
            .app
            .world()
            .resource::<AppSettings>()
            .terminal
            .as_ref()?;
        let resolved = terminal.resolve_theme(&terminal.default_theme);
        ThemePreset::parse(&resolved.color_scheme)
    }

    pub fn terminal_palette(&mut self) -> Option<Palette> {
        let (terminal, _) = self.focused_terminal()?;
        let ui = self
            .app
            .world()
            .get::<UiState<TerminalUiState>>(terminal)
            .and_then(UiState::current)?;
        let theme = ui.theme.as_ref()?;
        let bg = theme.background.components();
        let fg = theme.foreground.components();
        let mut ansi = [Rgb(0, 0, 0); 16];
        for (slot, color) in ansi.iter_mut().zip(theme.ansi.iter()) {
            let c = color.components();
            *slot = Rgb(c[0], c[1], c[2]);
        }
        Some(Palette::from_ansi(
            Rgb(bg[0], bg[1], bg[2]),
            Rgb(fg[0], fg[1], fg[2]),
            ansi,
        ))
    }

    pub fn layout_ui(&mut self) -> Option<LayoutUiState> {
        let mut query = self
            .app
            .world_mut()
            .query_filtered::<&UiState<LayoutUiState>, With<LayoutCef>>();
        query
            .iter(self.app.world())
            .find_map(|state| state.current().cloned())
    }

    pub fn layout_entity(&mut self) -> Option<Entity> {
        let mut query = self
            .app
            .world_mut()
            .query_filtered::<Entity, With<LayoutCef>>();
        query.iter(self.app.world()).next()
    }

    pub fn is_zoomed(&mut self) -> bool {
        self.active_tab_entity()
            .and_then(|tab| self.zoomed_leaf(tab))
            .is_some()
    }

    pub fn pane_gap(&self) -> bool {
        self.app.world().resource::<AppSettings>().layout.pane.gap > 0.0
    }

    pub fn toggle_pane_gap(&mut self) -> bool {
        let next = if self.app.world().resource::<AppSettings>().layout.pane.gap > 0.0 {
            0.0
        } else {
            8.0
        };
        self.app
            .world_mut()
            .resource_mut::<AppSettings>()
            .layout
            .pane
            .gap = next;
        self.app
            .world_mut()
            .resource_mut::<LayoutSettings>()
            .pane
            .gap = next;
        next > 0.0
    }

    pub fn toggle_side_sheet(&mut self) {
        self.app
            .world_mut()
            .write_message(CommandInvocation::new(Entity::PLACEHOLDER, "toggle_layout"));
        self.app.update();
        self.app.update();
    }

    pub fn send_tab_create(&mut self) {
        if let Some(webview) = self.layout_entity() {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: TabCreateRequest,
            });
        }
    }

    pub fn send_tab_activate(&mut self, tab_id: String) {
        if let Some(webview) = self.layout_entity() {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: TabActivateRequest { tab_id },
            });
        }
    }

    pub fn send_space_attach(&mut self, space_id: String) {
        if let Some(webview) = self.layout_entity() {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: SpaceAttachRequest { space_id },
            });
        }
    }

    pub fn send_project_activate(&mut self, path: String) {
        if let Some(webview) = self.layout_entity() {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: ProjectActivateRequest {
                    path,
                    branch: String::new(),
                    checkout: String::new(),
                    pane_id: None,
                },
            });
        }
    }

    pub fn send_project_picker(&mut self) {
        if let Some(webview) = self.layout_entity() {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: ProjectPickerRequest {
                    current_dir: String::new(),
                    pane_id: None,
                },
            });
        }
    }

    pub fn select_pane(&mut self, pane: Entity) {
        if let Some(tab) = self.active_tab_entity() {
            self.app
                .world_mut()
                .write_message(SelectRequest { root: tab, pane });
        }
        if let Some(webview) = self.layout_entity()
            && let Some(stack) = self.active_stack_of(pane)
        {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: SideSheetStackActivateRequest {
                    pane_id: pane.to_bits(),
                    stack_id: stack.to_bits(),
                },
            });
        }
    }

    pub fn send_terminal_input(&mut self, process_id: ProcessId, data: Vec<u8>) {
        self.app
            .world_mut()
            .write_message(ServiceRequest(ClientMessage::ProcessInput {
                process_id,
                data,
            }));
    }

    pub fn scroll_terminal(&mut self, terminal: Entity, delta: i32, pane_rows: u16) {
        let Some(ui) = self
            .app
            .world()
            .get::<UiState<TerminalUiState>>(terminal)
            .and_then(UiState::current)
        else {
            return;
        };
        let tail_top = ui.viewport.total_rows.saturating_sub(u32::from(pane_rows));
        let current = self.scroll_top.get(&terminal).copied().unwrap_or(tail_top);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as u32).min(tail_top)
        };
        let follow = next >= tail_top;
        if follow {
            self.scroll_top.remove(&terminal);
        } else {
            self.scroll_top.insert(terminal, next);
        }
        self.app.world_mut().trigger(UiInput {
            webview: terminal,
            payload: TermScrollEvent {
                top_row: next,
                follow,
            },
        });
    }

    pub fn clear_scroll(&mut self, terminal: Entity) {
        if self.scroll_top.remove(&terminal).is_none() {
            return;
        }
        let top_row = self
            .app
            .world()
            .get::<UiState<TerminalUiState>>(terminal)
            .and_then(UiState::current)
            .map(|ui| ui.viewport.total_rows)
            .unwrap_or(0);
        self.app.world_mut().trigger(UiInput {
            webview: terminal,
            payload: TermScrollEvent {
                top_row,
                follow: true,
            },
        });
    }

    pub fn sync_terminal_sizes(&mut self, views: &[PaneView], pane_gap: bool) {
        let mut updates = Vec::new();
        for view in views {
            let Some(terminal) = view.terminal else {
                continue;
            };
            let size = view.rect.pty_size(pane_gap);
            if self.last_sizes.get(&terminal).copied() != Some(size) {
                self.last_sizes.insert(terminal, size);
                updates.push((terminal, size));
            }
        }
        for (webview, (cols, rows)) in updates {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: TermResizeEvent {
                    char_width: 1.0,
                    char_height: 1.0,
                    viewport_width: f32::from(cols),
                    viewport_height: f32::from(rows),
                },
            });
        }
    }

    pub fn pane_count(&mut self) -> usize {
        let mut query = self
            .app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>();
        query.iter(self.app.world()).count()
    }

    pub fn focused_terminal(&mut self) -> Option<(Entity, ProcessId)> {
        let tab = self.active_tab_entity()?;
        let pane = self.focused_pane_in(tab)?;
        let stack = self.active_stack_of(pane)?;
        let terminal = self.terminal_of(stack)?;
        let process_id = self.app.world().get::<ProcessId>(terminal).copied()?;
        Some((terminal, process_id))
    }

    pub fn pane_views(
        &mut self,
        workspace: Rect,
        pane_gap: bool,
    ) -> (Vec<PaneView>, Vec<Divider>, Option<Entity>) {
        let Some(tab) = self.active_tab_entity() else {
            return (Vec::new(), Vec::new(), None);
        };
        let focused = self.focused_pane_in(tab);
        if let Some(zoomed_leaf) = self.zoomed_leaf(tab) {
            let view = self.build_pane_view(zoomed_leaf, workspace, true);
            return (vec![view], Vec::new(), Some(zoomed_leaf));
        }
        let Some(root) = self.root_pane_of(tab) else {
            return (Vec::new(), Vec::new(), focused);
        };
        let mut rects = Vec::new();
        let mut dividers = Vec::new();
        self.layout_pane_subtree(root, workspace, pane_gap, &mut rects, &mut dividers);
        let views = rects
            .into_iter()
            .map(|(pane, rect)| self.build_pane_view(pane, rect, Some(pane) == focused))
            .collect();
        (views, dividers, focused)
    }

    fn build_pane_view(&self, pane: Entity, rect: Rect, focused: bool) -> PaneView {
        let stack = self.active_stack_of(pane);
        let terminal = stack.and_then(|s| self.terminal_of(s));
        let viewport = terminal.and_then(|t| {
            self.app
                .world()
                .get::<UiState<TerminalUiState>>(t)
                .and_then(UiState::current)
                .map(|ui| ui.viewport.clone())
        });
        let title = self.terminal_title(terminal, stack);
        let url = self.terminal_url(terminal, stack);
        let scroll_top = terminal.and_then(|t| self.scroll_top.get(&t).copied());
        PaneView {
            pane,
            terminal,
            title,
            url,
            rect,
            focused,
            viewport,
            scroll_top,
        }
    }

    fn terminal_url(&self, terminal: Option<Entity>, stack: Option<Entity>) -> String {
        if let Some(entity) = terminal
            && let Some(meta) = self.app.world().get::<PageMetadata>(entity)
            && !meta.url.is_empty()
        {
            return meta.url.clone();
        }
        if let Some(entity) = stack
            && let Some(meta) = self.app.world().get::<PageMetadata>(entity)
            && !meta.url.is_empty()
        {
            return meta.url.clone();
        }
        TerminalPlugin::url().to_string()
    }

    fn terminal_title(&self, terminal: Option<Entity>, stack: Option<Entity>) -> String {
        if let Some(entity) = terminal {
            if let Some(id) = self.app.world().get::<PageIdentity>(entity)
                && let Some(title) = id.title.as_deref()
                && !title.is_empty()
            {
                return title.to_string();
            }
            if let Some(ui) = self
                .app
                .world()
                .get::<UiState<TerminalUiState>>(entity)
                .and_then(UiState::current)
                && !ui.title.is_empty()
            {
                return ui.title.clone();
            }
            if let Some(meta) = self.app.world().get::<PageMetadata>(entity)
                && !meta.title.is_empty()
            {
                return meta.title.clone();
            }
        }
        if let Some(entity) = stack
            && let Some(meta) = self.app.world().get::<PageMetadata>(entity)
            && !meta.title.is_empty()
        {
            return meta.title.clone();
        }
        "shell".into()
    }

    fn layout_pane_subtree(
        &self,
        pane: Entity,
        area: Rect,
        pane_gap: bool,
        rects: &mut Vec<(Entity, Rect)>,
        dividers: &mut Vec<Divider>,
    ) {
        let Some(split) = self.app.world().get::<PaneSplit>(pane) else {
            rects.push((pane, area));
            return;
        };
        let children: Vec<Entity> = self
            .app
            .world()
            .get::<Children>(pane)
            .map(|list| {
                list.iter()
                    .filter(|&child| self.app.world().get::<Pane>(child).is_some())
                    .collect()
            })
            .unwrap_or_default();
        if children.is_empty() {
            rects.push((pane, area));
            return;
        }
        let axis = match split.direction {
            PaneSplitDirection::Row => SplitAxis::Vertical,
            PaneSplitDirection::Column => SplitAxis::Horizontal,
        };
        self.layout_split_children(&children, axis, area, pane_gap, rects, dividers);
    }

    fn layout_split_children(
        &self,
        children: &[Entity],
        axis: SplitAxis,
        area: Rect,
        pane_gap: bool,
        rects: &mut Vec<(Entity, Rect)>,
        dividers: &mut Vec<Divider>,
    ) {
        let weights: Vec<f32> = children
            .iter()
            .map(|&child| {
                self.app
                    .world()
                    .get::<Node>(child)
                    .map(|node| node.flex_grow.max(0.05))
                    .unwrap_or(1.0)
            })
            .collect();
        let total_weight: f32 = weights.iter().sum::<f32>().max(0.05);
        let sep = u16::from(!pane_gap);
        let sep_count = children.len().saturating_sub(1) as u16;
        let total_span = match axis {
            SplitAxis::Vertical => area.w,
            SplitAxis::Horizontal => area.h,
        };
        let min_span: u16 = match axis {
            SplitAxis::Vertical => 2,
            SplitAxis::Horizontal => 1,
        };
        let avail = total_span.saturating_sub(sep * sep_count);
        let mut cursor = match axis {
            SplitAxis::Vertical => area.x,
            SplitAxis::Horizontal => area.y,
        };
        let mut remaining = avail;
        for (index, (&child, &weight)) in children.iter().zip(&weights).enumerate() {
            let is_last = index + 1 == children.len();
            let rest = (children.len() - index - 1) as u16;
            let proportional = (f32::from(avail) * (weight / total_weight)).round() as u16;
            let span = if is_last {
                remaining.max(1)
            } else if remaining >= min_span.saturating_mul(rest.saturating_add(1)) {
                let max_span = remaining.saturating_sub(min_span.saturating_mul(rest));
                proportional.clamp(min_span, max_span)
            } else {
                let max_span = remaining.saturating_sub(rest).max(1);
                proportional.clamp(1, max_span)
            };
            let child_rect = match axis {
                SplitAxis::Vertical => Rect {
                    x: cursor,
                    y: area.y,
                    w: span,
                    h: area.h,
                },
                SplitAxis::Horizontal => Rect {
                    x: area.x,
                    y: cursor,
                    w: area.w,
                    h: span,
                },
            };
            self.layout_pane_subtree(child, child_rect, pane_gap, rects, dividers);
            cursor = cursor.saturating_add(span);
            remaining = remaining.saturating_sub(span);
            if !is_last && sep > 0 {
                let divider = match axis {
                    SplitAxis::Vertical => Divider {
                        axis,
                        x: cursor,
                        y: area.y,
                        len: area.h,
                    },
                    SplitAxis::Horizontal => Divider {
                        axis,
                        x: area.x,
                        y: cursor,
                        len: area.w,
                    },
                };
                dividers.push(divider);
                cursor = cursor.saturating_add(sep);
            }
        }
    }

    fn active_tab_entity(&mut self) -> Option<Entity> {
        let mut space_q = self
            .app
            .world_mut()
            .query_filtered::<Entity, (With<Space>, With<Active>)>();
        let active_space = space_q.iter(self.app.world()).next();
        let mut tab_q = self.app.world_mut().query_filtered::<(
            Entity,
            &LastActivatedAt,
            Option<&ChildOf>,
        ), (With<Tab>, Without<Retiring>)>();
        tab_q
            .iter(self.app.world())
            .filter(|(_, _, parent)| {
                active_space.is_none() || parent.is_some_and(|p| Some(p.parent()) == active_space)
            })
            .max_by_key(|(_, ts, _)| ts.0)
            .map(|(entity, _, _)| entity)
    }

    fn root_pane_of(&self, tab: Entity) -> Option<Entity> {
        self.app
            .world()
            .get::<Children>(tab)?
            .iter()
            .find(|&child| self.app.world().get::<Pane>(child).is_some())
    }

    fn focused_pane_in(&self, tab: Entity) -> Option<Entity> {
        let root = self.root_pane_of(tab)?;
        let mut leaves = Vec::new();
        self.collect_leaves(root, &mut leaves);
        leaves.into_iter().max_by_key(|&leaf| {
            self.app
                .world()
                .get::<LastActivatedAt>(leaf)
                .map(|ts| ts.0)
                .unwrap_or(0)
        })
    }

    fn collect_leaves(&self, pane: Entity, out: &mut Vec<Entity>) {
        if self.app.world().get::<PaneSplit>(pane).is_none() {
            out.push(pane);
            return;
        }
        if let Some(children) = self.app.world().get::<Children>(pane) {
            for child in children.iter() {
                if self.app.world().get::<Pane>(child).is_some() {
                    self.collect_leaves(child, out);
                }
            }
        }
    }

    fn active_stack_of(&self, pane: Entity) -> Option<Entity> {
        let children = self.app.world().get::<Children>(pane)?;
        children
            .iter()
            .filter(|&child| {
                self.app.world().get::<Stack>(child).is_some()
                    && self.app.world().get::<Retiring>(child).is_none()
            })
            .max_by_key(|&stack| {
                self.app
                    .world()
                    .get::<LastActivatedAt>(stack)
                    .map(|ts| ts.0)
                    .unwrap_or(0)
            })
    }

    fn terminal_of(&self, stack: Entity) -> Option<Entity> {
        self.app
            .world()
            .get::<Children>(stack)?
            .iter()
            .find(|&child| self.app.world().get::<Terminal>(child).is_some())
    }

    fn zoomed_leaf(&self, tab: Entity) -> Option<Entity> {
        self.app.world().get::<Zoomed>(tab).map(|z| z.leaf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_store_spaces_read_only() {
        let sample = r#"
    4294964636: (
      components: {
        "bevy_ecs::name::Name": "saturn",
        "vmux_desktop::space::Space": (),
        "vmux_desktop::space::SpaceId": ("space-1"),
        "vmux_ecs::Order": (0),
        "vmux_history::LastActivatedAt": (1795612352899),
      },
    ),
"#;
        let spaces = DesktopConfig::parse_store_spaces(sample);
        assert_eq!(
            spaces,
            vec![SeedSpace {
                id: "space-1".into(),
                name: "saturn".into(),
                order: 0,
                last_activated: 1795612352899,
            }]
        );
    }

    #[test]
    fn headless_desktop_plugins_project_ui_state_and_handle_shortcuts() {
        let dir = tempfile::tempdir().unwrap();
        let config = DesktopConfig::for_test(dir.path());
        let mut world = TuiWorld::boot(120, 40, dir.path().to_path_buf(), config, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while (world.pane_count() == 0
            || world.focused_terminal().is_none()
            || world.layout_ui().is_none_or(|ui| {
                ui.tab_strip.is_none() || ui.side_sheet.is_none() || ui.spaces.spaces.is_empty()
            }))
            && std::time::Instant::now() < deadline
        {
            world.tick();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ui = world
            .layout_ui()
            .expect("LayoutUiState must materialize headlessly");
        assert!(world.pane_count() >= 1);
        assert!(ui.tab_strip.is_some());
        assert!(ui.side_sheet.is_some());
        assert!(!ui.spaces.spaces.is_empty());

        let (_, pid) = world
            .focused_terminal()
            .expect("default terminal entity must spawn");
        world.send_terminal_input(pid, b"echo VMUX_ECS_OK\r".to_vec());
        let pty_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut saw_output = false;
        let ws = Rect {
            x: 1,
            y: 4,
            w: 100,
            h: 30,
        };
        while !saw_output && std::time::Instant::now() < pty_deadline {
            world.tick();
            let (views, _, _) = world.pane_views(ws, false);
            saw_output = views.iter().any(|view| {
                view.viewport.as_ref().is_some_and(|vp| {
                    vp.rows.iter().any(|(_, line)| {
                        line.spans
                            .iter()
                            .any(|span| span.text.contains("VMUX_ECS_OK"))
                    })
                })
            });
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        assert!(saw_output, "TerminalUiState must project real PTY output");
        let (views, _, _) = world.pane_views(ws, false);
        let url = &views[0].url;
        assert!(
            url.strip_prefix("vmux://terminal/")
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())),
            "terminal address must include numeric PID, got: {url}"
        );

        let before = world.pane_count();
        assert_eq!(
            world.press_key(DecodedKey::parse(b"\x02").unwrap()),
            KeyOutcome::Consumed
        );
        assert!(world.leader_active());
        assert_eq!(
            world.press_key(DecodedKey::parse(b"%").unwrap()),
            KeyOutcome::Consumed
        );
        for _ in 0..8 {
            world.tick();
        }
        assert_eq!(world.pane_count(), before + 1);

        assert_eq!(
            world.press_key(DecodedKey::parse(b"\x02").unwrap()),
            KeyOutcome::Consumed
        );
        assert_eq!(
            world.press_key(DecodedKey::parse(b"s").unwrap()),
            KeyOutcome::Consumed
        );
        for _ in 0..4 {
            world.tick();
        }
        assert!(world.take_space_picker_requested());

        assert_eq!(
            world.press_key(DecodedKey::parse(b"\x02").unwrap()),
            KeyOutcome::Consumed
        );
        assert_eq!(
            world.press_key(DecodedKey::parse(b"x").unwrap()),
            KeyOutcome::Consumed
        );
        for _ in 0..8 {
            world.tick();
        }
        assert_eq!(world.pane_count(), before);
        assert!(!world.exit_requested());

        assert_eq!(
            world.press_key(DecodedKey::parse(b"\x02").unwrap()),
            KeyOutcome::Consumed
        );
        assert_eq!(
            world.press_key(DecodedKey::parse(b"x").unwrap()),
            KeyOutcome::Consumed
        );
        assert!(world.exit_requested());
    }

    #[test]
    fn exiting_sole_shell_process_requests_tui_exit() {
        let dir = tempfile::tempdir().unwrap();
        let config = DesktopConfig::for_test(dir.path());
        let mut world = TuiWorld::boot(100, 30, dir.path().to_path_buf(), config, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while (world.focused_terminal().is_none()
            || world
                .focused_terminal()
                .and_then(|(entity, _)| {
                    world
                        .app
                        .world()
                        .get::<UiState<TerminalUiState>>(entity)
                        .and_then(UiState::current)
                })
                .is_none())
            && std::time::Instant::now() < deadline
        {
            world.tick();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (_, pid) = world.focused_terminal().expect("terminal must spawn");
        world.send_terminal_input(pid, b"exit\r".to_vec());
        let exit_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !world.exit_requested() && std::time::Instant::now() < exit_deadline {
            world.tick();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        assert!(
            world.exit_requested(),
            "exiting the sole shell must request TUI exit"
        );
    }

    #[test]
    fn loads_registered_projects_from_vmux_project_filesystem_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let projects_dir = root.join("projects");
        let alpha_dir = projects_dir.join("alpha");
        let beta_dir = projects_dir.join("beta");
        fs::create_dir_all(&alpha_dir).unwrap();
        fs::create_dir_all(&beta_dir).unwrap();
        let store = format!(
            "(projects:{{\"vmux://projects/alpha\":(name:\"Alpha\",directory:\"{}\",repository:None,revision:1),\"vmux://projects/beta\":(name:\"Beta\",directory:\"{}\",repository:None,revision:1)}},hidden:[],archived:[],version:1)",
            alpha_dir.display(),
            beta_dir.display()
        );
        fs::write(projects_dir.join(".projects.ron"), store).unwrap();
        let config = DesktopConfig::for_test(&alpha_dir);
        let mut world = TuiWorld::boot(100, 30, root, config, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut labels = Vec::new();
        while std::time::Instant::now() < deadline {
            world.tick();
            if let Some(ui) = world.layout_ui() {
                labels = ui
                    .projects
                    .projects
                    .iter()
                    .map(|p| p.label.clone())
                    .collect();
                if labels.len() == 2 {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        labels.sort();
        assert_eq!(labels, vec!["Alpha".to_string(), "Beta".to_string()]);
    }

    #[test]
    fn closing_tab_removes_it_from_projected_tab_strip() {
        let dir = tempfile::tempdir().unwrap();
        let config = DesktopConfig::for_test(dir.path());
        let mut world = TuiWorld::boot(100, 30, dir.path().to_path_buf(), config, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while world
            .layout_ui()
            .and_then(|ui| ui.tab_strip)
            .is_none_or(|strip| strip.tabs.is_empty())
            && std::time::Instant::now() < deadline
        {
            world.tick();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(world.layout_ui().unwrap().tab_strip.unwrap().tabs.len(), 1);
        world.send_tab_create();
        for _ in 0..8 {
            world.tick();
        }
        assert_eq!(world.layout_ui().unwrap().tab_strip.unwrap().tabs.len(), 2);
        assert_eq!(
            world.press_key(DecodedKey::parse(b"\x02").unwrap()),
            KeyOutcome::Consumed
        );
        assert_eq!(
            world.press_key(DecodedKey::parse(b"&").unwrap()),
            KeyOutcome::Consumed
        );
        for _ in 0..8 {
            world.tick();
        }
        assert_eq!(
            world.layout_ui().unwrap().tab_strip.unwrap().tabs.len(),
            1,
            "closed retiring tab must be removed from tab_strip"
        );
    }

    #[test]
    fn narrow_nested_vertical_split_does_not_panic_and_zoom_is_tab_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let config = DesktopConfig::for_test(dir.path());
        let mut world = TuiWorld::boot(80, 24, dir.path().to_path_buf(), config, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while world.pane_count() == 0 && std::time::Instant::now() < deadline {
            world.tick();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        world.press_key(DecodedKey::parse(b"\x02").unwrap());
        world.press_key(DecodedKey::parse(b"%").unwrap());
        for _ in 0..8 {
            world.tick();
        }
        let narrow = Rect {
            x: 1,
            y: 1,
            w: 3,
            h: 10,
        };
        let (views, _, _) = world.pane_views(narrow, false);
        assert_eq!(views.len(), 2);
        assert!(views.iter().all(|view| view.rect.w >= 1));

        world.press_key(DecodedKey::parse(b"\x02").unwrap());
        world.press_key(DecodedKey::parse(b"z").unwrap());
        for _ in 0..4 {
            world.tick();
        }
        assert!(world.is_zoomed());
        world.send_tab_create();
        for _ in 0..8 {
            world.tick();
        }
        assert!(!world.is_zoomed());
    }
}
