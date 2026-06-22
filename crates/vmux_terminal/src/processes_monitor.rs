use std::collections::HashMap;

use bevy::{ecs::relationship::Relationship, picking::Pickable, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::page::PageReady;
use vmux_history::LastActivatedAt;
use vmux_service::event::*;
use vmux_service::protocol::{ClientMessage, ProcessId};

use crate::Terminal;
use crate::plugin::{ServiceClient, reattach_terminal_bundle};
use vmux_layout::{
    cef::Browser,
    event::SERVICES_PAGE_URL,
    pane::{Pane, PaneSplit},
    stack::{ActiveTabParam, Stack, focused_stack, stack_bundle},
};

#[derive(Component)]
pub struct ProcessesMonitor;

impl ProcessesMonitor {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SERVICES_PAGE_URL),
                ResolvedWebviewUri(SERVICES_PAGE_URL.to_string()),
                PageMetadata {
                    title: "Background Services".to_string(),
                    url: SERVICES_PAGE_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
                WebviewSize(Vec2::new(1280.0, 720.0)),
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
}

#[derive(Resource, Default)]
pub struct ServiceProcessList {
    pub processes: Vec<vmux_service::protocol::ProcessInfo>,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Usage {
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

#[derive(Resource, Default)]
pub struct ProcessUsage(pub HashMap<u32, Usage>);

struct ProcSample {
    parent: Option<u32>,
    cpu: f32,
    mem: u64,
}

fn subtree_usage(root: u32, procs: &HashMap<u32, ProcSample>) -> Usage {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, s) in procs {
        if let Some(parent) = s.parent {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut total = Usage::default();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(s) = procs.get(&pid) {
            total.cpu_percent += s.cpu;
            total.mem_bytes += s.mem;
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
    }
    total
}

#[derive(Resource)]
struct ProcessesPollTimer(Timer);

#[derive(Resource)]
struct SysinfoPollTimer(Timer);

#[derive(Resource)]
struct SysinfoState(sysinfo::System);

impl Default for SysinfoState {
    fn default() -> Self {
        Self(sysinfo::System::new())
    }
}

pub struct ProcessesMonitorPlugin;

impl Plugin for ProcessesMonitorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServiceProcessList>()
            .init_resource::<ProcessUsage>()
            .init_resource::<SysinfoState>()
            .insert_resource(ProcessesPollTimer(Timer::from_seconds(
                1.0,
                TimerMode::Repeating,
            )))
            .insert_resource(SysinfoPollTimer(Timer::from_seconds(
                1.0,
                TimerMode::Repeating,
            )))
            .add_plugins(BinEventEmitterPlugin::<(
                ProcessNavigateEvent,
                ProcessKillEvent,
                ProcessKillAllEvent,
            )>::for_hosts(&["services"]))
            .add_systems(
                Update,
                (
                    request_process_list,
                    sample_process_usage,
                    broadcast_to_monitors,
                )
                    .chain(),
            )
            .add_observer(on_process_navigate)
            .add_observer(on_process_kill)
            .add_observer(on_process_kill_all);
    }
}

/// Periodically send ListProcesses to the service.
fn request_process_list(
    time: Res<Time>,
    mut timer: ResMut<ProcessesPollTimer>,
    service: Option<Res<ServiceClient>>,
    monitors: Query<(), With<ProcessesMonitor>>,
) {
    if monitors.is_empty() {
        return;
    }
    timer.0.tick(time.delta());
    if timer.0.just_finished()
        && let Some(service) = service
    {
        service.0.send(ClientMessage::ListProcesses);
    }
}

fn sample_process_usage(
    time: Res<Time>,
    mut timer: ResMut<SysinfoPollTimer>,
    monitors: Query<(), With<ProcessesMonitor>>,
    process_list: Res<ServiceProcessList>,
    mut sys: ResMut<SysinfoState>,
    mut usage: ResMut<ProcessUsage>,
) {
    if monitors.is_empty() {
        return;
    }
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    sys.0
        .refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let procs: HashMap<u32, ProcSample> = sys
        .0
        .processes()
        .iter()
        .map(|(pid, p)| {
            (
                pid.as_u32(),
                ProcSample {
                    parent: p.parent().map(|pp| pp.as_u32()),
                    cpu: p.cpu_usage(),
                    mem: p.memory(),
                },
            )
        })
        .collect();

    let mut map = HashMap::with_capacity(process_list.processes.len());
    for info in &process_list.processes {
        map.insert(info.pid, subtree_usage(info.pid, &procs));
    }
    usage.0 = map;
}

fn build_process_entries(
    processes: &[vmux_service::protocol::ProcessInfo],
    usage: &ProcessUsage,
    attached_ids: &std::collections::HashSet<String>,
) -> Vec<ProcessEntry> {
    processes
        .iter()
        .map(|info| {
            let u = usage.0.get(&info.pid).copied().unwrap_or_default();
            ProcessEntry {
                id: info.id.to_string(),
                shell: info.shell.clone(),
                cwd: info.cwd.clone(),
                cols: info.cols,
                rows: info.rows,
                pid: info.pid,
                uptime_secs: info.created_at_secs,
                cpu_percent: u.cpu_percent,
                mem_bytes: u.mem_bytes,
                attached: attached_ids.contains(&info.id.to_string()),
                preview_lines: Vec::new(),
            }
        })
        .collect()
}

/// Broadcast the cached process list to all process monitor webviews.
fn broadcast_to_monitors(
    process_list: Res<ServiceProcessList>,
    usage: Res<ProcessUsage>,
    service: Option<Res<ServiceClient>>,
    monitors: Query<Entity, (With<ProcessesMonitor>, With<PageReady>)>,
    browsers: NonSend<Browsers>,
    terminal_pids: Query<&ProcessId, With<Terminal>>,
    mut commands: Commands,
) {
    if monitors.is_empty() || !(process_list.is_changed() || usage.is_changed()) {
        return;
    }

    let connected = service.is_some();

    // Build attached set from local terminal handles
    let attached_ids: std::collections::HashSet<String> =
        terminal_pids.iter().map(|pid| pid.to_string()).collect();

    let processes = build_process_entries(&process_list.processes, &usage, &attached_ids);

    let event = ProcessesListEvent {
        connected,
        processes,
    };

    for entity in &monitors {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                PROCESSES_LIST_EVENT,
                &event,
            ));
        }
    }
}

