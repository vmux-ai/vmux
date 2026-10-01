use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};

use super::project::SpaceProjects;
use super::{AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktreeOnBranch};
use vmux_api::protocol::{AgentRequest, AgentRequestId};
use vmux_chat::event::{
    ChatBranch, ChatBranchesRequest, ChatGoToBranch, ChatSelectWorkspace, ComposerContext,
};
use vmux_chat::host::{ChatBranchesProjection, ChatComposerContext, ChatView};
use vmux_ecs::agent::{AgentRequestInput, CommandOrigin};
use vmux_ecs::event::ProjectRow;
use vmux_ecs::page::PageReady;
use vmux_git::RepoInfoCache;
use vmux_git::worktree::RepoInfo;
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
use vmux_session::AcpSession;
use vmux_session::AgentApprovalPolicy;

use bevy::tasks::futures_lite::future;

pub(super) struct SpaceComposerPlugin;

impl Plugin for SpaceComposerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_plugins(UiEventPlugin::<(
                ChatSelectWorkspace,
                ChatBranchesRequest,
                ChatGoToBranch,
            )>::default())
            .add_observer(chat_select_workspace)
            .add_observer(chat_branches_request)
            .add_observer(chat_go_to_branch)
            .add_systems(Update, (push_context_to_page, drain_branch_reads));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposerContextInput {
    cwd: std::path::PathBuf,
    workspace_selected: bool,
    worktree: Option<TabWorktree>,
    can_manage_workspace: bool,
    auto_allow_count: u32,
    projects: Vec<ProjectRow>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct ComposerProjection<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            Option<&'static AcpSession>,
            Option<&'static AgentApprovalPolicy>,
        ),
    >,
    child_of: Query<'w, 's, &'static ChildOf>,
    tabs: Query<
        'w,
        's,
        (
            &'static Tab,
            Option<&'static TabWorkspace>,
            Option<&'static TabWorktree>,
        ),
    >,
    repo_info: Option<Single<'w, 's, &'static mut RepoInfoCache>>,
    space_projects: SpaceProjects<'w, 's>,
}

impl ComposerProjection<'_, '_> {
    fn context(&mut self, stack: Entity) -> Option<ComposerContext> {
        let (acp, policy) = self.sessions.get(stack).ok()?;
        let mut input = ComposerContextInput::at(stack, acp, policy, &self.child_of, &self.tabs);
        input.projects = self.space_projects.rows(stack);
        let info = if input.cwd.as_os_str().is_empty() {
            None
        } else {
            self.repo_info
                .as_mut()
                .and_then(|cache| cache.bypass_change_detection().lookup(&input.cwd))
        };
        Some(input.context(info.as_ref()))
    }
}

fn push_context_to_page(
    mut views: Query<(Entity, &ChildOf, Ref<PageReady>, &mut ChatComposerContext), With<ChatView>>,
    browsers: NonSend<Browsers>,
    mut projection: ComposerProjection,
    mut commands: Commands,
) {
    for (webview, parent, ready, mut current) in &mut views {
        if !browsers.can_emit_to(&webview) {
            continue;
        }
        let stack = parent.parent();
        let Some(context) = projection.context(stack) else {
            continue;
        };
        let changed = current.0 != context;
        if changed {
            current.0.clone_from(&context);
        }
        if changed || ready.is_changed() {
            commands.trigger(
                vmux_ecs::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                    webview, &context,
                ),
            );
        }
    }
}

impl ComposerContextInput {
    fn at(
        stack: Entity,
        acp: Option<&AcpSession>,
        policy: Option<&AgentApprovalPolicy>,
        child_of: &Query<&ChildOf>,
        tabs: &Query<(&Tab, Option<&TabWorkspace>, Option<&TabWorktree>)>,
    ) -> Self {
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
        Self {
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

    fn context(&self, info: Option<&RepoInfo>) -> ComposerContext {
        let is_git_repo =
            info.is_some() || self.worktree.is_some() || self.cwd.join(".git").exists();
        let branch = info
            .map(|info| info.branch.clone())
            .filter(|branch| !branch.is_empty())
            .or_else(|| {
                self.worktree
                    .as_ref()
                    .map(|worktree| worktree.branch.clone())
            })
            .unwrap_or_default();
        let workspace_name = match info {
            Some(info) => info.project_name(),
            None => self
                .cwd
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| self.cwd.to_string_lossy().into_owned()),
        };
        ComposerContext {
            cwd: self.cwd.to_string_lossy().into_owned(),
            workspace_name,
            workspace_selected: self.workspace_selected,
            is_git_repo,
            is_worktree: info.is_some_and(|info| info.is_worktree) || self.worktree.is_some(),
            branch,
            base_ref: self
                .worktree
                .as_ref()
                .map(|worktree| worktree.base_ref.clone())
                .unwrap_or_default(),
            uncommitted: info.map(|info| info.uncommitted).unwrap_or_default(),
            ahead: info.map(|info| info.ahead).unwrap_or_default(),
            can_manage_workspace: self.can_manage_workspace,
            auto_allow_count: self.auto_allow_count,
            projects: self.projects.clone(),
        }
    }
}

#[derive(Component)]
struct BranchRead {
    webview: Entity,
    request_id: u64,
    project: String,
    task: bevy::tasks::Task<Vec<ChatBranch>>,
}

fn chat_branches_request(
    trigger: On<UiInput<ChatBranchesRequest>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut projections: Query<(&ChatComposerContext, &mut ChatBranchesProjection), With<ChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((context, mut projection)) = projections.get_mut(webview) else {
        return;
    };
    let project = context.0.cwd.trim().to_string();
    if project.is_empty() {
        return;
    }
    let request_id = projection.start(project.clone());
    commands.trigger(
        vmux_ecs::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview,
            &projection.0,
        ),
    );
    let root = std::path::PathBuf::from(&project);
    let wake = vmux_ecs::host::wake::Wake::from_resource(proxy);
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let _wake = wake;
        let Ok(holders) = vmux_git::worktree::branch_holders(&root) else {
            return Vec::new();
        };
        let mut branches = Vec::with_capacity(holders.len());
        for holder in holders {
            let checkout = holder.checkout_path();
            let label = holder.checkout_label();
            branches.push(ChatBranch {
                branch: holder.branch,
                checkout,
                label,
                insertions: holder.change.insertions,
                deletions: holder.change.deletions,
            });
        }
        branches
    });
    commands.spawn(BranchRead {
        webview,
        request_id,
        project,
        task,
    });
}

