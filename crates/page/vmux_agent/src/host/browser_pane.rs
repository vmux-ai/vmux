use bevy::prelude::*;
use vmux_core::agent::AgentKind;
use vmux_layout::pane::Pane;

use crate::events::CommandOrigin;
use crate::session::AgentSession;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentBrowserResolve<'w, 's> {
    activate: MessageWriter<'w, vmux_layout::active_panes::ActivatePane>,
    agent_terms: Query<
        'w,
        's,
        (
            Entity,
            &'static vmux_service::protocol::ProcessId,
            &'static ChildOf,
        ),
    >,
    kinds: Query<'w, 's, &'static AgentSession>,
    child_of: Query<'w, 's, &'static ChildOf>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_q: Query<'w, 's, Entity, With<vmux_layout::stack::Stack>>,
    browser_stacks: Query<'w, 's, &'static ChildOf, With<vmux_layout::Browser>>,
    active: Res<'w, vmux_layout::active_panes::ActivePanes>,
}

impl AgentBrowserResolve<'_, '_> {
    fn browser_pane_for(&self, agent_pane: Entity) -> Option<Entity> {
        use bevy::ecs::relationship::Relationship;
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

    pub(crate) fn agent_pane(&self, anchor: vmux_service::protocol::ProcessId) -> Option<Entity> {
        use bevy::ecs::relationship::Relationship;
        let (_, _, term_co) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    pub(crate) fn working_directory(
        &self,
        anchor: vmux_service::protocol::ProcessId,
        tabs: &Query<&vmux_layout::tab::Tab>,
    ) -> Option<std::path::PathBuf> {
        let pane = self.agent_pane(anchor)?;
        let path = vmux_layout::tab::ancestor_tab_startup_dir(pane, &self.child_of, tabs)?;
        vmux_setting::StartupDir::from_tab(&path)
            .ok()
            .map(|dir| dir.path)
    }

    fn agent_kind(&self, anchor: vmux_service::protocol::ProcessId) -> Option<AgentKind> {
        let (entity, _, _) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.kinds.get(entity).ok().map(|session| session.kind)
    }

    pub(crate) fn claim_browser_pane(
        &mut self,
        anchor: vmux_service::protocol::ProcessId,
    ) -> Option<(Entity, Option<Entity>)> {
        let pane = self.browser_pane_for(self.agent_pane(anchor)?)?;
        let kind = self.agent_kind(anchor);
        let profile = vmux_layout::active_panes::ProfileId::Agent(format!("{anchor:?}"));
        let stack = self
            .active
            .get(&profile)
            .filter(|active| active.pane == Some(pane))
            .and_then(|active| active.stack);
        self.activate
            .write(vmux_layout::active_panes::ActivatePane {
                profile,
                active: vmux_layout::active_panes::ActiveStack {
                    tab: None,
                    pane: Some(pane),
                    stack,
                    kind,
                },
            });
        Some((pane, stack))
    }

    pub(crate) fn resolve_pane(
        &mut self,
        pane: &Option<String>,
        anchor: &Option<vmux_service::protocol::ProcessId>,
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
    use crate::host::test_support::spawn_stack_in_pane;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_layout::pane::PaneSplit;
    use vmux_service::protocol::ProcessId;
    use vmux_terminal::Terminal;

    #[derive(Resource)]
    pub(crate) struct BrowserPaneClaimInput {
        anchor: ProcessId,
    }

    #[derive(Resource, Default)]
    pub(crate) struct BrowserPaneClaimOutput(Option<Entity>);

    pub(crate) fn claim_browser_pane_test_system(
        input: Res<BrowserPaneClaimInput>,
        mut resolve: AgentBrowserResolve,
        mut out: ResMut<BrowserPaneClaimOutput>,
    ) {
        out.0 = resolve
            .claim_browser_pane(input.anchor)
            .map(|(pane, _)| pane);
    }

    pub(crate) fn browser_claim_app() -> (App, ProcessId, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .init_resource::<BrowserPaneClaimOutput>()
            .add_systems(Update, claim_browser_pane_test_system);
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: vmux_layout::pane::PaneSplitDirection::Row,
                },
            ))
            .id();
        let agent_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let agent_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((
            Terminal,
            anchor,
            AgentSession {
                kind: AgentKind::Codex,
            },
            ChildOf(agent_stack),
        ));
        app.insert_resource(BrowserPaneClaimInput { anchor });
        (app, anchor, split)
    }

    #[test]
    pub(crate) fn agent_working_directory_comes_from_its_tab() {
        let cwd = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin));
        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "Project".into(),
                startup_dir: Some(cwd.path().to_string_lossy().into_owned()),
            })
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((
            Terminal,
            anchor,
            AgentSession {
                kind: AgentKind::Codex,
            },
            ChildOf(stack),
        ));

        let resolved = app
            .world_mut()
            .run_system_once(
                move |resolve: AgentBrowserResolve, tabs: Query<&vmux_layout::tab::Tab>| {
                    resolve.working_directory(anchor, &tabs)
                },
            )
            .unwrap();

        assert_eq!(resolved, Some(cwd.path().canonicalize().unwrap()));
    }

    #[test]
    pub(crate) fn browser_pane_claim_ignores_mixed_file_browser_pane() {
        let (mut app, _anchor, split) = browser_claim_app();
        let mixed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        spawn_stack_in_pane(&mut app, mixed_pane, "file:///repo/src/main.rs");
        let browser_stack = spawn_stack_in_pane(&mut app, mixed_pane, "https://example.com");
        app.world_mut()
            .entity_mut(browser_stack)
            .insert(vmux_layout::Browser);

        app.update();

        assert_eq!(app.world().resource::<BrowserPaneClaimOutput>().0, None);
    }

    #[test]
    pub(crate) fn browser_pane_claim_prefers_pure_browser_pane_over_mixed_pane() {
        let (mut app, _anchor, split) = browser_claim_app();
        let mixed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        spawn_stack_in_pane(&mut app, mixed_pane, "file:///repo/src/main.rs");
        let mixed_browser = spawn_stack_in_pane(&mut app, mixed_pane, "https://mixed.example");
        app.world_mut()
            .entity_mut(mixed_browser)
            .insert(vmux_layout::Browser);
        let pure_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
        let pure_browser = spawn_stack_in_pane(&mut app, pure_pane, "https://pure.example");
        app.world_mut()
            .entity_mut(pure_browser)
            .insert(vmux_layout::Browser);

        app.update();

        assert_eq!(
            app.world().resource::<BrowserPaneClaimOutput>().0,
            Some(pure_pane)
        );
    }
}
