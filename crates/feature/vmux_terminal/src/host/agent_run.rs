use std::path::Path;
use vmux_ecs::agent::AgentRequestReady;

use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_api::protocol::{AgentCommandResult, ProcessId};
use vmux_api::service::RUN_OSC;
#[cfg(test)]
use vmux_api::terminal::CursorStyle;
use vmux_ecs::LastActivatedAt;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentReply, AgentRequestApplySet, AgentRequestBlocked, AgentRequestInput,
};
#[cfg(test)]
use vmux_ecs::profile::ProjectsDirectory;
use vmux_ecs::{PageMetadata, UnixMillis};
use vmux_layout::AgentPaneDirection;
#[cfg(test)]
use vmux_layout::LayoutContractPlugin;
use vmux_layout::pane::Pane;
use vmux_layout::pane::{PaneSplit, SpawnCounter, SpawnSeq};
use vmux_layout::placement::{
    PlaceStackRequest, PlacementCommitSet, PlacementPlugin, PlacementStarting, StackPlaced,
};
use vmux_layout::stack::Stack;
use vmux_layout::tab::Tab;
use vmux_setting::AppSettings;
#[cfg(test)]
use vmux_setting::{TerminalSettings, TerminalTheme};

#[cfg(test)]
use crate::TerminalContractPlugin;
use crate::launch::TerminalLaunch;
use crate::{
    AgentRunTerminal, ProcessExited, Terminal, TerminalReinputRequest, TerminalStackSpawnRequest,
};
use vmux_space::WorkspaceCwd;

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
        if !app.is_plugin_added::<vmux_ecs::agent::AgentRequestDispatchPlugin>() {
            app.add_plugins(vmux_ecs::agent::AgentRequestDispatchPlugin);
        }
        if !app.is_plugin_added::<PlacementPlugin>() {
            app.add_plugins(PlacementPlugin);
        }
        app.add_observer(allocate_run)
            .add_observer(validate_owner)
            .add_observer(prepare_run)
            .add_observer(placed)
            .add_message::<AgentRequestInput>()
            .add_message::<AgentRequestBlocked>()
            .configure_sets(
                Update,
                PlacementCommitSet.before(super::TerminalStackSpawnSet),
            )
            .add_systems(
                Update,
                (initialize_regions, ApplyDeferred, run_agent_commands)
                    .chain()
                    .in_set(AgentRequestApplySet)
                    .before(PlacementCommitSet)
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
        let Some(launch_cwd) = WorkspaceCwd::try_from(launch_cwd)
            .ok()
            .and_then(WorkspaceCwd::into_path)
        else {
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
                let stack = panes.parents.get(terminal).ok()?.get();
                let pane = panes.parents.get(stack).ok()?.get();
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
                    pane_spawn_seq: panes.sequences.get(pane).map(|s| s.0).unwrap_or(0),
                })
            })
            .collect()
    }

    fn pane(&self, process_id: ProcessId, panes: &AgentPanes) -> Option<Entity> {
        let (terminal, _) = self
            .terminals
            .iter()
            .find(|(_, candidate)| **candidate == process_id)?;
        let stack = panes.parents.get(terminal).ok()?.get();
        panes.parents.get(stack).ok().map(Relationship::get)
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
    parents: Query<'w, 's, &'static ChildOf>,
    tabs: Query<'w, 's, (), With<Tab>>,
    children: Query<'w, 's, &'static Children>,
    leaves: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<'w, 's, (), With<Stack>>,
    metadata: Query<'w, 's, &'static PageMetadata, With<Stack>>,
    sequences: Query<'w, 's, &'static SpawnSeq>,
    placements: vmux_ecs::page::PagePlacementCatalog<'w, 's>,
}

impl AgentPanes<'_, '_> {
    fn tab(&self, pane: Entity) -> Option<Entity> {
        let mut current = pane;
        for _ in 0..128 {
            if self.tabs.contains(current) {
                return Some(current);
            }
            current = self.parents.get(current).ok()?.parent();
        }
        None
    }

    fn activation_entities(&self, candidate: RunTerminalCandidate) -> Vec<Entity> {
        let mut entities = vec![candidate.stack, candidate.pane];
        if let Some(tab) = self.tab(candidate.pane) {
            entities.push(tab);
        }
        entities
    }

    fn bucket(&self, agent_pane: Entity, pane: Entity) -> bool {
        if agent_pane == pane
            || !self.leaves.contains(pane)
            || self.tab(pane) != self.tab(agent_pane)
        {
            return false;
        }
        let Ok(children) = self.children.get(pane) else {
            return false;
        };
        let mut found = false;
        for stack in children {
            if !self.stacks.contains(*stack) {
                continue;
            }
            found = true;
            let Ok(page) = self.metadata.get(*stack) else {
                return false;
            };
            if !self
                .placements
                .same_group(&page.url, crate::TerminalPlugin::url())
            {
                return false;
            }
        }
        found
    }

