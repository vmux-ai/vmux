use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::execution::Execution;

pub struct Scope {
    root: PathBuf,
    #[cfg(test)]
    _temporary: Option<tempfile::TempDir>,
}

impl Scope {
    pub(crate) fn policy(path: &Path) -> bool {
        if path.file_name().is_none_or(|name| name != "ecs-policy.ron") {
            return false;
        }
        if path == Path::new("ecs-policy.ron") {
            return true;
        }
        let Some(directory) = path.parent() else {
            return false;
        };
        if directory
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return false;
        }
        if matches!(directory.to_str(), Some("crates" | "website" | "scripts")) {
            return true;
        }
        matches!(
            directory.parent().and_then(Path::to_str),
            Some("crates/app" | "crates/feature" | "crates/util" | "patches" | "examples")
        )
    }

    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
            #[cfg(test)]
            _temporary: None,
        })
    }

    pub fn run(&self, args: &[OsString]) -> Result<()> {
        let comparison = Comparison::parse(args)?;
        let mut workspace = Workspace::load(&self.root)?;
        let selection = match comparison.files(&self.root)? {
            Some((base, files)) => {
                if files.iter().any(|file| Selection::global(file)) {
                    workspace.full()
                } else {
                    workspace.previous(&self.root, &base, &files)?;
                    let mut selection = workspace.select(&files)?;
                    workspace.variants(&self.root, &mut selection)?;
                    selection
                }
            }
            None => workspace.full(),
        };
        selection.emit(&workspace);
        Ok(())
    }
}

struct Comparison {
    base: Option<OsString>,
    head: OsString,
}

impl Comparison {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut comparison = Self {
            base: std::env::var_os("VMUX_CI_BASE").filter(|base| !base.is_empty()),
            head: "HEAD".into(),
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let value = args.next().ok_or("Missing CI comparison reference")?;
            if value.is_empty() || value.to_string_lossy().starts_with('-') {
                return Err("Invalid CI comparison reference".into());
            }
            match arg.to_str() {
                Some("--base") => comparison.base = Some(value.clone()),
                Some("--head") => comparison.head = value.clone(),
                _ => return Err(format!("Unknown CI scope argument: {arg:?}").into()),
            }
        }
        Ok(comparison)
    }

    fn files(&self, root: &Path) -> Result<Option<(String, Vec<PathBuf>)>> {
        let Some(base) = &self.base else {
            return Ok(None);
        };
        let mut command = Command::new("git");
        command
            .args(["merge-base", "--"])
            .arg(base)
            .arg(&self.head)
            .current_dir(root);
        let output = Execution::new(command).output(None)?;
        let base = String::from_utf8(output.stdout)?;
        let mut command = Command::new("git");
        command
            .args(["diff", "--name-only", "--no-renames", "-z"])
            .arg(base.trim())
            .arg(&self.head)
            .arg("--")
            .current_dir(root);
        let output = Execution::new(command).output(None)?;
        let paths = String::from_utf8(output.stdout)?;
        let mut files = Vec::new();
        for path in paths.split('\0') {
            if !path.is_empty() {
                files.push(PathBuf::from(path));
            }
        }
        Ok(Some((base.trim().to_owned(), files)))
    }
}

struct Workspace {
    packages: Vec<Package>,
    previous: Vec<Package>,
}

impl Workspace {
    fn load(root: &Path) -> Result<Self> {
        let mut command =
            Command::new(std::env::var_os("CARGO_BIN").unwrap_or_else(|| "cargo".into()));
        command
            .args(["metadata", "--locked", "--no-deps", "--format-version=1"])
            .current_dir(root);
        let metadata = Execution::new(command).output(None)?;
        let mut command = Command::new("jq");
        command.args([
            "-j",
            r#".packages[] | .name, "\u0000", .manifest_path, "\u0000", ([.dependencies[].name] | join(",")), "\u0000""#,
        ]);
        let output = Execution::new(command).output(Some(&metadata.stdout))?;
        Self::parse(root, &String::from_utf8(output.stdout)?)
    }

