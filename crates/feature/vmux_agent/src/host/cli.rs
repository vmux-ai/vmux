pub mod claude;
pub mod codex;
pub mod vibe;

#[cfg(test)]
pub(super) const CLAUDE: CliSessionSource = claude::SESSIONS;
#[cfg(test)]
pub(super) const CODEX: CliSessionSource = codex::SESSIONS;
#[cfg(test)]
pub(super) const VIBE: CliSessionSource = vibe::SESSIONS;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;

use crate::manifest::{CliProviderManifest, CliProviderManifests};
use crate::message::Message;
use vmux_core::agent::AgentKind;
use vmux_core::profile::mcp_credentials::McpCredentialAccess;
use vmux_core::terminal::TerminalLaunch;

use crate::host::launch::{
    AgentLaunchPolicy, AgentLaunchPolicyQuery, AgentLaunchRequest, AgentRestartRequest,
    CliLaunchProvider, PreparedAgentLaunch,
};
use crate::host::spawn::{AgentLaunchTask, AgentRestartTask, PrepareAgentLaunchSet};

pub(super) struct CliPlugin;

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            CliLaunchPlugin,
            vibe::VibeCliPlugin,
            claude::ClaudeCliPlugin,
            codex::CodexCliPlugin,
        ));
    }
}

pub(super) struct CliLaunchPlugin;

impl Plugin for CliLaunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                prepare_launches::<vibe::VibeLaunch>,
                prepare_launches::<claude::ClaudeLaunch>,
                prepare_launches::<codex::CodexLaunch>,
                prepare_restarts::<vibe::VibeLaunch>,
                prepare_restarts::<claude::ClaudeLaunch>,
                prepare_restarts::<codex::CodexLaunch>,
            )
                .in_set(PrepareAgentLaunchSet),
        );
    }
}

fn prepare_launches<P: CliLaunchProvider>(
    requests: Query<(Entity, &AgentLaunchRequest), Added<AgentLaunchRequest>>,
    policy: AgentLaunchPolicyQuery,
    providers: Single<&CliProviderManifests>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let policy = policy.snapshot();
    let provider = providers.get(P::KIND).clone();
    for (entity, request) in &requests {
        if request.kind != P::KIND {
            continue;
        }
        let request = request.clone();
        let policy = policy.clone();
        let provider = provider.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = prepare_launch::<P>(&request, &policy, &provider);
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).insert(AgentLaunchTask(task));
    }
}

fn prepare_launch<P: CliLaunchProvider>(
    request: &AgentLaunchRequest,
    policy: &AgentLaunchPolicy,
    provider: &CliProviderManifest,
) -> Result<PreparedAgentLaunch, String> {
    if request.kind != P::KIND {
        return Err(format!(
            "CLI launch provider mismatch: requested {:?}, prepared {:?}",
            request.kind,
            P::KIND
        ));
    }
    static ACCESS: OnceLock<Mutex<()>> = OnceLock::new();
    let _preparation = ACCESS
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?;
    for _ in 0..3 {
        let mcp_revision = McpCredentialAccess::stable_revision()?;
        let mcp_cfg =
            crate::mcp::McpLaunchSpec::cli(&request.cwd, request.anchor, P::KIND, &request.shell)
                .resolve()?;
        if let Err(error) = vmux_core::knowledge::sync_external_agent_configs() {
            bevy::log::warn!("external agent Knowledge sync failed: {error}");
        }
        P::prepare(&mcp_cfg, provider);
        let effort_key = format!("cli:{}", P::KIND.as_url_segment());
        let mut args = match request
            .effort
            .as_deref()
            .filter(|level| vmux_core::agent::effort_levels(&effort_key).contains(level))
        {
            Some(level) => P::effort_arguments(level),
            None => Vec::new(),
        };
        if let Some(model) = request.model.as_deref().filter(|model| !model.is_empty()) {
            args.extend(P::model_arguments(model));
        }
        args.extend(P::policy_arguments(policy, provider));
        args.extend(P::arguments(
            &mcp_cfg,
            request.session_id.as_deref(),
            provider,
        ));
        let mut env: Vec<(String, String)> = std::env::vars().collect();
        env.extend(P::environment(&mcp_cfg, provider));
        if let Some(model) = request.model.as_deref().filter(|model| !model.is_empty()) {
            env.extend(P::model_environment(model));
        }
        env.push(("VMUX_ANCHOR".to_string(), request.anchor.to_string()));
        if McpCredentialAccess::revision() != mcp_revision {
            continue;
        }
        return Ok(PreparedAgentLaunch {
            launch: TerminalLaunch {
                command: request.executable.to_string_lossy().to_string(),
                args,
                cwd: request.cwd.to_string_lossy().to_string(),
                env,
                kind: P::KIND.into(),
            },
            mcp_revision,
        });
    }
    Err("MCP configuration changed repeatedly while preparing the agent".to_string())
}

