use std::path::Path;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};
use vmux_core::event::{
    InstallPhase, LSP_CATALOG_EVENT, LSP_INSTALL_PROGRESS_EVENT, LSP_PKG_STATUS_EVENT,
    LspCatalogEvent, LspCatalogRequest, LspInstallProgress, LspInstallRequest, LspPackage,
    LspPkgStatus, LspPkgStatusEvent, LspUninstallRequest, LspUpdateRequest,
};

use vmux_core::{CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

use crate::lsp::catalog::{self, Package};
use crate::lsp::{install, purl, store, target};

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "lsp",
    title: "Language Servers",
    keywords: &["lsp", "language", "server", "install", "mason"],
    icon: "server",
    command_bar: true,
};

pub enum ManagerMsg {
    Catalog(LspCatalogEvent),
    Progress(LspInstallProgress),
    Status(LspPkgStatusEvent),
}

#[derive(Resource, Clone, Default)]
pub struct ManagerOutbox(pub Arc<Mutex<Vec<(Entity, ManagerMsg)>>>);

pub struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.init_resource::<ManagerOutbox>()
            .add_message::<CefPageAttachRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                LspCatalogRequest,
                LspInstallRequest,
                LspUninstallRequest,
                LspUpdateRequest,
            )>::default())
            .add_observer(on_catalog_request)
            .add_observer(on_install_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_update_request)
            .add_systems(
                Update,
                handle_lsp_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(Update, drain_manager_outbox);
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_lsp_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    mut attach_writer: MessageWriter<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url != "vmux://lsp/" {
            continue;
        }
        attach_writer.write(CefPageAttachRequest {
            stack: task.stack,
            url: task.url.clone(),
            title: "Language Servers".to_string(),
            bg_color: None,
        });
        commands.entity(entity).insert(PageOpenHandled);
    }
}

pub fn to_lsp_package(root: &Path, p: &Package) -> LspPackage {
    let kind = purl::parse(&p.source_id)
        .map(|x| x.kind)
        .unwrap_or_default();
    let installed = store::is_installed(root, &p.name);
    let on_path = !installed
        && matches!(
            store::resolved_command(root, &p.name),
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
        name: p.name.clone(),
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

fn on_catalog_request(trigger: On<BinReceive<LspCatalogRequest>>, outbox: Res<ManagerOutbox>) {
    let entity = trigger.event().webview;
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

fn install_named(outbox: &ManagerOutbox, entity: Entity, name: String) {
    let sink = outbox.clone();
    std::thread::spawn(move || {
        let root = store::default_root();
        let pkgs = catalog::ensure_catalog(&root, false).unwrap_or_default();
        let Some(pkg) = pkgs.iter().find(|p| p.name == name).cloned() else {
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

fn on_install_request(trigger: On<BinReceive<LspInstallRequest>>, outbox: Res<ManagerOutbox>) {
    install_named(
        &outbox,
        trigger.event().webview,
        trigger.event().payload.name.clone(),
    );
}

fn on_update_request(trigger: On<BinReceive<LspUpdateRequest>>, outbox: Res<ManagerOutbox>) {
    install_named(
        &outbox,
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
        if let Err(e) = store::remove(&root, &name) {
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

fn drain_manager_outbox(
    outbox: Res<ManagerOutbox>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let drained: Vec<(Entity, ManagerMsg)> = {
        let mut q = outbox.0.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    };
    for (entity, msg) in drained {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        match msg {
            ManagerMsg::Catalog(ev) => {
                commands.trigger(BinHostEmitEvent::from_rkyv(entity, LSP_CATALOG_EVENT, &ev))
            }
            ManagerMsg::Progress(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                LSP_INSTALL_PROGRESS_EVENT,
                &ev,
            )),
            ManagerMsg::Status(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                LSP_PKG_STATUS_EVENT,
                &ev,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::catalog::Package;

    fn pkg(name: &str, source_id: &str) -> Package {
        Package {
            name: name.into(),
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
        bin.insert("foo".to_string(), "foo-bin".to_string());
        store::write_receipt(
            root,
            &store::Receipt {
                name: "foo".into(),
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
    fn page_open_claims_lsp_url() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CefPageAttachRequest>()
            .add_systems(Update, handle_lsp_page_open);
        let stack = app.world_mut().spawn_empty().id();
        let claimed = app
            .world_mut()
            .spawn(PageOpenTask {
                id: vmux_core::PageOpenId::new(),
                stack,
                url: "vmux://lsp/".to_string(),
                request_id: None,
            })
            .id();
        let other = app
            .world_mut()
            .spawn(PageOpenTask {
                id: vmux_core::PageOpenId::new(),
                stack,
                url: "vmux://history/".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(claimed).is_some());
        assert!(app.world().get::<PageOpenHandled>(other).is_none());
    }
}