    fn parse(root: &Path, records: &str) -> Result<Self> {
        let mut fields = records.split_terminator('\0');
        let mut packages = Vec::new();
        while let Some(name) = fields.next() {
            if name.is_empty()
                || name.starts_with('-')
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                return Err("Invalid CI package name".into());
            }
            let manifest = fields.next().ok_or("Missing CI package manifest")?;
            let dependencies = fields.next().ok_or("Missing CI package dependencies")?;
            let directory = Path::new(manifest)
                .parent()
                .ok_or("Invalid CI package manifest")?
                .strip_prefix(root)?
                .to_path_buf();
            let mut uses = BTreeSet::new();
            for name in dependencies.split(',') {
                if !name.is_empty() {
                    uses.insert(name.to_owned());
                }
            }
            packages.push(Package {
                name: name.to_owned(),
                directory,
                dependencies: uses,
            });
        }
        if packages.is_empty() {
            return Err("Cargo metadata returned no workspace packages".into());
        }
        if packages
            .iter()
            .all(|package| package.directory.starts_with("patches"))
        {
            return Err("Cargo metadata returned no owned workspace packages".into());
        }
        Ok(Self {
            packages,
            previous: Vec::new(),
        })
    }

    fn previous(&mut self, root: &Path, base: &str, files: &[PathBuf]) -> Result<()> {
        let mut manifests = Vec::new();
        for file in files {
            if file.file_name().is_some_and(|name| name == "Cargo.toml")
                && file.starts_with("crates")
            {
                manifests.push(file);
            }
        }
        if manifests.is_empty() {
            return Ok(());
        }
        let mut command = Command::new("git");
        command
            .args(["show", &format!("{base}:Cargo.toml")])
            .current_dir(root);
        let output = Execution::new(command).output(None)?;
        let workspace: toml::Value = toml::from_str(&String::from_utf8(output.stdout)?)?;
        for manifest in manifests {
            let mut command = Command::new("git");
            command
                .args(["ls-tree", "-z", base, "--"])
                .arg(manifest)
                .current_dir(root);
            let output = Execution::new(command).output(None)?;
            if output.stdout.is_empty() {
                continue;
            }
            if !matches!(output.stdout.get(..6), Some(b"100644" | b"100755")) {
                return Err("Historical CI manifest must be a regular Git blob".into());
            }
            let path = manifest
                .to_str()
                .ok_or("Invalid historical CI manifest path")?;
            let mut command = Command::new("git");
            command
                .args(["show", &format!("{base}:{path}")])
                .current_dir(root);
            let output = Execution::new(command).output(None)?;
            self.previous.push(Package::previous(
                manifest,
                &String::from_utf8(output.stdout)?,
                &workspace,
            )?);
        }
        Ok(())
    }

    fn variants(&self, root: &Path, selection: &mut Selection) -> Result<()> {
        if selection.workspace {
            return Ok(());
        }
        let packages = selection.rust_packages();
        if !packages.is_empty() {
            let mut args = Vec::new();
            for package in &packages {
                args.extend(["-p", package.as_str()]);
            }
            selection
                .inputs
                .insert("test-rust".into(), self.tree(root, &args)?);
        }
        if selection.sdk {
            let mut sdk = self.tree(
                root,
                &["--manifest-path", "examples/framework-consumer/Cargo.toml"],
            )?;
            sdk.extend(self.tree(
                root,
                &["--manifest-path", "examples/host-consumer/Cargo.toml"],
            )?);
            for package in [
                "vmux_ecs",
                "vmux_project",
                "vmux_extension",
                "vmux_flex",
                "vmux_layout",
            ] {
                sdk.extend(self.dependencies(BTreeSet::from([package.to_owned()])));
            }
            selection.inputs.insert("test-sdk".into(), sdk);
        }
        for (key, enabled, args) in [
            (
                "test-features",
                selection.headless,
                &[
                    "-p",
                    "vmux_app",
                    "--no-default-features",
                    "--features",
                    "daemon,mcp",
                ][..],
            ),
            ("test-desktop", selection.lean, &["-p", "vmux_desktop"][..]),
            (
                "records",
                selection.records,
                &[
                    "-p",
                    "vmux_ecs",
                    "--no-default-features",
                    "--features",
                    "store,records",
                ][..],
            ),
            (
                "render-free",
                selection.render_free,
                &["-p", "vmux_session", "--no-default-features"][..],
            ),
        ] {
            if enabled {
                selection.inputs.insert(key.into(), self.tree(root, args)?);
            }
        }
        Ok(())
    }

    fn tree(&self, root: &Path, args: &[&str]) -> Result<BTreeSet<String>> {
        let mut command =
            Command::new(std::env::var_os("CARGO_BIN").unwrap_or_else(|| "cargo".into()));
        command
            .args([
                "tree",
                "--locked",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--prefix",
                "none",
                "--format",
                "{p}",
            ])
            .args(args)
            .current_dir(root);
        let output = Execution::new(command).output(None)?;
        let mut names = BTreeSet::new();
        for line in String::from_utf8(output.stdout)?.lines() {
            let Some(name) = line.split_whitespace().next() else {
                continue;
            };
            if self.packages.iter().any(|package| package.name == name) {
                names.insert(name.to_owned());
            }
        }
        if names.is_empty() {
            return Err("Cargo tree returned no owned CI inputs".into());
        }
        loop {
            let before = names.len();
            for package in &self.previous {
                if names.contains(&package.name)
                    || self.packages.iter().any(|current| {
                        current.directory == package.directory && names.contains(&current.name)
                    })
                {
                    names.extend(self.dependencies(package.dependencies.clone()));
                }
            }
            if names.len() == before {
                break;
            }
        }
        Ok(names)
    }

    fn dependencies(&self, mut names: BTreeSet<String>) -> BTreeSet<String> {
        loop {
            let before = names.len();
            for package in self.packages.iter().chain(&self.previous) {
                if names.contains(&package.name)
                    || self.packages.iter().any(|current| {
                        current.directory == package.directory && names.contains(&current.name)
                    })
                {
                    names.insert(package.name.clone());
                    names.extend(package.dependencies.clone());
                }
            }
            if names.len() == before {
                return names;
            }
        }
    }

    fn full(&self) -> Selection {
        let mut lint = BTreeSet::new();
        for package in &self.packages {
            if !package.directory.starts_with("patches") {
                lint.insert(package.name.clone());
            }
        }
        Selection {
            packages: BTreeSet::new(),
            lint,
            workspace: true,
            lean: true,
            sdk: true,
            headless: true,
            records: true,
            render_free: true,
            tooling: true,
            inputs: BTreeMap::new(),
        }
    }

    fn select(&self, files: &[PathBuf]) -> Result<Selection> {
        let mut selected = BTreeSet::new();
        let mut lean = false;
        let mut policy = false;
        let mut sdk = false;
        for file in files {
            let independent = file.starts_with("website")
                && !self
                    .packages
                    .iter()
                    .any(|package| package.directory.starts_with("website"));
            let consumer = file.starts_with("examples/framework-consumer")
                || file.starts_with("examples/host-consumer");
            if Scope::policy(file)
                || independent
                || consumer
                || matches!(
                    file.to_str(),
                    Some("docs/architecture.md" | "docs/dx/architecture.md")
                )
            {
                if !self.packages.iter().any(|package| {
                    package.name == "vmux_dx"
                        && package.directory == Path::new("crates/feature/vmux_dx")
                }) {
                    return Err("CI policy inputs require the canonical vmux_dx package".into());
                }
                policy = true;
                sdk |= consumer;
                continue;
            }
            if Selection::documentation(file) {
                continue;
            }
            if Selection::global(file) {
                return Ok(self.full());
            }
            let owner = self
                .packages
                .iter()
                .chain(&self.previous)
                .filter(|package| file.starts_with(&package.directory))
                .max_by_key(|package| package.directory.components().count());
            let Some(owner) = owner else {
                return Err(format!("CI input has no package owner: {}", file.display()).into());
            };
            for package in self.packages.iter().chain(&self.previous) {
                if package.directory == owner.directory {
                    lean |= package.name == "vmux_desktop";
                    selected.insert(package.name.clone());
                }
            }
        }
        Ok(self.selection(selected, files, lean, sdk, policy))
    }

    fn selection(
        &self,
        mut selected: BTreeSet<String>,
        files: &[PathBuf],
        lean: bool,
        mut sdk: bool,
        policy: bool,
    ) -> Selection {
        selected.retain(|name| self.packages.iter().any(|package| package.name == *name));
        let lean = lean && selected.contains("vmux_desktop");
        let tooling = selected.contains("vmux_dx") || selected.contains("vmux_cli");
        let headless = selected.contains("vmux_app");
        sdk |= selected.contains("vmux_app");
        let render_free = selected.contains("vmux_session");
        let records = selected.contains("vmux_ecs")
            && files.iter().any(|file| {
                file.starts_with("crates/util/vmux_ecs")
                    && !file.starts_with("crates/util/vmux_ecs/src/cli")
                    && file != Path::new("crates/util/vmux_ecs/src/cli.rs")
                    && !Scope::policy(file)
            });
        if policy {
            selected.insert("vmux_dx".into());
        }
        let mut packages = BTreeSet::new();
        let mut lint = BTreeSet::new();
        for package in &self.packages {
            if selected.contains(&package.name) && package.name != "vmux_dx" {
                packages.insert(package.name.clone());
            }
            if selected.contains(&package.name) && !package.directory.starts_with("patches") {
                lint.insert(package.name.clone());
            }
        }
        Selection {
            packages,
            lint,
            workspace: false,
            lean,
            sdk,
            headless,
            records,
            render_free,
            tooling,
            inputs: BTreeMap::new(),
        }
    }
}

