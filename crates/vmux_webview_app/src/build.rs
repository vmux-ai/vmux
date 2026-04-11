use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::time::SystemTime;

use regex::Regex;

pub const CEF_EMBEDDED_APP_INDEX_CSS: &str = "index.css";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CefMode {
    Browser,
    WebviewApp,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CefEmbeddedWebviewFinalize {
    pub strip_uncompiled_tailwind_css: bool,
}

#[derive(Debug)]
pub struct WebviewAppBuilder {
    pub manifest_dir: PathBuf,
    pub dx_package: &'static str,
    pub dx_bin: &'static str,
    pub dx_extra_args: &'static [&'static str],
    pub cef_finalize: CefEmbeddedWebviewFinalize,
    pub extra_tracked: Vec<PathBuf>,
    pub tailwind_postprocess_stale_prefixes: Option<&'static [&'static str]>,
}

impl WebviewAppBuilder {
    pub fn new(manifest_dir: PathBuf, dx_package: &'static str, dx_bin: &'static str) -> Self {
        Self {
            manifest_dir,
            dx_package,
            dx_bin,
            dx_extra_args: &[],
            cef_finalize: CefEmbeddedWebviewFinalize::default(),
            extra_tracked: Vec::new(),
            tailwind_postprocess_stale_prefixes: None,
        }
    }

    pub fn dx_extra_args(mut self, args: &'static [&'static str]) -> Self {
        self.dx_extra_args = args;
        self
    }

    pub fn cef_finalize(mut self, v: CefEmbeddedWebviewFinalize) -> Self {
        self.cef_finalize = v;
        self
    }

    pub fn track_manifest_rel_paths(mut self, rel: &[&str]) -> Self {
        for r in rel {
            self.extra_tracked.push(self.manifest_dir.join(r));
        }
        self
    }

    pub fn tailwind_postprocess_after_dx(mut self, stale_hashed_css_prefixes: &'static [&'static str]) -> Self {
        self.tailwind_postprocess_stale_prefixes = Some(stale_hashed_css_prefixes);
        self
    }

    pub fn run(self, warning_prefix: &'static str) {
        self.run_inner(warning_prefix);
    }

    fn run_inner(&self, warning_prefix: &'static str) {
        let workspace_root = workspace_root_from_manifest_dir(&self.manifest_dir);
        for p in self.tracked_paths() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
        println!("cargo:rerun-if-changed=build.rs");

        if std::env::var("TARGET")
            .unwrap_or_default()
            .contains("wasm32")
        {
            return;
        }

        let release = std::env::var("PROFILE").unwrap_or_default() == "release";
        let dist = self.manifest_dir.join("dist");
        let shell = self.manifest_dir.join("assets/index.html");
        if self.needs_dist_rebuild(release, &workspace_root) {
            run_dx_web_bundle(
                &workspace_root,
                self.dx_package,
                release,
                self.dx_extra_args,
            );
            let public = dx_web_public_dir(&workspace_root, self.dx_bin, release);
            copy_dx_public_to_dist(&public, &dist);
        }
        if dist.is_dir() {
            if let Err(e) = finish_cef_embedded_webview_dist(
                &dist,
                &self.manifest_dir,
                &workspace_root,
                &shell,
                self.cef_finalize,
            ) {
                println!("cargo:warning={warning_prefix}: finish CEF dist failed: {e}");
            }
        }
        if dist.is_dir() {
            if let Some(prefixes) = self.tailwind_postprocess_stale_prefixes {
                let dist_assets = dist.join("assets");
                if let Err(e) = compile_tailwind_index_css(&self.manifest_dir, &dist_assets) {
                    println!("cargo:warning={warning_prefix}: tailwind compile skipped: {e}");
                }
                if let Err(e) = remove_stale_prefixed_css_assets(&dist_assets, prefixes) {
                    println!("cargo:warning={warning_prefix}: could not remove stale css chunks: {e}");
                }
                if shell.is_file() {
                    merge_cef_shell_index(&dist, &shell, CefMode::Browser);
                }
            }
        }
        emit_dist_rerun_if_changed(&dist);
    }

    fn tracked_paths(&self) -> Vec<PathBuf> {
        let workspace_root = workspace_root_from_manifest_dir(&self.manifest_dir);
        let mut v = vec![
            self.manifest_dir.join("Cargo.toml"),
            self.manifest_dir.join("Dioxus.toml"),
            self.manifest_dir.join("assets/index.html"),
            self.manifest_dir
                .join("assets")
                .join(CEF_EMBEDDED_APP_INDEX_CSS),
            self.manifest_dir.join("../vmux_ui/assets/theme.css"),
        ];
        v.extend(self.extra_tracked.iter().cloned());
        collect_rs_files(&self.manifest_dir.join("src"), &mut v);
        collect_rs_files(&workspace_root.join("crates/vmux_ui/src"), &mut v);
        v.sort();
        v.dedup();
        v
    }

    fn dist_dependency_paths(&self) -> Vec<PathBuf> {
        let mut v = self.tracked_paths();
        v.push(self.manifest_dir.join("build.rs"));
        v
    }

    fn needs_dist_rebuild(&self, release: bool, workspace_root: &Path) -> bool {
        let dist = self.manifest_dir.join("dist");
        let index = dist.join("index.html");
        let Some(wasm_mtime) = newest_bg_wasm_mtime(&dist) else {
            return true;
        };
        if !index.is_file() {
            return true;
        }
        for p in self.dist_dependency_paths() {
            if let Ok(t) = fs::metadata(&p).and_then(|m| m.modified()) {
                if t > wasm_mtime {
                    return true;
                }
            }
        }
        let dx_public = dx_web_public_dir(workspace_root, self.dx_bin, release);
        if let Some(dx_mtime) = newest_bg_wasm_mtime(&dx_public) {
            if dx_mtime > wasm_mtime {
                return true;
            }
        }
        false
    }
}

