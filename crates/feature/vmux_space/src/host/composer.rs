use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};

use super::project::{ActiveProjectRows, SpaceProjects};
use super::{AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktreeOnBranch};
use vmux_api::protocol::{AgentRequest, AgentRequestId};
use vmux_api::service::ServiceMessageSet;
use vmux_ecs::agent::{AgentRequestInput, CommandOrigin};
use vmux_ecs::event::ProjectRow;
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    Cwd, EntityTarget, IdentityIndex, IdentityIndexPlugin, IdentityIndexSet, KeyboardOwner,
    ProcessAnchor,
};
use vmux_git::worktree::RepoInfo;
use vmux_git::{GitRuntime, GitTask, RepoInfoCache};
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
use vmux_session::event::{
    ChatBranch, ChatBranchesRequest, ChatGoToBranch, ChatSelectWorkspace, ComposerContext,
};
use vmux_session::host::{ChatBranchesProjection, ChatComposerContext, SessionView};
use vmux_session::{ApprovalPolicy, Route, Session, SessionId};

pub(super) struct SpaceComposerPlugin;

impl Plugin for SpaceComposerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<IdentityIndexPlugin<SessionId, Session>>() {
            app.add_plugins(IdentityIndexPlugin::<SessionId, Session>::default());
        }
        let index = IdentityIndexSet::of::<SessionId, Session>();
        app.configure_sets(Update, index.after(ServiceMessageSet))
            .add_message::<AgentRequestInput>()
            .init_resource::<GitRuntime>()
            .add_plugins(UiEventPlugin::<(
                ChatSelectWorkspace,
                ChatBranchesRequest,
                ChatGoToBranch,
            )>::default())
            .add_observer(chat_select_workspace)
            .add_observer(chat_branches_request)
            .add_observer(chat_go_to_branch)
            .add_systems(
                Update,
                (push_context_to_page.after(index), drain_branch_reads),
            );
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
    identities: Res<'w, IdentityIndex<SessionId, Session>>,
    sessions: Query<
        'w,
        's,
        (
            Entity,
            &'static SessionId,
            Option<&'static Cwd>,
            Option<&'static ApprovalPolicy>,
        ),
        With<Session>,
    >,
    pages: Query<'w, 's, &'static vmux_ecs::PageMetadata>,
    targets: Query<'w, 's, &'static EntityTarget<Session>>,
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
    active_project_rows: Res<'w, ActiveProjectRows>,
}

impl ComposerProjection<'_, '_> {
    fn context(&mut self, stack: Entity, webview: Entity, active: bool) -> Option<ComposerContext> {
        let route = self
            .pages
            .get(webview)
            .ok()
            .and_then(|page| Route::parse(&page.url));
        let session = match route {
            Some(Route::Session(id)) => {
                let Ok(Some(session)) = self.identities.entity(&id) else {
                    return None;
                };
                session
            }
            _ => self.targets.get(stack).ok()?.entity(),
        };
        let (_, _, cwd, policy) = self.sessions.get(session).ok()?;
        let observer = active.then_some(webview);
        let mut input = self.input(stack, cwd, policy);
        input.projects = self
            .space_projects
            .selector_rows(stack, &self.active_project_rows);
        let info = if input.cwd.as_os_str().is_empty() {
            if let Some(observer) = observer
                && let Some(cache) = self.repo_info.as_mut()
            {
                cache.bypass_change_detection().release(observer);
            }
            None
        } else {
            self.repo_info.as_mut().and_then(|cache| {
                let cache = cache.bypass_change_detection();
                match observer {
                    Some(observer) => cache.observe_active(observer, &input.cwd),
                    None => cache.snapshot(&input.cwd),
                }
            })
        };
        Some(input.context(info.as_ref()))
    }

    fn input(
        &self,
        stack: Entity,
        cwd: Option<&Cwd>,
        policy: Option<&ApprovalPolicy>,
    ) -> ComposerContextInput {
        let mut current = stack;
        let mut tab_dir = None;
        let mut workspace_selected = false;
        let mut worktree = None;
        loop {
            if let Ok((tab, workspace, managed)) = self.tabs.get(current) {
                tab_dir = tab.startup_dir.as_ref().map(std::path::PathBuf::from);
                workspace_selected = workspace.is_some() || tab.startup_dir.is_some();
                if cwd.is_none_or(|cwd| tab_dir.as_ref() == Some(&cwd.0)) {
                    worktree = managed.cloned();
                }
                break;
            }
            let Ok(parent) = self.child_of.get(current) else {
                break;
            };
            current = parent.parent();
        }
        ComposerContextInput {
            cwd: cwd.map(|cwd| cwd.0.clone()).or(tab_dir).unwrap_or_default(),
            workspace_selected: workspace_selected || cwd.is_some(),
            worktree,
            can_manage_workspace: cwd.is_some(),
            auto_allow_count: policy
                .map(|policy| u32::try_from(policy.auto.len()).unwrap_or(u32::MAX))
                .unwrap_or_default(),
            projects: Vec::new(),
        }
    }
}

