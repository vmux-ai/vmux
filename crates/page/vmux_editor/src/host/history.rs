use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::host::editing::FileView;
use crate::host::viewport::FileViewport;

pub(crate) struct EditorHistoryPlugin;

impl Plugin for EditorHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                show_traversed_file_view.in_set(vmux_core::host::page::HostHistorySet::Apply),
                record_file_view_visit.in_set(vmux_core::host::page::HostHistorySet::Record),
            ),
        );
    }
}

fn show_traversed_file_view(
    mut traversed: MessageReader<vmux_core::host::page::HostHistoryTraversed>,
    mut views: Query<(&mut FileView, &mut FileViewport, &mut PageMetadata)>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    for event in traversed.read() {
        let Ok((mut view, mut viewport, mut metadata)) = views.get_mut(event.webview) else {
            continue;
        };
        let Some(path) =
            vmux_core::file_url::FileUrl::parse(&event.entry.url).and_then(|url| url.path())
        else {
            continue;
        };
        view.navigate(
            event.webview,
            path,
            event.entry.top_line,
            &mut viewport,
            &mut metadata,
            &mut manager,
            &mut commands,
        );
    }
}

fn record_file_view_visit(
    mut views: Query<
        (
            &PageMetadata,
            &FileViewport,
            &mut vmux_core::host::page::HostHistory,
        ),
        With<FileView>,
    >,
) {
    for (metadata, viewport, mut history) in &mut views {
        if metadata.url.is_empty() || history.showing(&metadata.url, viewport.top_row) {
            continue;
        }
        history.observe(&metadata.url, viewport.top_row);
    }
}
