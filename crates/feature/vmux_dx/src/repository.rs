use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::*;

use crate::Result;

#[derive(Component, Clone, Debug)]
pub struct Checkout {
    pub root: PathBuf,
    pub target: PathBuf,
    pub build_target: Option<OsString>,
}

impl Checkout {
    pub fn title(&self, profile: &str, product: &str) -> Result<String> {
        if profile == "release" {
            return Ok(product.into());
        }
        let mut source = String::new();
        for arguments in [
            ["symbolic-ref", "--quiet", "--short", "HEAD"].as_slice(),
            ["rev-parse", "--short=7", "HEAD"].as_slice(),
        ] {
            let mut command = crate::environment::Environment::current().command("git", &self.root);
            command.args(arguments);
            let output = command.output()?;
            if output.status.success() {
                source = std::str::from_utf8(&output.stdout)?.trim().to_owned();
                if !source.is_empty() {
                    break;
                }
            }
        }
        if source.is_empty() {
            source = "unknown".into();
        }
        Ok(match profile {
            "dev" => format!("{product} Dev ({source})"),
            "local" => format!("{product} ({source})"),
            other => format!("{product} ({other})"),
        })
    }

    pub fn directory(&self, profile: &str) -> PathBuf {
        let mut path = self.target.clone();
        if let Some(target) = &self.build_target {
            path.push(target);
        }
        path.join(profile)
    }

    pub fn configure(
        &mut self,
        target: Option<&OsStr>,
        build_target: Option<&OsStr>,
    ) -> Result<()> {
        let target = target
            .filter(|value| !value.is_empty())
            .unwrap_or(OsStr::new("target"));
        let path = if Path::new(target).is_absolute() {
            PathBuf::from(target)
        } else {
            self.root.join(target)
        };
        let path = std::path::absolute(path)?;
        let parent = path.parent().ok_or("Missing target parent")?;
        fs::create_dir_all(parent)?;
        let path = parent
            .canonicalize()?
            .join(path.file_name().ok_or("Missing target name")?);
        if path == self.root || self.root.starts_with(&path) {
            return Err("Cargo target overlaps the source repository".into());
        }
        self.target = path;
        self.build_target = build_target
            .filter(|value| !value.is_empty())
            .map(OsStr::to_owned);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    struct Fixture {
        directory: tempfile::TempDir,
        root: PathBuf,
    }

    impl Fixture {
        fn resolve(
            &self,
            path: &Path,
            branch: Option<&OsStr>,
        ) -> std::result::Result<Checkout, String> {
            let mut app = bevy_app::App::new();
            app.add_plugins(crate::repository_selection::Plugin);
            let entity = app
                .world_mut()
                .spawn(crate::repository_selection::Input {
                    path: path.to_owned(),
                    branch: branch.map(OsString::from),
                })
                .id();
            app.update();
            if let Some(checkout) = app.world().get::<Checkout>(entity) {
                return Ok(checkout.clone());
            }
            Err(app
                .world()
                .get::<vmux_ecs::cli::CliResult>(entity)
                .unwrap()
                .0
                .as_ref()
                .unwrap_err()
                .clone())
        }

        fn cloud() -> Self {
            let fixture = Self::new();
            fs::remove_dir_all(fixture.root.join("crates")).unwrap();
            fs::write(fixture.root.join(".gitmodules"), "").unwrap();
            fs::write(
                fixture.root.join("Cargo.toml"),
                "[workspace]\nmembers = []\n",
            )
            .unwrap();
            fixture.git(&["add", "-A"]);
            fixture.git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "Cloud fixture",
            ]);
            fixture
        }

