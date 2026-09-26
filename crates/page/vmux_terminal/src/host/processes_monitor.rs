use std::collections::HashMap;

use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_command::{CommandDefinition, CommandInvocation, CommandRequest, CommandTypePlugin};
use vmux_core::host::{UiState, UiStatePlugin, UiStateWrite};
use vmux_core::page::PageReady;
use vmux_history::LastActivatedAt;
use vmux_service::client::ServiceRequest;
use vmux_service::event::*;
use vmux_service::plugin::ServiceConnected;
use vmux_service::protocol::{ClientMessage, ProcessId};

use crate::Terminal;
use crate::plugin::reattach_terminal_bundle;
use crate::process_index::TerminalProcessIndex;
use vmux_core::{KeyboardOwner, Order};
use vmux_layout::{
    native_open::{HostedPage, HostedPagePlugin},
    pane::{Pane, PaneSplit},
    stack::{ActiveTabParam, OpenRequest, Stack, focused_stack, stack_bundle},
};

pub struct ProcessesMonitorPlugin;

impl Plugin for ProcessesMonitorPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceProcessSnapshot>()
            .add_systems(Startup, spawn_process_monitor)
            .add_plugins(UiEventPlugin::<(
                ProcessNavigateEvent,
                ProcessKillEvent,
                ProcessKillAllEvent,
            )>::default())
            .add_plugins((
                UiStatePlugin::<ProcessesUiState>::default(),
                CommandTypePlugin::<OpenServicesRequest>::default(),
            ))
            .add_systems(
                Update,
                (
                    reconcile_service_processes,
                    request_process_list,
                    sample_process_usage,
                    broadcast_to_monitors,
                )
                    .chain()
                    .after(crate::plugin::ServiceMessageSet),
            )
            .add_systems(
                Update,
                open_services.before(vmux_core::workspace::StackCommandSet),
            )
            .add_observer(on_process_navigate)
            .add_observer(on_process_kill)
            .add_observer(on_process_kill_all)
            .add_plugins(HostedPagePlugin::<ProcessesMonitor>::default());
    }
}

#[derive(Component, Default)]
#[require(UiState<ProcessesUiState>)]
pub struct ProcessesMonitor;

impl ProcessesMonitor {}

impl HostedPage for ProcessesMonitor {
    const HOST: &'static str = "services";
    const URL: &'static str = vmux_service::PAGE_URL;
    const TITLE: &'static str = "Background Services";
}

#[derive(Message)]
struct OpenServicesRequest;

impl CommandRequest for OpenServicesRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("service_open", "Open Service Monitor", "Service")
                .expose_to_mcp(),
        ]
    }
}

impl TryFrom<&CommandInvocation> for OpenServicesRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "service_open" => Ok(Self),
            _ => Err(()),
        }
    }
}

fn open_services(
    mut requests: MessageReader<OpenServicesRequest>,
    mut stack_requests: MessageWriter<OpenRequest>,
) {
    for _ in requests.read() {
        stack_requests.write(OpenRequest {
            url: Some(vmux_service::PAGE_URL.to_string()),
        });
    }
}

#[derive(Message)]
pub(crate) struct ServiceProcessSnapshot(pub(crate) Vec<vmux_service::protocol::ProcessInfo>);

#[derive(Component)]
struct ProcessMonitor {
    process_poll: Timer,
    sysinfo_poll: Timer,
    system: sysinfo::System,
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self {
            process_poll: Timer::from_seconds(1.0, TimerMode::Repeating),
            sysinfo_poll: Timer::from_seconds(1.0, TimerMode::Repeating),
            system: sysinfo::System::new(),
        }
    }
}

#[derive(Component)]
struct ProcessMonitorDirty;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ServiceProcessId(ProcessId);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ProcessPid(u32);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct ServiceProcess {
    shell: String,
    cwd: String,
    cols: u16,
    rows: u16,
    uptime_secs: u64,
}

impl From<&vmux_service::protocol::ProcessInfo> for ServiceProcess {
    fn from(process: &vmux_service::protocol::ProcessInfo) -> Self {
        Self {
            shell: process.shell.clone(),
            cwd: process.cwd.clone(),
            cols: process.cols,
            rows: process.rows,
            uptime_secs: process.created_at_secs,
        }
    }
}

