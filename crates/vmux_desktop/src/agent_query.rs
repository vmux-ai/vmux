use crate::{
    agent::AgentQueryRequest,
    layout::{
        pane::{Pane, PaneSplit},
        space::Space,
        tab::{FocusedTab, Tab},
    },
    processes_monitor::ServiceProcessList,
    terminal::{ServiceClient, ServiceProcessHandle, Terminal},
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::{
    AgentQuery, AgentQueryResult, ClientMessage, FocusedInfo, PaneInfo, SpaceInfo, StateSnapshot,
    TabInfo, TerminalInfo,
};

type TerminalQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static ServiceProcessHandle), With<Terminal>>;
type TabQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static Children, Option<&'static PageMetadata>), With<Tab>>;
type PaneQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static Children), (With<Pane>, Without<PaneSplit>)>;
type SpaceQuery<'world, 'state> =
    Query<'world, 'state, (Entity, &'static Space, &'static Children)>;

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    spaces: SpaceQuery,
    panes: PaneQuery,
    tabs: TabQuery,
    terminals: TerminalQuery,
    process_list: Option<Res<ServiceProcessList>>,
    focused: Option<Res<FocusedTab>>,
) {
    let Some(service) = service else { return };
    let Some(focused) = focused else { return };
    let default_pl = ServiceProcessList::default();
    let process_list = process_list.as_deref().unwrap_or(&default_pl);

    for request in reader.read() {
        let result = match request.query {
            AgentQuery::GetState => AgentQueryResult::State(build_state_snapshot(
                &spaces,
                &panes,
                &tabs,
                &terminals,
                process_list,
                &focused,
            )),
            AgentQuery::ListTabs => AgentQueryResult::Tabs(collect_tabs(&tabs, &terminals)),
            AgentQuery::ListSpaces => AgentQueryResult::Spaces(collect_spaces(
                &spaces,
                &panes,
                &tabs,
                &terminals,
                &focused,
            )),
            AgentQuery::ListTerminals => {
                AgentQueryResult::Terminals(collect_terminals(&terminals, process_list))
            }
            AgentQuery::GetFocused => AgentQueryResult::Focused(focused_info(&focused)),
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn focused_info(focused: &FocusedTab) -> FocusedInfo {
    FocusedInfo {
        space: focused.space.map(|e| e.to_string()),
        pane: focused.pane.map(|e| e.to_string()),
        tab: focused.tab.map(|e| e.to_string()),
    }
}

fn collect_terminals(
    terminals: &TerminalQuery,
    process_list: &ServiceProcessList,
) -> Vec<TerminalInfo> {
    terminals
        .iter()
        .map(|(entity, handle)| {
            let info = process_list
                .processes
                .iter()
                .find(|p| p.id == handle.process_id);
            TerminalInfo {
                id: entity.to_string(),
                cwd: info.map(|i| i.cwd.clone()).unwrap_or_default(),
                pid: info.map(|i| i.pid).unwrap_or(0),
            }
        })
        .collect()
}

fn tab_kind(children: &Children, terminals: &TerminalQuery) -> &'static str {
    if children
        .iter()
        .any(|child| terminals.iter().any(|(t, _)| t == child))
    {
        "terminal"
    } else {
        "browser"
    }
}

fn tab_info(
    entity: Entity,
    children: &Children,
    page: Option<&PageMetadata>,
    terminals: &TerminalQuery,
) -> TabInfo {
    TabInfo {
        id: entity.to_string(),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        url: page.map(|p| p.url.clone()).unwrap_or_default(),
        kind: tab_kind(children, terminals).to_string(),
    }
}

fn collect_tabs(tabs: &TabQuery, terminals: &TerminalQuery) -> Vec<TabInfo> {
    tabs.iter()
        .map(|(entity, children, page)| tab_info(entity, children, page, terminals))
        .collect()
}

fn collect_spaces(
    spaces: &SpaceQuery,
    panes: &PaneQuery,
    tabs: &TabQuery,
    terminals: &TerminalQuery,
    focused: &FocusedTab,
) -> Vec<SpaceInfo> {
    spaces
        .iter()
        .map(|(space_entity, space, space_children)| SpaceInfo {
            id: space_entity.to_string(),
            name: space.name.clone(),
            panes: space_children
                .iter()
                .filter_map(|child| panes.get(child).ok())
                .map(|(pane_entity, pane_children)| PaneInfo {
                    id: pane_entity.to_string(),
                    tabs: pane_children
                        .iter()
                        .filter_map(|grandchild| tabs.get(grandchild).ok())
                        .map(|(tab_entity, tab_children, page)| {
                            tab_info(tab_entity, tab_children, page, terminals)
                        })
                        .collect(),
                })
                .collect(),
            active: focused.space == Some(space_entity),
        })
        .collect()
}

fn build_state_snapshot(
    spaces: &SpaceQuery,
    panes: &PaneQuery,
    tabs: &TabQuery,
    terminals: &TerminalQuery,
    _process_list: &ServiceProcessList,
    focused: &FocusedTab,
) -> StateSnapshot {
    StateSnapshot {
        spaces: collect_spaces(spaces, panes, tabs, terminals, focused),
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
        app.insert_resource(FocusedTab::default());
        let space = app.world_mut().spawn_empty().id();
        let pane = app.world_mut().spawn_empty().id();
        let tab = app.world_mut().spawn_empty().id();
        {
            let mut focus = app.world_mut().resource_mut::<FocusedTab>();
            focus.space = Some(space);
            focus.pane = Some(pane);
            focus.tab = Some(tab);
        }
        let info = focused_info(app.world().resource::<FocusedTab>());
        assert_eq!(info.space, Some(space.to_string()));
        assert_eq!(info.pane, Some(pane.to_string()));
        assert_eq!(info.tab, Some(tab.to_string()));
    }
}