    fn newest_bucket(&self, agent_pane: Entity) -> Option<Entity> {
        let mut newest = None;
        for pane in &self.leaves {
            if !self.bucket(agent_pane, pane) {
                continue;
            }
            let sequence = self.sequences.get(pane).map_or(0, |sequence| sequence.0);
            if newest.is_none_or(|(_, old)| sequence > old) {
                newest = Some((pane, sequence));
            }
        }
        newest.map(|(pane, _)| pane)
    }
}

impl TerminalStackSpawnRequest {
    fn append_input(
        &mut self,
        desired_cwd: &Path,
        command: &str,
        marker: Option<&str>,
    ) -> Option<ProcessId> {
        let process_id = self.process_id?;
        let shell = self.shell.as_deref()?;
        let request_cwd = self.cwd.as_deref()?.canonicalize().ok()?;
        let desired_cwd = desired_cwd.canonicalize().ok()?;
        if request_cwd != desired_cwd {
            return None;
        }
        let data = Self::format_input(shell, command, marker, false);
        match &mut self.pending_input {
            Some(input) => input.extend(data),
            None => self.pending_input = Some(data),
        }
        Some(process_id)
    }

    fn pager_prefix(base: &str, set_pager: bool) -> &'static str {
        if !set_pager {
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

    fn command_with_marker(shell: &str, command: &str, token: &str, set_pager: bool) -> String {
        let base = Path::new(shell)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(shell);
        let pager = Self::pager_prefix(base, set_pager);
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

    fn format_line(shell: &str, command: &str, token: Option<&str>, set_pager: bool) -> String {
        match token {
            Some(token) => Self::command_with_marker(shell, command, token, set_pager),
            None => command.to_string(),
        }
    }

    fn format_input(shell: &str, command: &str, token: Option<&str>, set_pager: bool) -> Vec<u8> {
        let mut data = Self::format_line(shell, command, token, set_pager).into_bytes();
        data.push(b'\r');
        data
    }

    fn reinput(
        process_id: ProcessId,
        launch: &TerminalLaunch,
        command: &str,
        token: Option<&str>,
        set_pager: bool,
    ) -> TerminalReinputRequest {
        TerminalReinputRequest {
            process_id,
            data: Self::format_input(&launch.command, command, token, set_pager),
        }
    }

    fn for_new_terminal(
        settings: &AppSettings,
        command: &str,
        token: Option<&str>,
    ) -> (String, Vec<u8>) {
        let shell = settings.shell();
        let input = Self::format_input(&shell, command, token, true);
        (shell, input)
    }

    fn validate_shell(shell: &str) -> Result<(), String> {
        if vmux_path::Executable::find(shell).is_some() {
            Ok(())
        } else {
            Err(format!(
                "terminal shell not found or not executable: {shell}"
            ))
        }
    }
}

impl AgentRun {
    const OVERRIDE_DISABLED: &'static str =
        "run placement overrides are disabled; omit mode, direction, and beside and retry";

    fn validate_placement(
        &self,
        placement_override: bool,
        settings: &AppSettings,
    ) -> Result<(), &'static str> {
        let requested_override = placement_override
            || self.beside.is_some()
            || self.mode != PlacementMode::Auto
            || self.direction != AgentPaneDirection::Right;
        if requested_override && !settings.agent.allow_run_placement_override {
            Err(Self::OVERRIDE_DISABLED)
        } else {
            Ok(())
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AgentRunContext<'w, 's> {
    agent_terminals: Query<'w, 's, (Entity, &'static ProcessId, &'static ChildOf)>,
    terminals: RunTerminals<'w, 's>,
    session_targets: Query<'w, 's, &'static vmux_ecs::EntityTarget<vmux_session::Session>>,
    sessions: Query<'w, 's, &'static vmux_ecs::Cwd, With<vmux_session::Session>>,
    panes: AgentPanes<'w, 's>,
    tabs: Query<'w, 's, &'static Tab>,
}

impl AgentRunContext<'_, '_> {
    fn resolve_pane(&self, anchor: ProcessId) -> Option<(Entity, Entity)> {
        let (terminal, _, terminal_parent) = self
            .agent_terminals
            .iter()
            .find(|(_, process_id, _)| **process_id == anchor)?;
        let stack = terminal_parent.get();
        let pane = self.panes.parents.get(stack).ok()?.get();
        Some((terminal, pane))
    }

    fn tab_cwd(&self, pane: Entity) -> Option<String> {
        let mut current = pane;
        loop {
            if let Ok(tab) = self.tabs.get(current) {
                return tab.startup_dir.clone();
            }
            current = self.panes.parents.get(current).ok()?.parent();
        }
    }

    fn agent_cwd(&self, terminal: Entity) -> Option<String> {
        self.terminals.cwd(terminal).or_else(|| {
            let mut current = terminal;
            loop {
                if let Ok(target) = self.session_targets.get(current)
                    && let Ok(cwd) = self.sessions.get(target.entity())
                {
                    return Some(cwd.0.to_string_lossy().into_owned());
                }
                current = self.panes.parents.get(current).ok()?.parent();
            }
        })
    }
}

#[derive(Component)]
struct ReceivedRun(AgentRequestReady);

impl ReceivedRun {
    fn decode_run(&self) -> Option<(AgentRun, bool)> {
        if let Ok(Some(payload)) = self.0.decode::<AgentRun>() {
            return Some((payload, false));
        }
        let payload = self
            .0
            .decode::<AgentRunWithPlacementOverride>()
            .ok()
            .flatten()?;
        Some((payload.0, true))
    }
}

#[derive(Component)]
struct BlockedRun(String);

#[derive(Component)]
struct PendingAgentPlacement {
    reply: AgentReply,
    owner: Entity,
    anchor: ProcessId,
    source_stack: Entity,
    source_pane: Entity,
    stack: Entity,
    request: TerminalStackSpawnRequest,
    update_latest: bool,
}

impl PendingAgentPlacement {
    fn append(
        &mut self,
        owner: Entity,
        run: &AgentRun,
        cwd: &Path,
        state: &vmux_ecs::OperationState,
    ) -> bool {
        if self.owner != owner
            || self.anchor != run.anchor
            || !self.update_latest
            || !matches!(
                state,
                vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
            )
        {
            return false;
        }
        self.request
            .append_input(cwd, &run.command, run.done_marker.as_deref())
            .is_some()
    }
}

#[derive(Component)]
struct PendingRunReply {
    placement: Entity,
    reply: AgentReply,
}

#[derive(EntityEvent)]
struct PrepareRun {
    #[event_target]
    operation: Entity,
}

fn initialize_regions(
    owners: Query<Entity, (With<ProcessId>, With<ChildOf>, Without<AgentTerminalRegion>)>,
    mut commands: Commands,
) {
    for owner in &owners {
        commands
            .entity(owner)
            .insert(AgentTerminalRegion::default());
    }
}

fn run_agent_commands(
    world: &mut World,
    inputs: &mut SystemState<(
        MessageReader<AgentRequestReady>,
        MessageReader<AgentRequestBlocked>,
    )>,
) {
    let (requests, blocked) = {
        let (mut requests, mut blocked) = inputs.get_mut(world).unwrap();
        (
            requests.read().cloned().collect::<Vec<_>>(),
            blocked.read().cloned().collect::<Vec<_>>(),
        )
    };
    for request in requests {
        let operation = world
            .spawn((
                Name::new("Agent terminal placement"),
                vmux_ecs::Operation::bundle("terminal.agent.placement")
                    .expect("fixed agent placement operation"),
                vmux_ecs::OperationRetained,
                ReceivedRun(request),
            ))
            .id();
        if let Some((run, _)) = world.get::<ReceivedRun>(operation).unwrap().decode_run()
            && let Some(reason) = blocked.iter().find(|reason| reason.anchor == run.anchor)
        {
            world
                .entity_mut(operation)
                .insert(BlockedRun(reason.reason.clone()));
        }
        world.trigger(PrepareRun { operation });
        world.flush();
    }
}

fn prepare_run(
    event: On<PrepareRun>,
    requests: Query<(&ReceivedRun, Option<&BlockedRun>)>,
    context: AgentRunContext,
    settings: Res<AppSettings>,
    mut regions: Query<&mut AgentTerminalRegion>,
    mut pending: Query<(
        Entity,
        &mut PendingAgentPlacement,
        &vmux_ecs::OperationState,
    )>,
    mut sequences: NextPaneSpawnSequence,
    mut commands: Commands,
    mut terminal_reinput: MessageWriter<TerminalReinputRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let operation = event.operation;
    let Ok((received, blocked)) = requests.get(operation) else {
        return;
    };
    let request = &received.0;
    let Some((run, placement_override)) = received.decode_run() else {
        commands.entity(operation).despawn();
        return;
    };
    let reply = AgentReply::new(request.request_id);
    let result = 'run: {
        if let Some(reason) = blocked {
            break 'run AgentCommandResult::Error(reason.0.clone());
        }
        if let Err(error) = run.validate_placement(placement_override, &settings) {
            break 'run AgentCommandResult::Error(error.into());
        }
        let focus = request.origin.allows_focus(run.focus);
        if let Some(process_id) = run.terminal {
            break 'run reinput(&run, process_id, &context.terminals, &mut terminal_reinput);
        }
        let Some((owner, pane)) = context.resolve_pane(run.anchor) else {
            break 'run AgentCommandResult::Error("self process not found".into());
        };
        let cwd = match WorkspaceCwd::resolve(
            context.tab_cwd(pane).as_deref(),
            context.agent_cwd(owner).as_deref(),
        ) {
            Ok(cwd) => cwd,
            Err(error) => break 'run AgentCommandResult::Error(error),
        };
        let Ok(mut region) = regions.get_mut(owner) else {
            break 'run AgentCommandResult::Error(
                "agent terminal placement owner is unavailable".into(),
            );
        };
        if run.beside.is_none() && run.mode == PlacementMode::Auto {
            for (placement, mut pending, state) in &mut pending {
                if !pending.append(owner, &run, &cwd, state) {
                    continue;
                }
                commands
                    .entity(operation)
                    .insert(PendingRunReply { placement, reply });
                return;
            }
        }
        let candidates = context.terminals.candidates(pane, &context.panes, &cwd);
        if run.beside.is_none()
            && run.mode == PlacementMode::Auto
            && let Some(candidate) = region.choose_reusable_terminal(pane, &candidates)
        {
            let launch = match context.terminals.launch(candidate.pid) {
                Ok(launch) => launch,
                Err(error) => break 'run AgentCommandResult::Error(error),
            };
            terminal_reinput.write(TerminalStackSpawnRequest::reinput(
                candidate.pid,
                &launch,
                &run.command,
                run.done_marker.as_deref(),
                true,
            ));
            region.run_terminal = Some(candidate.pid);
            region.run_pane = Some(candidate.pane);
            commands.entity(candidate.pane).insert(sequences.take());
            if focus {
                for entity in context.panes.activation_entities(candidate) {
                    commands.entity(entity).insert(LastActivatedAt::now());
                }
            }
            break 'run AgentCommandResult::Text(candidate.pid.to_string());
        }
        commands.entity(operation).insert(PreparedRun {
            run,
            reply,
            owner,
            pane,
            cwd,
            focus,
        });
        commands.trigger(AllocateRun { operation });
        return;
    };
    responses.write(reply.response(result));
    commands.entity(operation).despawn();
}

#[derive(Component)]
struct PreparedRun {
    run: AgentRun,
    reply: AgentReply,
    owner: Entity,
    pane: Entity,
    cwd: std::path::PathBuf,
    focus: bool,
}

#[derive(EntityEvent)]
struct AllocateRun {
    #[event_target]
    operation: Entity,
}

fn allocate_run(
    event: On<AllocateRun>,
    ready: Query<&PreparedRun>,
    context: AgentRunContext,
    settings: Res<AppSettings>,
    regions: Query<&AgentTerminalRegion>,
    mut commands: Commands,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let operation = event.operation;
    let Ok(ready) = ready.get(operation) else {
        return;
    };
    let run = &ready.run;
    let owner = ready.owner;
    let pane = ready.pane;
    let cwd = ready.cwd.clone();
    let focus = ready.focus;
    let reply = ready.reply;
    let candidates = context.terminals.candidates(pane, &context.panes, &cwd);
    let result = 'run: {
        let Ok(region) = regions.get(owner) else {
            break 'run AgentCommandResult::Error(
                "agent terminal placement owner is unavailable".into(),
            );
        };
        if context.resolve_pane(run.anchor) != Some((owner, pane)) {
            break 'run AgentCommandResult::Error("agent terminal placement source changed".into());
        }
        let (shell, data) = TerminalStackSpawnRequest::for_new_terminal(
            &settings,
            &run.command,
            run.done_marker.as_deref(),
        );
        if let Err(error) = TerminalStackSpawnRequest::validate_shell(&shell) {
            break 'run AgentCommandResult::Error(error);
        }
        let explicit = match run.beside {
            Some(pid) => match context.terminals.pane(pid, &context.panes) {
                Some(pane) => Some(pane),
                None => {
                    break 'run AgentCommandResult::Error(format!(
                        "run.beside page not found: {pid}"
                    ));
                }
            },
            None => None,
        };
        let bucket = if run.mode == PlacementMode::Split && explicit.is_none() {
            region
                .choose_bucket_pane(pane, &candidates)
                .filter(|target| context.panes.bucket(pane, *target))
                .or_else(|| context.panes.newest_bucket(pane))
        } else {
            None
        };
        let anchor = explicit.or(bucket).unwrap_or(pane);
        let direction = (run.mode == PlacementMode::Split && bucket.is_none())
            .then_some(PaneDirection::from(run.direction));
        let automatic = run.mode != PlacementMode::Split && explicit.is_none();
        let primary_anchor =
            run.mode == PlacementMode::Split && explicit.is_none() && bucket.is_none();
        let source_stack = context.panes.parents.get(owner).unwrap().parent();
        let stack = commands
            .spawn((
                Stack::bundle(),
                LastActivatedAt(if focus { UnixMillis::now().0 } else { 0 }),
            ))
            .insert(PageMetadata {
                url: crate::TerminalPlugin::url().into(),
                ..Default::default()
            })
            .id();
        let process_id = ProcessId::new();
        let request = TerminalStackSpawnRequest {
            pane: anchor,
            cwd: Some(cwd),
            shell: Some(shell),
            agent_run: true,
            pending_input: Some(data),
            process_id: Some(process_id),
            activate: focus,
        };
        commands.entity(operation).insert(PendingAgentPlacement {
            reply,
            owner,
            anchor: run.anchor,
            source_stack,
            source_pane: pane,
            stack,
            request,
            update_latest: run.beside.is_none() && run.mode != PlacementMode::Split,
        });
        commands.trigger(PlaceStackRequest {
            operation,
            stack,
            anchor,
            url: crate::TerminalPlugin::url().into(),
            direction,
            automatic,
            primary_anchor,
            focus,
        });

        return;
    };
    responses.write(reply.response(result));
    commands.entity(operation).despawn();
}

