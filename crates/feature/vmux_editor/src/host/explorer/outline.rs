use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::event::{OutlineEvent, OutlineRow};

use super::OutlineDirty;
use crate::host::editor::{Editor, FileView};
use crate::host::file_lifecycle::EditorFileLoadedSet;
use crate::host::markdown::ParsedNote;
use crate::host::status::FileInitialMetaSent;

pub(super) struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (clear_on_file_change, emit_markdown)
                .chain()
                .after(EditorFileLoadedSet),
        );
    }
}

pub(crate) struct OutlineRows(Vec<OutlineRow>);

impl OutlineRows {
    pub(crate) fn from_markdown(text: &str) -> Self {
        let mut rows: Vec<OutlineRow> = Vec::new();
        let mut in_fence = false;
        for (line_index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                continue;
            }
            let hashes = trimmed
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if !(1..=6).contains(&hashes) || !trimmed[hashes..].starts_with(' ') {
                continue;
            }
            let line = line_index as u32;
            let depth = (hashes - 1) as u16;
            for open in rows.iter_mut().rev() {
                if open.depth < depth {
                    break;
                }
                if open.end_line == OutlineRow::OPEN_END {
                    open.end_line = line.saturating_sub(1);
                }
            }
            rows.push(OutlineRow {
                name: trimmed[hashes..].trim().to_string(),
                kind: 15,
                line,
                end_line: OutlineRow::OPEN_END,
                depth,
            });
        }
        Self(rows)
    }

    pub(crate) fn from_lsp(value: &serde_json::Value) -> Self {
        let mut rows = Vec::new();
        if let Some(items) = value.as_array() {
            for item in items {
                Self::push_lsp(item, 0, &mut rows);
            }
        }
        Self(rows)
    }

    pub(crate) fn into_vec(self) -> Vec<OutlineRow> {
        self.0
    }

    fn push_lsp(item: &serde_json::Value, depth: u16, rows: &mut Vec<OutlineRow>) {
        let name = item
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            return;
        }
        let kind = item
            .get("kind")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u8;
        let span = SymbolSpan::from(item);
        rows.push(OutlineRow {
            name,
            kind,
            line: span.line,
            end_line: span.end_line,
            depth,
        });
        if let Some(children) = item.get("children").and_then(|value| value.as_array()) {
            for child in children {
                Self::push_lsp(child, depth + 1, rows);
            }
        }
    }
}

struct SymbolSpan {
    line: u32,
    end_line: u32,
}

impl From<&serde_json::Value> for SymbolSpan {
    fn from(item: &serde_json::Value) -> Self {
        let line = Self::pick(
            item,
            &[
                "/selectionRange/start/line",
                "/range/start/line",
                "/location/range/start/line",
            ],
        )
        .unwrap_or(0);
        let end_line = Self::pick(item, &["/range/end/line", "/location/range/end/line"])
            .filter(|end_line| *end_line >= line)
            .unwrap_or(OutlineRow::OPEN_END);
        Self { line, end_line }
    }
}

impl SymbolSpan {
    fn pick(item: &serde_json::Value, paths: &[&str]) -> Option<u32> {
        for path in paths {
            if let Some(found) = item.pointer(path).and_then(|value| value.as_u64()) {
                return Some(found as u32);
            }
        }
        None
    }
}

type DirtyOutline = (
    With<OutlineDirty>,
    With<vmux_ecs::page::PageReady>,
    With<FileInitialMetaSent>,
);

fn emit_markdown(
    query: Query<(Entity, &Editor, &FileView), DirtyOutline>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, edit, file) in &query {
        if !ParsedNote::supports(&file.path) {
            commands.entity(entity).remove::<OutlineDirty>();
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let items = OutlineRows::from_markdown(&edit.core.buffer.text()).into_vec();
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            entity,
            &OutlineEvent { items },
        ));
        commands.entity(entity).remove::<OutlineDirty>();
    }
}