impl ServiceProcess {
    fn entry(
        &self,
        id: ServiceProcessId,
        pid: ProcessPid,
        usage: Usage,
        attached: bool,
    ) -> ProcessEntry {
        ProcessEntry {
            id: id.0.to_string(),
            managed: true,
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            cols: self.cols,
            rows: self.rows,
            pid: pid.0,
            uptime_secs: self.uptime_secs,
            cpu_percent: usage.cpu_percent,
            mem_bytes: usage.mem_bytes,
            attached,
            preview_lines: Vec::new(),
        }
    }
}

#[derive(Component, Clone, Copy, Default, Debug, PartialEq)]
pub struct Usage {
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

impl Usage {
    fn subtree(root: u32, processes: &HashMap<u32, ProcSample>) -> Self {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&pid, sample) in processes {
            if let Some(parent) = sample.parent {
                children.entry(parent).or_default().push(pid);
            }
        }
        let mut total = Self::default();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![root];
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(sample) = processes.get(&pid) {
                total.cpu_percent += sample.cpu;
                total.mem_bytes += sample.mem;
                if let Some(children) = children.get(&pid) {
                    stack.extend(children.iter().copied());
                }
            }
        }
        total
    }
}

#[derive(Component, Clone, Debug, PartialEq)]
struct LocalVmuxProcess {
    shell: String,
    cwd: String,
    uptime_secs: u64,
}

impl LocalVmuxProcess {
    fn matches(name: &str, executable: &str) -> bool {
        if name.to_ascii_lowercase().contains("vmux") {
            return true;
        }
        let executable_name = executable.rsplit('/').next().unwrap_or(executable);
        executable_name.to_ascii_lowercase().contains("vmux")
    }

    fn entry(&self, pid: ProcessPid, usage: Usage) -> ProcessEntry {
        ProcessEntry {
            id: format!("system:{}", pid.0),
            managed: false,
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            cols: 0,
            rows: 0,
            pid: pid.0,
            uptime_secs: self.uptime_secs,
            cpu_percent: usage.cpu_percent,
            mem_bytes: usage.mem_bytes,
            attached: false,
            preview_lines: Vec::new(),
        }
    }
}

struct ProcSample {
    parent: Option<u32>,
    cpu: f32,
    mem: u64,
}

fn spawn_process_monitor(mut commands: Commands) {
    commands.spawn((Name::new("Process monitor"), ProcessMonitor::default()));
}

fn reconcile_service_processes(
    mut snapshots: MessageReader<ServiceProcessSnapshot>,
    existing: Query<(Entity, &ServiceProcessId), With<ServiceProcess>>,
    runtime: Query<Entity, With<ProcessMonitor>>,
    mut commands: Commands,
) {
    let Some(snapshot) = snapshots.read().last() else {
        return;
    };
    let mut by_id = HashMap::new();
    for (entity, id) in &existing {
        by_id.insert(id.0, entity);
    }
    for (index, process) in snapshot.0.iter().enumerate() {
        let components = (
            Name::new(format!("Service process {}", process.id)),
            ServiceProcessId(process.id),
            ProcessPid(process.pid),
            Order(index as u32),
            ServiceProcess::from(process),
        );
        if let Some(entity) = by_id.remove(&process.id) {
            commands.entity(entity).insert(components);
        } else {
            commands.spawn((components, Usage::default()));
        }
    }
    for entity in by_id.into_values() {
        commands.entity(entity).despawn();
    }
    if let Ok(entity) = runtime.single() {
        commands.entity(entity).insert(ProcessMonitorDirty);
    }
}

fn request_process_list(
    time: Res<Time>,
    mut runtime: Query<&mut ProcessMonitor>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    monitors: Query<(), With<ProcessesMonitor>>,
    claimed: Query<(), (With<ProcessesMonitor>, Added<KeyboardOwner>)>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    if monitors.is_empty() {
        return;
    }
    let Ok(mut runtime) = runtime.single_mut() else {
        return;
    };
    runtime.process_poll.tick(time.delta());
    if (!claimed.is_empty() || runtime.process_poll.just_finished()) && connected.is_some() {
        service_requests.write(ServiceRequest(ClientMessage::ListProcesses));
    }
}