        fn client(&self, cloud: &Path, branch: &str) -> PathBuf {
            let client = cloud.join("client");
            fs::create_dir_all(client.join("crates/app/vmux_cli")).unwrap();
            fs::write(
                client.join("crates/app/vmux_cli/Cargo.toml"),
                "[package]\nname = \"vmux_cli\"\n",
            )
            .unwrap();
            for arguments in [
                vec!["init", "-b", branch],
                vec!["add", "."],
                vec![
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-m",
                    "Client fixture",
                ],
            ] {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&client)
                    .args(arguments)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            client.canonicalize().unwrap()
        }
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("repository");
            fs::create_dir_all(root.join("crates/app/vmux_cli")).unwrap();
            fs::write(
                root.join("crates/app/vmux_cli/Cargo.toml"),
                "[package]\nname = \"vmux_cli\"\n",
            )
            .unwrap();
            let fixture = Self { directory, root };
            fixture.git(&["init", "-b", "main"]);
            fixture.git(&["add", "."]);
            fixture.git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "Fixture",
            ]);
            fixture
        }

        fn git(&self, args: &[&str]) {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn worktree(&self, branch: &str, relative: &str) -> PathBuf {
            let path = self.directory.path().join(relative);
            self.git(&["worktree", "add", "-b", branch, path.to_str().unwrap()]);
            path.canonicalize().unwrap()
        }
    }

    #[test]
    fn cloud_selection_discovers_client_branches_in_other_cloud_worktrees() {
        let fixture = Fixture::cloud();
        fixture.client(&fixture.root, "client-main");
        let cloud = fixture.worktree("cloud-topic", "different-cloud-directory");
        let client = fixture.client(&cloud, "client-topic");
        assert_eq!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("client-topic")))
                .unwrap()
                .root,
            client
        );
        let standalone = client.join(".worktrees/standalone-directory");
        let output = Command::new("git")
            .arg("-C")
            .arg(&client)
            .args(["worktree", "add", "-b", "client-standalone"])
            .arg(&standalone)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("client-standalone")))
                .unwrap()
                .root,
            standalone.canonicalize().unwrap()
        );
    }

    #[test]
    fn cloud_branch_precedes_client_branch_and_client_branch_precedes_directory_alias() {
        let fixture = Fixture::cloud();
        let client = fixture.client(&fixture.root, "navigation");
        fixture.worktree("other", "navigation");
        assert_eq!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("navigation")))
                .unwrap()
                .root,
            client
        );
        let selected = fixture.worktree("navigation", "cloud-priority");
        let selected_client = fixture.client(&selected, "unrelated-client-name");
        assert_eq!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("navigation")))
                .unwrap()
                .root,
            selected_client
        );
    }

    #[test]
    fn ambiguous_client_branches_across_cloud_worktrees_are_refused() {
        let fixture = Fixture::cloud();
        fixture.client(&fixture.root, "client-main");
        for (branch, directory) in [("first", "first-cloud"), ("second", "second-cloud")] {
            let cloud = fixture.worktree(branch, directory);
            fixture.client(&cloud, "shared-client-branch");
        }
        assert!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("shared-client-branch")))
                .unwrap_err()
                .to_string()
                .contains("multiple")
        );
    }

    #[test]
    fn build_titles_follow_the_selected_branch_and_detached_revision() {
        let fixture = Fixture::new();
        let worktree = fixture.worktree("fixture/topic", "different-directory");
        let checkout = fixture.resolve(&worktree, None).unwrap();
        assert_eq!(
            checkout.title("dev", "Vmux").unwrap(),
            "Vmux Dev (fixture/topic)"
        );
        assert_eq!(
            checkout.title("local", "Vmux").unwrap(),
            "Vmux (fixture/topic)"
        );
        assert_eq!(checkout.title("release", "Vmux").unwrap(), "Vmux");
        let status = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(["branch", "-m", "fixture/renamed"])
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            checkout.title("dev", "Vmux").unwrap(),
            "Vmux Dev (fixture/renamed)"
        );
        let status = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(["checkout", "--detach"])
            .output()
            .unwrap()
            .status;
        assert!(status.success());
        let revision = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .unwrap();
        assert!(revision.status.success());
        assert_eq!(
            checkout.title("local", "Vmux").unwrap(),
            format!(
                "Vmux ({})",
                std::str::from_utf8(&revision.stdout).unwrap().trim()
            )
        );
    }

    #[test]
    fn branch_name_resolves_a_differently_named_worktree_and_directory_alias_remains_available() {
        let fixture = Fixture::new();
        let worktree = fixture.worktree("topic/navigation", "checkout-with-another-name");
        assert_eq!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("topic/navigation")))
                .unwrap()
                .root,
            worktree
        );
        assert_eq!(
            fixture
                .resolve(
                    &fixture.root,
                    Some(OsStr::new("checkout-with-another-name"))
                )
                .unwrap()
                .root,
            worktree
        );
        let head = Command::new("git")
            .arg("-C")
            .arg(&fixture.root)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        assert_eq!(std::str::from_utf8(&head.stdout).unwrap().trim(), "main");
    }

    #[test]
    fn exact_branch_precedes_an_alias_and_ambiguous_directory_names_are_refused() {
        let fixture = Fixture::new();
        fixture.worktree("other", "navigation");
        let branch = fixture.worktree("navigation", "different-directory");
        assert_eq!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("navigation")))
                .unwrap()
                .root,
            branch
        );
        fixture.worktree("first", "one/shared");
        fixture.worktree("second", "two/shared");
        assert!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("shared")))
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
    }

    #[test]
    fn an_unchecked_out_branch_is_refused_and_no_selector_uses_the_current_checkout() {
        let fixture = Fixture::new();
        fixture.git(&["branch", "idle"]);
        assert!(
            fixture
                .resolve(&fixture.root, Some(OsStr::new("idle")))
                .is_err()
        );
        assert_eq!(
            fixture.resolve(&fixture.root, None).unwrap().root,
            fixture.root.canonicalize().unwrap()
        );
    }
}
