use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

use super::AgentChatView;
use crate::events::{AgentCommandRequest, CommandOrigin};
use vmux_chat::event::{
    CHAT_PROJECT_BRANCHES_EVENT, COMPOSER_CONTEXT_EVENT, ChatBranch, ChatBranchesRequest,
    ChatCreateWorktree, ChatGoToBranch, ChatProjectBranches, ChatSelectWorkspace, ComposerContext,
};
use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentRequestId};
use vmux_session::AcpSession;
use vmux_session::AgentApprovalPolicy;

pub(super) struct ChatWorkspacePlugin;

impl Plugin for ChatWorkspacePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(
            ChatSelectWorkspace,
            ChatCreateWorktree,
            ChatBranchesRequest,
            ChatGoToBranch,
        )>::for_hosts(&["agent", "start"]))
            .add_observer(on_chat_select_workspace)
            .add_observer(on_chat_create_worktree)
            .add_observer(on_chat_branches_request)
            .add_observer(on_chat_go_to_branch)
            .add_systems(Update, (push_composer_context_to_page, drain_branch_reads));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposerContextInput {
    cwd: std::path::PathBuf,
    workspace_selected: bool,
    worktree: Option<vmux_layout::tab::TabWorktree>,
    can_manage_workspace: bool,
    auto_allow_count: u32,
    projects: Vec<vmux_core::event::ProjectRow>,
}

use vmux_space::SpaceProjects;

#[derive(Default)]
struct ComposerContextCache {
    entries: std::collections::HashMap<Entity, ComposerContextCacheEntry>,
}

struct ComposerContextCacheEntry {
    input: ComposerContextInput,
    context: ComposerContext,
}

#[allow(clippy::too_many_arguments)]
fn push_composer_context_to_page(
    views: Query<(Entity, &ChildOf, Ref<vmux_core::page::PageReady>), With<AgentChatView>>,
    sessions: Query<(Option<&AcpSession>, Option<&AgentApprovalPolicy>)>,
    child_of: Query<&ChildOf>,
    tabs: Query<(
        &vmux_layout::tab::Tab,
        Option<&vmux_layout::tab::TabWorkspace>,
        Option<&vmux_layout::tab::TabWorktree>,
    )>,
    browsers: NonSend<Browsers>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
    space_projects: SpaceProjects,
    mut cache: Local<ComposerContextCache>,
    mut commands: Commands,
) {
    let live_views = views
        .iter()
        .map(|(webview, _, _)| webview)
        .collect::<std::collections::HashSet<_>>();
    cache
        .entries
        .retain(|webview, _| live_views.contains(webview));
    for (webview, parent, ready) in &views {
        if !browsers.can_emit_to(&webview) {
            continue;
        }
        let stack = parent.parent();
        let Ok((acp, policy)) = sessions.get(stack) else {
            continue;
        };
        let mut input = composer_context_input(stack, acp, policy, &child_of, &tabs);
        input.projects = space_projects.rows(stack);
        let info = (!input.cwd.as_os_str().is_empty())
            .then(|| {
                repo_info
                    .as_mut()
                    .and_then(|cache| cache.bypass_change_detection().get(&input.cwd))
            })
            .flatten();
        let context = composer_context_from_input(&input, info.as_ref());
        let changed = cache
            .entries
            .get(&webview)
            .is_none_or(|entry| entry.input != input || entry.context != context);
        if changed || ready.is_changed() {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                webview,
                COMPOSER_CONTEXT_EVENT,
                &context,
            ));
        }
        cache
            .entries
            .insert(webview, ComposerContextCacheEntry { input, context });
    }
}

fn composer_context_input(
    stack: Entity,
    acp: Option<&AcpSession>,
    policy: Option<&AgentApprovalPolicy>,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(
        &vmux_layout::tab::Tab,
        Option<&vmux_layout::tab::TabWorkspace>,
        Option<&vmux_layout::tab::TabWorktree>,
    )>,
) -> ComposerContextInput {
    let mut current = stack;
    let mut tab_dir = None;
    let mut workspace_selected = false;
    let mut worktree = None;
    loop {
        if let Ok((tab, workspace, managed)) = tabs.get(current) {
            tab_dir = tab.startup_dir.as_ref().map(std::path::PathBuf::from);
            workspace_selected = workspace.is_some() || tab.startup_dir.is_some();
            worktree = managed.cloned();
            break;
        }
        let Ok(parent) = child_of.get(current) else {
            break;
        };
        current = parent.parent();
    }
    ComposerContextInput {
        cwd: tab_dir
            .or_else(|| acp.map(|session| session.cwd.clone()))
            .unwrap_or_default(),
        workspace_selected,
        worktree,
        can_manage_workspace: acp.is_some(),
        auto_allow_count: policy
            .map(|policy| u32::try_from(policy.auto.len()).unwrap_or(u32::MAX))
            .unwrap_or_default(),
        projects: Vec::new(),
    }
}