fn sample_process_usage(
    time: Res<Time>,
    mut runtime: Query<(Entity, &mut ProcessMonitor)>,
    monitors: Query<(), With<ProcessesMonitor>>,
    claimed: Query<(), (With<ProcessesMonitor>, Added<KeyboardOwner>)>,
    mut service_processes: Query<(&ProcessPid, &mut Usage), With<ServiceProcess>>,
    local_processes: Query<(Entity, &ProcessPid), With<LocalVmuxProcess>>,
    mut commands: Commands,
) {
    if monitors.is_empty() {
        return;
    }
    let Ok((runtime_entity, mut runtime)) = runtime.single_mut() else {
        return;
    };
    runtime.sysinfo_poll.tick(time.delta());
    if claimed.is_empty() && !runtime.sysinfo_poll.just_finished() {
        return;
    }

    runtime.system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cwd(sysinfo::UpdateKind::OnlyIfNotSet),
    );

    let mut samples = HashMap::new();
    for (pid, process) in runtime.system.processes() {
        samples.insert(
            pid.as_u32(),
            ProcSample {
                parent: process.parent().map(|parent| parent.as_u32()),
                cpu: process.cpu_usage(),
                mem: process.memory(),
            },
        );
    }

    for (pid, mut usage) in &mut service_processes {
        *usage = Usage::subtree(pid.0, &samples);
    }

    let mut existing = HashMap::new();
    for (entity, pid) in &local_processes {
        existing.insert(pid.0, entity);
    }
    for (pid, process) in runtime.system.processes() {
        let name = process.name().to_string_lossy().into_owned();
        let executable = process
            .exe()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !LocalVmuxProcess::matches(&name, &executable) {
            continue;
        }
        let pid = ProcessPid(pid.as_u32());
        let details = LocalVmuxProcess {
            shell: if executable.is_empty() {
                name
            } else {
                executable
            },
            cwd: process
                .cwd()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            uptime_secs: process.run_time(),
        };
        let usage = Usage {
            cpu_percent: process.cpu_usage(),
            mem_bytes: process.memory(),
        };
        if let Some(entity) = existing.remove(&pid.0) {
            commands.entity(entity).insert((details, usage));
        } else {
            commands.spawn((
                Name::new(format!("Vmux process {}", pid.0)),
                pid,
                details,
                usage,
            ));
        }
    }
    for entity in existing.into_values() {
        commands.entity(entity).despawn();
    }
    commands.entity(runtime_entity).insert(ProcessMonitorDirty);
}

fn broadcast_to_monitors(
    runtime: Query<(Entity, Has<ProcessMonitorDirty>), With<ProcessMonitor>>,
    service_processes: Query<(
        &ServiceProcessId,
        &ProcessPid,
        &ServiceProcess,
        &Usage,
        &Order,
    )>,
    local_processes: Query<(&ProcessPid, &LocalVmuxProcess, &Usage)>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    monitors: Query<Entity, (With<ProcessesMonitor>, With<PageReady>)>,
    claimed: Query<(), (With<ProcessesMonitor>, Added<KeyboardOwner>)>,
    terminal_pids: Query<&ProcessId, With<Terminal>>,
    mut commands: Commands,
) {
    if monitors.is_empty() {
        return;
    }
    let Ok((runtime_entity, dirty)) = runtime.single() else {
        return;
    };
    if !dirty && claimed.is_empty() {
        return;
    }

    let connected = connected.is_some();
    let attached_ids: std::collections::HashSet<ProcessId> =
        terminal_pids.iter().copied().collect();
    let mut managed_pids = std::collections::HashSet::new();
    let mut ordered = Vec::new();
    for (id, pid, process, usage, order) in &service_processes {
        managed_pids.insert(pid.0);
        ordered.push((
            order.0,
            process.entry(*id, *pid, *usage, attached_ids.contains(&id.0)),
        ));
    }
    ordered.sort_by_key(|(order, _)| *order);
    let mut processes =
        Vec::with_capacity(service_processes.iter().len() + local_processes.iter().len());
    for (_, process) in ordered {
        processes.push(process);
    }
    let mut local = Vec::new();
    for (pid, process, usage) in &local_processes {
        if !managed_pids.contains(&pid.0) {
            local.push((pid.0, process.entry(*pid, *usage)));
        }
    }
    local.sort_by_key(|(pid, _)| *pid);
    for (_, process) in local {
        processes.push(process);
    }

    let state = ProcessesUiState {
        connected,
        processes,
    };

    for entity in &monitors {
        commands.trigger(UiStateWrite::<ProcessesUiState>::from_event(entity, &state));
    }
    commands
        .entity(runtime_entity)
        .remove::<ProcessMonitorDirty>();
}

