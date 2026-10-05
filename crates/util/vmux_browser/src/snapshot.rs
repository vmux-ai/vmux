use crate::dom_snapshot::{RawSnapshot, Snapshot};
use crate::host::PendingNavigationSnapshot;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, SnapshotResult};
use vmux_ecs::browser::{
    BrowserNavigationSnapshotResponse, BrowserScrollResponse, BrowserSnapshotRequest,
    BrowserSnapshotResponse,
};
use vmux_layout::active_pane::ActivePaneQuery;
use vmux_layout::{Browser, Loading};

pub(crate) struct SnapshotPlugin;

#[derive(Component)]
struct NavigationSnapshotResponseRoute {
    request_id: [u8; 16],
}

struct SnapshotToken([u8; 16]);

impl std::fmt::Display for SnapshotToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for SnapshotToken {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 32 {
            return Err(());
        }
        let mut bytes = [0u8; 16];
        for index in 0..16 {
            bytes[index] =
                u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| ())?;
        }
        Ok(Self(bytes))
    }
}

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserScrollResponse>()
            .add_message::<BrowserNavigationSnapshotResponse>()
            .add_systems(
                Update,
                drive_pending_nav_snapshots
                    .in_set(crate::BrowserSystemSet::DrivePendingNavigationSnapshots)
                    .after(crate::BrowserSystemSet::ApplyPendingNavigation)
                    .after(vmux_command::WriteCommandRequests),
            )
            .add_systems(
                Update,
                (start_snapshots, shape_results)
                    .chain()
                    .after(crate::BrowserSystemSet::Scroll)
                    .after(vmux_command::WriteCommandRequests),
            );
    }
}

fn start_snapshots(
    mut reader: MessageReader<BrowserSnapshotRequest>,
    cef_browsers: NonSend<Browsers>,
    targets: BrowserTarget,
    navigation_routes: Query<(Entity, &NavigationSnapshotResponseRoute)>,
    scroll_routes: Query<(Entity, &crate::scroll::ScrollSnapshotResponseRoute)>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
    mut navigation_writer: MessageWriter<BrowserNavigationSnapshotResponse>,
    mut scroll_writer: MessageWriter<BrowserScrollResponse>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let explicit_target = request.webview.is_some() || request.pane.is_some();
        let webview = targets.resolve(request.webview, request.pane.as_deref());
        let sent = webview
            .map(|webview| {
                cef_browsers
                    .request_snapshot(&webview, &SnapshotToken(request.request_id).to_string())
            })
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
            let scroll = scroll_routes
                .iter()
                .find(|(_, route)| route.request_id == request.request_id);
            if let Some((entity, _)) = scroll {
                scroll_writer.write(BrowserScrollResponse {
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

#[derive(SystemParam)]
pub(crate) struct BrowserTarget<'w, 's> {
    active: ActivePaneQuery<'w, 's>,
    targets: vmux_layout::target::BrowserTargets<'w, 's, Browser>,
}

impl BrowserTarget<'_, '_> {
    pub(crate) fn resolve(&self, webview: Option<Entity>, pane: Option<&str>) -> Option<Entity> {
        if let Some(webview) = webview {
            return self.targets.contains_webview(webview).then_some(webview);
        }
        if pane.is_some() {
            return self.resolve_pane(pane);
        }
        self.resolve_pane(None)
            .or_else(|| self.targets.most_recent_webview())
    }

    pub(crate) fn resolve_pane(&self, pane: Option<&str>) -> Option<Entity> {
        let target = match pane {
            Some(target) => self.targets.target(target),
            None => self
                .active
                .local()
                .pane
                .filter(|pane| self.targets.contains_pane(*pane))
                .map(vmux_layout::target::BrowserTarget::Pane),
        }?;
        self.targets.webview(target)
    }
}

fn drive_pending_nav_snapshots(
    time: Res<Time>,
    mut pending: Query<(Entity, &mut PendingNavigationSnapshot)>,
    loading_q: Query<(), With<Loading>>,
    alive_q: Query<(), With<Browser>>,
    ready_q: Query<(), With<vmux_ecs::page::PageReady>>,
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

fn shape_results(
    mut reader: MessageReader<SnapshotResult>,
    navigation_routes: Query<(Entity, &NavigationSnapshotResponseRoute)>,
    scroll_routes: Query<(Entity, &crate::scroll::ScrollSnapshotResponseRoute)>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
    mut navigation_writer: MessageWriter<BrowserNavigationSnapshotResponse>,
    mut scroll_writer: MessageWriter<BrowserScrollResponse>,
    mut commands: Commands,
) {
    for result in reader.read() {
        let Ok(SnapshotToken(request_id)) = result.request_id.parse() else {
            continue;
        };
        let mapped = serde_json::from_str::<RawSnapshot>(&result.json)
            .map(|raw| serde_json::to_string(&Snapshot::from(raw)).unwrap_or_default())
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
        let scroll = scroll_routes
            .iter()
            .find(|(_, route)| route.request_id == request_id);
        if let Some((entity, _)) = scroll {
            scroll_writer.write(BrowserScrollResponse {
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
            .add_message::<BrowserScrollResponse>()
            .add_message::<BrowserNavigationSnapshotResponse>()
            .add_systems(Update, shape_results);

        let navigation_id = [1; 16];
        let query_id = [2; 16];
        let scroll_id = [3; 16];
        let navigation_route = app
            .world_mut()
            .spawn(NavigationSnapshotResponseRoute {
                request_id: navigation_id,
            })
            .id();
        let scroll_route = app
            .world_mut()
            .spawn(crate::scroll::ScrollSnapshotResponseRoute {
                request_id: scroll_id,
            })
            .id();
        for request_id in [navigation_id, query_id, scroll_id] {
            app.world_mut()
                .resource_mut::<Messages<SnapshotResult>>()
                .write(SnapshotResult {
                    webview: Entity::PLACEHOLDER,
                    request_id: SnapshotToken(request_id).to_string(),
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
        let scroll = app
            .world_mut()
            .resource_mut::<Messages<BrowserScrollResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(navigation.len(), 1);
        assert_eq!(navigation[0].request_id, navigation_id);
        assert_eq!(query.len(), 1);
        assert_eq!(query[0].request_id, query_id);
        assert_eq!(scroll.len(), 1);
        assert_eq!(scroll[0].request_id, scroll_id);
        assert!(app.world().get_entity(navigation_route).is_err());
        assert!(app.world().get_entity(scroll_route).is_err());
    }
}
