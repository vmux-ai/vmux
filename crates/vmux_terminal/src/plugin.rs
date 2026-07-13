use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::{
    ecs::relationship::Relationship,
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    picking::Pickable,
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_command::shortcut::{KeyCombo, Modifiers, Shortcut};
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, OpenCommand, PaneDirection, PaneOpenMode,
    PaneTarget, StackCommand, WriteAppCommands,
};
use vmux_core::page::PageReady;
use vmux_core::terminal::{ProcessesMonitorSpawnRequest, TerminalSpawnRequest};
use vmux_core::{
    OscTitle, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::{CloseRequiresConfirmation, LayoutSpawnRequest};
use vmux_service::{
    client::{ServiceHandle, ServiceWake},
    protocol::{ClientMessage, ProcessId, ServiceMessage},
};
use vmux_setting::{AppSettings, SettingsSaveRequest};

use crate::event::*;
use crate::pid::{self, Pid};
use crate::processes_monitor::ProcessesMonitor;
use crate::{ProcessExited, RetainOnProcessExit, Terminal};

const MULTI_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
const MULTI_CLICK_CELL_TOLERANCE: i32 = 1;
/// `Ctrl+V` control byte. Sent on ⌘V when the clipboard holds an image, so the
/// focused agent CLI (Claude Code / Codex) reads the image from the pasteboard
/// itself — image data never transits the PTY.
const CTRL_V: u8 = 0x16;

/// Check if confirmation is needed based on settings.
pub fn should_confirm_close(settings: &AppSettings) -> bool {
    settings.terminal.as_ref().is_none_or(|t| t.confirm_close)
}

/// Check if a tab entity has any child terminal that is still running.
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

/// Show confirmation dialog for quitting with N running terminals.
pub fn confirm_quit_dialog(count: usize) -> bool {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let msg = if count == 1 {
        "A terminal is still running. Quit anyway?".to_string()
    } else {
        format!("{count} terminals are still running. Quit anyway?")
    };
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Quit Vmux?")
        .set_description(&msg)
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}

pub use vmux_service::client::ServiceClient;

#[derive(Resource, Clone)]
struct ServiceWakeCallback(Option<ServiceWake>);

/// Per-process terminal mode flags, last broadcast by the service.
#[derive(Resource, Default)]
pub struct TerminalModeMap {
    pub modes: std::collections::HashMap<ProcessId, TerminalModeFlags>,
}

/// Optimistic copy-mode state owned by the desktop. The service confirms
/// asynchronously via `TerminalMode`, but keyboard routing must switch on
/// immediately after the shortcut or first mouse drag.
#[derive(Resource, Default)]
struct LocalCopyModeState {
    active: std::collections::HashSet<ProcessId>,
    input_states: std::collections::HashMap<ProcessId, CopyModeInputState>,
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

const AGENT_LOADING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Minimum time the loading splash stays up for a plain (non-agent) terminal,
/// whose shell prints its prompt almost instantly. Agents instead clear when the
/// TUI takes over (alt-screen).
const TERMINAL_LOADING_MIN_DISPLAY: std::time::Duration = std::time::Duration::from_millis(700);

#[derive(Component, Debug, Clone, Copy)]
pub struct AgentLoading {
    pub since: Instant,
}

/// A prompt to type into an agent terminal once its TUI is up. Delivered by
/// [`flush_buffered_agent_prompt`] on alt-screen.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct BufferedAgentPrompt {
    pub text: String,
    pub submit: bool,
}

/// While an agent terminal's TUI is booting, the prompt the user types on the
/// boot screen. Keystrokes are captured here instead of being sent to the
/// not-yet-ready PTY (see [`handle_terminal_keyboard`]); on alt-screen
/// [`clear_agent_loading`] moves a non-empty draft into a [`BufferedAgentPrompt`]
/// and removes this, so keys then flow to the PTY normally.
#[derive(Component, Debug, Clone, Default)]
pub struct PromptCapture {
    pub draft: String,
    pub skipped: bool,
}

/// Bytes to deliver for a buffered prompt once the agent TUI is ready, or `None`
/// if not ready yet or there's nothing to send.
fn agent_prompt_flush_bytes(alt_screen: bool, buf: &BufferedAgentPrompt) -> Option<Vec<u8>> {
    if !alt_screen {
        return None;
    }
    let bytes = crate::shell_input::bracketed_paste_input(&buf.text, buf.submit);
    (!bytes.is_empty()).then_some(bytes)
}

fn flush_buffered_agent_prompt(
    q: Query<(Entity, &ProcessId, &BufferedAgentPrompt), With<vmux_core::agent::AgentSession>>,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    for (entity, pid, buf) in &q {
        // Readiness is already decided by `clear_agent_loading` (which only
        // creates a `BufferedAgentPrompt` once the TUI is up); deliver as soon as
        // one exists. Re-gating on `alt_screen` here strands the prompt forever
        // for inline TUIs (Claude Code, Codex, Vibe) that never use alt-screen.
        if let Some(data) = agent_prompt_flush_bytes(true, buf) {
            service.0.send(ClientMessage::ProcessInput {
                process_id: *pid,
                data,
            });
        }
        commands.entity(entity).remove::<BufferedAgentPrompt>();
    }
}

/// Last char-grid size (cols/rows) the page measured for this terminal, so a PTY
/// restart can recreate the process at the current pane size instead of 80x24.
#[derive(Component, Debug, Clone, Copy)]
pub struct TerminalGridSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TerminalGridSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// Triggered to restart the terminal process for a terminal entity.
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
pub struct TerminalSendRequest {
    pub text: String,
    pub terminal: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellMode {
    NewTab,
    Active,
}

#[derive(Message, Clone)]
pub struct RunShellRequest {
    pub command: String,
    pub cwd: String,
    pub mode: ShellMode,
}

#[derive(Message, Clone)]
pub struct TerminalStackSpawnRequest {
    pub pane: Entity,
    pub cwd: Option<PathBuf>,
    pub shell: Option<String>,
    pub agent_run: bool,
    pub pending_input: Option<Vec<u8>>,
    /// Pin this `ProcessId` on the spawned terminal (so the caller can address
    /// it later); `None` lets the bundle mint a fresh one.
    pub process_id: Option<ProcessId>,
    /// When true, the new stack is activated (focus moves to it). When false,
    /// focus stays where it is.
    pub activate: bool,
}

/// Wires the terminal domain: PTY spawning via the background service, terminal/stack/
/// process-monitor requests, keyboard and mouse forwarding, and snapshot updates.
pub struct TerminalPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceMessageSet;

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
            Some(Pid(p)) => format!("{TERMINAL_PAGE_URL}{p}"),
            None => TERMINAL_PAGE_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, "terminal");
        vmux_core::register_host_spawn(app, "services");
        app.register_type::<crate::launch::TerminalLaunch>()
            .register_type::<crate::launch::TerminalKind>()
            .add_message::<TerminalSendRequest>()
            .add_message::<RunShellRequest>()
            .add_message::<TerminalStackSpawnRequest>()
            .add_message::<TerminalSpawnRequest>()
            .add_message::<ProcessesMonitorSpawnRequest>()
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<vmux_service::agent_events::AgentCommandResultEvent>()
            .add_message::<vmux_service::agent_events::AgentQueryResultEvent>()
            .init_resource::<pid::PidToEntity>()
            .add_systems(
                Update,
                (pid::track_pid_inserts, pid::track_pid_removals).chain(),
            );
        let service_wake = service_wake_callback(app);
        ensure_service_started();
        app.insert_resource(ServiceConnectRetry::new());
        app.insert_resource(ServiceWakeCallback(service_wake))
            .init_resource::<MouseSelectionState>()
            .init_resource::<TerminalModeMap>()
            .init_resource::<LocalCopyModeState>()
            .init_resource::<TerminalWebShortcutState>()
            .add_systems(Update, format_terminal_url.after(pid::track_pid_inserts))
            .add_plugins(BinEventEmitterPlugin::<(
                TermResizeEvent,
                TermMouseEvent,
                TermScrollEvent,
                TermKeyEvent,
                TermLinkOpenRequest,
            )>::for_hosts(&["terminal"]))
            .add_systems(
                PreUpdate,
                handle_terminal_keyboard
                    .run_if(on_message::<KeyboardInput>)
                    .after(InputSystems),
            );
        add_terminal_update_systems(app)
            .add_systems(
                Update,
                (
                    handle_terminal_send_requests,
                    handle_run_shell_requests,
                    respond_terminal_stack_spawn,
                )
                    .after(ServiceMessageSet),
            )
            .add_systems(
                Update,
                (respond_terminal_spawn, respond_processes_monitor_spawn)
                    .in_set(vmux_command::ReadAppCommands),
            )
            .add_systems(
                Update,
                handle_terminal_font_size.after(vmux_command::ReadAppCommands),
            )
            .add_observer(on_term_ready)
            .add_observer(on_term_resize)
            .add_observer(on_term_mouse)
            .add_observer(on_term_scroll)
            .add_observer(on_term_key)
            .add_observer(on_term_link_open)
            .add_observer(on_restart_pty)
            .add_observer(on_terminal_removed)
            .add_plugins(crate::processes_monitor::ProcessesMonitorPlugin)
            .add_systems(
                Update,
                crate::snapshot_updater::update_terminals_snapshot
                    .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
            )
            .add_systems(
                Update,
                (
                    arm_agent_loading,
                    arm_agent_loading_on_restart,
                    clear_agent_loading.after(poll_service_messages),
                    flush_buffered_agent_prompt.after(poll_service_messages),
                    reset_terminal_title_on_agent_removed,
                    set_terminal_shell_icon,
                ),
            );
    }
}

/// Give a plain terminal a shell-specific tab icon (nushell/bash/zsh) derived
/// from its launch command. Agents keep their own icons; unrecognized shells
/// keep the generic terminal icon.
fn set_terminal_shell_icon(
    mut q: Query<(&crate::launch::TerminalLaunch, &mut vmux_core::PageMetadata), With<Terminal>>,
) {
    for (launch, mut meta) in &mut q {
        if !matches!(launch.kind, crate::launch::TerminalKind::Plain) {
            continue;
        }
        if !meta.icon.is_none() {
            continue;
        }
        if let Some(icon) = vmux_core::BuiltinIcon::for_shell(&launch.command) {
            meta.icon = vmux_core::PageIcon::Builtin(icon);
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

fn add_terminal_update_systems(app: &mut App) -> &mut App {
    app.add_message::<ProcessExitedEvent>()
        .add_message::<CommandLifecycleEvent>()
        .add_message::<TerminalReinputRequest>()
        .add_message::<OscTitleChanged>()
        .add_message::<vmux_core::notify::BellReceived>()
        .add_systems(
            Update,
            handle_terminal_reinput_requests
                .after(poll_service_messages)
                .before(flush_pending_terminal_input),
        )
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
                poll_service_messages
                    .in_set(WriteAppCommands)
                    .in_set(ServiceMessageSet),
                flush_pending_terminal_input,
                handle_terminal_copy_mode_command.in_set(vmux_command::ReadAppCommands),
            )
                .chain(),
        )
        .add_systems(Update, sync_terminal_theme.after(handle_terminal_font_size))
}

fn spawn_layout_requested_content(
    mut reader: MessageReader<LayoutSpawnRequest>,
    settings: Res<AppSettings>,
    active_space: Res<vmux_space::spaces::ActiveSpace>,
    child_of: Query<&ChildOf>,
    tabs: Query<&vmux_layout::tab::Tab>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        match request {
            LayoutSpawnRequest::Terminal { stack } => {
                let tab_dir = vmux_layout::tab::ancestor_tab_startup_dir(*stack, &child_of, &tabs);
                let cwd = vmux_setting::resolve_startup_dir_for_tab(
                    &settings,
                    &active_space.record.id,
                    tab_dir.as_deref(),
                );
                let terminal = commands
                    .spawn((
                        new_terminal_bundle_with_cwd(
                            &mut meshes,
                            &mut webview_mt,
                            &settings,
                            Some(&cwd),
                        ),
                        ChildOf(*stack),
                    ))
                    .id();
                commands.entity(terminal).insert(CefKeyboardTarget);
            }
        }
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
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
                &mut meshes,
                &mut webview_mt,
            ) {
                Ok(()) => {
                    commands.entity(entity).insert(PageOpenHandled);
                }
                Err(message) => {
                    commands.entity(entity).insert(PageOpenError { message });
                }
            }
        } else if task.url.starts_with(vmux_layout::event::SERVICES_PAGE_URL) {
            clear_stack_children(task.stack, &children_q, &mut commands);
            commands.entity(task.stack).insert(PageMetadata {
                url: vmux_layout::event::SERVICES_PAGE_URL.to_string(),
                title: "Background Services".to_string(),
                bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
                ..default()
            });
            commands.spawn((
                ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                ChildOf(task.stack),
            ));
            commands.entity(entity).insert(PageOpenHandled);
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
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
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
        Some(vmux_setting::resolve_startup_dir_for_tab(
            settings,
            &active_space.record.id,
            tab_dir.as_deref(),
        ))
    };
    clear_stack_children(task.stack, children_q, commands);
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
            new_terminal_bundle_with_cwd(meshes, webview_mt, settings, cwd.as_deref()),
            ChildOf(task.stack),
        ))
        .id();
    commands.entity(terminal).insert(CefKeyboardTarget);
    Ok(())
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

