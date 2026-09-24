use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive, Browsers, UiEventPlugin};
use vmux_core::event::{
    InstallPhase, LspCatalogEvent, LspCatalogRequest, LspInstallProgress, LspInstallRequest,
    LspManagerStateEvent, LspPackage, LspPkgStatus, LspPkgStatusEvent, LspUninstallRequest,
    LspUpdateRequest,
};
use vmux_core::host::page::PageReady;
use vmux_layout::native_open::HostedPage;

use crate::lsp::catalog::{self, Package};
use crate::lsp::{install, purl, store, target};

pub struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.init_resource::<ManagerOutbox>()
            .init_resource::<ActiveInstalls>()
            .add_plugins(vmux_layout::native_open::HostedPagePlugin::<LspManagerPage>::default())
            .add_plugins(UiEventPlugin::<(
                LspCatalogRequest,
                LspInstallRequest,
                LspUninstallRequest,
                LspUpdateRequest,
            )>::default())
            .add_observer(on_catalog_request)
            .add_observer(on_install_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_update_request)
            .add_observer(reset_state_on_page_ready)
            .add_systems(
                Update,
                (
                    drain_manager_outbox,
                    publish_manager_state.after(drain_manager_outbox),
                ),
            );
    }
}

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "lsp",
    title: "Language Servers",
    title_message_id: Some("lsp-title"),
    replaces_command: None,
    keywords: &["lsp", "language", "server", "install", "mason"],
    icon: Some(vmux_core::BuiltinIcon::Server),
    command_bar: true,
};

#[derive(Component, Default)]
#[require(ManagerState)]
struct LspManagerPage;

impl HostedPage for LspManagerPage {
    const HOST: &'static str = "lsp";
    const URL: &'static str = "vmux://lsp/";
    const TITLE: &'static str = "Language Servers";
}

#[derive(Component)]
struct ManagerState {
    packages: Vec<LspPackage>,
    progress: Vec<LspInstallProgress>,
    loading: bool,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            progress: Vec::new(),
            loading: true,
        }
    }
}

impl ManagerState {
    fn start_loading(&mut self) {
        self.loading = true;
    }

    fn apply_catalog(&mut self, event: LspCatalogEvent) {
        self.packages = event.packages;
        self.loading = false;
    }

    fn apply_progress(&mut self, event: LspInstallProgress) {
        let name = event.name.clone();
        let phase = event.phase;
        match self.progress.iter_mut().find(|item| item.name == name) {
            Some(current) => *current = event,
            None => self.progress.push(event),
        }
        let Some(package) = self
            .packages
            .iter_mut()
            .find(|package| package.name == name)
        else {
            return;
        };
        package.status = match phase {
            InstallPhase::Failed => LspPkgStatus::Failed,
            InstallPhase::Done => LspPkgStatus::Installed,
            _ => LspPkgStatus::Installing,
        };
    }

    fn apply_status(&mut self, event: LspPkgStatusEvent) {
        let name = event.name;
        if let Some(package) = self
            .packages
            .iter_mut()
            .find(|package| package.name == name)
        {
            package.status = event.status;
            package.version = event.version;
        }
        self.progress.retain(|item| item.name != name);
    }

    fn event(&self) -> LspManagerStateEvent {
        LspManagerStateEvent {
            packages: self.packages.clone(),
            progress: self.progress.clone(),
            loading: self.loading,
        }
    }
}

#[derive(Component)]
struct ManagerStateSent;

pub enum ManagerMsg {
    Catalog(LspCatalogEvent),
    Progress(LspInstallProgress),
    Status(LspPkgStatusEvent),
}

#[derive(Resource, Clone, Default)]
pub struct ManagerOutbox(pub Arc<Mutex<Vec<(Entity, ManagerMsg)>>>);

#[derive(Resource, Clone, Default)]
struct ActiveInstalls(Arc<Mutex<HashSet<String>>>);

