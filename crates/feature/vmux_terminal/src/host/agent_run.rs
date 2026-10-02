use std::path::{Path, PathBuf};

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_api::protocol::{AgentCommandResult, ProcessId};
use vmux_api::service::RUN_OSC;
#[cfg(test)]
use vmux_api::terminal::CursorStyle;
use vmux_ecs::LastActivatedAt;
#[cfg(test)]
use vmux_ecs::PageMetadata;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentReply, AgentRequestApplySet, AgentRequestBlocked, AgentRequestInput,
};
use vmux_ecs::profile::ProjectsDirectory;
use vmux_layout::AgentPaneDirection;
#[cfg(test)]
use vmux_layout::LayoutContractPlugin;
#[cfg(test)]
use vmux_layout::pane::Pane;
use vmux_layout::pane::{PaneSplitDirection, SpawnCounter, SpawnSeq};
use vmux_layout::placement::PageKind;
#[cfg(test)]
use vmux_layout::stack::Stack;
use vmux_layout::tab::Tab;
use vmux_setting::{AppSettings, StartupDir};
#[cfg(test)]
use vmux_setting::{TerminalSettings, TerminalTheme};

#[cfg(test)]
use crate::TerminalContractPlugin;
use crate::launch::TerminalLaunch;
use crate::{
    AgentRunTerminal, ProcessExited, Terminal, TerminalReinputRequest, TerminalStackSpawnRequest,
};
use vmux_space::valid_cwd;

#[vmux_api::contract(Copy, Eq)]
pub enum PlacementMode {
    Auto,
    Split,
    Stack,
}

#[vmux_api::agent]
pub struct AgentRun {
    pub anchor: ProcessId,
    pub command: String,
    pub direction: AgentPaneDirection,
    pub focus: bool,
    pub beside: Option<ProcessId>,
    pub mode: PlacementMode,
    pub terminal: Option<ProcessId>,
    pub done_marker: Option<String>,
}

#[vmux_api::agent]
pub struct AgentRunWithPlacementOverride(pub AgentRun);

pub(super) struct AgentRunPlugin;

impl Plugin for AgentRunPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentRequestBlocked>()
            .add_systems(
                Update,
                run_agent_commands
                    .in_set(AgentRequestApplySet)
                    .before(super::TerminalStackSpawnSet),
            );
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AgentTerminalRegion {
    run_terminal: Option<ProcessId>,
    run_pane: Option<Entity>,
}

impl AgentTerminalRegion {
    fn choose_reusable_terminal(
        &self,
        agent_pane: Entity,
        candidates: &[RunTerminalCandidate],
    ) -> Option<RunTerminalCandidate> {
        if let Some(pid) = self.run_terminal
            && let Some(candidate) = candidates.iter().find(|candidate| candidate.pid == pid)
        {
            return Some(*candidate);
        }
        if let Some(pane) = self.run_pane
            && let Some(candidate) = candidates
                .iter()
                .filter(|candidate| candidate.pane == pane)
                .max_by_key(|candidate| candidate.pane_spawn_seq)
        {
            return Some(*candidate);
        }
        candidates
            .iter()
            .filter(|c| c.pane != agent_pane)
            .max_by_key(|c| c.pane_spawn_seq)
            .copied()
    }

