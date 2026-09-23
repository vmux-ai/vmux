use std::path::Path;

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};
use vmux_core::{PageOpenRequest, PageOpenTarget};

use crate::event::{GitAppAction, GitAppRequest};

pub(super) struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GitCheckForUpdatesRequest>()
            .add_plugins(UiEventPlugin::<(GitAppRequest,)>::default())
            .add_observer(on_app_request);
    }
}

#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitCheckForUpdatesRequest;

fn on_app_request(
    trigger: On<BinReceive<GitAppRequest>>,
    child_of: Query<&ChildOf>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut update_requests: MessageWriter<GitCheckForUpdatesRequest>,
) {
    let target = child_of
        .get(trigger.event().webview)
        .ok()
        .and_then(|stack| child_of.get(stack.parent()).ok())
        .map(|pane| PageOpenTarget::NewStackInPane(pane.parent()))
        .unwrap_or(PageOpenTarget::ActiveStack);
    let url = match trigger.event().payload.action {
        GitAppAction::EditConfig => {
            let Ok(path) =
                super::runner::config_path(Path::new(&trigger.event().payload.repo_root))
            else {
                return;
            };
            let Ok(url) = url::Url::from_file_path(path) else {
                return;
            };
            url.to_string()
        }
        GitAppAction::CheckForUpdates => {
            update_requests.write(GitCheckForUpdatesRequest);
            return;
        }
    };
    page_open.write(PageOpenRequest {
        target,
        url,
        request_id: None,
    });
}
