use std::path::Path;
use std::process::Command;

pub fn emit() {
    println!("cargo::rerun-if-env-changed=VMUX_TARGET_DESTINATION");
    println!("cargo::rerun-if-env-changed=VMUX_HOST_NAME");
    let identity = BuildIdentity::read();
    println!("cargo::rustc-env=VMUX_GIT_HASH={}", identity.hash);
    println!("cargo::rustc-env=VMUX_GIT_BRANCH={}", identity.branch);

    let worktree =
        git_stdout(&["rev-parse", "--show-toplevel"]).unwrap_or_else(|| "unknown".to_string());
    println!(
        "cargo::rustc-env=VMUX_WORKTREE_ID={:016x}",
        stable_hash(&worktree)
    );

    let profile = std::env::var("VMUX_BUILD_PROFILE").unwrap_or_else(|_| {
        match std::env::var("PROFILE").as_deref() {
            Ok("release") => "release".to_string(),
            _ => "dev".to_string(),
        }
    });
    println!("cargo::rustc-env=VMUX_BUILD_PROFILE={profile}");
    println!(
        "cargo::rustc-env=VMUX_BUILD_TITLE={}",
        identity.title(&profile)
    );
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

struct BuildIdentity {
    hash: String,
    branch: String,
    name: String,
}

impl BuildIdentity {
    fn read() -> Self {
        Self {
            hash: git_stdout(&["rev-parse", "--short=7", "HEAD"])
                .unwrap_or_else(|| "unknown".to_string()),
            branch: git_stdout(&["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default(),
            name: std::env::var("VMUX_HOST_NAME").unwrap_or_else(|_| "Vmux".to_string()),
        }
    }

    fn title(&self, profile: &str) -> String {
        let source = if self.branch.is_empty() {
            &self.hash
        } else {
            &self.branch
        };
        match profile {
            "release" => self.name.clone(),
            "local" => format!("{} ({source})", self.name),
            "dev" => format!("{} Dev ({source})", self.name),
            other => format!("{} ({other})", self.name),
        }
    }
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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
