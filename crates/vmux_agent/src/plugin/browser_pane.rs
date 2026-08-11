//! The browser pane an agent opened beside itself.
//!
//! Recorded when layout reports the pane it created, not rediscovered afterwards. A sibling
//! search cannot tell a pane the agent opened from one the user happened to put next to it, and
//! it has to re-walk the tree for every question; the answer only changes when a pane is opened,
//! so that is where it is written down.

use bevy::prelude::*;
use vmux_core::agent::AgentKind;
use vmux_service::protocol::ProcessId;

use crate::events::CommandOrigin;
use crate::session::AgentSession;

/// Wires the record-keeping. Resolution itself is [`AgentBrowserResolve`], a system param.
pub(crate) struct AgentBrowserPanePlugin;

impl Plugin for AgentBrowserPanePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, record_opened_browser_pane);
    }
}

/// The browser pane this agent opened beside itself, and the stack inside it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentBrowserPane {
    pane: Entity,
    stack: Entity,
}

/// How an agent names itself to layout, which echoes it back untouched. The same key
/// `ActivePanes` is indexed by.
pub(crate) fn profile_key(anchor: ProcessId) -> String {
    format!("{anchor:?}")
}

/// Records the pane layout just opened on some agent's behalf.
fn record_opened_browser_pane(
    mut opened: MessageReader<vmux_layout::PaneOpenedForProfile>,
    anchors: Query<(Entity, &ProcessId)>,
    mut commands: Commands,
) {
    for event in opened.read() {
        let Some((entity, _)) = anchors
            .iter()
            .find(|(_, anchor)| profile_key(**anchor) == event.profile)
        else {
            continue;
        };
        commands.entity(entity).try_insert(AgentBrowserPane {
            pane: event.pane,
            stack: event.stack,
        });
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentBrowserResolve<'w, 's> {
    activate: MessageWriter<'w, vmux_layout::active_panes::ActivatePane>,
    /// Matches any anchored content (CLI terminal or ACP chat webview) by its unique ProcessId.
    agent_terms: Query<
        'w,
        's,
        (
            Entity,
            &'static ProcessId,
            &'static ChildOf,
            Option<&'static AgentBrowserPane>,
        ),
    >,
    kinds: Query<'w, 's, &'static AgentSession>,
    child_of: Query<'w, 's, &'static ChildOf>,
    panes: Query<'w, 's, (), With<vmux_layout::pane::Pane>>,
}

impl AgentBrowserResolve<'_, '_> {
    /// The agent's own pane (its stack's parent pane), from its anchor.
    pub(crate) fn agent_pane(&self, anchor: ProcessId) -> Option<Entity> {
        use bevy::ecs::relationship::Relationship;
        let (_, _, term_co, _) = self
            .agent_terms
            .iter()
            .find(|(_, pid, ..)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    /// The browser pane recorded for this agent, if it still exists. A pane the user has since
    /// closed leaves the component behind, so the entity is checked rather than trusted.
    fn browser_pane_for(&self, anchor: ProcessId) -> Option<AgentBrowserPane> {
        let (_, _, _, recorded) = self
            .agent_terms
            .iter()
            .find(|(_, pid, ..)| **pid == anchor)?;
        recorded.copied().filter(|it| self.panes.contains(it.pane))
    }

    /// The kind of the agent at `anchor` (Claude/Codex/Vibe), for its avatar badge.
    /// `None` for ACP sessions (no `AgentKind`).
    fn agent_kind(&self, anchor: ProcessId) -> Option<AgentKind> {
        let (entity, ..) = self
            .agent_terms
            .iter()
            .find(|(_, pid, ..)| **pid == anchor)?;
        self.kinds.get(entity).ok().map(|session| session.kind)
    }

    /// Resolve the agent's browser pane from its anchor, and record it as that
    /// agent's active pane (for its focus ring). Returns the pane entity, or
    /// `None` if the agent has no browser pane yet (caller keeps the default).
    pub(crate) fn claim_browser_pane(
        &mut self,
        anchor: ProcessId,
    ) -> Option<(Entity, Option<Entity>)> {
        let recorded = self.browser_pane_for(anchor)?;
        let kind = self.agent_kind(anchor);
        self.activate
            .write(vmux_layout::active_panes::ActivatePane {
                profile: vmux_layout::active_panes::ProfileId::Agent(profile_key(anchor)),
                active: vmux_layout::active_panes::ActiveStack {
                    tab: None,
                    pane: Some(recorded.pane),
                    stack: Some(recorded.stack),
                    kind,
                },
            });
        Some((recorded.pane, Some(recorded.stack)))
    }

    /// Returns the explicit pane if given, else the agent's resolved browser
    /// pane as a bare entity-bits string (the form `parse_pane_target` expects,
    /// matching explicit MCP targets after they reach this layer). Returns
    /// `None` if neither is available.
    pub(crate) fn resolve_pane(
        &mut self,
        pane: &Option<String>,
        anchor: &Option<ProcessId>,
    ) -> Option<String> {
        if pane.is_some() {
            return pane.clone();
        }
        let anchor = (*anchor)?;
        let (pane, stack) = self.claim_browser_pane(anchor)?;
        if let Some(stack) = stack {
            return Some(vmux_service::protocol::format_id(
                vmux_service::protocol::NodeKind::Stack,
                stack.to_bits(),
            ));
        }
        Some(vmux_service::protocol::format_id(
            vmux_service::protocol::NodeKind::Pane,
            pane.to_bits(),
        ))
    }

    /// Same as `resolve_pane` but reads the anchor from a command's origin, for
    /// agent browser commands (back/forward) that carry origin rather than a
    /// query anchor.
    pub(crate) fn command_pane(
        &mut self,
        pane: &Option<String>,
        origin: &CommandOrigin,
    ) -> Option<String> {
        let anchor = match origin {
            CommandOrigin::Agent { anchor, .. } => *anchor,
            _ => None,
        };
        self.resolve_pane(pane, &anchor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_layout::pane::Pane;

    #[derive(Resource, Default)]
    struct Claimed(Option<Entity>);

    fn claim(input: Res<Anchor>, mut resolve: AgentBrowserResolve, mut out: ResMut<Claimed>) {
        out.0 = resolve.claim_browser_pane(input.0).map(|(pane, _)| pane);
    }

    #[derive(Resource)]
    struct Anchor(ProcessId);

    /// Layout spawns the pane a frame after it is asked for, so the only thing tying the two
    /// together is the profile key travelling out on the request and back on the report.
    #[test]
    fn a_pane_layout_reports_for_this_agent_is_the_one_it_claims() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::LayoutContractPlugin,
            AgentBrowserPanePlugin,
        ))
        .init_resource::<Claimed>()
        .add_systems(Update, claim.after(record_opened_browser_pane));

        let anchor = ProcessId::new();
        app.insert_resource(Anchor(anchor));
        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        // An agent always lives in a stack; the resolver reaches its own pane through that.
        let agent_stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn((anchor, ChildOf(agent_stack)));

        app.world_mut()
            .resource_mut::<Messages<vmux_layout::PaneOpenedForProfile>>()
            .write(vmux_layout::PaneOpenedForProfile {
                profile: profile_key(anchor),
                pane,
                stack,
            });
        app.update();

        assert_eq!(app.world().resource::<Claimed>().0, Some(pane));
    }

    /// A report for somebody else must not become this agent's pane.
    #[test]
    fn a_pane_reported_for_another_profile_is_not_claimed() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::LayoutContractPlugin,
            AgentBrowserPanePlugin,
        ))
        .init_resource::<Claimed>()
        .add_systems(Update, claim.after(record_opened_browser_pane));

        let anchor = ProcessId::new();
        app.insert_resource(Anchor(anchor));
        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        // An agent always lives in a stack; the resolver reaches its own pane through that.
        let agent_stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn((anchor, ChildOf(agent_stack)));

        app.world_mut()
            .resource_mut::<Messages<vmux_layout::PaneOpenedForProfile>>()
            .write(vmux_layout::PaneOpenedForProfile {
                profile: profile_key(ProcessId::new()),
                pane,
                stack,
            });
        app.update();

        assert_eq!(app.world().resource::<Claimed>().0, None);
    }
}