fn emit_dist_rerun_if_changed(dist: &Path) {
    if let Ok(rd) = fs::read_dir(dist) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    }
    let wasm_dir = dist.join("wasm");
    if wasm_dir.is_dir() {
        if let Ok(rd) = fs::read_dir(&wasm_dir) {
            for e in rd.flatten() {
                println!("cargo:rerun-if-changed={}", e.path().display());
            }
        }
    }
    let assets_dir = dist.join("assets");
    if assets_dir.is_dir() {
        if let Ok(rd) = fs::read_dir(&assets_dir) {
            for e in rd.flatten() {
                println!("cargo:rerun-if-changed={}", e.path().display());
            }
        }
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

pub fn workspace_root_from_manifest_dir(manifest_dir: &Path) -> PathBuf {
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("vmux crates should live under workspace crates/")
        .to_path_buf()
}

fn dx_version_ok(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .status()
        .is_ok_and(|s| s.success())
}

pub fn resolve_dx_executable() -> PathBuf {
    if let Ok(dx) = std::env::var("DX") {
        let p = PathBuf::from(dx);
        if dx_version_ok(&p) {
            return p;
        }
    }
    if dx_version_ok(Path::new("dx")) {
        return PathBuf::from("dx");
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".cargo/bin/dx");
        if dx_version_ok(&p) {
            return p;
        }
    }
    panic!(
        "vmux: `dx` (dioxus-cli) not found. Install e.g.\n\
         cargo install dioxus-cli --locked --version 0.7.4\n\
         Or set DX=/path/to/dx\n\
         (vmux does not use npm for web bundles; optional Tailwind is a standalone `tailwindcss` binary.)"
    );
}

pub fn dx_web_public_dir(workspace_root: &Path, bin_name: &str, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    workspace_root
        .join("target")
        .join("dx")
        .join(bin_name)
        .join(profile)
        .join("web")
        .join("public")
}

pub fn run_dx_web_bundle(
    workspace_root: &Path,
    package: &str,
    release: bool,
    extra_dx_args: &[&str],
) {
    let dx = resolve_dx_executable();
    let mut cmd = Command::new(&dx);
    cmd.current_dir(workspace_root)
        .env_remove("CEF_PATH")
        .args(["build", "--platform", "web", "-p", package]);
    if release {
        cmd.arg("--release");
    }
    for a in extra_dx_args {
        cmd.arg(a);
    }
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("vmux: failed to spawn dx ({}): {e}", dx.display()));
    if !status.success() {
        panic!("vmux: `dx build --platform web -p {package}` failed (release={release})");
    }
}

