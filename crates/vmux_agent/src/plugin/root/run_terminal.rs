//! The terminal an agent runs commands in, and where it goes on screen.
//!
//! An agent reuses its own run terminal while one is live, so a session's output stays in one
//! place instead of spraying new panes. [`AgentTerminalRegions`] caches that choice; the candidate
//! search rebuilds it when the cache is cold or the terminal has since exited.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_core::{LastActivatedAt, PageMetadata};
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_service::protocol::ProcessId;
use vmux_setting::AppSettings;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::{AgentRunTerminal, ProcessExited, Terminal, TerminalStackSpawnRequest};

use crate::session::AgentSession;

use super::valid_cwd;

#[derive(Resource, Default)]
pub struct AgentTerminalRegions {
    pub run_terminals: std::collections::HashMap<ProcessId, ProcessId>,
    pub run_panes: std::collections::HashMap<ProcessId, Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunTerminalCandidate {
    pub(crate) terminal: Entity,
    pub(crate) pid: ProcessId,
    pub(crate) stack: Entity,
    pub(crate) pane: Entity,
    pub(crate) pane_spawn_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunTerminalBucketPaneCandidate {
    pane: Entity,
    pane_spawn_seq: u64,
}

pub(crate) fn choose_reusable_run_terminal(
    anchor: ProcessId,
    agent_pane: Entity,
    regions: &AgentTerminalRegions,
    candidates: &[RunTerminalCandidate],
) -> Option<RunTerminalCandidate> {
    if let Some(pid) = regions.run_terminals.get(&anchor)
        && let Some(candidate) = candidates.iter().find(|c| c.pid == *pid)
    {
        return Some(*candidate);
    }
    if let Some(pane) = regions.run_panes.get(&anchor)
        && let Some(candidate) = candidates
            .iter()
            .filter(|c| c.pane == *pane)
            .max_by_key(|c| c.pane_spawn_seq)
    {
        return Some(*candidate);
    }
    candidates
        .iter()
        .filter(|c| c.pane != agent_pane)
        .max_by_key(|c| c.pane_spawn_seq)
        .copied()
}

pub(crate) fn choose_run_terminal_bucket_pane(
    anchor: ProcessId,
    agent_pane: Entity,
    regions: &AgentTerminalRegions,
    candidates: &[RunTerminalCandidate],
) -> Option<Entity> {
    choose_reusable_run_terminal(anchor, agent_pane, regions, candidates)
        .map(|c| c.pane)
        .or_else(|| {
            regions
                .run_panes
                .get(&anchor)
                .copied()
                .filter(|pane| *pane != agent_pane)
        })
}

/// The pane containing the terminal whose `ProcessId` is `pid` (its stack's
/// parent pane). Used to anchor a `run` next to an existing terminal page.
pub(crate) fn resolve_pane_for_pid(
    pid: ProcessId,
    term_pids: &Query<(Entity, &ProcessId), With<Terminal>>,
    child_of_q: &Query<&ChildOf>,
) -> Option<Entity> {
    use bevy::ecs::relationship::Relationship;
    let (term, _) = term_pids.iter().find(|(_, p)| **p == pid)?;
    let stack = child_of_q.get(term).ok()?.get();
    let pane = child_of_q.get(stack).ok()?.get();
    Some(pane)
}

pub(crate) fn tab_of_run_pane(
    pane: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
) -> Option<Entity> {
    use bevy::ecs::relationship::Relationship;
    let mut cur = pane;
    for _ in 0..32 {
        if tab_q.contains(cur) {
            return Some(cur);
        }
        cur = child_of_q.get(cur).ok()?.get();
    }
    None
}

pub(crate) fn run_terminal_candidates(
    agent_pane: Entity,
    terminals: &Query<
        (Entity, &ProcessId, &TerminalLaunch, Has<AgentRunTerminal>),
        (
            With<Terminal>,
            Without<AgentSession>,
            Without<ProcessExited>,
        ),
    >,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
    seq_q: &Query<&vmux_layout::pane::SpawnSeq>,
    desired_cwd: &Path,
) -> Vec<RunTerminalCandidate> {
    use bevy::ecs::relationship::Relationship;
    let Some(agent_tab) = tab_of_run_pane(agent_pane, child_of_q, tab_q) else {
        return Vec::new();
    };
    let desired_cwd = desired_cwd
        .canonicalize()
        .unwrap_or_else(|_| desired_cwd.to_path_buf());
    terminals
        .iter()
        .filter_map(|(terminal, pid, launch, agent_run)| {
            if !agent_run {
                return None;
            }
            let stack = child_of_q.get(terminal).ok()?.get();
            let pane = child_of_q.get(stack).ok()?.get();
            if pane == agent_pane {
                return None;
            }
            if tab_of_run_pane(pane, child_of_q, tab_q) != Some(agent_tab) {
                return None;
            }
            if !run_terminal_launch_matches_canonical_cwd(&launch.cwd, &desired_cwd) {
                return None;
            }
            Some(RunTerminalCandidate {
                terminal,
                pid: *pid,
                stack,
                pane,
                pane_spawn_seq: seq_q.get(pane).map(|s| s.0).unwrap_or(0),
            })
        })
        .collect()
}

pub(crate) fn run_terminal_bucket_panes(
    agent_pane: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_q: &Query<Entity, With<vmux_layout::stack::Stack>>,
    page_q: &Query<&PageMetadata, With<vmux_layout::stack::Stack>>,
    seq_q: &Query<&vmux_layout::pane::SpawnSeq>,
) -> Vec<RunTerminalBucketPaneCandidate> {
    let Some(agent_tab) = tab_of_run_pane(agent_pane, child_of_q, tab_q) else {
        return Vec::new();
    };
    leaf_panes
        .iter()
        .filter_map(|pane| {
            if pane == agent_pane {
                return None;
            }
            if tab_of_run_pane(pane, child_of_q, tab_q) != Some(agent_tab) {
                return None;
            }
            let children = pane_children.get(pane).ok()?;
            let mut has_stack = false;
            for stack in children.iter().filter(|&child| stack_q.contains(child)) {
                has_stack = true;
                let meta = page_q.get(stack).ok()?;
                if vmux_layout::placement::page_kind_for_url(&meta.url)
                    != vmux_layout::placement::PageKind::Terminal
                {
                    return None;
                }
            }
            has_stack.then(|| RunTerminalBucketPaneCandidate {
                pane,
                pane_spawn_seq: seq_q.get(pane).map(|s| s.0).unwrap_or(0),
            })
        })
        .collect()
}

pub(crate) fn newest_run_terminal_bucket_pane(
    agent_pane: Entity,
    candidates: &[RunTerminalBucketPaneCandidate],
) -> Option<Entity> {
    candidates
        .iter()
        .filter(|c| c.pane != agent_pane)
        .max_by_key(|c| c.pane_spawn_seq)
        .map(|c| c.pane)
}

pub(crate) fn is_run_terminal_bucket_pane(
    pane: Entity,
    candidates: &[RunTerminalBucketPaneCandidate],
) -> bool {
    candidates.iter().any(|c| c.pane == pane)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingRunTerminalSpawn {
    pub(crate) pid: ProcessId,
    pub(crate) request_index: usize,
    pub(crate) shell: String,
}

pub(crate) fn append_pending_run_terminal_input(
    anchor: ProcessId,
    pending_spawns: &std::collections::HashMap<ProcessId, PendingRunTerminalSpawn>,
    terminal_spawns: &mut [TerminalStackSpawnRequest],
    desired_cwd: &Path,
    command: &str,
    token: Option<&str>,
) -> Option<ProcessId> {
    let pending = pending_spawns.get(&anchor)?;
    let request = terminal_spawns.get_mut(pending.request_index)?;
    let request_cwd = request.cwd.as_deref()?.canonicalize().ok()?;
    let desired_cwd = desired_cwd.canonicalize().ok()?;
    if request_cwd != desired_cwd {
        return None;
    }
    let data = run_command_input(command, token, &pending.shell);
    match &mut request.pending_input {
        Some(input) => input.extend(data),
        None => request.pending_input = Some(data),
    }
    Some(pending.pid)
}

pub(crate) fn touch_reused_run_pane_spawn_seq(
    pane: Entity,
    commands: &mut Commands,
    spawn_counter: &mut vmux_layout::pane::SpawnCounter,
    seq_q: &Query<&vmux_layout::pane::SpawnSeq>,
) {
    let max_existing = seq_q.iter().map(|s| s.0).max().unwrap_or(0);
    if spawn_counter.0 <= max_existing {
        spawn_counter.0 = max_existing;
    }
    spawn_counter.0 += 1;
    commands
        .entity(pane)
        .insert(vmux_layout::pane::SpawnSeq(spawn_counter.0));
}

pub(crate) fn focus_reused_run_terminal(
    candidate: RunTerminalCandidate,
    commands: &mut Commands,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<vmux_layout::tab::Tab>>,
) {
    commands
        .entity(candidate.stack)
        .insert(LastActivatedAt::now());
    commands
        .entity(candidate.pane)
        .insert(LastActivatedAt::now());
    if let Some(tab) = tab_of_run_pane(candidate.pane, child_of_q, tab_q) {
        commands.entity(tab).insert(LastActivatedAt::now());
    }
}

/// Split `pane` and return the new leaf pane. Batches several splits of the same
/// pane in one tick (extend an existing split instead of re-splitting the leaf).
#[allow(clippy::too_many_arguments)]
pub(crate) fn split_pane_off(
    commands: &mut Commands,
    pane: Entity,
    direction: &vmux_service::protocol::AgentPaneDirection,
    focus: bool,
    pane_children: &Query<&Children, With<Pane>>,
    tab_filter: &Query<Entity, With<vmux_layout::stack::Stack>>,
    split_dir_q: &Query<&PaneSplit>,
    split_this_batch: &mut std::collections::HashSet<Entity>,
) -> Entity {
    let existing_tabs: Vec<Entity> = pane_children
        .get(pane)
        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
        .unwrap_or_default();
    let split_dir = vmux_layout::pane::direction_to_split(&to_pane_direction(direction));
    let already_split = !split_this_batch.insert(pane) || split_dir_q.contains(pane);
    vmux_layout::pane::split_or_extend(
        commands,
        pane,
        split_dir,
        &existing_tabs,
        focus,
        already_split,
    )
}

pub(crate) fn to_pane_direction(
    d: &vmux_service::protocol::AgentPaneDirection,
) -> vmux_command::open::PaneDirection {
    use vmux_command::open::PaneDirection;
    use vmux_service::protocol::AgentPaneDirection as D;
    match d {
        D::Top => PaneDirection::Top,
        D::Right => PaneDirection::Right,
        D::Bottom => PaneDirection::Bottom,
        D::Left => PaneDirection::Left,
    }
}

pub(crate) fn agent_terminal_shell(settings: &AppSettings) -> String {
    settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()))
}

/// Wrap a `run` command so the shell emits an invisible OSC completion escape
/// carrying the exit code once the command finishes (success OR failure).
/// `token` is a unique per-run id; the escape is
/// `ESC ] <VMUX_RUN_OSC> ; <token> ; <exit_code> BEL` (see
/// [`vmux_service::run_marker`]). Because it is an OSC sequence the terminal
/// parser consumes it — it never renders as text, unlike the old
/// `__VMUX_DONE_…__` printf markers.
///
/// The command is prefixed with [`pager_env_prefix`] so an interactive command that would
/// normally open a pager (e.g. `git log` → `less`) prints straight to the terminal instead of
/// blocking the marker forever.
///
/// posix/fish chain with `;` (which continues after a non-zero command). nushell
/// aborts the rest of a `;` line when an external command fails, so it needs a
/// `try`/`catch` wrapper to always emit the escape and recover the exit code
/// from the caught error.
pub(crate) fn command_with_marker(shell: &str, command: &str, token: &str) -> String {
    let base = std::path::Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(shell);
    let pager = pager_env_prefix(base);
    let osc = vmux_service::run_marker::VMUX_RUN_OSC;
    match base {
        "nu" | "nushell" => format!(
            "{pager}try {{ {command}; print -rn $\"\\u{{1b}}]{osc};{token};($env.LAST_EXIT_CODE)\\u{{7}}\" }} catch {{ |e| print -rn $\"\\u{{1b}}]{osc};{token};($e.exit_code? | default 1)\\u{{7}}\" }}"
        ),
        "fish" => format!(
            "{pager}{command}; set __vmux_status $status; printf '\\033]{osc};{token};%s\\007' $__vmux_status"
        ),
        _ => format!(
            "{pager}{command}; __vmux_status=\"$?\"; printf '\\033]{osc};{token};%s\\007' \"$__vmux_status\""
        ),
    }
}

/// Shell-specific prelude that neutralizes pagers for a `run`, so an interactive command can't
/// stall the completion marker waiting on `less` (`git log`, `man`, `git diff`, …). Set as
/// session-exported env so follow-up runs in the same shell stay covered.
pub(crate) fn pager_env_prefix(base: &str) -> &'static str {
    match base {
        "nu" | "nushell" => "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; ",
        "fish" => "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ",
        _ => "export GIT_PAGER=cat PAGER=cat LESS=FRX; ",
    }
}

