use bevy::prelude::*;
use vmux_api::bookmark::{BookmarkNode, BookmarkStateEvent};
use vmux_core::event::team::{TeamEvent, TeamMemberRow};

use crate::LayoutUiStateUpdates;
use crate::cef::LayoutCef;
use crate::event::{
    ActiveSession, ActiveSessionState, ActiveWorkspaceProject, HeaderPageState, PaneTreeState,
    StackNavigationState, StackNode, TabBoundaryState,
};

pub struct LayoutUiProjectionPlugin;

impl Plugin for LayoutUiProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (publish_active_session, publish_header_page));
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PaneTreeProjection(pub PaneTreeState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ProjectProjection(pub TabBoundaryState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TeamProjection(pub TeamEvent);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct StackProjection(pub StackNavigationState);

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BookmarkProjection(pub BookmarkStateEvent);

impl ActiveWorkspaceProject {
    fn active(projects: &[vmux_core::event::ProjectRow]) -> Option<Self> {
        let index = projects
            .iter()
            .position(|project| project.depth == 0 && project.is_active)?;
        let root = projects[index].clone();
        let children = projects
            .iter()
            .skip(index + 1)
            .take_while(|project| project.depth > 0)
            .cloned()
            .collect();
        let choices = projects
            .iter()
            .filter(|project| project.depth == 0 && !project.missing)
            .cloned()
            .collect();
        Some(Self {
            root,
            children,
            choices,
        })
    }
}

impl ActiveSession {
    fn from_projections(
        panes: &PaneTreeState,
        projects: &TabBoundaryState,
        team: &TeamEvent,
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
            agent: Self::agent_for(&page, &team.members),
            project: ActiveWorkspaceProject::active(&projects.projects),
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
        {
            return Some(agent.clone());
        }
        team.iter()
            .filter(|member| !member.is_user && !member.url.is_empty())
            .find(|member| page.url.trim_end_matches('/') == member.url.trim_end_matches('/'))
            .cloned()
    }
}

impl HeaderPageState {
    fn from_projections(stacks: &StackNavigationState, bookmarks: &BookmarkStateEvent) -> Self {
        let active = stacks.stacks.iter().find(|stack| stack.is_active).cloned();
        let Some(url) = active
            .as_ref()
            .map(|stack| stack.url.as_str())
            .filter(|url| !url.is_empty())
        else {
            return Self {
                active,
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
            active,
            bookmarked,
            pinned_uuid,
        }
    }
}

fn publish_active_session(
    layouts: Query<
        (
            Entity,
            Option<&PaneTreeProjection>,
            Option<&ProjectProjection>,
            Option<&TeamProjection>,
        ),
        With<LayoutCef>,
    >,
    mut last: Local<std::collections::HashMap<Entity, ActiveSessionState>>,
    mut commands: Commands,
) {
    let empty_panes = PaneTreeState::default();
    let empty_projects = TabBoundaryState::default();
    let empty_team = TeamEvent::default();
    for (entity, panes, projects, team) in &layouts {
        let panes = panes
            .map(|projection| &projection.0)
            .unwrap_or(&empty_panes);
        let projects = projects
            .map(|projection| &projection.0)
            .unwrap_or(&empty_projects);
        let team = team.map(|projection| &projection.0).unwrap_or(&empty_team);
        let event = ActiveSessionState {
            session: ActiveSession::from_projections(panes, projects, team),
        };
        if last.get(&entity) == Some(&event) {
            continue;
        }
        LayoutUiStateUpdates::write(&mut commands, entity, &event);
        last.insert(entity, event);
    }
}

fn publish_header_page(
    layouts: Query<
        (
            Entity,
            Option<&StackProjection>,
            Option<&BookmarkProjection>,
        ),
        With<LayoutCef>,
    >,
    mut last: Local<std::collections::HashMap<Entity, HeaderPageState>>,
    mut commands: Commands,
) {
    let empty_stacks = StackNavigationState::default();
    let empty_bookmarks = BookmarkStateEvent::default();
    for (entity, stacks, bookmarks) in &layouts {
        let stacks = stacks
            .map(|projection| &projection.0)
            .unwrap_or(&empty_stacks);
        let bookmarks = bookmarks
            .map(|projection| &projection.0)
            .unwrap_or(&empty_bookmarks);
        let event = HeaderPageState::from_projections(stacks, bookmarks);
        if last.get(&entity) == Some(&event) {
            continue;
        }
        LayoutUiStateUpdates::write(&mut commands, entity, &event);
        last.insert(entity, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
