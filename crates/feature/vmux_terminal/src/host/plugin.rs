use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::{
    ecs::relationship::Relationship,
    input::keyboard::Key,
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_api::command_bar::TerminalRequest as CommandBarTerminalRequest;
use vmux_api::input::KeyStroke;
use vmux_api::protocol::{ClientMessage, CopyModeKey, ProcessId};
use vmux_clipboard::Clipboard;
use vmux_command::CommandBarDismiss;
#[cfg(test)]
use vmux_command::CommandDefinition;
use vmux_command::{
    CommandInvocation, CommandRegistry, CommandRuntimePlugin, ReadCommandRequests,
    WriteCommandRequests,
};
use vmux_command::{KeyCombo, Keymap, Modifiers};
#[cfg(test)]
use vmux_ecs::PageOpenId;
use vmux_ecs::event::TerminalUiState;
use vmux_ecs::host::UiStateWrite;
use vmux_ecs::host::page::{BindsEditingChords, HostsPage};
use vmux_ecs::host::persistence::{PageRestore, PersistenceAppExt};
use vmux_ecs::service::{ServiceConnected, ServiceRequest, ServiceUnavailable};
use vmux_ecs::terminal::{TerminalSpawnRequest, TerminalSpawnTarget};
use vmux_ecs::{
    KeyboardOwner, PageIcon, PageIdentity, PageMetadata, PageOpenError, PageOpenHandled,
    PageOpenSet, PageOpenTask,
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::event::TERMINAL_CEF_BG_COLOR;
use vmux_layout::space::FocusedSpace;
use vmux_layout::stack::{CloseRequest as StackCloseRequest, FocusRequest, FocusedStack, Stack};
#[cfg(test)]
use vmux_layout::tab::Tab;
use vmux_layout::tab::TabHierarchy;
use vmux_layout::{CloseRequiresConfirmation, TerminalLayoutSpawnRequest};
#[cfg(test)]
use vmux_setting::SpaceOverrides;
use vmux_setting::{AppSettings, TerminalTheme};
use vmux_space::model::BOOTSTRAP_SPACE_ID;
#[cfg(test)]
use vmux_space::model::SpaceRecord;

use crate::launch::TerminalLaunch;

#[cfg(test)]
use super::input_queue::InputQueuePlugin;
#[cfg(test)]
use super::input_queue::pending_terminal_input;
use super::input_queue::{QueueTerminalInput, TerminalProcessIndex};
use super::mouse::TerminalMouseState;
use super::process_control::{PendingTerminalSnapshot, ProcessControlPlugin, TerminalGridSize};
use super::service::{
    ServiceIngressPlugin, TerminalProcessCreateFailed, TerminalProcessCreated,
    TerminalSelectionText, TerminalServiceError, TerminalViewportUpdate,
};
use super::state::{
    CopyModeInputState, CopyModePendingKey, TerminalCopyMode, TerminalMode, TerminalShortcutState,
};
use crate::event::*;
use crate::pid::{self, Pid};
use crate::{ProcessExited, RetainOnProcessExit, Terminal};
use CopyModeKey as K;
use vmux_ecs::service::ServiceMessageSet;
use vmux_flex::prelude::*;
use vmux_ui::i18n::Locale;

#[vmux_native::page]
pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::TerminalPage::plugin());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .route(vmux_ecs::HostSpawnRoute::page("vmux://terminal/")),
        )
        .add_message::<ServiceRequest>()
        .add_message::<super::command::TerminalCloseRequest>()
        .add_message::<super::command::TerminalNextRequest>()
        .add_message::<super::command::TerminalPrevRequest>()
        .add_message::<super::command::TerminalClearRequest>()
        .add_message::<super::command::CopyModeRequest>()
        .add_systems(Startup, bind_commands.in_set(vmux_command::BindCommands))
        .add_plugins((
            vmux_ecs::host::UiStatePlugin::<TerminalUiState>::default(),
            super::agent::AgentTerminalPlugin,
            crate::TerminalToolPlugin,
        ))
        .add_plugins(crate::contract::TerminalContractPlugin)
        .add_plugins(UiEventPlugin::<(CommandBarTerminalRequest,)>::default())
        .add_observer(open_from_command_bar)
        .register_persisted::<TerminalLaunch>()
        .add_systems(Update, sync_launch_to_stack)
        .add_message::<TerminalStackSpawnRequest>()
        .add_message::<TerminalSpawnRequest>()
        .add_plugins((
            pid::PidPlugin,
            crate::host::request::TerminalRequestPlugin,
            TerminalServicePlugin,
            TerminalInputPlugin,
            crate::process_monitor::ProcessMonitorPlugin,
            super::loading::LoadingPlugin,
            crate::snapshot::Plugin,
            crate::theme::TerminalThemePlugin,
        ));
    }
}

fn open_from_command_bar(
    trigger: On<UiInput<CommandBarTerminalRequest>>,
    focus: FocusedStack,
    pid_indexes: Query<&pid::PidToEntity>,
    locale: Option<Res<vmux_command::ResolvedLocale>>,
    users: Query<Entity, With<vmux_ecs::team::User>>,
    mut spawn: MessageWriter<TerminalSpawnRequest>,
    mut invocations: MessageWriter<CommandInvocation>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let value = &trigger.event().payload.value;
    let running = pid_indexes.iter().find_map(|index| {
        index
            .iter()
            .find_map(|(pid, entity)| (Pid(pid).page_url() == *value).then_some(entity))
    });
    if let Some(entity) = running {
        commands.trigger(vmux_ecs::ActivateRequest { entity });
        commands.trigger(CommandBarDismiss::new(webview, false));
        return;
    }

    if value.starts_with(TerminalPlugin::URL) {
        warn!("no terminal pane for {}; spawning new", value);
    }
    let cwd = if value.is_empty() || value.contains("://") {
        None
    } else if let Some(rest) = value.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .or_else(|_| Ok::<_, std::convert::Infallible>(PathBuf::from(value)))
            .ok()
    } else if value.starts_with('/') {
        Some(PathBuf::from(value))
    } else {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(value))
            .or_else(|_| Ok::<_, std::convert::Infallible>(PathBuf::from(value)))
            .ok()
    };
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    if let Some(pane) = focus.pane {
        spawn.write(TerminalSpawnRequest {
            cwd,
            target: TerminalSpawnTarget::NewStackInPane(pane),
            metadata: Some(PageMetadata {
                url: TerminalPlugin::URL.to_string(),
                title: locale.translate("command-terminal"),
                ..default()
            }),
        });
    } else {
        let caller = users.single().unwrap_or(Entity::PLACEHOLDER);
        invocations.write(
            CommandInvocation::new(caller, "open_in_new_stack")
                .with_arguments(serde_json::json!({ "url": TerminalPlugin::URL })),
        );
    }
    commands.trigger(CommandBarDismiss::new(webview, true));
}

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.message::<super::command::TerminalCloseRequest>(&mut commands);
    registry.message::<super::command::TerminalNextRequest>(&mut commands);
    registry.message::<super::command::TerminalPrevRequest>(&mut commands);
    registry.message::<super::command::TerminalClearRequest>(&mut commands);
    registry.message::<super::command::CopyModeRequest>(&mut commands);
}

struct TerminalServicePlugin;

impl Plugin for TerminalServicePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TerminalUpdatePlugin)
            .add_systems(
                Update,
                respond_stack_spawn
                    .in_set(TerminalStackSpawnSet)
                    .after(ServiceMessageSet),
            )
            .add_systems(Update, respond_spawn.in_set(ReadCommandRequests))
            .add_systems(Update, prewarm.run_if(resource_added::<AppSettings>))
            .add_observer(restart)
            .add_observer(restart_pty)
            .add_observer(removed);
    }
}

struct TerminalInputPlugin;

impl Plugin for TerminalInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, initialize_state)
            .add_systems(Update, format_url.after(pid::PidIndexSet))
            .add_plugins((
                super::mouse::MousePlugin,
                super::link::LinkPlugin,
                ProcessControlPlugin,
            ))
            .add_observer(term_key);
    }
}

fn prewarm(settings: Res<AppSettings>) {
    super::LoginShellEnvironment::prewarm(TerminalBundle::default_shell(&settings));
}

fn initialize_state(terminals: Query<Entity, Added<Terminal>>, mut commands: Commands) {
    for entity in &terminals {
        commands.entity(entity).insert((
            TerminalMode::default(),
            TerminalCopyMode::default(),
            TerminalShortcutState::default(),
            TerminalMouseState::default(),
        ));
    }
}

fn sync_launch_to_stack(
    terminals: Query<(&ChildOf, &TerminalLaunch), (With<Terminal>, Changed<TerminalLaunch>)>,
    stacks: Query<(), With<Stack>>,
    mut commands: Commands,
) {
    for (parent, launch) in &terminals {
        if stacks.contains(parent.get()) {
            commands.entity(parent.get()).insert(launch.clone());
        }
    }
}

struct TerminalUpdatePlugin;

impl Plugin for TerminalUpdatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::contract::TerminalContractPlugin,
            ServiceIngressPlugin,
        ))
        .add_message::<ProcessExitedEvent>()
        .add_message::<CommandLifecycleEvent>()
        .add_message::<OscTitleChanged>()
        .add_message::<vmux_ecs::notify::BellReceived>()
        .add_systems(Update, apply_osc_title.after(ServiceMessageSet))
        .add_systems(Update, clear_osc_title_on_exit.after(ServiceMessageSet))
        .add_systems(
            Update,
            handle_page_open.in_set(PageOpenSet::HandleKnownPages),
        )
        .add_systems(
            Update,
            spawn_layout_requested_content.after(vmux_layout::stack::StackCommandSet),
        )
        .add_systems(
            Update,
            (
                publish_service_status,
                resolve_pending_cwd,
                (
                    send_service_requests,
                    apply_process_start,
                    apply_viewport_updates,
                    apply_process_exits,
                    apply_service_errors,
                    copy_service_selection,
                )
                    .after(ServiceMessageSet)
                    .in_set(WriteCommandRequests),
                navigate_terminals.in_set(ReadCommandRequests),
                clear.in_set(ReadCommandRequests),
                enter_copy_mode.in_set(ReadCommandRequests),
            )
                .chain(),
        );
    }
}

