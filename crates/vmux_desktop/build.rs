use std::process::Command;

fn main() {
    // Git short hash (7 chars)
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo::rustc-env=VMUX_GIT_HASH={hash}");

    // Profile: VMUX_PROFILE env var, default to "dev" for debug builds, "release" for release builds
    let profile = std::env::var("VMUX_PROFILE").unwrap_or_else(|_| {
        if std::env::var("PROFILE").unwrap_or_default() == "release" {
            "release".to_string()
        } else {
            "dev".to_string()
        }
    });
    println!("cargo::rustc-env=VMUX_PROFILE={profile}");

    // Rebuild when HEAD changes or VMUX_PROFILE changes
    println!("cargo::rerun-if-changed=../../.git/HEAD");
    println!("cargo::rerun-if-changed=../../.git/refs");
    println!("cargo::rerun-if-env-changed=VMUX_PROFILE");
}
