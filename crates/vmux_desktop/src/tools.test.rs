use super::*;

#[test]
fn parses_brew_inventory_with_versions() {
    assert_eq!(
        parse_brew_versions(b"ripgrep 14.1.1\nopenssl@3 3.5.0 3.5.1\n"),
        vec![
            ("ripgrep".to_string(), Some("14.1.1".to_string())),
            ("openssl@3".to_string(), Some("3.5.0 3.5.1".to_string())),
        ]
    );
}

#[test]
fn category_adds_declared_missing_packages() {
    let mut manifest = ToolsManifest::default();
    manifest.set_package(ToolProvider::Npm.id(), "typescript", true);
    let category = build_category(ToolProvider::Npm, Vec::new(), &manifest);
    assert_eq!(category.items.len(), 1);
    assert_eq!(category.items[0].status, ToolStatus::Missing);
    assert!(category.items[0].managed);
    assert_eq!(
        category.items[0].actions,
        [ToolAction::Install, ToolAction::Forget]
    );
}

#[test]
fn parses_scoped_npm_packages_and_outdated_state() {
    let inventory = parse_npm_inventory(
        br#"{"dependencies":{"@scope/tool":{"version":"2.0.0"},"typescript":{"version":"5.9.0"}}}"#,
        &BTreeSet::from(["@scope/tool".to_string()]),
    )
    .unwrap();
    assert_eq!(inventory.len(), 2);
    let scoped = inventory
        .iter()
        .find(|item| item.id == "@scope/tool")
        .unwrap();
    assert_eq!(scoped.version.as_deref(), Some("2.0.0"));
    assert_eq!(scoped.status, ToolStatus::Outdated);
}

#[test]
fn bulk_import_adopts_only_installed_inventory() {
    let mut manifest = ToolsManifest::default();
    let imported = import_inventory(
        &mut manifest,
        ToolProvider::Npm,
        vec![
            InventoryItem {
                id: "installed".to_string(),
                name: "installed".to_string(),
                version: Some("1".to_string()),
                detail: String::new(),
                status: ToolStatus::Installed,
                removable: true,
            },
            InventoryItem {
                id: "missing".to_string(),
                name: "missing".to_string(),
                version: None,
                detail: String::new(),
                status: ToolStatus::Missing,
                removable: true,
            },
        ],
    );

    assert_eq!(imported, 1);
    assert!(manifest.contains("npm", "installed"));
    assert!(!manifest.contains("npm", "missing"));
}

#[test]
fn unmanaged_installed_packages_can_be_adopted() {
    assert_eq!(
        package_actions(ToolStatus::Installed, false, true),
        [ToolAction::Adopt, ToolAction::Uninstall]
    );
    assert_eq!(
        package_actions(ToolStatus::Outdated, true, true),
        [ToolAction::Update, ToolAction::Uninstall]
    );
}

#[test]
fn vault_backup_watcher_ignores_runtime_and_access_events() {
    let root = vmux_core::profile::vault::root_dir();
    let knowledge = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
        .add_path(root.join("knowledge/note.md"));
    let runtime = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
        .add_path(root.join("workspace/repo/file.rs"));
    let access = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Any))
        .add_path(root.join("tools/tools.toml"));

    assert!(vault_event_requests_sync(&Ok(knowledge)));
    assert!(!vault_event_requests_sync(&Ok(runtime)));
    assert!(!vault_event_requests_sync(&Ok(access)));
}

#[test]
fn automatic_backup_queues_only_unlocked_vaults_needing_sync() {
    fn queued(vault: VaultSnapshot, remote_check: bool) -> usize {
        let mut app = App::new();
        app.init_resource::<ToolsState>()
            .init_resource::<VaultAutoSync>()
            .init_resource::<VaultActionQueue>()
            .add_systems(Update, queue_vault_auto_sync);
        {
            let mut state = app.world_mut().resource_mut::<ToolsState>();
            state.loaded = true;
            state.dirty = false;
            state.snapshot.vault = vault;
        }
        let mut auto_sync = app.world_mut().resource_mut::<VaultAutoSync>();
        auto_sync.requested = true;
        auto_sync.remote_check = remote_check;

        app.update();

        app.world().resource::<VaultActionQueue>().0.len()
    }

    let connected = VaultSnapshot {
        initialized: true,
        unlocked: true,
        remote: "https://example.com/vault.git".to_string(),
        dirty: 1,
        ..Default::default()
    };
    assert_eq!(queued(connected.clone(), false), 1);
    assert_eq!(
        queued(
            VaultSnapshot {
                unlocked: false,
                ..connected.clone()
            },
            false
        ),
        0
    );
    assert_eq!(
        queued(
            VaultSnapshot {
                dirty: 0,
                ahead: 1,
                ..connected.clone()
            },
            false
        ),
        1
    );
    assert_eq!(
        queued(
            VaultSnapshot {
                dirty: 0,
                ..connected.clone()
            },
            false
        ),
        0
    );
    assert_eq!(
        queued(
            VaultSnapshot {
                dirty: 0,
                ..connected
            },
            true,
        ),
        1
    );
}