fn respond_terminal_spawn(
    mut reader: MessageReader<TerminalSpawnRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: Res<AppSettings>,
    child_of_q: Query<&ChildOf>,
) {
    for req in reader.read() {
        let term_e = commands
            .spawn(new_terminal_bundle_with_cwd(
                &mut meshes,
                &mut webview_mt,
                &settings,
                req.cwd.as_deref(),
            ))
            .id();
        commands.entity(term_e).insert(CefKeyboardTarget);
        if let Some(stack_e) = req.target_stack {
            commands.entity(term_e).insert(ChildOf(stack_e));
            commands.entity(stack_e).insert(LastActivatedAt::now());
            if let Ok(parent) = child_of_q.get(stack_e) {
                commands.entity(parent.0).insert(LastActivatedAt::now());
            }
        }
    }
}

fn respond_processes_monitor_spawn(
    mut reader: MessageReader<ProcessesMonitorSpawnRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for req in reader.read() {
        let entity = commands
            .spawn(crate::processes_monitor::ProcessesMonitor::new(
                &mut meshes,
                &mut webview_mt,
            ))
            .id();
        commands.entity(entity).insert(ChildOf(req.target_stack));
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

pub fn new_terminal_bundle(
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> impl Bundle {
    new_terminal_bundle_with_cwd(meshes, webview_mt, settings, None)
}

pub fn new_terminal_bundle_with_cwd(
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
    cwd: Option<&std::path::Path>,
) -> impl Bundle {
    new_terminal_bundle_with_cwd_and_shell(meshes, webview_mt, settings, cwd, None)
}

fn new_terminal_bundle_with_cwd_and_shell(
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
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
            WebviewSource::new(TERMINAL_PAGE_URL),
            ResolvedWebviewUri(TERMINAL_PAGE_URL.to_string()),
            Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                Vec3::Z,
                Vec2::splat(0.5),
            ))),
        ),
        (
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            TerminalGridSize::default(),
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

pub fn respond_terminal_stack_spawn(
    mut reader: MessageReader<TerminalStackSpawnRequest>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
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
                    &mut meshes,
                    &mut webview_mt,
                    &settings,
                    request.cwd.as_deref(),
                    request.shell.as_deref(),
                ),
                ChildOf(stack),
            ))
            .id();
        commands.entity(terminal).insert(CefKeyboardTarget);
        if request.agent_run {
            commands.entity(terminal).insert(crate::AgentRunTerminal);
        }
        if let Some(pid) = request.process_id {
            commands.entity(terminal).insert(pid);
        }
        if let Some(data) = request.pending_input.clone() {
            commands
                .entity(terminal)
                .insert(PendingTerminalInput { data });
        }
    }
}

pub fn reattach_terminal_bundle(
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    process_id: ProcessId,
) -> impl Bundle {
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
            WebviewSource::new(TERMINAL_PAGE_URL),
            ResolvedWebviewUri(TERMINAL_PAGE_URL.to_string()),
            Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                Vec3::Z,
                Vec2::splat(0.5),
            ))),
        ),
        (
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            TerminalGridSize::default(),
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

#[derive(Component)]
pub struct PendingServiceCreate;

/// Temporary component: terminal needs an AttachProcess sent to service.
#[derive(Component)]
struct PendingServiceAttach;

#[derive(Component)]
pub struct PendingTerminalInput {
    pub data: Vec<u8>,
}

/// Marker: the terminal's shell has drawn its prompt and is reading input. Used
/// to defer flushing [`PendingTerminalInput`] until then, so a `run` command
/// isn't raw-echoed above the prompt before the shell renders it. Readiness is
/// decided by [`shell_prompt_ready`], which distinguishes the prompt from
/// earlier pre-prompt output (e.g. a node-version banner).
#[derive(Component)]
struct ShellOutputSeen;

/// Whether a viewport update means the shell has drawn its prompt and is reading
/// input, used to gate [`PendingTerminalInput`]. `has_content` is whether any
/// updated line carries non-whitespace and `cursor_col` is the cursor column
/// after the update.
///
/// A shell that prints a banner (e.g. fnm `Using Node vX`) before drawing its
/// prompt would trip a naive "any output" check on the banner, so a `run`
/// command gets raw-echoed above the prompt. A drawn prompt leaves the cursor
/// after the prompt string (`cursor_col > 0`); every banner line ends in a
/// newline (cursor back at column 0), so even multi-line startup output is
/// skipped until the prompt itself appears.
fn shell_prompt_ready(has_content: bool, cursor_col: u16) -> bool {
    has_content && cursor_col > 0
}

/// Marker: CreateProcess was sent, waiting for ProcessCreated response.
#[derive(Component)]
pub struct AwaitingProcessCreated;

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

/// A `CreateProcess` failed (e.g. the system PTY pool is exhausted). Despawn the
/// terminal entity: with no `ProcessId` and its in-flight markers gone, no system
/// would ever drive or reap it, so leaving it behind orphans a dead pane. Despawn
/// also frees the create budget (the entity drops out of the awaiting query).
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

fn terminal_shell(settings: &AppSettings) -> String {
    settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(default_shell)
}

/// Max `CreateProcess` requests outstanding (awaiting `ProcessCreated`) at once.
/// Restoring a large saved space would otherwise open hundreds of PTYs in one
/// tick and exhaust the system PTY pool (macOS `kern.tty.ptmx_max` ≈ 511),
/// which crashes startup. Bounding concurrency spreads restore across ticks;
/// each `ProcessCreated` response frees budget and wakes the loop for the next.
const MAX_CONCURRENT_PROCESS_CREATES: usize = 8;

/// How many new `CreateProcess` requests may be dispatched this tick, given how
/// many are already awaiting a `ProcessCreated` response.
fn process_create_budget(in_flight: usize, max_concurrent: usize) -> usize {
    max_concurrent.saturating_sub(in_flight)
}

fn missing_terminal_restart(
    process_id: ProcessId,
    terminals: impl IntoIterator<
        Item = (
            Entity,
            ProcessId,
            crate::launch::TerminalLaunch,
            Option<vmux_core::agent::AgentKind>,
        ),
    >,
) -> Option<MissingTerminalRestart> {
    terminals
        .into_iter()
        .find(|(_, terminal_pid, _, _)| *terminal_pid == process_id)
        .map(|(entity, _, launch, agent_kind)| {
            let new_id = ProcessId::new();
            let cwd = launch.cwd.clone();
            MissingTerminalRestart {
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
        })
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
    let binary = match vmux_service::daemon_binary_path() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "could not locate vmux_service binary");
            return;
        }
    };
    match vmux_service::registry::start_mode_for(&binary) {
        vmux_service::registry::StartMode::Register => {
            let profile = vmux_service::current_profile();
            if let Err(e) = vmux_service::registry::ensure_running(profile, &binary) {
                tracing::error!(error = ?e, "service registration failed");
            }
        }
        vmux_service::registry::StartMode::SpawnDetached => spawn_detached_service(&binary),
    }
}

#[cfg(unix)]
fn spawn_detached_service(binary: &std::path::Path) {
    use std::os::unix::process::CommandExt;
    let log_dir = vmux_service::log_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let stderr_cfg = match std::fs::File::create(vmux_service::log_path()) {
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
    browsers: &NonSend<Browsers>,
    commands: &mut Commands,
    message: String,
) {
    let evt = ServiceUnavailableEvent { message };
    for entity in terminals.iter() {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                SERVICE_UNAVAILABLE_EVENT,
                &evt,
            ));
        }
    }
}