fn placed(
    event: On<StackPlaced>,
    pending: Query<&PendingAgentPlacement>,
    sources: Query<(&ProcessId, &ChildOf)>,
    panes: Query<(), With<Pane>>,
    mut regions: Query<&mut AgentTerminalRegion>,
    replies: Query<(Entity, &PendingRunReply)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let Ok(pending) = pending.get(event.operation) else {
        return;
    };
    let valid = event.stack == pending.stack
        && panes.contains(pending.source_pane)
        && sources.get(pending.owner).is_ok_and(|(pid, parent)| {
            *pid == pending.anchor && parent.parent() == pending.source_stack
        });
    let result = match &event.result {
        Ok(pane) if valid => {
            let mut request = pending.request.clone();
            request.pane = *pane;
            commands.trigger(super::plugin::SpawnPlacedTerminal {
                stack: pending.stack,
                request,
            });
            if let Ok(mut region) = regions.get_mut(pending.owner) {
                region.run_pane = Some(*pane);
                if pending.update_latest {
                    region.run_terminal = pending.request.process_id;
                }
            }
            AgentCommandResult::Text(pending.request.process_id.unwrap().to_string())
        }
        Err(error) => AgentCommandResult::Error(error.clone()),
        _ => AgentCommandResult::Error("agent terminal placement source changed".into()),
    };
    if matches!(result, AgentCommandResult::Error(_))
        && !parents.contains(pending.stack)
        && children
            .get(pending.stack)
            .map_or(true, |children| children.is_empty())
    {
        commands.entity(pending.stack).try_despawn();
    }
    responses.write(pending.reply.response(result.clone()));
    for (entity, reply) in &replies {
        if reply.placement == event.operation {
            responses.write(reply.reply.response(result.clone()));
            commands.entity(entity).despawn();
        }
    }
    commands.entity(event.operation).despawn();
}