pub fn to_lsp_package(root: &Path, p: &Package) -> LspPackage {
    let kind = purl::parse(&p.source_id)
        .map(|x| x.kind)
        .unwrap_or_default();
    let installed = store::is_installed(root, &p.name);
    let on_path = !installed
        && matches!(
            store::resolved_command(root, p.name.as_str()),
            store::Resolution::OnPath
        );
    let catalog_version = purl::parse(&p.source_id).and_then(|x| x.version);
    let installed_version = installed
        .then(|| store::read_receipt(root, &p.name).and_then(|r| r.version))
        .flatten();
    let outdated = installed
        && installed_version.is_some()
        && catalog_version.is_some()
        && installed_version != catalog_version;
    let status = if outdated {
        LspPkgStatus::Outdated
    } else if installed {
        LspPkgStatus::Installed
    } else if on_path {
        LspPkgStatus::OnPath
    } else {
        LspPkgStatus::Available
    };
    let installable = kind == "github"
        || install::toolchain_for(&kind).is_some_and(crate::lsp::registry::executable_on_path);
    let requires = if installable {
        None
    } else {
        install::toolchain_for(&kind).map(String::from)
    };
    let version = if installed {
        installed_version
    } else {
        catalog_version
    };
    LspPackage {
        name: p.name.as_str().to_string(),
        description: p.description.clone(),
        languages: p.languages.clone(),
        categories: p.categories.clone(),
        status,
        version,
        installable,
        requires,
    }
}

fn push(outbox: &ManagerOutbox, entity: Entity, msg: ManagerMsg) {
    outbox
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((entity, msg));
}

fn on_catalog_request(
    trigger: On<BinReceive<LspCatalogRequest>>,
    outbox: Res<ManagerOutbox>,
    mut states: Query<&mut ManagerState>,
) {
    let entity = trigger.event().webview;
    if let Ok(mut state) = states.get_mut(entity) {
        state.start_loading();
    }
    let req = trigger.event().payload.clone();
    let sink = outbox.clone();
    std::thread::spawn(move || {
        let root = store::default_root();
        let pkgs = catalog::ensure_catalog(&root, req.refresh).unwrap_or_default();
        let mut out: Vec<LspPackage> =
            catalog::search(&pkgs, &req.query, &req.language, &req.category)
                .iter()
                .map(|p| to_lsp_package(&root, p))
                .collect();
        if req.installed_only {
            out.retain(|p| matches!(p.status, LspPkgStatus::Installed | LspPkgStatus::Outdated));
        }
        push(
            &sink,
            entity,
            ManagerMsg::Catalog(LspCatalogEvent { packages: out }),
        );
    });
}

fn install_named(outbox: &ManagerOutbox, active: &ActiveInstalls, entity: Entity, name: String) {
    {
        let mut installs = active.0.lock().unwrap_or_else(|error| error.into_inner());
        if !installs.insert(name.clone()) {
            return;
        }
    }
    let sink = outbox.clone();
    let active = active.clone();
    std::thread::spawn(move || {
        let root = store::default_root();
        let pkgs = catalog::ensure_catalog(&root, false).unwrap_or_default();
        let Some(pkg) = pkgs
            .iter()
            .find(|package| package.name.as_str() == name)
            .cloned()
        else {
            active
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&name);
            push(
                &sink,
                entity,
                ManagerMsg::Progress(LspInstallProgress {
                    name,
                    phase: InstallPhase::Failed,
                    pct: None,
                    message: "package not found in catalog".into(),
                }),
            );
            return;
        };
        let tid = target::host_target();
        let prog_sink = sink.clone();
        let prog_name = name.clone();
        let result = install::install(&pkg, &root, tid, |phase, pct, m| {
            push(
                &prog_sink,
                entity,
                ManagerMsg::Progress(LspInstallProgress {
                    name: prog_name.clone(),
                    phase,
                    pct,
                    message: m.to_string(),
                }),
            );
        });
        active
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&name);
        match result {
            Ok(receipt) => push(
                &sink,
                entity,
                ManagerMsg::Status(LspPkgStatusEvent {
                    name,
                    status: LspPkgStatus::Installed,
                    version: receipt.version,
                }),
            ),
            Err(e) => push(
                &sink,
                entity,
                ManagerMsg::Progress(LspInstallProgress {
                    name,
                    phase: InstallPhase::Failed,
                    pct: None,
                    message: e,
                }),
            ),
        }
    });
}

fn on_install_request(
    trigger: On<BinReceive<LspInstallRequest>>,
    outbox: Res<ManagerOutbox>,
    active: Res<ActiveInstalls>,
) {
    install_named(
        &outbox,
        &active,
        trigger.event().webview,
        trigger.event().payload.name.clone(),
    );
}