fn try_connect_service(
    mut retry: ResMut<ServiceConnectRetry>,
    time: Res<Time>,
    mut commands: Commands,
    wake: Res<ServiceWakeCallback>,
    terminal_webviews: Query<Entity, With<Terminal>>,
    browsers: NonSend<Browsers>,
) {
    retry.timer.tick(time.delta());
    if !retry.timer.just_finished() {
        return;
    }

    retry.remaining_attempts = retry.remaining_attempts.saturating_sub(1);

    let sock = vmux_service::socket_path();
    if !sock.exists() {
        if retry.remaining_attempts == 0 {
            tracing::warn!("service socket never appeared — giving up");
            commands.remove_resource::<ServiceConnectRetry>();
            broadcast_service_unavailable(
                &terminal_webviews,
                &browsers,
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
            broadcast_service_unavailable(
                &terminal_webviews,
                &browsers,
                &mut commands,
                String::new(),
            );
        }
        None => {
            if retry.remaining_attempts == 0 {
                tracing::error!("failed to connect to service after all retries");
                let log_path = vmux_service::log_path();
                if let Ok(log) = std::fs::read_to_string(&log_path)
                    && !log.is_empty()
                {
                    tracing::error!(service_log = %log, "service log contents");
                }
                commands.remove_resource::<ServiceConnectRetry>();
                broadcast_service_unavailable(
                    &terminal_webviews,
                    &browsers,
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
    app_commands: MessageWriter<'w, AppCommand>,
    agent_commands: MessageWriter<'w, vmux_service::agent_events::AgentCommandRequest>,
    agent_queries: MessageWriter<'w, vmux_service::agent_events::AgentQueryRequest>,
    agent_tool_calls: MessageWriter<'w, vmux_service::agent_events::AgentToolCallRequest>,
    page_agent_delta: MessageWriter<'w, vmux_service::agent_events::PageAgentDelta>,
    page_agent_run_status: MessageWriter<'w, vmux_service::agent_events::PageAgentRunStatus>,
    page_agent_awaiting: MessageWriter<'w, vmux_service::agent_events::PageAgentAwaitingApproval>,
    page_agent_snapshot: MessageWriter<'w, vmux_service::agent_events::PageAgentSnapshot>,
    page_agent_info: MessageWriter<'w, vmux_service::agent_events::PageAgentInfo>,
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

/// True when a rendered line carries any non-whitespace text. Used to decide
/// when a shell has actually drawn its prompt (vs. a blank pre-prompt frame), so
/// pending `run` input is flushed only once the line editor is ready.
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
    mut mode_map: ResMut<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
    mut mouse_state: ResMut<MouseSelectionState>,
    settings: Res<AppSettings>,
    launches: Query<&crate::launch::TerminalLaunch>,
    agent_sessions: Query<&vmux_core::agent::AgentSession>,
    output_seen: Query<(), With<ShellOutputSeen>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let Some(service) = service else { return };

    // Handle pending creates — send CreateProcess, wait for ProcessCreated
    // response which will carry the real process ID. Throttle by in-flight count
    // so restoring a large saved space can't open hundreds of PTYs in one tick.
    let create_budget = process_create_budget(
        awaiting_create.iter().count(),
        MAX_CONCURRENT_PROCESS_CREATES,
    );
    for (entity, process_id, launch, agent_run) in pending_create.iter().take(create_budget) {
        // Agents run as bare executables and don't load the user's shell config
        // the way a terminal does, so merge in the login-shell env (API keys
        // etc.). Done here (at spawn) rather than at launch-build time so it
        // also covers agents restored from a persisted space or restarted.
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

    // Handle pending attaches
    for (entity, pid) in &pending_attach {
        service
            .0
            .send(ClientMessage::AttachProcess { process_id: *pid });
        service
            .0
            .send(ClientMessage::RequestSnapshot { process_id: *pid });
        commands.entity(entity).remove::<PendingServiceAttach>();
    }

    // Drain service messages and dispatch
    let mut restarted_missing_processes = Vec::new();
    let (messages, capped) = service.0.drain_with_status();
    if capped && let Some(proxy) = proxy.as_deref() {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
    for msg in messages {
        match msg {
            ServiceMessage::ProcessCreated { process_id, pid } => {
                let entity = (&awaiting_create)
                    .into_iter()
                    .find(|(_, pid_c, _)| **pid_c == process_id)
                    .map(|(e, _, _)| e);
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
                if let Some(entity) = (&awaiting_create)
                    .into_iter()
                    .find(|(_, pid_c, _)| **pid_c == process_id)
                    .map(|(e, _, _)| e)
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
                for (entity, pid, _, _) in &terminals {
                    if *pid == process_id {
                        if !output_seen.contains(entity) {
                            let has_content =
                                changed_lines.iter().any(|(_, l)| line_has_content(l));
                            if shell_prompt_ready(has_content, cursor.col) {
                                commands.entity(entity).insert(ShellOutputSeen);
                            }
                        }
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
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
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
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
                for (entity, pid, _, _) in &terminals {
                    if *pid == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let evt = TermTitleEvent { title };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_TITLE_EVENT,
                            &evt,
                        ));
                        break;
                    }
                }
            }
            ServiceMessage::Snapshot {
                process_id,
                lines,
                cursor,
                cols,
                rows,
            } => {
                for (entity, pid, _, _) in &terminals {
                    if *pid == process_id {
                        if !output_seen.contains(entity) {
                            let has_content = lines.iter().any(line_has_content);
                            if shell_prompt_ready(has_content, cursor.col) {
                                commands.entity(entity).insert(ShellOutputSeen);
                            }
                        }
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
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
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
            }
            ServiceMessage::ProcessExited { process_id, .. } => {
                writers
                    .process_exited
                    .write(ProcessExitedEvent { process_id });
                mode_map.modes.remove(&process_id);
                set_local_copy_mode(&mut local_copy_mode, process_id, false);
                mouse_state.per_process.remove(&process_id);
                for (entity, pid, child_of, retain_on_exit) in &terminals {
                    if *pid == process_id {
                        commands
                            .entity(entity)
                            .insert(ProcessExited)
                            .remove::<CloseRequiresConfirmation>()
                            .remove::<AgentLoading>();
                        let is_agent = if let Ok(session) = agent_sessions.get(entity) {
                            commands.trigger(BinHostEmitEvent::from_rkyv(
                                entity,
                                TERM_LOADING_EVENT,
                                &crate::event::TermLoadingEvent {
                                    loading: false,
                                    label: session.kind.display_name().to_string(),
                                    segment: session.kind.as_url_segment().to_string(),
                                },
                            ));
                            true
                        } else {
                            false
                        };
                        // Plain terminals close their stack on exit. Agent
                        // terminals are closed by the agent crate (it
                        // force-closes the whole pane), so skip the stack close
                        // here to avoid a double close collapsing the wrong pane.
                        if should_close_terminal_stack_on_exit(is_agent, retain_on_exit) {
                            let tab = child_of.get();
                            commands.entity(tab).insert(LastActivatedAt::now());
                            writers
                                .app_commands
                                .write(AppCommand::Layout(LayoutCommand::Stack(
                                    StackCommand::Close,
                                )));
                        }
                        break;
                    }
                }
            }
            ServiceMessage::ProcessList { processes } => {
                commands
                    .insert_resource(crate::processes_monitor::ServiceProcessList { processes });
            }
            ServiceMessage::Error { message } => {
                if let Some(stale_pid) = missing_process_id(&message)
                    && !restarted_missing_processes.contains(&stale_pid)
                {
                    let candidates = terminals.iter().map(|(entity, terminal_pid, _, _)| {
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
                        (entity, *terminal_pid, launch, agent_kind)
                    });
                    if let Some(restart) = missing_terminal_restart(stale_pid, candidates) {
                        restarted_missing_processes.push(stale_pid);
                        let cwd = restart.cwd.clone();
                        let agent_kind = restart.agent_kind;
                        let new_id = restart.new_id;
                        let entity = restart.entity;
                        service.0.send(restart.command);
                        let mut ec = commands.entity(entity);
                        ec.insert(new_id);
                        ec.insert(AwaitingProcessCreated);
                        if let Some(kind) = agent_kind {
                            ec.insert(vmux_core::agent::PendingAgentSession {
                                kind,
                                spawn_time: std::time::SystemTime::now(),
                                cwd: std::path::PathBuf::from(&cwd),
                            });
                        }
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
                mode_map.modes.insert(
                    process_id,
                    TerminalModeFlags {
                        mouse_capture,
                        copy_mode,
                        alt_screen,
                        focus_reporting,
                    },
                );
                set_local_copy_mode(&mut local_copy_mode, process_id, copy_mode);
            }
            ServiceMessage::SelectionText {
                process_id: _,
                text,
            } if !text.is_empty() => {
                crate::clipboard::write(text);
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
                args_json,
            } => {
                writers
                    .agent_tool_calls
                    .write(vmux_service::agent_events::AgentToolCallRequest {
                        request_id,
                        sid,
                        name,
                        args_json,
                    });
            }
            ServiceMessage::AgentDelta { sid, text } => {
                writers
                    .page_agent_delta
                    .write(vmux_service::agent_events::PageAgentDelta { sid, text });
            }
            ServiceMessage::AgentRunStatusChanged { sid, status } => {
                writers
                    .page_agent_run_status
                    .write(vmux_service::agent_events::PageAgentRunStatus { sid, status });
            }
            ServiceMessage::AgentAwaitingApproval {
                sid,
                call_id,
                name,
                args_json,
            } => {
                writers.page_agent_awaiting.write(
                    vmux_service::agent_events::PageAgentAwaitingApproval {
                        sid,
                        call_id,
                        name,
                        args_json,
                    },
                );
            }
            ServiceMessage::AgentMessagesSnapshot { sid, messages_json } => {
                writers
                    .page_agent_snapshot
                    .write(vmux_service::agent_events::PageAgentSnapshot { sid, messages_json });
            }
            ServiceMessage::AcpAgentInfo { sid, name } => {
                writers
                    .page_agent_info
                    .write(vmux_service::agent_events::PageAgentInfo { sid, name });
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

fn flush_pending_terminal_input(
    pending: Query<
        (Entity, &ProcessId, &PendingTerminalInput),
        (
            With<Terminal>,
            With<ShellOutputSeen>,
            Without<PendingServiceCreate>,
            Without<AwaitingProcessCreated>,
            Without<ProcessExited>,
        ),
    >,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    for (entity, pid, input) in &pending {
        service.0.send(ClientMessage::ProcessInput {
            process_id: *pid,
            data: input.data.clone(),
        });
        commands.entity(entity).remove::<PendingTerminalInput>();
    }
}

fn handle_terminal_reinput_requests(
    mut requests: MessageReader<TerminalReinputRequest>,
    terminals: Query<(Entity, &ProcessId), With<Terminal>>,
    mut pending_inputs: Query<&mut PendingTerminalInput>,
    mut commands: Commands,
) {
    let mut queued = std::collections::HashMap::<Entity, Vec<u8>>::new();
    for req in requests.read() {
        for (entity, pid) in &terminals {
            if *pid == req.process_id {
                queued
                    .entry(entity)
                    .or_default()
                    .extend_from_slice(&req.data);
            }
        }
    }
    for (entity, data) in queued {
        if let Ok(mut pending) = pending_inputs.get_mut(entity) {
            pending.data.extend(data);
        } else {
            commands
                .entity(entity)
                .insert(PendingTerminalInput { data });
        }
    }
}

fn is_non_character_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::ArrowLeft
            | KeyCode::ArrowUp
            | KeyCode::ArrowRight
            | KeyCode::ArrowDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Escape
            | KeyCode::Tab
            | KeyCode::Enter
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Insert
    )
}

fn handle_terminal_keyboard(
    mut er: MessageReader<KeyboardInput>,
    targeted_terminals: Query<
        (&ProcessId, &ChildOf),
        (
            With<Terminal>,
            With<CefKeyboardTarget>,
            Without<ProcessExited>,
        ),
    >,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    terminal_kinds: Query<
        (
            &ProcessId,
            Option<&vmux_core::agent::AgentSession>,
            Option<&crate::launch::TerminalLaunch>,
        ),
        With<Terminal>,
    >,
    mut capture_q: Query<(Entity, &ProcessId, &mut PromptCapture), With<Terminal>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mode: Res<vmux_layout::scene::InteractionMode>,
    input: Res<ButtonInput<KeyCode>>,
    chord_state: Res<vmux_command::shortcut::ChordState>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
    mut commands: Commands,
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
        *mode,
    );

    if target_processes.is_empty() {
        for _ in er.read() {}
        return;
    };
    let active_process_id = target_processes.first().copied();
    let Some(service) = service else {
        for _ in er.read() {}
        return;
    };
    if chord_state.pending_prefix.is_some() {
        for _ in er.read() {}
        return;
    }
    let ctrl = input.pressed(KeyCode::ControlLeft) || input.pressed(KeyCode::ControlRight);
    let alt = input.pressed(KeyCode::AltLeft) || input.pressed(KeyCode::AltRight);
    let shift = input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight);
    let super_key = input.pressed(KeyCode::SuperLeft) || input.pressed(KeyCode::SuperRight);

    if let Some(cap_e) = active_process_id.and_then(|pid| {
        capture_q
            .iter()
            .find(|(_, p, _)| **p == pid)
            .map(|(e, _, _)| e)
    }) {
        let (mut draft, mut skipped) = capture_q
            .get(cap_e)
            .map(|(_, _, c)| (c.draft.clone(), c.skipped))
            .unwrap_or_default();
        let is_vibe = active_process_id
            .and_then(|pid| terminal_kinds.iter().find(|(p, ..)| **p == pid))
            .map(|(_, agent, launch)| {
                agent.map(|s| s.kind) == Some(vmux_core::agent::AgentKind::Vibe)
                    || launch.map(|l| l.kind.clone()) == Some(crate::launch::TerminalKind::Vibe)
            })
            .unwrap_or(false);
        let mut changed = false;
        let mut seen_keys: Vec<KeyCode> = Vec::new();
        for event in er.read() {
            if event.state != ButtonState::Pressed {
                continue;
            }
            if !event.repeat && is_non_character_key(event.key_code) {
                if seen_keys.contains(&event.key_code) {
                    continue;
                }
                seen_keys.push(event.key_code);
                if !input.just_pressed(event.key_code) {
                    continue;
                }
            }
            if ctrl && event.key_code == KeyCode::KeyC {
                draft.clear();
                skipped = false;
                changed = true;
                continue;
            }
            if super_key && event.key_code == KeyCode::KeyV {
                if let Some(pasted) =
                    active_process_id.and_then(|pid| resolve_paste_text(is_vibe, pid))
                {
                    if !draft.is_empty() && !draft.ends_with(char::is_whitespace) {
                        draft.push(' ');
                    }
                    draft.push_str(&pasted);
                    skipped = false;
                    changed = true;
                }
                continue;
            }
            match &event.logical_key {
                Key::Escape => {
                    draft.clear();
                    skipped = true;
                    changed = true;
                }
                Key::Backspace => {
                    draft.pop();
                    changed = true;
                }
                Key::Space => {
                    draft.push(' ');
                    skipped = false;
                    changed = true;
                }
                Key::Character(s) if !ctrl && !alt && !super_key => {
                    draft.push_str(s);
                    skipped = false;
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            if let Ok((_, _, mut cap)) = capture_q.get_mut(cap_e) {
                cap.draft = draft.clone();
                cap.skipped = skipped;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                cap_e,
                AGENT_PROMPT_DRAFT_EVENT,
                &AgentPromptDraftEvent { draft, skipped },
            ));
        }
        return;
    }

    let copy_mode_active = active_process_id
        .map(|process_id| is_copy_mode_active(&mode_map, &local_copy_mode, process_id))
        .unwrap_or(false);

    let mut seen_keys: Vec<KeyCode> = Vec::new();
    for event in er.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        if !event.repeat && is_non_character_key(event.key_code) {
            if seen_keys.contains(&event.key_code) {
                continue;
            }
            seen_keys.push(event.key_code);
            if !input.just_pressed(event.key_code) {
                continue;
            }
        }

        if copy_mode_active {
            let Some(active_process_id) = active_process_id else {
                continue;
            };
            let mapped = map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                active_process_id,
                CopyModeKeyInput {
                    key: &event.logical_key,
                    key_code: event.key_code,
                    ctrl,
                    shift,
                },
            );
            for k in mapped {
                if copy_mode_key_exits(k) {
                    set_local_copy_mode(&mut local_copy_mode, active_process_id, false);
                }
                service.0.send(ClientMessage::CopyModeKey {
                    process_id: active_process_id,
                    key: k,
                });
            }
            // While in copy mode, swallow ALL keys — never forward to PTY.
            continue;
        }

        if super_key {
            match event.key_code {
                KeyCode::KeyV => {
                    let active = active_process_id
                        .and_then(|pid| terminal_kinds.iter().find(|(p, ..)| **p == pid));
                    let agent_kind = active.and_then(|(_, agent, _)| agent.map(|s| s.kind));
                    let launch_kind =
                        active.and_then(|(_, _, launch)| launch.map(|l| l.kind.clone()));
                    let is_vibe = agent_kind == Some(vmux_core::agent::AgentKind::Vibe)
                        || launch_kind == Some(crate::launch::TerminalKind::Vibe);
                    if let Some(data) =
                        active_process_id.and_then(|pid| resolve_paste(is_vibe, pid))
                    {
                        for process_id in &target_processes {
                            service.0.send(ClientMessage::ProcessInput {
                                process_id: *process_id,
                                data: data.clone(),
                            });
                        }
                    }
                    continue;
                }
                KeyCode::KeyC => {
                    // Round-trip selection through the service, then copy to pasteboard.
                    if let Some(process_id) = active_process_id {
                        service
                            .0
                            .send(ClientMessage::GetSelectionText { process_id });
                    }
                    continue;
                }
                _ => continue,
            }
        }

        // Skip selection keys (Shift+Arrow etc) — service doesn't support local selection
        if shift
            && matches!(
                event.key_code,
                KeyCode::ArrowLeft
                    | KeyCode::ArrowRight
                    | KeyCode::ArrowUp
                    | KeyCode::ArrowDown
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown
            )
        {
            continue;
        }

        let bytes = logical_key_to_bytes(&event.logical_key, ctrl, alt);
        if bytes.is_empty() {
            continue;
        }
        for process_id in &target_processes {
            service.0.send(ClientMessage::ProcessInput {
                process_id: *process_id,
                data: bytes.clone(),
            });
        }
    }
}

/// Translate a Bevy logical key + ctrl modifier into the corresponding
/// Vim-style copy-mode action. Returns None if the key has no copy-mode
/// binding (caller should swallow it regardless).
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
    mode: vmux_layout::scene::InteractionMode,
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
        if mode == vmux_layout::scene::InteractionMode::User
            && let Some(focused) = focused
        {
            return focused;
        }
        if mode == vmux_layout::scene::InteractionMode::User && focused_stack.is_some() {
            return Vec::new();
        }
        return targeted
            .into_iter()
            .map(|(_, process_id)| process_id)
            .collect();
    }
    if any_keyboard_target_active || mode != vmux_layout::scene::InteractionMode::User {
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

fn term_key_event_to_key(event: &TermKeyEvent) -> Key {
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
        _ => {
            let text = event.text.as_deref().unwrap_or(event.key.as_str());
            Key::Character(text.into())
        }
    }
}

/// Wrap `payload` in terminal bracketed-paste markers.
fn bracketed_paste(payload: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(payload.len() + 12);
    data.extend_from_slice(b"\x1b[200~");
    data.extend_from_slice(payload);
    data.extend_from_slice(b"\x1b[201~");
    data
}

/// Paste payload for an image file path. Vibe attaches a single-quoted path
/// (it prepends its own `@` on paste; the quotes keep paths with spaces as one
/// token, with embedded `'` shell-escaped); Claude Code and Codex auto-detect a
/// bare path.
fn image_path_payload(is_vibe: bool, path: &str) -> String {
    if is_vibe {
        format!("'{}'", path.replace('\'', "'\\''"))
    } else {
        path.to_string()
    }
}

/// Write clipboard PNG bytes to a unique temp file and return its path. The
/// per-paste sequence number prevents a later paste from overwriting an earlier
/// one still pending delivery (e.g. several images in a boot-screen draft).
fn write_clipboard_image_temp(process_id: ProcessId, png: &[u8]) -> Option<std::path::PathBuf> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("vmux-clip-{process_id}-{seq}.png"));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

/// Resolve the bytes to forward to the PTY for a ⌘V paste. A copied image *file*
/// is pasted as its path; raw image *data* goes to the CLI via `Ctrl+V` (Claude
/// Code, Codex read the pasteboard), except Vibe (`is_vibe`) — which cannot read
/// the pasteboard, so its data is written to a temp file and pasted as a path.
/// Otherwise the clipboard text is bracketed-pasted. `None` when nothing to paste.
fn resolve_paste(is_vibe: bool, process_id: ProcessId) -> Option<Vec<u8>> {
    if let Some(path) = crate::clipboard::image_file_path() {
        return Some(bracketed_paste(
            image_path_payload(is_vibe, &path).as_bytes(),
        ));
    }
    if crate::clipboard::has_image() {
        if is_vibe {
            let png = crate::clipboard::read_image_png()?;
            let path = write_clipboard_image_temp(process_id, &png)?;
            let payload = image_path_payload(true, &path.to_string_lossy());
            return Some(bracketed_paste(payload.as_bytes()));
        }
        return Some(vec![CTRL_V]);
    }
    let text = crate::clipboard::read_blocking()?;
    (!text.is_empty()).then(|| bracketed_paste(text.as_bytes()))
}

/// Text to append to an agent's boot-time draft prompt for a ⌘V paste. Unlike
/// [`resolve_paste`], an image always resolves to a *path* (raw clipboard data is
/// written to a temp file) — the booting CLI isn't running yet to read the
/// pasteboard via `Ctrl+V`, and the draft is delivered later as text. Returns
/// `None` when there is nothing to paste.
fn resolve_paste_text(is_vibe: bool, process_id: ProcessId) -> Option<String> {
    if let Some(path) = crate::clipboard::image_file_path() {
        return Some(image_path_payload(is_vibe, &path));
    }
    if crate::clipboard::has_image() {
        let png = crate::clipboard::read_image_png()?;
        let path = write_clipboard_image_temp(process_id, &png)?;
        return Some(image_path_payload(is_vibe, &path.to_string_lossy()));
    }
    let text = crate::clipboard::read_blocking()?;
    (!text.is_empty()).then_some(text)
}

fn term_key_event_to_bytes(event: &TermKeyEvent) -> Vec<u8> {
    if is_web_modifier_key_event(event) {
        return Vec::new();
    }
    let ctrl = event.modifiers & MOD_CTRL != 0;
    let alt = event.modifiers & MOD_ALT != 0;
    let key = term_key_event_to_key(event);
    logical_key_to_bytes(&key, ctrl, alt)
}

fn is_web_modifier_key_event(event: &TermKeyEvent) -> bool {
    matches!(
        event.key.as_str(),
        "Shift" | "Control" | "Alt" | "Meta" | "OS" | "Fn" | "CapsLock"
    ) || matches!(
        event.code.as_str(),
        "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
            | "OSLeft"
            | "OSRight"
            | "CapsLock"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalWebShortcutAction {
    Command(AppCommand),
    Consume,
    PassThrough,
}

struct TerminalWebShortcutMap {
    bindings: Vec<(Shortcut, String)>,
    chord_timeout_ms: u64,
}

fn terminal_web_shortcut_map(settings: Option<&AppSettings>) -> TerminalWebShortcutMap {
    let mut map = TerminalWebShortcutMap {
        bindings: AppCommand::default_shortcuts(),
        chord_timeout_ms: 1000,
    };

    if let Some(settings) = settings {
        map.chord_timeout_ms = settings.shortcuts.chord_timeout_ms;
        if let Some(leader) = settings.shortcuts.leader.to_key_combo() {
            for (binding, _) in &mut map.bindings {
                if let Shortcut::Chord(prefix, _) = binding {
                    *prefix = leader.clone();
                }
            }
            for entry in &settings.shortcuts.bindings {
                if let Some(binding) = entry.binding.to_shortcut_with_leader(&leader) {
                    map.bindings.push((binding, entry.command.clone()));
                }
            }
        } else {
            for entry in &settings.shortcuts.bindings {
                if let Some(binding) = entry.binding.to_shortcut() {
                    map.bindings.push((binding, entry.command.clone()));
                }
            }
        }
    }

    map
}

fn resolve_terminal_web_shortcut(
    event: &TermKeyEvent,
    settings: Option<&AppSettings>,
    state: &mut TerminalWebShortcutState,
) -> TerminalWebShortcutAction {
    let Some(combo) = term_key_event_to_shortcut_combo(event) else {
        return TerminalWebShortcutAction::PassThrough;
    };
    let map = terminal_web_shortcut_map(settings);
    let now = Instant::now();
    if let Some((_, started)) = state.pending_prefix.as_ref()
        && now.duration_since(*started) > Duration::from_millis(map.chord_timeout_ms)
    {
        state.pending_prefix = None;
    }

    if let Some((prefix, _)) = state.pending_prefix.clone() {
        if let Some(cmd) = terminal_web_chord_command(&map, &prefix, &combo) {
            state.pending_prefix = None;
            return TerminalWebShortcutAction::Command(cmd);
        }
        state.pending_prefix = None;
    }

    if let Some(cmd) = terminal_web_direct_command(&map, &combo)
        && (combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key)
    {
        return TerminalWebShortcutAction::Command(cmd);
    }

    if terminal_web_has_chord_prefix(&map, &combo) {
        state.pending_prefix = Some((combo, now));
        return TerminalWebShortcutAction::Consume;
    }

    TerminalWebShortcutAction::PassThrough
}

fn term_key_event_to_shortcut_combo(event: &TermKeyEvent) -> Option<KeyCombo> {
    if is_web_modifier_key_event(event) {
        return None;
    }
    let key = shortcut_key_code_from_web_code(&event.code)?;
    Some(KeyCombo {
        key,
        modifiers: Modifiers {
            ctrl: event.modifiers & MOD_CTRL != 0,
            shift: event.modifiers & MOD_SHIFT != 0,
            alt: event.modifiers & MOD_ALT != 0,
            super_key: event.modifiers & MOD_SUPER != 0,
        },
    })
}

fn terminal_web_direct_command(
    map: &TerminalWebShortcutMap,
    pressed: &KeyCombo,
) -> Option<AppCommand> {
    map.bindings
        .iter()
        .find_map(|(binding, cmd_id)| match binding {
            Shortcut::Direct(combo) if combo == pressed => {
                terminal_command_from_shortcut_id(cmd_id)
            }
            _ => None,
        })
}

fn terminal_web_has_chord_prefix(map: &TerminalWebShortcutMap, pressed: &KeyCombo) -> bool {
    map.bindings
        .iter()
        .any(|(binding, _)| matches!(binding, Shortcut::Chord(prefix, _) if prefix == pressed))
}

fn terminal_web_chord_command(
    map: &TerminalWebShortcutMap,
    prefix: &KeyCombo,
    pressed: &KeyCombo,
) -> Option<AppCommand> {
    let effective = effective_terminal_web_chord_second(prefix, pressed);
    map.bindings
        .iter()
        .find_map(|(binding, cmd_id)| match binding {
            Shortcut::Chord(binding_prefix, second)
                if binding_prefix == prefix && second == &effective =>
            {
                terminal_command_from_shortcut_id(cmd_id)
            }
            _ => None,
        })
}

fn effective_terminal_web_chord_second(prefix: &KeyCombo, pressed: &KeyCombo) -> KeyCombo {
    let mut effective = pressed.clone();
    if prefix.modifiers.ctrl {
        effective.modifiers.ctrl = false;
    }
    if prefix.modifiers.alt {
        effective.modifiers.alt = false;
    }
    if prefix.modifiers.super_key {
        effective.modifiers.super_key = false;
    }
    effective
}

fn terminal_command_from_shortcut_id(cmd_id: &str) -> Option<AppCommand> {
    match cmd_id {
        "split_v" => Some(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            },
        ))),
        "split_h" => Some(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Bottom,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            },
        ))),
        _ => AppCommand::from_menu_id(cmd_id),
    }
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

/// Encode a mouse event as an SGR escape sequence.
fn sgr_mouse_sequence(button: u8, col: u16, row: u16, modifiers: u8, pressed: bool) -> Vec<u8> {
    let mut cb = button as u32;
    if modifiers & MOD_SHIFT != 0 {
        cb += 4;
    }
    if modifiers & MOD_ALT != 0 {
        cb += 8;
    }
    if modifiers & MOD_CTRL != 0 {
        cb += 16;
    }
    let suffix = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{};{};{}{}", cb, col + 1, row + 1, suffix).into_bytes()
}

/// Tracks the most recent mouse-down per process for click-count detection
/// (300ms / ~1 cell window) and an active drag anchor.
#[derive(Resource, Default)]
struct MouseSelectionState {
    per_process: std::collections::HashMap<ProcessId, MouseSessionState>,
}

#[derive(Default, Clone, Debug)]
struct MouseSessionState {
    last_click: Option<MouseClickRecord>,
    drag_active: bool,
    drag_visual_active: bool,
    /// Last (col, row) sent via ExtendSelectionTo during the active drag.
    /// Used to dedupe redundant move events at the same cell.
    last_extend_cell: Option<(u16, u16)>,
    /// Anchor cell from the most recent left-click that has not yet
    /// produced a real selection. We defer materializing the selection
    /// until the user actually drags so a single click doesn't draw a
    /// 1-character selection box.
    pending_anchor: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug)]
struct MouseClickRecord {
    when: std::time::Instant,
    col: u16,
    row: u16,
    count: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MouseTerminalAction {
    ForwardInput(Vec<u8>),
    EnterCopyMode,
    ExitCopyMode,
    SetSelection(Option<TermSelectionRange>),
    ExtendSelectionTo { col: u16, row: u16 },
    SelectWordAt { col: u16, row: u16 },
    SelectLineAt { row: u16 },
}

fn mouse_terminal_actions(
    entry: &mut MouseSessionState,
    event: &TermMouseEvent,
    mouse_capture: bool,
    now: std::time::Instant,
) -> Vec<MouseTerminalAction> {
    let shift = event.modifiers & MOD_SHIFT != 0;
    let is_left = event.button == 0;
    let select_mode = is_left && (!mouse_capture || shift);

    if !select_mode {
        if !mouse_capture {
            return Vec::new();
        }
        let button = if event.moving {
            event.button + 32
        } else {
            event.button
        };
        return vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
            button,
            event.col,
            event.row,
            event.modifiers,
            event.pressed,
        ))];
    }

    if event.pressed && !event.moving {
        let count = match entry.last_click {
            Some(prev)
                if now.duration_since(prev.when) <= MULTI_CLICK_WINDOW
                    && (prev.col as i32 - event.col as i32).abs() <= MULTI_CLICK_CELL_TOLERANCE
                    && (prev.row as i32 - event.row as i32).abs() <= MULTI_CLICK_CELL_TOLERANCE =>
            {
                if prev.count >= 3 {
                    1
                } else {
                    prev.count + 1
                }
            }
            _ => 1,
        };
        entry.last_click = Some(MouseClickRecord {
            when: now,
            col: event.col,
            row: event.row,
            count,
        });
        entry.drag_active = count == 1;
        entry.drag_visual_active = false;
        entry.last_extend_cell = Some((event.col, event.row));

        match count {
            1 if shift => {
                entry.pending_anchor = None;
                vec![MouseTerminalAction::ExtendSelectionTo {
                    col: event.col,
                    row: event.row,
                }]
            }
            1 => {
                entry.pending_anchor = Some((event.col, event.row));
                vec![MouseTerminalAction::SetSelection(None)]
            }
            2 => {
                entry.pending_anchor = None;
                vec![MouseTerminalAction::SelectWordAt {
                    col: event.col,
                    row: event.row,
                }]
            }
            _ => {
                entry.pending_anchor = None;
                vec![MouseTerminalAction::SelectLineAt { row: event.row }]
            }
        }
    } else if event.moving && entry.drag_active {
        if entry.last_extend_cell == Some((event.col, event.row)) {
            return Vec::new();
        }
        entry.last_extend_cell = Some((event.col, event.row));
        if let Some((ac, ar)) = entry.pending_anchor.take() {
            entry.drag_visual_active = true;
            vec![
                MouseTerminalAction::EnterCopyMode,
                MouseTerminalAction::SetSelection(Some(TermSelectionRange {
                    start_col: ac,
                    start_row: ar,
                    end_col: event.col,
                    end_row: event.row,
                    is_block: false,
                })),
            ]
        } else {
            vec![MouseTerminalAction::ExtendSelectionTo {
                col: event.col,
                row: event.row,
            }]
        }
    } else if !event.pressed {
        let actions = if entry.drag_visual_active {
            vec![MouseTerminalAction::ExitCopyMode]
        } else {
            Vec::new()
        };
        entry.drag_active = false;
        entry.drag_visual_active = false;
        entry.last_extend_cell = None;
        entry.pending_anchor = None;
        actions
    } else {
        Vec::new()
    }
}

fn send_mouse_action(service: &ServiceHandle, process_id: ProcessId, action: MouseTerminalAction) {
    match action {
        MouseTerminalAction::ForwardInput(data) => {
            service.send(ClientMessage::ProcessInput { process_id, data });
        }
        MouseTerminalAction::EnterCopyMode => {
            service.send(ClientMessage::EnterCopyMode { process_id });
        }
        MouseTerminalAction::ExitCopyMode => {
            service.send(ClientMessage::ExitCopyMode { process_id });
        }
        MouseTerminalAction::SetSelection(range) => {
            service.send(ClientMessage::SetSelection { process_id, range });
        }
        MouseTerminalAction::ExtendSelectionTo { col, row } => {
            service.send(ClientMessage::ExtendSelectionTo {
                process_id,
                col,
                row,
            });
        }
        MouseTerminalAction::SelectWordAt { col, row } => {
            service.send(ClientMessage::SelectWordAt {
                process_id,
                col,
                row,
            });
        }
        MouseTerminalAction::SelectLineAt { row } => {
            service.send(ClientMessage::SelectLineAt { process_id, row });
        }
    }
}

/// Handle mouse events from the terminal webview.
///
/// Selection mode (left-button + (no app mouse-capture OR shift held)) is
/// intercepted and translated into selection commands sent to the service.
/// Anything else is forwarded as SGR mouse-report bytes to the PTY, but only
/// when the app enabled mouse capture — otherwise a plain shell would echo
/// hover/motion reports as literal `^[[<..M` text.
fn on_term_mouse(
    trigger: On<BinReceive<TermMouseEvent>>,
    q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut state: ResMut<MouseSelectionState>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(pid) = q.get(entity) else { return };
    let process_id = *pid;

    if event.button == 64 || event.button == 65 {
        service.0.send(ClientMessage::MouseWheel {
            process_id,
            up: event.button == 64,
            col: event.col,
            row: event.row,
            modifiers: event.modifiers,
        });
        return;
    }

    let mouse_capture = mode_map
        .modes
        .get(&process_id)
        .map(|m| m.mouse_capture)
        .unwrap_or(false);
    let entry = state.per_process.entry(process_id).or_default();
    for action in mouse_terminal_actions(entry, event, mouse_capture, std::time::Instant::now()) {
        update_local_copy_mode_for_mouse_action(&mut local_copy_mode, process_id, &action);
        send_mouse_action(&service.0, process_id, action);
    }
}

/// Native-scroll intent from the terminal page: forward the requested window top
/// (and follow state) to the service, which serves the document-row window.
fn on_term_scroll(
    trigger: On<BinReceive<TermScrollEvent>>,
    q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(pid) = q.get(entity) else { return };
    service.0.send(ClientMessage::ScrollWindow {
        process_id: *pid,
        top_row: event.top_row,
        follow: event.follow,
    });
}

/// Open a URL or file the user cmd+clicked in the terminal, in a new stack
/// beside the current pane. Mirrors the web-shortcut dispatch in `on_term_key`.
fn on_term_link_open(
    trigger: On<BinReceive<TermLinkOpenRequest>>,
    mut app_commands: MessageWriter<AppCommand>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let url = trigger.payload.url.clone();
    if url.is_empty() {
        return;
    }
    let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
        url: Some(url),
    }));
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    issued.write(vmux_command::CommandIssued {
        caller,
        command: cmd.clone(),
    });
    app_commands.write(cmd);
    if let Some(proxy) = proxy.as_ref() {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

fn on_term_key(
    trigger: On<BinReceive<TermKeyEvent>>,
    q: Query<&ProcessId, With<Terminal>>,
    agents: Query<&vmux_core::agent::AgentSession>,
    launches: Query<&crate::launch::TerminalLaunch>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
    settings: Option<Res<AppSettings>>,
    mut web_shortcuts: ResMut<TerminalWebShortcutState>,
    mut app_commands: MessageWriter<AppCommand>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    match resolve_terminal_web_shortcut(event, settings.as_deref(), &mut web_shortcuts) {
        TerminalWebShortcutAction::Command(cmd) => {
            let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            app_commands.write(cmd);
            if let Some(proxy) = proxy.as_ref() {
                let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
            }
            return;
        }
        TerminalWebShortcutAction::Consume => return,
        TerminalWebShortcutAction::PassThrough => {}
    }
    if is_web_modifier_key_event(event) {
        return;
    }
    let Some(service) = service else { return };
    let Ok(pid) = q.get(entity) else { return };
    let process_id = *pid;
    let super_key = event.modifiers & MOD_SUPER != 0;
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
                ctrl: event.modifiers & MOD_CTRL != 0,
                shift: event.modifiers & MOD_SHIFT != 0,
            },
        );
        for k in mapped {
            if copy_mode_key_exits(k) {
                set_local_copy_mode(&mut local_copy_mode, process_id, false);
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

/// Loading splash label + url segment for a terminal. Agents use their brand
/// (color + name); plain terminals use a generic "Terminal" / default accent.
fn terminal_loading_labels(session: Option<&vmux_core::agent::AgentSession>) -> (String, String) {
    match session {
        Some(s) => (
            s.kind.display_name().to_string(),
            s.kind.as_url_segment().to_string(),
        ),
        None => ("Terminal".to_string(), "terminal".to_string()),
    }
}

fn arm_agent_loading(
    newly_ready: Query<
        (Entity, Option<&vmux_core::agent::AgentSession>),
        (With<Terminal>, Added<PageReady>, Without<AgentLoading>),
    >,
    mut commands: Commands,
) {
    for (entity, session) in &newly_ready {
        let (label, segment) = terminal_loading_labels(session);
        commands.entity(entity).insert(AgentLoading {
            since: Instant::now(),
        });
        if session.is_some() {
            commands.entity(entity).insert(PromptCapture::default());
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            TERM_LOADING_EVENT,
            &crate::event::TermLoadingEvent {
                loading: true,
                label,
                segment,
            },
        ));
    }
}

fn arm_agent_loading_on_restart(
    restarted: Query<
        (Entity, Option<&vmux_core::agent::AgentSession>),
        (
            With<Terminal>,
            With<PageReady>,
            Without<AgentLoading>,
            Changed<ProcessId>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, session) in &restarted {
        let (label, segment) = terminal_loading_labels(session);
        commands.entity(entity).insert(AgentLoading {
            since: Instant::now(),
        });
        if session.is_some() {
            commands.entity(entity).insert(PromptCapture::default());
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            TERM_LOADING_EVENT,
            &crate::event::TermLoadingEvent {
                loading: true,
                label,
                segment,
            },
        ));
    }
}

