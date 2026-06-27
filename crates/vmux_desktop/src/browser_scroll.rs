use bevy::prelude::*;
use bevy_cef::prelude::Browsers;
use vmux_agent::{BrowserScrollRequest, BrowserSnapshotRequest};
use vmux_core::LastActivatedAt;
use vmux_core::terminal::{ProcessExited, Terminal};
use vmux_layout::Browser;
use vmux_layout::active_panes::ActivePanes;
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::{Stack, active_stack_in_pane};
use vmux_layout::target::{active_webview_for_tab, parse_pane_target};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_scrolls(
    mut reader: MessageReader<BrowserScrollRequest>,
    cef_browsers: NonSend<Browsers>,
    active: Res<ActivePanes>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut snap_writer: MessageWriter<BrowserSnapshotRequest>,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => parse_pane_target(s, &panes),
            None => active.local().pane.filter(|p| panes.contains(*p)),
        };
        let webview = target.and_then(|pane| {
            active_webview_for_tab(
                active_stack_in_pane(pane, &pane_children, &stack_ts),
                &browsers,
                &terminals,
            )
        });
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
        });
    }
}
