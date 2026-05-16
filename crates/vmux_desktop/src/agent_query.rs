use crate::{
    agent::AgentQueryRequest,
    layout::{
        pane::{Pane, PaneSplit},
        stack::{FocusedStack, Stack},
        tab::Tab,
    },
    processes_monitor::ServiceProcessList,
    terminal::{ServiceClient, Terminal},
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::{
    AgentQuery, AgentQueryResult, ClientMessage, FocusedInfo, PaneInfo, ProcessId, SpaceInfo,
    StateSnapshot, TabInfo, TerminalInfo,
};

type TerminalQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static ProcessId), With<Terminal>>;
type StackQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static Children, Option<&'static PageMetadata>), With<Stack>>;
type PaneQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static Children), (With<Pane>, Without<PaneSplit>)>;
type TabQuery<'world, 'state> = Query<'world, 'state, (Entity, &'static Tab, &'static Children)>;
type ChildrenQuery<'world, 'state> = Query<'world, 'state, &'static Children>;

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    tabs: TabQuery,
    panes: PaneQuery,
    stacks: StackQuery,
    terminals: TerminalQuery,
    all_children: ChildrenQuery,
    process_list: Option<Res<ServiceProcessList>>,
    settings: Res<crate::settings::AppSettings>,
    focused: Option<Res<FocusedStack>>,
) {
    let Some(service) = service else { return };
    let Some(focused) = focused else { return };
    let default_pl = ServiceProcessList::default();
    let process_list = process_list.as_deref().unwrap_or(&default_pl);

    for request in reader.read() {
        let result = match request.query {
            AgentQuery::GetState => AgentQueryResult::State(build_state_snapshot(
                &tabs,
                &panes,
                &stacks,
                &terminals,
                &all_children,
                &focused,
            )),
            AgentQuery::ListTabs => AgentQueryResult::Tabs(collect_stacks(&stacks, &terminals)),
            AgentQuery::ListSpaces => AgentQueryResult::Spaces(collect_tabs(
                &tabs,
                &panes,
                &stacks,
                &terminals,
                &all_children,
                &focused,
            )),
            AgentQuery::ListTerminals => {
                AgentQueryResult::Terminals(collect_terminals(&terminals, process_list))
            }
            AgentQuery::GetFocused => AgentQueryResult::Focused(focused_info(&focused)),
            AgentQuery::GetSettings => {
                AgentQueryResult::Settings(crate::settings::serialize_settings_to_json(&settings))
            }
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn focused_info(focused: &FocusedStack) -> FocusedInfo {
    FocusedInfo {
        space: focused.tab.map(|e| e.to_bits().to_string()),
        pane: focused.pane.map(|e| e.to_bits().to_string()),
        tab: focused.stack.map(|e| e.to_bits().to_string()),
    }
}

fn collect_terminals(
    terminals: &TerminalQuery,
    process_list: &ServiceProcessList,
) -> Vec<TerminalInfo> {
    terminals
        .iter()
        .map(|(entity, pid)| {
            let info = process_list.processes.iter().find(|p| p.id == *pid);
            TerminalInfo {
                id: entity.to_bits().to_string(),
                cwd: info.map(|i| i.cwd.clone()).unwrap_or_default(),
                pid: info.map(|i| i.pid).unwrap_or(0),
            }
        })
        .collect()
}

fn stack_kind(children: &Children, terminals: &TerminalQuery) -> &'static str {
    if children
        .iter()
        .any(|child| terminals.iter().any(|(t, _)| t == child))
    {
        "terminal"
    } else {
        "browser"
    }
}

fn stack_info(
    entity: Entity,
    children: &Children,
    page: Option<&PageMetadata>,
    terminals: &TerminalQuery,
) -> TabInfo {
    TabInfo {
        id: entity.to_bits().to_string(),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        url: page.map(|p| p.url.clone()).unwrap_or_default(),
        kind: stack_kind(children, terminals).to_string(),
    }
}

fn collect_stacks(stacks: &StackQuery, terminals: &TerminalQuery) -> Vec<TabInfo> {
    stacks
        .iter()
        .map(|(entity, children, page)| stack_info(entity, children, page, terminals))
        .collect()
}

fn collect_tabs(
    tabs: &TabQuery,
    panes: &PaneQuery,
    stacks: &StackQuery,
    terminals: &TerminalQuery,
    all_children: &ChildrenQuery,
    focused: &FocusedStack,
) -> Vec<SpaceInfo> {
    tabs.iter()
        .map(|(tab_entity, tab, tab_children)| {
            let leaf_panes = gather_leaf_panes(tab_children, panes, all_children);
            SpaceInfo {
                id: tab_entity.to_bits().to_string(),
                name: tab.name.clone(),
                panes: leaf_panes
                    .into_iter()
                    .map(|(pane_entity, pane_children)| PaneInfo {
                        id: pane_entity.to_bits().to_string(),
                        tabs: pane_children
                            .iter()
                            .filter_map(|grandchild| stacks.get(grandchild).ok())
                            .map(|(stack_entity, stack_children, page)| {
                                stack_info(stack_entity, stack_children, page, terminals)
                            })
                            .collect(),
                    })
                    .collect(),
                active: focused.tab == Some(tab_entity),
            }
        })
        .collect()
}

fn gather_leaf_panes<'a>(
    roots: &'a Children,
    panes: &'a PaneQuery,
    all_children: &'a ChildrenQuery,
) -> Vec<(Entity, &'a Children)> {
    let mut out = Vec::new();
    let mut stack: Vec<Entity> = roots.iter().collect();
    while let Some(entity) = stack.pop() {
        if let Ok((pane_entity, pane_children)) = panes.get(entity) {
            out.push((pane_entity, pane_children));
        } else if let Ok(children) = all_children.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }
    out
}

fn build_state_snapshot(
    tabs: &TabQuery,
    panes: &PaneQuery,
    stacks: &StackQuery,
    terminals: &TerminalQuery,
    all_children: &ChildrenQuery,
    focused: &FocusedStack,
) -> StateSnapshot {
    StateSnapshot {
        spaces: collect_tabs(tabs, panes, stacks, terminals, all_children, focused),
        focused: focused_info(focused),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_info_propagates_entity_ids() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(FocusedStack::default());
        let tab = app.world_mut().spawn_empty().id();
        let pane = app.world_mut().spawn_empty().id();
        let stack = app.world_mut().spawn_empty().id();
        {
            let mut focus = app.world_mut().resource_mut::<FocusedStack>();
            focus.tab = Some(tab);
            focus.pane = Some(pane);
            focus.stack = Some(stack);
        }
        let info = focused_info(app.world().resource::<FocusedStack>());
        assert_eq!(info.space, Some(tab.to_bits().to_string()));
        assert_eq!(info.pane, Some(pane.to_bits().to_string()));
        assert_eq!(info.tab, Some(stack.to_bits().to_string()));
    }

    #[test]
    fn collect_tabs_descends_through_pane_splits() {
        use crate::layout::pane::PaneSplitDirection;
        use crate::layout::stack::Stack;
        use crate::layout::tab::Tab;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(FocusedStack::default());

        let tab = app.world_mut().spawn(Tab { name: "S1".into() }).id();
        let root_split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let nested_split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(root_split),
            ))
            .id();
        let leaf_a = app.world_mut().spawn((Pane, ChildOf(nested_split))).id();
        let leaf_b = app.world_mut().spawn((Pane, ChildOf(nested_split))).id();
        let leaf_c = app.world_mut().spawn((Pane, ChildOf(root_split))).id();
        let _stack_a = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(leaf_a)))
            .id();
        let _stack_b = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(leaf_b)))
            .id();
        let _stack_c = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(leaf_c)))
            .id();

        let world = app.world_mut();
        let root_children: Vec<Entity> = world.get::<Children>(tab).unwrap().iter().collect();
        let mut all_children_q = world.query::<&Children>();
        let mut leaf_panes_q =
            world.query_filtered::<(Entity, &Children), (With<Pane>, Without<PaneSplit>)>();

        let mut leaves: Vec<Entity> = Vec::new();
        let mut entity_stack: Vec<Entity> = root_children;
        while let Some(entity) = entity_stack.pop() {
            if let Ok((leaf_entity, _)) = leaf_panes_q.get(world, entity) {
                leaves.push(leaf_entity);
            } else if let Ok(children) = all_children_q.get(world, entity) {
                for child in children.iter() {
                    entity_stack.push(child);
                }
            }
        }
        leaves.sort();
        let mut expected = vec![leaf_a, leaf_b, leaf_c];
        expected.sort();
        assert_eq!(
            leaves, expected,
            "should find all 3 leaf panes through nested splits"
        );
    }
}