pub(crate) fn run_command_line(command: &str, token: Option<&str>, shell: &str) -> String {
    match token {
        Some(token) => command_with_marker(shell, command, token),
        None => command.to_string(),
    }
}

pub(crate) const RUN_PLACEMENT_OVERRIDE_DISABLED: &str =
    "run placement overrides are disabled; omit mode, direction, and beside and retry";

pub(crate) fn validate_run_placement_policy(
    settings: &AppSettings,
    placement_override: bool,
) -> Result<(), &'static str> {
    if placement_override && !settings.agent.allow_run_placement_override {
        Err(RUN_PLACEMENT_OVERRIDE_DISABLED)
    } else {
        Ok(())
    }
}

pub(crate) fn run_command_input(command: &str, token: Option<&str>, shell: &str) -> Vec<u8> {
    let mut data = run_command_line(command, token, shell).into_bytes();
    data.push(b'\r');
    data
}

pub(crate) fn terminal_run_command_input(
    command: &str,
    token: Option<&str>,
    launch: &TerminalLaunch,
) -> Vec<u8> {
    run_command_input(command, token, &launch.command)
}

pub(crate) fn explicit_run_terminal_launch(
    process_id: ProcessId,
    terminals: &Query<(Entity, &ProcessId), With<Terminal>>,
    launches: &Query<&TerminalLaunch>,
) -> Result<TerminalLaunch, String> {
    let Some(entity) = terminals
        .iter()
        .find_map(|(entity, candidate)| (*candidate == process_id).then_some(entity))
    else {
        return Err(format!("run.terminal page not found: {process_id}"));
    };
    launches
        .get(entity)
        .cloned()
        .map_err(|_| format!("run terminal launch not found: {process_id}"))
}

