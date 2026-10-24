use std::collections::HashSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::bookmark::{
    BookmarkFolderChoice, BookmarkFolderRow, BookmarkNode, BookmarkRow, BookmarkStateEvent,
};
use vmux_api::space::ProjectRowKind;
use vmux_ecs::event::team::{TeamMemberRow, UiTeam};
use vmux_ecs::{Description, EntityTarget};
use vmux_session::{RunState, Session, Stage, StageDefinition, StageId};

use crate::cef::{Browser, LayoutCef};
use crate::event::{
    ActiveSession, ActiveSessionState, ActiveTask, ActiveWorkspaceProject, BookmarkEntryState,
    BookmarkFolderState, BookmarkPinState, BookmarkTreeState, HeaderState, PaneTreeState,
    SideSheetPane, SideSheetState, StackNavigationState, StackNode, StackRevealTarget, TabBoundary,
    TabBoundaryState, TabListState, TabStripRow, TabStripState, UiBookmark,
};
use crate::side_sheet::{SideSheetSections, SideSheetSectionsExpanded};
use crate::state::UiLayout;

pub struct LayoutUiProjectionPlugin;

impl Plugin for LayoutUiProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(capture_tab_list).add_systems(
            Update,
            (
                publish_active_session,
                (
                    project_header,
                    project_tab_strip,
                    publish_header,
                    publish_tab_strip,
                )
                    .chain(),
                (
                    project_side_sheet,
                    project_bookmark_ui,
                    publish_side_sheet,
                    publish_bookmark_ui,
                )
                    .chain(),
            ),
        );
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PaneTreeProjection(pub PaneTreeState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ProjectProjection(pub TabBoundaryState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TeamProjection(pub UiTeam);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct StackProjection(pub StackNavigationState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BookmarkProjection(pub BookmarkStateEvent);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SpacesProjection(pub vmux_ecs::event::space::SpacesListEvent);

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct TabListProjection(TabListState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct HeaderProjection(HeaderState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct TabStripProjection(TabStripState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct SideSheetProjection {
    state: SideSheetState,
    reveal_revision: u64,
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct BookmarkUiProjection(UiBookmark);

impl TabStripProjection {
    fn from_sources(tabs: &TabListState, header: Option<&HeaderState>) -> Self {
        let active_bg_color = header
            .and_then(|header| header.active.as_ref())
            .and_then(|active| active.bg_color.as_ref());
        let mut rows = Vec::with_capacity(tabs.tabs.len());
        let mut order = Vec::with_capacity(tabs.tabs.len());
        for source in &tabs.tabs {
            let mut tab = source.clone();
            if tab.is_active && active_bg_color.is_some() {
                tab.bg_color = active_bg_color.cloned();
            }
            let display_title = if !tab.title.is_empty() {
                tab.title.clone()
            } else {
                tab.name.clone()
            };
            let metadata = vmux_ecs::PageMetadata {
                title: display_title.clone(),
                url: tab.url.clone(),
                icon: tab.icon.clone(),
                bg_color: tab.bg_color.clone(),
            };
            order.push(tab.id.clone());
            rows.push(TabStripRow {
                tab,
                display_title,
                metadata,
            });
        }
        Self(TabStripState {
            drag_region_revision: order.join(":"),
            tabs: rows,
            order,
        })
    }
}

struct BookmarkUiBuilder {
    folders: Vec<BookmarkFolderRow>,
    choices: Vec<BookmarkFolderChoice>,
    rows: Vec<BookmarkTreeState>,
    visited: HashSet<String>,
}

impl BookmarkUiBuilder {
    fn build(bookmarks: &BookmarkStateEvent, active_page: Option<&StackNode>) -> UiBookmark {
        let active_route = active_page.and_then(|page| vmux_api::VmuxRoute::parse(&page.url));
        let mut pins = Vec::with_capacity(bookmarks.pins.len());
        for row in &bookmarks.pins {
            let active = active_route.as_ref().is_some_and(|active| {
                vmux_api::VmuxRoute::parse(&row.metadata.url)
                    .is_some_and(|pin| active.same_page(&pin))
            });
            pins.push(BookmarkPinState {
                row: row.clone(),
                active,
            });
        }

        let mut folders = Vec::new();
        for node in &bookmarks.roots {
            if let BookmarkNode::Folder(folder) = node {
                folders.push(folder.clone());
            }
        }
        let mut builder = Self {
            folders,
            choices: bookmarks.folders.clone(),
            rows: Vec::new(),
            visited: HashSet::new(),
        };
        for node in bookmarks.roots.clone() {
            match node {
                BookmarkNode::Folder(folder) if folder.parent.is_none() => {
                    builder.append_folder(folder, 0);
                }
                BookmarkNode::Entry(row) => builder.append_entry(row, None, 0),
                BookmarkNode::Folder(_) => {}
            }
        }

        UiBookmark {
            pins,
            rows: builder.rows,
            folders: bookmarks.folders.clone(),
            active_page: active_page.map(|page| vmux_ecs::PageMetadata {
                title: page.title.clone(),
                url: page.url.clone(),
                icon: page.icon.clone(),
                bg_color: page.bg_color.clone(),
            }),
        }
    }

    fn append_folder(&mut self, folder: BookmarkFolderRow, depth: u32) {
        if !self.visited.insert(folder.uuid.clone()) {
            return;
        }
        let mut child_folders = Vec::new();
        for candidate in &self.folders {
            if candidate.parent.as_deref() == Some(folder.uuid.as_str()) {
                child_folders.push(candidate.clone());
            }
        }
        let child_count = child_folders.len().saturating_add(folder.children.len()) as u32;
        let move_targets = self.folder_move_targets(&folder.uuid);
        self.rows
            .push(BookmarkTreeState::Folder(BookmarkFolderState {
                uuid: folder.uuid.clone(),
                name: folder.name.clone(),
                collapsed: folder.collapsed,
                depth,
                child_count,
                move_to_root: folder.parent.is_some(),
                move_targets,
            }));
        if folder.collapsed {
            return;
        }
        let child_depth = depth.saturating_add(1);
        for child in child_folders {
            self.append_folder(child, child_depth);
        }
        for row in folder.children {
            self.append_entry(row, Some(folder.uuid.clone()), child_depth);
        }
    }

    fn append_entry(&mut self, row: BookmarkRow, folder: Option<String>, depth: u32) {
        let move_targets = self.entry_move_targets(folder.as_deref());
        self.rows.push(BookmarkTreeState::Entry(BookmarkEntryState {
            row,
            depth,
            move_to_root: folder.is_some(),
            move_targets,
        }));
    }

    fn folder_move_targets(&self, uuid: &str) -> Vec<BookmarkFolderChoice> {
        let mut targets = Vec::new();
        for target in &self.choices {
            if target.uuid == uuid || target.ancestors.iter().any(|ancestor| ancestor == uuid) {
                continue;
            }
            targets.push(target.clone());
        }
        targets
    }

    fn entry_move_targets(&self, folder: Option<&str>) -> Vec<BookmarkFolderChoice> {
        let mut targets = Vec::new();
        for target in &self.choices {
            if Some(target.uuid.as_str()) == folder {
                continue;
            }
            targets.push(target.clone());
        }
        targets
    }
}

impl ActiveWorkspaceProject {
    fn active(projects: &TabBoundaryState) -> Option<Self> {
        let index = projects
            .projects
            .iter()
            .position(|project| project.depth == 0 && project.is_active)?;
        let root = projects.projects[index].clone();
        let children = projects
            .projects
            .iter()
            .skip(index + 1)
            .take_while(|project| project.depth > 0)
            .cloned()
            .collect();
        let choices = Self::choices(projects);
        let boundary = projects
            .boundary
            .as_ref()
            .filter(|boundary| boundary.effective_dir == root.path);
        Some(Self {
            root,
            children,
            choices,
            project_dir: boundary
                .map(|boundary| boundary.project_dir.clone())
                .unwrap_or_default(),
            branch: boundary
                .map(|boundary| boundary.branch.clone())
                .unwrap_or_default(),
            branches: boundary
                .map(|_| projects.branches.clone())
                .unwrap_or_default(),
            expanded: false,
        })
    }

    fn viewed(projects: &TabBoundaryState, path: &str, name: &str, branch: &str) -> Option<Self> {
        let index = projects.projects.iter().position(|project| {
            project.depth == 0
                && project.kind == ProjectRowKind::Project
                && (project.path == path || project.display_path == path)
        });
        let root = index
            .map(|index| projects.projects[index].clone())
            .unwrap_or_else(|| vmux_api::space::ProjectRow {
                path: path.to_string(),
                label: name.to_string(),
                display_path: path.to_string(),
                is_active: true,
                branch: branch.to_string(),
                ..Default::default()
            });
        let children = index
            .map(|index| {
                projects
                    .projects
                    .iter()
                    .skip(index + 1)
                    .take_while(|project| project.depth > 0)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut choices = Self::choices(projects);
        if !choices.iter().any(|choice| choice.path == root.path) {
            choices.push(root.clone());
        }
        let branches = projects
            .boundary
            .as_ref()
            .filter(|boundary| boundary.effective_dir == path || boundary.project_dir == path)
            .map(|_| projects.branches.clone())
            .unwrap_or_default();
        Some(Self {
            root,
            children,
            choices,
            project_dir: path.to_string(),
            branch: branch.to_string(),
            branches,
            expanded: false,
        })
    }

    fn choices(projects: &TabBoundaryState) -> Vec<vmux_api::space::ProjectRow> {
        let mut choices: Vec<vmux_api::space::ProjectRow> = Vec::new();
        for project in &projects.projects {
            if project.depth != 0 || project.missing || project.kind != ProjectRowKind::Project {
                continue;
            }
            let existing = choices.iter_mut().find(|held| {
                held.display_path == project.display_path && held.label == project.label
            });
            if let Some(existing) = existing {
                if project.is_active && !existing.is_active {
                    existing.clone_from(project);
                    existing.is_worktree = false;
                }
                continue;
            }
            let mut projected = project.clone();
            projected.is_worktree = false;
            choices.push(projected);
        }
        choices
    }
}

impl ActiveSession {
    fn from_projections(
        panes: &PaneTreeState,
        projects: &TabBoundaryState,
        team: &UiTeam,
    ) -> Option<Self> {
        let pane = panes
            .panes
            .iter()
            .find(|pane| pane.is_active)
            .or_else(|| panes.panes.first())?;
        let page = pane
            .stacks
            .iter()
            .find(|stack| stack.is_active && !stack.url.is_empty())?
            .clone();
        Some(Self {
            task: None,
            agent: Self::agent_for(&page, &team.members),
            project: ActiveWorkspaceProject::active(projects),
            boundary: projects.boundary.clone(),
            page,
            pane_id: pane.id,
        })
    }

    fn agent_for(page: &StackNode, team: &[TeamMemberRow]) -> Option<TeamMemberRow> {
        if let Some(agent_id) = page.agent_id.as_deref()
            && let Some(agent) = team
                .iter()
                .find(|member| !member.is_user && member.id == agent_id)
            && Self::same_page_family(&page.url, &agent.url)
        {
            return Some(agent.clone());
        }
        team.iter()
            .filter(|member| !member.is_user && !member.url.is_empty())
            .find(|member| page.url.trim_end_matches('/') == member.url.trim_end_matches('/'))
            .cloned()
    }

    fn same_page_family(page: &str, member: &str) -> bool {
        let Some(page) = vmux_api::VmuxRoute::parse(page) else {
            return false;
        };
        let Some(member) = vmux_api::VmuxRoute::parse(member) else {
            return false;
        };
        page.in_subtree(&member) || member.in_subtree(&page)
    }
}

#[derive(SystemParam)]
struct SessionProjection<'w, 's> {
    targets: Query<'w, 's, &'static EntityTarget<Session>>,
    session_entities: Query<'w, 's, (), With<Session>>,
    stage_entities: Query<'w, 's, (), With<StageDefinition>>,
    sessions: Query<
        'w,
        's,
        (
            &'static Name,
            &'static Description,
            &'static Stage,
            Option<&'static RunState>,
        ),
        With<Session>,
    >,
    stages: Query<'w, 's, (&'static StageId, &'static Name), With<StageDefinition>>,
    changed_targets: Query<'w, 's, (), Changed<EntityTarget<Session>>>,
    changed_sessions: Query<
        'w,
        's,
        (),
        (
            With<Session>,
            Or<(
                Changed<Name>,
                Changed<Description>,
                Changed<Stage>,
                Changed<RunState>,
            )>,
        ),
    >,
    changed_stages:
        Query<'w, 's, (), (With<StageDefinition>, Or<(Changed<StageId>, Changed<Name>)>)>,
    removed_targets: RemovedComponents<'w, 's, EntityTarget<Session>>,
    removed_sessions: RemovedComponents<'w, 's, Session>,
    removed_names: RemovedComponents<'w, 's, Name>,
    removed_descriptions: RemovedComponents<'w, 's, Description>,
    removed_stages: RemovedComponents<'w, 's, Stage>,
    removed_states: RemovedComponents<'w, 's, RunState>,
    removed_stage_definitions: RemovedComponents<'w, 's, StageDefinition>,
    removed_stage_ids: RemovedComponents<'w, 's, StageId>,
}

#[derive(Clone)]
struct GitPageSelection {
    path: String,
    name: String,
    branch: String,
}

#[derive(SystemParam)]
struct GitSelectionProjection<'w, 's> {
    pages: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static vmux_git::state::RepositoryPath,
            &'static vmux_git::state::RepositoryName,
            &'static vmux_git::state::CheckedOutBranch,
        ),
        With<Browser>,
    >,
    changed: Query<
        'w,
        's,
        (),
        (
            With<Browser>,
            Or<(
                Changed<vmux_git::state::RepositoryPath>,
                Changed<vmux_git::state::RepositoryName>,
                Changed<vmux_git::state::CheckedOutBranch>,
            )>,
        ),
    >,
    removed_paths: RemovedComponents<'w, 's, vmux_git::state::RepositoryPath>,
    removed_names: RemovedComponents<'w, 's, vmux_git::state::RepositoryName>,
    removed_branches: RemovedComponents<'w, 's, vmux_git::state::CheckedOutBranch>,
}

impl GitSelectionProjection<'_, '_> {
    fn is_changed(&mut self) -> bool {
        let mut changed = !self.changed.is_empty();
        changed |= Self::drain_removed(&mut self.removed_paths);
        changed |= Self::drain_removed(&mut self.removed_names);
        changed |= Self::drain_removed(&mut self.removed_branches);
        changed
    }

    fn drain_removed<T: Component>(removed: &mut RemovedComponents<T>) -> bool {
        let changed = !removed.is_empty();
        removed.clear();
        changed
    }

    fn page(&self, page: &StackNode) -> Option<GitPageSelection> {
        let stack = Entity::from_bits(page.id);
        self.pages.iter().find_map(|(parent, path, name, branch)| {
            (parent.parent() == stack).then(|| GitPageSelection {
                path: path.0.clone(),
                name: name.0.clone(),
                branch: branch.0.clone(),
            })
        })
    }
}

impl SessionProjection<'_, '_> {
    fn is_changed(&mut self) -> bool {
        let mut changed = !self.changed_targets.is_empty()
            || !self.changed_sessions.is_empty()
            || !self.changed_stages.is_empty();
        changed |= Self::drain_removed(&mut self.removed_targets);
        changed |= Self::drain_removed(&mut self.removed_sessions);
        changed |= Self::drain_removed(&mut self.removed_stage_definitions);
        for entity in self.removed_names.read() {
            changed |=
                self.session_entities.contains(entity) || self.stage_entities.contains(entity);
        }
        for entity in self.removed_descriptions.read() {
            changed |= self.session_entities.contains(entity);
        }
        for entity in self.removed_stages.read() {
            changed |= self.session_entities.contains(entity);
        }
        for entity in self.removed_states.read() {
            changed |= self.session_entities.contains(entity);
        }
        for entity in self.removed_stage_ids.read() {
            changed |= self.stage_entities.contains(entity);
        }
        changed
    }

    fn drain_removed<T: Component>(removed: &mut RemovedComponents<T>) -> bool {
        let changed = !removed.is_empty();
        removed.clear();
        changed
    }

    fn task(&self, page: &StackNode) -> Option<ActiveTask> {
        let stack = Entity::from_bits(page.id);
        let target = self.targets.get(stack).ok()?;
        let (name, description, stage, runtime) = self.sessions.get(target.entity()).ok()?;
        let stage_name = self
            .stages
            .iter()
            .find_map(|(id, name)| (id == &stage.0).then(|| name.as_str().to_string()))
            .unwrap_or_else(|| stage.0.0.clone());
        Some(ActiveTask {
            name: name.as_str().to_string(),
            description: description.0.clone(),
            stage: stage.0.0.clone(),
            stage_name,
            runtime: runtime
                .map(RunState::status)
                .unwrap_or("inactive")
                .to_string(),
        })
    }
}

impl HeaderState {
    fn from_projections(
        stacks: &StackNavigationState,
        bookmarks: &BookmarkStateEvent,
        team: &UiTeam,
    ) -> Self {
        let active = stacks.stacks.iter().find(|stack| stack.is_active).cloned();
        let user = team.members.iter().find(|member| member.is_user).cloned();
        let agents = team
            .members
            .iter()
            .filter(|member| !member.is_user)
            .cloned()
            .collect();
        let Some(url) = active
            .as_ref()
            .map(|stack| stack.url.as_str())
            .filter(|url| !url.is_empty())
        else {
            return Self {
                metadata: active.as_ref().map(|row| vmux_ecs::PageMetadata {
                    title: row.title.clone(),
                    url: row.url.clone(),
                    icon: row.icon.clone(),
                    bg_color: row.bg_color.clone(),
                }),
                active,
                user,
                agents,
                ..Default::default()
            };
        };
        let bookmarked = bookmarks.roots.iter().any(|node| match node {
            BookmarkNode::Entry(bookmark) => bookmark.metadata.url == url,
            BookmarkNode::Folder(folder) => folder
                .children
                .iter()
                .any(|bookmark| bookmark.metadata.url == url),
        }) || bookmarks
            .pins
            .iter()
            .any(|pin| pin.metadata.url == url && pin.bookmarked);
        let pinned_uuid = bookmarks
            .pins
            .iter()
            .find(|pin| pin.metadata.url == url)
            .map(|pin| pin.uuid.clone());
        Self {
            metadata: active.as_ref().map(|row| vmux_ecs::PageMetadata {
                title: row.title.clone(),
                url: row.url.clone(),
                icon: row.icon.clone(),
                bg_color: row.bg_color.clone(),
            }),
            active,
            bookmarked,
            pinned_uuid,
            user,
            agents,
        }
    }
}

impl SideSheetProjection {
    fn from_sources(
        panes: &PaneTreeState,
        spaces: &vmux_ecs::event::space::SpacesListEvent,
        current: Option<&Self>,
    ) -> Self {
        let active_space = spaces.spaces.iter().find(|space| space.is_active).cloned();
        let mut side_sheet_panes = Vec::with_capacity(panes.panes.len());
        for pane in &panes.panes {
            let mut stacks = Vec::with_capacity(pane.stacks.len());
            for stack in &pane.stacks {
                if stack.url.is_empty() && stack.title == "New Stack" {
                    continue;
                }
                stacks.push(stack.clone());
            }
            let collapsed_stack = stacks
                .iter()
                .find(|stack| stack.is_active)
                .or_else(|| stacks.first())
                .cloned();
            side_sheet_panes.push(SideSheetPane {
                id: pane.id,
                is_active: pane.is_active,
                collapsed: pane.collapsed,
                bookmarks_expanded: pane.bookmarks_expanded,
                any_loading: stacks.iter().any(|stack| stack.is_loading),
                collapsed_stack,
                stacks,
            });
        }
        let active_pane = side_sheet_panes
            .iter()
            .find(|pane| pane.is_active)
            .or_else(|| side_sheet_panes.first())
            .cloned();
        let active_page = active_pane.as_ref().and_then(|pane| {
            pane.stacks
                .iter()
                .find(|stack| stack.is_active && !stack.url.is_empty())
                .cloned()
        });
        let reveal_location = active_pane
            .as_ref()
            .and_then(|pane| {
                pane.stacks
                    .iter()
                    .find(|stack| stack.is_active)
                    .map(|stack| (pane.id, stack.id))
            })
            .or_else(|| {
                side_sheet_panes.iter().find_map(|pane| {
                    pane.stacks
                        .iter()
                        .find(|stack| stack.is_active)
                        .map(|stack| (pane.id, stack.id))
                })
            });
        let previous_reveal = current.and_then(|projection| projection.state.reveal.as_ref());
        let mut reveal_revision = current
            .map(|projection| projection.reveal_revision)
            .unwrap_or_default();
        let reveal = reveal_location.map(|(pane_id, stack_id)| {
            let unchanged = previous_reveal.is_some_and(|previous| {
                previous.pane_id == pane_id && previous.stack_id == stack_id
            });
            if !unchanged {
                reveal_revision = reveal_revision.wrapping_add(1).max(1);
            }
            StackRevealTarget {
                pane_id,
                stack_id,
                revision: reveal_revision,
            }
        });
        Self {
            state: SideSheetState {
                active_space,
                active_pane,
                active_page,
                reveal,
                panes: side_sheet_panes,
            },
            reveal_revision,
        }
    }
}

fn publish_active_session(
    layouts: Query<
        (
            Entity,
            Option<Ref<PaneTreeProjection>>,
            Option<Ref<ProjectProjection>>,
            Option<Ref<TeamProjection>>,
        ),
        With<LayoutCef>,
    >,
    sections: SideSheetSections,
    changed_sections: Query<(), Changed<SideSheetSectionsExpanded>>,
    mut removed_sections: RemovedComponents<SideSheetSectionsExpanded>,
    mut sessions: SessionProjection,
    mut git: GitSelectionProjection,
    mut last: Local<std::collections::HashMap<Entity, ActiveSessionState>>,
    mut commands: Commands,
) {
    let empty_panes = PaneTreeState::default();
    let empty_projects = TabBoundaryState::default();
    let empty_team = UiTeam::default();
    let sessions_changed = sessions.is_changed();
    let git_changed = git.is_changed();
    let sections_changed = !changed_sections.is_empty() || removed_sections.read().next().is_some();
    for (entity, panes, projects, team) in &layouts {
        let projection_changed = panes.as_ref().is_some_and(|value| value.is_changed())
            || projects.as_ref().is_some_and(|value| value.is_changed())
            || team.as_ref().is_some_and(|value| value.is_changed());
        if last.contains_key(&entity)
            && !sessions_changed
            && !git_changed
            && !projection_changed
            && !sections_changed
        {
            continue;
        }
        let panes = panes
            .as_deref()
            .map(|projection| &projection.0)
            .unwrap_or(&empty_panes);
        let projects = projects
            .as_deref()
            .map(|projection| &projection.0)
            .unwrap_or(&empty_projects);
        let team = team
            .as_deref()
            .map(|projection| &projection.0)
            .unwrap_or(&empty_team);
        let session = ActiveSession::from_projections(panes, projects, team).map(|mut active| {
            active.task = sessions.task(&active.page);
            if let Some(selection) = git.page(&active.page) {
                active.project = ActiveWorkspaceProject::viewed(
                    projects,
                    &selection.path,
                    &selection.name,
                    &selection.branch,
                );
                let boundary = active.boundary.get_or_insert_with(TabBoundary::default);
                boundary.effective_dir.clone_from(&selection.path);
                boundary.project_dir.clone_from(&selection.path);
                boundary.repository.clone_from(&selection.name);
                boundary.is_git_repo = true;
                boundary.branch.clone_from(&selection.branch);
            }
            if let Some(project) = active.project.as_mut() {
                project.expanded = sections.under(Entity::from_bits(active.pane_id)).projects;
            }
            active
        });
        let event = ActiveSessionState { session };
        if last.get(&entity) == Some(&event) {
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<UiLayout>::from_event(
            entity, &event,
        ));
        last.insert(entity, event);
    }
}

fn capture_tab_list(trigger: On<vmux_ecs::UiStateWrite<UiLayout>>, mut commands: Commands) {
    let Some(tabs) = &trigger.event().update().tabs else {
        return;
    };
    commands
        .entity(trigger.event().webview())
        .insert(TabListProjection(tabs.clone()));
}

fn project_header(
    layouts: Query<
        (
            Entity,
            Option<&StackProjection>,
            Option<&BookmarkProjection>,
            Option<&TeamProjection>,
            Option<&HeaderProjection>,
        ),
        With<LayoutCef>,
    >,
    mut commands: Commands,
) {
    let empty_stacks = StackNavigationState::default();
    let empty_bookmarks = BookmarkStateEvent::default();
    let empty_team = UiTeam::default();
    for (entity, stacks, bookmarks, team, current) in &layouts {
        let stacks = stacks
            .map(|projection| &projection.0)
            .unwrap_or(&empty_stacks);
        let bookmarks = bookmarks
            .map(|projection| &projection.0)
            .unwrap_or(&empty_bookmarks);
        let team = team.map(|projection| &projection.0).unwrap_or(&empty_team);
        let next = HeaderProjection(HeaderState::from_projections(stacks, bookmarks, team));
        if current == Some(&next) {
            continue;
        }
        commands.entity(entity).insert(next);
    }
}

fn project_tab_strip(
    layouts: Query<
        (
            Entity,
            &TabListProjection,
            Option<&HeaderProjection>,
            Option<&TabStripProjection>,
        ),
        With<LayoutCef>,
    >,
    mut commands: Commands,
) {
    for (entity, tabs, header, current) in &layouts {
        let next = TabStripProjection::from_sources(&tabs.0, header.map(|header| &header.0));
        if current == Some(&next) {
            continue;
        }
        commands.entity(entity).insert(next);
    }
}

fn publish_header(
    projections: Query<(Entity, &HeaderProjection), Changed<HeaderProjection>>,
    mut commands: Commands,
) {
    for (entity, projection) in &projections {
        commands.trigger(vmux_ecs::UiStateWrite::<UiLayout>::from_event(
            entity,
            &projection.0,
        ));
    }
}

fn publish_tab_strip(
    projections: Query<(Entity, &TabStripProjection), Changed<TabStripProjection>>,
    mut commands: Commands,
) {
    for (entity, projection) in &projections {
        commands.trigger(vmux_ecs::UiStateWrite::<UiLayout>::from_event(
            entity,
            &projection.0,
        ));
    }
}

fn project_side_sheet(
    layouts: Query<
        (
            Entity,
            &PaneTreeProjection,
            &SpacesProjection,
            Option<&SideSheetProjection>,
        ),
        With<LayoutCef>,
    >,
    mut commands: Commands,
) {
    for (entity, panes, spaces, current) in &layouts {
        let next = SideSheetProjection::from_sources(&panes.0, &spaces.0, current);
        if current == Some(&next) {
            continue;
        }
        commands.entity(entity).insert(next);
    }
}

fn project_bookmark_ui(
    layouts: Query<
        (
            Entity,
            Option<&BookmarkProjection>,
            Option<&SideSheetProjection>,
            Option<&BookmarkUiProjection>,
        ),
        With<LayoutCef>,
    >,
    mut commands: Commands,
) {
    let empty = BookmarkStateEvent::default();
    for (entity, bookmarks, side_sheet, current) in &layouts {
        let bookmarks = bookmarks.map(|projection| &projection.0).unwrap_or(&empty);
        let active_page = side_sheet.and_then(|projection| projection.state.active_page.as_ref());
        let next = BookmarkUiProjection(BookmarkUiBuilder::build(bookmarks, active_page));
        if current == Some(&next) {
            continue;
        }
        commands.entity(entity).insert(next);
    }
}

fn publish_side_sheet(
    projections: Query<(Entity, &SideSheetProjection), Changed<SideSheetProjection>>,
    mut commands: Commands,
) {
    for (entity, projection) in &projections {
        commands.trigger(vmux_ecs::UiStateWrite::<UiLayout>::from_event(
            entity,
            &projection.state,
        ));
    }
}

fn publish_bookmark_ui(
    projections: Query<(Entity, &BookmarkUiProjection), Changed<BookmarkUiProjection>>,
    mut commands: Commands,
) {
    for (entity, projection) in &projections {
        commands.trigger(vmux_ecs::UiStateWrite::<UiLayout>::from_event(
            entity,
            &projection.0,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[derive(Resource, Default)]
    struct SessionProjectionChanges(u32);

    fn count_changes(
        mut sessions: SessionProjection,
        mut changes: ResMut<SessionProjectionChanges>,
    ) {
        if sessions.is_changed() {
            changes.0 += 1;
        }
    }

    #[test]
    fn session_projection_reads_task_stage_and_runtime_from_the_target() {
        let mut world = World::new();
        let session = world
            .spawn((
                Session,
                Name::new("Ship session manager"),
                Description("End-to-end slice".into()),
                Stage(StageId("in_review".into())),
                RunState::Streaming,
            ))
            .id();
        let stack = world.spawn(EntityTarget::<Session>::new(session)).id();
        world.spawn((
            StageDefinition,
            StageId("in_review".into()),
            Name::new("session-stage-in-review"),
        ));
        let page = StackNode {
            id: stack.to_bits(),
            agent_id: None,
            title: String::new(),
            url: "vmux://sessions/session-1".into(),
            icon: Default::default(),
            is_active: true,
            is_loading: false,
            is_dirty: false,
            bg_color: None,
        };

        let task = world
            .run_system_once(move |sessions: SessionProjection| sessions.task(&page))
            .unwrap()
            .unwrap();

        assert_eq!(task.name, "Ship session manager");
        assert_eq!(task.description, "End-to-end slice");
        assert_eq!(task.stage, "in_review");
        assert_eq!(task.stage_name, "session-stage-in-review");
        assert_eq!(task.runtime, "streaming");
    }

    #[test]
    fn session_projection_reacts_to_component_removal() {
        let mut app = App::new();
        app.init_resource::<SessionProjectionChanges>()
            .add_systems(Update, count_changes);
        let session = app
            .world_mut()
            .spawn((
                Session,
                Name::new("Task"),
                Description::default(),
                Stage(StageId("in_progress".into())),
                RunState::Idle,
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let stage = app
            .world_mut()
            .spawn((
                StageDefinition,
                StageId("in_progress".into()),
                Name::new("In progress"),
            ))
            .id();

        app.update();
        let initial = app.world().resource::<SessionProjectionChanges>().0;

        app.world_mut().entity_mut(session).remove::<RunState>();
        app.update();
        app.world_mut().despawn(stage);
        app.update();
        app.world_mut()
            .entity_mut(stack)
            .remove::<EntityTarget<Session>>();
        app.update();

        assert_eq!(
            app.world().resource::<SessionProjectionChanges>().0,
            initial + 3
        );
    }

    #[test]
    fn session_projection_ignores_unrelated_component_removal() {
        let mut app = App::new();
        app.init_resource::<SessionProjectionChanges>()
            .add_systems(Update, count_changes);
        let unrelated = app.world_mut().spawn(Name::new("Unrelated")).id();

        app.update();
        let initial = app.world().resource::<SessionProjectionChanges>().0;
        app.world_mut().despawn(unrelated);
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<SessionProjectionChanges>().0,
            initial
        );
    }

    #[test]
    fn tab_strip_projection_resolves_render_state_before_ui_delivery() {
        let tabs = TabListState {
            tabs: vec![crate::event::TabRow {
                id: "tab-1".into(),
                name: "Workspace".into(),
                is_active: true,
                bg_color: None,
                title: String::new(),
                url: "vmux://terminal/".into(),
                icon: Default::default(),
                is_done_unseen: false,
            }],
        };
        let header = HeaderState {
            active: Some(crate::event::StackRow {
                title: "Terminal".into(),
                url: "vmux://terminal/".into(),
                icon: Default::default(),
                is_active: true,
                bg_color: Some("#123456".into()),
                address: Default::default(),
            }),
            ..Default::default()
        };

        let projection = TabStripProjection::from_sources(&tabs, Some(&header));

        assert_eq!(projection.0.order, vec!["tab-1".to_string()]);
        assert_eq!(projection.0.drag_region_revision, "tab-1");
        assert_eq!(projection.0.tabs[0].display_title, "Workspace");
        assert_eq!(projection.0.tabs[0].metadata.title, "Workspace");
        assert_eq!(
            projection.0.tabs[0].tab.bg_color.as_deref(),
            Some("#123456")
        );
    }

    #[test]
    fn active_session_matches_the_agent_by_entity_id() {
        let page = StackNode {
            id: 1,
            agent_id: Some("second".into()),
            title: String::new(),
            url: "vmux://sessions/codex/cli/session-two".into(),
            icon: Default::default(),
            is_active: true,
            is_loading: false,
            is_dirty: false,
            bg_color: None,
        };
        let team = vec![
            TeamMemberRow {
                id: "first".into(),
                name: "Codex one".into(),
                url: "vmux://sessions/codex/".into(),
                sid: "session-one".into(),
                ..Default::default()
            },
            TeamMemberRow {
                id: "second".into(),
                name: "Codex two".into(),
                url: "vmux://sessions/codex/".into(),
                sid: "session-two".into(),
                ..Default::default()
            },
        ];

        let agent = ActiveSession::agent_for(&page, &team).unwrap();

        assert_eq!(agent.id, "second");
    }

    #[test]
    fn active_terminal_does_not_present_a_stale_agent() {
        let page = StackNode {
            id: 1,
            agent_id: Some("agent".into()),
            title: "Terminal".into(),
            url: "vmux://terminal/15284".into(),
            icon: Default::default(),
            is_active: true,
            is_loading: false,
            is_dirty: false,
            bg_color: None,
        };
        let team = vec![TeamMemberRow {
            id: "agent".into(),
            name: "Codex".into(),
            url: "vmux://sessions/codex/".into(),
            ..Default::default()
        }];

        assert_eq!(ActiveSession::agent_for(&page, &team), None);
    }

    #[test]
    fn active_project_choices_reuse_projected_repository_identity() {
        let active = vmux_api::space::ProjectRow {
            path: "/repo/vmux/.worktrees/vmx-226".into(),
            label: "vmux".into(),
            display_path: "/repo/vmux".into(),
            is_active: true,
            is_worktree: true,
            ..Default::default()
        };
        let duplicate = vmux_api::space::ProjectRow {
            path: "/repo/vmux".into(),
            label: "vmux".into(),
            display_path: "/repo/vmux".into(),
            ..Default::default()
        };
        let projects = TabBoundaryState {
            projects: vec![active.clone(), duplicate],
            ..Default::default()
        };

        let projected = ActiveWorkspaceProject::active(&projects).unwrap();

        assert_eq!(projected.root, active);
        assert_eq!(projected.choices.len(), 1);
        assert_eq!(projected.choices[0].path, "/repo/vmux/.worktrees/vmx-226");
        assert_eq!(projected.choices[0].display_path, "/repo/vmux");
        assert!(!projected.choices[0].is_worktree);
    }

    #[test]
    fn header_projection_contains_render_ready_team_rows() {
        let user = TeamMemberRow {
            id: "user".into(),
            name: "You".into(),
            is_user: true,
            ..Default::default()
        };
        let agent = TeamMemberRow {
            id: "agent".into(),
            name: "Codex".into(),
            ..Default::default()
        };
        let state = HeaderState::from_projections(
            &StackNavigationState::default(),
            &BookmarkStateEvent::default(),
            &UiTeam {
                members: vec![user.clone(), agent.clone()],
                ..Default::default()
            },
        );

        assert_eq!(state.user, Some(user));
        assert_eq!(state.agents, vec![agent]);
    }

    #[test]
    fn side_sheet_projection_resolves_active_rows_before_ui_delivery() {
        let panes = PaneTreeState {
            panes: vec![
                crate::event::PaneNode {
                    id: 1,
                    is_active: false,
                    collapsed: false,
                    bookmarks_expanded: false,
                    stacks: vec![StackNode {
                        id: 11,
                        agent_id: None,
                        title: "Fallback".into(),
                        url: "vmux://fallback/".into(),
                        icon: Default::default(),
                        is_active: true,
                        is_loading: false,
                        is_dirty: false,
                        bg_color: None,
                    }],
                },
                crate::event::PaneNode {
                    id: 2,
                    is_active: true,
                    collapsed: true,
                    bookmarks_expanded: true,
                    stacks: vec![
                        StackNode {
                            id: 21,
                            agent_id: None,
                            title: "New Stack".into(),
                            url: String::new(),
                            icon: Default::default(),
                            is_active: false,
                            is_loading: false,
                            is_dirty: false,
                            bg_color: None,
                        },
                        StackNode {
                            id: 22,
                            agent_id: None,
                            title: "Active".into(),
                            url: "vmux://active/".into(),
                            icon: Default::default(),
                            is_active: true,
                            is_loading: true,
                            is_dirty: false,
                            bg_color: None,
                        },
                    ],
                },
            ],
        };
        let spaces = vmux_ecs::event::space::SpacesListEvent {
            spaces: vec![
                vmux_ecs::event::space::SpaceRow {
                    id: "inactive".into(),
                    ..Default::default()
                },
                vmux_ecs::event::space::SpaceRow {
                    id: "active".into(),
                    is_active: true,
                    ..Default::default()
                },
            ],
            selected: 1,
        };

        let projection = SideSheetProjection::from_sources(&panes, &spaces, None);
        let state = &projection.state;

        assert_eq!(
            state.active_space.as_ref().map(|space| space.id.as_str()),
            Some("active")
        );
        assert_eq!(state.active_pane.as_ref().map(|pane| pane.id), Some(2));
        assert_eq!(state.active_page.as_ref().map(|page| page.id), Some(22));
        assert_eq!(state.panes.len(), 2);
        assert_eq!(state.panes[1].stacks.len(), 1);
        assert!(state.panes[1].any_loading);
        assert_eq!(
            state.panes[1]
                .collapsed_stack
                .as_ref()
                .map(|stack| stack.id),
            Some(22)
        );
        assert_eq!(
            state.reveal,
            Some(StackRevealTarget {
                pane_id: 2,
                stack_id: 22,
                revision: 1,
            })
        );

        let unchanged = SideSheetProjection::from_sources(&panes, &spaces, Some(&projection));
        assert_eq!(unchanged.state.reveal.as_ref().unwrap().revision, 1);

        let mut changed_panes = panes;
        changed_panes.panes[0].is_active = true;
        changed_panes.panes[1].is_active = false;
        let changed = SideSheetProjection::from_sources(&changed_panes, &spaces, Some(&unchanged));
        assert_eq!(changed.state.reveal.as_ref().unwrap().revision, 2);
    }
}
