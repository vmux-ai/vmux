use crate::{
    agent::AgentQueryRequest,
    agent_layout::build_layout_snapshot,
    layout::{
        pane::{Pane, PaneSize, PaneSplit, Zoomed},
        stack::{FocusedStack, Stack},
        tab::Tab,
    },
    terminal::{ServiceClient, Terminal},
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::{AgentQuery, AgentQueryResult, ClientMessage};

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    spaces: Query<(Entity, &Tab, Option<&Children>)>,
    splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    terminals: Query<Entity, With<Terminal>>,
    pane_sizes: Query<&PaneSize>,
    zoomed: Query<&Zoomed>,
    settings: Res<crate::settings::AppSettings>,
    focused: Option<Res<FocusedStack>>,
) {
    let Some(service) = service else { return };
    let Some(focused) = focused else { return };

    for request in reader.read() {
        let result = match request.query {
            AgentQuery::ReadLayout => AgentQueryResult::Layout(build_layout_snapshot(
                &spaces,
                &splits,
                &leaves,
                &stacks,
                &pane_sizes,
                &terminals,
                &zoomed,
                &focused,
            )),
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
