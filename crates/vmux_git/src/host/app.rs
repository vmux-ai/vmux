use std::path::Path;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::{PageOpenRequest, PageOpenTarget};

use crate::event::{GitConfigEditRequest, GitUpdateCheckRequest};

pub(super) struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GitCheckForUpdatesRequest>()
            .add_plugins(UiEventPlugin::<(GitConfigEditRequest, GitUpdateCheckRequest)>::default())
            .add_observer(on_config_edit_request)
            .add_observer(on_update_check_request);
    }
}

#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitCheckForUpdatesRequest;

fn on_config_edit_request(
    trigger: On<UiInput<GitConfigEditRequest>>,
    child_of: Query<&ChildOf>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    let target = child_of
        .get(trigger.event().webview)
        .ok()
        .and_then(|stack| child_of.get(stack.parent()).ok())
        .map(|pane| PageOpenTarget::NewStackInPane(pane.parent()))
        .unwrap_or(PageOpenTarget::ActiveStack);
    let Ok(path) = super::runner::config_path(Path::new(&trigger.event().payload.repo_root)) else {
        return;
    };
    let Ok(url) = url::Url::from_file_path(path) else {
        return;
    };
    page_open.write(PageOpenRequest {
        target,
        url: url.to_string(),
        request_id: None,
    });
}

fn on_update_check_request(
    _trigger: On<UiInput<GitUpdateCheckRequest>>,
    mut requests: MessageWriter<GitCheckForUpdatesRequest>,
) {
    requests.write(GitCheckForUpdatesRequest);
}
