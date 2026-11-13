use std::path::PathBuf;
#[cfg(target_os = "macos")]
use vmux_app::host::safe_storage::{SafeStorage, SafeStorageContext, SafeStorageError};
use vmux_app::host::{ProfilePaths, ServicePaths};

#[test]
fn compiled_product_namespaces_control_config_data_and_runtime_paths() {
    let namespace = std::env::var("EXPECTED_NAMESPACE").expect("selected host fixture namespace");
    let home = PathBuf::from(std::env::var_os("HOME").expect("home directory"));
    let custom = namespace != "ai.vmux";
    let data_name = if custom { namespace.as_str() } else { "Vmux" };
    let config_name = if custom {
        format!(".{}", namespace.replace('.', "-"))
    } else {
        ".vmux".into()
    };
    #[cfg(target_os = "macos")]
    let data = home.join("Library/Application Support").join(data_name);
    #[cfg(not(target_os = "macos"))]
    let data = std::env::temp_dir().join(data_name);
    let paths = ProfilePaths::current();
    assert_eq!(paths.application_data(), data);
    assert_eq!(paths.config(), home.join(config_name));
    assert_eq!(paths.profile(), data.join("profiles/host-probe"));
    let stem = if custom {
        namespace.replace('.', "-")
    } else {
        "vmux".into()
    };
    let service = ServicePaths::current();
    assert_eq!(
        service.socket(),
        data.join("dev/services")
            .join(format!("{stem}-dev-host-probe.sock"))
    );
    assert_eq!(
        service.daemon_lock(),
        data.join("dev/services")
            .join(format!("{stem}-dev-host-probe.lock"))
    );
    assert_eq!(
        service.identity(),
        data.join("dev/services")
            .join(format!("{stem}-dev-host-probe.identity"))
    );
}

#[cfg(target_os = "macos")]
#[test]
fn importing_host_metadata_does_not_grant_desktop_credential_access() {
    let directory = tempfile::tempdir().unwrap();
    let context = SafeStorageContext::at(directory.path().to_path_buf());
    assert!(matches!(
        SafeStorage::wrap_vault_key(&context, "host-probe", &[7; 32]),
        Err(SafeStorageError::DesktopProcessRequired)
    ));
    assert_eq!(directory.path().read_dir().unwrap().count(), 0);
}