struct Package {
    name: String,
    directory: PathBuf,
    dependencies: BTreeSet<String>,
}

impl Package {
    fn previous(manifest: &Path, source: &str, workspace: &toml::Value) -> Result<Self> {
        let document: toml::Value = toml::from_str(source)?;
        let name = document
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing historical CI package name")?;
        let mut dependencies = BTreeSet::new();
        Self::dependencies(&document, workspace, &mut dependencies)?;
        if let Some(targets) = document.get("target").and_then(toml::Value::as_table) {
            for target in targets.values() {
                Self::dependencies(target, workspace, &mut dependencies)?;
            }
        }
        Ok(Self {
            name: name.to_owned(),
            directory: manifest
                .parent()
                .ok_or("Missing historical CI package directory")?
                .to_owned(),
            dependencies,
        })
    }

    fn dependencies(
        document: &toml::Value,
        workspace: &toml::Value,
        names: &mut BTreeSet<String>,
    ) -> Result<()> {
        for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(dependencies) = document.get(kind).and_then(toml::Value::as_table) else {
                continue;
            };
            for (alias, value) in dependencies {
                let value = if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                    workspace
                        .get("workspace")
                        .and_then(|workspace| workspace.get("dependencies"))
                        .and_then(|dependencies| dependencies.get(alias))
                        .ok_or("Missing inherited historical CI dependency")?
                } else {
                    value
                };
                names.insert(
                    value
                        .get("package")
                        .and_then(toml::Value::as_str)
                        .unwrap_or(alias)
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
}

struct Selection {
    packages: BTreeSet<String>,
    lint: BTreeSet<String>,
    workspace: bool,
    lean: bool,
    sdk: bool,
    headless: bool,
    records: bool,
    render_free: bool,
    tooling: bool,
    inputs: BTreeMap<String, BTreeSet<String>>,
}

impl Selection {
    fn documentation(path: &Path) -> bool {
        path.starts_with("docs")
            || (!path.starts_with("crates")
                && path.extension().is_some_and(|extension| extension == "md"))
    }

    fn global(path: &Path) -> bool {
        if Self::documentation(path) || Scope::policy(path) {
            return false;
        }
        if path.file_name().is_some_and(|name| name == "Cargo.lock") {
            return !path.starts_with("website") && !path.starts_with("examples");
        }
        if matches!(
            path.to_str(),
            Some(
                "Cargo.toml"
                    | "build.rs"
                    | "rust-toolchain.toml"
                    | "tool-versions.env"
                    | ".gitattributes"
                    | ".env"
            )
        ) || path.to_str().is_some_and(|path| path.starts_with(".env."))
        {
            return true;
        }
        for directory in [".cargo", ".github", "scripts", "cef", "packaging"] {
            if path.starts_with(directory) {
                return true;
            }
        }
        path.parent() == Some(Path::new("crates"))
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("build_") && name.ends_with(".rs"))
    }

    fn rust_packages(&self) -> BTreeSet<String> {
        let mut packages = self.packages.clone();
        packages.remove("vmux_desktop");
        packages
    }

    fn arguments(&self) -> String {
        if self.workspace {
            return "--workspace --exclude vmux_dx --exclude vmux_desktop".into();
        }
        let mut args = String::new();
        for package in self.rust_packages() {
            if !args.is_empty() {
                args.push(' ');
            }
            args.push_str("-p ");
            args.push_str(&package);
        }
        args
    }

    fn emit(&self, workspace: &Workspace) {
        let packages = self.rust_packages();
        let args = self.arguments();
        eprintln!("CI test scope: {args}");
        println!("args={args}");
        print!("lint_args=");
        let mut separator = "";
        for package in &self.lint {
            print!("{separator}-p {package}");
            separator = " ";
        }
        println!();
        println!("lean={}", self.lean);
        println!("desktop={}", self.lean);
        println!("rust={}", self.workspace || !packages.is_empty());
        println!("sdk={}", self.sdk);
        println!("headless={}", self.headless);
        println!("records={}", self.records);
        println!("render_free={}", self.render_free);
        println!("tooling={}", self.tooling);
        let mut paths = BTreeMap::new();
        for (key, names) in &self.inputs {
            paths.insert(key.clone(), self.paths(workspace, names));
        }
        if !paths.contains_key("test-rust") {
            paths.insert(
                "test-rust".into(),
                self.paths(workspace, &workspace.dependencies(packages)),
            );
        }
        println!(
            "inputs={}",
            serde_json::to_string(&paths).expect("CI path serialization")
        );
    }

    fn paths(&self, workspace: &Workspace, names: &BTreeSet<String>) -> Vec<String> {
        if self.workspace {
            return vec![
                "crates/**".into(),
                "patches/**".into(),
                "examples/**".into(),
            ];
        }
        let mut paths = BTreeSet::from(["crates/build_*.rs".to_owned()]);
        for package in workspace.packages.iter().chain(&workspace.previous) {
            if names.contains(&package.name) {
                paths.insert(format!("{}/**", package.directory.display()));
            }
        }
        paths.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Workspace {
        fn fixture() -> Self {
            Self::parse(
                Path::new("/repo"),
                concat!(
                    "leaf\0/repo/crates/feature/leaf/Cargo.toml\0consumer\0",
                    "consumer\0/repo/crates/feature/consumer/Cargo.toml\0leaf\0",
                    "app\0/repo/crates/app/app/Cargo.toml\0consumer\0",
                    "unrelated\0/repo/crates/feature/leaf-extra/Cargo.toml\0\0",
                    "vmux_desktop\0/repo/crates/app/vmux_desktop/Cargo.toml\0app\0",
                ),
            )
            .unwrap()
        }
    }

    #[test]
    fn policy_ledger_inputs_do_not_expand_the_production_dependency_graph() {
        let mut workspace = Workspace::fixture();
        workspace.packages.push(Package {
            name: "vmux_dx".into(),
            directory: "crates/feature/vmux_dx".into(),
            dependencies: BTreeSet::new(),
        });
        for package in &mut workspace.packages {
            if package.name == "unrelated" {
                package.dependencies.insert("vmux_dx".into());
            }
        }
        for path in [
            "ecs-policy.ron",
            "crates/ecs-policy.ron",
            "crates/feature/leaf/ecs-policy.ron",
            "crates/feature/vmux_dx/ecs-policy.ron",
            "crates/util/vmux_macro/ecs-policy.ron",
            "crates/app/vmux_desktop/ecs-policy.ron",
            "patches/bevy_window-0.19.0/ecs-policy.ron",
            "examples/framework-consumer/ecs-policy.ron",
            "website/ecs-policy.ron",
            "scripts/ecs-policy.ron",
        ] {
            let policy = workspace.select(&[path.into()]).unwrap();
            assert_eq!(policy.lint, BTreeSet::from(["vmux_dx".into()]), "{path}");
            assert!(!policy.workspace, "{path}");
            assert!(!policy.lean, "{path}");
        }
        for path in [
            "other/ecs-policy.ron",
            "crates/feature/leaf/src/ecs-policy.ron",
            "website/nested/ecs-policy.ron",
        ] {
            assert!(!Scope::policy(Path::new(path)), "{path}");
        }
        let policy = workspace.select(&["ecs-policy.ron".into()]).unwrap();
        assert_eq!(policy.lint, BTreeSet::from(["vmux_dx".into()]));
        assert!(policy.packages.is_empty());
        assert!(!policy.workspace);
        assert!(!policy.lean);
        let feature = workspace
            .select(&[
                "ecs-policy.ron".into(),
                "crates/feature/leaf/src/lib.rs".into(),
            ])
            .unwrap();
        assert_eq!(feature.packages, BTreeSet::from(["leaf".into()]));
        let mut lint = feature.packages.clone();
        lint.insert("vmux_dx".into());
        assert_eq!(feature.lint, lint);
        assert!(!feature.workspace);
        assert!(!feature.lean);
        for path in ["crates/util/vmux_macro/src/lib.rs", "other-policy.ron"] {
            assert!(
                workspace
                    .select(&["ecs-policy.ron".into(), path.into()])
                    .is_err(),
                "{path}"
            );
        }
        assert!(
            Workspace::fixture()
                .select(&["ecs-policy.ron".into()])
                .is_err()
        );
        for package in &mut workspace.packages {
            if package.name == "vmux_dx" {
                package.directory = "crates/feature/foreign-policy".into();
            }
        }
        assert!(workspace.select(&["ecs-policy.ron".into()]).is_err());
    }

    #[test]
    fn independent_website_inputs_run_policy_without_rebuilding_product_consumers() {
        let mut workspace = Workspace::fixture();
        workspace.packages.push(Package {
            name: "vmux_dx".into(),
            directory: "crates/feature/vmux_dx".into(),
            dependencies: BTreeSet::new(),
        });
        for path in [
            "website/src/app.rs",
            "website/build.rs",
            "website/Cargo.toml",
            "website/Cargo.lock",
        ] {
            let selected = workspace
                .select(&[path.into(), "website/ecs-policy.ron".into()])
                .unwrap();
            assert_eq!(selected.lint, BTreeSet::from(["vmux_dx".into()]), "{path}");
            assert!(!selected.workspace, "{path}");
            assert!(!selected.lean, "{path}");
        }
        assert!(
            workspace
                .select(&["website/public/updates.json".into()])
                .unwrap()
                .packages
                .is_empty()
        );
        workspace.packages.push(Package {
            name: "site".into(),
            directory: "website".into(),
            dependencies: BTreeSet::new(),
        });
        let selected = workspace.select(&["website/src/app.rs".into()]).unwrap();
        assert_eq!(selected.packages, BTreeSet::from(["site".into()]));
        assert_eq!(
            workspace
                .select(&["website/build.rs".into()])
                .unwrap()
                .packages,
            BTreeSet::from(["site".into()])
        );
    }

    #[test]
    fn feature_changes_select_the_owning_target_without_downstream_test_targets() {
        let selection = Workspace::fixture()
            .select(&["crates/feature/leaf/src/lib.rs".into()])
            .unwrap();
        assert_eq!(selection.packages, BTreeSet::from(["leaf".into()]));
        assert!(!selection.workspace);
        assert!(!selection.lean);
    }

    #[test]
    fn package_prefixes_do_not_capture_a_sibling_with_a_similar_name() {
        let selection = Workspace::fixture()
            .select(&[
                "crates/feature/leaf-extra/src/lib.rs".into(),
                "docs/ux/terminal.md".into(),
            ])
            .unwrap();
        assert_eq!(selection.packages, BTreeSet::from(["unrelated".into()]));
        assert!(!selection.workspace);
    }

    #[test]
    fn member_manifests_build_scripts_and_core_sources_select_only_their_own_target() {
        let mut workspace = Workspace::fixture();
        for package in &mut workspace.packages {
            if package.name == "leaf" {
                package.directory = "crates/util/vmux_ecs".into();
            }
        }
        for path in [
            "crates/util/vmux_ecs/src/lib.rs",
            "crates/util/vmux_ecs/Cargo.toml",
            "crates/util/vmux_ecs/build.rs",
        ] {
            let selection = workspace.select(&[path.into()]).unwrap();
            assert!(!selection.workspace, "{path}");
            assert_eq!(
                selection.packages,
                BTreeSet::from(["leaf".into()]),
                "{path}"
            );
            assert!(!selection.lint.contains("unrelated"), "{path}");
        }
    }

    #[test]
    fn removed_dependency_edges_remain_inputs_without_selecting_consumer_targets() {
        let mut workspace = Workspace::fixture();
        for package in &mut workspace.packages {
            if package.name == "consumer" {
                package.dependencies.clear();
            }
        }
        let inherited: toml::Value = toml::from_str(
            "[workspace.dependencies]\nrenamed = { package = 'leaf', version = '1' }",
        )
        .unwrap();
        workspace.previous.push(Package::previous(Path::new("crates/feature/consumer/Cargo.toml"), "[package]\nname = 'consumer'\n[dependencies]\nrenamed.workspace = true\n[target.'cfg(unix)'.build-dependencies]\nbuild_alias = { package = 'leaf', version = '1' }\n[dev-dependencies]\ndev_alias = { package = 'unrelated', version = '1' }", &inherited).unwrap());
        let selection = workspace
            .select(&["crates/feature/leaf/src/lib.rs".into()])
            .unwrap();
        assert_eq!(selection.packages, BTreeSet::from(["leaf".into()]));
        assert_eq!(
            workspace.dependencies(BTreeSet::from(["consumer".into()])),
            BTreeSet::from(["consumer".into(), "leaf".into(), "unrelated".into()])
        );
    }

    #[test]
    fn renamed_and_deleted_packages_never_become_invalid_cargo_arguments() {
        let mut workspace = Workspace::fixture();
        let old = workspace.packages.remove(0);
        workspace.previous.push(old);
        workspace.packages.push(Package {
            name: "renamed".into(),
            directory: "crates/feature/leaf".into(),
            dependencies: BTreeSet::new(),
        });
        let files = ["crates/feature/leaf/Cargo.toml".into()];
        assert_eq!(
            workspace.select(&files).unwrap().packages,
            BTreeSet::from(["renamed".into()])
        );
        workspace.packages.pop();
        let selection = workspace.select(&files).unwrap();
        assert!(!selection.workspace);
        assert!(selection.packages.is_empty());
        assert!(!selection.lint.contains("leaf"));
        let mut workspace = Workspace::fixture();
        let desktop = workspace.packages.pop().unwrap();
        workspace.previous.push(desktop);
        let selection = workspace
            .select(&["crates/app/vmux_desktop/Cargo.toml".into()])
            .unwrap();
        assert!(selection.packages.is_empty());
        assert!(!selection.lean);
    }

    impl Scope {
        fn fixture() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let repository = Self {
                root: temporary.path().canonicalize().unwrap(),
                _temporary: Some(temporary),
            };
            repository.write("Cargo.toml", "[workspace]\nmembers = ['crates/app/*', 'crates/feature/*', 'crates/util/*']\nresolver = '3'");
            for (name, directory) in [
                ("portable", "util"),
                ("renderer", "util"),
                ("unrelated", "util"),
                ("vmux_mcp", "util"),
                ("vmux_project", "feature"),
                ("vmux_extension", "feature"),
                ("vmux_layout", "feature"),
                ("vmux_flex", "util"),
            ] {
                repository.package(name, directory, &[], &[]);
            }
            repository.package(
                "vmux_ecs",
                "util",
                &[("store", &[]), ("records", &[])],
                &[("portable", "../portable", "")],
            );
            repository.package(
                "vmux_app",
                "util",
                &[
                    ("contracts", &["dep:vmux_ecs"]),
                    ("host", &["dep:portable"]),
                    ("daemon", &["dep:vmux_session"]),
                    ("mcp", &["dep:vmux_mcp"]),
                    ("desktop", &["dep:renderer"]),
                ],
                &[
                    ("vmux_ecs", "../vmux_ecs", "optional = true"),
                    ("portable", "../portable", "optional = true"),
                    ("renderer", "../renderer", "optional = true"),
                    (
                        "vmux_session",
                        "../../feature/vmux_session",
                        "optional = true, default-features = false",
                    ),
                    ("vmux_mcp", "../vmux_mcp", "optional = true"),
                ],
            );
            repository.package(
                "vmux_session",
                "feature",
                &[("default", &["dep:renderer"])],
                &[
                    ("vmux_ecs", "../../util/vmux_ecs", ""),
                    ("renderer", "../../util/renderer", "optional = true"),
                ],
            );
            repository.package(
                "vmux_desktop",
                "app",
                &[],
                &[
                    ("vmux_app", "../../util/vmux_app", "features = ['desktop']"),
                    ("vmux_ecs", "../../util/vmux_ecs", ""),
                ],
            );
            repository.package(
                "vmux_dx",
                "feature",
                &[],
                &[("vmux_ecs", "../../util/vmux_ecs", "")],
            );
            for (consumer, feature) in [
                ("framework-consumer", "contracts"),
                ("host-consumer", "host"),
            ] {
                repository.write(&format!("examples/{consumer}/Cargo.toml"), &format!("[workspace]\n[package]\nname = '{consumer}'\nversion = '1.0.0'\nedition = '2024'\n[dependencies]\nvmux_app = {{ path = '../../crates/util/vmux_app', features = ['{feature}'] }}"));
                repository.write(&format!("examples/{consumer}/src/lib.rs"), "");
            }
            for manifest in [
                "Cargo.toml",
                "examples/framework-consumer/Cargo.toml",
                "examples/host-consumer/Cargo.toml",
            ] {
                let status = Command::new("cargo")
                    .args([
                        "generate-lockfile",
                        "--offline",
                        "--manifest-path",
                        manifest,
                    ])
                    .current_dir(&repository.root)
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            repository
        }

        fn package(
            &self,
            name: &str,
            directory: &str,
            features: &[(&str, &[&str])],
            dependencies: &[(&str, &str, &str)],
        ) {
            let path = format!("crates/{directory}/{name}");
            let mut source =
                format!("[package]\nname = '{name}'\nversion = '1.0.0'\nedition = '2024'\n");
            if !features.is_empty() {
                source.push_str("[features]\n");
                for (feature, values) in features {
                    source.push_str(&format!(
                        "{feature} = {}\n",
                        serde_json::to_string(values).unwrap()
                    ));
                }
            }
            if !dependencies.is_empty() {
                source.push_str("[dependencies]\n");
                for (dependency, target, options) in dependencies {
                    source.push_str(&format!(
                        "{dependency} = {{ path = '{target}', {options} }}\n"
                    ));
                }
            }
            self.write(&format!("{path}/Cargo.toml"), &source);
            self.write(&format!("{path}/src/lib.rs"), "pub fn value() -> u32 { 1 }");
        }

        fn write(&self, path: &str, source: &str) {
            let path = self.root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }

        fn git(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .args(args)
                .current_dir(&self.root)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        }

        fn snapshot(&self) -> String {
            self.git(&["add", "."]);
            self.git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "Fixture",
            ]);
            self.git(&["rev-parse", "HEAD"])
        }
    }