fn clear_agent_loading(
    loading_q: Query<
        (
            Entity,
            &ProcessId,
            Option<&vmux_core::agent::AgentSession>,
            &AgentLoading,
            Option<&PromptCapture>,
        ),
        With<Terminal>,
    >,
    mode_map: Res<TerminalModeMap>,
    mut commands: Commands,
) {
    for (entity, pid, session, loading, capture) in &loading_q {
        // Agents clear when the TUI takes over its terminal. Inline TUIs (Claude
        // Code, Codex, Vibe) never enter alt-screen, so also treat mouse or focus
        // reporting — enabled when their input is ready — as readiness. Plain
        // terminals, whose shell prints instantly, show a brief minimum splash.
        let ready = match session {
            Some(_) => mode_map
                .modes
                .get(pid)
                .map(|m| m.alt_screen || m.mouse_capture || m.focus_reporting)
                .unwrap_or(false),
            None => loading.since.elapsed() >= TERMINAL_LOADING_MIN_DISPLAY,
        };
        if ready || loading.since.elapsed() >= AGENT_LOADING_TIMEOUT {
            let (label, segment) = terminal_loading_labels(session);
            // Flip keyboard back to the PTY: deliver the captured boot prompt (if
            // any) and drop the capture so keys stop being buffered.
            if let Some(capture) = capture {
                if !capture.skipped && !capture.draft.trim().is_empty() {
                    commands.entity(entity).insert(BufferedAgentPrompt {
                        text: capture.draft.clone(),
                        submit: true,
                    });
                }
                commands.entity(entity).remove::<PromptCapture>();
            }
            commands.entity(entity).remove::<AgentLoading>();
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                TERM_LOADING_EVENT,
                &crate::event::TermLoadingEvent {
                    loading: false,
                    label,
                    segment,
                },
            ));
        }
    }
}

