use std::path::Path;

use bevy::prelude::*;
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
    state: FileUiState,
    projects: ProjectNavigation,
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
    if let Ok(mut source) = sources.get_mut(event.webview()) {
        source.projects = event.update().clone();
    } else {
        commands.entity(event.webview()).insert(AddressSource {
            projects: event.update().clone(),
            ..Default::default()
        });
    }
}

impl AddressSource {
    fn apply(&mut self, patch: &FileUiStatePatch) {
        let update = FileUiStatePatch {
            meta: patch.meta.clone(),
            view_mode: patch.view_mode,
            directory: patch.directory.clone(),
            breadcrumb: patch.breadcrumb.clone(),
            git_state: patch.git_state.clone(),
            checkout: patch.checkout.clone(),
            outline: patch.outline.clone(),
            explorer_panel: patch.explorer_panel,
            ..Default::default()
        };
        if let Some(viewport) = &patch.viewport
            && let Some(line) = viewport.lines.first()
        {
            self.line = line.line_no;
        }
        if let Some(cursor) = &patch.cursor {
            self.line = cursor.source_primary.line;
        }
        vmux_api::UiStateProjection::apply(&mut self.state, update);
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
        let git = self.state.git.git_state.clone().unwrap_or_default();
        let project = self
            .projects
            .rows
            .iter()
            .filter(|row| row.depth == 0 && !row.missing && view.path.starts_with(&row.path))
            .max_by_key(|row| Path::new(&row.path).components().count());
        let root = project
            .map(|row| row.path.clone())
            .unwrap_or_else(|| git.repo_root.clone());
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
        if let Some(url) = vmux_git::GitUrl::from_path(Path::new(&git.repo_root)) {
            result.contextual.push(AddressOption {
                label_key: "git-title".to_string(),
                icon: Some(vmux_api::BuiltinIcon::GitBranch),
                intent: Some(AddressIntent::Navigate { url }),
                ..Default::default()
            });
        }
        if !root.is_empty() {
            let label = project.map(|row| row.label.clone()).unwrap_or_else(|| {
                Path::new(&root)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            let entry = AddressOption::file(label, root.clone(), true).identity(root.clone());
            let mut options = Vec::new();
            for row in &self.projects.rows {
                if row.depth == 0 && !row.missing {
                    options.push(
                        AddressOption::file(row.label.clone(), row.path.clone(), true)
                            .identity(row.path.clone()),
                    );
                }
            }
            if !options.iter().any(|option| option.path == root) {
                options.insert(0, entry.clone());
            }
            result.fixed.push(AddressCrumb {
                entry,
                options,
                ..Default::default()
            });
        }
        if !git.branch.is_empty() {
            let entry = AddressOption::file(git.branch.clone(), absolute.clone(), false)
                .identity(git.branch.clone());
            let mut options = Vec::new();
            if let Ok(relative) = view.path.strip_prefix(&git.repo_root) {
                for branch in &git.branches {
                    if branch.checkout.is_empty() {
                        continue;
                    }
                    let path = Path::new(&branch.checkout)
                        .join(relative)
                        .to_string_lossy()
                        .into_owned();
                    let mut option = AddressOption::file(branch.name.clone(), path.clone(), false)
                        .identity(branch.name.clone());
                    option.intent = Some(AddressIntent::Preview {
                        path,
                        repository: root.clone(),
                        checkout: branch.checkout.clone(),
                    });
                    options.push(option);
                }
            }
            if !options.iter().any(|option| option.label == git.branch) {
                options.insert(0, entry.clone());
            }
            result.fixed.push(AddressCrumb {
                entry,
                options,
                on_open: Some(AddressIntent::Branches),
                pending: git.branches_loading,
                ..Default::default()
            });
        }
        if let Some(checkout) = self
            .state
            .document
            .checkout
            .as_ref()
            .filter(|state| state.read_only && state.path == absolute)
        {
            result.checkout = Some(AddressOption {
                label_key: "git-checkout".to_string(),
                icon: Some(vmux_api::BuiltinIcon::GitBranch),
                intent: Some(AddressIntent::Checkout {
                    repository: checkout.repository.clone(),
                    branch: git.branch,
                    checkout: checkout.checkout.clone(),
                }),
                ..Default::default()
            });
        }
        let is_dir = self
            .state
            .document
            .directory
            .as_ref()
            .is_some_and(|directory| directory.abs_path == absolute);
        let (relative, mut path) = match view.path.strip_prefix(&root) {
            Ok(relative) if !root.is_empty() => (relative, std::path::PathBuf::from(&root)),
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
            let siblings = &self.state.document.breadcrumb;
            let pending = siblings.path != parent || siblings.pending;
            if siblings.path == parent {
                for entry in &siblings.entries {
                    options.push(AddressOption::file(
                        entry.name.clone(),
                        entry.path.clone(),
                        entry.is_dir,
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
        let reading_note = self
            .state
            .document
            .meta
            .as_ref()
            .is_some_and(|meta| meta.kind == FileDocumentKind::Markdown)
            && self
                .state
                .document
                .view_mode
                .as_ref()
                .is_none_or(|view| view.mode == FileViewMode::Note);
        let rows = self
            .state
            .explorer
            .outline
            .as_ref()
            .map(|outline| outline.items.as_slice())
            .unwrap_or_default();
        let mut chain: Vec<usize> = Vec::new();
        let rows = if reading_note || is_dir { &[] } else { rows };
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
                    options.push(AddressSymbol(sibling).option());
                }
            }
            result.symbols.push(AddressCrumb {
                entry: AddressSymbol(row).option(),
                options,
                active: level + 1 == chain.len(),
                ..Default::default()
            });
        }
        result.search_active = self
            .state
            .explorer
            .explorer_panel
            .as_ref()
            .is_some_and(|panel| panel.search);
        result
    }
}

struct AddressSymbol<'a>(&'a OutlineRow);

impl AddressSymbol<'_> {
    fn option(self) -> AddressOption {
        AddressOption {
            label: self.0.name.clone(),
            icon: Some(vmux_api::BuiltinIcon::Code),
            intent: Some(AddressIntent::Goto { line: self.0.line }),
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
                    search: !sources.get(webview).is_ok_and(|source| {
                        source
                            .state
                            .explorer
                            .explorer_panel
                            .as_ref()
                            .is_some_and(|panel| panel.search)
                    }),
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
        | AddressIntent::GitCatalogNext { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Opened(Vec<(Entity, String)>);

    fn record(event: On<UiInput<FileOpenEvent>>, mut opened: ResMut<Opened>) {
        opened.0.push((event.webview, event.payload.path.clone()));
    }

    #[test]
    fn address_requests_reject_other_pages_and_stale_document_selections() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AddressPlugin))
            .init_resource::<Opened>()
            .add_observer(record);
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
        assert!(app.world().resource::<Opened>().0.is_empty());
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request.clone(),
        });
        app.world_mut().flush();
        assert_eq!(
            app.world().resource::<Opened>().0.as_slice(),
            [(page, "/repo/source/selected.rs".to_string())]
        );
        app.world_mut().get_mut::<FileView>(page).unwrap().path = "/repo/source/replaced.rs".into();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request,
        });
        assert_eq!(app.world().resource::<Opened>().0.len(), 1);
    }

    #[test]
    fn project_relative_paths_and_branch_previews_keep_their_checkout_targets() {
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
        let branch = address.fixed.last().unwrap();
        let preview = branch
            .options
            .iter()
            .find(|option| option.label == "feature")
            .unwrap();
        assert_eq!(
            preview.intent,
            Some(AddressIntent::Preview {
                path: "/worktrees/feature/source/very-long-file-name.rs".to_string(),
                repository: "/repo".to_string(),
                checkout: "/worktrees/feature".to_string()
            })
        );
    }
}