fn composer_context_from_input(
    input: &ComposerContextInput,
    info: Option<&vmux_git::worktree::RepoInfo>,
) -> ComposerContext {
    let is_git_repo = info.is_some() || input.worktree.is_some() || input.cwd.join(".git").exists();
    let branch = info
        .map(|info| info.branch.clone())
        .filter(|branch| !branch.is_empty())
        .or_else(|| {
            input
                .worktree
                .as_ref()
                .map(|worktree| worktree.branch.clone())
        })
        .unwrap_or_default();
    let workspace_name = input
        .cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| input.cwd.to_string_lossy().into_owned());
    ComposerContext {
        cwd: input.cwd.to_string_lossy().into_owned(),
        workspace_name,
        workspace_selected: input.workspace_selected,
        is_git_repo,
        is_worktree: info.is_some_and(|info| info.is_worktree) || input.worktree.is_some(),
        branch,
        base_ref: input
            .worktree
            .as_ref()
            .map(|worktree| worktree.base_ref.clone())
            .unwrap_or_default(),
        uncommitted: info.map(|info| info.uncommitted).unwrap_or_default(),
        ahead: info.map(|info| info.ahead).unwrap_or_default(),
        can_manage_workspace: input.can_manage_workspace,
        auto_allow_count: input.auto_allow_count,
        projects: input.projects.clone(),
    }
}

#[derive(Component)]
struct BranchRead {
    webview: Entity,
    project: String,
    task: bevy::tasks::Task<Vec<ChatBranch>>,
}

fn on_chat_branches_request(trigger: On<BinReceive<ChatBranchesRequest>>, mut commands: Commands) {
    let webview = trigger.event().webview;
    let project = trigger.event().payload.project.trim().to_string();
    if project.is_empty() {
        return;
    }
    let root = std::path::PathBuf::from(&project);
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let Ok(holders) = vmux_git::worktree::branch_holders(&root) else {
            return Vec::new();
        };
        holders
            .into_iter()
            .map(|holder| {
                let checkout = holder
                    .checkout
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let label = holder
                    .checkout
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                ChatBranch {
                    branch: holder.branch,
                    checkout,
                    label,
                }
            })
            .collect()
    });
    commands.spawn(BranchRead {
        webview,
        project,
        task,
    });
}

fn drain_branch_reads(
    mut reads: Query<(Entity, &mut BranchRead)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    use bevy::tasks::futures_lite::future;

    for (entity, mut read) in &mut reads {
        let Some(branches) = future::block_on(future::poll_once(&mut read.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if !browsers.can_emit_to(&read.webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            read.webview,
            CHAT_PROJECT_BRANCHES_EVENT,
            &ChatProjectBranches {
                project: read.project.clone(),
                branches,
            },
        ));
    }
}

fn on_chat_go_to_branch(
    trigger: On<BinReceive<ChatGoToBranch>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    let evt = &trigger.event().payload;
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    let checkout = evt.checkout.trim();
    let command = if checkout.is_empty() {
        ServiceAgentCommand::CreateWorktreeOnBranch {
            anchor: session.anchor,
            branch: evt.branch.clone(),
        }
    } else {
        ServiceAgentCommand::ChooseWorkspaceAtPath {
            anchor: session.anchor,
            path: checkout.to_string(),
        }
    };
    requests.write(AgentCommandRequest {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        command,
    });
}

fn on_chat_select_workspace(
    trigger: On<BinReceive<ChatSelectWorkspace>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    requests.write(AgentCommandRequest {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        command: ServiceAgentCommand::ChooseWorkspace {
            anchor: session.anchor,
        },
    });
}

fn on_chat_create_worktree(
    trigger: On<BinReceive<ChatCreateWorktree>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    requests.write(AgentCommandRequest {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        command: ServiceAgentCommand::CreateWorktree {
            anchor: session.anchor,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_workspace_controls_dispatch_for_current_session() {
        let mut app = App::new();
        app.add_message::<AgentCommandRequest>()
            .add_observer(on_chat_select_workspace)
            .add_observer(on_chat_create_worktree);
        let anchor = vmux_core::ProcessId::new();
        let stack = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor,
                resume: None,
            })
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatSelectWorkspace,
        });
        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatCreateWorktree,
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 2);
        assert!(matches!(requests[0].origin, CommandOrigin::User));
        assert!(matches!(
            requests[0].command,
            ServiceAgentCommand::ChooseWorkspace { anchor: got } if got == anchor
        ));
        assert!(matches!(
            requests[1].command,
            ServiceAgentCommand::CreateWorktree { anchor: got } if got == anchor
        ));
    }
}