fn reset_terminal_title_on_agent_removed(
    mut removed: RemovedComponents<vmux_core::agent::AgentSession>,
    mut q: Query<(&ProcessId, &mut PageMetadata), With<Terminal>>,
) {
    for entity in removed.read() {
        if let Ok((pid, mut meta)) = q.get_mut(entity) {
            let title = format!("Terminal ({})", &pid.to_string()[..8]);
            if meta.title != title {
                meta.title = title;
            }
        }
    }
}

/// Mark dirty when webview becomes ready so initial viewport is sent.
fn on_term_ready(
    trigger: On<Add, PageReady>,
    q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let Some(service) = service else { return };
    if let Ok(pid) = q.get(entity) {
        service
            .0
            .send(ClientMessage::RequestSnapshot { process_id: *pid });
    }
}

/// Handle resize event from webview (reports char cell dimensions).
fn on_term_resize(
    trigger: On<BinReceive<TermResizeEvent>>,
    webview_q: Query<&WebviewSize, With<Terminal>>,
    pid_q: Query<&ProcessId, With<Terminal>>,
    mut grid_q: Query<&mut TerminalGridSize, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;

    let Ok(webview_size) = webview_q.get(entity) else {
        return;
    };

    if event.char_width <= 0.0 || event.char_height <= 0.0 {
        return;
    }

    let vw = if event.viewport_width > 0.0 {
        event.viewport_width
    } else {
        webview_size.0.x
    };
    let vh = if event.viewport_height > 0.0 {
        event.viewport_height
    } else {
        webview_size.0.y
    };

    let cols = (vw / event.char_width).floor().max(1.0) as u16;
    let rows = (vh / event.char_height).floor().max(1.0) as u16;

    if let Ok(mut grid) = grid_q.get_mut(entity) {
        grid.cols = cols;
        grid.rows = rows;
    }

    let Some(service) = service else { return };
    let Ok(pid) = pid_q.get(entity) else {
        return;
    };

    service.0.send(ClientMessage::ResizeProcess {
        process_id: *pid,
        cols,
        rows,
    });
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalFontSizeCommand {
    Increase,
    Decrease,
    Reset,
}

pub fn handle_terminal_font_size(
    mut reader: MessageReader<TerminalFontSizeCommand>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    for cmd in reader.read() {
        let Some(terminal) = settings.terminal.as_ref() else {
            continue;
        };
        let name = terminal.default_theme.clone();
        let idx = match terminal.themes.iter().position(|t| t.name == name) {
            Some(idx) => idx,
            None => {
                let resolved = terminal.resolve_theme(&name);
                let terminal = settings.terminal.as_mut().unwrap();
                terminal.themes.push(resolved);
                terminal.themes.len() - 1
            }
        };
        let terminal = settings.terminal.as_mut().unwrap();
        let cur = terminal.themes[idx].font_size;
        let new = match cmd {
            TerminalFontSizeCommand::Increase => (cur + 1.0).min(40.0),
            TerminalFontSizeCommand::Decrease => (cur - 1.0).max(6.0),
            TerminalFontSizeCommand::Reset => 14.0,
        };
        if new == cur {
            continue;
        }
        terminal.themes[idx].font_size = new;
        saves.write(SettingsSaveRequest);
    }
}

fn theme_signature(
    theme: &vmux_setting::TerminalTheme,
    colors: &vmux_setting::themes::TerminalColorScheme,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    colors.foreground.hash(&mut hasher);
    colors.background.hash(&mut hasher);
    colors.cursor.hash(&mut hasher);
    colors.ansi.hash(&mut hasher);
    theme.font_size.to_bits().hash(&mut hasher);
    theme.line_height.to_bits().hash(&mut hasher);
    theme.padding.to_bits().hash(&mut hasher);
    theme.font_family.hash(&mut hasher);
    theme.cursor_style.hash(&mut hasher);
    theme.cursor_blink.hash(&mut hasher);
    hasher.finish()
}

/// Map a terminal color scheme across the light/dark boundary for the app
/// appearance. Only crosses the boundary: a chosen dark flavor (e.g. frappe,
/// macchiato) is preserved in dark mode, and any scheme without a known
/// counterpart is honored as-is.
fn scheme_for_appearance(name: &str, dark: bool) -> &str {
    match (name, dark) {
        ("catppuccin-mocha" | "catppuccin-frappe" | "catppuccin-macchiato", false) => {
            "catppuccin-latte"
        }
        ("catppuccin-latte", true) => "catppuccin-mocha",
        ("solarized-dark", false) => "solarized-light",
        ("solarized-light", true) => "solarized-dark",
        (other, _) => other,
    }
}

fn sync_terminal_theme(
    q: Query<Entity, With<Terminal>>,
    new_terminals: Query<Entity, Added<Terminal>>,
    newly_ready: Query<Entity, (With<Terminal>, Added<PageReady>)>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    scheme: Option<Res<vmux_setting::ResolvedColorScheme>>,
    mut commands: Commands,
    mut last_theme_hash: Local<u64>,
) {
    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let theme = terminal_settings.resolve_theme(&terminal_settings.default_theme);
    let dark = scheme
        .map(|s| matches!(s.0, vmux_setting::ResolvedScheme::Dark))
        .unwrap_or(true);
    let scheme_name = scheme_for_appearance(&theme.color_scheme, dark);
    let colors = vmux_setting::themes::resolve_theme(scheme_name, &terminal_settings.custom_themes);

    let hash = theme_signature(&theme, &colors);

    let theme_changed = hash != *last_theme_hash;
    if !theme_changed && new_terminals.is_empty() && newly_ready.is_empty() {
        return;
    }
    *last_theme_hash = hash;

    let base_event = crate::event::TermThemeEvent {
        foreground: colors.foreground,
        background: colors.background,
        cursor: colors.cursor,
        ansi: colors.ansi,
        font_family: theme.font_family.clone(),
        font_size: theme.font_size,
        line_height: theme.line_height,
        padding: theme.padding,
        cursor_style: theme.cursor_style.clone(),
        cursor_blink: theme.cursor_blink,
    };
    let targets: Vec<Entity> = if theme_changed {
        q.iter().collect()
    } else {
        new_terminals.iter().chain(newly_ready.iter()).collect()
    };

    for entity in targets {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                TERM_THEME_EVENT,
                &base_event,
            ));
        }
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
    commands.entity(entity).insert(AwaitingProcessCreated);
    if let Some(l) = launch.as_mut() {
        l.args = args;
    } else {
        meta.url = TERMINAL_PAGE_URL.to_string();
        meta.title = format!("Terminal ({})", &new_id.to_string()[..8]);
    }
}