fn validate_owner(
    event: On<PlacementStarting>,
    pending: Query<&PendingAgentPlacement>,
    sources: Query<(&ProcessId, &ChildOf)>,
    mut commands: Commands,
) {
    let Ok(pending) = pending.get(event.operation) else {
        return;
    };
    if !sources.get(pending.owner).is_ok_and(|(pid, parent)| {
        *pid == pending.anchor && parent.parent() == pending.source_stack
    }) {
        commands
            .entity(event.operation)
            .insert(vmux_ecs::OperationState::Canceled);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct NextPaneSpawnSequence<'w, 's> {
    counter: Single<'w, 's, &'static mut SpawnCounter>,
    sequences: Query<'w, 's, &'static SpawnSeq>,
}

impl NextPaneSpawnSequence<'_, '_> {
    fn take(&mut self) -> SpawnSeq {
        for sequence in &self.sequences {
            self.counter.0 = self.counter.0.max(sequence.0);
        }
        self.counter.0 += 1;
        SpawnSeq(self.counter.0)
    }
}

fn reinput(
    run: &AgentRun,
    process_id: ProcessId,
    terminals: &RunTerminals,
    writer: &mut MessageWriter<TerminalReinputRequest>,
) -> AgentCommandResult {
    let launch = match terminals.launch(process_id) {
        Ok(launch) => launch,
        Err(error) => return AgentCommandResult::Error(error),
    };
    writer.write(TerminalStackSpawnRequest::reinput(
        process_id,
        &launch,
        &run.command,
        run.done_marker.as_deref(),
        false,
    ));
    AgentCommandResult::Text(process_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
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
                pane: PaneSettings::default(),
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

    #[derive(Resource, Default)]
    struct NativeSpawns(Vec<(Entity, TerminalStackSpawnRequest)>);

    struct PlacementFixture {
        app: App,
        directory: tempfile::TempDir,
        owner: Entity,
        process: ProcessId,
        pane: Entity,
    }

    impl PlacementFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            let mut settings = test_settings();
            settings.terminal = Some(TerminalSettings {
                default_theme: "default".into(),
                themes: vec![
                    serde_json::from_value(
                        serde_json::json!({ "name": "default", "shell": "/bin/sh" }),
                    )
                    .unwrap(),
                ],
                ..Default::default()
            });
            app.add_plugins((MinimalPlugins, AgentRunPlugin))
                .insert_resource(settings)
                .add_message::<AgentCommandResponse>()
                .add_message::<TerminalReinputRequest>()
                .init_resource::<NativeSpawns>()
                .add_observer(Self::receive);
            app.world_mut().spawn(crate::TerminalPlugin::manifest());
            let tab = app
                .world_mut()
                .spawn(Tab {
                    name: "Test".into(),
                    startup_dir: Some(directory.path().to_string_lossy().into_owned()),
                })
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane::bundle(), SpawnSeq(1), ChildOf(tab)))
                .id();
            let stack = spawn_stack_in_pane(&mut app, pane, "vmux://sessions/test");
            let process = ProcessId::new();
            let owner = app
                .world_mut()
                .spawn((
                    Terminal,
                    process,
                    TerminalLaunch {
                        command: "/bin/sh".into(),
                        args: Vec::new(),
                        cwd: directory.path().to_string_lossy().into_owned(),
                        env: Vec::new(),
                    },
                    ChildOf(stack),
                ))
                .id();
            app.update();
            Self {
                app,
                directory,
                owner,
                process,
                pane,
            }
        }

        fn receive(
            event: On<super::super::plugin::SpawnPlacedTerminal>,
            mut spawns: ResMut<NativeSpawns>,
        ) {
            spawns.0.push((event.stack, event.request.clone()));
        }

        fn request(&mut self, command: &str) -> vmux_api::protocol::AgentRequestId {
            let request_id = vmux_api::protocol::AgentRequestId::new();
            self.app.world_mut().write_message(AgentRequestInput {
                request_id,
                origin: vmux_ecs::agent::CommandOrigin::User,
                request: vmux_api::protocol::AgentRequest::encode(&AgentRun {
                    anchor: self.process,
                    command: command.into(),
                    direction: AgentPaneDirection::Right,
                    focus: false,
                    beside: None,
                    mode: PlacementMode::Auto,
                    terminal: None,
                    done_marker: None,
                })
                .unwrap(),
            });
            request_id
        }
    }

    #[test]
    fn same_frame_agent_input_uses_one_placed_stack_and_one_native_spawn() {
        let mut fixture = PlacementFixture::new();
        let first = fixture.request("printf first");
        let second = fixture.request("printf second");
        fixture.app.update();
        let spawns = fixture.app.world().resource::<NativeSpawns>().0.clone();
        assert_eq!(spawns.len(), 1);
        let (stack, request) = &spawns[0];
        assert_eq!(
            fixture.app.world().get::<ChildOf>(*stack).unwrap().parent(),
            request.pane
        );
        assert!(fixture.app.world().get::<Pane>(request.pane).is_some());
        assert!(fixture.app.world().get::<PaneSplit>(fixture.pane).is_some());
        let input = String::from_utf8(request.pending_input.clone().unwrap()).unwrap();
        assert!(input.contains("printf first"));
        assert!(input.contains("printf second"));
        assert_eq!(input.matches('\r').count(), 2);
        assert_eq!(
            vmux_path::PathIdentity::resolve(request.cwd.as_deref().unwrap()),
            vmux_path::PathIdentity::resolve(fixture.directory.path())
        );
        let replies = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AgentCommandResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(replies.len(), 2);
        assert!(replies.iter().any(|reply| reply.request_id == first));
        assert!(replies.iter().any(|reply| reply.request_id == second));
        assert!(replies.iter().all(|reply| reply.result
            == AgentCommandResult::Text(request.process_id.unwrap().to_string())));
    }

    #[test]
    fn a_retired_agent_owner_refuses_placement_before_native_admission() {
        let mut fixture = PlacementFixture::new();
        let owner = fixture.owner;
        fixture
            .app
            .add_observer(move |_: On<PlaceStackRequest>, mut commands: Commands| {
                commands.entity(owner).try_despawn();
            });
        fixture.request("printf unstarted");
        fixture.app.update();
        assert!(fixture.app.world().resource::<NativeSpawns>().0.is_empty());
        assert!(fixture.app.world().get::<PaneSplit>(fixture.pane).is_none());
        let replies = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AgentCommandResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(replies.len(), 1);
        assert!(matches!(replies[0].result, AgentCommandResult::Error(_)));
    }

    #[test]
    fn a_prepared_run_with_a_retired_owner_replies_and_retires() {
        let mut fixture = PlacementFixture::new();
        let request_id = vmux_api::protocol::AgentRequestId::new();
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                vmux_ecs::Operation::bundle("test.prepared-run").unwrap(),
                vmux_ecs::OperationRetained,
                PreparedRun {
                    run: AgentRun {
                        anchor: fixture.process,
                        command: "printf unstarted".into(),
                        direction: AgentPaneDirection::Right,
                        focus: false,
                        beside: None,
                        mode: PlacementMode::Auto,
                        terminal: None,
                        done_marker: None,
                    },
                    reply: AgentReply::new(request_id),
                    owner: fixture.owner,
                    pane: fixture.pane,
                    cwd: fixture.directory.path().to_path_buf(),
                    focus: false,
                },
            ))
            .id();
        fixture.app.world_mut().despawn(fixture.owner);
        fixture.app.world_mut().trigger(AllocateRun { operation });
        fixture.app.world_mut().flush();
        assert!(fixture.app.world().get_entity(operation).is_err());
        assert!(fixture.app.world().resource::<NativeSpawns>().0.is_empty());
        assert!(fixture.app.world().get::<PaneSplit>(fixture.pane).is_none());
        let replies = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AgentCommandResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].request_id, request_id);
        assert!(matches!(replies[0].result, AgentCommandResult::Error(_)));
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
            .add_plugins(AgentRunPlugin);
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
            WorkspaceCwd::resolve(
                Some(tab_dir.to_string_lossy().as_ref()),
                Some(agent_dir.to_string_lossy().as_ref()),
            )
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
        let got = WorkspaceCwd::resolve(None, Some(&dir.to_string_lossy())).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, dir);
    }

    #[test]
    fn run_terminal_cwd_requires_tab_or_agent_workspace() {
        assert!(WorkspaceCwd::resolve(None, Some("")).is_err());
        assert!(WorkspaceCwd::resolve(None, None).is_err());
    }

    #[test]
    fn run_terminal_cwd_rejects_invalid_stored_tab_directory() {
        let agent_dir = std::env::temp_dir();

        assert!(
            WorkspaceCwd::resolve(Some("/no/such/vmux-tab-workspace"), agent_dir.to_str()).is_err()
        );
    }

    #[test]
    fn run_terminal_cwd_rejects_relative_stored_tab_directory() {
        assert!(WorkspaceCwd::resolve(Some("."), None).is_err());
    }

    #[test]
    fn only_the_first_command_in_a_terminal_sets_the_pager() {
        let primed = TerminalStackSpawnRequest::command_with_marker(
            "/opt/homebrew/bin/nu",
            "ls",
            "abc",
            true,
        );
        let later = TerminalStackSpawnRequest::command_with_marker(
            "/opt/homebrew/bin/nu",
            "ls",
            "abc",
            false,
        );

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
            TerminalStackSpawnRequest::command_with_marker(
                "/opt/homebrew/bin/nu",
                "ls",
                "abc",
                true
            ),
            "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; $env.LAST_EXIT_CODE = 0; let __vmux_status = try { ls; $env.LAST_EXIT_CODE } catch {|error| $error.exit_code? | default 1 }; print -rn $\"\\u{1b}]6973;abc;($__vmux_status)\\u{7}\""
        );
        assert_eq!(
            TerminalStackSpawnRequest::command_with_marker(
                "/usr/local/bin/fish",
                "ls",
                "abc",
                true
            ),
            "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ls; set __vmux_status $status; printf '\\033]6973;abc;%s\\007' $__vmux_status"
        );
        assert_eq!(
            TerminalStackSpawnRequest::command_with_marker("/bin/zsh", "ls", "abc", true),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
        assert_eq!(
            TerminalStackSpawnRequest::command_with_marker("/usr/bin/xonsh", "ls", "abc", true),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
    }

    #[test]
    fn run_command_line_noop_when_token_absent() {
        assert_eq!(
            TerminalStackSpawnRequest::format_line("/bin/zsh", "ls -la", None, true),
            "ls -la"
        );
    }

    #[test]
    fn run_command_line_embeds_marker_when_token_present() {
        let out = TerminalStackSpawnRequest::format_line("/bin/zsh", "ls -la", Some("tok9"), true);
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

        let (shell, input) =
            TerminalStackSpawnRequest::for_new_terminal(&settings, "cd /tmp", Some("tok9"));

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
        let line = TerminalStackSpawnRequest::command_with_marker(
            "/opt/homebrew/bin/nu",
            "input",
            "tok",
            false,
        );

        assert_eq!(line.matches('\r').count(), 0);
        assert!(line.contains("try { input; $env.LAST_EXIT_CODE }"));
        assert!(line.ends_with("($__vmux_status)\\u{7}\""));
    }

    #[test]
    fn new_agent_run_terminal_rejects_missing_configured_shell() {
        let shell = "/definitely/missing/vmux-terminal-shell";

        assert_eq!(
            TerminalStackSpawnRequest::validate_shell(shell),
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

        let input =
            TerminalStackSpawnRequest::format_input(&launch.command, "pwd", Some("tok2"), true);
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
        let process_id = ProcessId::new();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TerminalContractPlugin));
        let launch = TerminalLaunch {
            command: "/usr/local/bin/fish".to_string(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
        };
        app.world_mut()
            .write_message(TerminalStackSpawnRequest::reinput(
                process_id,
                &launch,
                "pwd",
                Some("tok4"),
                false,
            ));

        let captured = app
            .world_mut()
            .resource_mut::<Messages<TerminalReinputRequest>>()
            .drain()
            .collect::<Vec<_>>();
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
            .leaves
            .iter()
            .filter(|pane| panes.bucket(input.agent_pane, *pane))
            .collect();
    }

    #[test]
    fn run_terminal_bucket_panes_include_pure_terminal_layout_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalBucketPaneOutput>()
            .add_systems(Update, collect_run_bucket_panes);
        app.world_mut().spawn(crate::TerminalPlugin::manifest());

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
        let terminal = ProcessId::new();
        let pane = Entity::from_bits(20);
        let mut terminal_spawns = [TerminalStackSpawnRequest {
            pane,
            cwd: Some(std::env::temp_dir()),
            shell: Some("/opt/homebrew/bin/nu".to_string()),
            agent_run: true,
            pending_input: Some(b"one\r".to_vec()),
            process_id: Some(terminal),
            activate: false,
        }];

        let picked = terminal_spawns[0].append_input(&std::env::temp_dir(), "pwd", Some("tok2"));

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
        let terminal = ProcessId::new();
        let mut terminal_spawns = [TerminalStackSpawnRequest {
            pane: Entity::from_bits(20),
            cwd: Some(old_cwd.clone()),
            shell: Some("/opt/homebrew/bin/nu".to_string()),
            agent_run: true,
            pending_input: Some(b"one\r".to_vec()),
            process_id: Some(terminal),
            activate: false,
        }];

        let picked = terminal_spawns[0].append_input(&new_cwd, "pwd", Some("tok2"));

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

    fn split_run_pane(input: Res<SplitRunPaneInput>, mut commands: Commands) {
        let stack = commands
            .spawn((Stack::bundle(), LastActivatedAt(0)))
            .insert(PageMetadata {
                url: crate::TerminalPlugin::url().into(),
                ..Default::default()
            })
            .id();
        let operation = commands
            .spawn(vmux_ecs::Operation::bundle("test.run.split").unwrap())
            .id();
        commands.trigger(PlaceStackRequest {
            operation,
            stack,
            anchor: input.pane,
            url: crate::TerminalPlugin::url().into(),
            direction: Some(PaneDirection::Bottom),
            automatic: false,
            primary_anchor: false,
            focus: false,
        });
    }

    fn split_run_placed(
        event: On<StackPlaced>,
        mut out: ResMut<SplitRunPaneOutput>,
        mut commands: Commands,
    ) {
        out.0 = event.result.as_ref().ok().copied();
        commands.entity(event.operation).despawn();
    }

    #[test]
    fn split_run_pane_becomes_newest_for_followup_placement() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .init_resource::<SplitRunPaneOutput>()
            .add_plugins(PlacementPlugin)
            .add_observer(split_run_placed)
            .add_systems(Update, split_run_pane.before(PlacementCommitSet));

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
        let mut fixture = PlacementFixture::new();
        let request_id = vmux_api::protocol::AgentRequestId::new();
        fixture.app.world_mut().write_message(AgentRequestInput {
            request_id,
            origin: vmux_ecs::agent::CommandOrigin::User,
            request: vmux_api::protocol::AgentRequest::encode(&AgentRunWithPlacementOverride(
                AgentRun {
                    anchor: fixture.process,
                    command: "pwd".into(),
                    direction: AgentPaneDirection::Right,
                    focus: false,
                    beside: None,
                    mode: PlacementMode::Auto,
                    terminal: None,
                    done_marker: None,
                },
            ))
            .unwrap(),
        });
        fixture.app.update();
        let replies = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AgentCommandResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].request_id, request_id);
        assert_eq!(
            replies[0].result,
            AgentCommandResult::Error(
                "run placement overrides are disabled; omit mode, direction, and beside and retry"
                    .into()
            )
        );
    }

    #[test]
    fn run_placement_policy_allows_bare_run() {
        let settings = test_settings();
        let run = AgentRun {
            anchor: ProcessId::new(),
            command: "pwd".into(),
            direction: AgentPaneDirection::Right,
            focus: false,
            beside: None,
            mode: PlacementMode::Auto,
            terminal: None,
            done_marker: None,
        };
        assert_eq!(run.validate_placement(false, &settings), Ok(()));
    }

    #[test]
    fn run_placement_policy_honors_user_opt_out() {
        let mut settings = test_settings();
        settings.agent.allow_run_placement_override = true;
        let run = AgentRun {
            anchor: ProcessId::new(),
            command: "pwd".into(),
            direction: AgentPaneDirection::Bottom,
            focus: false,
            beside: None,
            mode: PlacementMode::Split,
            terminal: None,
            done_marker: None,
        };
        assert_eq!(run.validate_placement(true, &settings), Ok(()));
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
