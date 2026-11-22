use std::path::Path;

use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy_cef::prelude::UiInput;
use vmux_api::address::{
    AddressCrumb, AddressIntent, AddressOption, PageAddress, PageAddressRequest,
    PageAddressUiState, PageAddressUiStatePatch,
};
use vmux_ecs::event::*;
use vmux_ecs::page::{AddressContribution, AddressIntentRequest};
use vmux_ecs::{FileUiStateWrite, PageMetadata, UiStateWrite};

use super::editor::{FileDocumentRevision, FileView};

pub(super) struct AddressPlugin;

impl Plugin for AddressPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::address::AddressPlugin>() {
            app.add_plugins(vmux_ecs::address::AddressPlugin);
        }
        app.add_observer(capture)
            .add_observer(capture_projects)
            .add_observer(request)
            .add_observer(forward)
            .add_systems(
                PostUpdate,
                publish.before(vmux_layout::projection::LayoutUiProjectionSet::SourceFlush),
            );
    }
}

#[derive(Component, Default)]
pub(super) struct AddressSource {
    repo_root: String,
    branch: String,
    directory_abs_path: Option<String>,
    document_kind: Option<FileDocumentKind>,
    view_mode: Option<FileViewMode>,
    checkout: Option<(String, String, String, bool)>,
    breadcrumb_path: String,
    breadcrumb_pending: bool,
    breadcrumb_entries: Vec<(String, String, bool)>,
    outline: Vec<OutlineRow>,
    search_active: bool,
    project_roots: Vec<String>,
    line: u32,
}

fn capture(
    event: On<FileUiStateWrite>,
    mut sources: Query<&mut AddressSource>,
    mut commands: Commands,
) {
    let patch = event.update();
    if patch.meta.is_none()
        && patch.directory.is_none()
        && patch.breadcrumb.is_none()
        && patch.git_state.is_none()
        && patch.checkout.is_none()
        && patch.outline.is_none()
        && patch.cursor.is_none()
        && patch.viewport.is_none()
        && patch.explorer_panel.is_none()
        && patch.view_mode.is_none()
    {
        return;
    }
    let entity = event.webview();
    if let Ok(mut source) = sources.get_mut(entity) {
        source.apply(patch);
    } else {
        let mut source = AddressSource::default();
        source.apply(patch);
        commands.entity(entity).insert(source);
    }
}

fn capture_projects(
    event: On<UiStateWrite<ProjectNavigation>>,
    mut sources: Query<&mut AddressSource>,
    mut commands: Commands,
) {
    let roots = event
        .update()
        .rows
        .iter()
        .filter(|row| row.depth == 0 && !row.missing)
        .map(|row| row.path.clone())
        .collect::<Vec<_>>();
    if let Ok(mut source) = sources.get_mut(event.webview()) {
        source.project_roots = roots;
    } else {
        commands.entity(event.webview()).insert(AddressSource {
            project_roots: roots,
            ..Default::default()
        });
    }
}

impl AddressSource {
    fn apply(&mut self, patch: &FileUiStatePatch) {
        if let Some(meta) = &patch.meta {
            self.document_kind = Some(meta.kind);
            self.directory_abs_path = None;
        }
        if let Some(view_mode) = &patch.view_mode {
            self.view_mode = Some(view_mode.mode);
        }
        if let Some(directory) = &patch.directory {
            self.directory_abs_path = Some(directory.abs_path.clone());
        }
        if let Some(breadcrumb) = &patch.breadcrumb {
            self.breadcrumb_path.clone_from(&breadcrumb.path);
            self.breadcrumb_pending = breadcrumb.pending;
            self.breadcrumb_entries = breadcrumb
                .entries
                .iter()
                .map(|entry| (entry.name.clone(), entry.path.clone(), entry.is_dir))
                .collect();
        }
        if let Some(git) = &patch.git_state {
            self.repo_root.clone_from(&git.repo_root);
            self.branch.clone_from(&git.branch);
        }
        if let Some(checkout) = &patch.checkout {
            self.checkout = Some((
                checkout.path.clone(),
                checkout.repository.clone(),
                checkout.checkout.clone(),
                checkout.read_only,
            ));
        }
        if let Some(outline) = &patch.outline {
            self.outline.clone_from(&outline.items);
        }
        if let Some(panel) = &patch.explorer_panel {
            self.search_active = panel.search;
        }
        if let Some(viewport) = &patch.viewport
            && let Some(line) = viewport.lines.first()
        {
            self.line = line.line_no;
        }
        if let Some(cursor) = &patch.cursor {
            self.line = cursor.source_primary.line;
        }
    }

