use std::path::Path;
use std::process::Command;

pub fn emit() {
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo::rustc-env=VMUX_GIT_HASH={hash}");

    let profile = std::env::var("VMUX_BUILD_PROFILE").unwrap_or_else(|_| {
        match std::env::var("PROFILE").as_deref() {
            Ok("release") => "release".to_string(),
            _ => "dev".to_string(),
        }
    });
    println!("cargo::rustc-env=VMUX_BUILD_PROFILE={profile}");
    println!("cargo::rerun-if-env-changed=VMUX_BUILD_PROFILE");

    let mut refs = vec!["HEAD".to_string(), "logs/HEAD".to_string()];
    if let Some(head_ref) = symbolic_head_ref() {
        refs.push(head_ref);
    }
    for rel in refs {
        if let Some(p) = git_path(&rel)
            && Path::new(&p).exists()
        {
            println!("cargo::rerun-if-changed={p}");
        }
    }
}

fn git_path(rel: &str) -> Option<String> {
    git_stdout(&["rev-parse", "--git-path", rel])
}

fn symbolic_head_ref() -> Option<String> {
    git_stdout(&["symbolic-ref", "--quiet", "HEAD"])
}

fn git_stdout(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}
