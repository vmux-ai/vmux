use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_ecs::agent::CommandOrigin;
use vmux_layout::pane::Pane;

#[derive(bevy::ecs::system::SystemParam)]
pub struct AgentBrowserResolve<'w, 's> {
    agent_terms: Query<'w, 's, (Entity, &'static vmux_ecs::ProcessId, &'static ChildOf)>,
    child_of: Query<'w, 's, &'static ChildOf>,
    panes: Query<'w, 's, (), With<Pane>>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_q: Query<'w, 's, Entity, With<vmux_layout::stack::Stack>>,
    browser_stacks: Query<'w, 's, &'static ChildOf, With<vmux_layout::Browser>>,
    active: vmux_layout::active_pane::ActivePaneQuery<'w, 's>,
    spaces: vmux_layout::space::SpaceHierarchy<'w, 's>,
    tabs: vmux_layout::tab::TabHierarchy<'w, 's>,
}

pub(super) struct AgentBrowserPaneClaim {
    pub(super) pane: Entity,
    pub(super) stack: Option<Entity>,
    pub(super) activation: vmux_layout::active_pane::ActivatePane,
}

pub struct AgentBrowserPaneResolution {
    pub pane: Option<String>,
    pub activation: Option<vmux_layout::active_pane::ActivatePane>,
}

impl AgentBrowserResolve<'_, '_> {
    fn browser_pane_for(&self, agent_pane: Entity) -> Option<Entity> {
        let agent_parent = self.child_of.get(agent_pane).ok()?.get();
        for stack_co in self.browser_stacks.iter() {
            let pane = stack_co.get();
            if pane == agent_pane {
                continue;
            }
            if let Ok(parent_co) = self.child_of.get(pane)
                && parent_co.get() == agent_parent
                && self.pane_has_only_browser_stacks(pane)
            {
                return Some(pane);
            }
        }
        None
    }

    fn pane_has_only_browser_stacks(&self, pane: Entity) -> bool {
        self.pane_children
            .get(pane)
            .ok()
            .map(|children| {
                children
                    .iter()
                    .filter(|&child| self.stack_q.contains(child))
                    .all(|child| self.browser_stacks.contains(child))
            })
            .unwrap_or(false)
    }

    pub fn agent_pane(&self, anchor: vmux_ecs::ProcessId) -> Option<Entity> {
        let (_, _, term_co) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    pub fn working_directory(&self, anchor: vmux_ecs::ProcessId) -> Option<std::path::PathBuf> {
        let pane = self.agent_pane(anchor)?;
        let path = self.tabs.startup_dir(pane)?;
        vmux_setting::StartupDir::from_tab(&path)
            .ok()
            .map(|dir| dir.path)
    }

    pub(super) fn claim_browser_pane(
        &self,
        anchor: vmux_ecs::ProcessId,
    ) -> Option<AgentBrowserPaneClaim> {
        let pane = self.browser_pane_for(self.agent_pane(anchor)?)?;
        let scope = vmux_layout::active_pane::ActivePaneScope::Agent(format!("{anchor:?}"));
        let stack = self
            .active
            .get(&scope)
            .filter(|active| active.pane == Some(pane))
            .and_then(|active| active.stack);
        Some(AgentBrowserPaneClaim {
            pane,
            stack,
            activation: vmux_layout::active_pane::ActivatePane {
                scope,
                active: vmux_layout::active_pane::ActiveStack {
                    tab: None,
                    pane: Some(pane),
                    stack,
                },
            },
        })
    }

    pub fn resolve_pane(
        &self,
        pane: &Option<String>,
        anchor: &Option<vmux_ecs::ProcessId>,
    ) -> Option<AgentBrowserPaneResolution> {
        if let Some(pane) = pane {
            if let Some(anchor) = anchor {
                let agent_pane = self.agent_pane(*anchor)?;
                let (_, bits) = vmux_api::protocol::NodeKind::parse_id(pane)
                    .ok()
                    .filter(|(kind, _)| *kind == vmux_api::protocol::NodeKind::Pane)
                    .or_else(|| {
                        pane.parse::<u64>()
                            .ok()
                            .map(|bits| (vmux_api::protocol::NodeKind::Pane, bits))
                    })?;
                let target = Entity::try_from_bits(bits)?;
                if !self.panes.contains(target)
                    || self.spaces.get(agent_pane) != self.spaces.get(target)
                {
                    return None;
                }
            }
            return Some(AgentBrowserPaneResolution {
                pane: Some(pane.clone()),
                activation: None,
            });
        }
        let Some(anchor) = *anchor else {
            return Some(AgentBrowserPaneResolution {
                pane: None,
                activation: None,
            });
        };
        let claim = self.claim_browser_pane(anchor)?;
        let pane = if let Some(stack) = claim.stack {
            vmux_api::protocol::NodeKind::Stack.id(stack.to_bits())
        } else {
            vmux_api::protocol::NodeKind::Pane.id(claim.pane.to_bits())
        };
        Some(AgentBrowserPaneResolution {
            pane: Some(pane),
            activation: Some(claim.activation),
        })
    }

    pub(crate) fn command_pane(
        &self,
        pane: &Option<String>,
        origin: &CommandOrigin,
    ) -> Option<AgentBrowserPaneResolution> {
        let anchor = match origin {
            CommandOrigin::Agent { anchor, .. } => *anchor,
            _ => None,
        };
        self.resolve_pane(pane, &anchor)
    }
}