    #[test]
    fn git_manifest_history_retains_removed_inputs_without_reviving_old_test_targets() {
        let repository = Scope::fixture();
        let root = repository.root.clone();
        repository.write("crates/feature/vmux_layout/Cargo.toml", "[package]\nname = 'vmux_layout'\nversion = '1.0.0'\nedition = '2024'\n[dependencies]\nportable = { path = '../../util/portable' }");
        repository.git(&["init"]);
        let base = repository.snapshot();
        repository.write(
            "crates/feature/vmux_layout/Cargo.toml",
            "[package]\nname = 'renamed-layout'\nversion = '1.0.0'\nedition = '2024'",
        );
        repository.snapshot();
        let comparison = Comparison::parse(&["--base".into(), base.into()]).unwrap();
        let (base, files) = comparison.files(&root).unwrap().unwrap();
        assert_eq!(
            files,
            vec![PathBuf::from("crates/feature/vmux_layout/Cargo.toml")]
        );
        let mut workspace = Workspace::load(&root).unwrap();
        workspace.previous(&root, &base, &files).unwrap();
        let selection = workspace.select(&files).unwrap();
        assert_eq!(
            selection.packages,
            BTreeSet::from(["renamed-layout".into()])
        );
        assert_eq!(
            workspace.dependencies(selection.packages),
            BTreeSet::from([
                "renamed-layout".into(),
                "vmux_layout".into(),
                "portable".into()
            ])
        );
        repository.git(&["rm", "-r", "crates/feature/vmux_layout"]);
        repository.snapshot();
        let (base, files) = comparison.files(&root).unwrap().unwrap();
        let mut workspace = Workspace::load(&root).unwrap();
        workspace.previous(&root, &base, &files).unwrap();
        let selection = workspace.select(&files).unwrap();
        assert!(!selection.workspace);
        assert!(selection.packages.is_empty());
        assert!(selection.lint.is_empty());
    }

