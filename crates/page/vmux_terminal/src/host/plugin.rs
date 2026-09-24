use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::{
    ecs::relationship::Relationship,
    input::keyboard::Key,
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_command::WriteCommandRequests;
use vmux_command::shortcut::{KeyCombo, Keymap, Modifiers};
use vmux_core::input::KeyStroke;
use vmux_core::terminal::{TerminalSpawnRequest, TerminalSpawnTarget};
use vmux_core::{
    PageIdentity, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::stack::{CloseRequest as StackCloseRequest, FocusRequest};
use vmux_layout::{CloseRequiresConfirmation, TerminalLayoutSpawnRequest};
use vmux_service::{
    client::{ServiceHandle, ServiceWake},
    protocol::{ClientMessage, ProcessId, ServiceMessage, SharedEvent},
};
use vmux_setting::AppSettings;

#[cfg(test)]
use super::input_queue::InputQueuePlugin;
use super::input_queue::{NextTerminalInputSequence, TerminalInput};
use super::loading::AgentLoading;
use super::mouse::MouseSelectionState;
use super::process_control::{PendingTerminalSnapshot, ProcessControlPlugin, TerminalGridSize};
use super::prompt::PromptCapture;
use crate::event::*;
use crate::pid::{self, Pid};
use crate::process_index::TerminalProcessIndex;
use crate::{ProcessExited, RetainOnProcessExit, Terminal};
use vmux_core::KeyboardOwner;
use vmux_flex::prelude::*;

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::TerminalPage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.world_mut()
            .spawn(vmux_core::HostSpawnRoute::host("terminal"));
        app.add_plugins((
            vmux_core::host::UiStatePlugin::<vmux_core::event::TerminalUiState>::default(),
            vmux_command::CommandTypePlugin::<super::command::CloseRequest>::default(),
            vmux_command::CommandTypePlugin::<super::command::NextRequest>::default(),
            vmux_command::CommandTypePlugin::<super::command::PrevRequest>::default(),
            vmux_command::CommandTypePlugin::<super::command::ClearRequest>::default(),
            vmux_command::CommandTypePlugin::<super::command::CopyModeRequest>::default(),
        ))
        .add_plugins(crate::contract::TerminalContractPlugin)
        .register_type::<crate::launch::TerminalLaunch>()
        .register_type::<crate::launch::TerminalKind>()
        .add_message::<TerminalStackSpawnRequest>()
        .add_message::<TerminalSpawnRequest>()
        .add_message::<vmux_service::agent_events::AgentCommandResultEvent>()
        .add_message::<vmux_service::agent_events::AgentQueryResultEvent>()
        .add_plugins((
            crate::pid::PidPlugin,
            crate::host::request::TerminalRequestPlugin,
            TerminalServicePlugin,
            TerminalInputPlugin,
            crate::processes_monitor::ProcessesMonitorPlugin,
            super::loading::LoadingPlugin,
            super::prompt::PromptPlugin,
            crate::snapshot_updater::SnapshotPlugin,
            crate::theme::TerminalThemePlugin,
        ));
    }
}

struct TerminalServicePlugin;

impl Plugin for TerminalServicePlugin {
    fn build(&self, app: &mut App) {
        let service_wake = service_wake_callback(app);
        ensure_service_started();
        app.insert_resource(ServiceConnectRetry::new());
        app.insert_resource(ServiceWakeCallback(service_wake))
            .add_plugins(TerminalUpdatePlugin)
            .add_systems(
                Update,
                respond_terminal_stack_spawn
                    .in_set(TerminalStackSpawnSet)
                    .after(ServiceMessageSet),
            )
            .add_systems(
                Update,
                respond_terminal_spawn.in_set(vmux_command::ReadCommandRequests),
            )
            .add_systems(
                Update,
                prewarm_login_shell_env.run_if(resource_added::<AppSettings>),
            )
            .add_observer(on_restart_pty)
            .add_observer(on_terminal_removed);
    }
}

struct TerminalInputPlugin;

impl Plugin for TerminalInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerminalModeMap>()
            .init_resource::<LocalCopyModeState>()
            .init_resource::<TerminalWebShortcutState>()
            .add_systems(Update, format_terminal_url.after(pid::track_pid_inserts))
            .add_plugins((
                super::mouse::MousePlugin,
                super::link::LinkPlugin,
                ProcessControlPlugin,
            ))
            .add_observer(on_term_key);
    }
}

fn prewarm_login_shell_env(settings: Res<AppSettings>) {
    crate::shell_env::prewarm_login_shell_env(terminal_shell(&settings));
}

struct TerminalUpdatePlugin;

impl Plugin for TerminalUpdatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::contract::TerminalContractPlugin)
            .add_message::<ProcessExitedEvent>()
            .add_message::<CommandLifecycleEvent>()
            .add_message::<OscTitleChanged>()
            .add_message::<vmux_core::notify::BellReceived>()
            .add_systems(Update, apply_osc_title.after(poll_service_messages))
            .add_systems(Update, clear_osc_title_on_exit.after(poll_service_messages))
            .add_systems(Update, sync_agent_focus.after(poll_service_messages))
            .add_systems(
                Update,
                handle_terminal_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                spawn_layout_requested_content.after(vmux_layout::stack::StackCommandSet),
            )
            .add_systems(
                Update,
                (
                    try_connect_service.run_if(resource_exists::<ServiceConnectRetry>),
                    resolve_pending_terminal_cwd,
                    poll_service_messages
                        .in_set(WriteCommandRequests)
                        .in_set(ServiceMessageSet),
                    handle_terminal_navigation_commands.in_set(vmux_command::ReadCommandRequests),
                    handle_terminal_clear_command.in_set(vmux_command::ReadCommandRequests),
                    handle_terminal_copy_mode_command.in_set(vmux_command::ReadCommandRequests),
                )
                    .chain(),
            );
    }
}

const CTRL_V: u8 = 0x16;

pub fn should_confirm_close(settings: &AppSettings) -> bool {
    settings.terminal.as_ref().is_none_or(|t| t.confirm_close)
}

pub fn has_live_terminal(
    tab: Entity,
    children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<Terminal>, Without<ProcessExited>)>,
) -> bool {
    if let Ok(children) = children_q.get(tab) {
        children.iter().any(|child| terminal_q.contains(child))
    } else {
        false
    }
}

pub use vmux_service::client::ServiceClient;

#[derive(Resource, Clone)]
struct ServiceWakeCallback(Option<ServiceWake>);

#[derive(Resource, Default)]
pub struct TerminalModeMap {
    pub modes: std::collections::HashMap<ProcessId, TerminalModeFlags>,
}

impl TerminalModeMap {
    pub(crate) fn agent_ready(&self, process_id: &ProcessId) -> bool {
        self.modes
            .get(process_id)
            .is_some_and(|mode| mode.alt_screen || mode.mouse_capture || mode.focus_reporting)
    }
}

#[derive(Resource, Default)]
pub(super) struct LocalCopyModeState {
    active: std::collections::HashSet<ProcessId>,
    input_states: std::collections::HashMap<ProcessId, CopyModeInputState>,
}

impl LocalCopyModeState {
    pub(super) fn set(&mut self, process_id: ProcessId, active: bool) {
        if active {
            self.active.insert(process_id);
        } else {
            self.active.remove(&process_id);
            self.input_states.remove(&process_id);
        }
    }
}

#[derive(Resource, Default)]
struct TerminalWebShortcutState {
    pending_prefix: Option<(KeyCombo, Instant)>,
}