const CTRL_V: u8 = 0x16;

#[derive(Clone, Copy)]
struct CopyModeKeyInput<'a> {
    key: &'a Key,
    key_code: KeyCode,
    ctrl: bool,
    shift: bool,
}

#[cfg(test)]
impl<'a> CopyModeKeyInput<'a> {
    fn new(key: &'a Key, key_code: KeyCode) -> Self {
        Self {
            key,
            key_code,
            ctrl: false,
            shift: false,
        }
    }

    fn shift(key: &'a Key, key_code: KeyCode) -> Self {
        Self {
            shift: true,
            ..Self::new(key, key_code)
        }
    }
}

#[derive(Event)]
pub struct RestartPty {
    pub entity: Entity,
}

#[derive(Message, Clone)]
pub struct TerminalStackSpawnRequest {
    pub pane: Entity,
    pub cwd: Option<PathBuf>,
    pub shell: Option<String>,
    pub agent_run: bool,
    pub pending_input: Option<Vec<u8>>,
    pub process_id: Option<ProcessId>,
    pub activate: bool,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalStackSpawnSet;

fn format_url(
    mut q: Query<
        (Option<&Pid>, &mut PageMetadata),
        (With<Terminal>, Or<(Changed<Pid>, Added<PageMetadata>)>),
    >,
) {
    for (pid, mut meta) in &mut q {
        let next = match pid {
            Some(pid) => pid.page_url(),
            None => TerminalPlugin::URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}

fn removed(
    trigger: On<Remove, ProcessId>,
    pids: Query<&ProcessId>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event_target();
    let Ok(process_id) = pids.get(entity) else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::KillProcess {
        process_id: *process_id,
    }));
}

fn spawn_layout_requested_content(
    mut reader: MessageReader<TerminalLayoutSpawnRequest>,
    settings: Res<AppSettings>,
    active_space: FocusedSpace,
    tabs: TabHierarchy,
    mut commands: Commands,
) {
    let space_id = active_space.id().unwrap_or(BOOTSTRAP_SPACE_ID);
    for request in reader.read() {
        let tab_dir = tabs.startup_dir(request.stack);
        let Ok(cwd) = settings.workspace_dir(space_id, tab_dir.as_deref()) else {
            continue;
        };
        let terminal = commands
            .spawn((
                TerminalBundle::with_cwd(&settings, cwd.as_deref()),
                ChildOf(request.stack),
            ))
            .id();
        commands.entity(terminal).insert(KeyboardOwner);
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_page_open(
    tasks: Query<(Entity, &PageOpenTask, Has<PageRestore>), PendingPageOpen>,
    pid_indexes: Query<&pid::PidToEntity>,
    tabs: TabHierarchy,
    saved_launches: Query<&TerminalLaunch, With<Stack>>,
    settings: Res<AppSettings>,
    active_space: FocusedSpace,
    mut commands: Commands,
) {
    let space_id = active_space.id().unwrap_or(BOOTSTRAP_SPACE_ID);
    for (entity, task, restoring) in &tasks {
        if task.url != TerminalPlugin::URL.trim_end_matches('/')
            && !task.url.starts_with(TerminalPlugin::URL)
        {
            continue;
        }
        let parsed = match url::Url::parse(&task.url) {
            Ok(parsed) => parsed,
            Err(error) => {
                commands.entity(entity).insert(PageOpenError {
                    message: format!("invalid terminal URL '{}': {error}", task.url),
                });
                continue;
            }
        };
        let path = parsed.path().trim_start_matches('/');
        if !path.is_empty() && !restoring {
            let Ok(pid) = path.parse::<u32>() else {
                commands.entity(entity).insert(PageOpenError {
                    message: format!("malformed terminal URL '{}'", task.url),
                });
                continue;
            };
            let mut existing = None;
            for index in &pid_indexes {
                if let Some(terminal) = index.get(pid) {
                    existing = Some(terminal);
                    break;
                }
            }
            if let Some(terminal) = existing {
                commands.trigger(vmux_ecs::ActivateRequest { entity: terminal });
                commands.entity(entity).insert(PageOpenHandled);
                continue;
            }
            warn!("no terminal pane for pid {pid}; spawning new");
        }
        let saved_launch = restoring
            .then(|| saved_launches.get(task.stack).ok())
            .flatten()
            .cloned();
        let cwd_param = parsed
            .query_pairs()
            .find(|(key, _)| key == "cwd")
            .map(|(_, value)| value.into_owned());
        let cwd = if let Some(launch) = saved_launch.as_ref() {
            Some(PathBuf::from(&launch.cwd))
        } else if let Some(cwd) = cwd_param.as_deref() {
            match vmux_space::WorkspaceCwd::try_from(cwd) {
                Ok(cwd) => cwd.into_path(),
                Err(message) => {
                    commands.entity(entity).insert(PageOpenError { message });
                    continue;
                }
            }
        } else {
            let tab_dir = tabs.startup_dir(task.stack);
            match settings.workspace_dir(space_id, tab_dir.as_deref()) {
                Ok(cwd) => cwd,
                Err(message) => {
                    commands.entity(entity).insert(PageOpenError { message });
                    continue;
                }
            }
        };
        commands.entity(task.stack).despawn_children();
        let title = cwd
            .as_ref()
            .map(|cwd| format!("Terminal ({})", cwd.display()))
            .unwrap_or_else(|| "Terminal".to_string());
        commands.entity(task.stack).insert(PageMetadata {
            url: TerminalPlugin::URL.to_string(),
            title,
            bg_color: Some(TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let terminal = commands
            .spawn((
                TerminalBundle::with_cwd(&settings, cwd.as_deref()),
                ChildOf(task.stack),
            ))
            .id();
        if let Some(launch) = saved_launch {
            commands.entity(terminal).insert(launch);
        }
        commands.entity(terminal).insert(KeyboardOwner);
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn respond_spawn(
    mut reader: MessageReader<TerminalSpawnRequest>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    child_of_q: Query<&ChildOf>,
) {
    for req in reader.read() {
        let term_e = commands
            .spawn(TerminalBundle::with_cwd(&settings, req.cwd.as_deref()))
            .id();
        commands.entity(term_e).insert(KeyboardOwner);
        let (stack_e, requested_pane) = match req.target {
            TerminalSpawnTarget::Detached => continue,
            TerminalSpawnTarget::Stack(stack) => (stack, None),
            TerminalSpawnTarget::NewStackInPane(pane) => (
                commands
                    .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id(),
                Some(pane),
            ),
        };
        if let Some(metadata) = req.metadata.clone() {
            commands.entity(stack_e).insert(metadata);
        }
        commands.entity(term_e).insert(ChildOf(stack_e));
        commands.entity(stack_e).insert(LastActivatedAt::now());
        match requested_pane {
            Some(pane) => {
                commands.entity(pane).insert(LastActivatedAt::now());
            }
            None => {
                if let Ok(parent) = child_of_q.get(stack_e) {
                    commands.entity(parent.0).insert(LastActivatedAt::now());
                }
            }
        }
    }
}

#[derive(Bundle)]
pub struct TerminalBundle {
    terminal: Terminal,
    browser: Browser,
    close_confirmation: CloseRequiresConfirmation,
    process_id: ProcessId,
    launch: TerminalLaunch,
    pending_create: PendingServiceCreate,
    metadata: PageMetadata,
    webview: WebviewWindowed,
    page_host: HostsPage,
    editing_chords: BindsEditingChords,
    size: WebviewSize,
    grid_size: TerminalGridSize,
    transform: Transform,
    node: Node,
    visibility: Visibility,
}

impl TerminalBundle {
    fn default_shell(settings: &AppSettings) -> String {
        settings
            .terminal
            .as_ref()
            .map(|terminal| terminal.resolve_theme(&terminal.default_theme).shell)
            .unwrap_or_else(TerminalTheme::default_shell)
    }

    pub fn new(settings: &AppSettings) -> Self {
        Self::with_cwd(settings, None)
    }

    pub fn with_cwd(settings: &AppSettings, cwd: Option<&std::path::Path>) -> Self {
        Self::with_shell(settings, cwd, None)
    }

    fn with_shell(
        settings: &AppSettings,
        cwd: Option<&std::path::Path>,
        shell: Option<&str>,
    ) -> Self {
        let shell = shell.map(str::to_string).unwrap_or_else(|| {
            settings
                .terminal
                .as_ref()
                .map(|t| t.resolve_theme(&t.default_theme).shell)
                .unwrap_or_else(TerminalTheme::default_shell)
        });
        let cwd = cwd
            .filter(|directory| !directory.to_string_lossy().contains("://"))
            .map(|directory| directory.to_string_lossy().to_string())
            .unwrap_or_default();
        let process_id = ProcessId::new();
        Self {
            terminal: Terminal,
            browser: Browser,
            close_confirmation: CloseRequiresConfirmation,
            process_id,
            launch: TerminalLaunch {
                command: shell,
                args: Vec::new(),
                cwd,
                env: Vec::new(),
            },
            pending_create: PendingServiceCreate,
            metadata: PageMetadata {
                title: format!("Terminal ({})", &process_id.to_string()[..8]),
                url: TerminalPlugin::URL.to_string(),
                icon: PageIcon::None,
                bg_color: None,
            },
            webview: WebviewWindowed,
            page_host: HostsPage,
            editing_chords: BindsEditingChords,
            size: WebviewSize(Vec2::new(1280.0, 720.0)),
            grid_size: TerminalGridSize::default(),
            transform: Transform::default(),
            node: Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            visibility: Visibility::Visible,
        }
    }
}

fn respond_stack_spawn(
    mut reader: MessageReader<TerminalStackSpawnRequest>,
    settings: Res<AppSettings>,
    mut terminal_inputs: MessageWriter<QueueTerminalInput>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack_ts = if request.activate {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        let stack = commands
            .spawn((Stack::bundle(), stack_ts, ChildOf(request.pane)))
            .id();
        let title = request
            .cwd
            .as_ref()
            .map(|cwd| format!("Terminal ({})", cwd.display()))
            .unwrap_or_else(|| "Terminal".to_string());
        commands.entity(stack).insert(PageMetadata {
            url: TerminalPlugin::URL.to_string(),
            title,
            bg_color: Some(TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let terminal = commands
            .spawn((
                TerminalBundle::with_shell(
                    &settings,
                    request.cwd.as_deref(),
                    request.shell.as_deref(),
                ),
                ChildOf(stack),
            ))
            .id();
        commands.entity(terminal).insert(KeyboardOwner);
        if request.agent_run {
            commands.entity(terminal).insert(crate::AgentRunTerminal);
        }
        if let Some(pid) = request.process_id {
            commands.entity(terminal).insert(pid);
        }
        if let Some(data) = request.pending_input.clone() {
            terminal_inputs.write(QueueTerminalInput { terminal, data });
        }
    }
}

#[derive(Bundle)]
pub struct ReattachedTerminalBundle {
    terminal: Terminal,
    browser: Browser,
    close_confirmation: CloseRequiresConfirmation,
    process_id: ProcessId,
    pending_attach: PendingServiceAttach,
    metadata: PageMetadata,
    webview: WebviewWindowed,
    page_host: HostsPage,
    editing_chords: BindsEditingChords,
    size: WebviewSize,
    grid_size: TerminalGridSize,
    transform: Transform,
    node: Node,
    visibility: Visibility,
}

impl ReattachedTerminalBundle {
    pub fn new(process_id: ProcessId) -> Self {
        Self {
            terminal: Terminal,
            browser: Browser,
            close_confirmation: CloseRequiresConfirmation,
            process_id,
            pending_attach: PendingServiceAttach,
            metadata: PageMetadata {
                title: format!("Terminal ({})", &process_id.to_string()[..8]),
                url: TerminalPlugin::URL.to_string(),
                icon: PageIcon::None,
                bg_color: None,
            },
            webview: WebviewWindowed,
            page_host: HostsPage,
            editing_chords: BindsEditingChords,
            size: WebviewSize(Vec2::new(1280.0, 720.0)),
            grid_size: TerminalGridSize::default(),
            transform: Transform::default(),
            node: Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            visibility: Visibility::Visible,
        }
    }
}

#[derive(Component)]
pub struct PendingServiceCreate;

#[derive(Component)]
struct PendingServiceAttach;

#[derive(Component)]
pub(crate) struct ShellOutputSeen;

impl ShellOutputSeen {
    fn ready(has_content: bool, cursor_col: u16) -> bool {
        has_content && cursor_col > 0
    }
}

#[derive(Component)]
pub struct AwaitingProcessCreated;

#[derive(EntityEvent)]
pub struct TerminalRestartRequest {
    #[event_target]
    pub terminal: Entity,
}

fn restart(trigger: On<TerminalRestartRequest>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .remove::<ShellOutputSeen>()
        .insert((
            AwaitingProcessCreated,
            TerminalMode::default(),
            TerminalCopyMode::default(),
            TerminalShortcutState::default(),
            TerminalMouseState::default(),
        ));
}

const MAX_CONCURRENT_PROCESS_CREATES: usize = 8;

impl PendingServiceCreate {
    fn budget(in_flight: usize, max_concurrent: usize) -> usize {
        max_concurrent.saturating_sub(in_flight)
    }
}

impl TerminalServiceError {
    fn missing_process_id(&self) -> Option<ProcessId> {
        self.message
            .strip_prefix("process not found: ")
            .and_then(|id| id.parse().ok())
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct TerminalServiceStatus<'w, 's> {
    terminals: Query<'w, 's, Entity, With<Terminal>>,
    commands: Commands<'w, 's>,
}

impl TerminalServiceStatus<'_, '_> {
    fn broadcast(&mut self, message: String) {
        let event = ServiceUnavailableEvent { message };
        for entity in &self.terminals {
            self.commands
                .trigger(UiStateWrite::<TerminalUiState>::from_event(entity, &event));
        }
    }
}

fn publish_service_status(
    connected: Query<(), Added<ServiceConnected>>,
    unavailable: Query<&ServiceUnavailable, Changed<ServiceUnavailable>>,
    mut status: TerminalServiceStatus,
) {
    if !connected.is_empty() {
        status.broadcast(String::new());
    }
    for unavailable in &unavailable {
        status.broadcast(unavailable.0.clone());
    }
}

fn resolve_pending_cwd(
    mut pending: Query<(Entity, &mut TerminalLaunch), (With<Terminal>, With<PendingServiceCreate>)>,
    tabs: TabHierarchy,
    space_hierarchy: vmux_layout::space::SpaceHierarchy,
    settings: Res<AppSettings>,
    active_space: FocusedSpace,
) {
    for (entity, mut launch) in &mut pending {
        if !launch.cwd.is_empty() {
            continue;
        }
        let tab_dir = tabs.startup_dir(entity);
        let space_id = space_hierarchy
            .id(entity)
            .or_else(|| active_space.id().map(str::to_string))
            .unwrap_or_else(|| BOOTSTRAP_SPACE_ID.to_string());
        let Ok(Some(cwd)) = settings.workspace_dir(&space_id, tab_dir.as_deref()) else {
            continue;
        };
        launch.cwd = cwd.to_string_lossy().into_owned();
    }
}

fn send_service_requests(
    pending_create: Query<
        (
            Entity,
            &ProcessId,
            &TerminalLaunch,
            Has<crate::AgentRunTerminal>,
        ),
        (With<Terminal>, With<PendingServiceCreate>),
    >,
    pending_attach: Query<(Entity, &ProcessId), (With<Terminal>, With<PendingServiceAttach>)>,
    awaiting_create: Query<(), (With<Terminal>, With<AwaitingProcessCreated>)>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
    settings: Res<AppSettings>,
) {
    if connected.is_none() {
        return;
    }

    let create_budget = PendingServiceCreate::budget(
        awaiting_create.iter().count(),
        MAX_CONCURRENT_PROCESS_CREATES,
    );
    for (entity, process_id, launch, agent_run) in pending_create.iter().take(create_budget) {
        let mut env = launch.env.clone();
        if agent_run {
            super::LoginShellEnvironment::merge(
                &mut env,
                &TerminalBundle::default_shell(&settings),
            );
        }
        service_requests.write(ServiceRequest(ClientMessage::CreateProcess {
            process_id: *process_id,
            command: launch.command.clone(),
            args: launch.args.clone(),
            cwd: launch.cwd.clone(),
            env,
            cols: 80,
            rows: 24,
        }));
        commands
            .entity(entity)
            .remove::<PendingServiceCreate>()
            .insert(AwaitingProcessCreated);
    }

    for (entity, pid) in &pending_attach {
        service_requests.write(ServiceRequest(ClientMessage::AttachProcess {
            process_id: *pid,
        }));
        service_requests.write(ServiceRequest(ClientMessage::RequestSnapshot {
            process_id: *pid,
        }));
        commands.entity(entity).remove::<PendingServiceAttach>();
    }
}

fn apply_process_start(
    mut created: MessageReader<TerminalProcessCreated>,
    mut failed: MessageReader<TerminalProcessCreateFailed>,
    awaiting_create: Query<(), (With<Terminal>, With<AwaitingProcessCreated>)>,
    process_index: Single<&TerminalProcessIndex>,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for created in created.read() {
        let entity = process_index
            .get(&created.process_id)
            .filter(|entity| awaiting_create.contains(*entity));
        if let Some(entity) = entity {
            service_requests.write(ServiceRequest(ClientMessage::AttachProcess {
                process_id: created.process_id,
            }));
            commands
                .entity(entity)
                .insert(created.process_id)
                .insert(pid::Pid(created.pid))
                .remove::<AwaitingProcessCreated>();
        } else {
            bevy::log::warn!(
                "ProcessCreated for unknown process_id {}; dropping",
                created.process_id
            );
        }
    }

    for failed in failed.read() {
        bevy::log::warn!("service failed to create process: {}", failed.reason);
        if let Some(entity) = process_index
            .get(&failed.process_id)
            .filter(|entity| awaiting_create.contains(*entity))
        {
            commands.entity(entity).despawn();
        }
    }
}

fn apply_viewport_updates(
    mut updates: MessageReader<TerminalViewportUpdate>,
    terminals: Query<(), ServiceTerminalFilter>,
    process_index: Single<&TerminalProcessIndex>,
    output_seen: Query<(), With<ShellOutputSeen>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for update in updates.read() {
        let Some(entity) = process_index.get(&update.process_id) else {
            continue;
        };
        if !terminals.contains(entity) {
            continue;
        }
        if !output_seen.contains(entity) {
            let has_content = update
                .patch
                .changed_lines
                .iter()
                .any(|(_, line)| line.spans.iter().any(|span| !span.text.trim().is_empty()));
            if ShellOutputSeen::ready(has_content, update.patch.cursor.col) {
                commands.entity(entity).insert(ShellOutputSeen);
            }
        }
        if !browsers.can_emit_to(&entity) {
            if update.request_snapshot_if_hidden {
                commands.entity(entity).insert(PendingTerminalSnapshot);
            }
            continue;
        }
        let mut patch = update.patch.clone();
        for (_, line) in patch.changed_lines.iter_mut() {
            super::link::LinkDetector::new(None).annotate(line);
        }
        commands.trigger(UiStateWrite::<TerminalUiState>::from_event(entity, &patch));
    }
}

fn apply_process_exits(
    mut exited: MessageReader<ProcessExitedEvent>,
    terminals: Query<
        (Entity, &ProcessId, &ChildOf, Has<RetainOnProcessExit>),
        ServiceTerminalFilter,
    >,
    mut terminal_states: Query<
        (
            &mut TerminalMode,
            &mut TerminalCopyMode,
            &mut TerminalShortcutState,
            &mut TerminalMouseState,
        ),
        With<Terminal>,
    >,
    process_index: Single<&TerminalProcessIndex>,
    mut stack_close_requests: MessageWriter<StackCloseRequest>,
    mut commands: Commands,
) {
    for exited in exited.read() {
        let Some(entity) = process_index.get(&exited.process_id) else {
            continue;
        };
        if let Ok((mut mode, mut copy_mode, mut shortcut, mut mouse)) =
            terminal_states.get_mut(entity)
        {
            *mode = TerminalMode::default();
            *copy_mode = TerminalCopyMode::default();
            *shortcut = TerminalShortcutState::default();
            *mouse = TerminalMouseState::default();
        }
        let Ok((_, _, child_of, retain_on_exit)) = terminals.get(entity) else {
            continue;
        };
        commands
            .entity(entity)
            .insert(ProcessExited)
            .remove::<CloseRequiresConfirmation>()
            .remove::<super::loading::ShellLoading>();
        if should_close_terminal_stack_on_exit(retain_on_exit) {
            let tab = child_of.get();
            commands.entity(tab).insert(LastActivatedAt::now());
            stack_close_requests.write(StackCloseRequest);
        }
    }
}

fn apply_service_errors(
    mut errors: MessageReader<TerminalServiceError>,
    terminals: Query<(), ServiceTerminalFilter>,
    process_index: Single<&TerminalProcessIndex>,
    launches: Query<&TerminalLaunch>,
    settings: Res<AppSettings>,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    let mut restarted_missing_processes = Vec::new();
    for error in errors.read() {
        if let Some(stale_pid) = error.missing_process_id()
            && !restarted_missing_processes.contains(&stale_pid)
            && let Some(entity) = process_index.get(&stale_pid)
            && terminals.contains(entity)
        {
            let launch = launches
                .get(entity)
                .cloned()
                .unwrap_or_else(|_| TerminalLaunch {
                    command: TerminalBundle::default_shell(&settings),
                    args: vec![],
                    cwd: String::new(),
                    env: vec![],
                });
            let new_id = ProcessId::new();
            restarted_missing_processes.push(stale_pid);
            service_requests.write(ServiceRequest(ClientMessage::CreateProcess {
                process_id: new_id,
                command: launch.command,
                args: launch.args,
                cwd: launch.cwd,
                env: launch.env,
                cols: 80,
                rows: 24,
            }));
            commands.entity(entity).insert(new_id);
            commands.trigger(TerminalRestartRequest { terminal: entity });
        }
        warn!("Service error: {}", error.message);
    }
}

fn copy_service_selection(
    mut selections: MessageReader<TerminalSelectionText>,
    process_index: Single<&TerminalProcessIndex>,
) {
    for selection in selections.read() {
        if process_index.get(&selection.process_id).is_some() && !selection.text.is_empty() {
            Clipboard::write(selection.text.clone());
        }
    }
}

type ServiceTerminalFilter = (
    With<Terminal>,
    Or<(Without<ProcessExited>, With<RetainOnProcessExit>)>,
    Without<AwaitingProcessCreated>,
);

fn should_close_terminal_stack_on_exit(retain_on_exit: bool) -> bool {
    !retain_on_exit
}

#[cfg(test)]
fn map_copy_mode_key(key: &Key, ctrl: bool) -> Option<CopyModeKey> {
    map_copy_mode_key_from_input(CopyModeKeyInput {
        key,
        key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
        ctrl,
        shift: false,
    })
}

fn map_copy_mode_key_from_input(input: CopyModeKeyInput<'_>) -> Option<CopyModeKey> {
    match (input.key, input.ctrl) {
        (Key::ArrowLeft, _) => Some(K::Left),
        (Key::ArrowRight, _) => Some(K::Right),
        (Key::ArrowUp, _) => Some(K::Up),
        (Key::ArrowDown, _) => Some(K::Down),
        (Key::Enter, _) => Some(K::Copy),
        (Key::Escape, _) => Some(K::Exit),
        (Key::Home, _) => Some(K::LineStart),
        (Key::End, _) => Some(K::LineEnd),
        (Key::PageUp, _) => Some(K::PageUp),
        (Key::PageDown, _) => Some(K::PageDown),
        _ if input.ctrl && key_char_eq(input, 'u') => Some(K::PageUp),
        _ if input.ctrl && key_char_eq(input, 'd') => Some(K::PageDown),
        _ if input.ctrl && key_char_eq(input, 'e') => Some(K::Down),
        _ if input.ctrl && key_char_eq(input, 'y') => Some(K::Up),
        _ if input.ctrl && key_char_eq(input, 'b') => Some(K::PageUp),
        _ if input.ctrl && key_char_eq(input, 'f') => Some(K::PageDown),
        _ if input.ctrl && key_char_eq(input, 'c') => Some(K::Exit),
        _ if key_char_eq(input, 'h') => Some(K::Left),
        _ if key_char_eq(input, 'j') => Some(K::Down),
        _ if key_char_eq(input, 'k') => Some(K::Up),
        _ if key_char_eq(input, 'l') => Some(K::Right),
        _ if key_char_eq(input, '0') => Some(K::LineStart),
        _ if key_char_eq(input, '$') => Some(K::LineEnd),
        _ if key_char_eq(input, '^') => Some(K::FirstNonBlank),
        _ if key_char_eq(input, 'w') => Some(K::WordForward),
        _ if key_char_eq(input, 'W') => Some(K::BigWordForward),
        _ if key_char_eq(input, 'b') => Some(K::WordBackward),
        _ if key_char_eq(input, 'B') => Some(K::BigWordBackward),
        _ if key_char_eq(input, 'e') => Some(K::WordEndForward),
        _ if key_char_eq(input, 'E') => Some(K::BigWordEndForward),
        _ if key_char_eq(input, 'G') => Some(K::Bottom),
        _ if key_char_eq(input, 'H') => Some(K::ScreenTop),
        _ if key_char_eq(input, 'M') => Some(K::ScreenMiddle),
        _ if key_char_eq(input, 'L') => Some(K::ScreenBottom),
        _ if key_char_eq(input, '{') => Some(K::PrevParagraph),
        _ if key_char_eq(input, '}') => Some(K::NextParagraph),
        _ if key_char_eq(input, ';') => Some(K::RepeatFind),
        _ if key_char_eq(input, ',') => Some(K::RepeatFindReverse),
        _ if key_char_eq(input, 'o') => Some(K::SwapSelectionEnds),
        _ if key_char_eq(input, 'v') => Some(K::StartSelection),
        _ if key_char_eq(input, 'V') => Some(K::StartLineSelection),
        _ if key_char_eq(input, 'y') => Some(K::Copy),
        _ if key_char_eq(input, 'q') => Some(K::Exit),
        _ => None,
    }
}

#[cfg(test)]
fn map_copy_mode_key_with_state(
    copy_mode: &mut TerminalCopyMode,
    key: &Key,
    ctrl: bool,
) -> Option<CopyModeKey> {
    map_copy_mode_keys_with_state(
        copy_mode,
        CopyModeKeyInput {
            key,
            key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
            ctrl,
            shift: false,
        },
    )
    .into_iter()
    .next()
}

fn map_copy_mode_keys_with_state(
    copy_mode: &mut TerminalCopyMode,
    input: CopyModeKeyInput<'_>,
) -> Vec<CopyModeKey> {
    let state = &mut copy_mode.input;
    if let Some(pending) = state.pending_key.take() {
        let key = match pending {
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, '_') => {
                Some(K::LastNonBlank)
            }
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, 'g') => Some(K::Top),
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, 'e') => {
                Some(K::WordEndBackward)
            }
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, 'E') => {
                Some(K::BigWordEndBackward)
            }
            CopyModePendingKey::FindForward => input_char(input).map(K::FindForward),
            CopyModePendingKey::FindBackward => input_char(input).map(K::FindBackward),
            CopyModePendingKey::TillForward => input_char(input).map(K::TillForward),
            CopyModePendingKey::TillBackward => input_char(input).map(K::TillBackward),
            _ => None,
        };
        if let Some(key) = key {
            return repeat_copy_mode_key(state, key);
        }
    }

    if let Some(digit) = input_digit(input)
        && (!matches!(digit, 0) || state.count.is_some())
        && !input.ctrl
    {
        let current = state.count.unwrap_or(0);
        state.count = Some(current.saturating_mul(10).saturating_add(digit).min(999));
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'g') {
        state.pending_key = Some(CopyModePendingKey::G);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'f') {
        state.pending_key = Some(CopyModePendingKey::FindForward);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'F') {
        state.pending_key = Some(CopyModePendingKey::FindBackward);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 't') {
        state.pending_key = Some(CopyModePendingKey::TillForward);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'T') {
        state.pending_key = Some(CopyModePendingKey::TillBackward);
        return Vec::new();
    }

    map_copy_mode_key_from_input(input)
        .map(|key| repeat_copy_mode_key(state, key))
        .unwrap_or_default()
}

fn repeat_copy_mode_key(state: &mut CopyModeInputState, key: CopyModeKey) -> Vec<CopyModeKey> {
    let repeat = if copy_mode_key_uses_count(key) {
        state.count.take().unwrap_or(1)
    } else {
        state.count = None;
        1
    };
    vec![key; repeat as usize]
}

fn copy_mode_key_uses_count(key: CopyModeKey) -> bool {
    !matches!(
        key,
        K::StartSelection | K::StartLineSelection | K::Copy | K::Exit
    )
}

fn input_char(input: CopyModeKeyInput<'_>) -> Option<char> {
    match input.key {
        Key::Character(s) => s.chars().next(),
        _ => None,
    }
}

fn input_digit(input: CopyModeKeyInput<'_>) -> Option<u16> {
    let c = input_char(input)?;
    c.to_digit(10).map(|d| d as u16)
}

fn key_char_eq(input: CopyModeKeyInput<'_>, expected: char) -> bool {
    if input_char(input) == Some(expected) {
        return true;
    }
    match expected {
        '_' => input.shift && input.key_code == KeyCode::Minus,
        '$' => input.shift && input.key_code == KeyCode::Digit4,
        '^' => input.shift && input.key_code == KeyCode::Digit6,
        '{' => input.shift && input.key_code == KeyCode::BracketLeft,
        '}' => input.shift && input.key_code == KeyCode::BracketRight,
        'W' => input.shift && input.key_code == KeyCode::KeyW,
        'B' => input.shift && input.key_code == KeyCode::KeyB,
        'E' => input.shift && input.key_code == KeyCode::KeyE,
        'G' => input.shift && input.key_code == KeyCode::KeyG,
        'H' => input.shift && input.key_code == KeyCode::KeyH,
        'M' => input.shift && input.key_code == KeyCode::KeyM,
        'L' => input.shift && input.key_code == KeyCode::KeyL,
        'F' => input.shift && input.key_code == KeyCode::KeyF,
        'T' => input.shift && input.key_code == KeyCode::KeyT,
        'V' => input.shift && input.key_code == KeyCode::KeyV,
        _ => false,
    }
}

fn resolve_terminal_input_targets(
    targeted_terminal_ids_by_stack: impl IntoIterator<Item = (Entity, ProcessId)>,
    any_keyboard_target_active: bool,
    focused_stack: Option<Entity>,
    terminal_ids_by_stack: impl IntoIterator<Item = (Entity, ProcessId)>,
) -> Vec<ProcessId> {
    let targeted: Vec<(Entity, ProcessId)> = targeted_terminal_ids_by_stack.into_iter().collect();
    let focused = focused_stack.and_then(|focused_stack| {
        let focused: Vec<ProcessId> = terminal_ids_by_stack
            .into_iter()
            .filter_map(|(stack, process_id)| (stack == focused_stack).then_some(process_id))
            .collect();
        (!focused.is_empty()).then_some(focused)
    });
    if !targeted.is_empty() {
        if let Some(focused_stack) = focused_stack {
            let focused: Vec<ProcessId> = targeted
                .iter()
                .filter_map(|(stack, process_id)| (*stack == focused_stack).then_some(*process_id))
                .collect();
            if !focused.is_empty() {
                return focused;
            }
        }
        if let Some(focused) = focused {
            return focused;
        }
        if focused_stack.is_some() {
            return Vec::new();
        }
        return targeted
            .into_iter()
            .map(|(_, process_id)| process_id)
            .collect();
    }
    if any_keyboard_target_active {
        return Vec::new();
    }
    focused.unwrap_or_default()
}

struct TerminalInput;

impl TerminalInput {
    fn bytes_for_key(key: &Key, ctrl: bool, alt: bool) -> Vec<u8> {
        match key {
            Key::Character(s) => {
                if ctrl && let Some(c) = s.chars().next() {
                    let code = (c.to_ascii_lowercase() as u8)
                        .wrapping_sub(b'a')
                        .wrapping_add(1);
                    if code <= 26 {
                        let mut v = Vec::new();
                        if alt {
                            v.push(0x1b);
                        }
                        v.push(code);
                        return v;
                    }
                }
                if alt {
                    let mut v = vec![0x1b];
                    v.extend_from_slice(s.as_bytes());
                    return v;
                }
                s.as_bytes().to_vec()
            }
            Key::Enter => b"\r".to_vec(),
            Key::Backspace => {
                if ctrl {
                    vec![0x08]
                } else {
                    vec![0x7f]
                }
            }
            Key::Tab => b"\t".to_vec(),
            Key::Escape => vec![0x1b],
            Key::Space => {
                if ctrl {
                    let mut v = Vec::new();
                    if alt {
                        v.push(0x1b);
                    }
                    v.push(0);
                    return v;
                }
                b" ".to_vec()
            }
            Key::ArrowUp => b"\x1b[A".to_vec(),
            Key::ArrowDown => b"\x1b[B".to_vec(),
            Key::ArrowRight => b"\x1b[C".to_vec(),
            Key::ArrowLeft => b"\x1b[D".to_vec(),
            Key::Home => b"\x1b[H".to_vec(),
            Key::End => b"\x1b[F".to_vec(),
            Key::PageUp => b"\x1b[5~".to_vec(),
            Key::PageDown => b"\x1b[6~".to_vec(),
            Key::Delete => b"\x1b[3~".to_vec(),
            Key::Insert => b"\x1b[2~".to_vec(),
            _ => Vec::new(),
        }
    }

    fn key(event: &KeyStroke) -> Key {
        match event.key.as_str() {
            "Enter" => Key::Enter,
            "Backspace" => Key::Backspace,
            "Tab" => Key::Tab,
            "Escape" | "Esc" => Key::Escape,
            " " | "Space" => Key::Space,
            "ArrowUp" => Key::ArrowUp,
            "ArrowDown" => Key::ArrowDown,
            "ArrowRight" => Key::ArrowRight,
            "ArrowLeft" => Key::ArrowLeft,
            "Home" => Key::Home,
            "End" => Key::End,
            "PageUp" => Key::PageUp,
            "PageDown" => Key::PageDown,
            "Delete" => Key::Delete,
            "Insert" => Key::Insert,
            _ => Key::Character(event.typed_text().into()),
        }
    }

    fn bracketed(payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::with_capacity(payload.len() + 12);
        data.extend_from_slice(b"\x1b[200~");
        data.extend_from_slice(payload);
        data.extend_from_slice(b"\x1b[201~");
        data
    }

    fn paste() -> Option<Vec<u8>> {
        if let Some(path) = Clipboard::image_file_path() {
            return Some(Self::bracketed(path.as_bytes()));
        }
        if Clipboard::has_image() {
            return Some(vec![CTRL_V]);
        }
        let text = Clipboard::read_text()?;
        (!text.is_empty()).then(|| Self::bracketed(text.as_bytes()))
    }

    fn bytes(event: &KeyStroke) -> Vec<u8> {
        if event.is_modifier_key() {
            return Vec::new();
        }
        let key = Self::key(event);
        Self::bytes_for_key(&key, event.mods.ctrl, event.mods.alt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalWebShortcutResolution {
    Command(String),
    Consume,
    PassThrough,
}

impl TerminalShortcutState {
    fn resolve(&mut self, event: &KeyStroke, map: &Keymap) -> TerminalWebShortcutResolution {
        let Some(combo) = TerminalInput::shortcut(event) else {
            return TerminalWebShortcutResolution::PassThrough;
        };
        let now = Instant::now();
        if let Some((_, started)) = self.pending_prefix.as_ref()
            && now.duration_since(*started) > Duration::from_millis(map.chord_timeout_ms)
        {
            self.pending_prefix = None;
        }

        if let Some((prefix, _)) = self.pending_prefix.clone() {
            if let Some(cmd) = map.chord(&prefix, &combo) {
                self.pending_prefix = None;
                return TerminalWebShortcutResolution::Command(cmd);
            }
            self.pending_prefix = None;
        }

        if let Some(cmd) = map.direct(&combo)
            && (combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key)
        {
            return TerminalWebShortcutResolution::Command(cmd);
        }

        if map.has_chord_prefix(&combo) {
            self.pending_prefix = Some((combo, now));
            return TerminalWebShortcutResolution::Consume;
        }

        TerminalWebShortcutResolution::PassThrough
    }
}

impl TerminalInput {
    fn shortcut(event: &KeyStroke) -> Option<KeyCombo> {
        if event.is_modifier_key() {
            return None;
        }
        let key = Self::shortcut_code(&event.code)?;
        Some(KeyCombo {
            key,
            modifiers: Modifiers {
                ctrl: event.mods.ctrl,
                shift: event.mods.shift,
                alt: event.mods.alt,
                super_key: event.mods.super_key,
            },
        })
    }

    fn shortcut_code(code: &str) -> Option<KeyCode> {
        let key = Self::key_code(code);
        if matches!(
            key,
            KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified)
        ) {
            None
        } else {
            Some(key)
        }
    }

    fn key_code(code: &str) -> KeyCode {
        match code {
            "KeyA" => KeyCode::KeyA,
            "KeyB" => KeyCode::KeyB,
            "KeyC" => KeyCode::KeyC,
            "KeyD" => KeyCode::KeyD,
            "KeyE" => KeyCode::KeyE,
            "KeyF" => KeyCode::KeyF,
            "KeyG" => KeyCode::KeyG,
            "KeyH" => KeyCode::KeyH,
            "KeyI" => KeyCode::KeyI,
            "KeyJ" => KeyCode::KeyJ,
            "KeyK" => KeyCode::KeyK,
            "KeyL" => KeyCode::KeyL,
            "KeyM" => KeyCode::KeyM,
            "KeyN" => KeyCode::KeyN,
            "KeyO" => KeyCode::KeyO,
            "KeyP" => KeyCode::KeyP,
            "KeyQ" => KeyCode::KeyQ,
            "KeyR" => KeyCode::KeyR,
            "KeyS" => KeyCode::KeyS,
            "KeyT" => KeyCode::KeyT,
            "KeyU" => KeyCode::KeyU,
            "KeyV" => KeyCode::KeyV,
            "KeyW" => KeyCode::KeyW,
            "KeyX" => KeyCode::KeyX,
            "KeyY" => KeyCode::KeyY,
            "KeyZ" => KeyCode::KeyZ,
            "Digit0" => KeyCode::Digit0,
            "Digit1" => KeyCode::Digit1,
            "Digit2" => KeyCode::Digit2,
            "Digit3" => KeyCode::Digit3,
            "Digit4" => KeyCode::Digit4,
            "Digit5" => KeyCode::Digit5,
            "Digit6" => KeyCode::Digit6,
            "Digit7" => KeyCode::Digit7,
            "Digit8" => KeyCode::Digit8,
            "Digit9" => KeyCode::Digit9,
            "Equal" => KeyCode::Equal,
            "Minus" => KeyCode::Minus,
            "Period" => KeyCode::Period,
            "Comma" => KeyCode::Comma,
            "Quote" => KeyCode::Quote,
            "Semicolon" => KeyCode::Semicolon,
            "Slash" => KeyCode::Slash,
            "Backslash" => KeyCode::Backslash,
            "Backquote" => KeyCode::Backquote,
            "BracketLeft" => KeyCode::BracketLeft,
            "BracketRight" => KeyCode::BracketRight,
            "Enter" => KeyCode::Enter,
            "Space" => KeyCode::Space,
            "Tab" => KeyCode::Tab,
            "Backspace" => KeyCode::Backspace,
            "Delete" => KeyCode::Delete,
            "Insert" => KeyCode::Insert,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            "ArrowUp" => KeyCode::ArrowUp,
            "ArrowDown" => KeyCode::ArrowDown,
            "ArrowLeft" => KeyCode::ArrowLeft,
            "ArrowRight" => KeyCode::ArrowRight,
            "Escape" => KeyCode::Escape,
            "F1" => KeyCode::F1,
            "F2" => KeyCode::F2,
            "F3" => KeyCode::F3,
            "F4" => KeyCode::F4,
            "F5" => KeyCode::F5,
            "F6" => KeyCode::F6,
            "F7" => KeyCode::F7,
            "F8" => KeyCode::F8,
            "F9" => KeyCode::F9,
            "F10" => KeyCode::F10,
            "F11" => KeyCode::F11,
            "F12" => KeyCode::F12,
            _ => KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
        }
    }
}

fn term_key(
    trigger: On<UiInput<KeyStroke>>,
    mut terminals: Query<
        (
            &ProcessId,
            &TerminalMode,
            &mut TerminalCopyMode,
            &mut TerminalShortcutState,
        ),
        With<Terminal>,
    >,
    keymap: Single<&Keymap>,
    mut command_invocations: MessageWriter<CommandInvocation>,
    user_q: Query<Entity, With<vmux_ecs::team::User>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Ok((pid, mode, mut copy_mode, mut shortcuts)) = terminals.get_mut(entity) else {
        return;
    };
    match shortcuts.resolve(event, &keymap) {
        TerminalWebShortcutResolution::Command(id) => {
            let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
            command_invocations.write(CommandInvocation::new(caller, id));
            if let Some(proxy) = proxy.as_ref() {
                let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
            }
            return;
        }
        TerminalWebShortcutResolution::Consume => return,
        TerminalWebShortcutResolution::PassThrough => {}
    }
    if event.is_modifier_key() {
        return;
    }
    let process_id = *pid;
    let super_key = event.mods.super_key;
    if super_key {
        match event.code.as_str() {
            "KeyV" => {
                if let Some(data) = TerminalInput::paste() {
                    service_requests.write(ServiceRequest(ClientMessage::ProcessInput {
                        process_id,
                        data,
                    }));
                }
                return;
            }
            "KeyC" => {
                service_requests.write(ServiceRequest(ClientMessage::GetSelectionText {
                    process_id,
                }));
                return;
            }
            _ => return,
        }
    }

    if is_copy_mode_active(mode, &copy_mode) {
        let key = TerminalInput::key(event);
        let mapped = map_copy_mode_keys_with_state(
            &mut copy_mode,
            CopyModeKeyInput {
                key: &key,
                key_code: TerminalInput::key_code(&event.code),
                ctrl: event.mods.ctrl,
                shift: event.mods.shift,
            },
        );
        for k in mapped {
            if copy_mode_key_exits(k) {
                copy_mode.set(false);
            }
            service_requests.write(ServiceRequest(ClientMessage::CopyModeKey {
                process_id,
                key: k,
            }));
        }
        return;
    }

    let data = TerminalInput::bytes(event);
    if !data.is_empty() {
        service_requests.write(ServiceRequest(ClientMessage::ProcessInput {
            process_id,
            data,
        }));
    }
}

fn restart_pty(
    trigger: On<RestartPty>,
    mut q: Query<(
        &mut ProcessId,
        &mut PageMetadata,
        Option<&mut TerminalLaunch>,
        Option<&TerminalGridSize>,
        Has<crate::AgentRunTerminal>,
    )>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event().entity;
    let Ok((mut pid, mut meta, mut launch, grid, agent_run)) = q.get_mut(entity) else {
        return;
    };

    service_requests.write(ServiceRequest(ClientMessage::KillProcess {
        process_id: *pid,
    }));

    let (command, args, cwd, mut env) = match launch.as_deref() {
        Some(l) => (
            l.command.clone(),
            l.args.clone(),
            l.cwd.clone(),
            l.env.clone(),
        ),
        None => {
            let shell = settings
                .terminal
                .as_ref()
                .map(|t| t.resolve_theme(&t.default_theme).shell)
                .unwrap_or_else(TerminalTheme::default_shell);
            (shell, vec![], String::new(), Vec::new())
        }
    };
    if agent_run {
        super::LoginShellEnvironment::merge(&mut env, &TerminalBundle::default_shell(&settings));
    }

    let (cols, rows) = grid.map(|g| (g.cols, g.rows)).unwrap_or((80, 24));
    let new_id = ProcessId::new();
    service_requests.write(ServiceRequest(ClientMessage::CreateProcess {
        process_id: new_id,
        command: command.clone(),
        args: args.clone(),
        cwd: cwd.clone(),
        env: env.clone(),
        cols,
        rows,
    }));

    *pid = new_id;
    commands.trigger(TerminalRestartRequest { terminal: entity });
    if let Some(l) = launch.as_mut() {
        l.args = args;
    } else {
        meta.url = TerminalPlugin::URL.to_string();
        meta.title = format!("Terminal ({})", &new_id.to_string()[..8]);
    }
}

fn enter_copy_mode(
    mut requests: MessageReader<super::command::CopyModeRequest>,
    targeted_terminals: Query<
        (&ProcessId, &ChildOf),
        (With<Terminal>, With<KeyboardOwner>, Without<ProcessExited>),
    >,
    keyboard_targets: Query<(), With<KeyboardOwner>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: FocusedStack,
    process_index: Single<&TerminalProcessIndex>,
    mut copy_modes: Query<&mut TerminalCopyMode, With<Terminal>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals
            .iter()
            .map(|(pid, child_of)| (child_of.get(), *pid)),
        !keyboard_targets.is_empty(),
        focus.stack,
        terminals
            .iter()
            .map(|(pid, child_of)| (child_of.get(), *pid)),
    );
    let active_process_id = target_processes.first().copied();
    for _ in requests.read() {
        if let Some(process_id) = active_process_id {
            if let Some(entity) = process_index.get(&process_id)
                && let Ok(mut copy_mode) = copy_modes.get_mut(entity)
            {
                copy_mode.set(true);
            }
            service_requests.write(ServiceRequest(ClientMessage::EnterCopyMode { process_id }));
        }
    }
}

fn navigate_terminals(
    mut close_requests: MessageReader<super::command::TerminalCloseRequest>,
    mut next_requests: MessageReader<super::command::TerminalNextRequest>,
    mut previous_requests: MessageReader<super::command::TerminalPrevRequest>,
    focus: FocusedStack,
    terminals: Query<&ChildOf, With<Terminal>>,
    mut stack_close_requests: MessageWriter<StackCloseRequest>,
    mut stack_focus_requests: MessageWriter<FocusRequest>,
) {
    let terminal_is_focused = focus
        .stack
        .is_some_and(|stack| terminals.iter().any(|child_of| child_of.get() == stack));
    if !terminal_is_focused {
        close_requests.clear();
        next_requests.clear();
        previous_requests.clear();
        return;
    }
    for _ in close_requests.read() {
        stack_close_requests.write(StackCloseRequest);
    }
    for _ in next_requests.read() {
        stack_focus_requests.write(FocusRequest(vmux_layout::target::SiblingDirection::Next));
    }
    for _ in previous_requests.read() {
        stack_focus_requests.write(FocusRequest(
            vmux_layout::target::SiblingDirection::Previous,
        ));
    }
}

fn clear(
    mut requests: MessageReader<super::command::TerminalClearRequest>,
    focus: FocusedStack,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut terminal_inputs: MessageWriter<QueueTerminalInput>,
) {
    let mut terminal = None;
    if let Some(stack) = focus.stack {
        for (entity, _, child_of) in &terminals {
            if child_of.get() == stack {
                terminal = Some(entity);
                break;
            }
        }
    }
    for _ in requests.read() {
        let Some(terminal) = terminal else {
            continue;
        };
        terminal_inputs.write(QueueTerminalInput {
            terminal,
            data: vec![0x0c],
        });
    }
}

fn is_copy_mode_active(mode: &TerminalMode, copy_mode: &TerminalCopyMode) -> bool {
    mode.copy_mode || copy_mode.active
}

fn copy_mode_key_exits(key: CopyModeKey) -> bool {
    matches!(key, K::Copy | K::Exit)
}

#[derive(Message, Debug, Clone)]
pub struct ProcessExitedEvent {
    pub process_id: ProcessId,
}

#[derive(Message, Debug, Clone)]
pub struct CommandLifecycleEvent {
    pub process_id: ProcessId,
    pub kind: vmux_api::protocol::CommandLifecycleKind,
}

#[derive(Message, Debug, Clone)]
pub struct TerminalReinputRequest {
    pub process_id: ProcessId,
    pub data: Vec<u8>,
}

#[derive(Message, Debug, Clone)]
pub struct OscTitleChanged {
    pub process_id: ProcessId,
    pub title: String,
}

fn apply_osc_title(
    mut reader: MessageReader<OscTitleChanged>,
    mut commands: Commands,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<Option<&PageIdentity>, With<Terminal>>,
    browsers: Option<NonSend<Browsers>>,
) {
    for ev in reader.read() {
        let Some(entity) = process_index.get(&ev.process_id) else {
            continue;
        };
        let Ok(current) = terminals.get(entity) else {
            continue;
        };
        if ev.title.is_empty() {
            if current.is_some() {
                commands.entity(entity).remove::<PageIdentity>();
            }
        } else if current.and_then(|identity| identity.title.as_deref()) != Some(ev.title.as_str())
        {
            commands
                .entity(entity)
                .insert(PageIdentity::from(ev.title.clone()));
        }
        if browsers
            .as_ref()
            .is_some_and(|browsers| browsers.can_emit_to(&entity))
        {
            commands.trigger(UiStateWrite::<TerminalUiState>::from_event(
                entity,
                &TermTitleEvent {
                    title: ev.title.clone(),
                },
            ));
        }
    }
}

fn clear_osc_title_on_exit(
    mut reader: MessageReader<ProcessExitedEvent>,
    mut commands: Commands,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<(), (With<Terminal>, With<PageIdentity>)>,
) {
    for ev in reader.read() {
        if let Some(entity) = process_index.get(&ev.process_id)
            && terminals.contains(entity)
        {
            commands.entity(entity).remove::<PageIdentity>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::Schedules;
    use vmux_api::input::KeyModifiers;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};

    use CopyModeKey as K;
    use bevy::ecs::message::Messages;

    fn spawn_active_space(app: &mut App, record: &SpaceRecord) {
        app.world_mut()
            .spawn((record.bundle(), vmux_layout::space::CurrentSpace));
    }

    #[test]
    fn bracketed_paste_wraps_payload() {
        assert_eq!(
            TerminalInput::bracketed(b"hi"),
            b"\x1b[200~hi\x1b[201~".to_vec()
        );
    }

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    #[test]
    fn terminal_reinput_preserves_existing_queued_input() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin));
        let pid = process_id(7);
        let terminal = app.world_mut().spawn((Terminal, pid)).id();
        app.world_mut().write_message(QueueTerminalInput {
            terminal,
            data: b"initial\r".to_vec(),
        });
        app.world_mut().write_message(TerminalReinputRequest {
            process_id: pid,
            data: b"next\r".to_vec(),
        });
        app.update();

        assert_eq!(
            pending_terminal_input(app.world_mut(), terminal),
            [b"initial\r".to_vec(), b"next\r".to_vec()]
        );
    }

    #[test]
    fn terminal_reinput_preserves_multiple_messages_in_order() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin));
        let pid = process_id(8);
        let terminal = app.world_mut().spawn((Terminal, pid)).id();

        app.world_mut()
            .resource_mut::<Messages<TerminalReinputRequest>>()
            .write(TerminalReinputRequest {
                process_id: pid,
                data: b"one\r".to_vec(),
            });
        app.world_mut()
            .resource_mut::<Messages<TerminalReinputRequest>>()
            .write(TerminalReinputRequest {
                process_id: pid,
                data: b"two\r".to_vec(),
            });
        app.update();

        assert_eq!(
            pending_terminal_input(app.world_mut(), terminal),
            [b"one\r".to_vec(), b"two\r".to_vec()]
        );
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

    #[test]
    fn terminal_send_resolves_target_by_process_id_uuid() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::host::request::TerminalRequestPlugin));
        app.world_mut()
            .spawn(vmux_layout::active_pane::ActiveStack::default().local_bundle());

        let parent = app.world_mut().spawn_empty().id();
        let pid = process_id(7);
        let terminal = app
            .world_mut()
            .spawn((Terminal, pid))
            .insert(ChildOf(parent))
            .id();

        app.world_mut()
            .resource_mut::<Messages<crate::TerminalSendRequest>>()
            .write(crate::TerminalSendRequest {
                text: "hi".to_string(),
                terminal: Some(pid.to_string()),
            });
        app.update();

        assert_eq!(
            pending_terminal_input(app.world_mut(), terminal),
            [b"hi".to_vec()]
        );
    }

    #[test]
    fn terminal_stack_spawn_uses_requested_shell() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(InputQueuePlugin)
            .add_message::<TerminalStackSpawnRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, respond_stack_spawn);

        let pane = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Messages<TerminalStackSpawnRequest>>()
            .write(TerminalStackSpawnRequest {
                pane,
                cwd: None,
                shell: Some("/bin/agent-sh".to_string()),
                agent_run: true,
                pending_input: None,
                process_id: None,
                activate: false,
            });
        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<(Entity, &TerminalLaunch), With<Terminal>>();
        let (terminal, launch) = launches.iter(app.world()).next().expect("terminal spawned");
        assert_eq!(launch.command, "/bin/agent-sh");
        assert!(
            app.world()
                .get::<crate::AgentRunTerminal>(terminal)
                .is_some()
        );
    }

    #[test]
    fn terminal_page_open_accepts_url_without_trailing_slash() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_systems(Update, handle_page_open);
        spawn_active_space(&mut app, &SpaceRecord::bootstrap());

        let stack = app.world_mut().spawn(Stack::bundle()).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://terminal".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let mut terminals = app.world_mut().query_filtered::<&ChildOf, With<Terminal>>();
        assert_eq!(
            terminals
                .iter(app.world())
                .filter(|child_of| child_of.get() == stack)
                .count(),
            1
        );
    }

