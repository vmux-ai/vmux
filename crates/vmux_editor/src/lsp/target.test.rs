use super::*;

fn asset(target: &str) -> Asset {
    Asset {
        target: target.into(),
        file: format!("file-{target}.gz"),
        bin: Some("bin".into()),
    }
}

#[test]
fn host_target_is_known_on_this_machine() {
    assert_ne!(host_target(), "unsupported");
}

#[test]
fn picks_exact_target() {
    let assets = vec![asset("darwin_arm64"), asset("linux_x64_gnu")];
    assert_eq!(
        pick_asset(&assets, "darwin_arm64").unwrap().target,
        "darwin_arm64"
    );
}

#[test]
fn linux_x64_falls_back_to_musl() {
    let assets = vec![asset("linux_x64_musl"), asset("darwin_arm64")];
    assert_eq!(
        pick_asset(&assets, "linux_x64_gnu").unwrap().target,
        "linux_x64_musl"
    );
}

#[test]
fn no_match_is_none() {
    let assets = vec![asset("win_x64")];
    assert!(pick_asset(&assets, "darwin_arm64").is_none());
}