    fn choose_bucket_pane(
        &self,
        agent_pane: Entity,
        candidates: &[RunTerminalCandidate],
    ) -> Option<Entity> {
        self.choose_reusable_terminal(agent_pane, candidates)
            .map(|c| c.pane)
            .or_else(|| self.run_pane.filter(|pane| *pane != agent_pane))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RunTerminalCandidate {
    terminal: Entity,
    pid: ProcessId,
    stack: Entity,
    pane: Entity,
    pane_spawn_seq: u64,
}

impl RunTerminalCandidate {
    fn launch_matches_canonical_cwd(launch_cwd: &str, desired_cwd: &Path) -> bool {
        let Some(launch_cwd) = valid_cwd(launch_cwd).ok().flatten() else {
            return false;
        };
        let launch_cwd = launch_cwd.canonicalize().unwrap_or(launch_cwd);
        launch_cwd == desired_cwd
    }

    #[cfg(test)]
    fn launch_matches_cwd(launch_cwd: &str, desired_cwd: &Path) -> bool {
        let desired_cwd = desired_cwd
            .canonicalize()
            .unwrap_or_else(|_| desired_cwd.to_path_buf());
        Self::launch_matches_canonical_cwd(launch_cwd, &desired_cwd)
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct RunTerminals<'w, 's> {
    candidates: Query<
        'w,
        's,
        (
            Entity,
            &'static ProcessId,
            &'static TerminalLaunch,
            Has<AgentRunTerminal>,
        ),
        (With<Terminal>, Without<ProcessExited>),
    >,
    terminals: Query<'w, 's, (Entity, &'static ProcessId), With<Terminal>>,
    launches: Query<'w, 's, &'static TerminalLaunch>,
}

impl RunTerminals<'_, '_> {
    fn candidates(
        &self,
        agent_pane: Entity,
        panes: &AgentPanes,
        desired_cwd: &Path,
    ) -> Vec<RunTerminalCandidate> {
        let Some(agent_tab) = panes.tab(agent_pane) else {
            return Vec::new();
        };
        let desired_cwd = desired_cwd
            .canonicalize()
            .unwrap_or_else(|_| desired_cwd.to_path_buf());
        self.candidates
            .iter()
            .filter_map(|(terminal, pid, launch, agent_run)| {
                if !agent_run {
                    return None;
                }
                let stack = panes.placement.child_of_q.get(terminal).ok()?.get();
                let pane = panes.placement.child_of_q.get(stack).ok()?.get();
                if pane == agent_pane {
                    return None;
                }
                if panes.tab(pane) != Some(agent_tab) {
                    return None;
                }
                if !RunTerminalCandidate::launch_matches_canonical_cwd(&launch.cwd, &desired_cwd) {
                    return None;
                }
                Some(RunTerminalCandidate {
                    terminal,
                    pid: *pid,
                    stack,
                    pane,
                    pane_spawn_seq: panes.placement.seq_q.get(pane).map(|s| s.0).unwrap_or(0),
                })
            })
            .collect()
    }

    fn pane(&self, process_id: ProcessId, panes: &AgentPanes) -> Option<Entity> {
        let (terminal, _) = self
            .terminals
            .iter()
            .find(|(_, candidate)| **candidate == process_id)?;
        let stack = panes.placement.child_of_q.get(terminal).ok()?.get();
        panes
            .placement
            .child_of_q
            .get(stack)
            .ok()
            .map(Relationship::get)
    }

    fn launch(&self, process_id: ProcessId) -> Result<TerminalLaunch, String> {
        let Some(entity) = self
            .terminals
            .iter()
            .find_map(|(entity, candidate)| (*candidate == process_id).then_some(entity))
        else {
            return Err(format!("run.terminal page not found: {process_id}"));
        };
        self.launches
            .get(entity)
            .cloned()
            .map_err(|_| format!("run terminal launch not found: {process_id}"))
    }

    fn cwd(&self, entity: Entity) -> Option<String> {
        self.launches
            .get(entity)
            .ok()
            .map(|launch| launch.cwd.clone())
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentPanes<'w, 's> {
    placement: vmux_layout::pane::PanePlacement<'w, 's>,
}

impl AgentPanes<'_, '_> {
    fn tab(&self, pane: Entity) -> Option<Entity> {
        let mut cur = pane;
        for _ in 0..32 {
            if self.placement.tab_q.contains(cur) {
                return Some(cur);
            }
            cur = self.placement.child_of_q.get(cur).ok()?.get();
        }
        None
    }

    fn split(
        &self,
        pane: Entity,
        direction: &AgentPaneDirection,
        focus: bool,
        split_this_batch: &mut std::collections::HashSet<Entity>,
    ) -> AgentPaneSplit {
        let existing_tabs: Vec<Entity> = self
            .placement
            .pane_children
            .get(pane)
            .map(|children| {
                children
                    .iter()
                    .filter(|&entity| self.placement.tab_filter.contains(entity))
                    .collect()
            })
            .unwrap_or_default();
        let split_dir = PaneSplitDirection::from(PaneDirection::from(*direction));
        let already_split =
            !split_this_batch.insert(pane) || self.placement.split_dir_q.contains(pane);
        AgentPaneSplit {
            pane,
            direction: split_dir,
            existing_tabs,
            focus,
            already_split,
        }
    }

    fn activation_entities(&self, candidate: RunTerminalCandidate) -> Vec<Entity> {
        let mut entities = vec![candidate.stack, candidate.pane];
        if let Some(tab) = self.tab(candidate.pane) {
            entities.push(tab);
        }
        entities
    }

    fn bucket_panes(&self, agent_pane: Entity) -> RunTerminalBucketPanes {
        let Some(agent_tab) = self.tab(agent_pane) else {
            return RunTerminalBucketPanes(Vec::new());
        };
        let mut candidates = Vec::new();
        for pane in &self.placement.leaf_panes {
            if pane == agent_pane || self.tab(pane) != Some(agent_tab) {
                continue;
            }
            let Ok(children) = self.placement.pane_children.get(pane) else {
                continue;
            };
            let mut has_stack = false;
            let mut valid = true;
            for stack in children
                .iter()
                .filter(|&child| self.placement.tab_filter.contains(child))
            {
                has_stack = true;
                let Ok(meta) = self.placement.page_q.get(stack) else {
                    valid = false;
                    break;
                };
                if PageKind::for_url(&meta.url) != PageKind::Terminal {
                    valid = false;
                    break;
                }
            }
            if has_stack && valid {
                candidates.push(RunTerminalBucketPaneCandidate {
                    pane,
                    pane_spawn_seq: self
                        .placement
                        .seq_q
                        .get(pane)
                        .map(|sequence| sequence.0)
                        .unwrap_or(0),
                });
            }
        }
        RunTerminalBucketPanes(candidates)
    }
}

struct AgentPaneSplit {
    pane: Entity,
    direction: PaneSplitDirection,
    existing_tabs: Vec<Entity>,
    focus: bool,
    already_split: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
struct NextPaneSpawnSequence<'w, 's> {
    counter: Single<'w, 's, &'static mut SpawnCounter>,
    sequences: Query<'w, 's, &'static SpawnSeq>,
}

impl NextPaneSpawnSequence<'_, '_> {
    fn take(&mut self) -> SpawnSeq {
        let max_existing = self
            .sequences
            .iter()
            .map(|sequence| sequence.0)
            .max()
            .unwrap_or(0);
        if self.counter.0 <= max_existing {
            self.counter.0 = max_existing;
        }
        self.counter.0 += 1;
        SpawnSeq(self.counter.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RunTerminalBucketPaneCandidate {
    pane: Entity,
    pane_spawn_seq: u64,
}

struct RunTerminalBucketPanes(Vec<RunTerminalBucketPaneCandidate>);

impl RunTerminalBucketPanes {
    fn newest(&self, agent_pane: Entity) -> Option<Entity> {
        self.0
            .iter()
            .filter(|c| c.pane != agent_pane)
            .max_by_key(|c| c.pane_spawn_seq)
            .map(|c| c.pane)
    }

    fn contains(&self, pane: Entity) -> bool {
        self.0.iter().any(|c| c.pane == pane)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRunTerminalSpawn {
    pid: ProcessId,
    request_index: usize,
    shell: String,
}

#[derive(Default)]
struct PendingRunTerminalSpawns(std::collections::HashMap<ProcessId, PendingRunTerminalSpawn>);

impl PendingRunTerminalSpawns {
    fn insert(&mut self, anchor: ProcessId, spawn: PendingRunTerminalSpawn) {
        self.0.insert(anchor, spawn);
    }

    fn append_input(
        &self,
        anchor: ProcessId,
        terminal_spawns: &mut [TerminalStackSpawnRequest],
        desired_cwd: &Path,
        run: RunCommand<'_>,
    ) -> Option<ProcessId> {
        let pending = self.0.get(&anchor)?;
        let request = terminal_spawns.get_mut(pending.request_index)?;
        let request_cwd = request.cwd.as_deref()?.canonicalize().ok()?;
        let desired_cwd = desired_cwd.canonicalize().ok()?;
        if request_cwd != desired_cwd {
            return None;
        }
        let data = run.input(&pending.shell, PagerEnv::Inherited);
        match &mut request.pending_input {
            Some(input) => input.extend(data),
            None => request.pending_input = Some(data),
        }
        Some(pending.pid)
    }
}

#[derive(Clone, Copy, Debug)]
struct RunCommand<'a> {
    command: &'a str,
    token: Option<&'a str>,
}

impl<'a> RunCommand<'a> {
    fn new(command: &'a str, token: Option<&'a str>) -> Self {
        Self { command, token }
    }

    fn line(&self, shell: &str, pager: PagerEnv) -> String {
        match self.token {
            Some(token) => command_with_marker(shell, self.command, token, pager),
            None => self.command.to_string(),
        }
    }

    fn input(&self, shell: &str, pager: PagerEnv) -> Vec<u8> {
        let mut data = self.line(shell, pager).into_bytes();
        data.push(b'\r');
        data
    }

    fn reinput(
        &self,
        process_id: ProcessId,
        launch: &TerminalLaunch,
        pager: PagerEnv,
    ) -> TerminalReinputRequest {
        TerminalReinputRequest {
            process_id,
            data: self.input(&launch.command, pager),
        }
    }

    fn for_new_terminal(&self, settings: &AppSettings) -> (AgentTerminalShell, Vec<u8>) {
        let shell = AgentTerminalShell::configured(settings);
        let input = self.input(shell.as_str(), PagerEnv::Set);
        (shell, input)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PagerEnv {
    Set,
    Inherited,
}

impl PagerEnv {
    fn prefix(self, base: &str) -> &'static str {
        if self == Self::Inherited {
            return "";
        }
        match base {
            "nu" | "nushell" => {
                "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; "
            }
            "fish" => "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ",
            _ => "export GIT_PAGER=cat PAGER=cat LESS=FRX; ",
        }
    }
}

fn command_with_marker(shell: &str, command: &str, token: &str, env: PagerEnv) -> String {
    let base = Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(shell);
    let pager = env.prefix(base);
    let osc = RUN_OSC;
    match base {
        "nu" | "nushell" => format!(
            "{pager}$env.LAST_EXIT_CODE = 0; let __vmux_status = try {{ {command}; $env.LAST_EXIT_CODE }} catch {{|error| $error.exit_code? | default 1 }}; print -rn $\"\\u{{1b}}]{osc};{token};($__vmux_status)\\u{{7}}\""
        ),
        "fish" => format!(
            "{pager}{command}; set __vmux_status $status; printf '\\033]{osc};{token};%s\\007' $__vmux_status"
        ),
        _ => format!(
            "{pager}{command}; __vmux_status=\"$?\"; printf '\\033]{osc};{token};%s\\007' \"$__vmux_status\""
        ),
    }
}

#[derive(Clone, Debug)]
pub struct AgentTerminalShell(String);

impl AgentTerminalShell {
    pub fn configured(settings: &AppSettings) -> Self {
        Self(
            settings
                .terminal
                .as_ref()
                .map(|t| t.resolve_theme(&t.default_theme).shell)
                .unwrap_or_else(|| {
                    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
                }),
        )
    }

    fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    fn validate(&self) -> Result<(), String> {
        let shell = &self.0;
        if vmux_path::Executable::find(shell).is_some() {
            Ok(())
        } else {
            Err(format!(
                "terminal shell not found or not executable: {shell}"
            ))
        }
    }
}

struct RunPlacementPolicy {
    placement_override: bool,
}

impl RunPlacementPolicy {
    const OVERRIDE_DISABLED: &'static str =
        "run placement overrides are disabled; omit mode, direction, and beside and retry";

    fn new(placement_override: bool) -> Self {
        Self { placement_override }
    }

    fn validate(&self, settings: &AppSettings) -> Result<(), &'static str> {
        if self.placement_override && !settings.agent.allow_run_placement_override {
            Err(Self::OVERRIDE_DISABLED)
        } else {
            Ok(())
        }
    }
}

pub struct AgentCwd<'a> {
    tab_cwd: Option<&'a str>,
}

impl<'a> AgentCwd<'a> {
    pub fn from_tab(tab_cwd: Option<&'a str>) -> Self {
        Self { tab_cwd }
    }

    pub fn stored(&self) -> Result<Option<PathBuf>, String> {
        let Some(tab_cwd) = self.tab_cwd else {
            return Ok(None);
        };
        StartupDir::from_tab(tab_cwd).map(|dir| Some(dir.path))
    }

    pub fn or_agent_launch(&self, agent_launch_cwd: Option<&str>) -> Result<PathBuf, String> {
        if let Some(path) = self.stored()? {
            return Ok(path);
        }
        if let Some(Ok(Some(path))) = agent_launch_cwd.map(valid_cwd) {
            return Ok(path);
        }
        Err("tab and agent project directories are missing".to_string())
    }

    pub fn projects() -> Result<PathBuf, String> {
        ProjectsDirectory::ensure().map(ProjectsDirectory::into_path)
    }
}

struct DecodedAgentRun {
    payload: AgentRun,
    placement_override: bool,
}

impl DecodedAgentRun {
    fn from_request(request: &AgentRequestInput) -> Option<Self> {
        if let Ok(Some(payload)) = request.decode::<AgentRun>() {
            return Some(Self {
                payload,
                placement_override: false,
            });
        }
        let payload = request
            .decode::<AgentRunWithPlacementOverride>()
            .ok()
            .flatten()?;
        Some(Self {
            payload: payload.0,
            placement_override: true,
        })
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AgentRunContext<'w, 's> {
    agent_terminals: Query<
        'w,
        's,
        (
            Entity,
            &'static ProcessId,
            &'static ChildOf,
            Option<&'static AgentTerminalRegion>,
        ),
    >,
    terminals: RunTerminals<'w, 's>,
    acp_sessions: Query<'w, 's, &'static vmux_session::AcpSession>,
    panes: AgentPanes<'w, 's>,
    tabs: Query<'w, 's, &'static Tab>,
    next_pane_sequence: NextPaneSpawnSequence<'w, 's>,
}

impl AgentRunContext<'_, '_> {
    fn resolve_pane(&self, anchor: ProcessId) -> Option<(Entity, Entity, AgentTerminalRegion)> {
        let (terminal, _, terminal_parent, region) = self
            .agent_terminals
            .iter()
            .find(|(_, process_id, _, _)| **process_id == anchor)?;
        let stack = terminal_parent.get();
        let pane = self.panes.placement.child_of_q.get(stack).ok()?.get();
        Some((terminal, pane, region.copied().unwrap_or_default()))
    }

    fn tab_cwd(&self, pane: Entity) -> Option<String> {
        let mut current = pane;
        loop {
            if let Ok(tab) = self.tabs.get(current) {
                return tab.startup_dir.clone();
            }
            current = self.panes.placement.child_of_q.get(current).ok()?.parent();
        }
    }

    fn agent_cwd(&self, terminal: Entity) -> Option<String> {
        self.terminals.cwd(terminal).or_else(|| {
            let mut current = terminal;
            loop {
                if let Ok(session) = self.acp_sessions.get(current) {
                    return Some(session.cwd.to_string_lossy().into_owned());
                }
                current = self.panes.placement.child_of_q.get(current).ok()?.parent();
            }
        })
    }
}

fn run_agent_commands(
    mut requests: MessageReader<AgentRequestInput>,
    mut blocked: MessageReader<AgentRequestBlocked>,
    mut context: AgentRunContext,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut terminal_spawns: MessageWriter<TerminalStackSpawnRequest>,
    mut terminal_reinput: MessageWriter<TerminalReinputRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let blocked = blocked
        .read()
        .map(|blocked| (blocked.anchor, blocked.reason.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut split_this_batch = std::collections::HashSet::new();
    let mut pending_spawns = PendingRunTerminalSpawns::default();
    let mut queued_spawns = Vec::new();
    let mut regions = std::collections::HashMap::new();

    for request in requests.read() {
        let Some(decoded) = DecodedAgentRun::from_request(request) else {
            continue;
        };
        let run = &decoded.payload;
        if let Some(reason) = blocked.get(&run.anchor) {
            responses.write(
                AgentReply::new(request.request_id)
                    .response(AgentCommandResult::Error(reason.clone())),
            );
            continue;
        }
        let result = 'run: {
            let placement_override = decoded.placement_override
                || run.beside.is_some()
                || run.mode != PlacementMode::Auto
                || run.direction != AgentPaneDirection::Right;
            if let Err(error) = RunPlacementPolicy::new(placement_override).validate(&settings) {
                break 'run AgentCommandResult::Error(error.to_string());
            }
            let focus = request.origin.allows_focus(run.focus);
            let command = RunCommand::new(&run.command, run.done_marker.as_deref());
            if let Some(process_id) = run.terminal {
                break 'run match context.terminals.launch(process_id) {
                    Ok(launch) => {
                        terminal_reinput.write(command.reinput(
                            process_id,
                            &launch,
                            PagerEnv::Inherited,
                        ));
                        AgentCommandResult::Text(process_id.to_string())
                    }
                    Err(error) => AgentCommandResult::Error(error),
                };
            }
            let Some((agent_terminal, agent_pane, stored_region)) =
                context.resolve_pane(run.anchor)
            else {
                break 'run AgentCommandResult::Error("self process not found".to_string());
            };
            let tab_cwd = context.tab_cwd(agent_pane);
            let agent_cwd = context.agent_cwd(agent_terminal);
            let cwd = match AgentCwd::from_tab(tab_cwd.as_deref())
                .or_agent_launch(agent_cwd.as_deref())
            {
                Ok(cwd) => cwd,
                Err(message) => break 'run AgentCommandResult::Error(message),
            };
            let candidates = context
                .terminals
                .candidates(agent_pane, &context.panes, &cwd);
            let terminal_bucket_panes = context.panes.bucket_panes(agent_pane);
            if run.beside.is_none()
                && run.mode == PlacementMode::Auto
                && let Some(process_id) =
                    pending_spawns.append_input(run.anchor, &mut queued_spawns, &cwd, command)
            {
                break 'run AgentCommandResult::Text(process_id.to_string());
            }
            let region = regions
                .entry(run.anchor)
                .or_insert((agent_terminal, stored_region));
            if run.beside.is_none()
                && run.mode == PlacementMode::Auto
                && let Some(candidate) = region.1.choose_reusable_terminal(agent_pane, &candidates)
            {
                let Ok(launch) = context.terminals.launch(candidate.pid) else {
                    break 'run AgentCommandResult::Error(format!(
                        "run terminal launch not found: {}",
                        candidate.pid
                    ));
                };
                terminal_reinput.write(command.reinput(candidate.pid, &launch, PagerEnv::Set));
                region.1.run_terminal = Some(candidate.pid);
                region.1.run_pane = Some(candidate.pane);
                let sequence = context.next_pane_sequence.take();
                commands.entity(candidate.pane).insert(sequence);
                if focus {
                    for entity in context.panes.activation_entities(candidate) {
                        commands.entity(entity).insert(LastActivatedAt::now());
                    }
                }
                break 'run AgentCommandResult::Text(candidate.pid.to_string());
            }
            let beside_pane = match run.beside {
                Some(process_id) => match context.terminals.pane(process_id, &context.panes) {
                    Some(pane) => Some(pane),
                    None => {
                        break 'run AgentCommandResult::Error(format!(
                            "run.beside page not found: {process_id}"
                        ));
                    }
                },
                None => None,
            };
            let (shell, data) = command.for_new_terminal(&settings);
            if let Err(error) = shell.validate() {
                break 'run AgentCommandResult::Error(error);
            }
            let shell = shell.into_string();
            let target_pane = match (beside_pane, run.mode) {
                (anchor_pane, PlacementMode::Split) => {
                    let bucket_pane = if anchor_pane.is_none() {
                        region
                            .1
                            .choose_bucket_pane(agent_pane, &candidates)
                            .filter(|pane| terminal_bucket_panes.contains(*pane))
                            .or_else(|| terminal_bucket_panes.newest(agent_pane))
                    } else {
                        None
                    };
                    if let Some(pane) = bucket_pane {
                        pane
                    } else {
                        let anchor_pane = anchor_pane
                            .unwrap_or_else(|| context.panes.placement.split_anchor(agent_pane));
                        let split = context.panes.split(
                            anchor_pane,
                            &run.direction,
                            focus,
                            &mut split_this_batch,
                        );
                        context.panes.placement.tree.split_or_extend(
                            split.pane,
                            split.direction,
                            &split.existing_tabs,
                            split.focus,
                            split.already_split,
                        )
                    }
                }
                (Some(pane), _) => pane,
                (None, _) => context.panes.placement.resolve_spiral(
                    agent_pane,
                    crate::TerminalPlugin::URL,
                    focus,
                    &mut split_this_batch,
                ),
            };
            let sequence = context.next_pane_sequence.take();
            commands.entity(target_pane).insert(sequence);
            let process_id = ProcessId::new();
            let request_index = queued_spawns.len();
            queued_spawns.push(TerminalStackSpawnRequest {
                pane: target_pane,
                cwd: Some(cwd),
                shell: Some(shell.clone()),
                agent_run: true,
                pending_input: Some(data),
                process_id: Some(process_id),
                activate: focus,
            });
            region.1.run_pane = Some(target_pane);
            if run.beside.is_none() && run.mode != PlacementMode::Split {
                region.1.run_terminal = Some(process_id);
                pending_spawns.insert(
                    run.anchor,
                    PendingRunTerminalSpawn {
                        pid: process_id,
                        request_index,
                        shell,
                    },
                );
            }
            AgentCommandResult::Text(process_id.to_string())
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
    for spawn in queued_spawns {
        terminal_spawns.write(spawn);
    }
    for (_, (entity, region)) in regions {
        commands.entity(entity).insert(region);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};

    use bevy::ecs::system::RunSystemOnce;

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

    fn spawn_stack_in_pane(app: &mut App, pane: Entity, url: &str) -> Entity {
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: url.to_string(),
            ..default()
        });
        stack
    }

    #[test]
    fn agent_run_is_handled_by_the_terminal_feature() {
        let mut app = App::new();
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentRequestBlocked>()
            .add_message::<AgentCommandResponse>()
            .add_message::<TerminalStackSpawnRequest>()
            .add_message::<TerminalReinputRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, run_agent_commands);
        let process_id = ProcessId::new();
        app.world_mut().spawn(SpawnCounter::default());
        app.world_mut().spawn((
            Terminal,
            process_id,
            TerminalLaunch {
                command: "/bin/zsh".to_string(),
                args: Vec::new(),
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                env: Vec::new(),
            },
        ));
        app.update();
        let request_id = vmux_api::protocol::AgentRequestId::new();
        app.world_mut().write_message(AgentRequestInput {
            request_id,
            origin: vmux_ecs::agent::CommandOrigin::User,
            request: vmux_api::protocol::AgentRequest::encode(&AgentRun {
                anchor: ProcessId::new(),
                command: "pwd".to_string(),
                direction: AgentPaneDirection::Right,
                focus: false,
                beside: None,
                mode: PlacementMode::Auto,
                terminal: Some(process_id),
                done_marker: None,
            })
            .unwrap(),
        });

        app.update();

        let reinputs = app
            .world_mut()
            .resource_mut::<Messages<TerminalReinputRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(reinputs.len(), 1);
        assert_eq!(reinputs[0].process_id, process_id);
        assert!(String::from_utf8_lossy(&reinputs[0].data).contains("pwd"));
        let responses = app
            .world_mut()
            .resource_mut::<Messages<AgentCommandResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, request_id);
        assert_eq!(
            responses[0].result,
            AgentCommandResult::Text(process_id.to_string())
        );
    }

    #[test]
    fn run_terminal_cwd_prefers_tab_dir() {
        let tab_dir = std::env::temp_dir().join(format!("vmux-tab-cwd-{}", std::process::id()));
        let agent_dir = std::env::temp_dir().join(format!("vmux-agent-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&tab_dir).unwrap();
        std::fs::create_dir_all(&agent_dir).unwrap();
        let canonical_tab_dir = tab_dir.canonicalize().unwrap();
        assert_eq!(
            AgentCwd::from_tab(Some(tab_dir.to_string_lossy().as_ref()))
                .or_agent_launch(Some(agent_dir.to_string_lossy().as_ref()))
                .unwrap(),
            canonical_tab_dir
        );
        let _ = std::fs::remove_dir_all(&agent_dir);
        let _ = std::fs::remove_dir_all(&tab_dir);
    }

    #[test]
    fn projects_directory_is_created_and_contains_only_its_tree() {
        let root = tempfile::tempdir().unwrap();
        let projects = ProjectsDirectory::ensure_at(root.path().join("projects")).unwrap();

        assert!(projects.contains(&projects.path().join("github.com/vmux-ai/vmux")));
        assert!(!projects.contains(root.path()));
        assert!(projects.into_path().is_dir());
    }

    #[test]
    fn run_terminal_launch_must_match_rebound_cwd_for_reuse() {
        let current = std::env::temp_dir().join(format!("vmux-current-cwd-{}", std::process::id()));
        let stale = std::env::temp_dir().join(format!("vmux-stale-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&stale).unwrap();
        assert!(RunTerminalCandidate::launch_matches_cwd(
            current.to_string_lossy().as_ref(),
            &current,
        ));
        assert!(!RunTerminalCandidate::launch_matches_cwd(
            stale.to_string_lossy().as_ref(),
            &current,
        ));
        let _ = std::fs::remove_dir_all(&stale);
        let _ = std::fs::remove_dir_all(&current);
    }

    #[test]
    fn run_terminal_cwd_inherits_agent_launch_dir() {
        let dir = std::env::temp_dir().join(format!("vmux-run-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let got = AgentCwd::from_tab(None)
            .or_agent_launch(Some(&dir.to_string_lossy()))
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, dir);
    }

    #[test]
    fn run_terminal_cwd_requires_tab_or_agent_workspace() {
        assert!(AgentCwd::from_tab(None).or_agent_launch(Some("")).is_err());
        assert!(AgentCwd::from_tab(None).or_agent_launch(None).is_err());
    }

    #[test]
    fn run_terminal_cwd_rejects_invalid_stored_tab_directory() {
        let agent_dir = std::env::temp_dir();

        assert!(
            AgentCwd::from_tab(Some("/no/such/vmux-tab-workspace"))
                .or_agent_launch(agent_dir.to_str())
                .is_err()
        );
    }

    #[test]
    fn run_terminal_cwd_rejects_relative_stored_tab_directory() {
        assert!(AgentCwd::from_tab(Some(".")).or_agent_launch(None).is_err());
    }

    #[test]
    fn only_the_first_command_in_a_terminal_sets_the_pager() {
        let primed = command_with_marker("/opt/homebrew/bin/nu", "ls", "abc", PagerEnv::Set);
        let later = command_with_marker("/opt/homebrew/bin/nu", "ls", "abc", PagerEnv::Inherited);

        assert!(primed.starts_with("$env.GIT_PAGER"), "got: {primed}");
        assert!(
            later.starts_with("$env.LAST_EXIT_CODE = 0; let __vmux_status = try { ls;"),
            "the shell keeps its environment between commands, so repeating the assignment only \
             buries the command the reader is looking at: {later}"
        );
        assert!(later.contains("]6973;abc;"), "got: {later}");
    }

    #[test]
    fn command_with_marker_is_shell_aware() {
        assert_eq!(
            command_with_marker("/opt/homebrew/bin/nu", "ls", "abc", PagerEnv::Set),
            "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; $env.LAST_EXIT_CODE = 0; let __vmux_status = try { ls; $env.LAST_EXIT_CODE } catch {|error| $error.exit_code? | default 1 }; print -rn $\"\\u{1b}]6973;abc;($__vmux_status)\\u{7}\""
        );
        assert_eq!(
            command_with_marker("/usr/local/bin/fish", "ls", "abc", PagerEnv::Set),
            "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ls; set __vmux_status $status; printf '\\033]6973;abc;%s\\007' $__vmux_status"
        );
        assert_eq!(
            command_with_marker("/bin/zsh", "ls", "abc", PagerEnv::Set),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
        assert_eq!(
            command_with_marker("/usr/bin/xonsh", "ls", "abc", PagerEnv::Set),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
    }

    #[test]
    fn run_command_line_noop_when_token_absent() {
        assert_eq!(
            RunCommand::new("ls -la", None).line("/bin/zsh", PagerEnv::Set),
            "ls -la"
        );
    }

    #[test]
    fn run_command_line_embeds_marker_when_token_present() {
        let out = RunCommand::new("ls -la", Some("tok9")).line("/bin/zsh", PagerEnv::Set);
        assert!(out.contains("ls -la"), "got: {out}");
        assert!(out.contains("]6973;tok9;"), "got: {out}");
        assert!(
            !out.contains("__VMUX_DONE_"),
            "marker must be invisible: {out}"
        );
    }

    #[test]
    fn new_agent_run_terminal_uses_configured_shell_for_launch_and_input() {
        let mut settings = test_settings();
        settings.terminal = Some(TerminalSettings {
            default_theme: "default".to_string(),
            themes: vec![TerminalTheme {
                name: "default".to_string(),
                color_scheme: "catppuccin-mocha".to_string(),
                font_family: "JetBrainsMono Nerd Font".to_string(),
                font_size: 14.0,
                line_height: 1.2,
                padding: 4.0,
                cursor_style: CursorStyle::Block,
                cursor_blink: true,
                shell: "/opt/homebrew/bin/nu".to_string(),
            }],
            ..Default::default()
        });

        let (shell, input) = RunCommand::new("cd /tmp", Some("tok9")).for_new_terminal(&settings);

        assert_eq!(shell.as_str(), "/opt/homebrew/bin/nu");
        let input = String::from_utf8(input).unwrap();
        assert!(
            input.contains("cd /tmp; $env.LAST_EXIT_CODE"),
            "got: {input}"
        );
        assert!(input.contains("]6973;tok9;"), "got: {input}");
        assert!(input.ends_with('\r'));
        assert_eq!(input.matches('\r').count(), 1);
        assert!(!input.contains("export GIT_PAGER"), "got: {input}");
    }

    #[test]
    fn nushell_marker_shares_one_submission_with_a_stdin_command() {
        let line = command_with_marker("/opt/homebrew/bin/nu", "input", "tok", PagerEnv::Inherited);

        assert_eq!(line.matches('\r').count(), 0);
        assert!(line.contains("try { input; $env.LAST_EXIT_CODE }"));
        assert!(line.ends_with("($__vmux_status)\\u{7}\""));
    }

    #[test]
    fn new_agent_run_terminal_rejects_missing_configured_shell() {
        let shell = "/definitely/missing/vmux-terminal-shell";

        assert_eq!(
            AgentTerminalShell(shell.to_string()).validate(),
            Err(format!(
                "terminal shell not found or not executable: {shell}"
            ))
        );
    }

    #[test]
    fn existing_agent_run_terminal_uses_launch_shell_for_input() {
        let launch = TerminalLaunch {
            command: "/usr/local/bin/fish".to_string(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
        };

        let input = RunCommand::new("pwd", Some("tok2")).input(&launch.command, PagerEnv::Set);
        let input = String::from_utf8(input).unwrap();

        assert!(input.contains("set __vmux_status $status"), "got: {input}");
        assert!(input.contains("]6973;tok2;"), "got: {input}");
        assert!(input.ends_with('\r'));
    }

    #[test]
    fn explicit_run_terminal_errors_distinguish_missing_page_and_launch() {
        let mut app = App::new();
        let terminal_pid = ProcessId::new();
        let missing_pid = ProcessId::new();
        app.world_mut().spawn((Terminal, terminal_pid));

        let (missing_page, missing_launch) = app
            .world_mut()
            .run_system_once(move |terminals: RunTerminals| {
                (
                    terminals.launch(missing_pid).unwrap_err(),
                    terminals.launch(terminal_pid).unwrap_err(),
                )
            })
            .unwrap();

        assert_eq!(
            missing_page,
            format!("run.terminal page not found: {missing_pid}")
        );
        assert_eq!(
            missing_launch,
            format!("run terminal launch not found: {terminal_pid}")
        );
    }

    #[test]
    fn existing_agent_run_terminal_routes_input_through_terminal_queue() {
        #[derive(Resource)]
        struct Input {
            process_id: ProcessId,
            launch: TerminalLaunch,
        }

        #[derive(Resource, Default)]
        struct Captured(Vec<TerminalReinputRequest>);

        fn emit(input: Res<Input>, mut writer: MessageWriter<TerminalReinputRequest>) {
            writer.write(RunCommand::new("pwd", Some("tok4")).reinput(
                input.process_id,
                &input.launch,
                PagerEnv::Inherited,
            ));
        }

        fn capture(
            mut reader: MessageReader<TerminalReinputRequest>,
            mut captured: ResMut<Captured>,
        ) {
            captured.0.extend(reader.read().cloned());
        }

        let process_id = ProcessId::new();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TerminalContractPlugin))
            .insert_resource(Input {
                process_id,
                launch: TerminalLaunch {
                    command: "/usr/local/bin/fish".to_string(),
                    args: vec![],
                    cwd: String::new(),
                    env: vec![],
                },
            })
            .init_resource::<Captured>()
            .add_systems(Update, (emit, capture).chain());

        app.update();

        let captured = &app.world().resource::<Captured>().0;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].process_id, process_id);
        let input = String::from_utf8(captured[0].data.clone()).unwrap();

        assert!(input.contains("set __vmux_status $status"), "got: {input}");
        assert!(input.contains("]6973;tok4;"), "got: {input}");
        assert!(input.ends_with('\r'));
    }

    #[derive(Resource)]
    struct RunTerminalCandidateInput {
        agent_pane: Entity,
        desired_cwd: PathBuf,
    }

    #[derive(Resource, Default)]
    struct RunTerminalCandidateOutput(Vec<RunTerminalCandidate>);

    fn collect_run_candidates(
        input: Res<RunTerminalCandidateInput>,
        terminals: RunTerminals,
        panes: AgentPanes,
        mut out: ResMut<RunTerminalCandidateOutput>,
    ) {
        out.0 = terminals.candidates(input.agent_pane, &panes, &input.desired_cwd);
    }

    #[test]
    fn run_terminal_candidates_fail_closed_when_agent_tab_missing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_candidates);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let terminal_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(7), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(terminal_pane)))
            .id();
        let desired_cwd = std::env::temp_dir();
        app.world_mut().spawn((
            Terminal,
            ProcessId::new(),
            AgentRunTerminal,
            TerminalLaunch {
                command: "/bin/zsh".to_string(),
                args: vec![],
                cwd: desired_cwd.to_string_lossy().into_owned(),
                env: vec![],
            },
            ChildOf(stack),
        ));
        let agent_pane = app.world_mut().spawn((Pane, SpawnSeq(9))).id();

        app.insert_resource(RunTerminalCandidateInput {
            agent_pane,
            desired_cwd,
        });
        app.update();

        assert!(
            app.world()
                .resource::<RunTerminalCandidateOutput>()
                .0
                .is_empty(),
            "unresolved agent tab must not match terminals from other tabs"
        );
    }

    #[test]
    fn run_terminal_candidates_require_agent_run_marker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_candidates);
        let tab = app.world_mut().spawn(Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(1), ChildOf(tab)))
            .id();
        let desired_cwd = std::env::temp_dir();
        let agent_pid = ProcessId::new();
        let user_pid = ProcessId::new();
        let mut agent_terminal = None;
        for (sequence, pid, agent_run) in [(2, agent_pid, true), (3, user_pid, false)] {
            let pane = app
                .world_mut()
                .spawn((Pane, SpawnSeq(sequence), ChildOf(tab)))
                .id();
            let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
            let terminal = app
                .world_mut()
                .spawn((
                    Terminal,
                    pid,
                    TerminalLaunch {
                        command: "/bin/zsh".to_string(),
                        args: vec![],
                        cwd: desired_cwd.to_string_lossy().into_owned(),
                        env: vec![],
                    },
                    ChildOf(stack),
                ))
                .id();
            if agent_run {
                app.world_mut()
                    .entity_mut(terminal)
                    .insert(AgentRunTerminal);
                agent_terminal = Some(terminal);
            }
        }

        app.insert_resource(RunTerminalCandidateInput {
            agent_pane,
            desired_cwd,
        });
        app.update();

        let candidates = &app.world().resource::<RunTerminalCandidateOutput>().0;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].pid, agent_pid);
        assert_eq!(candidates[0].terminal, agent_terminal.unwrap());
    }

    #[test]
    fn run_terminal_candidates_exclude_stale_launch_cwd() {
        let current =
            std::env::temp_dir().join(format!("vmux-current-candidate-{}", std::process::id()));
        let stale =
            std::env::temp_dir().join(format!("vmux-stale-candidate-{}", std::process::id()));
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&stale).unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_candidates);
        let tab = app.world_mut().spawn(Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(1), ChildOf(tab)))
            .id();
        let current_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(2), ChildOf(tab)))
            .id();
        let current_stack = app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(current_pane)))
            .id();
        let current_pid = ProcessId::new();
        app.world_mut().spawn((
            Terminal,
            current_pid,
            AgentRunTerminal,
            TerminalLaunch {
                command: "/bin/zsh".into(),
                args: vec![],
                cwd: current.to_string_lossy().into_owned(),
                env: vec![],
            },
            ChildOf(current_stack),
        ));
        let stale_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(3), ChildOf(tab)))
            .id();
        let stale_stack = app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(stale_pane)))
            .id();
        app.world_mut().spawn((
            Terminal,
            ProcessId::new(),
            AgentRunTerminal,
            TerminalLaunch {
                command: "/bin/zsh".into(),
                args: vec![],
                cwd: stale.to_string_lossy().into_owned(),
                env: vec![],
            },
            ChildOf(stale_stack),
        ));
        app.insert_resource(RunTerminalCandidateInput {
            agent_pane,
            desired_cwd: current.clone(),
        });
        app.update();

        let candidates = &app.world().resource::<RunTerminalCandidateOutput>().0;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].pid, current_pid);
        let _ = std::fs::remove_dir_all(&stale);
        let _ = std::fs::remove_dir_all(&current);
    }

    #[derive(Resource)]
    struct RunTerminalBucketPaneInput {
        agent_pane: Entity,
    }

    #[derive(Resource, Default)]
    struct RunTerminalBucketPaneOutput(Vec<Entity>);

    fn collect_run_bucket_panes(
        input: Res<RunTerminalBucketPaneInput>,
        panes: AgentPanes,
        mut out: ResMut<RunTerminalBucketPaneOutput>,
    ) {
        out.0 = panes
            .bucket_panes(input.agent_pane)
            .0
            .into_iter()
            .map(|candidate| candidate.pane)
            .collect();
    }

    #[test]
    fn run_terminal_bucket_panes_include_pure_terminal_layout_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalBucketPaneOutput>()
            .add_systems(Update, collect_run_bucket_panes);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(1), ChildOf(tab)))
            .id();
        let terminal_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(3), ChildOf(tab)))
            .id();
        spawn_stack_in_pane(&mut app, terminal_pane, "vmux://terminal/68001");
        let file_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(9), ChildOf(tab)))
            .id();
        spawn_stack_in_pane(&mut app, file_pane, "file:///repo/src/plugin.rs");

        app.insert_resource(RunTerminalBucketPaneInput { agent_pane });
        app.update();

        assert_eq!(
            app.world().resource::<RunTerminalBucketPaneOutput>().0,
            vec![terminal_pane]
        );
    }

    #[test]
    fn pending_run_terminal_spawn_uses_selected_shell() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let pane = Entity::from_bits(20);
        let mut pending_spawns = PendingRunTerminalSpawns::default();
        pending_spawns.insert(
            anchor,
            PendingRunTerminalSpawn {
                pid: terminal,
                request_index: 0,
                shell: "/opt/homebrew/bin/nu".to_string(),
            },
        );
        let mut terminal_spawns = vec![TerminalStackSpawnRequest {
            pane,
            cwd: Some(std::env::temp_dir()),
            shell: Some("/opt/homebrew/bin/nu".to_string()),
            agent_run: true,
            pending_input: Some(b"one\r".to_vec()),
            process_id: Some(terminal),
            activate: false,
        }];

        let picked = pending_spawns.append_input(
            anchor,
            &mut terminal_spawns,
            &std::env::temp_dir(),
            RunCommand::new("pwd", Some("tok2")),
        );

        assert_eq!(picked, Some(terminal));
        let input = String::from_utf8(terminal_spawns[0].pending_input.clone().unwrap()).unwrap();
        assert!(
            input.starts_with("one\r$env.LAST_EXIT_CODE = 0; let __vmux_status"),
            "got: {input}"
        );
        assert_eq!(input.matches('\r').count(), 2);
        assert!(input.contains("print -rn"), "got: {input}");
        assert!(input.contains("]6973;tok2;"), "got: {input}");
        assert_eq!(terminal_spawns.len(), 1);
    }

    #[test]
    fn pending_run_terminal_spawn_rejects_changed_cwd() {
        let old_cwd = std::env::temp_dir().join(format!("vmux-old-cwd-{}", std::process::id()));
        let new_cwd = std::env::temp_dir().join(format!("vmux-new-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&old_cwd).unwrap();
        std::fs::create_dir_all(&new_cwd).unwrap();
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let mut pending_spawns = PendingRunTerminalSpawns::default();
        pending_spawns.insert(
            anchor,
            PendingRunTerminalSpawn {
                pid: terminal,
                request_index: 0,
                shell: "/opt/homebrew/bin/nu".to_string(),
            },
        );
        let mut terminal_spawns = vec![TerminalStackSpawnRequest {
            pane: Entity::from_bits(20),
            cwd: Some(old_cwd.clone()),
            shell: Some("/opt/homebrew/bin/nu".to_string()),
            agent_run: true,
            pending_input: Some(b"one\r".to_vec()),
            process_id: Some(terminal),
            activate: false,
        }];

        let picked = pending_spawns.append_input(
            anchor,
            &mut terminal_spawns,
            &new_cwd,
            RunCommand::new("pwd", Some("tok2")),
        );

        let _ = std::fs::remove_dir_all(&old_cwd);
        let _ = std::fs::remove_dir_all(&new_cwd);
        assert_eq!(picked, None);
        assert_eq!(
            terminal_spawns[0].pending_input.as_deref(),
            Some(&b"one\r"[..])
        );
    }

    #[derive(Resource)]
    struct ReusedRunPaneTouchInput {
        pane: Entity,
    }

    fn touch_reused_run_pane_spawn_seq(
        input: Res<ReusedRunPaneTouchInput>,
        mut commands: Commands,
        mut next_sequence: NextPaneSpawnSequence,
    ) {
        let sequence = next_sequence.take();
        commands.entity(input.pane).insert(sequence);
    }

    #[test]
    fn reusable_run_pane_touch_refreshes_spawn_seq() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .add_systems(Update, touch_reused_run_pane_spawn_seq);

        let reused = app.world_mut().spawn((Pane, SpawnSeq(2))).id();
        app.world_mut().spawn(SpawnCounter::default());
        app.world_mut().spawn((Pane, SpawnSeq(10)));
        app.insert_resource(ReusedRunPaneTouchInput { pane: reused });
        app.update();

        assert_eq!(app.world().get::<SpawnSeq>(reused).unwrap().0, 11);
    }

    #[derive(Resource)]
    struct SplitRunPaneInput {
        pane: Entity,
    }

    #[derive(Resource, Default)]
    struct SplitRunPaneOutput(Option<Entity>);

    fn split_run_pane(
        input: Res<SplitRunPaneInput>,
        mut out: ResMut<SplitRunPaneOutput>,
        mut commands: Commands,
        mut panes: AgentPanes,
        mut next_sequence: NextPaneSpawnSequence,
    ) {
        let mut split_batch = std::collections::HashSet::new();
        let split = panes.split(
            input.pane,
            &AgentPaneDirection::Bottom,
            false,
            &mut split_batch,
        );
        let target = panes.placement.tree.split_or_extend(
            split.pane,
            split.direction,
            &split.existing_tabs,
            split.focus,
            split.already_split,
        );
        let sequence = next_sequence.take();
        commands.entity(target).insert(sequence);
        out.0 = Some(target);
    }

    #[test]
    fn split_run_pane_becomes_newest_for_followup_placement() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .init_resource::<SplitRunPaneOutput>()
            .add_systems(Update, split_run_pane);

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let browser_pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(10), ChildOf(tab)))
            .id();
        app.world_mut().spawn(SpawnCounter::default());
        let browser_stack = app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(browser_pane)))
            .id();
        app.world_mut()
            .entity_mut(browser_stack)
            .insert(PageMetadata {
                url: "https://news.ycombinator.com".into(),
                ..default()
            });
        app.insert_resource(SplitRunPaneInput { pane: browser_pane });

        app.update();

        let terminal_pane = app.world().resource::<SplitRunPaneOutput>().0.unwrap();
        let seq = app
            .world()
            .get::<SpawnSeq>(terminal_pane)
            .expect("split run target gets fresh spawn seq")
            .0;
        assert!(seq > 10, "split run target must become newest");
    }

    #[test]
    fn run_reuses_existing_terminal_when_region_cache_is_empty() {
        let terminal = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let region = AgentTerminalRegion::default();
        let candidates = [RunTerminalCandidate {
            terminal: Entity::from_bits(19),
            pid: terminal,
            stack: Entity::from_bits(21),
            pane: terminal_pane,
            pane_spawn_seq: 7,
        }];

        let picked = region
            .choose_reusable_terminal(agent_pane, &candidates)
            .unwrap();

        assert_eq!(picked.pid, terminal);
        assert_eq!(picked.pane, terminal_pane);
    }

    #[test]
    fn run_placement_policy_rejects_override_by_default() {
        let settings = test_settings();
        assert_eq!(
            RunPlacementPolicy::new(true).validate(&settings),
            Err("run placement overrides are disabled; omit mode, direction, and beside and retry")
        );
    }

    #[test]
    fn run_placement_policy_allows_bare_run() {
        let settings = test_settings();
        assert_eq!(RunPlacementPolicy::new(false).validate(&settings), Ok(()));
    }

    #[test]
    fn run_placement_policy_honors_user_opt_out() {
        let mut settings = test_settings();
        settings.agent.allow_run_placement_override = true;
        assert_eq!(RunPlacementPolicy::new(true).validate(&settings), Ok(()));
    }

    #[test]
    fn run_reuses_cached_terminal_before_newer_terminal_candidates() {
        let cached = ProcessId::new();
        let newer = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let cached_pane = Entity::from_bits(20);
        let newer_pane = Entity::from_bits(30);
        let region = AgentTerminalRegion {
            run_terminal: Some(cached),
            run_pane: Some(cached_pane),
        };
        let candidates = [
            RunTerminalCandidate {
                terminal: Entity::from_bits(19),
                pid: cached,
                stack: Entity::from_bits(21),
                pane: cached_pane,
                pane_spawn_seq: 3,
            },
            RunTerminalCandidate {
                terminal: Entity::from_bits(29),
                pid: newer,
                stack: Entity::from_bits(31),
                pane: newer_pane,
                pane_spawn_seq: 9,
            },
        ];

        let picked = region
            .choose_reusable_terminal(agent_pane, &candidates)
            .unwrap();

        assert_eq!(picked.pid, cached);
        assert_eq!(picked.pane, cached_pane);
    }

    #[derive(Resource)]
    struct ReusedRunTerminalFocusInput {
        candidate: RunTerminalCandidate,
    }

    fn focus_reused_run(
        input: Res<ReusedRunTerminalFocusInput>,
        mut commands: Commands,
        panes: AgentPanes,
    ) {
        for entity in panes.activation_entities(input.candidate) {
            commands.entity(entity).insert(LastActivatedAt::now());
        }
    }

    #[test]
    fn reused_run_terminal_focus_activates_stack_pane_and_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, focus_reused_run);
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, SpawnSeq(7), LastActivatedAt(2), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt(3), ChildOf(pane)))
            .id();
        app.insert_resource(ReusedRunTerminalFocusInput {
            candidate: RunTerminalCandidate {
                terminal: Entity::from_bits(4),
                pid: ProcessId::new(),
                stack,
                pane,
                pane_spawn_seq: 7,
            },
        });

        app.update();

        assert!(app.world().get::<LastActivatedAt>(tab).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(pane).unwrap().0 > 2);
        assert!(app.world().get::<LastActivatedAt>(stack).unwrap().0 > 3);
    }

    #[test]
    fn split_run_stacks_into_cached_terminal_bucket_pane() {
        let terminal = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let region = AgentTerminalRegion {
            run_pane: Some(terminal_pane),
            ..default()
        };
        let candidates = [RunTerminalCandidate {
            terminal: Entity::from_bits(19),
            pid: terminal,
            stack: Entity::from_bits(21),
            pane: terminal_pane,
            pane_spawn_seq: 7,
        }];

        assert_eq!(
            region.choose_bucket_pane(agent_pane, &candidates),
            Some(terminal_pane)
        );
    }

    #[test]
    fn split_run_keeps_cached_terminal_bucket_after_process_exits() {
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let region = AgentTerminalRegion {
            run_pane: Some(terminal_pane),
            ..default()
        };
        let candidates = [];

        assert_eq!(
            region.choose_bucket_pane(agent_pane, &candidates),
            Some(terminal_pane)
        );
    }
}