    #[test]
    fn open_terminal_page_uses_per_space_startup_dir() {
        let dir = tempfile::tempdir().unwrap();
        let record = SpaceRecord::bootstrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .add_systems(Update, handle_page_open);
        spawn_active_space(&mut app, &record);

        let stack = app.world_mut().spawn(Stack::bundle()).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert_eq!(launch.cwd, dir.path().to_string_lossy());
    }

    #[test]
    fn open_terminal_page_without_workspace_uses_shell_default() {
        let record = SpaceRecord::bootstrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_systems(Update, handle_page_open);
        spawn_active_space(&mut app, &record);

        let stack = app.world_mut().spawn(Stack::bundle()).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert!(launch.cwd.is_empty());
    }

    #[test]
    fn open_terminal_page_prefers_ancestor_tab_startup_dir() {
        let space_dir = tempfile::tempdir().unwrap();
        let tab_dir = tempfile::tempdir().unwrap();
        let record = SpaceRecord::bootstrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(space_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .add_systems(Update, handle_page_open);
        spawn_active_space(&mut app, &record);

        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "t".into(),
                startup_dir: Some(tab_dir.path().to_string_lossy().into()),
            })
            .id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(tab))).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert_eq!(
            launch.cwd,
            tab_dir.path().canonicalize().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn open_terminal_page_rejects_invalid_ancestor_tab_startup_dir() {
        let fallback_dir = tempfile::tempdir().unwrap();
        let record = SpaceRecord::bootstrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(fallback_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .add_systems(Update, handle_page_open);
        spawn_active_space(&mut app, &record);

        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "t".into(),
                startup_dir: Some("/no/such/vmux-tab-workspace".into()),
            })
            .id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(tab))).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://terminal".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenError>(task).is_some());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Terminal>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn layout_terminal_rejects_invalid_ancestor_tab_startup_dir() {
        let fallback_dir = tempfile::tempdir().unwrap();
        let record = SpaceRecord::bootstrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(fallback_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<TerminalLayoutSpawnRequest>()
            .insert_resource(settings)
            .add_systems(Update, spawn_layout_requested_content);
        spawn_active_space(&mut app, &record);

        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "t".into(),
                startup_dir: Some("/no/such/vmux-tab-workspace".into()),
            })
            .id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(tab))).id();
        app.world_mut()
            .resource_mut::<Messages<TerminalLayoutSpawnRequest>>()
            .write(TerminalLayoutSpawnRequest { stack });

        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Terminal>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn process_create_budget_bounds_in_flight() {
        assert_eq!(
            PendingServiceCreate::budget(0, 8),
            8,
            "full budget when nothing in flight"
        );
        assert_eq!(PendingServiceCreate::budget(3, 8), 5);
        assert_eq!(
            PendingServiceCreate::budget(8, 8),
            0,
            "no budget at the cap"
        );
        assert_eq!(
            PendingServiceCreate::budget(99, 8),
            0,
            "never negative when over the cap"
        );
    }

    #[test]
    fn process_not_found_message_parses_process_id() {
        let missing = process_id(9);

        assert_eq!(
            TerminalServiceError {
                message: format!("process not found: {missing}"),
            }
            .missing_process_id(),
            Some(missing)
        );
        assert_eq!(
            TerminalServiceError {
                message: "permission denied".to_string(),
            }
            .missing_process_id(),
            None
        );
    }

    #[test]
    fn terminal_update_schedule_has_no_before_after_cycle() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            vmux_layout::stack::StackPlugin,
        ))
        .add_message::<TerminalLayoutSpawnRequest>()
        .add_plugins(TerminalUpdatePlugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        let result = update.initialize(app.world_mut());

        if let Err(error) = result {
            panic!("{}", error.to_string(update.graph(), app.world()));
        }
    }

    #[test]
    fn terminal_input_targets_fallback_to_focused_terminal_in_user_mode() {
        let stack = Entity::from_bits(1);
        let process_id = process_id(7);

        let targets = resolve_terminal_input_targets([], false, Some(stack), [(stack, process_id)]);

        assert_eq!(targets, vec![process_id]);
    }

    #[test]
    fn terminal_input_targets_do_not_steal_input_from_non_terminal_target() {
        let stack = Entity::from_bits(1);

        let targets =
            resolve_terminal_input_targets([], true, Some(stack), [(stack, process_id(7))]);

        assert!(targets.is_empty());
    }

    #[test]
    fn terminal_input_targets_choose_focused_terminal_when_multiple_targets_exist() {
        let stale_stack = Entity::from_bits(1);
        let focused_stack = Entity::from_bits(2);
        let stale_pid = process_id(7);
        let focused_pid = process_id(8);

        let targets = resolve_terminal_input_targets(
            [(stale_stack, stale_pid), (focused_stack, focused_pid)],
            true,
            Some(focused_stack),
            [(stale_stack, stale_pid), (focused_stack, focused_pid)],
        );

        assert_eq!(targets, vec![focused_pid]);
    }

    #[test]
    fn terminal_input_targets_choose_focused_terminal_when_targets_are_stale() {
        let stale_stack = Entity::from_bits(1);
        let focused_stack = Entity::from_bits(2);
        let stale_pid = process_id(7);
        let focused_pid = process_id(8);

        let targets = resolve_terminal_input_targets(
            [(stale_stack, stale_pid)],
            true,
            Some(focused_stack),
            [(stale_stack, stale_pid), (focused_stack, focused_pid)],
        );

        assert_eq!(targets, vec![focused_pid]);
    }

    #[test]
    fn terminal_input_targets_ignore_stale_targets_when_focus_is_not_terminal() {
        let stale_stack = Entity::from_bits(1);
        let focused_stack = Entity::from_bits(2);
        let stale_pid = process_id(7);

        let targets = resolve_terminal_input_targets(
            [(stale_stack, stale_pid)],
            true,
            Some(focused_stack),
            [(stale_stack, stale_pid)],
        );

        assert!(targets.is_empty());
    }

    #[test]
    fn web_terminal_key_events_delegate_text_to_pty_bytes() {
        let event = KeyStroke {
            key: "a".to_string(),
            code: "KeyA".to_string(),
            text: Some("a".to_string()),
            ..Default::default()
        };

        assert_eq!(TerminalInput::bytes(&event), b"a".to_vec());
    }

    #[test]
    fn web_terminal_key_events_delegate_control_sequences() {
        let event = KeyStroke {
            key: "c".to_string(),
            code: "KeyC".to_string(),
            mods: KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(TerminalInput::bytes(&event), vec![3]);
    }

    #[test]
    fn web_terminal_key_events_ignore_modifier_keys() {
        let event = KeyStroke {
            key: "Shift".to_string(),
            code: "ShiftLeft".to_string(),
            mods: KeyModifiers {
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(TerminalInput::bytes(&event).is_empty());
    }

    #[test]
    fn web_terminal_shortcuts_emit_command_before_pty_input() {
        let event = KeyStroke {
            key: "l".to_string(),
            code: "KeyL".to_string(),
            mods: KeyModifiers {
                super_key: true,
                ..Default::default()
            },
            text: Some("l".to_string()),
            ..Default::default()
        };
        let mut state = TerminalShortcutState::default();
        let definitions =
            [
                CommandDefinition::new("command_bar_edit_page", "Edit Page", "Browser > Bar")
                    .direct("Super+l"),
            ];
        let keymap = Keymap::defaults_with(&definitions);

        assert_eq!(
            state.resolve(&event, &keymap),
            TerminalWebShortcutResolution::Command("command_bar_edit_page".to_string())
        );
    }

    #[test]
    fn web_terminal_menu_accel_shortcuts_emit_command_before_pty_input() {
        let event = KeyStroke {
            key: "S".to_string(),
            code: "KeyS".to_string(),
            mods: KeyModifiers {
                shift: true,
                super_key: true,
                ..Default::default()
            },
            text: Some("S".to_string()),
            ..Default::default()
        };
        let mut state = TerminalShortcutState::default();
        let definitions =
            [
                CommandDefinition::new("toggle_layout", "Toggle Layout", "Layout > Layout")
                    .direct("Super+Shift+S"),
            ];
        let keymap = Keymap::defaults_with(&definitions);

        assert_eq!(
            state.resolve(&event, &keymap),
            TerminalWebShortcutResolution::Command("toggle_layout".to_string())
        );
    }

    #[test]
    fn shell_prompt_ready_only_once_cursor_is_past_column_zero() {
        assert!(!ShellOutputSeen::ready(false, 0), "no output yet");
        assert!(
            !ShellOutputSeen::ready(true, 0),
            "banner line ends in a newline (cursor at column 0)"
        );
        assert!(
            !ShellOutputSeen::ready(true, 0),
            "further banner lines are still column 0"
        );
        assert!(
            ShellOutputSeen::ready(true, 3),
            "drawn prompt leaves the cursor after the prompt string"
        );
    }

    #[test]
    fn vim_visual_keys_map_to_copy_mode_actions() {
        assert_eq!(
            map_copy_mode_key(&Key::Character("v".into()), false),
            Some(K::StartSelection)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("V".into()), false),
            Some(K::StartLineSelection)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("e".into()), true),
            Some(K::Down)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("y".into()), true),
            Some(K::Up)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("y".into()), false),
            Some(K::Copy)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("c".into()), true),
            Some(K::Exit)
        );
    }

    #[test]
    fn vim_g_ends_visual_selection_at_last_non_blank() {
        let mut copy_mode = TerminalCopyMode::default();

        assert_eq!(
            map_copy_mode_key_with_state(&mut copy_mode, &Key::Character("g".into()), false),
            None
        );
        assert_eq!(
            map_copy_mode_key_with_state(&mut copy_mode, &Key::Character("_".into()), false),
            Some(K::LastNonBlank)
        );
    }

    #[test]
    fn vim_visual_motion_keys_map_to_copy_mode_actions() {
        let mut copy_mode = TerminalCopyMode::default();

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
            ),
            vec![K::WordForward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::shift(&Key::Character("W".into()), KeyCode::KeyW)
            ),
            vec![K::BigWordForward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("b".into()), KeyCode::KeyB)
            ),
            vec![K::WordBackward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
            ),
            vec![K::WordEndForward]
        );

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
            ),
            vec![K::WordEndBackward]
        );

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("3".into()), KeyCode::Digit3)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
            ),
            vec![K::WordForward, K::WordForward, K::WordForward]
        );
    }

    #[test]
    fn shifted_minus_resolves_g_() {
        let mut copy_mode = TerminalCopyMode::default();

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut copy_mode,
                CopyModeKeyInput::shift(&Key::Character("-".into()), KeyCode::Minus)
            ),
            vec![K::LastNonBlank]
        );
    }

    #[test]
    fn local_copy_mode_is_active_before_service_broadcast() {
        let mode = TerminalMode::default();
        let mut copy_mode = TerminalCopyMode::default();

        assert!(!is_copy_mode_active(&mode, &copy_mode));

        copy_mode.set(true);

        assert!(is_copy_mode_active(&mode, &copy_mode));
    }

    #[test]
    fn service_copy_mode_broadcast_reconciles_local_latch() {
        let mode = TerminalMode::default();
        let mut copy_mode = TerminalCopyMode::default();

        copy_mode.set(true);
        copy_mode.set(false);

        assert!(!is_copy_mode_active(&mode, &copy_mode));
    }

    #[test]
    fn exiting_copy_mode_clears_local_latch() {
        let mut copy_mode = TerminalCopyMode::default();
        copy_mode.set(true);

        if copy_mode_key_exits(K::Exit) {
            copy_mode.set(false);
        }

        assert!(!copy_mode.active);
    }

    #[test]
    fn restart_state_clears_shell_output_seen_and_preserves_pending_input() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin))
            .add_observer(restart);
        let entity = app.world_mut().spawn((Terminal, ShellOutputSeen)).id();
        app.world_mut().write_message(QueueTerminalInput {
            terminal: entity,
            data: b"queued\r".to_vec(),
        });
        app.update();

        app.world_mut()
            .run_system_cached_with(
                |In(entity): In<Entity>, mut commands: Commands| {
                    commands.trigger(TerminalRestartRequest { terminal: entity });
                },
                entity,
            )
            .unwrap();

        assert!(app.world().get::<ShellOutputSeen>(entity).is_none());
        assert!(app.world().get::<AwaitingProcessCreated>(entity).is_some());
        assert_eq!(
            pending_terminal_input(app.world_mut(), entity),
            [b"queued\r".to_vec()]
        );
    }

    #[test]
    fn process_created_matches_by_id_not_by_position() {
        let mut app = bevy::prelude::App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin))
            .add_message::<TerminalProcessCreated>()
            .add_message::<TerminalProcessCreateFailed>()
            .add_systems(Update, apply_process_start);
        let id1 = ProcessId::new();
        let id2 = ProcessId::new();
        let id3 = ProcessId::new();
        let e1 = app
            .world_mut()
            .spawn((
                Terminal,
                id1,
                PendingServiceCreate,
                AwaitingProcessCreated,
                TerminalLaunch {
                    command: "/bin/sh".into(),
                    args: vec![],
                    cwd: "/tmp/1".into(),
                    env: vec![],
                },
            ))
            .id();
        let e2 = app
            .world_mut()
            .spawn((
                Terminal,
                id2,
                AwaitingProcessCreated,
                TerminalLaunch {
                    command: "/bin/sh".into(),
                    args: vec![],
                    cwd: "/tmp/2".into(),
                    env: vec![],
                },
            ))
            .id();
        let e3 = app
            .world_mut()
            .spawn((
                Terminal,
                id3,
                AwaitingProcessCreated,
                TerminalLaunch {
                    command: "/bin/sh".into(),
                    args: vec![],
                    cwd: "/tmp/3".into(),
                    env: vec![],
                },
            ))
            .id();

        app.update();
        for (process_id, pid) in [(id3, 333u32), (id1, 111), (id2, 222)] {
            app.world_mut()
                .resource_mut::<Messages<TerminalProcessCreated>>()
                .write(TerminalProcessCreated { process_id, pid });
        }
        app.update();

        let world = app.world();
        assert_eq!(world.get::<pid::Pid>(e1).map(|p| p.0), Some(111));
        assert_eq!(world.get::<pid::Pid>(e2).map(|p| p.0), Some(222));
        assert_eq!(world.get::<pid::Pid>(e3).map(|p| p.0), Some(333));
    }

    #[test]
    fn apply_process_create_failed_despawns_terminal() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin))
            .add_message::<TerminalProcessCreated>()
            .add_message::<TerminalProcessCreateFailed>()
            .add_systems(Update, apply_process_start);
        let process_id = ProcessId::new();
        let entity = app
            .world_mut()
            .spawn((Terminal, process_id, AwaitingProcessCreated))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<TerminalProcessCreateFailed>>()
            .write(TerminalProcessCreateFailed {
                process_id,
                reason: String::new(),
            });
        app.update();
        assert!(
            !app.world().entities().contains(entity),
            "failed create must despawn the orphaned terminal so no system is left to drive or reap it"
        );
    }

    #[test]
    fn apply_osc_title_sets_and_clears() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin))
            .add_message::<OscTitleChanged>()
            .add_systems(Update, apply_osc_title);
        let pid = ProcessId::new();
        let e = app.world_mut().spawn((Terminal, pid)).id();

        app.world_mut()
            .resource_mut::<Messages<OscTitleChanged>>()
            .write(OscTitleChanged {
                process_id: pid,
                title: "claude — repo".to_string(),
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<PageIdentity>(e)
                .and_then(|identity| identity.title.clone()),
            Some("claude — repo".to_string())
        );

        app.world_mut()
            .resource_mut::<Messages<OscTitleChanged>>()
            .write(OscTitleChanged {
                process_id: pid,
                title: String::new(),
            });
        app.update();
        assert!(app.world().get::<PageIdentity>(e).is_none());
    }

    #[test]
    fn clear_osc_title_on_exit_removes_override() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin))
            .add_message::<ProcessExitedEvent>()
            .add_systems(Update, clear_osc_title_on_exit);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((Terminal, pid, PageIdentity::from("working")))
            .id();

        app.world_mut()
            .resource_mut::<Messages<ProcessExitedEvent>>()
            .write(ProcessExitedEvent { process_id: pid });
        app.update();
        assert!(app.world().get::<PageIdentity>(e).is_none());
    }

    #[test]
    fn retained_terminal_stays_in_service_query_after_exit() {
        let mut world = World::new();
        let entity = world
            .spawn((Terminal, ProcessExited, RetainOnProcessExit))
            .id();
        let mut query = world.query_filtered::<Entity, ServiceTerminalFilter>();

        assert!(query.get(&world, entity).is_ok());
    }

    #[test]
    fn retained_terminal_does_not_close_stack_on_exit() {
        assert!(!should_close_terminal_stack_on_exit(true));
    }
}
