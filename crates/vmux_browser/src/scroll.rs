use bevy::prelude::*;
use bevy_cef::prelude::Browsers;
use vmux_core::LastActivatedAt;
use vmux_core::browser::{BrowserScrollRequest, BrowserSnapshotRequest};
use vmux_core::terminal::{ProcessExited, Terminal};
use vmux_layout::Browser;
use vmux_layout::active_panes::ActivePanes;
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::{Stack, active_stack_in_pane};
use vmux_layout::target::active_webview_for_tab;

/// Scrolls the page an agent asks about before it is snapshotted, so the two land in one
/// frame. Sits between [`crate::snapshot::drive_pending_nav_snapshots`] and the capture
/// [`crate::snapshot::SnapshotPlugin`] orders after it.
pub(crate) struct ScrollPlugin;

impl Plugin for ScrollPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            run_scrolls
                .after(crate::snapshot::drive_pending_nav_snapshots)
                .after(vmux_command::WriteAppCommands),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_scrolls(
    mut reader: MessageReader<BrowserScrollRequest>,
    cef_browsers: NonSend<Browsers>,
    active: Res<ActivePanes>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut snap_writer: MessageWriter<BrowserSnapshotRequest>,
) {
    for request in reader.read() {
        let webview = request
            .pane
            .as_deref()
            .and_then(|target| vmux_layout::target::parse_browser_target(target, &panes, &stacks))
            .and_then(|target| {
                vmux_layout::target::webview_for_target(
                    target,
                    &pane_children,
                    &stack_ts,
                    &browsers,
                    &terminals,
                )
            })
            .or_else(|| {
                active
                    .local()
                    .pane
                    .filter(|p| panes.contains(*p))
                    .and_then(|pane| {
                        active_webview_for_tab(
                            active_stack_in_pane(pane, &pane_children, &stack_ts),
                            &browsers,
                            &terminals,
                        )
                    })
            })
            .or_else(|| crate::snapshot::most_recent_browser(&browsers, &terminals, &stack_ts));
        if let Some(webview) = webview {
            let js = match (request.to.as_deref(), request.delta) {
                (Some("top"), _) => "window.scrollTo(0,0)".to_string(),
                (Some("bottom"), _) => {
                    "window.scrollTo(0,document.documentElement.scrollHeight)".to_string()
                }
                (_, Some(delta)) => format!("window.scrollBy(0,{delta})"),
                _ => "void 0".to_string(),
            };
            cef_browsers.execute_js(&webview, &js);
        }
        snap_writer.write(BrowserSnapshotRequest {
            request_id: request.request_id,
            pane: request.pane.clone(),
            webview,
        });
    }
}