fn on_update_request(
    trigger: On<BinReceive<LspUpdateRequest>>,
    outbox: Res<ManagerOutbox>,
    active: Res<ActiveInstalls>,
) {
    install_named(
        &outbox,
        &active,
        trigger.event().webview,
        trigger.event().payload.name.clone(),
    );
}

fn on_uninstall_request(trigger: On<BinReceive<LspUninstallRequest>>, outbox: Res<ManagerOutbox>) {
    let entity = trigger.event().webview;
    let name = trigger.event().payload.name.clone();
    let sink = outbox.clone();
    std::thread::spawn(move || {
        let root = store::default_root();
        let Ok(package) = crate::lsp::package_path::PackageName::parse(&name) else {
            push(
                &sink,
                entity,
                ManagerMsg::Progress(LspInstallProgress {
                    name,
                    phase: InstallPhase::Failed,
                    pct: None,
                    message: "invalid package name".to_string(),
                }),
            );
            return;
        };
        if let Err(e) = store::remove(&root, &package) {
            push(
                &sink,
                entity,
                ManagerMsg::Progress(LspInstallProgress {
                    name,
                    phase: InstallPhase::Failed,
                    pct: None,
                    message: format!("uninstall failed: {e}"),
                }),
            );
            return;
        }
        let on_path = matches!(
            store::resolved_command(&root, &name),
            store::Resolution::OnPath
        );
        let status = if on_path {
            LspPkgStatus::OnPath
        } else {
            LspPkgStatus::Available
        };
        push(
            &sink,
            entity,
            ManagerMsg::Status(LspPkgStatusEvent {
                name,
                status,
                version: None,
            }),
        );
    });
}

fn file_uses_package(view: &crate::host::editor::FileView, package: &str) -> bool {
    view.path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::lsp::registry::preferred_package)
        == Some(package)
}

fn install_targets(
    source: Entity,
    package: &str,
    views: &Query<(Entity, &crate::host::editor::FileView)>,
) -> Vec<Entity> {
    let mut targets = vec![source];
    for (entity, view) in views {
        if file_uses_package(view, package) && !targets.contains(&entity) {
            targets.push(entity);
        }
    }
    targets
}

fn drain_manager_outbox(
    outbox: Res<ManagerOutbox>,
    browsers: NonSend<Browsers>,
    views: Query<(Entity, &crate::host::editor::FileView)>,
    mut managers: Query<&mut ManagerState>,
    mut commands: Commands,
) {
    let drained: Vec<(Entity, ManagerMsg)> = {
        let mut q = outbox.0.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    };
    for (entity, msg) in drained {
        match msg {
            ManagerMsg::Catalog(ev) => {
                if let Ok(mut state) = managers.get_mut(entity) {
                    state.apply_catalog(ev);
                } else if browsers.can_emit_to(&entity) {
                    commands.trigger(BinHostEmitEvent::from_event(entity, &ev));
                }
            }
            ManagerMsg::Progress(ev) => {
                if let Ok(mut state) = managers.get_mut(entity) {
                    state.apply_progress(ev.clone());
                }
                for target in install_targets(entity, &ev.name, &views) {
                    if browsers.can_emit_to(&target) {
                        if views.contains(target) {
                            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                                target, &ev,
                            ));
                        } else if !managers.contains(target) {
                            commands.trigger(BinHostEmitEvent::from_event(target, &ev));
                        }
                    }
                }
            }
            ManagerMsg::Status(ev) => {
                if let Ok(mut state) = managers.get_mut(entity) {
                    state.apply_status(ev.clone());
                }
                let targets = install_targets(entity, &ev.name, &views);
                if ev.status == LspPkgStatus::Installed {
                    for target in targets.iter().copied() {
                        if views
                            .get(target)
                            .is_ok_and(|(_, view)| file_uses_package(view, &ev.name))
                        {
                            commands
                                .entity(target)
                                .remove::<crate::lsp::manager::LspOpened>()
                                .remove::<crate::lsp::manager::LspStatusSent>();
                        }
                    }
                }
                for target in targets {
                    if browsers.can_emit_to(&target) {
                        if views.contains(target) {
                            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                                target, &ev,
                            ));
                        } else if !managers.contains(target) {
                            commands.trigger(BinHostEmitEvent::from_event(target, &ev));
                        }
                    }
                }
            }
        }
    }
}

