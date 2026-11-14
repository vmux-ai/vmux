use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::Native;
use crate::Result;
use crate::execution::Execution;
use crate::temporary::Temporary;

impl Native {
    pub fn build_cef(&mut self) -> Result<()> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err("CEF sources require macOS ARM64".into());
        }
        self.text("git-lfs", &[OsStr::new("--version")])?;
        let build = PathBuf::from(self.get(
            "CEF_BUILD_DIR",
            PathBuf::from(self.get("RUNNER_TEMP", std::env::temp_dir())).join("vmux-cef-build"),
        ));
        let dist = PathBuf::from(self.get("CEF_DIST_DIR", self.checkout.root.join("cef/dist")));
        if build == self.checkout.root || self.checkout.root.starts_with(&build) {
            return Err("CEF build directory overlaps repository sources".into());
        }
        fs::create_dir_all(&build)?;
        fs::create_dir_all(&dist)?;
        let capacity = self.text("df", &[OsStr::new("-Pk"), build.as_os_str()])?;
        let available: u64 = capacity
            .lines()
            .nth(1)
            .and_then(|line| line.split_whitespace().nth(3))
            .ok_or("Missing CEF filesystem capacity")?
            .parse()?;
        let usage = self.text("du", &[OsStr::new("-sk"), build.as_os_str()])?;
        let existing: u64 = usage
            .split_whitespace()
            .next()
            .ok_or("Missing CEF source usage")?
            .parse()?;
        if available + existing < 190 * 1024 * 1024 {
            return Err(
                "CEF sources require at least 190 GiB of available and existing build storage"
                    .into(),
            );
        }
        let cef_commit = self.cef_value("cef_commit")?;
        let chromium_version = self.cef_value("chromium_version")?;
        let depot_commit = self.cef_value("depot_tools_commit")?;
        let expected = self.cef_value("artifact_sha256")?;
        if expected.is_empty()
            && self.environment.get("CEF_ALLOW_UNPINNED") != Some(OsStr::new("1"))
        {
            return Err("CEF sources require a published archive checksum".into());
        }
        if !expected.is_empty()
            && (expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err("Invalid CEF archive checksum".into());
        }
        let automate = build.join("automate-git.py");
        let depot = build.join("depot_tools");
        self.download(&format!("https://raw.githubusercontent.com/chromiumembedded/cef/{cef_commit}/tools/automate/automate-git.py"), &automate)?;
        self.environment.set("GIT_LFS_SKIP_SMUDGE", "1");
        if !depot.join(".git").is_dir() {
            self.run_command(
                "git",
                &[
                    OsStr::new("clone"),
                    OsStr::new("https://chromium.googlesource.com/chromium/tools/depot_tools.git"),
                    depot.as_os_str(),
                ],
            )?;
        }
        self.git_at(&depot, &["fetch", "origin", &depot_commit])?;
        self.git_at(&depot, &["checkout", "--force", &depot_commit])?;
        let manifest = depot.join(if depot.join(".bleeding_edge").exists() {
            "bootstrap/manifest_bleeding_edge.txt"
        } else {
            "bootstrap/manifest.txt"
        });
        let manifest_text = fs::read_to_string(&manifest)?;
        let mut python_version = None;
        for line in manifest_text.lines() {
            if !line.starts_with('#')
                && line.contains("cpython3/")
                && let Some((_, version)) = line.split_once("version:")
            {
                python_version = Some(version.split_whitespace().collect::<String>());
            }
        }
        let bootstrap = format!(
            "bootstrap-{}_bin",
            python_version.ok_or("CIPD manifest has no Python version")?
        );
        let mut cipd = self.environment.command(depot.join("cipd"), &depot);
        cipd.args(["ensure", "-log-level", "warning", "-ensure-file"])
            .arg(&manifest)
            .arg("-root")
            .arg(&bootstrap);
        Execution::new(cipd).run()?;
        let mut bootstrap_command = self
            .environment
            .command(depot.join(&bootstrap).join("python3/bin/python3"), &depot);
        bootstrap_command
            .arg(depot.join("bootstrap/bootstrap.py"))
            .arg("--bootstrap-name")
            .arg(&bootstrap);
        Execution::new(bootstrap_command).run()?;
        let python = depot.join(fs::read_to_string(depot.join("python3_bin_reldir.txt"))?.trim());
        let old_path = self.environment.required("PATH")?.to_owned();
        self.environment.set(
            "PATH",
            format!(
                "{}:{}:{}",
                python.display(),
                depot.display(),
                old_path.to_string_lossy()
            ),
        );
        self.environment.set("PYTHONPATH", depot.as_os_str());
        self.environment.set(
            "VPYTHON_BYPASS",
            "manually managed python not supported by chrome operations",
        );
        self.environment.set("DEPOT_TOOLS_UPDATE", "0");
        let source = build.join("chromium/src");
        let cef = source.join("cef");
        let mut common = vec![
            automate.as_os_str().to_owned(),
            format!("--download-dir={}", build.display()).into(),
            format!("--depot-tools-dir={}", depot.display()).into(),
            "--no-depot-tools-update".into(),
            format!("--checkout={cef_commit}").into(),
            format!("--chromium-checkout=refs/tags/{chromium_version}").into(),
            "--arm64-build".into(),
            "--no-chromium-history".into(),
            "--with-pgo-profiles".into(),
        ];
        if self.environment.get("CEF_RESUME") == Some(OsStr::new("1")) {
            if !source.join(".git").is_dir() || !cef.is_dir() {
                return Err("CEF_RESUME requires initialized Chromium and CEF sources".into());
            }
        } else {
            let mut args = common.clone();
            args.extend(
                [
                    "--force-config",
                    "--force-clean",
                    "--no-build",
                    "--no-distrib",
                ]
                .map(Into::into),
            );
            self.tool(OsStr::new("python3"), &args)?;
        }
        let sync_marker = build.join(format!(".gclient-{chromium_version}"));
        let cef_patch = self
            .checkout
            .root
            .join("cef/patches/cef-safe-storage.patch");
        let chromium_patch = self
            .checkout
            .root
            .join("cef/patches/chromium-safe-storage.patch");
        if !sync_marker.is_file() {
            self.patch(&cef, &cef_patch, true)?;
            self.patch(&source, &chromium_patch, true)?;
            self.git_at(
                &source,
                &[
                    "config",
                    "--replace-all",
                    "remote.origin.fetch",
                    &format!("+refs/tags/{chromium_version}:refs/tags/{chromium_version}"),
                ],
            )?;
            let mut command = self.environment.command("gclient", &build.join("chromium"));
            command
                .args([
                    "sync",
                    "--nohooks",
                    "--no-history",
                    "--shallow",
                    "--revision",
                ])
                .arg(format!("src@refs/tags/{chromium_version}"));
            Execution::new(command).run()?;
            fs::write(sync_marker, "")?;
        }
        let hooks_marker = build.join(format!(".gclient-hooks-{chromium_version}"));
        if !hooks_marker.is_file() {
            let mut command = self.environment.command("gclient", &build.join("chromium"));
            command.arg("runhooks");
            Execution::new(command).run()?;
            fs::write(hooks_marker, "")?;
        }
        self.tool(
            OsStr::new("python3"),
            &[
                source.join("build/util/lastchange.py").into_os_string(),
                "-o".into(),
                source.join("build/util/LASTCHANGE").into_os_string(),
            ],
        )?;
        self.tool(
            OsStr::new("python3"),
            &[
                source.join("tools/update_pgo_profiles.py").into_os_string(),
                "--target".into(),
                "mac-arm".into(),
                "update".into(),
                "--gs-url-base=chromium-optimization-profiles/pgo_profiles".into(),
            ],
        )?;
        self.tool(
            OsStr::new("python3"),
            &[
                source
                    .join("v8/tools/builtins-pgo/download_profiles.py")
                    .into_os_string(),
                "download".into(),
                "--depot-tools".into(),
                depot.clone().into_os_string(),
                "--check-v8-revision".into(),
            ],
        )?;
        let compatibility = fs::read_to_string(cef.join("CHROMIUM_BUILD_COMPATIBILITY.txt"))?;
        if !compatibility.lines().any(|line| {
            line.contains("chromium_checkout")
                && line.contains(&format!("refs/tags/{chromium_version}"))
        }) {
            return Err("CEF and Chromium source revisions disagree".into());
        }
        self.patch(&cef, &cef_patch, false)?;
        self.patch(&source, &chromium_patch, false)?;
        self.git_at(
            &cef,
            &[
                "diff",
                "--check",
                "--",
                "BUILD.gn",
                "libcef/common/base_impl.cc",
            ],
        )?;
        self.git_at(
            &source,
            &[
                "diff",
                "--check",
                "--",
                "chrome/browser/browser_process_impl.cc",
                "components/os_crypt/async/browser/keychain_key_provider.h",
                "components/os_crypt/async/browser/keychain_key_provider.mm",
            ],
        )?;
        self.environment.set("GN_DEFINES", "is_official_build=true");
        self.environment.set("CEF_ARCHIVE_FORMAT", "tar.bz2");
        common.extend(
            [
                "--no-update",
                "--force-build",
                "--force-distrib",
                "--minimal-distrib-only",
                "--no-debug-build",
                "--build-target=cefclient",
            ]
            .map(Into::into),
        );
        self.tool(OsStr::new("python3"), &common)?;
        let mut archives = Vec::new();
        for entry in fs::read_dir(cef.join("binary_distrib"))? {
            let path = entry?.path();
            let name = path.file_name().unwrap().to_string_lossy();
            if name.starts_with("cef_binary_") && name.ends_with("_macosarm64_minimal.tar.bz2") {
                archives.push(path);
            }
        }
        archives.sort();
        let archive = archives
            .last()
            .ok_or("CEF produced no minimal distribution")?;
        let extracted = Temporary::create(&std::env::temp_dir(), "vmux-cef-distribution")?;
        self.run_command(
            "tar",
            &[
                OsStr::new("-xjf"),
                archive.as_os_str(),
                OsStr::new("-C"),
                extracted.path().as_os_str(),
            ],
        )?;
        let mut framework = None;
        for entry in fs::read_dir(extracted.path())? {
            let candidate = entry?
                .path()
                .join("Release/Chromium Embedded Framework.framework");
            if candidate.is_dir() {
                framework = Some(candidate);
                break;
            }
        }
        let framework = framework.ok_or("CEF distribution has no release framework")?;
        self.validate_cef(&framework)?;
        self.environment.set("PATH", old_path);
        let name = self.cef_value("artifact_name")?;
        let artifact = dist.join(&name);
        let mut command = self.command("/usr/bin/python3");
        command
            .arg("-c")
            .arg(Archive::SOURCE)
            .arg(&framework)
            .arg(&artifact);
        Execution::new(command).run()?;
        let checksum = self.checksum(&artifact)?;
        if !expected.is_empty() && checksum != expected {
            return Err("CEF archive checksum differs from the pinned artifact".into());
        }
        fs::write(
            artifact.with_extension("gz.sha256"),
            format!("{checksum}  {name}\n"),
        )?;
        let metadata = serde_json::json!({
            "cef_version": self.cef_value("cef_version")?,
            "cef_commit": cef_commit,
            "chromium_version": chromium_version,
            "depot_tools_commit": depot_commit,
            "patch_revision": self.cef_value("patch_revision")?.parse::<u64>()?,
            "artifact_sha256": checksum,
            "repository_commit": self.text("git", &[OsStr::new("rev-parse"), OsStr::new("HEAD")])?.trim(),
            "xcode": self.text("xcodebuild", &[OsStr::new("-version")])?.replace('\n', " "),
        });
        fs::write(
            artifact.with_extension("gz.metadata.json"),
            serde_json::to_vec_pretty(&metadata)?,
        )?;
        if let Some(path) = self.environment.get("GITHUB_OUTPUT") {
            let mut output = fs::OpenOptions::new().append(true).open(path)?;
            writeln!(
                output,
                "artifact={}\nchecksum={}\nchecksum_file={}\nmetadata={}",
                artifact.display(),
                checksum,
                artifact.with_extension("gz.sha256").display(),
                artifact.with_extension("gz.metadata.json").display()
            )?;
        }
        Ok(())
    }

    fn git_at(&self, root: &Path, args: &[&str]) -> Result<()> {
        let mut command = self.environment.command("git", root);
        command.args(args);
        Execution::new(command).run()
    }

    fn patch(&self, root: &Path, path: &Path, reverse: bool) -> Result<()> {
        let mut command = self.environment.command("git", root);
        command.args(["apply", "--reverse", "--check"]).arg(path);
        let applied = Execution::new(command).output_any(None)?.status.success();
        if reverse && !applied || !reverse && applied {
            return Ok(());
        }
        let mut command = self.environment.command("git", root);
        command.arg("apply");
        if reverse {
            command.arg("--reverse");
        }
        command.arg(path);
        Execution::new(command).run()
    }

    pub fn cef_manifest(&self, key: &str) -> Result<()> {
        println!("{}", self.cef_value(key)?);
        Ok(())
    }
}

struct Archive;

impl Archive {
    const SOURCE: &str = r#"import gzip, pathlib, sys, tarfile
source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
root = source.parent
paths = [source] + sorted(source.rglob("*"), key=lambda path: path.relative_to(root).as_posix())
with destination.open("wb") as output:
    with gzip.GzipFile(filename="", mode="wb", fileobj=output, compresslevel=9, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for path in paths:
                info = archive.gettarinfo(str(path), path.relative_to(root).as_posix())
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ""
                info.mode = 0o755 if info.isdir() else 0o777 if info.issym() else 0o755 if info.mode & 0o111 else 0o644
                if info.isfile():
                    with path.open("rb") as file:
                        archive.addfile(info, file)
                else:
                    archive.addfile(info)
"#;
}