    #[test]
    fn special_suites_use_direct_roots_and_feature_graphs_only_for_reuse_inputs() {
        let repository = Scope::fixture();
        let root = repository.root.clone();
        let workspace = Workspace::load(&root).unwrap();
        for (file, expected) in [
            (
                "crates/util/unrelated/src/lib.rs",
                [false, false, false, false, false, false],
            ),
            (
                "crates/util/renderer/src/lib.rs",
                [false, false, false, false, false, false],
            ),
            (
                "crates/util/vmux_mcp/src/lib.rs",
                [false, false, false, false, false, false],
            ),
            (
                "crates/util/vmux_app/src/lib.rs",
                [true, true, false, false, false, false],
            ),
            (
                "crates/util/vmux_ecs/src/lib.rs",
                [false, false, false, true, false, false],
            ),
            (
                "crates/app/vmux_desktop/src/lib.rs",
                [false, false, true, false, false, false],
            ),
            (
                "crates/feature/vmux_session/src/lib.rs",
                [false, false, false, false, true, false],
            ),
        ] {
            let mut selection = workspace.select(&[file.into()]).unwrap();
            workspace.variants(&root, &mut selection).unwrap();
            assert_eq!(
                [
                    selection.sdk,
                    selection.headless,
                    selection.lean,
                    selection.records,
                    selection.render_free,
                    selection.tooling
                ],
                expected,
                "{file}"
            );
            if file == "crates/util/vmux_app/src/lib.rs" {
                assert_eq!(
                    selection.inputs["test-features"],
                    BTreeSet::from([
                        "vmux_app".into(),
                        "vmux_mcp".into(),
                        "vmux_session".into(),
                        "vmux_ecs".into(),
                        "portable".into()
                    ])
                );
            }
            if file == "crates/util/unrelated/src/lib.rs" {
                assert_eq!(
                    selection.inputs["test-rust"],
                    BTreeSet::from(["unrelated".into()])
                );
            }
        }
        let mut selection = workspace
            .select(&["examples/framework-consumer/src/lib.rs".into()])
            .unwrap();
        workspace.variants(&root, &mut selection).unwrap();
        assert!(selection.sdk);
        assert!(!selection.headless);
        assert!(selection.packages.is_empty());
    }