#[derive(Default)]
struct CopyModeInputState {
    pending_key: Option<CopyModePendingKey>,
    count: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyModePendingKey {
    G,
    FindForward,
    FindBackward,
    TillForward,
    TillBackward,
}

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

#[derive(Default, Clone, Copy, Debug)]
pub struct TerminalModeFlags {
    pub mouse_capture: bool,
    pub copy_mode: bool,
    pub alt_screen: bool,
    pub focus_reporting: bool,
}

#[derive(Component)]
pub struct AgentFocusBlurred;

#[derive(Event)]
pub struct RestartPty {
    pub entity: Entity,
}

#[derive(Resource)]
struct ServiceConnectRetry {
    timer: Timer,
    next_delay_ms: u64,
    remaining_attempts: u32,
}

impl ServiceConnectRetry {
    fn new() -> Self {
        Self {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            next_delay_ms: 50,
            remaining_attempts: 6,
        }
    }
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
pub struct ServiceMessageSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalStackSpawnSet;

pub fn format_terminal_url(
    mut q: Query<
        (Option<&Pid>, &mut PageMetadata),
        (
            With<Terminal>,
            Without<vmux_core::agent::AgentSession>,
            Or<(Changed<Pid>, Added<PageMetadata>)>,
        ),
    >,
) {
    for (pid, mut meta) in &mut q {
        let next = match pid {
            Some(pid) => pid.page_url(),
            None => TERMINAL_PAGE_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}

fn on_terminal_removed(
    trigger: On<Remove, ProcessId>,
    service: Option<Res<ServiceClient>>,
    pids: Query<&ProcessId>,
) {
    let Some(service) = service else { return };
    let entity = trigger.event_target();
    let Ok(process_id) = pids.get(entity) else {
        return;
    };
    service.0.send(ClientMessage::KillProcess {
        process_id: *process_id,
    });
}

fn spawn_layout_requested_content(
    mut reader: MessageReader<TerminalLayoutSpawnRequest>,
    settings: Res<AppSettings>,
    active_space: Res<vmux_space::spaces::ActiveSpace>,
    child_of: Query<&ChildOf>,
    tabs: Query<&vmux_layout::tab::Tab>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let tab_dir = vmux_layout::tab::ancestor_tab_startup_dir(request.stack, &child_of, &tabs);
        let Ok(cwd) = settings.workspace_dir(&active_space.record.id, tab_dir.as_deref()) else {
            continue;
        };
        let terminal = commands
            .spawn((
                new_terminal_bundle_with_cwd(&settings, cwd.as_deref()),
                ChildOf(request.stack),
            ))
            .id();
        commands.entity(terminal).insert(KeyboardOwner);
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_terminal_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    pid_to_entity: Option<Res<pid::PidToEntity>>,
    child_of_q: Query<&ChildOf>,
    children_q: Query<&Children>,
    tabs: Query<&vmux_layout::tab::Tab>,
    settings: Res<AppSettings>,
    active_space: Res<vmux_space::spaces::ActiveSpace>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url == TERMINAL_PAGE_URL.trim_end_matches('/')
            || task.url.starts_with(TERMINAL_PAGE_URL)
        {
            match open_terminal_page(
                task,
                pid_to_entity.as_deref(),
                &child_of_q,
                &children_q,
                &tabs,
                &settings,
                &active_space,
                &mut commands,
            ) {
                Ok(()) => {
                    commands.entity(entity).insert(PageOpenHandled);
                }
                Err(message) => {
                    commands.entity(entity).insert(PageOpenError { message });
                }
            }
        }
    }
}

fn open_terminal_page(
    task: &PageOpenTask,
    pid_to_entity: Option<&pid::PidToEntity>,
    child_of_q: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    tabs: &Query<&vmux_layout::tab::Tab>,
    settings: &AppSettings,
    active_space: &vmux_space::spaces::ActiveSpace,
    commands: &mut Commands,
) -> Result<(), String> {
    let parsed = url::Url::parse(&task.url)
        .map_err(|e| format!("invalid terminal URL '{}': {e}", task.url))?;
    let path = parsed.path().trim_start_matches('/');
    if !path.is_empty() {
        match path.parse::<u32>() {
            Ok(pid) => {
                if let Some(map) = pid_to_entity
                    && let Some(&entity) = map.0.get(&pid)
                {
                    pid::focus_pane_entity(entity, commands, child_of_q);
                    return Ok(());
                }
                warn!("no terminal pane for pid {pid}; spawning new");
            }
            Err(_) => return Err(format!("malformed terminal URL '{}'", task.url)),
        }
    }
    let cwd_param = parsed
        .query_pairs()
        .find(|(k, _)| k == "cwd")
        .map(|(_, v)| v.into_owned());
    let cwd = if let Some(cwd) = cwd_param.as_deref() {
        vmux_space::cwd::valid_cwd(cwd)?
    } else {
        let tab_dir = vmux_layout::tab::ancestor_tab_startup_dir(task.stack, child_of_q, tabs);
        settings.workspace_dir(&active_space.record.id, tab_dir.as_deref())?
    };
    vmux_layout::stack::Stack::clear_children(task.stack, children_q, commands);
    let title = cwd
        .as_ref()
        .map(|cwd| format!("Terminal ({})", cwd.display()))
        .unwrap_or_else(|| "Terminal".to_string());
    commands.entity(task.stack).insert(PageMetadata {
        url: TERMINAL_PAGE_URL.to_string(),
        title,
        bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
        ..default()
    });
    let terminal = commands
        .spawn((
            new_terminal_bundle_with_cwd(settings, cwd.as_deref()),
            ChildOf(task.stack),
        ))
        .id();
    commands.entity(terminal).insert(KeyboardOwner);
    Ok(())
}

fn respond_terminal_spawn(
    mut reader: MessageReader<TerminalSpawnRequest>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    child_of_q: Query<&ChildOf>,
) {
    for req in reader.read() {
        let term_e = commands
            .spawn(new_terminal_bundle_with_cwd(&settings, req.cwd.as_deref()))
            .id();
        commands.entity(term_e).insert(KeyboardOwner);
        let (stack_e, requested_pane) = match req.target {
            TerminalSpawnTarget::Detached => continue,
            TerminalSpawnTarget::Stack(stack) => (stack, None),
            TerminalSpawnTarget::NewStackInPane(pane) => (
                commands
                    .spawn((
                        vmux_layout::stack::stack_bundle(),
                        LastActivatedAt::now(),
                        ChildOf(pane),
                    ))
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

fn service_wake_callback(app: &App) -> Option<ServiceWake> {
    app.world()
        .get_resource::<bevy::winit::EventLoopProxyWrapper>()
        .map(|wrapper| {
            let proxy = (**wrapper).clone();
            std::sync::Arc::new(move || {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }) as ServiceWake
        })
}

pub fn new_terminal_bundle(settings: &AppSettings) -> impl Bundle {
    new_terminal_bundle_with_cwd(settings, None)
}

pub fn new_terminal_bundle_with_cwd(
    settings: &AppSettings,
    cwd: Option<&std::path::Path>,
) -> impl Bundle {
    new_terminal_bundle_with_cwd_and_shell(settings, cwd, None)
}

fn new_terminal_bundle_with_cwd_and_shell(
    settings: &AppSettings,
    cwd: Option<&std::path::Path>,
    shell: Option<&str>,
) -> impl Bundle {
    let shell = shell.map(str::to_string).unwrap_or_else(|| {
        settings
            .terminal
            .as_ref()
            .map(|t| t.resolve_theme(&t.default_theme).shell)
            .unwrap_or_else(default_shell)
    });

    let cwd_str = cwd
        .filter(|d| !d.to_string_lossy().contains("://"))
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();

    let launch = crate::launch::TerminalLaunch {
        command: shell,
        args: vec![],
        cwd: cwd_str,
        env: vec![],
        kind: crate::launch::TerminalKind::Plain,
    };

    let process_id = ProcessId::new();

    (
        (
            Terminal,
            Browser,
            CloseRequiresConfirmation,
            process_id,
            launch,
            PendingServiceCreate,
            PageMetadata {
                title: format!("Terminal ({})", &process_id.to_string()[..8]),
                url: TERMINAL_PAGE_URL.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewWindowed,
            vmux_core::host::page::HostsPage,
            vmux_core::host::page::BindsEditingChords,
        ),
        (
            WebviewSize(Vec2::new(1280.0, 720.0)),
            TerminalGridSize::default(),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        ),
    )
}

fn respond_terminal_stack_spawn(
    mut reader: MessageReader<TerminalStackSpawnRequest>,
    settings: Res<AppSettings>,
    mut sequence: ResMut<NextTerminalInputSequence>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack_ts = if request.activate {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                stack_ts,
                ChildOf(request.pane),
            ))
            .id();
        let title = request
            .cwd
            .as_ref()
            .map(|cwd| format!("Terminal ({})", cwd.display()))
            .unwrap_or_else(|| "Terminal".to_string());
        commands.entity(stack).insert(PageMetadata {
            url: TERMINAL_PAGE_URL.to_string(),
            title,
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let terminal = commands
            .spawn((
                new_terminal_bundle_with_cwd_and_shell(
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
            TerminalInput::enqueue(&mut commands, &mut sequence, terminal, data);
        }
    }
}

pub fn reattach_terminal_bundle(process_id: ProcessId) -> impl Bundle {
    (
        (
            Terminal,
            Browser,
            CloseRequiresConfirmation,
            process_id,
            PendingServiceAttach,
            PageMetadata {
                title: format!("Terminal ({})", &process_id.to_string()[..8]),
                url: TERMINAL_PAGE_URL.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewWindowed,
            vmux_core::host::page::HostsPage,
            vmux_core::host::page::BindsEditingChords,
        ),
        (
            WebviewSize(Vec2::new(1280.0, 720.0)),
            TerminalGridSize::default(),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        ),
    )
}

#[derive(Component)]
pub struct PendingServiceCreate;

#[derive(Component)]
struct PendingServiceAttach;

#[derive(Component)]
pub(crate) struct ShellOutputSeen;

fn shell_prompt_ready(has_content: bool, cursor_col: u16) -> bool {
    has_content && cursor_col > 0
}

#[derive(Component)]
pub struct AwaitingProcessCreated;

pub fn mark_terminal_restarting(commands: &mut Commands, entity: Entity) {
    commands
        .entity(entity)
        .remove::<ShellOutputSeen>()
        .insert(AwaitingProcessCreated);
}

pub fn apply_process_created(
    commands: &mut Commands,
    entity: Entity,
    process_id: ProcessId,
    process_pid: u32,
) {
    commands
        .entity(entity)
        .insert(process_id)
        .insert(pid::Pid(process_pid))
        .remove::<AwaitingProcessCreated>();
}

fn apply_process_create_failed(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).despawn();
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

struct MissingTerminalRestart {
    entity: Entity,
    new_id: ProcessId,
    command: ClientMessage,
    cwd: String,
    agent_kind: Option<vmux_core::agent::AgentKind>,
}

impl MissingTerminalRestart {
    fn new(
        entity: Entity,
        launch: crate::launch::TerminalLaunch,
        agent_kind: Option<vmux_core::agent::AgentKind>,
    ) -> Self {
        let new_id = ProcessId::new();
        let cwd = launch.cwd.clone();
        Self {
            entity,
            new_id,
            command: ClientMessage::CreateProcess {
                process_id: new_id,
                command: launch.command,
                args: launch.args,
                cwd: launch.cwd,
                env: launch.env,
                cols: 80,
                rows: 24,
            },
            cwd,
            agent_kind,
        }
    }
}

fn terminal_shell(settings: &AppSettings) -> String {
    settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(default_shell)
}

const MAX_CONCURRENT_PROCESS_CREATES: usize = 8;

fn process_create_budget(in_flight: usize, max_concurrent: usize) -> usize {
    max_concurrent.saturating_sub(in_flight)
}

fn missing_process_id(message: &str) -> Option<ProcessId> {
    message
        .strip_prefix("process not found: ")
        .and_then(|id| id.parse().ok())
}

fn ensure_service_started() {
    if ServiceHandle::service_running() {
        tracing::info!("service already running");
        return;
    }
    let binary = match vmux_service::DaemonBinary::current() {
        Ok(b) => b.into_path(),
        Err(e) => {
            tracing::error!(error = %e, "could not locate vmux_service binary");
            return;
        }
    };
    match vmux_service::registry::start_mode_for(&binary) {
        vmux_service::registry::StartMode::Register => {
            let profile = vmux_service::ServicePaths::build_profile();
            if let Err(e) = vmux_service::registry::ensure_running(profile, &binary) {
                tracing::error!(error = ?e, "service registration failed");
            }
        }
        vmux_service::registry::StartMode::SpawnDetached => {
            vmux_service::registry::prepare_spawn_detached(&binary);
            spawn_detached_service(&binary);
        }
    }
}

#[cfg(unix)]
fn spawn_detached_service(binary: &std::path::Path) {
    use std::os::unix::process::CommandExt;
    let log_dir = vmux_service::ServicePaths::log_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let stderr_cfg = match std::fs::File::create(vmux_service::ServicePaths::current().log()) {
        Ok(f) => std::process::Stdio::from(f),
        Err(e) => {
            tracing::warn!(error = %e, "could not create service log; stderr will be discarded");
            std::process::Stdio::null()
        }
    };
    let spawn_result = unsafe {
        std::process::Command::new(binary)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(stderr_cfg)
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn()
    };
    if let Err(e) = spawn_result {
        tracing::error!(error = %e, "failed to spawn vmux_service");
    }
}

fn broadcast_service_unavailable(
    terminals: &Query<Entity, With<Terminal>>,
    commands: &mut Commands,
    message: String,
) {
    let evt = ServiceUnavailableEvent { message };
    for entity in terminals.iter() {
        crate::TerminalUiStateUpdates::write(commands, entity, &evt);
    }
}

fn try_connect_service(
    mut retry: ResMut<ServiceConnectRetry>,
    time: Res<Time>,
    mut commands: Commands,
    wake: Res<ServiceWakeCallback>,
    terminal_webviews: Query<Entity, With<Terminal>>,
) {
    retry.timer.tick(time.delta());
    if !retry.timer.just_finished() {
        return;
    }

    retry.remaining_attempts = retry.remaining_attempts.saturating_sub(1);

    let sock = vmux_service::ServicePaths::current().socket();
    if !sock.exists() {
        if retry.remaining_attempts == 0 {
            tracing::warn!("service socket never appeared — giving up");
            commands.remove_resource::<ServiceConnectRetry>();
            broadcast_service_unavailable(
                &terminal_webviews,
                &mut commands,
                "vmux service unavailable \u{2014} run `vmux service logs` for details.".into(),
            );
        } else {
            retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
            retry.timer = Timer::new(
                std::time::Duration::from_millis(retry.next_delay_ms),
                TimerMode::Once,
            );
        }
        return;
    }

    match ServiceHandle::connect_with_wake(wake.0.clone()) {
        Some(handle) => {
            tracing::info!("connected to service after retry");
            handle.send(ClientMessage::SubscribeAgentCommands);
            commands.insert_resource(ServiceClient(handle));
            commands.remove_resource::<ServiceConnectRetry>();
            broadcast_service_unavailable(&terminal_webviews, &mut commands, String::new());
        }
        None => {
            if retry.remaining_attempts == 0 {
                tracing::error!("failed to connect to service after all retries");
                let log_path = vmux_service::ServicePaths::current().log();
                if let Ok(log) = std::fs::read_to_string(&log_path)
                    && !log.is_empty()
                {
                    tracing::error!(service_log = %log, "service log contents");
                }
                commands.remove_resource::<ServiceConnectRetry>();
                broadcast_service_unavailable(
                    &terminal_webviews,
                    &mut commands,
                    "vmux service unavailable \u{2014} run `vmux service logs` for details.".into(),
                );
            } else {
                retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
                retry.timer = Timer::new(
                    std::time::Duration::from_millis(retry.next_delay_ms),
                    TimerMode::Once,
                );
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PollServiceWriters<'w> {
    stack_close_requests: MessageWriter<'w, StackCloseRequest>,
    agent_commands: MessageWriter<'w, vmux_service::agent_events::AgentCommandRequest>,
    agent_queries: MessageWriter<'w, vmux_service::agent_events::AgentQueryRequest>,
    agent_tool_calls: MessageWriter<'w, vmux_service::agent_events::AgentToolCallRequest>,
    page_agent_delta: MessageWriter<'w, vmux_service::agent_events::PageAgentDelta>,
    page_agent_run_status: MessageWriter<'w, vmux_service::agent_events::PageAgentRunStatus>,
    page_agent_awaiting: MessageWriter<'w, vmux_service::agent_events::PageAgentAwaitingApproval>,
    page_agent_approval_resolved:
        MessageWriter<'w, vmux_service::agent_events::PageAgentApprovalResolved>,
    page_agent_snapshot: MessageWriter<'w, vmux_service::agent_events::PageAgentSnapshot>,
    page_agent_info: MessageWriter<'w, vmux_service::agent_events::PageAgentInfo>,
    page_agent_workspace_changed:
        MessageWriter<'w, vmux_service::agent_events::PageAgentWorkspaceChanged>,
    page_agent_model_info: MessageWriter<'w, vmux_service::agent_events::PageAgentModelInfo>,
    page_agent_model_selection_result:
        MessageWriter<'w, vmux_service::agent_events::PageAgentModelSelectionResult>,
    page_agent_mode_info: MessageWriter<'w, vmux_service::agent_events::PageAgentModeInfo>,
    page_agent_mode_selection_result:
        MessageWriter<'w, vmux_service::agent_events::PageAgentModeSelectionResult>,
    page_agent_session_created:
        MessageWriter<'w, vmux_service::agent_events::PageAgentSessionCreated>,
    page_agent_acp_terminal_created:
        MessageWriter<'w, vmux_service::agent_events::PageAgentAcpTerminalCreated>,
    agent_command_results: MessageWriter<'w, vmux_service::agent_events::AgentCommandResultEvent>,
    agent_query_results: MessageWriter<'w, vmux_service::agent_events::AgentQueryResultEvent>,
    process_exited: MessageWriter<'w, ProcessExitedEvent>,
    command_lifecycle: MessageWriter<'w, CommandLifecycleEvent>,
    osc_title: MessageWriter<'w, OscTitleChanged>,
    bell: MessageWriter<'w, vmux_core::notify::BellReceived>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PollServiceState<'w> {
    process_index: Res<'w, TerminalProcessIndex>,
    mode_map: ResMut<'w, TerminalModeMap>,
    local_copy_mode: ResMut<'w, LocalCopyModeState>,
    mouse_state: ResMut<'w, MouseSelectionState>,
}

fn line_has_content(line: &vmux_core::event::TermLine) -> bool {
    line.spans.iter().any(|s| !s.text.trim().is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentFocusTransition {
    FocusIn,
    FocusOut,
}

fn agent_focus_transition(
    focus_reporting: bool,
    active: bool,
    blurred: bool,
) -> Option<AgentFocusTransition> {
    if !focus_reporting {
        None
    } else if active && blurred {
        Some(AgentFocusTransition::FocusIn)
    } else if !active && !blurred {
        Some(AgentFocusTransition::FocusOut)
    } else {
        None
    }
}

#[allow(clippy::type_complexity)]
fn sync_agent_focus(
    agents: Query<
        (Entity, &ProcessId, Has<AgentFocusBlurred>),
        With<vmux_core::agent::AgentSession>,
    >,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mode_map: Res<TerminalModeMap>,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    let active_pid = crate::target::active_terminal_for_tab(focus.stack, &terminals)
        .and_then(|entity| agents.get(entity).ok().map(|(_, pid, _)| *pid));
    for (entity, process_id, blurred) in &agents {
        let focus_reporting = mode_map
            .modes
            .get(process_id)
            .is_some_and(|m| m.focus_reporting);
        let active = Some(*process_id) == active_pid;
        match agent_focus_transition(focus_reporting, active, blurred) {
            Some(AgentFocusTransition::FocusIn) => {
                service.0.send(ClientMessage::ProcessInput {
                    process_id: *process_id,
                    data: b"\x1b[I".to_vec(),
                });
                commands.entity(entity).remove::<AgentFocusBlurred>();
            }
            Some(AgentFocusTransition::FocusOut) => {
                service.0.send(ClientMessage::ProcessInput {
                    process_id: *process_id,
                    data: b"\x1b[O".to_vec(),
                });
                commands.entity(entity).insert(AgentFocusBlurred);
            }
            None => {}
        }
    }
}

fn resolve_pending_terminal_cwd(
    mut pending: Query<
        (Entity, &mut crate::launch::TerminalLaunch),
        (With<Terminal>, With<PendingServiceCreate>),
    >,
    child_of: Query<&ChildOf>,
    tabs: Query<&vmux_layout::tab::Tab>,
    spaces: Query<(), With<vmux_layout::space::Space>>,
    space_ids: Query<&vmux_layout::space::SpaceId>,
    settings: Res<AppSettings>,
    active_space: Res<vmux_space::spaces::ActiveSpace>,
) {
    for (entity, mut launch) in &mut pending {
        if !launch.cwd.is_empty() {
            continue;
        }
        let tab_dir = vmux_layout::tab::ancestor_tab_startup_dir(entity, &child_of, &tabs);
        let space_id = vmux_layout::space::space_id_of(entity, &child_of, &spaces, &space_ids)
            .unwrap_or_else(|| active_space.record.id.clone());
        let Ok(Some(cwd)) = settings.workspace_dir(&space_id, tab_dir.as_deref()) else {
            continue;
        };
        launch.cwd = cwd.to_string_lossy().into_owned();
    }
}

fn poll_service_messages(
    pending_create: Query<
        (
            Entity,
            &ProcessId,
            &crate::launch::TerminalLaunch,
            Has<crate::AgentRunTerminal>,
        ),
        (With<Terminal>, With<PendingServiceCreate>),
    >,
    pending_attach: Query<(Entity, &ProcessId), (With<Terminal>, With<PendingServiceAttach>)>,
    awaiting_create: Query<
        (Entity, &ProcessId, &ChildOf),
        (With<Terminal>, With<AwaitingProcessCreated>),
    >,
    terminals: Query<
        (Entity, &ProcessId, &ChildOf, Has<RetainOnProcessExit>),
        ServiceTerminalFilter,
    >,
    service: Option<Res<ServiceClient>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut writers: PollServiceWriters,
    mut state: PollServiceState,
    settings: Res<AppSettings>,
    launches: Query<&crate::launch::TerminalLaunch>,
    agent_sessions: Query<&vmux_core::agent::AgentSession>,
    output_seen: Query<(), With<ShellOutputSeen>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let Some(service) = service else { return };

    let create_budget = process_create_budget(
        awaiting_create.iter().count(),
        MAX_CONCURRENT_PROCESS_CREATES,
    );
    for (entity, process_id, launch, agent_run) in pending_create.iter().take(create_budget) {
        let mut env = launch.env.clone();
        if should_merge_login_shell_env(agent_sessions.contains(entity), agent_run) {
            crate::shell_env::merge_login_shell_env(&mut env, &terminal_shell(&settings));
        }
        service.0.send(ClientMessage::CreateProcess {
            process_id: *process_id,
            command: launch.command.clone(),
            args: launch.args.clone(),
            cwd: launch.cwd.clone(),
            env,
            cols: 80,
            rows: 24,
        });
        commands
            .entity(entity)
            .remove::<PendingServiceCreate>()
            .insert(AwaitingProcessCreated);
    }

    for (entity, pid) in &pending_attach {
        service
            .0
            .send(ClientMessage::AttachProcess { process_id: *pid });
        service
            .0
            .send(ClientMessage::RequestSnapshot { process_id: *pid });
        commands.entity(entity).remove::<PendingServiceAttach>();
    }

    let mut restarted_missing_processes = Vec::new();
    let (messages, capped) = service.0.drain_with_status();
    if capped && let Some(proxy) = proxy.as_deref() {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
    for msg in messages {
        match msg {
            ServiceMessage::ProcessCreated { process_id, pid } => {
                let entity = state
                    .process_index
                    .get(&process_id)
                    .filter(|entity| awaiting_create.contains(*entity));
                if let Some(entity) = entity {
                    service.0.send(ClientMessage::AttachProcess { process_id });
                    apply_process_created(&mut commands, entity, process_id, pid);
                } else {
                    bevy::log::warn!(
                        "ProcessCreated for unknown process_id {process_id}; dropping"
                    );
                }
            }
            ServiceMessage::ProcessCreateFailed { process_id, reason } => {
                bevy::log::warn!("service failed to create process: {reason}");
                if let Some(entity) = state
                    .process_index
                    .get(&process_id)
                    .filter(|entity| awaiting_create.contains(*entity))
                {
                    apply_process_create_failed(&mut commands, entity);
                }
            }
            ServiceMessage::ViewportPatch {
                process_id,
                changed_lines,
                cursor,
                cols,
                rows,
                selection,
                copy_mode,
                full,
                first_row,
                total_rows,
                alt,
                mouse,
                evicted_total,
            } => {
                let Some(entity) = state.process_index.get(&process_id) else {
                    continue;
                };
                if !terminals.contains(entity) {
                    continue;
                }
                if !output_seen.contains(entity) {
                    let has_content = changed_lines.iter().any(|(_, l)| line_has_content(l));
                    if shell_prompt_ready(has_content, cursor.col) {
                        commands.entity(entity).insert(ShellOutputSeen);
                    }
                }
                if !browsers.can_emit_to(&entity) {
                    commands.entity(entity).insert(PendingTerminalSnapshot);
                    continue;
                }
                let mut changed_lines = changed_lines;
                for (_, line) in changed_lines.iter_mut() {
                    crate::link::annotate_links(line, None);
                }
                let patch = TermViewportPatch {
                    changed_lines,
                    cursor,
                    cols,
                    rows,
                    selection,
                    copy_mode,
                    full,
                    first_row,
                    total_rows,
                    alt,
                    mouse,
                    evicted_total,
                };
                crate::TerminalUiStateUpdates::write(&mut commands, entity, &patch);
            }
            ServiceMessage::Bell { process_id } => {
                writers
                    .bell
                    .write(vmux_core::notify::BellReceived { process_id });
            }
            ServiceMessage::ProcessTitle { process_id, title } => {
                writers.osc_title.write(OscTitleChanged {
                    process_id,
                    title: title.clone(),
                });
                let Some(entity) = state.process_index.get(&process_id) else {
                    continue;
                };
                if !terminals.contains(entity) || !browsers.can_emit_to(&entity) {
                    continue;
                }
                let evt = TermTitleEvent { title };
                crate::TerminalUiStateUpdates::write(&mut commands, entity, &evt);
            }
            ServiceMessage::Snapshot {
                process_id,
                lines,
                cursor,
                cols,
                rows,
            } => {
                let Some(entity) = state.process_index.get(&process_id) else {
                    continue;
                };
                if !terminals.contains(entity) {
                    continue;
                }
                if !output_seen.contains(entity) {
                    let has_content = lines.iter().any(line_has_content);
                    if shell_prompt_ready(has_content, cursor.col) {
                        commands.entity(entity).insert(ShellOutputSeen);
                    }
                }
                if !browsers.can_emit_to(&entity) {
                    continue;
                }
                let mut changed_lines: Vec<(u32, TermLine)> = lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| (i as u32, l))
                    .collect();
                for (_, line) in changed_lines.iter_mut() {
                    crate::link::annotate_links(line, None);
                }
                let patch = TermViewportPatch {
                    changed_lines,
                    cursor,
                    cols,
                    rows,
                    selection: None,
                    copy_mode: false,
                    full: true,
                    first_row: 0,
                    total_rows: rows as u32,
                    alt: false,
                    mouse: false,
                    evicted_total: 0,
                };
                crate::TerminalUiStateUpdates::write(&mut commands, entity, &patch);
            }
            ServiceMessage::ProcessExited { process_id, .. } => {
                writers
                    .process_exited
                    .write(ProcessExitedEvent { process_id });
                state.mode_map.modes.remove(&process_id);
                state.local_copy_mode.set(process_id, false);
                state.mouse_state.remove(&process_id);
                let Some(entity) = state.process_index.get(&process_id) else {
                    continue;
                };
                let Ok((_, _, child_of, retain_on_exit)) = terminals.get(entity) else {
                    continue;
                };
                commands
                    .entity(entity)
                    .insert(ProcessExited)
                    .remove::<CloseRequiresConfirmation>()
                    .remove::<AgentLoading>();
                let is_agent = if let Ok(session) = agent_sessions.get(entity) {
                    crate::TerminalUiStateUpdates::write(
                        &mut commands,
                        entity,
                        &crate::event::TermLoadingEvent {
                            loading: false,
                            label: session.kind.display_name().to_string(),
                            segment: session.kind.as_url_segment().to_string(),
                        },
                    );
                    true
                } else {
                    false
                };
                if should_close_terminal_stack_on_exit(is_agent, retain_on_exit) {
                    let tab = child_of.get();
                    commands.entity(tab).insert(LastActivatedAt::now());
                    writers.stack_close_requests.write(StackCloseRequest);
                }
            }
            ServiceMessage::ProcessList { processes } => {
                commands
                    .insert_resource(crate::processes_monitor::ServiceProcessList { processes });
            }
            ServiceMessage::Error { message } => {
                if let Some(stale_pid) = missing_process_id(&message)
                    && !restarted_missing_processes.contains(&stale_pid)
                    && let Some(entity) = state.process_index.get(&stale_pid)
                    && terminals.contains(entity)
                {
                    let launch = launches.get(entity).cloned().unwrap_or_else(|_| {
                        crate::launch::TerminalLaunch {
                            command: terminal_shell(&settings),
                            args: vec![],
                            cwd: String::new(),
                            env: vec![],
                            kind: crate::launch::TerminalKind::Plain,
                        }
                    });
                    let agent_kind = agent_sessions.get(entity).ok().map(|s| s.kind);
                    let restart = MissingTerminalRestart::new(entity, launch, agent_kind);
                    restarted_missing_processes.push(stale_pid);
                    let cwd = restart.cwd.clone();
                    let agent_kind = restart.agent_kind;
                    let new_id = restart.new_id;
                    let entity = restart.entity;
                    service.0.send(restart.command);
                    commands.entity(entity).insert(new_id);
                    mark_terminal_restarting(&mut commands, entity);
                    if let Some(kind) = agent_kind {
                        commands
                            .entity(entity)
                            .insert(vmux_core::agent::PendingAgentSession {
                                kind,
                                spawn_time: std::time::SystemTime::now(),
                                cwd: std::path::PathBuf::from(&cwd),
                            });
                    }
                }
                warn!("Service error: {message}");
            }
            ServiceMessage::TerminalMode {
                process_id,
                mouse_capture,
                copy_mode,
                alt_screen,
                focus_reporting,
            } => {
                state.mode_map.modes.insert(
                    process_id,
                    TerminalModeFlags {
                        mouse_capture,
                        copy_mode,
                        alt_screen,
                        focus_reporting,
                    },
                );
                state.local_copy_mode.set(process_id, copy_mode);
            }
            ServiceMessage::SelectionText {
                process_id: _,
                text,
            } if !text.is_empty() => {
                vmux_clipboard::write(text);
            }
            ServiceMessage::AgentCommand {
                request_id,
                anchor,
                command,
            } => {
                writers
                    .agent_commands
                    .write(vmux_service::agent_events::AgentCommandRequest {
                        request_id,
                        origin: vmux_service::agent_events::CommandOrigin::Agent {
                            sid: None,
                            anchor,
                        },
                        command,
                    });
            }
            ServiceMessage::AgentQuery { request_id, query } => {
                writers
                    .agent_queries
                    .write(vmux_service::agent_events::AgentQueryRequest { request_id, query });
            }
            ServiceMessage::AgentToolCall {
                request_id,
                sid,
                name,
                args,
            } => {
                writers
                    .agent_tool_calls
                    .write(vmux_service::agent_events::AgentToolCallRequest {
                        request_id,
                        sid,
                        name,
                        args,
                    });
            }
            ServiceMessage::Shared(SharedEvent::AgentDelta { sid, text }) => {
                writers
                    .page_agent_delta
                    .write(vmux_service::agent_events::PageAgentDelta { sid, text });
            }
            ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged { sid, status }) => {
                tracing::info!(%sid, ?status, "run status from the daemon");
                writers
                    .page_agent_run_status
                    .write(vmux_service::agent_events::PageAgentRunStatus { sid, status });
            }
            ServiceMessage::Shared(SharedEvent::AgentAwaitingApproval {
                sid,
                call_id,
                name,
                args,
            }) => {
                let args = serde_json::Value::try_from(&args)
                    .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                writers.page_agent_awaiting.write(
                    vmux_service::agent_events::PageAgentAwaitingApproval {
                        sid,
                        call_id,
                        name,
                        args,
                    },
                );
            }
            ServiceMessage::Shared(SharedEvent::AgentApprovalResolved { sid, call_id }) => {
                writers
                    .page_agent_approval_resolved
                    .write(vmux_service::agent_events::PageAgentApprovalResolved { sid, call_id });
            }
            ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { sid, messages }) => {
                writers
                    .page_agent_snapshot
                    .write(vmux_service::agent_events::PageAgentSnapshot { sid, messages });
            }
            ServiceMessage::Shared(SharedEvent::AcpAgentInfo { sid, name }) => {
                writers
                    .page_agent_info
                    .write(vmux_service::agent_events::PageAgentInfo { sid, name });
            }
            ServiceMessage::Shared(SharedEvent::AcpWorkspaceChanged {
                sid,
                name,
                branch,
                cwd,
                workspace_cwd,
            }) => {
                writers.page_agent_workspace_changed.write(
                    vmux_service::agent_events::PageAgentWorkspaceChanged {
                        sid,
                        name,
                        branch,
                        cwd,
                        workspace_cwd,
                    },
                );
            }
            ServiceMessage::Shared(SharedEvent::AcpModelInfo {
                sid,
                config_id,
                current_model_id,
                models,
            }) => {
                writers.page_agent_model_info.write(
                    vmux_service::agent_events::PageAgentModelInfo {
                        sid,
                        config_id,
                        current_model_id,
                        models,
                    },
                );
            }
            ServiceMessage::AcpModelSelectionResult {
                sid,
                request_id,
                model_id,
                succeeded,
            } => {
                writers.page_agent_model_selection_result.write(
                    vmux_service::agent_events::PageAgentModelSelectionResult {
                        sid,
                        request_id,
                        model_id,
                        succeeded,
                    },
                );
            }
            ServiceMessage::AcpModeInfo {
                sid,
                config_id,
                current_mode_id,
                modes,
            } => {
                writers
                    .page_agent_mode_info
                    .write(vmux_service::agent_events::PageAgentModeInfo {
                        sid,
                        config_id,
                        current_mode_id,
                        modes,
                    });
            }
            ServiceMessage::AcpModeSelectionResult {
                sid,
                request_id,
                mode_id,
                succeeded,
            } => {
                writers.page_agent_mode_selection_result.write(
                    vmux_service::agent_events::PageAgentModeSelectionResult {
                        sid,
                        request_id,
                        mode_id,
                        succeeded,
                    },
                );
            }
            ServiceMessage::AgentCommandResult { request_id, result } => {
                writers.agent_command_results.write(
                    vmux_service::agent_events::AgentCommandResultEvent { request_id, result },
                );
            }
            ServiceMessage::AgentQueryResult { request_id, result } => {
                writers.agent_query_results.write(
                    vmux_service::agent_events::AgentQueryResultEvent { request_id, result },
                );
            }
            ServiceMessage::CommandLifecycle { process_id, kind } => {
                writers
                    .command_lifecycle
                    .write(CommandLifecycleEvent { process_id, kind });
            }
            ServiceMessage::AcpSessionCreated {
                sid,
                acp_session_id,
            } => {
                writers.page_agent_session_created.write(
                    vmux_service::agent_events::PageAgentSessionCreated {
                        sid,
                        acp_session_id,
                    },
                );
            }
            ServiceMessage::AcpTerminalCreated {
                sid,
                terminal_id,
                process_id,
                command,
                args,
                cwd,
            } => {
                writers.page_agent_acp_terminal_created.write(
                    vmux_service::agent_events::PageAgentAcpTerminalCreated {
                        sid,
                        terminal_id,
                        process_id,
                        command,
                        args,
                        cwd,
                    },
                );
            }
            _ => {}
        }
    }
}

fn should_merge_login_shell_env(agent_session: bool, agent_run: bool) -> bool {
    agent_session || agent_run
}

type ServiceTerminalFilter = (
    With<Terminal>,
    Or<(Without<ProcessExited>, With<RetainOnProcessExit>)>,
    Without<AwaitingProcessCreated>,
);

fn should_close_terminal_stack_on_exit(is_agent: bool, retain_on_exit: bool) -> bool {
    !is_agent && !retain_on_exit
}

#[cfg(test)]
fn map_copy_mode_key(key: &Key, ctrl: bool) -> Option<vmux_service::protocol::CopyModeKey> {
    map_copy_mode_key_from_input(CopyModeKeyInput {
        key,
        key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
        ctrl,
        shift: false,
    })
}

fn map_copy_mode_key_from_input(
    input: CopyModeKeyInput<'_>,
) -> Option<vmux_service::protocol::CopyModeKey> {
    use vmux_service::protocol::CopyModeKey as K;
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
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    key: &Key,
    ctrl: bool,
) -> Option<vmux_service::protocol::CopyModeKey> {
    map_copy_mode_keys_with_state(
        local_copy_mode,
        process_id,
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
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    input: CopyModeKeyInput<'_>,
) -> Vec<vmux_service::protocol::CopyModeKey> {
    use vmux_service::protocol::CopyModeKey as K;

    let state = local_copy_mode.input_states.entry(process_id).or_default();
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

fn repeat_copy_mode_key(
    state: &mut CopyModeInputState,
    key: vmux_service::protocol::CopyModeKey,
) -> Vec<vmux_service::protocol::CopyModeKey> {
    let repeat = if copy_mode_key_uses_count(key) {
        state.count.take().unwrap_or(1)
    } else {
        state.count = None;
        1
    };
    vec![key; repeat as usize]
}

fn copy_mode_key_uses_count(key: vmux_service::protocol::CopyModeKey) -> bool {
    use vmux_service::protocol::CopyModeKey as K;
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

fn logical_key_to_bytes(key: &Key, ctrl: bool, alt: bool) -> Vec<u8> {
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

fn term_key_event_to_key(event: &KeyStroke) -> Key {
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

fn bracketed_paste(payload: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(payload.len() + 12);
    data.extend_from_slice(b"\x1b[200~");
    data.extend_from_slice(payload);
    data.extend_from_slice(b"\x1b[201~");
    data
}

pub fn image_path_payload(is_vibe: bool, path: &str) -> String {
    if is_vibe {
        format!("'{}'", path.replace('\'', "'\\''"))
    } else {
        path.to_string()
    }
}

fn write_clipboard_image_temp(process_id: ProcessId, png: &[u8]) -> Option<std::path::PathBuf> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("vmux-clip-{process_id}-{seq}.png"));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

fn resolve_paste(is_vibe: bool, process_id: ProcessId) -> Option<Vec<u8>> {
    if let Some(path) = vmux_clipboard::image_file_path() {
        return Some(bracketed_paste(
            image_path_payload(is_vibe, &path).as_bytes(),
        ));
    }
    if vmux_clipboard::has_image() {
        if is_vibe {
            let png = vmux_clipboard::read_image_png()?;
            let path = write_clipboard_image_temp(process_id, &png)?;
            let payload = image_path_payload(true, &path.to_string_lossy());
            return Some(bracketed_paste(payload.as_bytes()));
        }
        return Some(vec![CTRL_V]);
    }
    let text = vmux_clipboard::read_blocking()?;
    (!text.is_empty()).then(|| bracketed_paste(text.as_bytes()))
}

fn resolve_paste_text(is_vibe: bool, process_id: ProcessId) -> Option<String> {
    if let Some(path) = vmux_clipboard::image_file_path() {
        return Some(image_path_payload(is_vibe, &path));
    }
    if vmux_clipboard::has_image() {
        let png = vmux_clipboard::read_image_png()?;
        let path = write_clipboard_image_temp(process_id, &png)?;
        return Some(image_path_payload(is_vibe, &path.to_string_lossy()));
    }
    let text = vmux_clipboard::read_blocking()?;
    (!text.is_empty()).then_some(text)
}

fn term_key_event_to_bytes(event: &KeyStroke) -> Vec<u8> {
    if event.is_modifier_key() {
        return Vec::new();
    }
    let key = term_key_event_to_key(event);
    logical_key_to_bytes(&key, event.mods.ctrl, event.mods.alt)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalWebShortcutAction {
    Command(String),
    Consume,
    PassThrough,
}

fn resolve_terminal_web_shortcut(
    event: &KeyStroke,
    map: &Keymap,
    state: &mut TerminalWebShortcutState,
) -> TerminalWebShortcutAction {
    let Some(combo) = term_key_event_to_shortcut_combo(event) else {
        return TerminalWebShortcutAction::PassThrough;
    };
    let now = Instant::now();
    if let Some((_, started)) = state.pending_prefix.as_ref()
        && now.duration_since(*started) > Duration::from_millis(map.chord_timeout_ms)
    {
        state.pending_prefix = None;
    }

    if let Some((prefix, _)) = state.pending_prefix.clone() {
        if let Some(cmd) = map.chord(&prefix, &combo) {
            state.pending_prefix = None;
            return TerminalWebShortcutAction::Command(cmd);
        }
        state.pending_prefix = None;
    }

    if let Some(cmd) = map.direct(&combo)
        && (combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key)
    {
        return TerminalWebShortcutAction::Command(cmd);
    }

    if map.has_chord_prefix(&combo) {
        state.pending_prefix = Some((combo, now));
        return TerminalWebShortcutAction::Consume;
    }

    TerminalWebShortcutAction::PassThrough
}

fn term_key_event_to_shortcut_combo(event: &KeyStroke) -> Option<KeyCombo> {
    if event.is_modifier_key() {
        return None;
    }
    let key = shortcut_key_code_from_web_code(&event.code)?;
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

fn shortcut_key_code_from_web_code(code: &str) -> Option<KeyCode> {
    let key = key_code_from_web_code(code);
    if matches!(
        key,
        KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified)
    ) {
        None
    } else {
        Some(key)
    }
}

fn key_code_from_web_code(code: &str) -> KeyCode {
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

fn on_term_key(
    trigger: On<BinReceive<KeyStroke>>,
    terminals: Query<(), With<Terminal>>,
    q: Query<&ProcessId, With<Terminal>>,
    agents: Query<&vmux_core::agent::AgentSession>,
    launches: Query<&crate::launch::TerminalLaunch>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
    keymap: Res<Keymap>,
    mut web_shortcuts: ResMut<TerminalWebShortcutState>,
    mut command_invocations: MessageWriter<vmux_command::CommandInvocation>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut capture_q: Query<&mut PromptCapture, With<Terminal>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    if terminals.get(entity).is_err() {
        return;
    }
    match resolve_terminal_web_shortcut(event, &keymap, &mut web_shortcuts) {
        TerminalWebShortcutAction::Command(id) => {
            let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
            command_invocations.write(vmux_command::CommandInvocation::new(caller, id));
            if let Some(proxy) = proxy.as_ref() {
                let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
            }
            return;
        }
        TerminalWebShortcutAction::Consume => return,
        TerminalWebShortcutAction::PassThrough => {}
    }
    if event.is_modifier_key() {
        return;
    }
    let Some(service) = service else { return };
    let Ok(pid) = q.get(entity) else { return };
    let process_id = *pid;
    let is_vibe = agents.get(entity).ok().map(|session| session.kind)
        == Some(vmux_core::agent::AgentKind::Vibe)
        || launches.get(entity).ok().map(|launch| launch.kind.clone())
            == Some(crate::launch::TerminalKind::Vibe);
    if let Ok(mut capture) = capture_q.get_mut(entity) {
        let pasted = PromptCapture::wants_paste(event)
            .then(|| resolve_paste_text(is_vibe, process_id))
            .flatten();
        if capture.apply(event, pasted) {
            let (draft, skipped) = (capture.draft.clone(), capture.skipped);
            crate::TerminalUiStateUpdates::write(
                &mut commands,
                entity,
                &AgentPromptDraftEvent { draft, skipped },
            );
        }
        return;
    }
    let super_key = event.mods.super_key;
    if super_key {
        match event.code.as_str() {
            "KeyV" => {
                let agent_kind = agents.get(entity).ok().map(|session| session.kind);
                let launch_kind = launches.get(entity).ok().map(|launch| launch.kind.clone());
                let is_vibe = agent_kind == Some(vmux_core::agent::AgentKind::Vibe)
                    || launch_kind == Some(crate::launch::TerminalKind::Vibe);
                if let Some(data) = resolve_paste(is_vibe, process_id) {
                    service
                        .0
                        .send(ClientMessage::ProcessInput { process_id, data });
                }
                return;
            }
            "KeyC" => {
                service
                    .0
                    .send(ClientMessage::GetSelectionText { process_id });
                return;
            }
            _ => return,
        }
    }

    if is_copy_mode_active(&mode_map, &local_copy_mode, process_id) {
        let key = term_key_event_to_key(event);
        let mapped = map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput {
                key: &key,
                key_code: key_code_from_web_code(&event.code),
                ctrl: event.mods.ctrl,
                shift: event.mods.shift,
            },
        );
        for k in mapped {
            if copy_mode_key_exits(k) {
                local_copy_mode.set(process_id, false);
            }
            service
                .0
                .send(ClientMessage::CopyModeKey { process_id, key: k });
        }
        return;
    }

    let data = term_key_event_to_bytes(event);
    if !data.is_empty() {
        service
            .0
            .send(ClientMessage::ProcessInput { process_id, data });
    }
}

fn on_restart_pty(
    trigger: On<RestartPty>,
    mut q: Query<(
        &mut ProcessId,
        &mut PageMetadata,
        Option<&mut crate::launch::TerminalLaunch>,
        Option<&vmux_core::agent::AgentSession>,
        Option<&TerminalGridSize>,
        Has<crate::AgentRunTerminal>,
    )>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
    mut restart_agent: MessageWriter<vmux_core::agent::RestartAgentPty>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    let Some(service) = service else { return };
    let Ok((mut pid, mut meta, mut launch, agent_session, grid, agent_run)) = q.get_mut(entity)
    else {
        return;
    };

    if agent_session.is_some() {
        restart_agent.write(vmux_core::agent::RestartAgentPty { entity });
        return;
    }

    service
        .0
        .send(ClientMessage::KillProcess { process_id: *pid });

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
                .unwrap_or_else(default_shell);
            (shell, vec![], String::new(), Vec::new())
        }
    };
    if should_merge_login_shell_env(false, agent_run) {
        crate::shell_env::merge_login_shell_env(&mut env, &terminal_shell(&settings));
    }

    let (cols, rows) = grid.map(|g| (g.cols, g.rows)).unwrap_or((80, 24));
    let new_id = ProcessId::new();
    service.0.send(ClientMessage::CreateProcess {
        process_id: new_id,
        command: command.clone(),
        args: args.clone(),
        cwd: cwd.clone(),
        env: env.clone(),
        cols,
        rows,
    });

    *pid = new_id;
    mark_terminal_restarting(&mut commands, entity);
    if let Some(l) = launch.as_mut() {
        l.args = args;
    } else {
        meta.url = TERMINAL_PAGE_URL.to_string();
        meta.title = format!("Terminal ({})", &new_id.to_string()[..8]);
    }
}

fn handle_terminal_copy_mode_command(
    mut requests: MessageReader<super::command::CopyModeRequest>,
    targeted_terminals: Query<
        (&ProcessId, &ChildOf),
        (With<Terminal>, With<KeyboardOwner>, Without<ProcessExited>),
    >,
    keyboard_targets: Query<(), With<KeyboardOwner>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    service: Option<Res<ServiceClient>>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let Some(service) = service else {
        for _ in requests.read() {}
        return;
    };
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
            local_copy_mode.set(process_id, true);
            service.0.send(ClientMessage::EnterCopyMode { process_id });
        }
    }
}

fn handle_terminal_navigation_commands(
    mut close_requests: MessageReader<super::command::CloseRequest>,
    mut next_requests: MessageReader<super::command::NextRequest>,
    mut previous_requests: MessageReader<super::command::PrevRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
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

fn handle_terminal_clear_command(
    mut requests: MessageReader<super::command::ClearRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut sequence: ResMut<NextTerminalInputSequence>,
    mut commands: Commands,
) {
    let terminal = crate::target::active_terminal_for_tab(focus.stack, &terminals);
    for _ in requests.read() {
        let Some(terminal) = terminal else {
            continue;
        };
        TerminalInput::enqueue(&mut commands, &mut sequence, terminal, vec![0x0c]);
    }
}

fn is_copy_mode_active(
    mode_map: &TerminalModeMap,
    local_copy_mode: &LocalCopyModeState,
    process_id: ProcessId,
) -> bool {
    mode_map
        .modes
        .get(&process_id)
        .map(|m| m.copy_mode)
        .unwrap_or(false)
        || local_copy_mode.active.contains(&process_id)
}

fn copy_mode_key_exits(key: vmux_service::protocol::CopyModeKey) -> bool {
    use vmux_service::protocol::CopyModeKey as K;
    matches!(key, K::Copy | K::Exit)
}

#[derive(Message, Debug, Clone)]
pub struct ProcessExitedEvent {
    pub process_id: ProcessId,
}

#[derive(Message, Debug, Clone)]
pub struct CommandLifecycleEvent {
    pub process_id: ProcessId,
    pub kind: vmux_service::protocol::CommandLifecycleKind,
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
    process_index: Res<TerminalProcessIndex>,
    terminals: Query<Option<&PageIdentity>, With<Terminal>>,
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
    }
}

fn clear_osc_title_on_exit(
    mut reader: MessageReader<ProcessExitedEvent>,
    mut commands: Commands,
    process_index: Res<TerminalProcessIndex>,
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
    use crate::process_index::TerminalProcessIndexPlugin;
    use bevy::ecs::schedule::Schedules;
    use vmux_core::input::KeyModifiers;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};

    #[test]
    fn bracketed_paste_wraps_payload() {
        assert_eq!(bracketed_paste(b"hi"), b"\x1b[200~hi\x1b[201~".to_vec());
    }

    #[test]
    fn image_path_payload_uses_vibe_attach_syntax() {
        assert_eq!(image_path_payload(true, "/tmp/a b.png"), "'/tmp/a b.png'");
        assert_eq!(image_path_payload(false, "/tmp/a b.png"), "/tmp/a b.png");
        assert_eq!(
            image_path_payload(true, "/tmp/bob's.png"),
            "'/tmp/bob'\\''s.png'"
        );
    }

    #[test]
    fn write_clipboard_image_temp_writes_png_bytes() {
        let png = [137u8, 80, 78, 71, 1, 2, 3];
        let path = write_clipboard_image_temp(process_id(7), &png).expect("temp write");
        assert_eq!(std::fs::read(&path).unwrap(), png);
        let _ = std::fs::remove_file(&path);
    }

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    #[test]
    fn terminal_reinput_preserves_existing_queued_input() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TerminalProcessIndexPlugin, InputQueuePlugin));
        let pid = process_id(7);
        let terminal = app.world_mut().spawn((Terminal, pid)).id();
        app.world_mut()
            .run_system_cached_with(
                |In((terminal, data)): In<(Entity, Vec<u8>)>,
                 mut sequence: ResMut<NextTerminalInputSequence>,
                 mut commands: Commands| {
                    TerminalInput::enqueue(&mut commands, &mut sequence, terminal, data);
                },
                (terminal, b"initial\r".to_vec()),
            )
            .unwrap();

        app.world_mut()
            .resource_mut::<Messages<TerminalReinputRequest>>()
            .write(TerminalReinputRequest {
                process_id: pid,
                data: b"next\r".to_vec(),
            });
        app.update();

        assert_eq!(
            TerminalInput::pending(app.world_mut(), terminal),
            [b"initial\r".to_vec(), b"next\r".to_vec()]
        );
    }

    #[test]
    fn terminal_reinput_preserves_multiple_messages_in_order() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TerminalProcessIndexPlugin, InputQueuePlugin));
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
            TerminalInput::pending(app.world_mut(), terminal),
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
        app.add_plugins((MinimalPlugins, crate::host::request::TerminalRequestPlugin))
            .insert_resource(vmux_layout::stack::FocusedStack::default());

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
            TerminalInput::pending(app.world_mut(), terminal),
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
            .add_systems(Update, respond_terminal_stack_spawn);

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
            .query_filtered::<(Entity, &crate::launch::TerminalLaunch), With<Terminal>>();
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
            .init_resource::<vmux_space::spaces::ActiveSpace>()
            .add_systems(Update, handle_terminal_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: vmux_core::PageOpenId::new(),
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
        let record = vmux_space::model::bootstrap_space_record();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .add_systems(Update, handle_terminal_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert_eq!(launch.cwd, dir.path().to_string_lossy());
    }

    #[test]
    fn open_terminal_page_without_workspace_uses_shell_default() {
        let record = vmux_space::model::bootstrap_space_record();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .add_systems(Update, handle_terminal_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert!(launch.cwd.is_empty());
    }

    #[test]
    fn open_terminal_page_prefers_ancestor_tab_startup_dir() {
        let space_dir = tempfile::tempdir().unwrap();
        let tab_dir = tempfile::tempdir().unwrap();
        let record = vmux_space::model::bootstrap_space_record();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(space_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .add_systems(Update, handle_terminal_page_open);

        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "t".into(),
                startup_dir: Some(tab_dir.path().to_string_lossy().into()),
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert_eq!(
            launch.cwd,
            tab_dir.path().canonicalize().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn open_terminal_page_rejects_invalid_ancestor_tab_startup_dir() {
        let fallback_dir = tempfile::tempdir().unwrap();
        let record = vmux_space::model::bootstrap_space_record();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(fallback_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .add_systems(Update, handle_terminal_page_open);

        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "t".into(),
                startup_dir: Some("/no/such/vmux-tab-workspace".into()),
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: vmux_core::PageOpenId::new(),
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
        let record = vmux_space::model::bootstrap_space_record();
        let mut settings = test_settings();
        settings.spaces.insert(
            record.id.clone(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(fallback_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<TerminalLayoutSpawnRequest>()
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .add_systems(Update, spawn_layout_requested_content);

        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "t".into(),
                startup_dir: Some("/no/such/vmux-tab-workspace".into()),
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
            .id();
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
    fn missing_service_process_restart_preserves_launch() {
        let target = Entity::from_bits(1);
        let launch = crate::launch::TerminalLaunch {
            command: default_shell(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
            kind: crate::launch::TerminalKind::Plain,
        };
        let restart = MissingTerminalRestart::new(target, launch, None);

        assert_eq!(restart.entity, target);
        assert!(restart.agent_kind.is_none());
        assert!(matches!(
            restart.command,
            ClientMessage::CreateProcess {
                process_id: _,
                command,
                args,
                cwd,
                env,
                cols: 80,
                rows: 24
            } if command == default_shell() && args.is_empty() && cwd.is_empty() && env.is_empty()
        ));
    }

    #[test]
    fn process_create_budget_bounds_in_flight() {
        assert_eq!(
            process_create_budget(0, 8),
            8,
            "full budget when nothing in flight"
        );
        assert_eq!(process_create_budget(3, 8), 5);
        assert_eq!(process_create_budget(8, 8), 0, "no budget at the cap");
        assert_eq!(
            process_create_budget(99, 8),
            0,
            "never negative when over the cap"
        );
    }

    #[test]
    fn process_not_found_message_parses_process_id() {
        let missing = process_id(9);

        assert_eq!(
            missing_process_id(&format!("process not found: {missing}")),
            Some(missing)
        );
        assert_eq!(missing_process_id("permission denied"), None);
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
    fn agent_focus_transition_restores_focus_to_active_blurred_agent() {
        assert_eq!(
            agent_focus_transition(true, true, true),
            Some(AgentFocusTransition::FocusIn)
        );
    }

    #[test]
    fn web_terminal_key_events_delegate_text_to_pty_bytes() {
        let event = KeyStroke {
            key: "a".to_string(),
            code: "KeyA".to_string(),
            text: Some("a".to_string()),
            ..Default::default()
        };

        assert_eq!(term_key_event_to_bytes(&event), b"a".to_vec());
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

        assert_eq!(term_key_event_to_bytes(&event), vec![3]);
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

        assert!(term_key_event_to_bytes(&event).is_empty());
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
        let mut state = TerminalWebShortcutState::default();
        let definitions = [vmux_command::CommandDefinition::new(
            "browser_open_page_in_command_bar",
            "Edit Page",
            "Browser > Bar",
        )
        .direct("Super+l")];
        let keymap = Keymap::defaults_with(&definitions);

        assert_eq!(
            resolve_terminal_web_shortcut(&event, &keymap, &mut state),
            TerminalWebShortcutAction::Command("browser_open_page_in_command_bar".to_string())
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
        let mut state = TerminalWebShortcutState::default();
        let definitions = [vmux_command::CommandDefinition::new(
            "toggle_layout",
            "Toggle Layout",
            "Layout > Layout",
        )
        .direct("Super+Shift+S")];
        let keymap = Keymap::defaults_with(&definitions);

        assert_eq!(
            resolve_terminal_web_shortcut(&event, &keymap, &mut state),
            TerminalWebShortcutAction::Command("toggle_layout".to_string())
        );
    }

    #[test]
    fn shell_prompt_ready_only_once_cursor_is_past_column_zero() {
        assert!(!shell_prompt_ready(false, 0), "no output yet");
        assert!(
            !shell_prompt_ready(true, 0),
            "banner line ends in a newline (cursor at column 0)"
        );
        assert!(
            !shell_prompt_ready(true, 0),
            "further banner lines are still column 0"
        );
        assert!(
            shell_prompt_ready(true, 3),
            "drawn prompt leaves the cursor after the prompt string"
        );
    }

    #[test]
    fn vim_visual_keys_map_to_copy_mode_actions() {
        use vmux_service::protocol::CopyModeKey as K;

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
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert_eq!(
            map_copy_mode_key_with_state(
                &mut local_copy_mode,
                process_id,
                &Key::Character("g".into()),
                false
            ),
            None
        );
        assert_eq!(
            map_copy_mode_key_with_state(
                &mut local_copy_mode,
                process_id,
                &Key::Character("_".into()),
                false
            ),
            Some(K::LastNonBlank)
        );
    }

    #[test]
    fn vim_visual_motion_keys_map_to_copy_mode_actions() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
            ),
            vec![K::WordForward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::shift(&Key::Character("W".into()), KeyCode::KeyW)
            ),
            vec![K::BigWordForward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("b".into()), KeyCode::KeyB)
            ),
            vec![K::WordBackward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
            ),
            vec![K::WordEndForward]
        );

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
            ),
            vec![K::WordEndBackward]
        );

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("3".into()), KeyCode::Digit3)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
            ),
            vec![K::WordForward, K::WordForward, K::WordForward]
        );
    }

    #[test]
    fn shifted_minus_resolves_g_() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::shift(&Key::Character("-".into()), KeyCode::Minus)
            ),
            vec![K::LastNonBlank]
        );
    }

    #[test]
    fn local_copy_mode_is_active_before_service_broadcast() {
        let process_id = ProcessId::new();
        let mode_map = TerminalModeMap::default();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert!(!is_copy_mode_active(
            &mode_map,
            &local_copy_mode,
            process_id
        ));

        local_copy_mode.set(process_id, true);

        assert!(is_copy_mode_active(&mode_map, &local_copy_mode, process_id));
    }

    #[test]
    fn service_copy_mode_broadcast_reconciles_local_latch() {
        let process_id = ProcessId::new();
        let mut mode_map = TerminalModeMap::default();
        let mut local_copy_mode = LocalCopyModeState::default();

        local_copy_mode.set(process_id, true);
        mode_map.modes.insert(
            process_id,
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
                alt_screen: false,
                focus_reporting: false,
            },
        );
        local_copy_mode.set(process_id, false);

        assert!(!is_copy_mode_active(
            &mode_map,
            &local_copy_mode,
            process_id
        ));
    }

    #[test]
    fn exiting_copy_mode_clears_local_latch() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();
        local_copy_mode.set(process_id, true);

        if copy_mode_key_exits(K::Exit) {
            local_copy_mode.set(process_id, false);
        }

        assert!(!local_copy_mode.active.contains(&process_id));
    }

    #[test]
    fn restart_state_clears_shell_output_seen_and_preserves_pending_input() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputQueuePlugin));
        let entity = app.world_mut().spawn((Terminal, ShellOutputSeen)).id();
        app.world_mut()
            .run_system_cached_with(
                |In((terminal, data)): In<(Entity, Vec<u8>)>,
                 mut sequence: ResMut<NextTerminalInputSequence>,
                 mut commands: Commands| {
                    TerminalInput::enqueue(&mut commands, &mut sequence, terminal, data);
                },
                (entity, b"queued\r".to_vec()),
            )
            .unwrap();

        app.world_mut()
            .run_system_cached_with(
                |In(entity): In<Entity>, mut commands: Commands| {
                    mark_terminal_restarting(&mut commands, entity);
                },
                entity,
            )
            .unwrap();

        assert!(app.world().get::<ShellOutputSeen>(entity).is_none());
        assert!(app.world().get::<AwaitingProcessCreated>(entity).is_some());
        assert_eq!(
            TerminalInput::pending(app.world_mut(), entity),
            [b"queued\r".to_vec()]
        );
    }

    #[test]
    fn process_created_matches_by_id_not_by_position() {
        use crate::launch::{TerminalKind, TerminalLaunch};

        let mut app = bevy::prelude::App::new();
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
                    kind: TerminalKind::Plain,
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
                    kind: TerminalKind::Plain,
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
                    kind: TerminalKind::Plain,
                },
            ))
            .id();

        for (process_id, pid) in [(id3, 333u32), (id1, 111), (id2, 222)] {
            let entity = app
            .world_mut()
            .query_filtered::<(bevy::prelude::Entity, &ProcessId), With<AwaitingProcessCreated>>()
            .iter(app.world())
            .find(|(_, pid_c)| **pid_c == process_id)
            .map(|(e, _)| e)
            .expect("matching entity for process_id");
            app.world_mut()
                .run_system_cached_with(
                    |In((entity, process_id, pid)): In<(Entity, ProcessId, u32)>,
                     mut commands: Commands| {
                        apply_process_created(&mut commands, entity, process_id, pid);
                    },
                    (entity, process_id, pid),
                )
                .unwrap();
        }

        let world = app.world();
        assert_eq!(world.get::<crate::pid::Pid>(e1).map(|p| p.0), Some(111));
        assert_eq!(world.get::<crate::pid::Pid>(e2).map(|p| p.0), Some(222));
        assert_eq!(world.get::<crate::pid::Pid>(e3).map(|p| p.0), Some(333));
    }

    #[test]
    fn apply_process_created_stamps_pid_and_process_id() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app
            .world_mut()
            .spawn((Terminal, AwaitingProcessCreated))
            .id();
        let id = process_id(7);
        let pid_val = 4242u32;
        app.world_mut()
            .run_system_cached_with(
                |In((entity, id, pid_val)): In<(Entity, ProcessId, u32)>,
                 mut commands: Commands| {
                    apply_process_created(&mut commands, entity, id, pid_val);
                },
                (entity, id, pid_val),
            )
            .unwrap();
        let stored_pid = app.world().get::<pid::Pid>(entity).unwrap();
        assert_eq!(stored_pid.0, pid_val);
        assert!(app.world().get::<AwaitingProcessCreated>(entity).is_none());
        let stored_process_id = app.world().get::<ProcessId>(entity).unwrap();
        assert_eq!(*stored_process_id, id);
    }

    #[test]
    fn apply_process_create_failed_despawns_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app
            .world_mut()
            .spawn((Terminal, AwaitingProcessCreated))
            .id();
        app.world_mut()
            .run_system_cached_with(
                |In(entity): In<Entity>, mut commands: Commands| {
                    apply_process_create_failed(&mut commands, entity);
                },
                entity,
            )
            .unwrap();
        assert!(
            !app.world().entities().contains(entity),
            "failed create must despawn the orphaned terminal so no system is left to drive or reap it"
        );
    }

    #[test]
    fn apply_osc_title_sets_and_clears() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TerminalProcessIndexPlugin))
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
                .get::<vmux_core::PageIdentity>(e)
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
        assert!(app.world().get::<vmux_core::PageIdentity>(e).is_none());
    }

    #[test]
    fn clear_osc_title_on_exit_removes_override() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TerminalProcessIndexPlugin))
            .add_message::<ProcessExitedEvent>()
            .add_systems(Update, clear_osc_title_on_exit);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((Terminal, pid, vmux_core::PageIdentity::from("working")))
            .id();

        app.world_mut()
            .resource_mut::<Messages<ProcessExitedEvent>>()
            .write(ProcessExitedEvent { process_id: pid });
        app.update();
        assert!(app.world().get::<vmux_core::PageIdentity>(e).is_none());
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
        assert!(!should_close_terminal_stack_on_exit(false, true));
    }

    #[test]
    fn agent_run_terminal_inherits_login_shell_environment() {
        assert!(should_merge_login_shell_env(false, true));
        assert!(should_merge_login_shell_env(true, false));
        assert!(!should_merge_login_shell_env(false, false));
    }
}
