use std::collections::{HashMap, VecDeque};

use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_api::service::*;
use vmux_command::{CommandInvocation, ResolvedLocale};
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::page::{Activity, PageReady};
use vmux_ecs::service::ServiceConnected;
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};
use vmux_history::LastActivatedAt;
use vmux_ui::i18n::{Locale, TranslationValue};

use super::input_queue::TerminalProcessIndex;
use super::process_sampler::{LocalProcess, ProcSample, ProcessSampler};
use crate::Terminal;
use crate::plugin::ReattachedTerminalBundle;
use vmux_ecs::{KeyboardOwner, Order};
use vmux_layout::{
    hosted_page::HostedUiPlugin,
    stack::{ActiveTabParam, LayoutFocus, OpenRequest, Stack},
};

#[vmux_page::page(page = "process_monitor")]
struct ProcessMonitorPageManifest;

pub struct ProcessMonitorPlugin;

impl Plugin for ProcessMonitorPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::monitor::ProcessMonitorPage::plugin());
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.add_message::<ServiceProcessSnapshot>()
            .add_message::<OpenServicesRequest>()
            .add_systems(Startup, spawn)
            .add_plugins(UiEventPlugin::<(
                ProcessNavigateEvent,
                ProcessKillEvent,
                ProcessKillAllEvent,
                ProcessSearchRequest,
            )>::default())
            .add_plugins(UiStatePlugin::<ProcessesUiState>::default())
            .add_systems(
                Update,
                (
                    reconcile_service_processes,
                    (
                        request_process_list,
                        sample_process_usage,
                        broadcast_to_monitors,
                    )
                        .chain()
                        .run_if(Activity::<ProcessMonitorView>::active),
                )
                    .chain()
                    .after(vmux_ecs::service::ServiceMessageSet),
            )
            .add_systems(
                Update,
                open_services.before(vmux_ecs::workspace::StackCommandSet),
            )
            .add_observer(process_navigate)
            .add_observer(process_kill)
            .add_observer(process_kill_all)
            .add_observer(search)
            .add_plugins(HostedUiPlugin::<ProcessMonitorView>::new(
                ProcessMonitorPageManifest::MANIFEST,
            ));
    }
}

const PROCESS_HISTORY_LIMIT: usize = 72;

#[derive(Clone, Copy)]
struct ProcessMemory(u64);

impl ProcessMemory {
    fn label(self) -> String {
        const MB: f64 = 1024.0 * 1024.0;
        const GB: f64 = MB * 1024.0;
        let bytes = self.0 as f64;
        if self.0 == 0 {
            "—".to_string()
        } else if bytes < MB {
            "<1 MB".to_string()
        } else if bytes < GB {
            format!("{:.0} MB", bytes / MB)
        } else {
            format!("{:.1} GB", bytes / GB)
        }
    }
}

#[derive(Clone, Copy)]
struct ProcessUptime(u64);

impl ProcessUptime {
    fn label(self, locale: &Locale) -> String {
        let seconds = self.0;
        if seconds < 60 {
            locale.translate_with(
                "services-uptime-seconds",
                &[("seconds", TranslationValue::Number(seconds as i64))],
            )
        } else if seconds < 3600 {
            locale.translate_with(
                "services-uptime-minutes",
                &[
                    ("minutes", TranslationValue::Number((seconds / 60) as i64)),
                    ("seconds", TranslationValue::Number((seconds % 60) as i64)),
                ],
            )
        } else if seconds < 86400 {
            locale.translate_with(
                "services-uptime-hours",
                &[
                    ("hours", TranslationValue::Number((seconds / 3600) as i64)),
                    (
                        "minutes",
                        TranslationValue::Number(((seconds % 3600) / 60) as i64),
                    ),
                ],
            )
        } else {
            locale.translate_with(
                "services-uptime-days",
                &[
                    ("days", TranslationValue::Number((seconds / 86400) as i64)),
                    (
                        "hours",
                        TranslationValue::Number(((seconds % 86400) / 3600) as i64),
                    ),
                ],
            )
        }
    }
}

struct Sparkline {
    line: String,
    area: String,
}

