
use super::*;

pub fn run(dir: &Path, args: &[&str]) {
    let status = git_command(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

pub fn init() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    run(p, &["init", "-q", "-b", "main"]);
    run(p, &["config", "user.email", "t@example.com"]);
    run(p, &["config", "user.name", "Test"]);
    run(p, &["config", "commit.gpgsign", "false"]);
    dir
}

pub fn write(dir: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::write(&path, contents).unwrap();
    path
}
