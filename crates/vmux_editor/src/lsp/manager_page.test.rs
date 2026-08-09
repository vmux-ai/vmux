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