fn push_context_to_page(
    mut views: Query<
        (
            Entity,
            &ChildOf,
            Ref<PageReady>,
            Has<KeyboardOwner>,
            &mut ChatComposerContext,
            Option<&vmux_ecs::UiState<vmux_session::state::ChatUiState>>,
        ),
        With<SessionView>,
    >,
    browsers: NonSend<Browsers>,
    mut projection: ComposerProjection,
    mut commands: Commands,
) {
    for (webview, parent, ready, active, mut current, ui) in &mut views {
        if !browsers.can_emit_to(&webview) {
            continue;
        }
        let pending = ui
            .and_then(vmux_ecs::UiState::current)
            .is_none_or(|state| !state.composer_ready);
        if !active && !ready.is_changed() && !pending {
            continue;
        }
        let stack = parent.parent();
        let context = projection.context(stack, webview, active);

        let Some(context) = context else {
            continue;
        };
        let changed = current.0 != context;
        if changed {
            current.0.clone_from(&context);
        }
        if changed || ready.is_changed() || pending {
            commands.trigger(
                vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                    webview, &context,
                ),
            );
        }
    }
}

impl ComposerContextInput {
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
        let directory = info
            .map(|info| info.project_root().to_string_lossy().into_owned())
            .unwrap_or_else(|| self.cwd.to_string_lossy().into_owned());
        let project = ProjectRow::for_directory(&self.projects, &directory);
        let workspace_name = match (project, info) {
            (Some(project), _) => project.label.clone(),
            (_, Some(info)) => info.project_name(),
            (_, None) => self
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
    task: GitTask<Vec<ChatBranch>>,
}

fn chat_branches_request(
    trigger: On<UiInput<ChatBranchesRequest>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    runtime: Res<GitRuntime>,
    mut projections: Query<(&ChatComposerContext, &mut ChatBranchesProjection), With<SessionView>>,
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
        vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
            webview,
            &projection.0,
        ),
    );
    let root = std::path::PathBuf::from(&project);
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let Some(task) = runtime.spawn(wake, move || {
        let Ok(checkout) = vmux_git::worktree::CheckoutInfo::try_from(root.as_path()) else {
            return Vec::new();
        };
        let Ok(holders) = checkout.branch_holders() else {
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
    }) else {
        projection.finish(request_id, &project, Vec::new());
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                webview,
                &projection.0,
            ),
        );
        return;
    };
    commands.spawn(BranchRead {
        webview,
        request_id,
        project,
        task,
    });
}

fn drain_branch_reads(
    mut reads: Query<(Entity, &mut BranchRead)>,
    mut projections: Query<&mut ChatBranchesProjection, With<SessionView>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut read) in &mut reads {
        let Some(branches) = read.task.poll() else {
            continue;
        };
        commands.entity(entity).despawn();
        if !browsers.can_emit_to(&read.webview) {
            continue;
        }
        let Ok(mut projection) = projections.get_mut(read.webview) else {
            continue;
        };
        if !projection.finish(read.request_id, &read.project, branches.unwrap_or_default()) {
            continue;
        }
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                read.webview,
                &projection.0,
            ),
        );
    }
}