    #[test]
    fn pr_530_dx_cli_and_ecs_cli_changes_do_not_select_product_consumers_or_variants() {
        let workspace = Workspace::parse(
            Path::new("/repo"),
            concat!(
                "vmux_cli\0/repo/crates/app/vmux_cli/Cargo.toml\0vmux_dx,vmux_ecs\0",
                "vmux_dx\0/repo/crates/feature/vmux_dx/Cargo.toml\0vmux_ecs\0",
                "vmux_ecs\0/repo/crates/util/vmux_ecs/Cargo.toml\0\0",
                "vmux_desktop\0/repo/crates/app/vmux_desktop/Cargo.toml\0vmux_cli,vmux_ecs\0",
                "vmux_app\0/repo/crates/util/vmux_app/Cargo.toml\0vmux_ecs\0",
                "unrelated\0/repo/crates/feature/unrelated/Cargo.toml\0vmux_ecs\0",
            ),
        )
        .unwrap();
        let selection = workspace
            .select(&[
                "crates/app/vmux_cli/ecs-policy.ron".into(),
                "crates/app/vmux_cli/src/plugin.rs".into(),
                "crates/app/vmux_cli/tests/dx.rs".into(),
                "crates/feature/vmux_dx/Cargo.toml".into(),
                "crates/feature/vmux_dx/src/cargo.rs".into(),
                "crates/feature/vmux_dx/src/debug.rs".into(),
                "crates/util/vmux_ecs/src/cli.rs".into(),
                "docs/dx/architecture.md".into(),
            ])
            .unwrap();
        assert_eq!(
            selection.lint,
            BTreeSet::from(["vmux_cli".into(), "vmux_dx".into(), "vmux_ecs".into()])
        );
        assert_eq!(
            selection.packages,
            BTreeSet::from(["vmux_cli".into(), "vmux_ecs".into()])
        );
        assert!(selection.tooling);
        assert!(!selection.workspace);
        assert!(!selection.sdk);
        assert!(!selection.headless);
        assert!(!selection.lean);
        assert!(!selection.records);
        assert!(!selection.render_free);
    }