fn prepare_restarts<P: CliLaunchProvider>(
    requests: Query<(Entity, &AgentRestartRequest), Added<AgentRestartRequest>>,
    policy: AgentLaunchPolicyQuery,
    providers: Single<&CliProviderManifests>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let policy = policy.snapshot();
    let provider = providers.get(P::KIND).clone();
    for (entity, request) in &requests {
        if request.kind != P::KIND {
            continue;
        }
        let request = request.clone();
        let policy = policy.clone();
        let provider = provider.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = prepare_restart::<P>(&request, &policy, &provider);
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).insert(AgentRestartTask(task));
    }
}

fn prepare_restart<P: CliLaunchProvider>(
    request: &AgentRestartRequest,
    policy: &AgentLaunchPolicy,
    provider: &CliProviderManifest,
) -> Result<PreparedAgentLaunch, String> {
    if request.kind != P::KIND {
        return Err(format!(
            "CLI restart provider mismatch: requested {:?}, prepared {:?}",
            request.kind,
            P::KIND
        ));
    }
    for _ in 0..3 {
        let mcp_revision = McpCredentialAccess::stable_revision()?;
        let mcp_cfg = crate::mcp::McpLaunchSpec::cli(
            Path::new(&request.launch.cwd),
            request.anchor,
            P::KIND,
            &request.shell,
        )
        .resolve()?;
        let mut args = P::policy_arguments(policy, provider);
        args.extend(P::arguments(
            &mcp_cfg,
            request.session_id.as_deref(),
            provider,
        ));
        let fresh = P::environment(&mcp_cfg, provider);
        let fresh_keys: HashSet<String> = fresh.iter().map(|(key, _)| key.clone()).collect();
        let mut env: Vec<(String, String)> = request
            .launch
            .env
            .iter()
            .filter(|(key, _)| {
                !fresh_keys.contains(key)
                    && !crate::managed_mcp::McpAuthorization::is_environment_variable(key)
            })
            .cloned()
            .collect();
        env.extend(fresh);
        if McpCredentialAccess::revision() != mcp_revision {
            continue;
        }
        return Ok(PreparedAgentLaunch {
            launch: TerminalLaunch {
                command: request.launch.command.clone(),
                args,
                cwd: request.launch.cwd.clone(),
                env,
                kind: P::KIND.into(),
            },
            mcp_revision,
        });
    }
    Err("MCP configuration changed repeatedly while preparing the agent restart".to_string())
}

#[cfg(test)]
pub(super) fn prepare_restart_for_test<P: CliLaunchProvider>(
    request: &AgentRestartRequest,
) -> Result<PreparedAgentLaunch, String> {
    prepare_restart::<P>(
        request,
        &AgentLaunchPolicy::default(),
        &CliProviderManifest::bundled(P::KIND),
    )
}

#[derive(Component)]
pub(super) struct VibeCli;

#[derive(Component)]
pub(super) struct ClaudeCli;