pub(crate) fn queue_terminal_run_command_input(
    writer: &mut MessageWriter<vmux_terminal::TerminalReinputRequest>,
    process_id: ProcessId,
    command: &str,
    token: Option<&str>,
    launch: &TerminalLaunch,
) {
    writer.write(vmux_terminal::TerminalReinputRequest {
        process_id,
        data: terminal_run_command_input(command, token, launch),
    });
}

pub(crate) fn new_run_terminal_command(
    settings: &AppSettings,
    command: &str,
    token: Option<&str>,
) -> (String, Vec<u8>) {
    let shell = agent_terminal_shell(settings);
    let input = run_command_input(command, token, &shell);
    (shell, input)
}

pub(crate) fn validate_agent_terminal_shell(shell: &str) -> Result<(), String> {
    if crate::exec::find_executable(shell).is_some() {
        Ok(())
    } else {
        Err(format!(
            "terminal shell not found or not executable: {shell}"
        ))
    }
}

pub(crate) fn stored_tab_cwd(tab_cwd: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(tab_cwd) = tab_cwd else {
        return Ok(None);
    };
    vmux_setting::validate_tab_workspace_dir(tab_cwd).map(Some)
}

pub(crate) fn process_cwd() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub(crate) fn run_terminal_cwd(
    tab_cwd: Option<&str>,
    agent_launch_cwd: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(path) = stored_tab_cwd(tab_cwd)? {
        return Ok(path);
    }
    if let Some(Ok(Some(path))) = agent_launch_cwd.map(valid_cwd) {
        return Ok(path);
    }
    Err("tab and agent project directories are missing".to_string())
}

