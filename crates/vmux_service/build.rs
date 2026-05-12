fn main() {
    let profile = std::env::var("VMUX_PROFILE").unwrap_or_else(|_| {
        match std::env::var("PROFILE").as_deref() {
            Ok("release") => "release".to_string(),
            _ => "dev".to_string(),
        }
    });
    println!("cargo::rustc-env=VMUX_PROFILE={profile}");
    println!("cargo::rerun-if-env-changed=VMUX_PROFILE");
}
