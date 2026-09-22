use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_flex::prelude::*;
use vmux_layout::Browser;

use super::explorer::ExplorerState;
use super::plugin::{FileView, PendingGoto};
use super::viewport::FileViewport;

pub(super) struct EditorPageOpenPlugin;

impl Plugin for EditorPageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_core::event::RecordVisitRequest>()
            .add_systems(
                Update,
                handle_file_page_open.in_set(PageOpenSet::HandleKnownPages),
            );
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type NavigableFileView = (
    &'static mut FileView,
    &'static mut FileViewport,
    &'static mut PageMetadata,
);

fn new_file_view_bundle(url: &str, path: PathBuf) -> impl Bundle {
    let title = if url.starts_with("vmux://") {
        url.to_string()
    } else {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    };
    (
        (
            FileView { path },
            FileViewport {
                top_row: 0,
                rows: 0,
                wrap_columns: 0,
                word_wrap: vmux_core::editor::WordWrap::default(),
                word_wrap_column: 80,
            },
            ExplorerState::default(),
            Browser,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            PageMetadata {
                title,
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            vmux_core::host::page::HostsPage,
            vmux_core::host::page::BindsEditingChords,
            vmux_core::host::page::HostHistory::default(),
        ),
        (
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        ),
    )
}

pub fn restore_file_view_bundle(url: &str) -> Option<impl Bundle> {
    let path = vmux_core::file_url::FileUrl::parse(url)?.path()?;
    Some(new_file_view_bundle(url, path))
}

fn handle_file_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children: Query<&Children>,
    mut views: Query<NavigableFileView>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    effective_startup_dir: Option<Res<vmux_layout::settings::EffectiveStartupDir>>,
    mut commands: Commands,
    mut record_writer: MessageWriter<vmux_core::event::RecordVisitRequest>,
) {
    for (entity, task) in &tasks {
        let project_dir = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.as_ref())
            .and_then(|(_, path)| path.as_deref());
        let knowledge_root = vmux_core::knowledge::KnowledgeVault::user().into_root();
        let Some(target) = FilePageTarget::resolve(&task.url, project_dir, &knowledge_root) else {
            continue;
        };
        let Some(path) = target.path else {
            commands.entity(entity).insert(PageOpenError {
                message: target.error,
            });
            continue;
        };
        let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();
        let page_url = if clean_url.trim_end_matches('/')
            == vmux_core::knowledge::KNOWLEDGE_PAGE_URL.trim_end_matches('/')
        {
            FileView { path: path.clone() }.url()
        } else {
            clean_url.clone()
        };
        if !path.is_dir() {
            let title = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            record_writer.write(vmux_core::event::RecordVisitRequest {
                url: clean_url.clone(),
                title,
            });
        }
        let pending = PendingGoto::from_url(&task.url);
        let view = match FileView::in_stack(task.stack, &children, &views) {
            Some(view) => {
                if let Ok((mut file_view, mut viewport, mut metadata)) = views.get_mut(view)
                    && file_view.path != path
                {
                    file_view.navigate(
                        view,
                        path,
                        0,
                        &mut viewport,
                        &mut metadata,
                        &mut manager,
                        &mut commands,
                    );
                }
                if let Ok((_, _, mut metadata)) = views.get_mut(view)
                    && page_url.starts_with("vmux://")
                {
                    metadata.title.clone_from(&page_url);
                    metadata.url = page_url.clone();
                    metadata.icon = vmux_core::PageIcon::None;
                }
                view
            }
            None => {
                vmux_layout::stack::Stack::clear_children(task.stack, &children, &mut commands);
                commands
                    .spawn((new_file_view_bundle(&page_url, path), ChildOf(task.stack)))
                    .id()
            }
        };
        if let Some(pending) = pending {
            commands.entity(view).insert(pending);
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

struct FilePageTarget {
    path: Option<PathBuf>,
    error: String,
}

impl FilePageTarget {
    fn resolve(url: &str, project_dir: Option<&Path>, knowledge_root: &Path) -> Option<Self> {
        if url.trim_end_matches('/') == vmux_api::space::PROJECTS_PAGE_URL.trim_end_matches('/') {
            return Some(Self {
                path: Some(
                    project_dir
                        .map(Path::to_path_buf)
                        .unwrap_or_else(vmux_core::profile::projects_dir),
                ),
                error: String::new(),
            });
        }
        if url.trim_end_matches('/')
            == vmux_core::knowledge::KNOWLEDGE_PAGE_URL.trim_end_matches('/')
        {
            return Some(Self {
                path: Some(knowledge_root.to_path_buf()),
                error: String::new(),
            });
        }
        if !url.starts_with("file:") {
            return None;
        }
        Some(Self {
            path: vmux_core::file_url::FileUrl::parse(url).and_then(|file| file.path()),
            error: format!("malformed file URL '{url}'"),
        })
    }
}