#[derive(Component)]
pub(super) struct CodexCli;

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CliModelCatalog {
    pub selected: String,
    pub models: Vec<vmux_api::room::ModelOptionEntry>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct CliSessionRoot(pub(super) PathBuf);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumableSession {
    pub kind: AgentKind,
    pub sid: String,
    pub cwd: PathBuf,
    pub transcript: PathBuf,
    pub mtime: SystemTime,
    pub title: String,
    pub latest: String,
    pub cross_runtime: bool,
}

impl ResumableSession {
    pub(crate) fn newest_unique(mut sessions: Vec<Self>) -> Vec<Self> {
        sessions.sort_by_key(|session| std::cmp::Reverse(session.mtime));
        let mut seen = HashSet::new();
        sessions.retain(|session| seen.insert((session.kind, session.sid.clone())));
        sessions
    }
}

pub(crate) struct PromptHistory;

impl PromptHistory {
    const KEEP: usize = 200;
    const BUDGET: u64 = 1024 * 1024;

    pub(crate) fn lines_of(path: &Path) -> Vec<String> {
        SessionTail::tail_of(path, Self::BUDGET)
    }

    pub(crate) fn recent(spoken: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut history = Vec::new();
        for text in spoken.into_iter().rev() {
            if text.trim().is_empty() || !seen.insert(text.clone()) {
                continue;
            }
            history.push(text);
            if history.len() == Self::KEEP {
                break;
            }
        }
        history.reverse();
        history
    }
}

pub(crate) struct SameProject;

impl SameProject {
    pub(crate) fn covers(entry: &str, cwd: &Path) -> bool {
        let entry = Path::new(entry);
        entry.starts_with(cwd) || cwd.starts_with(entry)
    }
}

pub(crate) struct SessionTail;

impl SessionTail {
    const BUDGET: u64 = 256 * 1024;

    pub(crate) fn lines_of(path: &Path) -> Vec<String> {
        Self::tail_of(path, Self::BUDGET)
    }

    fn tail_of(path: &Path, budget: u64) -> Vec<String> {
        use std::io::{Read, Seek, SeekFrom};

        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(end) = file.seek(SeekFrom::End(0)) else {
            return Vec::new();
        };
        let from = end.saturating_sub(budget);
        if file.seek(SeekFrom::Start(from)).is_err() {
            return Vec::new();
        }
        let mut read = Vec::new();
        if file.read_to_end(&mut read).is_err() {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&read);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if from > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        lines
    }
}

pub(crate) fn lines_skipping_invalid_utf8<R: std::io::BufRead>(
    reader: R,
) -> impl Iterator<Item = String> {
    reader
        .lines()
        .map_while(|line| match line {
            Ok(line) => Some(Some(line)),
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => Some(None),
            Err(_) => None,
        })
        .flatten()
}

#[derive(Component, Clone, Copy)]
pub struct CliSessionSource {
    pub kind: AgentKind,
    pub list_sessions: fn() -> Vec<ResumableSession>,
    pub load_transcript: fn(&str) -> Result<Vec<Message>, String>,
}

#[derive(Component, Clone, Copy)]
pub struct CliPromptHistory(pub fn(&Path) -> Vec<String>);

#[cfg(test)]
mod tests {
    use super::PromptHistory;

    #[test]
    fn a_worktree_and_the_repo_it_came_from_share_a_history() {
        use super::SameProject;
        use std::path::Path;

        let repo = "/w/vmux-cloud";
        let tree = Path::new("/w/vmux-cloud/.worktrees/vmx-198");

        assert!(
            SameProject::covers(repo, tree),
            "prompts typed in the repo are the same work as prompts typed in its worktree, and \
             a worktree that has only just been made would otherwise recall nothing"
        );
        assert!(SameProject::covers(
            "/w/vmux-cloud/.worktrees/vmx-198",
            Path::new(repo)
        ));
        assert!(
            !SameProject::covers("/w/vmux-cloud-2", tree),
            "a sibling whose name merely starts the same is a different project"
        );
    }

    #[test]
    fn history_ends_with_the_newest_prompt_and_keeps_one_of_each() {
        let spoken = vec![
            "oldest".to_string(),
            "repeated".to_string(),
            "  ".to_string(),
            "repeated".to_string(),
            "newest".to_string(),
        ];

        assert_eq!(
            PromptHistory::recent(spoken),
            vec![
                "oldest".to_string(),
                "repeated".to_string(),
                "newest".to_string()
            ],
            "the reader presses up expecting what they typed last, so the newest entry has to \
             be the one the walker reaches first; a duplicate keeps only its latest place, and \
             blank lines are not prompts"
        );
    }
}