impl Sparkline {
    fn plot(samples: &[f32], minimum_span: f32) -> Self {
        let samples = if samples.is_empty() {
            vec![0.0, 0.0]
        } else if samples.len() == 1 {
            vec![samples[0], samples[0]]
        } else {
            samples.to_vec()
        };
        let minimum = samples.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let sample_span = maximum - minimum;
        let span = (sample_span * 1.1).max(minimum_span).max(1.0);
        let center = (minimum + maximum) / 2.0;
        let mut lower = center - span / 2.0;
        let mut upper = center + span / 2.0;
        if lower < 0.0 {
            upper -= lower;
            lower = 0.0;
        }
        let last = (samples.len() - 1) as f32;
        let mut points = Vec::with_capacity(samples.len());
        for (index, sample) in samples.iter().enumerate() {
            let x = index as f32 / last * 100.0;
            let y = 39.0 - ((sample - lower) / (upper - lower)).clamp(0.0, 1.0) * 37.0;
            points.push((x, y));
        }
        let segments = Self::curve_segments(&points);
        let line = format!("M {:.2} {:.2}{segments}", points[0].0, points[0].1);
        let area = format!(
            "M 0 40 L {:.2} {:.2}{segments} L 100 40 Z",
            points[0].0, points[0].1
        );
        Self { line, area }
    }

    fn curve_segments(points: &[(f32, f32)]) -> String {
        let mut path = String::new();
        for index in 0..points.len() - 1 {
            let previous = points[index.saturating_sub(1)];
            let current = points[index];
            let next = points[index + 1];
            let following = points[(index + 2).min(points.len() - 1)];
            let control_a = (
                current.0 + (next.0 - previous.0) / 6.0,
                current.1 + (next.1 - previous.1) / 6.0,
            );
            let control_b = (
                next.0 - (following.0 - current.0) / 6.0,
                next.1 - (following.1 - current.1) / 6.0,
            );
            path.push_str(&format!(
                " C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
                control_a.0, control_a.1, control_b.0, control_b.1, next.0, next.1
            ));
        }
        path
    }
}

#[derive(Component, Default)]
#[require(UiState<ProcessesUiState>)]
pub struct ProcessMonitorView {
    query: String,
    cpu_history: VecDeque<f32>,
    memory_history_mb: VecDeque<f32>,
}

#[derive(Component)]
struct ProcessMonitorViewDirty;

#[vmux_command::command(message)]
#[derive(Message)]
struct OpenServicesRequest;

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
            url: Some(ProcessMonitorPageManifest::URL.to_string()),
        });
    }
}

#[derive(Message)]
pub(crate) struct ServiceProcessSnapshot(pub(crate) Vec<vmux_api::protocol::ProcessInfo>);

#[derive(Component)]
struct ProcessMonitor {
    process_poll: Timer,
    sampler: ProcessSampler,
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self {
            process_poll: Timer::from_seconds(5.0, TimerMode::Repeating),
            sampler: ProcessSampler::default(),
        }
    }
}

#[derive(Component)]
struct ProcessMonitorDirty;

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