    #[test]
    fn explicit_global_inputs_require_full_validation() {
        for path in [
            "Cargo.lock",
            "tool-versions.env",
            "Cargo.toml",
            ".cargo/config.toml",
            "rust-toolchain.toml",
            "crates/build_platform_cfg.rs",
            ".github/workflows/ci.yml",
        ] {
            let selection = Workspace::fixture().select(&[path.into()]).unwrap();
            assert!(selection.workspace, "{path}");
            assert!(selection.lean, "{path}");
        }
    }

    #[test]
    fn unknown_owners_report_the_path_without_expanding_pr_validation() {
        let workspace = Workspace::fixture();
        for path in [
            "crates/feature/removed/src/lib.rs",
            "patches/unknown/src/lib.rs",
            "other-policy.ron",
        ] {
            let error = workspace.select(&[path.into()]).err().unwrap().to_string();
            assert_eq!(error, format!("CI input has no package owner: {path}"));
        }
        let docs = workspace.select(&["docs/ux/terminal.md".into()]).unwrap();
        assert!(!docs.workspace);
        assert!(docs.packages.is_empty());
        assert!(docs.lint.is_empty());
    }

    #[test]
    fn desktop_test_targets_are_partitioned_from_other_changed_packages() {
        let workspace = Workspace::fixture();
        for (files, expected) in [
            (vec!["crates/app/vmux_desktop/src/lib.rs"], ""),
            (vec!["crates/feature/leaf/src/lib.rs"], "-p leaf"),
            (
                vec![
                    "crates/app/vmux_desktop/src/lib.rs",
                    "crates/feature/leaf/src/lib.rs",
                ],
                "-p leaf",
            ),
        ] {
            let mut paths = Vec::new();
            for file in files {
                paths.push(PathBuf::from(file));
            }
            assert_eq!(workspace.select(&paths).unwrap().arguments(), expected);
        }
    }

    #[test]
    fn desktop_changes_keep_lean_feature_tests() {
        let selection = Workspace::fixture()
            .select(&["crates/app/vmux_desktop/src/lib.rs".into()])
            .unwrap();
        assert_eq!(selection.packages, BTreeSet::from(["vmux_desktop".into()]));
        assert!(selection.lean);
    }

    #[test]
    fn vendored_consumers_are_tested_without_formatting_or_linting_them() {
        let workspace = Workspace::parse(
            Path::new("/repo"),
            concat!(
                "owned\0/repo/crates/feature/owned/Cargo.toml\0\0",
                "vendor\0/repo/patches/vendor/Cargo.toml\0owned\0",
            ),
        )
        .unwrap();
        let selection = workspace
            .select(&["crates/feature/owned/src/lib.rs".into()])
            .unwrap();
        assert_eq!(selection.packages, BTreeSet::from(["owned".into()]));
        assert_eq!(selection.lint, BTreeSet::from(["owned".into()]));
        assert_eq!(workspace.full().lint, BTreeSet::from(["owned".into()]));
    }
}
