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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::BinReceive;
    use vmux_core::PageOpenId;
    use vmux_core::event::FileOpenEvent;
    use vmux_core::host::page::{HostHistory, HostHistoryDelta, HostHistoryStep};
    use vmux_core::page_open::PageOpenTask;

    use crate::host::navigation::EditorNavigationPlugin;
    use crate::host::page_open::EditorPageOpenPlugin;

    struct Editor {
        app: App,
        view: Entity,
        dir: tempfile::TempDir,
    }

    impl Editor {
        fn opened(name: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(vmux_core::CorePlugin)
                .add_plugins(EditorNavigationPlugin)
                .add_plugins(EditorHistoryPlugin)
                .add_plugins(EditorPageOpenPlugin);
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));
            let stack = app.world_mut().spawn_empty().id();
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: format!("file://{}", dir.path().join(name).display()),
                request_id: None,
            });
            app.update();
            app.update();
            let view = app
                .world_mut()
                .query_filtered::<Entity, With<FileView>>()
                .single(app.world())
                .expect("opening a file:// page spawns one file view");
            Self { app, view, dir }
        }

        fn open(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(BinReceive {
                webview: self.view,
                payload: FileOpenEvent { path },
            });
            self.app.update();
        }

        fn scroll_to(&mut self, top_row: u32) {
            self.app
                .world_mut()
                .get_mut::<FileViewport>(self.view)
                .expect("a file view has a viewport")
                .top_row = top_row;
            self.app.update();
        }

        fn step(&mut self, delta: HostHistoryDelta) {
            self.app.world_mut().write_message(HostHistoryStep {
                webview: self.view,
                delta,
            });
            self.app.update();
        }

        fn showing(&self) -> String {
            self.app
                .world()
                .get::<FileView>(self.view)
                .expect("a file view")
                .path
                .file_name()
                .expect("a named file")
                .to_string_lossy()
                .into_owned()
        }

        fn top_row(&self) -> u32 {
            self.app
                .world()
                .get::<FileViewport>(self.view)
                .expect("a file view has a viewport")
                .top_row
        }

        fn history(&self) -> &HostHistory {
            self.app
                .world()
                .get::<HostHistory>(self.view)
                .expect("a file view owns its history")
        }
    }

    #[test]
    fn back_and_forward_walk_the_files_the_editor_opened() {
        let mut editor = Editor::opened("a.rs");
        editor.open("b.rs");
        editor.open("c.rs");
        assert!(editor.history().can_go_back());
        assert!(!editor.history().can_go_forward());

        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "b.rs");
        assert!(editor.history().can_go_forward());

        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "a.rs");
        assert!(!editor.history().can_go_back());

        editor.step(HostHistoryDelta::Forward);
        assert_eq!(editor.showing(), "b.rs");
    }

    #[test]
    fn opening_a_file_after_going_back_drops_the_forward_trail() {
        let mut editor = Editor::opened("a.rs");
        editor.open("b.rs");
        editor.open("c.rs");
        editor.step(HostHistoryDelta::Back);
        editor.step(HostHistoryDelta::Back);

        editor.open("d.rs");

        assert!(!editor.history().can_go_forward());
        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "a.rs");
        editor.step(HostHistoryDelta::Forward);
        assert_eq!(editor.showing(), "d.rs");
    }

    #[test]
    fn going_back_lands_on_the_line_the_file_was_left_at() {
        let mut editor = Editor::opened("a.rs");
        editor.scroll_to(120);
        editor.open("b.rs");
        assert_eq!(editor.top_row(), 0);

        editor.step(HostHistoryDelta::Back);

        assert_eq!(editor.showing(), "a.rs");
        assert_eq!(editor.top_row(), 120);
    }
}