    fn address(
        &self,
        owner: Entity,
        view: &FileView,
        revision: &FileDocumentRevision,
        metadata: &PageMetadata,
        workspace: Option<&vmux_ecs::WorkspaceKind>,
    ) -> PageAddress {
        let absolute = view.path.to_string_lossy().into_owned();
        let root = self.project_root(view);
        let is_dir = self.directory_abs_path.as_deref() == Some(absolute.as_str());
        let mut result = self.base_address(owner, view, revision, metadata, workspace, &absolute);
        self.append_path_crumbs(&mut result, view, &root, is_dir);
        self.append_symbol_crumbs(&mut result, is_dir);
        result.search_active = self.search_active;
        result.search_available = true;
        result
    }

    fn project_root(&self, view: &FileView) -> String {
        let mut best: Option<(&str, usize)> = None;
        for path in &self.project_roots {
            if !view.path.starts_with(path) {
                continue;
            }
            let count = Path::new(path).components().count();
            if best.is_none_or(|(_, prev)| count >= prev) {
                best = Some((path.as_str(), count));
            }
        }
        best.map(|(path, _)| path.to_string())
            .unwrap_or_else(|| self.repo_root.clone())
    }

    fn base_address(
        &self,
        owner: Entity,
        view: &FileView,
        revision: &FileDocumentRevision,
        metadata: &PageMetadata,
        workspace: Option<&vmux_ecs::WorkspaceKind>,
        absolute: &str,
    ) -> PageAddress {
        let knowledge = workspace == Some(&vmux_ecs::WorkspaceKind::Knowledge)
            || metadata.url.starts_with("vmux://knowledge");
        let mut result = PageAddress {
            owner: owner.to_bits(),
            revision: revision.get(),
            url: metadata.url.clone(),
            current: AddressOption {
                label_key: if knowledge {
                    "layout-knowledge"
                } else {
                    "editor-editor"
                }
                .to_string(),
                icon: Some(if knowledge {
                    vmux_api::BuiltinIcon::Brain
                } else {
                    vmux_api::BuiltinIcon::Code
                }),
                intent: Some(AddressIntent::Navigate {
                    url: if knowledge {
                        "vmux://knowledge/".to_string()
                    } else {
                        vmux_path::FileUrl::from_path(&view.path, None, None, None)
                    },
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        result.contextual.push(AddressOption {
            label_key: "editor-editor".to_string(),
            icon: Some(vmux_api::BuiltinIcon::Code),
            intent: Some(AddressIntent::Navigate {
                url: vmux_path::FileUrl::from_path(&view.path, None, None, None),
            }),
            ..Default::default()
        });
        if let Some(url) = vmux_git::GitUrl::from_path(Path::new(&self.repo_root)) {
            result.contextual.push(AddressOption {
                label_key: "git-title".to_string(),
                icon: Some(vmux_api::BuiltinIcon::GitBranch),
                intent: Some(AddressIntent::Navigate { url }),
                ..Default::default()
            });
        }
        if let Some((path, repository, checkout, read_only)) = &self.checkout
            && *read_only
            && path == absolute
        {
            result.checkout = Some(AddressOption {
                label_key: "git-checkout".to_string(),
                icon: Some(vmux_api::BuiltinIcon::GitBranch),
                intent: Some(AddressIntent::Checkout {
                    repository: repository.clone(),
                    branch: self.branch.clone(),
                    checkout: checkout.clone(),
                }),
                ..Default::default()
            });
        }
        result
    }

    fn append_path_crumbs(
        &self,
        result: &mut PageAddress,
        view: &FileView,
        root: &str,
        is_dir: bool,
    ) {
        let (relative, mut path) = match view.path.strip_prefix(root) {
            Ok(relative) if !root.is_empty() => (relative, std::path::PathBuf::from(root)),
            _ => (view.path.as_path(), std::path::PathBuf::from("/")),
        };
        let components = relative
            .components()
            .filter(|part| matches!(part, std::path::Component::Normal(_)))
            .collect::<Vec<_>>();
        for (index, component) in components.iter().enumerate() {
            let parent = path.to_string_lossy().into_owned();
            path.push(component.as_os_str());
            let target = path.to_string_lossy().into_owned();
            let mut options = Vec::new();
            let pending = self.breadcrumb_path != parent || self.breadcrumb_pending;
            if self.breadcrumb_path == parent {
                for (name, entry_path, entry_is_dir) in &self.breadcrumb_entries {
                    options.push(AddressOption::file(
                        name.clone(),
                        entry_path.clone(),
                        *entry_is_dir,
                    ));
                }
            }
            result.paths.push(AddressCrumb {
                entry: AddressOption::file(
                    component.as_os_str().to_string_lossy().into_owned(),
                    target,
                    is_dir || index + 1 < components.len(),
                ),
                options,
                on_open: Some(AddressIntent::Siblings { path: parent }),
                pending,
                active: index + 1 == components.len(),
                ..Default::default()
            });
        }
    }

    fn append_symbol_crumbs(&self, result: &mut PageAddress, is_dir: bool) {
        let reading_note = self.document_kind == Some(FileDocumentKind::Markdown)
            && self.view_mode.is_none_or(|mode| mode == FileViewMode::Note);
        let rows = if reading_note || is_dir {
            &[][..]
        } else {
            self.outline.as_slice()
        };
        let mut chain: Vec<usize> = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if !row.contains(self.line) {
                continue;
            }
            while let Some(&open) = chain.last() {
                if rows[open].depth < row.depth {
                    break;
                }
                chain.pop();
            }
            chain.push(index);
        }
        for (level, &index) in chain.iter().enumerate() {
            let row = &rows[index];
            let start = if level == 0 { 0 } else { chain[level - 1] + 1 };
            let mut options = Vec::new();
            for sibling in &rows[start..] {
                if level > 0 && sibling.depth <= rows[chain[level - 1]].depth {
                    break;
                }
                if sibling.depth == row.depth {
                    options.push(Self::symbol_option(sibling));
                }
            }
            result.symbols.push(AddressCrumb {
                entry: Self::symbol_option(row),
                options,
                active: level + 1 == chain.len(),
                ..Default::default()
            });
        }
    }

    fn symbol_option(row: &OutlineRow) -> AddressOption {
        AddressOption {
            label: row.name.clone(),
            icon: Some(vmux_api::BuiltinIcon::Code),
            intent: Some(AddressIntent::Goto { line: row.line }),
            ..Default::default()
        }
    }
}

type Documents<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static AddressSource,
        &'static FileView,
        &'static FileDocumentRevision,
        &'static PageMetadata,
        Option<&'static AddressContribution>,
        Option<&'static vmux_ecs::WorkspaceKind>,
    ),
>;

fn publish(sources: Documents, mut commands: Commands) {
    for (entity, source, view, revision, metadata, current, workspace) in &sources {
        let next = source.address(entity, view, revision, metadata, workspace);
        if current.is_some_and(|current| current.0 == next) {
            continue;
        }
        commands
            .entity(entity)
            .insert(AddressContribution(next.clone()));
        commands.trigger(UiStateWrite::<PageAddressUiState>::from_event(
            entity,
            &PageAddressUiStatePatch {
                address: Some(next),
                ..Default::default()
            },
        ));
    }
}

fn request(
    event: On<UiInput<PageAddressRequest>>,
    addresses: Query<&AddressContribution>,
    views: Query<(&FileView, &FileDocumentRevision)>,
    mut commands: Commands,
) {
    let request = &event.payload;
    let owner = event.webview;
    let Ok(address) = addresses.get(owner) else {
        return;
    };
    let Ok((view, revision)) = views.get(owner) else {
        return;
    };
    if !address.0.accepts(request)
        || revision.get() != request.revision
        || address
            .0
            .paths
            .last()
            .is_some_and(|crumb| view.path != Path::new(&crumb.entry.path))
    {
        return;
    }
    commands.trigger(AddressIntentRequest {
        entity: owner,
        revision: request.revision,
        url: request.url.clone(),
        intent: request.intent.clone(),
    });
}

fn forward(
    event: On<AddressIntentRequest>,
    sources: Query<&AddressSource>,
    documents: Query<(&FileDocumentRevision, &PageMetadata)>,
    mut commands: Commands,
) {
    let webview = event.entity;
    let Ok((revision, metadata)) = documents.get(webview) else {
        return;
    };
    if revision.get() != event.revision || metadata.url != event.url {
        return;
    }
    match &event.intent {
        AddressIntent::Navigate { .. } => {}
        AddressIntent::File { path } => {
            commands.trigger(UiInput {
                webview,
                payload: FileOpenEvent { path: path.clone() },
            });
        }
        AddressIntent::Preview {
            path,
            repository,
            checkout,
        } => {
            commands.trigger(UiInput {
                webview,
                payload: FileCheckoutPreviewRequest {
                    path: path.clone(),
                    repository: repository.clone(),
                    checkout: checkout.clone(),
                },
            });
        }
        AddressIntent::Branches => {
            commands.trigger(UiInput {
                webview,
                payload: vmux_git::event::FileGitBranchesRequest,
            });
        }
        AddressIntent::Siblings { path } => {
            commands.trigger(UiInput {
                webview,
                payload: FileBreadcrumbRequest { path: path.clone() },
            });
        }
        AddressIntent::Goto { line } => {
            commands.trigger(UiInput {
                webview,
                payload: ExplorerGoto {
                    path: String::new(),
                    line: *line,
                },
            });
        }
        AddressIntent::Search => {
            commands.trigger(UiInput {
                webview,
                payload: ExplorerPanelSetVisible { visible: true },
            });
            commands.trigger(UiInput {
                webview,
                payload: ExplorerPanelViewSet {
                    search: !sources
                        .get(webview)
                        .is_ok_and(|source| source.search_active),
                },
            });
        }
        AddressIntent::Checkout {
            repository,
            branch,
            checkout,
        } => {
            commands.trigger(UiInput {
                webview,
                payload: vmux_ecs::event::space::ProjectActivateRequest {
                    path: repository.clone(),
                    branch: branch.clone(),
                    checkout: checkout.clone(),
                    pane_id: None,
                },
            });
        }
        AddressIntent::Session { .. }
        | AddressIntent::SessionLocation { .. }
        | AddressIntent::Section { .. }
        | AddressIntent::GitBranch { .. }
        | AddressIntent::GitCommit { .. }
        | AddressIntent::GitCatalogNext { .. }
        | AddressIntent::TerminalDirectory { .. }
        | AddressIntent::TerminalDirectories { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_requests_reject_other_pages_and_stale_document_selections() {
        let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(Entity, String)>::new()));
        let recorded = opened.clone();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AddressPlugin))
            .add_observer(move |event: On<UiInput<FileOpenEvent>>| {
                recorded
                    .lock()
                    .unwrap()
                    .push((event.webview, event.payload.path.clone()));
            });
        let page = app
            .world_mut()
            .spawn((
                FileView {
                    path: "/repo/source/first.rs".into(),
                },
                PageMetadata {
                    url: "file:///repo/source/first.rs".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        let other = app
            .world_mut()
            .spawn((
                FileView {
                    path: "/repo/source/second.rs".into(),
                },
                PageMetadata {
                    url: "file:///repo/source/second.rs".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        app.update();
        let address = app
            .world()
            .get::<AddressContribution>(page)
            .unwrap()
            .0
            .clone();
        let request = PageAddressRequest {
            owner: address.owner,
            revision: address.revision,
            url: address.url,
            intent: AddressIntent::File {
                path: "/repo/source/selected.rs".to_string(),
            },
        };
        app.world_mut().trigger(UiInput {
            webview: other,
            payload: request.clone(),
        });
        let mut stale = request.clone();
        stale.revision += 1;
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: stale,
        });
        assert!(opened.lock().unwrap().is_empty());
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request.clone(),
        });
        app.world_mut().flush();
        assert_eq!(
            opened.lock().unwrap().as_slice(),
            [(page, "/repo/source/selected.rs".to_string())]
        );
        app.world_mut().get_mut::<FileView>(page).unwrap().path = "/repo/source/replaced.rs".into();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request,
        });
        assert_eq!(opened.lock().unwrap().len(), 1);
    }

    #[test]
    fn file_breadcrumbs_stay_relative_without_workspace_context() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AddressPlugin));
        let page = app
            .world_mut()
            .spawn((
                FileView {
                    path: "/repo/source/very-long-file-name.rs".into(),
                },
                PageMetadata {
                    url: "file:///repo/source/very-long-file-name.rs".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().trigger(FileUiStateWrite::from_event(
            page,
            &vmux_api::git::FileGitState {
                repo_root: "/repo".to_string(),
                branch: "main".to_string(),
                branches: vec![vmux_api::git::FileGitBranch {
                    name: "feature".to_string(),
                    checkout: "/worktrees/feature".to_string(),
                }],
                ..Default::default()
            },
        ));
        app.update();
        let address = &app.world().get::<AddressContribution>(page).unwrap().0;
        assert_eq!(
            address
                .paths
                .iter()
                .map(|crumb| (crumb.entry.label.as_str(), crumb.entry.path.as_str()))
                .collect::<Vec<_>>(),
            [
                ("source", "/repo/source"),
                (
                    "very-long-file-name.rs",
                    "/repo/source/very-long-file-name.rs"
                )
            ]
        );
        assert!(address.fixed.is_empty());
    }
}