fn chat_go_to_branch(
    trigger: On<UiInput<ChatGoToBranch>>,
    child_of: Query<&ChildOf>,
    targets: Query<&EntityTarget<Session>>,
    sessions: Query<&ProcessAnchor, With<Session>>,
    mut requests: MessageWriter<AgentRequestInput>,
) {
    let evt = &trigger.event().payload;
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(target) = targets.get(parent.parent()) else {
        return;
    };
    let Ok(anchor) = sessions.get(target.entity()) else {
        return;
    };
    let checkout = evt.checkout.trim();
    let project = evt.project.trim();
    let request = if !checkout.is_empty() {
        AgentRequest::encode(&AgentChooseWorkspaceAtPath {
            anchor: anchor.0,
            path: checkout.to_string(),
        })
    } else if evt.branch.trim().is_empty() {
        AgentRequest::encode(&AgentChooseWorkspaceAtPath {
            anchor: anchor.0,
            path: project.to_string(),
        })
    } else {
        AgentRequest::encode(&AgentCreateWorktreeOnBranch {
            anchor: anchor.0,
            branch: evt.branch.clone(),
            project: (!project.is_empty()).then(|| project.to_string()),
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
    targets: Query<&EntityTarget<Session>>,
    sessions: Query<&ProcessAnchor, With<Session>>,
    mut requests: MessageWriter<AgentRequestInput>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(target) = targets.get(parent.parent()) else {
        return;
    };
    let Ok(anchor) = sessions.get(target.entity()) else {
        return;
    };
    let Ok(request) = AgentRequest::encode(&AgentChooseWorkspace { anchor: anchor.0 }) else {
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

    #[derive(Resource, Default)]
    struct ContextWrites(usize);

    impl ContextWrites {
        fn record(
            event: On<vmux_ecs::UiStateWrite<vmux_session::state::ChatUiState>>,
            mut writes: ResMut<Self>,
        ) {
            if event.update().composer.is_some() {
                writes.0 += 1;
            }
        }
    }

    struct ComposerPage {
        app: App,
        view: Entity,
    }

    impl ComposerPage {
        fn waiting() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                SpaceComposerPlugin,
                vmux_ecs::UiStatePlugin::<vmux_session::state::ChatUiState>::default(),
            ))
            .init_resource::<ActiveProjectRows>()
            .init_resource::<ContextWrites>()
            .add_observer(ContextWrites::record)
            .add_observer(Self::delivered);
            let stack = app.world_mut().spawn_empty().id();
            let view = app
                .world_mut()
                .spawn((
                    SessionView,
                    ChildOf(stack),
                    PageReady {},
                    vmux_ecs::PageMetadata {
                        url: "vmux://sessions/late-session".to_string(),
                        ..Default::default()
                    },
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                ))
                .id();
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: "vmux://sessions/",
                owns_subtree: true,
                permissions: &["ChatUiState"],
            });
            let mut browsers = Browsers::default();
            browsers.set_externally_hosted(view);
            app.insert_non_send(browsers);
            Self { app, view }
        }

        fn delivered(event: On<bevy_cef::prelude::BinHostEmitEvent>) {
            event.mark_delivered();
        }

        fn bind(&mut self, path: &std::path::Path) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Session,
                    SessionId("late-session".to_string()),
                    Cwd(path.to_path_buf()),
                ))
                .id()
        }

        fn context(&self) -> &ComposerContext {
            let state = self
                .app
                .world()
                .get::<vmux_ecs::UiState<vmux_session::state::ChatUiState>>(self.view)
                .unwrap()
                .current()
                .unwrap();
            assert!(state.composer_ready);
            &state.composer
        }
    }

    #[test]
    fn a_session_composer_finishes_loading_after_binding_missed_its_first_ready_frame() {
        let root = tempfile::tempdir().unwrap();
        let mut page = ComposerPage::waiting();
        page.app.update();
        assert_eq!(page.app.world().resource::<ContextWrites>().0, 0);
        page.bind(root.path());
        page.app.update();
        assert_eq!(page.context().cwd, root.path().to_string_lossy());
        assert!(page.context().workspace_selected);
        assert!(
            !page
                .app
                .world()
                .entity(page.view)
                .contains::<KeyboardOwner>()
        );
        assert_eq!(page.app.world().resource::<ContextWrites>().0, 1);
        page.app.update();
        assert_eq!(page.app.world().resource::<ContextWrites>().0, 1);
    }

    #[test]
    fn ambiguous_session_routes_wait_until_one_identity_remains() {
        let first = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let mut page = ComposerPage::waiting();
        page.bind(first.path());
        let duplicate = page.bind(other.path());
        page.app.update();
        assert_eq!(page.app.world().resource::<ContextWrites>().0, 0);
        page.app.world_mut().despawn(duplicate);
        page.app.update();
        assert_eq!(page.context().cwd, first.path().to_string_lossy());
        assert_eq!(page.app.world().resource::<ContextWrites>().0, 1);
    }

    #[test]
    fn an_unchanged_context_is_republished_when_the_page_has_not_received_it() {
        let root = tempfile::tempdir().unwrap();
        let mut page = ComposerPage::waiting();
        page.bind(root.path());
        page.app.update();
        assert_eq!(page.context().cwd, root.path().to_string_lossy());
        page.app
            .world_mut()
            .entity_mut(page.view)
            .remove::<vmux_ecs::UiState<vmux_session::state::ChatUiState>>();
        page.app.update();
        assert_eq!(page.context().cwd, root.path().to_string_lossy());
        assert_eq!(page.app.world().resource::<ContextWrites>().0, 2);
    }

    #[test]
    fn session_workspace_context_uses_its_own_directory_instead_of_the_tabs_startup_directory() {
        let session_root = tempfile::tempdir().unwrap();
        let tab_root = tempfile::tempdir().unwrap();
        let mut page = ComposerPage::waiting();
        let stack = page.app.world().get::<ChildOf>(page.view).unwrap().parent();
        let tab = page
            .app
            .world_mut()
            .spawn(Tab {
                startup_dir: Some(tab_root.path().display().to_string()),
                ..Default::default()
            })
            .id();
        page.app.world_mut().entity_mut(stack).insert(ChildOf(tab));
        page.app.world_mut().entity_mut(tab).insert(TabWorktree {
            branch: "unrelated-tab-branch".into(),
            ..Default::default()
        });
        page.bind(session_root.path());
        page.app.update();

        assert_eq!(page.context().cwd, session_root.path().to_string_lossy());
        assert!(page.context().workspace_selected);
        assert!(page.context().branch.is_empty());
        assert!(!page.context().is_worktree);
    }

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
        let session = app
            .world_mut()
            .spawn((Session, Cwd("/tmp".into()), ProcessAnchor(anchor)))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
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
        let session = app
            .world_mut()
            .spawn((Session, Cwd("/tmp/here".into()), ProcessAnchor(anchor)))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
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
