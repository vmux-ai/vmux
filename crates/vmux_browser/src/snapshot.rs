use crate::PendingNavigationSnapshot;
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, SnapshotResult};
use vmux_core::LastActivatedAt;
use vmux_core::browser::{
    BrowserNavigationSnapshotResponse, BrowserSnapshotRequest, BrowserSnapshotResponse,
};
use vmux_core::dom_snapshot::{RawSnapshot, shape_snapshot};
use vmux_core::terminal::{ProcessExited, Terminal};
use vmux_layout::active_pane::ActivePaneQuery;
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::{Stack, active_stack_in_pane};
use vmux_layout::target::active_webview_for_tab;
use vmux_layout::{Browser, Loading};

pub(crate) struct SnapshotPlugin;

#[derive(Component)]
struct NavigationSnapshotResponseRoute {
    request_id: [u8; 16],
}

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            drive_pending_nav_snapshots
                .after(crate::apply_pending_navigation_updates)
                .after(vmux_command::WriteCommandRequests),
        )
        .add_systems(
            Update,
            (start_snapshots, shape_snapshot_results)
                .chain()
                .after(crate::scroll::run_scrolls)
                .after(vmux_command::WriteCommandRequests),
        );
    }
}

fn hex(id: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in id {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn parse_hex(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[allow(clippy::too_many_arguments)]
fn start_snapshots(
    mut reader: MessageReader<BrowserSnapshotRequest>,
    cef_browsers: NonSend<Browsers>,
    active: ActivePaneQuery,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    navigation_routes: Query<(Entity, &NavigationSnapshotResponseRoute)>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
    mut navigation_writer: MessageWriter<BrowserNavigationSnapshotResponse>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let explicit_target = request.webview.is_some() || request.pane.is_some();
        let webview = if let Some(webview) = request.webview {
            browsers.contains(webview).then_some(webview)
        } else if let Some(target) = request.pane.as_deref() {
            vmux_layout::target::parse_browser_target(target, &panes, &stacks).and_then(|target| {
                vmux_layout::target::webview_for_target(
                    target,
                    &pane_children,
                    &stack_ts,
                    &browsers,
                    &terminals,
                )
            })
        } else {
            default_browser(
                &active,
                &panes,
                &terminals,
                &browsers,
                &pane_children,
                &stack_ts,
            )
        };
        let sent = webview
            .map(|webview| cef_browsers.request_snapshot(&webview, &hex(&request.request_id)))
            .unwrap_or(false);
        if !sent {
            let message = if explicit_target {
                "browser target not found"
            } else {
                "no browser page to snapshot"
            };
            let result = Err(message.to_string());
            let navigation = navigation_routes
                .iter()
                .find(|(_, route)| route.request_id == request.request_id);
            if let Some((entity, _)) = navigation {
                navigation_writer.write(BrowserNavigationSnapshotResponse {
                    request_id: request.request_id,
                    result,
                });
                commands.entity(entity).despawn();
                continue;
            }
            writer.write(BrowserSnapshotResponse {
                request_id: request.request_id,
                result,
            });
        }
    }
}

pub(crate) fn default_browser(
    active: &ActivePaneQuery,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: &Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> Option<Entity> {
    active
        .local()
        .pane
        .filter(|pane| panes.contains(*pane))
        .and_then(|pane| {
            active_webview_for_tab(
                active_stack_in_pane(pane, pane_children, stack_ts),
                browsers,
                terminals,
            )
        })
        .or_else(|| most_recent_browser(browsers, terminals, stack_ts))
}

pub(crate) fn most_recent_browser(
    browsers: &Query<(Entity, &ChildOf), With<Browser>>,
    terminals: &Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> Option<Entity> {
    browsers
        .iter()
        .filter_map(|(entity, child_of)| {
            if terminals.iter().any(|(t, _)| t == entity) {
                return None;
            }
            let (_, ts) = stack_ts.get(child_of.get()).ok()?;
            Some((entity, ts.0))
        })
        .max_by_key(|&(_, ts)| ts)
        .map(|(entity, _)| entity)
}

pub(crate) fn drive_pending_nav_snapshots(
    time: Res<Time>,
    mut pending: Query<(Entity, &mut PendingNavigationSnapshot)>,
    loading_q: Query<(), With<Loading>>,
    alive_q: Query<(), With<Browser>>,
    ready_q: Query<(), With<vmux_core::page::PageReady>>,
    mut snapshot_writer: MessageWriter<BrowserSnapshotRequest>,
    mut commands: Commands,
) {
    if pending.is_empty() {
        return;
    }
    let now = time.elapsed();
    for (entity, mut nav) in &mut pending {
        let alive = alive_q.contains(nav.webview);
        let ready = ready_q.contains(nav.webview);
        let loading = loading_q.contains(nav.webview);
        if loading {
            nav.saw_loading = true;
        }
        let elapsed = now.saturating_sub(nav.started).as_secs_f32();
        let settled = nav.saw_loading && !loading;
        let assume_instant = !nav.saw_loading && elapsed > 2.0;
        let timed_out = elapsed > 10.0;
        if !alive || ready && (settled || assume_instant) || timed_out {
            snapshot_writer.write(BrowserSnapshotRequest {
                request_id: nav.request_id,
                pane: nav.pane.clone(),
                webview: Some(nav.webview),
            });
            commands
                .entity(entity)
                .remove::<PendingNavigationSnapshot>()
                .insert(NavigationSnapshotResponseRoute {
                    request_id: nav.request_id,
                });
        }
    }
}

fn shape_snapshot_results(
    mut reader: MessageReader<SnapshotResult>,
    navigation_routes: Query<(Entity, &NavigationSnapshotResponseRoute)>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
    mut navigation_writer: MessageWriter<BrowserNavigationSnapshotResponse>,
    mut commands: Commands,
) {
    for result in reader.read() {
        let Some(request_id) = parse_hex(&result.request_id) else {
            continue;
        };
        let mapped = serde_json::from_str::<RawSnapshot>(&result.json)
            .map(|raw| serde_json::to_string(&shape_snapshot(raw)).unwrap_or_default())
            .map_err(|e| format!("snapshot parse error: {e}"));
        let navigation = navigation_routes
            .iter()
            .find(|(_, route)| route.request_id == request_id);
        if let Some((entity, _)) = navigation {
            navigation_writer.write(BrowserNavigationSnapshotResponse {
                request_id,
                result: mapped,
            });
            commands.entity(entity).despawn();
            continue;
        }
        writer.write(BrowserSnapshotResponse {
            request_id,
            result: mapped,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn snapshot_results_follow_the_request_entity_route() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SnapshotResult>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserNavigationSnapshotResponse>()
            .add_systems(Update, shape_snapshot_results);

        let navigation_id = [1; 16];
        let query_id = [2; 16];
        let route = app
            .world_mut()
            .spawn(NavigationSnapshotResponseRoute {
                request_id: navigation_id,
            })
            .id();
        for request_id in [navigation_id, query_id] {
            app.world_mut()
                .resource_mut::<Messages<SnapshotResult>>()
                .write(SnapshotResult {
                    webview: Entity::PLACEHOLDER,
                    request_id: hex(&request_id),
                    json: "invalid".to_string(),
                });
        }

        app.update();

        let navigation = app
            .world_mut()
            .resource_mut::<Messages<BrowserNavigationSnapshotResponse>>()
            .drain()
            .collect::<Vec<_>>();
        let query = app
            .world_mut()
            .resource_mut::<Messages<BrowserSnapshotResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(navigation.len(), 1);
        assert_eq!(navigation[0].request_id, navigation_id);
        assert_eq!(query.len(), 1);
        assert_eq!(query[0].request_id, query_id);
        assert!(app.world().get_entity(route).is_err());
    }
}
