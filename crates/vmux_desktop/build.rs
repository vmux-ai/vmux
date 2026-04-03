#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() {
    use std::env;
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=CEF_DIR");
    println!("cargo:rerun-if-env-changed=USERPROFILE");

    if let Ok(dir) = env::var("CEF_DIR") {
        let p = PathBuf::from(dir.trim());
        if p.is_dir() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }

    if let Ok(home) = env::var("USERPROFILE") {
        let cef = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("cef");
        println!("cargo:rerun-if-changed={}", cef.display());
    }
}