fn on_process_navigate(
    trigger: On<UiInput<ProcessNavigateEvent>>,
    process_index: Res<TerminalProcessIndex>,
    terminals: Query<&ChildOf, With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Stack>>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    let pid = &trigger.event().payload.process_id;
    let Ok(process_id) = pid.parse::<ProcessId>() else {
        warn!("Invalid process ID from navigate event: {pid}");
        return;
    };
    if let Some(entity) = process_index.get(&process_id)
        && let Ok(content_child_of) = terminals.get(entity)
    {
        let tab = content_child_of.get();
        commands.entity(tab).insert(LastActivatedAt::now());
        if let Ok(tab_child_of) = tab_parent.get(tab) {
            commands
                .entity(tab_child_of.get())
                .insert(LastActivatedAt::now());
        }
        return;
    }
    let (_, active_pane, _) = focused_stack(
        active_tab_param.get(),
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &stack_ts,
    );
    let Some(pane) = active_pane else { return };

    let tab = commands
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    commands.spawn((reattach_terminal_bundle(process_id), ChildOf(tab)));
}

fn on_process_kill(
    trigger: On<UiInput<ProcessKillEvent>>,
    service_processes: Query<(Entity, &ServiceProcessId), With<ServiceProcess>>,
    runtime: Query<Entity, With<ProcessMonitor>>,
    process_index: Res<TerminalProcessIndex>,
    terminals: Query<&ChildOf, With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Stack>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let pid = &trigger.event().payload.process_id;

    if let Ok(process_id) = pid.parse::<ProcessId>() {
        service_requests.write(ServiceRequest(ClientMessage::KillProcess { process_id }));
        for (entity, id) in &service_processes {
            if id.0 == process_id {
                commands.entity(entity).despawn();
            }
        }
        if let Ok(entity) = runtime.single() {
            commands.entity(entity).insert(ProcessMonitorDirty);
        }
        service_requests.write(ServiceRequest(ClientMessage::ListProcesses));

        if let Some(entity) = process_index.get(&process_id)
            && let Ok(content_child_of) = terminals.get(entity)
        {
            let tab = content_child_of.get();
            if tab_parent.get(tab).is_ok() || commands.get_entity(tab).is_ok() {
                commands.entity(tab).despawn();
            }
        }
    }
}