pub fn copy_dx_public_to_dist(public: &Path, dist: &Path) {
    if !public.is_dir() {
        panic!(
            "vmux: expected dx web output at {} (directory missing after dx build)",
            public.display()
        );
    }
    if dist.exists() {
        fs::remove_dir_all(dist).unwrap_or_else(|e| {
            panic!("vmux: failed to remove {}: {e}", dist.display());
        });
    }
    fs::create_dir_all(dist).unwrap_or_else(|e| {
        panic!("vmux: failed to create {}: {e}", dist.display());
    });
    copy_dir_recursive(public, dist).unwrap_or_else(|e| {
        panic!("vmux: copy {} -> {}: {e}", public.display(), dist.display());
    });
}

pub fn merge_cef_shell_index(dist: &Path, shell_index: &Path, cef_mode: CefMode) {
    let dx_html = fs::read_to_string(dist.join("index.html")).unwrap_or_else(|e| {
        panic!("vmux: read dx index.html in {}: {e}", dist.display());
    });
    let entry_href = dx_module_script_href(&dx_html);
    let wasm_href = find_bg_wasm_href(dist);
    let style_links = cef_stylesheet_link_tags(dist, cef_mode);

    let shell = fs::read_to_string(shell_index).unwrap_or_else(|e| {
        panic!("vmux: read shell {}: {e}", shell_index.display());
    });
    let mut merged = shell
        .replace("__VMUX_DX_ENTRY__", &entry_href)
        .replace("__VMUX_DX_WASM__", &wasm_href);
    if merged.contains("__VMUX_DX_ENTRY__") || merged.contains("__VMUX_DX_WASM__") {
        panic!(
            "vmux: shell {} is missing __VMUX_DX_ENTRY__ / __VMUX_DX_WASM__ placeholders",
            shell_index.display()
        );
    }
    if !style_links.is_empty() {
        merged = merged.replace("</head>", &format!("  {style_links}\n</head>"));
    }
    fs::write(dist.join("index.html"), merged).unwrap_or_else(|e| {
        panic!("vmux: write merged index.html: {e}");
    });
}

pub fn replace_dist_from_dx_public(public: &Path, dist: &Path, shell_index: &Path) {
    copy_dx_public_to_dist(public, dist);
    merge_cef_shell_index(dist, shell_index, CefMode::Browser);
}

pub fn copy_shared_theme_css_to_cef_dist(dist: &Path, workspace_root: &Path) -> io::Result<()> {
    let src = workspace_root.join("crates/vmux_ui/assets/theme.css");
    if !src.is_file() {
        return Ok(());
    }
    for dest in [
        dist.join("assets/theme.css"),
        dist.join("vmux_ui/assets/theme.css"),
    ] {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dest)?;
    }
    Ok(())
}

pub fn strip_dx_uncompiled_tailwind_css_assets(dist: &Path, keep_css_names: &[&str]) {
    let assets = dist.join("assets");
    let Ok(rd) = fs::read_dir(&assets) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x == "css") {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if keep_css_names.iter().any(|k| name == *k) {
            continue;
        }
        let Ok(s) = fs::read_to_string(&p) else {
            continue;
        };
        if s.contains("@tailwind") {
            let _ = fs::remove_file(&p);
        }
    }
}

pub fn finish_cef_embedded_webview_dist(
    dist: &Path,
    manifest_dir: &Path,
    workspace_root: &Path,
    shell_index: &Path,
    opts: CefEmbeddedWebviewFinalize,
) -> io::Result<()> {
    copy_shared_theme_css_to_cef_dist(dist, workspace_root)?;
    let manifest_assets = manifest_dir.join("assets");
    let index_src = manifest_assets.join(CEF_EMBEDDED_APP_INDEX_CSS);
    if index_src.is_file() {
        let dest = dist.join("assets").join(CEF_EMBEDDED_APP_INDEX_CSS);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&index_src, &dest)?;
    }
    if opts.strip_uncompiled_tailwind_css {
        strip_dx_uncompiled_tailwind_css_assets(dist, &["theme.css", CEF_EMBEDDED_APP_INDEX_CSS]);
    }
    merge_cef_shell_index(dist, shell_index, CefMode::Browser);
    Ok(())
}

