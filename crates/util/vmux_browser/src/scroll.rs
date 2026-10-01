use bevy::prelude::*;
use bevy_cef::prelude::Browsers;
use vmux_ecs::browser::{BrowserScrollRequest, BrowserSnapshotRequest};

use crate::snapshot::BrowserTarget;

pub(crate) struct ScrollPlugin;

#[derive(Component)]
pub(crate) struct ScrollSnapshotResponseRoute {
    pub request_id: [u8; 16],
}

impl Plugin for ScrollPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BrowserScrollRequest>().add_systems(
            Update,
            run_scrolls
                .in_set(crate::BrowserSystemSet::Scroll)
                .after(crate::BrowserSystemSet::DrivePendingNavigationSnapshots)
                .after(vmux_command::WriteCommandRequests),
        );
    }
}

fn run_scrolls(
    mut reader: MessageReader<BrowserScrollRequest>,
    cef_browsers: NonSend<Browsers>,
    targets: BrowserTarget,
    mut snap_writer: MessageWriter<BrowserSnapshotRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let webview = targets.resolve(None, request.pane.as_deref());
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
        commands.spawn(ScrollSnapshotResponseRoute {
            request_id: request.request_id,
        });
        snap_writer.write(BrowserSnapshotRequest {
            request_id: request.request_id,
            pane: request.pane.clone(),
            webview,
        });
    }
}