/// Navigate to the terminal tab for the clicked process, or open a new one.
fn on_process_navigate(
    trigger: On<BinReceive<ProcessNavigateEvent>>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Stack>>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut commands: Commands,
) {
    let pid = &trigger.event().payload.process_id;

    // If a tab already has this process attached, activate it
    for (_, process_id, content_child_of) in &terminals {
        if process_id.to_string() == *pid {
            let tab = content_child_of.get();
            commands.entity(tab).insert(LastActivatedAt::now());
            if let Ok(tab_child_of) = tab_parent.get(tab) {
                commands
                    .entity(tab_child_of.get())
                    .insert(LastActivatedAt::now());
            }
            return;
        }
    }

    // No existing tab — open a new one with reattach
    let Ok(process_id) = pid.parse::<ProcessId>() else {
        warn!("Invalid process ID from navigate event: {pid}");
        return;
    };
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
    commands.spawn((
        reattach_terminal_bundle(&mut meshes, &mut webview_mt, process_id),
        ChildOf(tab),
    ));
}

/// Kill a single service-managed process and close the associated terminal tab if any.
fn on_process_kill(
    trigger: On<BinReceive<ProcessKillEvent>>,
    service: Option<Res<ServiceClient>>,
    mut process_list: ResMut<ServiceProcessList>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Stack>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    let pid = &trigger.event().payload.process_id;

    if let Ok(process_id) = pid.parse::<ProcessId>() {
        service.0.send(ClientMessage::KillProcess { process_id });
        remove_processes_from_cached_list(&mut process_list, [process_id]);
        service.0.send(ClientMessage::ListProcesses);

        for (_, terminal_pid, content_child_of) in &terminals {
            if *terminal_pid == process_id {
                let tab = content_child_of.get();
                // Only despawn if it's actually a tab
                if tab_parent.get(tab).is_ok() || commands.get_entity(tab).is_ok() {
                    commands.entity(tab).despawn();
                }
                break;
            }
        }
    }
}

/// Kill all service-managed processes and close their terminal tabs.
fn on_process_kill_all(
    _trigger: On<BinReceive<ProcessKillAllEvent>>,
    service: Option<Res<ServiceClient>>,
    mut process_list: ResMut<ServiceProcessList>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), With<Terminal>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    let process_ids: Vec<ProcessId> = process_list.processes.iter().map(|info| info.id).collect();

    for process_id in &process_ids {
        service.0.send(ClientMessage::KillProcess {
            process_id: *process_id,
        });

        for (_, terminal_pid, content_child_of) in &terminals {
            if *terminal_pid == *process_id {
                let tab = content_child_of.get();
                commands.entity(tab).despawn();
                break;
            }
        }
    }
    if !process_ids.is_empty() {
        remove_processes_from_cached_list(&mut process_list, process_ids);
        service.0.send(ClientMessage::ListProcesses);
    }
}

fn remove_processes_from_cached_list(
    process_list: &mut ServiceProcessList,
    process_ids: impl IntoIterator<Item = ProcessId>,
) {
    let process_ids: std::collections::HashSet<ProcessId> = process_ids.into_iter().collect();
    if process_ids.is_empty() {
        return;
    }
    process_list
        .processes
        .retain(|info| !process_ids.contains(&info.id));
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
    fn remove_process_from_cached_list_is_optimistic() {
        let keep = process_id(1);
        let kill = process_id(2);
        let mut list = ServiceProcessList {
            processes: vec![process_info(keep), process_info(kill)],
        };

        remove_processes_from_cached_list(&mut list, [kill]);

        assert_eq!(list.processes.len(), 1);
        assert_eq!(list.processes[0].id, keep);
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
        let u = subtree_usage(1, &procs);
        assert_eq!(u.cpu_percent, 16.0);
        assert_eq!(u.mem_bytes, 350);
    }

    #[test]
    fn subtree_usage_missing_root_is_zero() {
        let procs = HashMap::new();
        assert_eq!(subtree_usage(5, &procs), Usage::default());
    }

    #[test]
    fn build_entries_attaches_usage() {
        let id = process_id(1);
        let mut usage = ProcessUsage::default();
        usage.0.insert(
            42,
            Usage {
                cpu_percent: 12.5,
                mem_bytes: 332 * 1024 * 1024,
            },
        );
        let entries = build_process_entries(&[process_info(id)], &usage, &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pid, 42);
        assert_eq!(entries[0].cpu_percent, 12.5);
        assert_eq!(entries[0].mem_bytes, 332 * 1024 * 1024);
        assert!(!entries[0].attached);
    }

    #[test]
    fn build_entries_defaults_usage_when_missing() {
        let entries = build_process_entries(
            &[process_info(process_id(1))],
            &ProcessUsage::default(),
            &Default::default(),
        );
        assert_eq!(entries[0].cpu_percent, 0.0);
        assert_eq!(entries[0].mem_bytes, 0);
    }
}