#[cfg(test)]
pub(crate) fn run_terminal_launch_matches_cwd(launch_cwd: &str, desired_cwd: &Path) -> bool {
    let desired_cwd = desired_cwd
        .canonicalize()
        .unwrap_or_else(|_| desired_cwd.to_path_buf());
    run_terminal_launch_matches_canonical_cwd(launch_cwd, &desired_cwd)
}

pub(crate) fn run_terminal_launch_matches_canonical_cwd(
    launch_cwd: &str,
    desired_cwd: &Path,
) -> bool {
    let Some(launch_cwd) = valid_cwd(launch_cwd).ok().flatten() else {
        return false;
    };
    let launch_cwd = launch_cwd.canonicalize().unwrap_or(launch_cwd);
    launch_cwd == desired_cwd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::root::test_support::{spawn_stack_in_pane, test_settings};
    use vmux_terminal::Terminal;

    #[test]
    pub(crate) fn run_terminal_cwd_prefers_tab_dir() {
        let tab_dir = std::env::temp_dir().join(format!("vmux-tab-cwd-{}", std::process::id()));
        let agent_dir = std::env::temp_dir().join(format!("vmux-agent-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&tab_dir).unwrap();
        std::fs::create_dir_all(&agent_dir).unwrap();
        let canonical_tab_dir = tab_dir.canonicalize().unwrap();
        assert_eq!(
            run_terminal_cwd(
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
    pub(crate) fn run_terminal_launch_must_match_rebound_cwd_for_reuse() {
        let current = std::env::temp_dir().join(format!("vmux-current-cwd-{}", std::process::id()));
        let stale = std::env::temp_dir().join(format!("vmux-stale-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&stale).unwrap();
        assert!(run_terminal_launch_matches_cwd(
            current.to_string_lossy().as_ref(),
            &current,
        ));
        assert!(!run_terminal_launch_matches_cwd(
            stale.to_string_lossy().as_ref(),
            &current,
        ));
        let _ = std::fs::remove_dir_all(&stale);
        let _ = std::fs::remove_dir_all(&current);
    }

    #[test]
    pub(crate) fn run_terminal_cwd_inherits_agent_launch_dir() {
        let dir = std::env::temp_dir().join(format!("vmux-run-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let got = run_terminal_cwd(None, Some(&dir.to_string_lossy())).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, dir);
    }

    #[test]
    pub(crate) fn run_terminal_cwd_requires_tab_or_agent_workspace() {
        assert!(run_terminal_cwd(None, Some("")).is_err());
        assert!(run_terminal_cwd(None, None).is_err());
    }

    #[test]
    pub(crate) fn run_terminal_cwd_rejects_invalid_stored_tab_directory() {
        let agent_dir = std::env::temp_dir();

        assert!(run_terminal_cwd(Some("/no/such/vmux-tab-workspace"), agent_dir.to_str()).is_err());
    }

    #[test]
    pub(crate) fn run_terminal_cwd_rejects_relative_stored_tab_directory() {
        assert!(run_terminal_cwd(Some("."), None).is_err());
    }

    #[test]
    pub(crate) fn command_with_marker_is_shell_aware() {
        // The completion marker is an invisible OSC escape
        // (ESC ] 6973 ; token ; exit BEL), consumed by the terminal parser so it
        // never renders. nushell aborts `;` on failure, so it wraps in try/catch
        // and reads the exit code from the caught error.
        assert_eq!(
            command_with_marker("/opt/homebrew/bin/nu", "ls", "abc"),
            "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; try { ls; print -rn $\"\\u{1b}]6973;abc;($env.LAST_EXIT_CODE)\\u{7}\" } catch { |e| print -rn $\"\\u{1b}]6973;abc;($e.exit_code? | default 1)\\u{7}\" }"
        );
        assert_eq!(
            command_with_marker("/usr/local/bin/fish", "ls", "abc"),
            "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ls; set __vmux_status $status; printf '\\033]6973;abc;%s\\007' $__vmux_status"
        );
        assert_eq!(
            command_with_marker("/bin/zsh", "ls", "abc"),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
        // Unknown shells fall back to posix syntax.
        assert_eq!(
            command_with_marker("/usr/bin/xonsh", "ls", "abc"),
            "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
        );
    }

    #[test]
    pub(crate) fn run_command_line_noop_when_token_absent() {
        assert_eq!(run_command_line("ls -la", None, "/bin/zsh"), "ls -la");
    }

    #[test]
    pub(crate) fn run_command_line_embeds_marker_when_token_present() {
        let out = run_command_line("ls -la", Some("tok9"), "/bin/zsh");
        assert!(out.contains("ls -la"), "got: {out}");
        assert!(out.contains("]6973;tok9;"), "got: {out}");
        assert!(
            !out.contains("__VMUX_DONE_"),
            "marker must be invisible: {out}"
        );
    }

    #[test]
    pub(crate) fn new_agent_run_terminal_uses_configured_shell_for_launch_and_input() {
        let mut settings = test_settings();
        settings.terminal = Some(vmux_setting::TerminalSettings {
            default_theme: "default".to_string(),
            themes: vec![vmux_setting::TerminalTheme {
                name: "default".to_string(),
                color_scheme: "catppuccin-mocha".to_string(),
                font_family: "JetBrainsMono Nerd Font".to_string(),
                font_size: 14.0,
                line_height: 1.2,
                padding: 4.0,
                cursor_style: "block".to_string(),
                cursor_blink: true,
                shell: "/opt/homebrew/bin/nu".to_string(),
            }],
            ..Default::default()
        });

        let (shell, input) = new_run_terminal_command(&settings, "cd /tmp", Some("tok9"));

        assert_eq!(shell, "/opt/homebrew/bin/nu");
        let input = String::from_utf8(input).unwrap();
        assert!(input.contains("try { cd /tmp;"), "got: {input}");
        assert!(input.contains("]6973;tok9;"), "got: {input}");
        assert!(input.ends_with('\r'));
        assert!(!input.contains("export GIT_PAGER"), "got: {input}");
    }

    #[test]
    pub(crate) fn new_agent_run_terminal_rejects_missing_configured_shell() {
        let shell = "/definitely/missing/vmux-terminal-shell";

        assert_eq!(
            validate_agent_terminal_shell(shell),
            Err(format!(
                "terminal shell not found or not executable: {shell}"
            ))
        );
    }

    #[test]
    pub(crate) fn existing_agent_run_terminal_uses_launch_shell_for_input() {
        let launch = TerminalLaunch {
            command: "/usr/local/bin/fish".to_string(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
            kind: vmux_terminal::launch::TerminalKind::Plain,
        };

        let input = terminal_run_command_input("pwd", Some("tok2"), &launch);
        let input = String::from_utf8(input).unwrap();

        assert!(input.contains("set __vmux_status $status"), "got: {input}");
        assert!(input.contains("]6973;tok2;"), "got: {input}");
        assert!(input.ends_with('\r'));
    }

    #[test]
    pub(crate) fn explicit_run_terminal_errors_distinguish_missing_page_and_launch() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        let terminal_pid = ProcessId::new();
        let missing_pid = ProcessId::new();
        app.world_mut().spawn((Terminal, terminal_pid));

        let (missing_page, missing_launch) = app
            .world_mut()
            .run_system_once(
                move |terminals: Query<(Entity, &ProcessId), With<Terminal>>,
                      launches: Query<&TerminalLaunch>| {
                    (
                        explicit_run_terminal_launch(missing_pid, &terminals, &launches)
                            .unwrap_err(),
                        explicit_run_terminal_launch(terminal_pid, &terminals, &launches)
                            .unwrap_err(),
                    )
                },
            )
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
    pub(crate) fn existing_agent_run_terminal_routes_input_through_terminal_queue() {
        #[derive(Resource)]
        struct Input {
            process_id: ProcessId,
            launch: TerminalLaunch,
        }

        #[derive(Resource, Default)]
        struct Captured(Vec<vmux_terminal::TerminalReinputRequest>);

        fn emit(
            input: Res<Input>,
            mut writer: MessageWriter<vmux_terminal::TerminalReinputRequest>,
        ) {
            queue_terminal_run_command_input(
                &mut writer,
                input.process_id,
                "pwd",
                Some("tok4"),
                &input.launch,
            );
        }

        fn capture(
            mut reader: MessageReader<vmux_terminal::TerminalReinputRequest>,
            mut captured: ResMut<Captured>,
        ) {
            captured.0.extend(reader.read().cloned());
        }

        let process_id = ProcessId::new();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_terminal::TerminalReinputRequest>()
            .insert_resource(Input {
                process_id,
                launch: TerminalLaunch {
                    command: "/usr/local/bin/fish".to_string(),
                    args: vec![],
                    cwd: String::new(),
                    env: vec![],
                    kind: vmux_terminal::launch::TerminalKind::Plain,
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
    pub(crate) struct RunTerminalCandidateInput {
        agent_pane: Entity,
        desired_cwd: PathBuf,
    }

    #[derive(Resource, Default)]
    pub(crate) struct RunTerminalCandidateOutput(Vec<RunTerminalCandidate>);

    pub(crate) fn collect_run_terminal_candidates(
        input: Res<RunTerminalCandidateInput>,
        terminals: Query<
            (Entity, &ProcessId, &TerminalLaunch, Has<AgentRunTerminal>),
            (
                With<Terminal>,
                Without<AgentSession>,
                Without<ProcessExited>,
            ),
        >,
        child_of_q: Query<&ChildOf>,
        tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
        mut out: ResMut<RunTerminalCandidateOutput>,
    ) {
        out.0 = run_terminal_candidates(
            input.agent_pane,
            &terminals,
            &child_of_q,
            &tab_q,
            &seq_q,
            &input.desired_cwd,
        );
    }

    #[test]
    pub(crate) fn run_terminal_candidates_fail_closed_when_agent_tab_missing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_terminal_candidates);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let terminal_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(7), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(terminal_pane)))
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
                kind: vmux_terminal::launch::TerminalKind::Plain,
            },
            ChildOf(stack),
        ));
        let agent_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(9)))
            .id();

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
    pub(crate) fn run_terminal_candidates_require_agent_run_marker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_terminal_candidates);
        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
            .id();
        let desired_cwd = std::env::temp_dir();
        let agent_pid = ProcessId::new();
        let user_pid = ProcessId::new();
        let mut agent_terminal = None;
        for (sequence, pid, agent_run) in [(2, agent_pid, true), (3, user_pid, false)] {
            let pane = app
                .world_mut()
                .spawn((Pane, vmux_layout::pane::SpawnSeq(sequence), ChildOf(tab)))
                .id();
            let stack = app
                .world_mut()
                .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
                .id();
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
                        kind: vmux_terminal::launch::TerminalKind::Plain,
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
    pub(crate) fn run_terminal_candidates_exclude_stale_launch_cwd() {
        let current =
            std::env::temp_dir().join(format!("vmux-current-candidate-{}", std::process::id()));
        let stale =
            std::env::temp_dir().join(format!("vmux-stale-candidate-{}", std::process::id()));
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&stale).unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalCandidateOutput>()
            .add_systems(Update, collect_run_terminal_candidates);
        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
            .id();
        let current_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(2), ChildOf(tab)))
            .id();
        let current_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(current_pane)))
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
                kind: vmux_core::terminal::TerminalKind::Plain,
            },
            ChildOf(current_stack),
        ));
        let stale_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(3), ChildOf(tab)))
            .id();
        let stale_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(stale_pane)))
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
                kind: vmux_core::terminal::TerminalKind::Plain,
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
    pub(crate) struct RunTerminalBucketPaneInput {
        agent_pane: Entity,
    }

    #[derive(Resource, Default)]
    pub(crate) struct RunTerminalBucketPaneOutput(Vec<Entity>);

    pub(crate) fn collect_run_terminal_bucket_panes(
        input: Res<RunTerminalBucketPaneInput>,
        child_of_q: Query<&ChildOf>,
        tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
        leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
        pane_children: Query<&Children, With<Pane>>,
        stack_q: Query<Entity, With<vmux_layout::stack::Stack>>,
        page_q: Query<&PageMetadata, With<vmux_layout::stack::Stack>>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
        mut out: ResMut<RunTerminalBucketPaneOutput>,
    ) {
        out.0 = run_terminal_bucket_panes(
            input.agent_pane,
            &child_of_q,
            &tab_q,
            &leaf_panes,
            &pane_children,
            &stack_q,
            &page_q,
            &seq_q,
        )
        .into_iter()
        .map(|candidate| candidate.pane)
        .collect();
    }

    #[test]
    pub(crate) fn run_terminal_bucket_panes_include_pure_terminal_layout_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RunTerminalBucketPaneOutput>()
            .add_systems(Update, collect_run_terminal_bucket_panes);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
            .id();
        let terminal_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(3), ChildOf(tab)))
            .id();
        spawn_stack_in_pane(&mut app, terminal_pane, "vmux://terminal/68001");
        let file_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(9), ChildOf(tab)))
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
    pub(crate) fn pending_run_terminal_spawn_uses_selected_shell() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let pane = Entity::from_bits(20);
        let mut pending_spawns = std::collections::HashMap::new();
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

        let picked = append_pending_run_terminal_input(
            anchor,
            &pending_spawns,
            &mut terminal_spawns,
            &std::env::temp_dir(),
            "pwd",
            Some("tok2"),
        );

        assert_eq!(picked, Some(terminal));
        let input = String::from_utf8(terminal_spawns[0].pending_input.clone().unwrap()).unwrap();
        assert!(input.starts_with("one\r"), "got: {input}");
        assert!(input.contains("try { pwd;"), "got: {input}");
        assert!(input.contains("]6973;tok2;"), "got: {input}");
        assert_eq!(terminal_spawns.len(), 1);
    }

    #[test]
    pub(crate) fn pending_run_terminal_spawn_rejects_changed_cwd() {
        let old_cwd = std::env::temp_dir().join(format!("vmux-old-cwd-{}", std::process::id()));
        let new_cwd = std::env::temp_dir().join(format!("vmux-new-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&old_cwd).unwrap();
        std::fs::create_dir_all(&new_cwd).unwrap();
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let mut pending_spawns = std::collections::HashMap::new();
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

        let picked = append_pending_run_terminal_input(
            anchor,
            &pending_spawns,
            &mut terminal_spawns,
            &new_cwd,
            "pwd",
            Some("tok2"),
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
    pub(crate) struct ReusedRunPaneTouchInput {
        pane: Entity,
    }

    pub(crate) fn touch_reused_run_pane_spawn_seq_test_system(
        input: Res<ReusedRunPaneTouchInput>,
        mut commands: Commands,
        mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
    ) {
        touch_reused_run_pane_spawn_seq(input.pane, &mut commands, &mut spawn_counter, &seq_q);
    }

    #[test]
    pub(crate) fn reusable_run_pane_touch_refreshes_spawn_seq() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<vmux_layout::pane::SpawnCounter>()
            .add_systems(Update, touch_reused_run_pane_spawn_seq_test_system);

        let reused = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(2)))
            .id();
        app.world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(10)));
        app.insert_resource(ReusedRunPaneTouchInput { pane: reused });
        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_layout::pane::SpawnSeq>(reused)
                .unwrap()
                .0,
            11
        );
    }

    #[derive(Resource)]
    pub(crate) struct SplitRunPaneInput {
        pane: Entity,
    }

    #[derive(Resource, Default)]
    pub(crate) struct SplitRunPaneOutput(Option<Entity>);

    pub(crate) fn split_run_pane_test_system(
        input: Res<SplitRunPaneInput>,
        mut out: ResMut<SplitRunPaneOutput>,
        mut commands: Commands,
        mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
        pane_children: Query<&Children, With<Pane>>,
        tab_filter: Query<Entity, With<vmux_layout::stack::Stack>>,
        split_dir_q: Query<&PaneSplit>,
        seq_q: Query<&vmux_layout::pane::SpawnSeq>,
    ) {
        let mut split_batch = std::collections::HashSet::new();
        let target = split_pane_off(
            &mut commands,
            input.pane,
            &vmux_service::protocol::AgentPaneDirection::Bottom,
            false,
            &pane_children,
            &tab_filter,
            &split_dir_q,
            &mut split_batch,
        );
        touch_reused_run_pane_spawn_seq(target, &mut commands, &mut spawn_counter, &seq_q);
        out.0 = Some(target);
    }

    #[test]
    pub(crate) fn split_run_pane_becomes_newest_for_followup_placement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<vmux_layout::pane::SpawnCounter>()
            .init_resource::<SplitRunPaneOutput>()
            .add_systems(Update, split_run_pane_test_system);

        let tab = app
            .world_mut()
            .spawn((vmux_layout::tab::Tab::default(), LastActivatedAt(1)))
            .id();
        let browser_pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(10), ChildOf(tab)))
            .id();
        let browser_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(browser_pane)))
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
            .get::<vmux_layout::pane::SpawnSeq>(terminal_pane)
            .expect("split run target gets fresh spawn seq")
            .0;
        assert!(seq > 10, "split run target must become newest");
    }

    #[test]
    pub(crate) fn run_reuses_existing_terminal_when_region_cache_is_empty() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let regions = AgentTerminalRegions::default();
        let candidates = [RunTerminalCandidate {
            terminal: Entity::from_bits(19),
            pid: terminal,
            stack: Entity::from_bits(21),
            pane: terminal_pane,
            pane_spawn_seq: 7,
        }];

        let picked =
            choose_reusable_run_terminal(anchor, agent_pane, &regions, &candidates).unwrap();

        assert_eq!(picked.pid, terminal);
        assert_eq!(picked.pane, terminal_pane);
    }

    #[test]
    pub(crate) fn run_placement_policy_rejects_override_by_default() {
        let settings = test_settings();
        assert_eq!(
            validate_run_placement_policy(&settings, true),
            Err("run placement overrides are disabled; omit mode, direction, and beside and retry")
        );
    }

    #[test]
    pub(crate) fn run_placement_policy_allows_bare_run() {
        let settings = test_settings();
        assert_eq!(validate_run_placement_policy(&settings, false), Ok(()));
    }

    #[test]
    pub(crate) fn run_placement_policy_honors_user_opt_out() {
        let mut settings = test_settings();
        settings.agent.allow_run_placement_override = true;
        assert_eq!(validate_run_placement_policy(&settings, true), Ok(()));
    }

    #[test]
    pub(crate) fn run_reuses_cached_terminal_before_newer_terminal_candidates() {
        let anchor = ProcessId::new();
        let cached = ProcessId::new();
        let newer = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let cached_pane = Entity::from_bits(20);
        let newer_pane = Entity::from_bits(30);
        let mut regions = AgentTerminalRegions::default();
        regions.run_terminals.insert(anchor, cached);
        regions.run_panes.insert(anchor, cached_pane);
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

        let picked =
            choose_reusable_run_terminal(anchor, agent_pane, &regions, &candidates).unwrap();

        assert_eq!(picked.pid, cached);
        assert_eq!(picked.pane, cached_pane);
    }

    #[derive(Resource)]
    pub(crate) struct ReusedRunTerminalFocusInput {
        candidate: RunTerminalCandidate,
    }

    pub(crate) fn focus_reused_run_terminal_test_system(
        input: Res<ReusedRunTerminalFocusInput>,
        mut commands: Commands,
        child_of_q: Query<&ChildOf>,
        tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
    ) {
        focus_reused_run_terminal(input.candidate, &mut commands, &child_of_q, &tab_q);
    }

    #[test]
    pub(crate) fn reused_run_terminal_focus_activates_stack_pane_and_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, focus_reused_run_terminal_test_system);
        let tab = app
            .world_mut()
            .spawn((vmux_layout::tab::Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                vmux_layout::pane::SpawnSeq(7),
                LastActivatedAt(2),
                ChildOf(tab),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt(3),
                ChildOf(pane),
            ))
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
    pub(crate) fn split_run_stacks_into_cached_terminal_bucket_pane() {
        let anchor = ProcessId::new();
        let terminal = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let mut regions = AgentTerminalRegions::default();
        regions.run_panes.insert(anchor, terminal_pane);
        let candidates = [RunTerminalCandidate {
            terminal: Entity::from_bits(19),
            pid: terminal,
            stack: Entity::from_bits(21),
            pane: terminal_pane,
            pane_spawn_seq: 7,
        }];

        assert_eq!(
            choose_run_terminal_bucket_pane(anchor, agent_pane, &regions, &candidates),
            Some(terminal_pane)
        );
    }

    #[test]
    pub(crate) fn split_run_keeps_cached_terminal_bucket_after_process_exits() {
        let anchor = ProcessId::new();
        let agent_pane = Entity::from_bits(10);
        let terminal_pane = Entity::from_bits(20);
        let mut regions = AgentTerminalRegions::default();
        regions.run_panes.insert(anchor, terminal_pane);
        let candidates = [];

        assert_eq!(
            choose_run_terminal_bucket_pane(anchor, agent_pane, &regions, &candidates),
            Some(terminal_pane)
        );
    }
}