impl From<&vmux_api::protocol::ProcessInfo> for ServiceProcess {
    fn from(process: &vmux_api::protocol::ProcessInfo) -> Self {
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
        id: ProcessId,
        pid: ProcessPid,
        usage: Usage,
        attached: bool,
        locale: &Locale,
    ) -> ProcessEntry {
        ProcessEntry {
            id: id.to_string(),
            managed: true,
            shell: self.shell.clone(),
            shell_label: self
                .shell
                .rsplit('/')
                .next()
                .unwrap_or(&self.shell)
                .to_string(),
            cwd: self.cwd.clone(),
            cwd_label: match self.cwd.as_str() {
                "" | "/" => None,
                cwd => Some(cwd.to_string()),
            },
            cols: self.cols,
            rows: self.rows,
            pid: pid.0,
            uptime_secs: self.uptime_secs,
            uptime_label: ProcessUptime(self.uptime_secs).label(locale),
            cpu_percent: usage.cpu_percent,
            cpu_label: format!("{:.1}", usage.cpu_percent),
            mem_bytes: usage.mem_bytes,
            memory_label: ProcessMemory(usage.mem_bytes).label(),
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
struct LocalVmuxProcess(LocalProcess);

impl LocalVmuxProcess {
    fn entry(&self, pid: ProcessPid, usage: Usage, locale: &Locale) -> ProcessEntry {
        let process = &self.0;
        ProcessEntry {
            id: format!("system:{}", pid.0),
            managed: false,
            shell: process.shell.clone(),
            shell_label: process
                .shell
                .rsplit('/')
                .next()
                .unwrap_or(&process.shell)
                .to_string(),
            cwd: process.cwd.clone(),
            cwd_label: match process.cwd.as_str() {
                "" | "/" => None,
                cwd => Some(cwd.to_string()),
            },
            cols: 0,
            rows: 0,
            pid: pid.0,
            uptime_secs: process.uptime_secs,
            uptime_label: ProcessUptime(process.uptime_secs).label(locale),
            cpu_percent: usage.cpu_percent,
            cpu_label: format!("{:.1}", usage.cpu_percent),
            mem_bytes: usage.mem_bytes,
            memory_label: ProcessMemory(usage.mem_bytes).label(),
            attached: false,
            preview_lines: Vec::new(),
        }
    }
}

impl From<LocalProcess> for LocalVmuxProcess {
    fn from(process: LocalProcess) -> Self {
        Self(process)
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Process monitor"), ProcessMonitor::default()));
}

fn reconcile_service_processes(
    mut snapshots: MessageReader<ServiceProcessSnapshot>,
    existing: Query<(Entity, &ProcessId), With<ServiceProcess>>,
    runtime: Query<Entity, With<ProcessMonitor>>,
    mut commands: Commands,
) {
    let Some(snapshot) = snapshots.read().last() else {
        return;
    };
    let mut by_id = HashMap::new();
    for (entity, id) in &existing {
        by_id.insert(*id, entity);
    }
    for (index, process) in snapshot.0.iter().enumerate() {
        let components = (
            Name::new(format!("Service process {}", process.id)),
            process.id,
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
    monitors: Query<(), (With<ProcessMonitorView>, With<KeyboardOwner>)>,
    claimed: Query<(), (With<ProcessMonitorView>, Added<KeyboardOwner>)>,
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
    monitors: Query<(), (With<ProcessMonitorView>, With<KeyboardOwner>)>,
    claimed: Query<(), (With<ProcessMonitorView>, Added<KeyboardOwner>)>,
    opened: Query<(), Added<ProcessMonitorView>>,
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
    let service_roots = service_processes
        .iter_mut()
        .map(|(pid, _)| pid.0)
        .collect::<Vec<_>>();
    let Some(snapshot) = runtime.sampler.refresh(
        time.delta(),
        !claimed.is_empty(),
        !opened.is_empty() || !claimed.is_empty(),
        service_roots,
    ) else {
        return;
    };

    for (pid, mut usage) in &mut service_processes {
        *usage = Usage::subtree(pid.0, &snapshot.samples);
    }

    let mut existing = HashMap::new();
    for (entity, pid) in &local_processes {
        existing.insert(pid.0, entity);
    }
    for (pid, details) in snapshot.local {
        let pid = ProcessPid(pid);
        let usage = snapshot
            .samples
            .get(&pid.0)
            .map(|sample| Usage {
                cpu_percent: sample.cpu,
                mem_bytes: sample.mem,
            })
            .unwrap_or_default();
        if let Some(entity) = existing.remove(&pid.0) {
            commands
                .entity(entity)
                .insert((LocalVmuxProcess::from(details), usage));
        } else {
            commands.spawn((
                Name::new(format!("Vmux process {}", pid.0)),
                pid,
                LocalVmuxProcess::from(details),
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
    service_processes: Query<(&ProcessId, &ProcessPid, &ServiceProcess, &Usage, &Order)>,
    local_processes: Query<(&ProcessPid, &LocalVmuxProcess, &Usage)>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut monitors: Query<
        (
            Entity,
            &mut ProcessMonitorView,
            Has<ProcessMonitorViewDirty>,
        ),
        (With<PageReady>, With<KeyboardOwner>),
    >,
    claimed: Query<(), (With<ProcessMonitorView>, Added<KeyboardOwner>)>,
    terminal_pids: Query<&ProcessId, With<Terminal>>,
    locale: Option<Res<ResolvedLocale>>,
    mut commands: Commands,
) {
    if monitors.is_empty() {
        return;
    }
    let Ok((runtime_entity, dirty)) = runtime.single() else {
        return;
    };
    if !dirty && claimed.is_empty() && monitors.iter().all(|(_, _, view_dirty)| !view_dirty) {
        return;
    }

    let connected = connected.is_some();
    let locale = locale
        .as_deref()
        .map(|resolved| resolved.0.clone())
        .unwrap_or_else(Locale::preferred);
    let attached_ids: std::collections::HashSet<ProcessId> =
        terminal_pids.iter().copied().collect();
    let mut managed_pids = std::collections::HashSet::new();
    let mut ordered = Vec::new();
    for (id, pid, process, usage, order) in &service_processes {
        managed_pids.insert(pid.0);
        ordered.push((
            order.0,
            process.entry(*id, *pid, *usage, attached_ids.contains(id), &locale),
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
            local.push((pid.0, process.entry(*pid, *usage, &locale)));
        }
    }
    local.sort_by_key(|(pid, _)| *pid);
    for (_, process) in local {
        processes.push(process);
    }

    let total_count = processes.len() as u32;
    let managed_count = processes.iter().filter(|process| process.managed).count() as u32;
    let total_cpu_percent = processes
        .iter()
        .map(|process| process.cpu_percent)
        .sum::<f32>();
    let total_memory_bytes = processes
        .iter()
        .map(|process| process.mem_bytes)
        .sum::<u64>();

    for (entity, mut monitor, view_dirty) in &mut monitors {
        if dirty || monitor.cpu_history.is_empty() {
            monitor.cpu_history.push_back(total_cpu_percent.max(0.0));
            monitor
                .memory_history_mb
                .push_back(total_memory_bytes as f32 / (1024.0 * 1024.0));
            while monitor.cpu_history.len() > PROCESS_HISTORY_LIMIT {
                monitor.cpu_history.pop_front();
            }
            while monitor.memory_history_mb.len() > PROCESS_HISTORY_LIMIT {
                monitor.memory_history_mb.pop_front();
            }
        }
        let query = monitor.query.trim().to_ascii_lowercase();
        let mut visible = processes
            .iter()
            .filter(|process| {
                query.is_empty()
                    || process.id.to_ascii_lowercase().contains(&query)
                    || process.shell.to_ascii_lowercase().contains(&query)
                    || process.cwd.to_ascii_lowercase().contains(&query)
                    || process.pid.to_string().contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>();
        visible.sort_by(|left, right| {
            right
                .cpu_percent
                .total_cmp(&left.cpu_percent)
                .then_with(|| right.mem_bytes.cmp(&left.mem_bytes))
                .then_with(|| left.pid.cmp(&right.pid))
        });
        let peak_cpu_percent = monitor.cpu_history.iter().copied().fold(0.0_f32, f32::max);
        let peak_memory_bytes = (monitor
            .memory_history_mb
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            * 1024.0
            * 1024.0) as u64;
        let cpu_history = monitor.cpu_history.iter().copied().collect::<Vec<_>>();
        let memory_history = monitor
            .memory_history_mb
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let cpu_graph = Sparkline::plot(&cpu_history, 5.0);
        let memory_graph = Sparkline::plot(&memory_history, 64.0);
        let state = ProcessesUiState {
            connected,
            query: monitor.query.clone(),
            total_count,
            managed_count,
            cpu: ProcessUsageUiState {
                label: "CPU".to_string(),
                value: format!("{total_cpu_percent:.1}%"),
                peak: format!("↑ {peak_cpu_percent:.1}%"),
                line: cpu_graph.line,
                area: cpu_graph.area,
            },
            memory: ProcessUsageUiState {
                label: locale.translate("services-memory"),
                value: ProcessMemory(total_memory_bytes).label(),
                peak: format!("↑ {}", ProcessMemory(peak_memory_bytes).label()),
                line: memory_graph.line,
                area: memory_graph.area,
            },
            processes: visible,
        };
        commands.trigger(UiStateWrite::<ProcessesUiState>::from_event(entity, &state));
        if view_dirty {
            commands.entity(entity).remove::<ProcessMonitorViewDirty>();
        }
    }
    commands
        .entity(runtime_entity)
        .remove::<ProcessMonitorDirty>();
}

fn search(
    trigger: On<UiInput<ProcessSearchRequest>>,
    mut monitors: Query<&mut ProcessMonitorView>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok(mut monitor) = monitors.get_mut(target) else {
        return;
    };
    let query = trigger.event().payload.query.trim().to_string();
    if monitor.query == query {
        return;
    }
    monitor.query = query;
    commands.entity(target).insert(ProcessMonitorViewDirty);
}

fn process_navigate(
    trigger: On<UiInput<ProcessNavigateEvent>>,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<&ChildOf, With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Stack>>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
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
    let (_, active_pane, _) = focus.resolve(active_tab_param.get());
    let Some(pane) = active_pane else { return };

    let tab = commands
        .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    commands.spawn((ReattachedTerminalBundle::new(process_id), ChildOf(tab)));
}

fn process_kill(
    trigger: On<UiInput<ProcessKillEvent>>,
    service_processes: Query<(Entity, &ProcessId), With<ServiceProcess>>,
    runtime: Query<Entity, With<ProcessMonitor>>,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<&ChildOf, With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Stack>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let pid = &trigger.event().payload.process_id;

    if let Ok(process_id) = pid.parse::<ProcessId>() {
        service_requests.write(ServiceRequest(ClientMessage::KillProcess { process_id }));
        for (entity, id) in &service_processes {
            if *id == process_id {
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

fn process_kill_all(
    _trigger: On<UiInput<ProcessKillAllEvent>>,
    service_processes: Query<(Entity, &ProcessId), With<ServiceProcess>>,
    runtime: Query<Entity, With<ProcessMonitor>>,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<&ChildOf, With<Terminal>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let process_ids: Vec<(Entity, ProcessId)> = service_processes
        .iter()
        .map(|(entity, id)| (entity, *id))
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

    #[test]
    fn sparkline_uses_the_visible_sample_range() {
        let graph = Sparkline::plot(&[19.0, 19.5, 20.0], 5.0);

        assert_eq!(
            graph.line,
            "M 0.00 24.20 C 8.33 23.58, 33.33 21.73, 50.00 20.50 C 66.67 19.27, 91.67 17.42, 100.00 16.80"
        );
        assert!(graph.area.starts_with("M 0 40 L 0.00 24.20"));
        assert!(graph.area.ends_with("L 100 40 Z"));
    }

    #[test]
    fn process_memory_uses_readable_units() {
        assert_eq!(ProcessMemory(0).label(), "—");
        assert_eq!(ProcessMemory(512 * 1024).label(), "<1 MB");
        assert_eq!(ProcessMemory(332 * 1024 * 1024).label(), "332 MB");
        assert_eq!(ProcessMemory(3 * 1024 * 1024 * 1024 / 2).label(), "1.5 GB");
    }

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    fn process_info(id: ProcessId) -> vmux_api::protocol::ProcessInfo {
        vmux_api::protocol::ProcessInfo {
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
            .add_systems(Startup, spawn)
            .add_systems(Update, reconcile_service_processes);
        app.world_mut().write_message(ServiceProcessSnapshot(vec![
            process_info(keep),
            process_info(remove),
        ]));
        app.update();

        let mut ids = app
            .world_mut()
            .query_filtered::<&ProcessId, With<ServiceProcess>>()
            .iter(app.world())
            .copied()
            .collect::<Vec<_>>();
        ids.sort_by_key(|id| id.0);
        assert_eq!(ids, vec![keep, remove]);

        app.world_mut()
            .write_message(ServiceProcessSnapshot(vec![process_info(keep)]));
        app.update();

        let ids = app
            .world_mut()
            .query_filtered::<&ProcessId, With<ServiceProcess>>()
            .iter(app.world())
            .copied()
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
        let locale = Locale::from("en-US");
        let entry = ServiceProcess::from(&process_info(id)).entry(
            id,
            ProcessPid(42),
            Usage {
                cpu_percent: 12.5,
                mem_bytes: 332 * 1024 * 1024,
            },
            false,
            &locale,
        );
        assert_eq!(entry.pid, 42);
        assert_eq!(entry.cpu_percent, 12.5);
        assert_eq!(entry.mem_bytes, 332 * 1024 * 1024);
        assert!(!entry.attached);
    }

    #[test]
    fn service_process_entry_defaults_usage() {
        let id = process_id(1);
        let locale = Locale::from("en-US");
        let entry = ServiceProcess::from(&process_info(id)).entry(
            id,
            ProcessPid(42),
            Usage::default(),
            false,
            &locale,
        );
        assert_eq!(entry.cpu_percent, 0.0);
        assert_eq!(entry.mem_bytes, 0);
    }

    #[test]
    fn local_vmux_process_entry_is_unmanaged() {
        let locale = Locale::from("en-US");
        let process = LocalVmuxProcess(LocalProcess {
            shell: "/Applications/Vmux.app/Contents/MacOS/vmux_desktop".to_string(),
            cwd: "/tmp".to_string(),
            uptime_secs: 10,
        });
        let entry = process.entry(
            ProcessPid(42),
            Usage {
                cpu_percent: 3.0,
                mem_bytes: 1024,
            },
            &locale,
        );
        assert!(!entry.managed);
        assert_eq!(entry.pid, 42);
        assert_eq!(entry.cpu_percent, 3.0);
        assert_eq!(entry.mem_bytes, 1024);
    }
}