fn clear_on_file_change(
    query: Query<Entity, (With<FileView>, Changed<FileView>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for entity in &query {
        if browsers.can_emit_to(&entity) {
            commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                entity,
                &OutlineEvent { items: Vec::new() },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{EditCore, highlight_cache::HighlightCache};
    use vmux_api::UiState;
    use vmux_ecs::event::{FileDocumentKind, FileMetaEvent, FileUiState};

    #[derive(Resource, Default)]
    struct Published(FileUiState);

    impl Published {
        fn capture(event: On<vmux_ecs::FileUiStateWrite>, mut state: ResMut<Self>) {
            let previous = std::mem::take(&mut state.0);
            state.0 =
                FileUiState::from_updates(Some(previous), vec![event.event().update().clone()]);
        }
    }

    struct OutlinePage {
        app: App,
        page: Entity,
    }

    impl OutlinePage {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, OutlinePlugin))
                .init_non_send::<Browsers>()
                .init_resource::<Published>()
                .add_observer(Published::capture);
            let path = std::path::PathBuf::from("/repo/one.md");
            let editor = Self::editor(path.clone(), "# One\nbody\n## Child\ntext\n");
            let page = app
                .world_mut()
                .spawn((
                    FileView { path },
                    editor,
                    OutlineDirty,
                    vmux_ecs::page::PageReady {},
                ))
                .id();
            app.world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(page);
            Self { app, page }
        }

        fn editor(path: std::path::PathBuf, text: &str) -> Editor {
            let core = EditCore::new(
                path.clone(),
                "Markdown".to_string(),
                text,
                vmux_api::editor::EditMode::Normal,
            );
            Editor::new(core, HighlightCache::plain(&path), Default::default())
        }

        fn metadata(&mut self, revision: u64) {
            let path = self
                .app
                .world()
                .get::<FileView>(self.page)
                .unwrap()
                .path
                .to_string_lossy()
                .into_owned();
            self.app
                .world_mut()
                .trigger(vmux_ecs::FileUiStateWrite::from_event(
                    self.page,
                    &FileMetaEvent {
                        revision,
                        path: path.clone(),
                        abs_path: path,
                        kind: FileDocumentKind::Markdown,
                        language: "Markdown".to_string(),
                        total_lines: 4,
                        indent: Default::default(),
                        line_ending: Default::default(),
                        encoding: Default::default(),
                    },
                ));
            self.app
                .world_mut()
                .entity_mut(self.page)
                .insert(FileInitialMetaSent);
        }

        fn names(&self) -> Vec<String> {
            let mut names = Vec::new();
            if let Some(outline) = &self.app.world().resource::<Published>().0.explorer.outline {
                for item in &outline.items {
                    names.push(item.name.clone());
                }
            }
            names
        }
    }

    #[test]
    fn markdown_outline_survives_document_metadata_arriving_after_page_readiness() {
        let mut fixture = OutlinePage::new();
        fixture.app.update();
        fixture.metadata(1);
        fixture.app.update();
        assert_eq!(fixture.names(), ["One", "Child"]);
    }

    #[test]
    fn markdown_outline_survives_file_change_and_metadata_in_the_same_frame() {
        let mut fixture = OutlinePage::new();
        fixture.metadata(1);
        fixture.app.update();
        assert_eq!(fixture.names(), ["One", "Child"]);
        let path = std::path::PathBuf::from("/repo/two.md");
        let editor = OutlinePage::editor(path.clone(), "# Two\nbody\n");
        fixture.app.world_mut().entity_mut(fixture.page).insert((
            FileView { path },
            editor,
            OutlineDirty,
        ));
        fixture.metadata(2);
        fixture.app.update();
        assert_eq!(fixture.names(), ["Two"]);
    }

    #[test]
    fn markdown_outline_tracks_heading_ranges() {
        let markdown = "# One\na\n## Sub\nb\n## Sub2\nc\n# Two\nd\n";
        let rows = OutlineRows::from_markdown(markdown).into_vec();
        let actual = rows
            .into_iter()
            .map(|row| (row.name, row.line, row.end_line))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("One".to_string(), 0, 5),
                ("Sub".to_string(), 2, 3),
                ("Sub2".to_string(), 4, 5),
                ("Two".to_string(), 6, OutlineRow::OPEN_END),
            ]
        );
    }

    #[test]
    fn markdown_outline_ignores_fenced_headings() {
        let rows = OutlineRows::from_markdown("# Real\n```\n# Fake\n```\n## After\n").into_vec();
        let names = rows.into_iter().map(|row| row.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["Real".to_string(), "After".to_string()]);
    }

    #[test]
    fn lsp_outline_flattens_symbol_children() {
        let value = serde_json::json!([
            {
                "name": "Foo",
                "kind": 5,
                "range": { "start": { "line": 2 }, "end": { "line": 9 } },
                "selectionRange": { "start": { "line": 2 } },
                "children": [
                    { "name": "bar", "kind": 6, "selectionRange": { "start": { "line": 4 } } }
                ]
            }
        ]);
        let rows = OutlineRows::from_lsp(&value).into_vec();
        let actual = rows
            .iter()
            .map(|row| {
                (
                    row.name.as_str(),
                    row.kind,
                    row.line,
                    row.end_line,
                    row.depth,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![("Foo", 5, 2, 9, 0), ("bar", 6, 4, OutlineRow::OPEN_END, 1)]
        );
    }

    #[test]
    fn invalid_lsp_symbol_range_stays_open() {
        let value = serde_json::json!([
            {
                "name": "broken",
                "kind": 12,
                "range": { "start": { "line": 3 }, "end": { "line": 1 } },
                "selectionRange": { "start": { "line": 3 } }
            }
        ]);
        let rows = OutlineRows::from_lsp(&value).into_vec();
        assert_eq!(rows[0].line, 3);
        assert_eq!(rows[0].end_line, OutlineRow::OPEN_END);
    }
}