fn reset_state_on_page_ready(
    trigger: On<BinReceive<PageReady>>,
    pages: Query<(), With<LspManagerPage>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if pages.contains(entity) {
        commands.entity(entity).remove::<ManagerStateSent>();
    }
}

fn publish_manager_state(
    pages: Query<(Entity, Ref<ManagerState>), With<LspManagerPage>>,
    ready: Query<(), With<PageReady>>,
    sent: Query<(), With<ManagerStateSent>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, state) in &pages {
        if !ready.contains(entity) {
            continue;
        }
        let already_sent = sent.contains(entity);
        if already_sent && !state.is_changed() {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_event(entity, &state.event()));
        if !already_sent {
            commands.entity(entity).insert(ManagerStateSent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::catalog::Package;

    fn pkg(name: &str, source_id: &str) -> Package {
        Package {
            name: crate::lsp::package_path::PackageName::parse(name).unwrap(),
            description: String::new(),
            languages: vec![],
            categories: vec![],
            source_id: source_id.into(),
            assets: vec![],
            bin: Default::default(),
        }
    }

    #[test]
    fn installability_by_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let gh = to_lsp_package(root, &pkg("zzz-fake-lsp", "pkg:github/x/zzz-fake-lsp@1"));
        assert!(gh.installable);
        assert_eq!(gh.requires, None);
        assert_eq!(gh.status, LspPkgStatus::Available);

        let np = to_lsp_package(root, &pkg("zzz-fake-ts", "pkg:npm/zzz-fake-ts@1"));
        let npm_present = crate::lsp::registry::executable_on_path("npm");
        assert_eq!(np.installable, npm_present);
        assert_eq!(np.requires.is_some(), !npm_present);

        let uk = to_lsp_package(root, &pkg("weird", "pkg:weirdsrc/weird@1"));
        assert!(!uk.installable);
        assert_eq!(uk.requires, None);
    }

    #[test]
    fn installed_with_newer_catalog_is_outdated() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(store::packages_dir(root).join("foo")).unwrap();
        let mut bin = std::collections::BTreeMap::new();
        let name = crate::lsp::package_path::PackageName::parse("foo").unwrap();
        bin.insert(
            name.clone(),
            crate::lsp::package_path::PackagePath::parse("foo-bin").unwrap(),
        );
        store::write_receipt(
            root,
            &name,
            &store::Receipt {
                name: name.clone(),
                version: Some("1.0".into()),
                source_id: "pkg:github/x/foo@1.0".into(),
                bin,
            },
        )
        .unwrap();
        let lp = to_lsp_package(root, &pkg("foo", "pkg:github/x/foo@2.0"));
        assert_eq!(lp.status, LspPkgStatus::Outdated);
        assert_eq!(lp.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        let outbox = ManagerOutbox::default();
        app.add_plugins(MinimalPlugins)
            .insert_resource(outbox.clone());
        outbox.0.lock().unwrap().push((
            Entity::PLACEHOLDER,
            ManagerMsg::Status(LspPkgStatusEvent {
                name: "x".into(),
                status: LspPkgStatus::Available,
                version: None,
            }),
        ));
        app.add_systems(Update, |ob: Res<ManagerOutbox>| {
            ob.0.lock().unwrap().drain(..).for_each(drop);
        });
        app.update();
        assert!(outbox.0.lock().unwrap().is_empty());
    }

    #[test]
    fn manager_state_applies_progress_and_final_status() {
        let mut state = ManagerState {
            packages: vec![LspPackage {
                name: "rust-analyzer".into(),
                description: String::new(),
                languages: vec!["Rust".into()],
                categories: Vec::new(),
                status: LspPkgStatus::Available,
                version: None,
                installable: true,
                requires: None,
            }],
            ..Default::default()
        };

        state.apply_progress(LspInstallProgress {
            name: "rust-analyzer".into(),
            phase: InstallPhase::Downloading,
            pct: Some(50),
            message: "Downloading".into(),
        });

        assert_eq!(state.packages[0].status, LspPkgStatus::Installing);
        assert_eq!(state.progress.len(), 1);

        state.apply_status(LspPkgStatusEvent {
            name: "rust-analyzer".into(),
            status: LspPkgStatus::Installed,
            version: Some("1.0".into()),
        });

        assert_eq!(state.packages[0].status, LspPkgStatus::Installed);
        assert_eq!(state.packages[0].version.as_deref(), Some("1.0"));
        assert!(state.progress.is_empty());
    }
}