/// Consume `AppCommand::Terminal::CopyMode` and ask the service to enter
/// visual/copy mode for the currently focused terminal process.
fn handle_terminal_copy_mode_command(
    mut er: MessageReader<AppCommand>,
    targeted_terminals: Query<
        (&ProcessId, &ChildOf),
        (
            With<Terminal>,
            With<CefKeyboardTarget>,
            Without<ProcessExited>,
        ),
    >,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mode: Res<vmux_layout::scene::InteractionMode>,
    service: Option<Res<ServiceClient>>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let Some(service) = service else {
        for _ in er.read() {}
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
        *mode,
    );
    let active_process_id = target_processes.first().copied();
    for cmd in er.read() {
        if matches!(
            cmd,
            AppCommand::Terminal(vmux_command::TerminalCommand::CopyMode)
        ) && let Some(process_id) = active_process_id
        {
            set_local_copy_mode(&mut local_copy_mode, process_id, true);
            service.0.send(ClientMessage::EnterCopyMode { process_id });
        }
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

fn set_local_copy_mode(
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    active: bool,
) {
    if active {
        local_copy_mode.active.insert(process_id);
    } else {
        local_copy_mode.active.remove(&process_id);
        local_copy_mode.input_states.remove(&process_id);
    }
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

pub fn apply_osc_title(
    mut reader: MessageReader<OscTitleChanged>,
    mut commands: Commands,
    terminals: Query<(Entity, &ProcessId, Option<&OscTitle>), With<Terminal>>,
) {
    for ev in reader.read() {
        let Some((entity, _, current)) =
            terminals.iter().find(|(_, pid, _)| **pid == ev.process_id)
        else {
            continue;
        };
        if ev.title.is_empty() {
            if current.is_some() {
                commands.entity(entity).remove::<OscTitle>();
            }
        } else if current.map(|o| o.0.as_str()) != Some(ev.title.as_str()) {
            commands.entity(entity).insert(OscTitle(ev.title.clone()));
        }
    }
}

pub fn clear_osc_title_on_exit(
    mut reader: MessageReader<ProcessExitedEvent>,
    mut commands: Commands,
    terminals: Query<(Entity, &ProcessId), (With<Terminal>, With<OscTitle>)>,
) {
    for ev in reader.read() {
        if let Some((entity, _)) = terminals.iter().find(|(_, pid)| **pid == ev.process_id) {
            commands.entity(entity).remove::<OscTitle>();
        }
    }
}

fn update_local_copy_mode_for_mouse_action(
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    action: &MouseTerminalAction,
) {
    match action {
        MouseTerminalAction::EnterCopyMode => {
            set_local_copy_mode(local_copy_mode, process_id, true)
        }
        MouseTerminalAction::ExitCopyMode => {
            set_local_copy_mode(local_copy_mode, process_id, false)
        }
        _ => {}
    }
}

pub fn handle_terminal_send_requests(
    mut reader: MessageReader<crate::TerminalSendRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let crate::TerminalSendRequest { text, terminal } = request.clone();

        let target = if let Some(s) = terminal.as_deref() {
            match crate::target::parse_terminal_target(s, &terminals) {
                Some(t) => Ok(Some(t)),
                None => Err(format!("terminal_send: invalid terminal id '{s}'")),
            }
        } else {
            Ok(crate::target::active_terminal_for_tab(
                focus.stack,
                &terminals,
            ))
        };

        match target {
            Err(_) => {}
            Ok(Some(terminal_entity)) => {
                commands
                    .entity(terminal_entity)
                    .insert(PendingTerminalInput {
                        data: text.as_bytes().to_vec(),
                    });
            }
            Ok(None) => {}
        }
    }
}

pub fn handle_run_shell_requests(
    mut reader: MessageReader<crate::RunShellRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<
        Entity,
        (
            With<vmux_layout::pane::Pane>,
            Without<vmux_layout::pane::PaneSplit>,
        ),
    >,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut commands: Commands,
    mut terminal_stack_spawns: Option<MessageWriter<TerminalStackSpawnRequest>>,
) {
    for request in reader.read() {
        let crate::RunShellRequest { command, cwd, mode } = request.clone();
        let input = crate::shell_input::shell_command_input(&command);
        if matches!(mode, crate::ShellMode::Active)
            && let Some(terminal) = crate::target::active_terminal_for_tab(focus.stack, &terminals)
        {
            commands
                .entity(terminal)
                .insert(PendingTerminalInput { data: input });
        } else if let Some(terminal_stack_spawns) = terminal_stack_spawns.as_mut()
            && let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane))
            && let Ok(cwd_path) = vmux_space::cwd::valid_cwd(&cwd)
        {
            terminal_stack_spawns.write(TerminalStackSpawnRequest {
                pane,
                cwd: cwd_path,
                shell: None,
                agent_run: false,
                pending_input: Some(input),
                process_id: None,
                activate: true,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::Schedules;
    use std::time::{Duration, Instant};
    use vmux_core::agent::{AgentKind, AgentSession};
    use vmux_core::page::PageReady;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};

    #[test]
    fn service_bridge_routes_acp_agent_info() {
        let source = include_str!("plugin.rs");
        let handler = source
            .split("fn poll_service_messages")
            .nth(1)
            .expect("service handler")
            .split("fn flush_pending_terminal_input")
            .next()
            .expect("service handler body");
        assert!(handler.contains("ServiceMessage::AcpAgentInfo"));
        assert!(handler.contains(".page_agent_info"));
    }

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
    fn terminal_reinput_appends_to_existing_pending_input() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<TerminalReinputRequest>()
            .add_systems(Update, handle_terminal_reinput_requests);
        let pid = process_id(7);
        let terminal = app
            .world_mut()
            .spawn((
                Terminal,
                pid,
                PendingTerminalInput {
                    data: b"initial\r".to_vec(),
                },
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<TerminalReinputRequest>>()
            .write(TerminalReinputRequest {
                process_id: pid,
                data: b"next\r".to_vec(),
            });
        app.update();

        assert_eq!(
            app.world()
                .get::<PendingTerminalInput>(terminal)
                .unwrap()
                .data,
            b"initial\rnext\r"
        );
    }

    #[test]
    fn terminal_reinput_preserves_multiple_messages_in_order() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<TerminalReinputRequest>()
            .add_systems(Update, handle_terminal_reinput_requests);
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
            app.world()
                .get::<PendingTerminalInput>(terminal)
                .unwrap()
                .data,
            b"one\rtwo\r"
        );
    }

    #[test]
    fn term_link_open_emits_browser_open_command() {
        #[derive(Resource, Default)]
        struct Captured(Vec<AppCommand>);
        fn capture(mut r: MessageReader<AppCommand>, mut c: ResMut<Captured>) {
            for m in r.read() {
                c.0.push(m.clone());
            }
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppCommand>()
            .add_message::<vmux_command::CommandIssued>()
            .init_resource::<Captured>()
            .add_observer(on_term_link_open)
            .add_systems(Update, capture);
        let webview = app.world_mut().spawn(vmux_core::team::User).id();

        app.world_mut().trigger(BinReceive::<TermLinkOpenRequest> {
            webview,
            payload: TermLinkOpenRequest {
                url: "https://vmux.ai".into(),
            },
        });
        app.update();

        let captured = app.world().resource::<Captured>();
        assert!(
            captured.0.iter().any(|c| matches!(
                c,
                AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                    url: Some(u),
                })) if u == "https://vmux.ai"
            )),
            "expected InNewStack open command, got {:?}",
            captured.0
        );
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
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

    #[test]
    fn terminal_send_resolves_target_by_process_id_uuid() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalSendRequest>()
            .insert_resource(vmux_layout::stack::FocusedStack::default())
            .add_systems(Update, handle_terminal_send_requests);

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

        let pending = app
            .world()
            .get::<PendingTerminalInput>(terminal)
            .expect("input routed to terminal by process id uuid");
        assert_eq!(pending.data, b"hi".to_vec());
    }

    #[test]
    fn terminal_stack_spawn_uses_requested_shell() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<TerminalStackSpawnRequest>()
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
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
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
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
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
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
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(vmux_space::spaces::ActiveSpace { record })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
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
        assert_eq!(launch.cwd, tab_dir.path().to_string_lossy());
    }

    #[test]
    fn missing_service_process_restarts_matching_terminal() {
        let missing = process_id(7);
        let target = Entity::from_bits(1);
        let plain_launch = || crate::launch::TerminalLaunch {
            command: default_shell(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
            kind: crate::launch::TerminalKind::Plain,
        };
        let restart = missing_terminal_restart(
            missing,
            [
                (Entity::from_bits(2), process_id(8), plain_launch(), None),
                (target, missing, plain_launch(), None),
            ],
        )
        .unwrap();

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
        .add_message::<LayoutSpawnRequest>();
        add_terminal_update_systems(&mut app);

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

        let targets = resolve_terminal_input_targets(
            [],
            false,
            Some(stack),
            [(stack, process_id)],
            vmux_layout::scene::InteractionMode::User,
        );

        assert_eq!(targets, vec![process_id]);
    }

    #[test]
    fn terminal_input_targets_do_not_steal_input_from_non_terminal_target() {
        let stack = Entity::from_bits(1);

        let targets = resolve_terminal_input_targets(
            [],
            true,
            Some(stack),
            [(stack, process_id(7))],
            vmux_layout::scene::InteractionMode::User,
        );

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
            vmux_layout::scene::InteractionMode::User,
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
            vmux_layout::scene::InteractionMode::User,
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
            vmux_layout::scene::InteractionMode::User,
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
        let event = TermKeyEvent {
            key: "a".to_string(),
            code: "KeyA".to_string(),
            modifiers: 0,
            text: Some("a".to_string()),
        };

        assert_eq!(term_key_event_to_bytes(&event), b"a".to_vec());
    }

    #[test]
    fn web_terminal_key_events_delegate_control_sequences() {
        let event = TermKeyEvent {
            key: "c".to_string(),
            code: "KeyC".to_string(),
            modifiers: MOD_CTRL,
            text: None,
        };

        assert_eq!(term_key_event_to_bytes(&event), vec![3]);
    }

    #[test]
    fn web_terminal_key_events_ignore_modifier_keys() {
        let event = TermKeyEvent {
            key: "Shift".to_string(),
            code: "ShiftLeft".to_string(),
            modifiers: MOD_SHIFT,
            text: None,
        };

        assert!(term_key_event_to_bytes(&event).is_empty());
    }

    #[test]
    fn web_terminal_shortcuts_emit_app_command_before_pty_input() {
        let event = TermKeyEvent {
            key: "l".to_string(),
            code: "KeyL".to_string(),
            modifiers: MOD_SUPER,
            text: Some("l".to_string()),
        };
        let mut state = TerminalWebShortcutState::default();

        assert_eq!(
            resolve_terminal_web_shortcut(&event, None, &mut state),
            TerminalWebShortcutAction::Command(AppCommand::Browser(
                vmux_command::BrowserCommand::Bar(
                    vmux_command::BrowserBarCommand::OpenPageInCommandBar
                )
            ))
        );
    }

    #[test]
    fn web_terminal_menu_accel_shortcuts_emit_app_command_before_pty_input() {
        let event = TermKeyEvent {
            key: "S".to_string(),
            code: "KeyS".to_string(),
            modifiers: MOD_SUPER | MOD_SHIFT,
            text: Some("S".to_string()),
        };
        let mut state = TerminalWebShortcutState::default();

        assert_eq!(
            resolve_terminal_web_shortcut(&event, None, &mut state),
            TerminalWebShortcutAction::Command(AppCommand::Layout(
                vmux_command::LayoutCommand::ToggleLayout(
                    vmux_command::ToggleLayoutCommand::Toggle
                )
            ))
        );
    }

    #[test]
    fn terminal_page_emits_key_events_from_native_webview() {
        let source = include_str!("page.rs");

        assert!(source.contains("emit_key("));
        assert!(source.contains("onkeydown"));
        assert!(source.contains("TermKeyEvent"));
    }

    #[test]
    fn terminal_page_focus_does_not_draw_browser_outline() {
        let source = include_str!("page.rs");

        assert!(source.contains("outline:none"));
    }

    #[test]
    fn agent_loading_uses_matrix_rain() {
        let page = include_str!("page.rs");
        assert!(page.contains("MatrixRain {"));
        assert!(page.contains("accent.rain_rgb"));

        let rain = include_str!("matrix_rain.rs");
        assert!(rain.contains("request_animation_frame"));
        assert!(rain.contains("use_drop"));
        assert!(rain.contains("prefers-reduced-motion"));
    }

    #[test]
    fn terminal_web_shortcut_wakes_next_command_frame() {
        let source = include_str!("plugin.rs");
        let on_term_key = source
            .split("fn on_term_key")
            .nth(1)
            .and_then(|tail| tail.split("fn on_term_ready").next())
            .unwrap_or_default();

        assert!(on_term_key.contains("EventLoopProxyWrapper"));
        assert!(on_term_key.contains("WinitUserEvent::WakeUp"));
    }

    fn mouse_event(button: u8, col: u16, row: u16, pressed: bool, moving: bool) -> TermMouseEvent {
        TermMouseEvent {
            button,
            col,
            row,
            modifiers: 0,
            pressed,
            moving,
        }
    }

    #[test]
    fn drag_enters_visual_mode_on_first_motion_and_exits_on_release() {
        let mut state = MouseSessionState::default();
        let now = std::time::Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            mouse_terminal_actions(&mut state, &down, false, now),
            vec![MouseTerminalAction::SetSelection(None)]
        );

        let drag = mouse_event(0, 5, 3, true, true);
        assert_eq!(
            mouse_terminal_actions(
                &mut state,
                &drag,
                false,
                now + std::time::Duration::from_millis(10),
            ),
            vec![
                MouseTerminalAction::EnterCopyMode,
                MouseTerminalAction::SetSelection(Some(TermSelectionRange {
                    start_col: 2,
                    start_row: 3,
                    end_col: 5,
                    end_row: 3,
                    is_block: false,
                })),
            ]
        );

        let release = mouse_event(0, 5, 3, false, false);
        assert_eq!(
            mouse_terminal_actions(
                &mut state,
                &release,
                false,
                now + std::time::Duration::from_millis(20),
            ),
            vec![MouseTerminalAction::ExitCopyMode]
        );
    }

    #[test]
    fn single_click_never_enters_visual_mode() {
        let mut state = MouseSessionState::default();
        let now = std::time::Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            mouse_terminal_actions(&mut state, &down, false, now),
            vec![MouseTerminalAction::SetSelection(None)]
        );

        let release = mouse_event(0, 2, 3, false, false);
        assert_eq!(
            mouse_terminal_actions(
                &mut state,
                &release,
                false,
                now + std::time::Duration::from_millis(20),
            ),
            Vec::<MouseTerminalAction>::new()
        );
    }

    #[test]
    fn captured_mouse_without_shift_still_forwards_drag_motion() {
        let mut state = MouseSessionState::default();
        let event = mouse_event(0, 4, 5, true, true);

        assert_eq!(
            mouse_terminal_actions(&mut state, &event, true, std::time::Instant::now()),
            vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
                32, 4, 5, 0, true,
            ))]
        );
    }

    #[test]
    fn hover_motion_without_app_capture_is_not_forwarded() {
        let mut state = MouseSessionState::default();
        let hover = mouse_event(3, 9, 4, true, true);

        assert_eq!(
            mouse_terminal_actions(&mut state, &hover, false, std::time::Instant::now()),
            Vec::<MouseTerminalAction>::new(),
            "bare hover with no app mouse capture must not be echoed into the PTY"
        );
    }

    #[test]
    fn hover_motion_with_app_capture_is_forwarded() {
        let mut state = MouseSessionState::default();
        let hover = mouse_event(3, 9, 4, true, true);

        assert_eq!(
            mouse_terminal_actions(&mut state, &hover, true, std::time::Instant::now()),
            vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
                35, 9, 4, 0, true,
            ))]
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

        set_local_copy_mode(&mut local_copy_mode, process_id, true);

        assert!(is_copy_mode_active(&mode_map, &local_copy_mode, process_id));
    }

    #[test]
    fn service_copy_mode_broadcast_reconciles_local_latch() {
        let process_id = ProcessId::new();
        let mut mode_map = TerminalModeMap::default();
        let mut local_copy_mode = LocalCopyModeState::default();

        set_local_copy_mode(&mut local_copy_mode, process_id, true);
        mode_map.modes.insert(
            process_id,
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
                alt_screen: false,
                focus_reporting: false,
            },
        );
        set_local_copy_mode(&mut local_copy_mode, process_id, false);

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
        set_local_copy_mode(&mut local_copy_mode, process_id, true);

        if copy_mode_key_exits(K::Exit) {
            set_local_copy_mode(&mut local_copy_mode, process_id, false);
        }

        assert!(!local_copy_mode.active.contains(&process_id));
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
                .query_filtered::<(bevy::prelude::Entity, &ProcessId), With<AwaitingProcessCreated>>(
                )
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
    fn agent_terminal_armed_loading_on_page_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, arm_agent_loading);
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                PageReady {},
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }

    #[test]
    fn agent_loading_armed_on_pty_restart() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, arm_agent_loading_on_restart);
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                ProcessId::new(),
            ))
            .id();

        // ProcessId added before the page is ready must not arm.
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());

        // Page becomes ready without a pid change: this system must not arm
        // (first launch is handled by arm_agent_loading).
        app.world_mut().entity_mut(e).insert(PageReady {});
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());

        // A restart mutates ProcessId while the page is ready: must arm.
        *app.world_mut().get_mut::<ProcessId>(e).unwrap() = ProcessId::new();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }

    #[test]
    fn agent_loading_cleared_when_alt_screen_active() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                pid,
                AgentLoading {
                    since: Instant::now(),
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<TerminalModeMap>()
            .modes
            .insert(
                pid,
                TerminalModeFlags {
                    mouse_capture: false,
                    copy_mode: false,
                    alt_screen: true,
                    focus_reporting: false,
                },
            );
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());
    }

    fn clear_with_capture(capture: PromptCapture) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Claude,
                },
                pid,
                AgentLoading {
                    since: Instant::now(),
                },
                capture,
            ))
            .id();
        app.world_mut()
            .resource_mut::<TerminalModeMap>()
            .modes
            .insert(
                pid,
                TerminalModeFlags {
                    mouse_capture: false,
                    copy_mode: false,
                    alt_screen: true,
                    focus_reporting: false,
                },
            );
        app.update();
        (app, e)
    }

    #[test]
    fn ready_flips_capture_into_buffered_prompt() {
        let (app, e) = clear_with_capture(PromptCapture {
            draft: "find me a hotel".to_string(),
            skipped: false,
        });
        assert!(app.world().get::<PromptCapture>(e).is_none());
        let buffered = app.world().get::<BufferedAgentPrompt>(e).unwrap();
        assert_eq!(buffered.text, "find me a hotel");
        assert!(buffered.submit);
    }

    #[test]
    fn ready_with_skipped_capture_delivers_nothing() {
        let (app, e) = clear_with_capture(PromptCapture {
            draft: "ignored".to_string(),
            skipped: true,
        });
        assert!(app.world().get::<PromptCapture>(e).is_none());
        assert!(app.world().get::<BufferedAgentPrompt>(e).is_none());
    }

    #[test]
    fn agent_loading_cleared_after_timeout() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                pid,
                AgentLoading {
                    since: Instant::now() - AGENT_LOADING_TIMEOUT - Duration::from_secs(1),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());
    }

    #[test]
    fn agent_loading_retained_while_starting() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                pid,
                AgentLoading {
                    since: Instant::now(),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }

    #[test]
    fn arm_loading_arms_plain_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, arm_agent_loading);
        let e = app.world_mut().spawn((Terminal, PageReady {})).id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }

    #[test]
    fn plain_terminal_loading_retained_before_min_display() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                ProcessId::new(),
                AgentLoading {
                    since: Instant::now(),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }

    #[test]
    fn plain_terminal_loading_cleared_after_min_display() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                ProcessId::new(),
                AgentLoading {
                    since: Instant::now() - TERMINAL_LOADING_MIN_DISPLAY - Duration::from_millis(1),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());
    }

    #[test]
    fn terminal_title_resets_to_plain_when_agent_session_removed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reset_terminal_title_on_agent_removed);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                pid,
                PageMetadata {
                    title: "Vibe (abc12345)".to_string(),
                    url: "vmux://agent/vibe/abc12345".to_string(),
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
                AgentSession {
                    kind: AgentKind::Vibe,
                },
            ))
            .id();
        app.update();
        app.world_mut().entity_mut(e).remove::<AgentSession>();
        app.update();
        let expected = format!("Terminal ({})", &pid.to_string()[..8]);
        let title = app.world().get::<PageMetadata>(e).unwrap().title.clone();
        assert_eq!(title, expected);
    }

    #[test]
    fn apply_osc_title_sets_and_clears() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
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
                .get::<vmux_core::OscTitle>(e)
                .map(|o| o.0.clone()),
            Some("claude — repo".to_string())
        );

        app.world_mut()
            .resource_mut::<Messages<OscTitleChanged>>()
            .write(OscTitleChanged {
                process_id: pid,
                title: String::new(),
            });
        app.update();
        assert!(app.world().get::<vmux_core::OscTitle>(e).is_none());
    }

    #[test]
    fn clear_osc_title_on_exit_removes_override() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ProcessExitedEvent>()
            .add_systems(Update, clear_osc_title_on_exit);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((Terminal, pid, vmux_core::OscTitle("working".to_string())))
            .id();

        app.world_mut()
            .resource_mut::<Messages<ProcessExitedEvent>>()
            .write(ProcessExitedEvent { process_id: pid });
        app.update();
        assert!(app.world().get::<vmux_core::OscTitle>(e).is_none());
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

    fn term_theme(font_size: f32) -> vmux_setting::TerminalTheme {
        vmux_setting::TerminalTheme {
            name: "default".to_string(),
            color_scheme: "catppuccin-mocha".to_string(),
            font_family: "JetBrainsMono Nerd Font".to_string(),
            font_size,
            line_height: 1.2,
            padding: 4.0,
            cursor_style: "block".to_string(),
            cursor_blink: true,
            shell: "/bin/sh".to_string(),
        }
    }

    fn settings_with_font(font_size: f32) -> AppSettings {
        let mut s = test_settings();
        s.terminal = Some(vmux_setting::TerminalSettings {
            default_theme: "default".to_string(),
            themes: vec![term_theme(font_size)],
            ..Default::default()
        });
        s
    }

    fn run_font_size_command(start: f32, cmd: TerminalFontSizeCommand) -> (f32, usize) {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings_with_font(start))
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<SettingsSaveRequest>()
            .add_systems(Update, handle_terminal_font_size);
        app.world_mut()
            .resource_mut::<Messages<TerminalFontSizeCommand>>()
            .write(cmd);
        app.update();
        let size = app
            .world()
            .resource::<AppSettings>()
            .terminal
            .as_ref()
            .unwrap()
            .themes[0]
            .font_size;
        let saves = app
            .world_mut()
            .resource_mut::<Messages<SettingsSaveRequest>>()
            .drain()
            .count();
        (size, saves)
    }

    #[test]
    fn font_size_materializes_missing_default_theme() {
        use bevy::ecs::message::Messages;
        let mut settings = test_settings();
        settings.terminal = Some(vmux_setting::TerminalSettings {
            default_theme: "default".to_string(),
            themes: Vec::new(),
            ..Default::default()
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<SettingsSaveRequest>()
            .add_systems(Update, handle_terminal_font_size);
        app.world_mut()
            .resource_mut::<Messages<TerminalFontSizeCommand>>()
            .write(TerminalFontSizeCommand::Increase);
        app.update();

        let terminal = app
            .world()
            .resource::<AppSettings>()
            .terminal
            .clone()
            .unwrap();
        let theme = terminal
            .themes
            .iter()
            .find(|t| t.name == "default")
            .expect("missing default theme must be materialized so zoom persists");
        assert_eq!(theme.font_size, 15.0);
        let saves = app
            .world_mut()
            .resource_mut::<Messages<SettingsSaveRequest>>()
            .drain()
            .count();
        assert_eq!(saves, 1);
    }

    #[test]
    fn font_size_increase_steps_up_and_persists() {
        let (size, writes) = run_font_size_command(14.0, TerminalFontSizeCommand::Increase);
        assert_eq!(size, 15.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn font_size_decrease_steps_down_and_persists() {
        let (size, writes) = run_font_size_command(14.0, TerminalFontSizeCommand::Decrease);
        assert_eq!(size, 13.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn font_size_increase_clamps_at_40() {
        let (size, _) = run_font_size_command(40.0, TerminalFontSizeCommand::Increase);
        assert_eq!(size, 40.0);
    }

    #[test]
    fn font_size_decrease_clamps_at_6() {
        let (size, _) = run_font_size_command(6.0, TerminalFontSizeCommand::Decrease);
        assert_eq!(size, 6.0);
    }

    #[test]
    fn font_size_reset_returns_to_14() {
        let (size, writes) = run_font_size_command(20.0, TerminalFontSizeCommand::Reset);
        assert_eq!(size, 14.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn theme_signature_changes_with_font_size() {
        let colors = vmux_setting::themes::resolve_theme("catppuccin-mocha", &[]);
        let small = term_theme(14.0);
        let large = term_theme(15.0);
        assert_ne!(
            theme_signature(&small, &colors),
            theme_signature(&large, &colors)
        );
    }
}