fn drain_branch_reads(
    mut reads: Query<(Entity, &mut BranchRead)>,
    mut projections: Query<&mut ChatBranchesProjection, With<ChatView>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut read) in &mut reads {
        let Some(branches) = future::block_on(future::poll_once(&mut read.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if !browsers.can_emit_to(&read.webview) {
            continue;
        }
        let Ok(mut projection) = projections.get_mut(read.webview) else {
            continue;
        };
        if !projection.finish(read.request_id, &read.project, branches) {
            continue;
        }
        commands.trigger(
            vmux_ecs::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                read.webview,
                &projection.0,
            ),
        );
    }
}

fn chat_go_to_branch(
    trigger: On<UiInput<ChatGoToBranch>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentRequestInput>,
) {
    let evt = &trigger.event().payload;
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    let checkout = evt.checkout.trim();
    let request = if checkout.is_empty() {
        let project = evt.project.trim();
        AgentRequest::encode(&AgentCreateWorktreeOnBranch {
            anchor: session.anchor,
            branch: evt.branch.clone(),
            project: (!project.is_empty()).then(|| project.to_string()),
        })
    } else {
        AgentRequest::encode(&AgentChooseWorkspaceAtPath {
            anchor: session.anchor,
            path: checkout.to_string(),
        })
    };
    let Ok(request) = request else {
        return;
    };
    requests.write(AgentRequestInput {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        request,
    });
}

fn chat_select_workspace(
    trigger: On<UiInput<ChatSelectWorkspace>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentRequestInput>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    let Ok(request) = AgentRequest::encode(&AgentChooseWorkspace {
        anchor: session.anchor,
    }) else {
        return;
    };
    requests.write(AgentRequestInput {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        request,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_projection_rejects_stale_results() {
        let mut projection = ChatBranchesProjection::default();
        let stale = projection.start("/one".into());
        let current = projection.start("/two".into());
        assert!(!projection.finish(stale, "/one", Vec::new()));
        assert!(projection.0.loading);
        assert!(projection.finish(current, "/two", Vec::new()));
        assert!(!projection.0.loading);
    }

    #[test]
    fn composer_workspace_selection_dispatches_for_current_session() {
        let mut app = App::new();
        app.add_message::<AgentRequestInput>()
            .add_observer(chat_select_workspace);
        let anchor = vmux_ecs::ProcessId::new();
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

        app.world_mut().trigger(UiInput {
            webview,
            payload: ChatSelectWorkspace,
        });
        let requests = app
            .world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert!(matches!(requests[0].origin, CommandOrigin::User));
        assert_eq!(
            requests[0]
                .decode::<AgentChooseWorkspace>()
                .unwrap()
                .unwrap()
                .anchor,
            anchor
        );
    }

    #[test]
    fn an_unheld_branch_names_the_project_it_was_picked_from() {
        let mut app = App::new();
        app.add_message::<AgentRequestInput>()
            .add_observer(chat_go_to_branch);
        let anchor = vmux_ecs::ProcessId::new();
        let stack = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp/here".into(),
                anchor,
                resume: None,
            })
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ChatGoToBranch {
                project: "/tmp/elsewhere".into(),
                branch: "feature/x".into(),
                checkout: String::new(),
            },
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        let command = requests[0]
            .decode::<AgentCreateWorktreeOnBranch>()
            .unwrap()
            .expect("an unheld branch creates a worktree");
        assert_eq!(
            command.project.as_deref(),
            Some("/tmp/elsewhere"),
            "without it the worktree lands under whatever project the tab happens to hold"
        );
    }
}