fn cef_stylesheet_link_tags(dist: &Path, mode: CefMode) -> String {
    let assets = dist.join("assets");
    let Ok(rd) = fs::read_dir(&assets) else {
        return String::new();
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "css"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    match mode {
        CefMode::Browser => {
            names.sort();
        }
        CefMode::WebviewApp => {
            names.retain(|n| n == "theme.css" || n == "index.css" || n.starts_with("index-"));
            names.sort_by(|a, b| {
                fn ord(n: &str) -> (u8, &str) {
                    if n == "theme.css" { (0, n) } else { (1, n) }
                }
                ord(a).cmp(&ord(b))
            });
        }
    }
    names
        .into_iter()
        .map(|n| format!(r#"<link rel="stylesheet" href="./assets/{n}" crossorigin="anonymous"/>"#))
        .collect::<Vec<_>>()
        .join("\n  ")
}

fn dx_module_script_href(dx_html: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)<script\s+[^>]*type="module"[^>]*\ssrc="([^"]+)""#).expect("regex")
    });
    let Some(cap) = RE.captures(dx_html) else {
        panic!("vmux: dx index.html has no <script type=\"module\" src=\"...\">");
    };
    href_for_shell(&cap[1])
}

fn href_for_shell(dx_url: &str) -> String {
    let mut s = dx_url.trim().to_string();
    while s.starts_with('/') {
        s.remove(0);
    }
    while s.starts_with("./") || s.starts_with("/./") {
        if s.starts_with("./") {
            s.drain(..2);
        } else {
            s.drain(..3);
        }
        while s.starts_with('/') {
            s.remove(0);
        }
    }
    format!("./{s}")
}

fn find_bg_wasm_href(dist: &Path) -> String {
    let wasm_dir = dist.join("wasm");
    if wasm_dir.is_dir() {
        let Ok(rd) = fs::read_dir(&wasm_dir) else {
            panic!("vmux: read {}", wasm_dir.display());
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with("_bg.wasm") {
                return format!("./wasm/{name}");
            }
        }
    }
    let assets = dist.join("assets");
    if assets.is_dir() {
        let Ok(rd) = fs::read_dir(&assets) else {
            panic!("vmux: read {}", assets.display());
        };
        let mut picks: Vec<String> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with("_bg.wasm") || name.contains("_bg-") && name.ends_with(".wasm") {
                picks.push(name);
            }
        }
        picks.sort();
        if let Some(name) = picks.into_iter().next() {
            return format!("./assets/{name}");
        }
    }
    panic!(
        "vmux: no *_bg*.wasm under {}/wasm or {}/assets",
        dist.display(),
        dist.display()
    );
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            fs::create_dir_all(&dest)?;
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn newest_bg_wasm_mtime(dir: &Path) -> Option<SystemTime> {
    let wasm_dir = dir.join("wasm");
    if !wasm_dir.is_dir() {
        return None;
    }
    let mut newest: Option<SystemTime> = None;
    let rd = fs::read_dir(&wasm_dir).ok()?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with("_bg.wasm") {
            if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                newest = Some(newest.map_or(t, |n: SystemTime| n.max(t)));
            }
        }
    }
    newest
}

fn tailwind_cli() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TAILWINDCSS") {
        let pb = PathBuf::from(p);
        if !pb.as_os_str().is_empty() {
            return Some(pb);
        }
    }
    let tw = PathBuf::from("tailwindcss");
    if Command::new(&tw)
        .arg("--help")
        .status()
        .is_ok_and(|s| s.success())
    {
        return Some(tw);
    }
    None
}

fn compile_tailwind_index_css(manifest_dir: &Path, dist_assets: &Path) -> io::Result<()> {
    if !dist_assets.is_dir() {
        return Ok(());
    }
    let Some(tw) = tailwind_cli() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "tailwindcss not found (install v3 CLI on PATH or set TAILWINDCSS)",
        ));
    };
    let out = dist_assets.join(CEF_EMBEDDED_APP_INDEX_CSS);
    let status = Command::new(&tw)
        .args([
            "-c",
            "tailwind.config.js",
            "-i",
            "assets/index.css",
            "-o",
        ])
        .arg(&out)
        .arg("--minify")
        .current_dir(manifest_dir)
        .status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("tailwindcss exited with {status}"),
        ));
    }
    Ok(())
}

fn remove_stale_prefixed_css_assets(dist_assets: &Path, stale_prefixes: &[&str]) -> io::Result<()> {
    let Ok(rd) = fs::read_dir(dist_assets) else {
        return Ok(());
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".css") {
            continue;
        }
        if stale_prefixes.iter().any(|p| name.starts_with(p)) {
            fs::remove_file(e.path())?;
        }
    }
    Ok(())
}