fn on_process_kill_all(
    _trigger: On<UiInput<ProcessKillAllEvent>>,
    service_processes: Query<(Entity, &ServiceProcessId), With<ServiceProcess>>,
    runtime: Query<Entity, With<ProcessMonitor>>,
    process_index: Res<TerminalProcessIndex>,
    terminals: Query<&ChildOf, With<Terminal>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let process_ids: Vec<(Entity, ProcessId)> = service_processes
        .iter()
        .map(|(entity, id)| (entity, id.0))
        .collect();

    for (process_entity, process_id) in &process_ids {
        service_requests.write(ServiceRequest(ClientMessage::KillProcess {
            process_id: *process_id,
        }));
        commands.entity(*process_entity).despawn();

        if let Some(entity) = process_index.get(process_id)
            && let Ok(content_child_of) = terminals.get(entity)
        {
            commands.entity(content_child_of.get()).despawn();
        }
    }
    if !process_ids.is_empty() {
        if let Ok(entity) = runtime.single() {
            commands.entity(entity).insert(ProcessMonitorDirty);
        }
        service_requests.write(ServiceRequest(ClientMessage::ListProcesses));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    fn process_info(id: ProcessId) -> vmux_service::protocol::ProcessInfo {
        vmux_service::protocol::ProcessInfo {
            id,
            shell: "/bin/sh".to_string(),
            cwd: String::new(),
            cols: 80,
            rows: 24,
            pid: 42,
            created_at_secs: 0,
        }
    }

    #[test]
    fn service_process_snapshots_reconcile_entities() {
        let keep = process_id(1);
        let remove = process_id(2);
        let mut app = App::new();
        app.add_message::<ServiceProcessSnapshot>()
            .add_systems(Startup, spawn_process_monitor)
            .add_systems(Update, reconcile_service_processes);
        app.world_mut().write_message(ServiceProcessSnapshot(vec![
            process_info(keep),
            process_info(remove),
        ]));
        app.update();

        let mut ids = app
            .world_mut()
            .query_filtered::<&ServiceProcessId, With<ServiceProcess>>()
            .iter(app.world())
            .map(|id| id.0)
            .collect::<Vec<_>>();
        ids.sort_by_key(|id| id.0);
        assert_eq!(ids, vec![keep, remove]);

        app.world_mut()
            .write_message(ServiceProcessSnapshot(vec![process_info(keep)]));
        app.update();

        let ids = app
            .world_mut()
            .query_filtered::<&ServiceProcessId, With<ServiceProcess>>()
            .iter(app.world())
            .map(|id| id.0)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![keep]);
    }

    #[test]
    fn subtree_usage_sums_whole_tree() {
        let mut procs = HashMap::new();
        procs.insert(
            1,
            ProcSample {
                parent: None,
                cpu: 5.0,
                mem: 100,
            },
        );
        procs.insert(
            2,
            ProcSample {
                parent: Some(1),
                cpu: 10.0,
                mem: 200,
            },
        );
        procs.insert(
            3,
            ProcSample {
                parent: Some(2),
                cpu: 1.0,
                mem: 50,
            },
        );
        procs.insert(
            99,
            ProcSample {
                parent: None,
                cpu: 7.0,
                mem: 999,
            },
        );
        let u = Usage::subtree(1, &procs);
        assert_eq!(u.cpu_percent, 16.0);
        assert_eq!(u.mem_bytes, 350);
    }

    #[test]
    fn subtree_usage_missing_root_is_zero() {
        let procs = HashMap::new();
        assert_eq!(Usage::subtree(5, &procs), Usage::default());
    }

    #[test]
    fn service_process_entry_attaches_usage() {
        let id = process_id(1);
        let entry = ServiceProcess::from(&process_info(id)).entry(
            ServiceProcessId(id),
            ProcessPid(42),
            Usage {
                cpu_percent: 12.5,
                mem_bytes: 332 * 1024 * 1024,
            },
            false,
        );
        assert_eq!(entry.pid, 42);
        assert_eq!(entry.cpu_percent, 12.5);
        assert_eq!(entry.mem_bytes, 332 * 1024 * 1024);
        assert!(!entry.attached);
    }

    #[test]
    fn service_process_entry_defaults_usage() {
        let id = process_id(1);
        let entry = ServiceProcess::from(&process_info(id)).entry(
            ServiceProcessId(id),
            ProcessPid(42),
            Usage::default(),
            false,
        );
        assert_eq!(entry.cpu_percent, 0.0);
        assert_eq!(entry.mem_bytes, 0);
    }

    #[test]
    fn local_vmux_process_entry_is_unmanaged() {
        let process = LocalVmuxProcess {
            shell: "/Applications/Vmux.app/Contents/MacOS/vmux_desktop".to_string(),
            cwd: "/tmp".to_string(),
            uptime_secs: 10,
        };
        let entry = process.entry(
            ProcessPid(42),
            Usage {
                cpu_percent: 3.0,
                mem_bytes: 1024,
            },
        );
        assert!(!entry.managed);
        assert_eq!(entry.pid, 42);
        assert_eq!(entry.cpu_percent, 3.0);
        assert_eq!(entry.mem_bytes, 1024);
    }

    #[test]
    fn vmux_process_match_uses_process_or_executable_name() {
        assert!(LocalVmuxProcess::matches("vmux_desktop Helper", ""));
        assert!(LocalVmuxProcess::matches(
            "helper",
            "/Applications/Vmux.app/Contents/MacOS/vmux_service"
        ));
        assert!(!LocalVmuxProcess::matches(
            "codex",
            "/Users/test/.vmux/projects/repo/codex"
        ));
    }
}
