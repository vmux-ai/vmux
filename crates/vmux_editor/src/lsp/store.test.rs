use super::*;

fn receipt(name: &str) -> Receipt {
    let mut bin = BTreeMap::new();
    bin.insert(name.to_string(), format!("{name}-bin"));
    Receipt {
        name: name.to_string(),
        version: Some("1.0".into()),
        source_id: "pkg:github/x/y@1.0".into(),
        bin,
    }
}

#[test]
fn write_read_installed_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let pkgdir = packages_dir(root).join("foo");
    std::fs::create_dir_all(&pkgdir).unwrap();
    std::fs::write(pkgdir.join("foo-bin"), b"#!/bin/sh\n").unwrap();
    write_receipt(root, &receipt("foo")).unwrap();

    assert!(is_installed(root, "foo"));
    assert_eq!(installed(root).len(), 1);
    assert_eq!(
        read_receipt(root, "foo").unwrap().version.as_deref(),
        Some("1.0")
    );
}

#[test]
fn link_and_remove() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let pkgdir = packages_dir(root).join("foo");
    std::fs::create_dir_all(&pkgdir).unwrap();
    std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
    write_receipt(root, &receipt("foo")).unwrap();
    link_bin(root, "foo", "foo-bin", "foo").unwrap();

    assert!(bin_path(root, "foo").is_some());
    remove(root, "foo").unwrap();
    assert!(!is_installed(root, "foo"));
    assert!(!bin_dir(root).join("foo").exists());
}

#[test]
fn resolution_prefers_managed_then_path_then_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let pkgdir = packages_dir(root).join("foo");
    std::fs::create_dir_all(&pkgdir).unwrap();
    std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
    write_receipt(root, &receipt("foo")).unwrap();
    link_bin(root, "foo", "foo-bin", "foo").unwrap();
    assert!(matches!(
        resolved_command(root, "foo"),
        Resolution::Managed(_)
    ));
    assert_eq!(resolved_command(root, "cargo"), Resolution::OnPath);
    assert_eq!(
        resolved_command(root, "definitely-not-real-zzz"),
        Resolution::Missing
    );
}
