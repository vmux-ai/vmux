# `vmux://lsp` Manager — Implementation Plan (single PR, on top of M1)

> **For agentic workers:** Implement **directly / inline** (superpowers:executing-plans), NOT via subagents — vmux CEF builds are large and long agents drop sockets. Keep a warm target dir. **Everything ships in ONE PR on `editor-lsp` (do not split). The user runtime-tests only at the very end**, so every task carries automated tests (unit + fixture-backed integration). Steps use checkbox (`- [ ]`) tracking.

**Goal:** A Mason-style `vmux://lsp` page that browses the reused mason-registry catalog, installs/updates/uninstalls language tools self-contained into `~/.vmux/lsp/`, wires installed servers into M1's spawn path, and runs installed linters on open — editor stays read-only (formatters install-only).

**Architecture:** All in `vmux_editor` (native backend + wasm page), no new crate. Catalog + install run off-main-thread (worker threads + per-frame-drained outbox, the `vmux_git` pattern; no tokio). Downloads via `reqwest` blocking (rustls). Managed dir mirrors Mason (`bin/` symlinks, `packages/<name>/`, receipts). Transport = rkyv bin events.

**Tech stack:** Rust, Bevy, bevy_cef (rkyv IPC), reqwest(blocking,rustls), sha2, flate2, tar, zip, Dioxus 0.7. Reuses M1's `lsp::{framing,client,registry,manager}`.

**Specs:** `docs/specs/2026-06-24-vmux-lsp-manager-design.md` (umbrella), `docs/specs/2026-06-24-editor-lsp-design.md` (M1).

**Plan-density note:** B1 tasks below carry full code (foundational + testable). B2-B4 and the Dioxus page carry precise specs (files, signatures, types, test strategy); their exact bodies are completed via TDD during execution, following the patterns B1 establishes. This is deliberate for a large, partly-exploratory effort executed inline — not placeholder filler.

---

## Reference facts (from the live machine, 2026-06-24)

- mason-registry catalog: `registry.json` (579 pkgs) downloaded from `mason-org/mason-registry` GitHub releases as `registry.json.zip`; `info.json` carries `version` + sha256 `checksums`.
- Package entry shape (from a real receipt): `source.id` is a PURL `pkg:github/rust-lang/rust-analyzer@2026-05-25`; `source.asset` is `[{target, file, bin}]` where `target ∈ {darwin_arm64, darwin_x64, linux_x64_gnu, linux_arm64_gnu, linux_x64_musl, win_x64, win_arm64}`. Install creates `bin/<name> -> packages/<name>/<bin-file>` symlinks.
- Mason layout: `bin/` (symlinks), `packages/<name>/` (payload + `mason-receipt.json`), `registries/`, `staging/`.
- Existing M1 dir `~/.vmux/lsp/` does not exist yet; create it.

---

## File structure

**Create (native, `vmux_editor/src/lsp/`):**
- `purl.rs` — parse `pkg:type/ns/name@ver` → `{ kind, namespace, name, version }`.
- `catalog.rs` — registry download/verify/cache + parse `registry.json` → `Package` model + search/filter.
- `target.rs` — host `(os, arch)` → Mason target id; pick asset.
- `store.rs` — `~/.vmux/lsp/` layout, receipts, installed-state, bin/PATH resolution, "on PATH" detection.
- `download.rs` — `reqwest::blocking` GET to file + sha256 verify (worker-thread helpers).
- `archive.rs` — extract `.gz` / `.tar.gz` / `.zip` into a dir.
- `install.rs` — install engine: per-source handlers (`github`, `npm`, `pypi`, `cargo`, `golang`), uninstall, update; `InstallOutbox` + progress.
- `lint.rs` — lint-on-open runner: per-linter argv + stdout parser → `FileDiagnostic`.

**Create (wasm):** `lsp_page.rs` — the `vmux://lsp` page component.

**Modify:**
- `vmux_core/src/event.rs` — manager event contract (catalog/install/progress/status) + lint diagnostics source tag.
- `vmux_editor/src/lsp.rs` — declare new modules; extend `LspPlugin` to add catalog/install/lint systems + `ManagerPlugin`.
- `vmux_editor/src/lsp/manager.rs` — `resolve_spec` consults `store` first; spawn env = login-shell PATH + `~/.vmux/lsp/bin`; run linters on open and merge into the diagnostics outbox.
- `vmux_editor/src/lsp/registry.rs` — add linter registry (ext → linter argv) + lang→mason-package bridge/aliases.
- `vmux_editor/src/page_model.rs` — manager page pure helpers (filter/search/status-badge) + lint column mapping if needed.
- `vmux_editor/Cargo.toml` — add `reqwest`(blocking,rustls-tls), `sha2`, `flate2`, `tar`, `zip`.
- `vmux_editor/src/lib.rs` — `pub mod lsp_page` (wasm).
- `vmux_server/src/lib.rs` — register `render_lsp: "lsp" => vmux_editor::lsp_page::Page`.
- A new `PageManifest { host:"lsp", title:"Language Servers", command_bar:true }`.

---

# PHASE B1 — catalog + github installs + managed store + page + resolution

## Task B1.1: Manager event contract (vmux_core)

**Files:** Modify `crates/vmux_core/src/event.rs`

- [ ] **Step 1: failing test** — append to `file_event_tests`:

```rust
    #[test]
    fn lsp_catalog_event_rkyv_roundtrip() {
        let ev = LspCatalogEvent {
            packages: vec![LspPackage {
                name: "rust-analyzer".into(),
                description: "Rust LSP".into(),
                languages: vec!["rust".into()],
                categories: vec!["LSP".into()],
                status: LspPkgStatus::Available,
                version: None,
                installable: true,
                requires: None,
            }],
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).unwrap();
        let d = rkyv::from_bytes::<LspCatalogEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.packages[0].name, "rust-analyzer");
        assert_eq!(d.packages[0].status, LspPkgStatus::Available);
    }
```

- [ ] **Step 2:** run → fails (types missing). `cargo test -p vmux_core lsp_catalog`.

- [ ] **Step 3:** add types + consts (near `FILE_DIAGNOSTICS_EVENT`). Derive set matches existing rkyv types.

```rust
pub const LSP_CATALOG_REQUEST: &str = "lsp_catalog_request";
pub const LSP_CATALOG_EVENT: &str = "lsp_catalog";
pub const LSP_INSTALL_REQUEST: &str = "lsp_install_request";
pub const LSP_UNINSTALL_REQUEST: &str = "lsp_uninstall_request";
pub const LSP_UPDATE_REQUEST: &str = "lsp_update_request";
pub const LSP_INSTALL_PROGRESS_EVENT: &str = "lsp_install_progress";
pub const LSP_PKG_STATUS_EVENT: &str = "lsp_pkg_status";

// (full derive set: Debug,Clone,PartialEq,Eq,Serialize,Deserialize,rkyv::{Archive,Serialize,Deserialize})
pub enum LspPkgStatus { Available, OnPath, Installing, Installed, Outdated, Running, Failed }
pub struct LspPackage {
    pub name: String, pub description: String,
    pub languages: Vec<String>, pub categories: Vec<String>,
    pub status: LspPkgStatus, pub version: Option<String>,
    pub installable: bool, pub requires: Option<String>, // e.g. "node"
}
pub struct LspCatalogRequest { pub query: String, pub language: String, pub category: String, pub installed_only: bool }
pub struct LspCatalogEvent { pub packages: Vec<LspPackage> }
pub struct LspInstallRequest { pub name: String }
pub struct LspUninstallRequest { pub name: String }
pub struct LspUpdateRequest { pub name: String }
pub enum InstallPhase { Resolving, Downloading, Extracting, Linking, Done, Failed }
pub struct LspInstallProgress { pub name: String, pub phase: InstallPhase, pub pct: Option<u8>, pub message: String }
pub struct LspPkgStatusEvent { pub name: String, pub status: LspPkgStatus, pub version: Option<String> }
```

- [ ] **Step 4:** run → pass. **Step 5:** commit `feat(core): vmux://lsp manager event contract`.

## Task B1.2: deps + module scaffold

**Files:** `vmux_editor/Cargo.toml`, `vmux_editor/src/lsp.rs`, new module stub files.

- [ ] Add to native deps: `reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }`, `sha2 = "0.10"`, `flate2 = "1"`, `tar = "0.4"`, `zip = { version = "2", default-features = false, features = ["deflate"] }`.
- [ ] Create stub files `purl.rs target.rs catalog.rs store.rs download.rs archive.rs install.rs lint.rs` (each `// filled in <task>`), declare `pub mod` in `lsp.rs`.
- [ ] `cargo build -p vmux_editor` → clean. Commit `feat(editor): manager module scaffold + deps`.

## Task B1.3: PURL parser

**Files:** `lsp/purl.rs`

- [ ] TDD: `Purl { kind: String, namespace: Option<String>, name: String, version: Option<String> }`, `parse(s) -> Option<Purl>`.

```rust
pub fn parse(s: &str) -> Option<Purl> {
    let rest = s.strip_prefix("pkg:")?;
    let (path, version) = match rest.split_once('@') { Some((p,v)) => (p, Some(v.to_string())), None => (rest, None) };
    let mut it = path.splitn(3, '/');
    let kind = it.next()?.to_string();
    let a = it.next()?; let b = it.next();
    let (namespace, name) = match b { Some(n) => (Some(a.to_string()), n.to_string()), None => (None, a.to_string()) };
    Some(Purl { kind, namespace, name, version })
}
```

Tests: `pkg:github/rust-lang/rust-analyzer@2026-05-25` → kind github, ns rust-lang, name rust-analyzer, ver set; `pkg:npm/typescript-language-server` → kind npm, ns None, name set, ver None; `pkg:cargo/taplo-cli@0.9.0`. Commit.

## Task B1.4: host target mapping

**Files:** `lsp/target.rs`

- [ ] `fn host_target() -> &'static str` from `std::env::consts::{OS,ARCH}`: macos+aarch64→`darwin_arm64`, macos+x86_64→`darwin_x64`, linux+x86_64→`linux_x64_gnu`, linux+aarch64→`linux_arm64_gnu`, windows+x86_64→`win_x64`, etc.
- [ ] `fn pick_asset<'a>(assets: &'a [Asset], target: &str) -> Option<&'a Asset>` (exact match; linux x64 falls back gnu→musl). Tests for darwin_arm64 selection + linux musl fallback. Commit.

## Task B1.5: catalog parse (fixture-backed)

**Files:** `lsp/catalog.rs`. **Test fixture:** `crates/vmux_editor/tests/fixtures/registry_sample.json` (hand-made: 3 entries — rust-analyzer (github), typescript-language-server (npm), ruff (pypi) — with the real field shape).

- [ ] `Package { name, description, homepage, languages: Vec<String>, categories: Vec<String>, source_id: String (PURL), assets: Vec<Asset{target,file,bin}>, bin: BTreeMap<String,String> }`; `Asset`.
- [ ] `parse_registry(json: &str) -> Result<Vec<Package>, String>` (serde_json, tolerant of unknown fields via `#[serde(default)]`).
- [ ] `fn search<'a>(pkgs, query, language, category, installed_set) -> Vec<&'a Package>` (case-insensitive name/desc match; language/category filter).
- [ ] Tests parse the fixture (assert 3 pkgs, rust-analyzer categories contains "LSP", assets contain darwin_arm64), and search filters. Commit. (No network in tests.)

## Task B1.6: managed store

**Files:** `lsp/store.rs`

- [ ] `fn root() -> PathBuf` = `~/.vmux/lsp` (via `vmux_core::profile` if it exposes a base, else `dirs`/home). Subdirs `bin packages registries staging`.
- [ ] Receipt type (Mason-compatible subset): `Receipt { name, version, source_id, bin: BTreeMap<String,String> }` serialized to `packages/<name>/vmux-receipt.json`.
- [ ] `installed() -> BTreeMap<String, Receipt>` (scan packages dir). `is_installed(name)`. `link_bin(name, file, link_name)` (symlink under `bin/`). `remove(name)` (rm package dir + its bin links). `bin_path(name) -> Option<PathBuf>`.
- [ ] `fn resolved_command(cmd: &str) -> Resolution` where `Resolution ∈ { Managed(PathBuf), OnPath, Missing }` — managed `bin/<cmd>` first, else `registry::executable_on_path(cmd)`.
- [ ] Tests in a temp `root` (inject root via param to keep pure): write a fake receipt + binary, assert `installed`/`is_installed`/`bin_path`/`remove`; resolved_command precedence. Commit.

## Task B1.7: download + checksum

**Files:** `lsp/download.rs`

- [ ] `fn download_to(url, dest: &Path, progress: impl FnMut(u64,Option<u64>)) -> Result<(),String>` using `reqwest::blocking::Client` streaming to file, calling progress.
- [ ] `fn sha256_file(path) -> Result<String,String>` (sha2, hex).
- [ ] Test: spin a tiny `std::net::TcpListener` HTTP server in a thread serving fixed bytes; download → assert file contents + sha256 matches precomputed. (Self-contained, no external network.) Commit.

## Task B1.8: archive extraction

**Files:** `lsp/archive.rs`

- [ ] `fn extract(file: &Path, kind: ArchiveKind, dest: &Path) -> Result<(),String>` for `Gz` (flate2 → single file named after stripping `.gz`), `TarGz` (flate2+tar unpack), `Zip` (zip crate). `fn kind_for(file: &str) -> ArchiveKind` by extension (`.tar.gz`/`.tgz`→TarGz, `.gz`→Gz, `.zip`→Zip, else Raw=copy).
- [ ] Tests: build a gz + a zip in-memory/temp, extract, assert payload. Commit.

## Task B1.9: github install handler + outbox

**Files:** `lsp/install.rs`, `lsp.rs` (add `InstallOutbox`)

- [ ] `InstallOutbox(Arc<Mutex<Vec<InstallMsg>>>)` Resource (like `LspOutbox`); `InstallMsg { name, progress: LspInstallProgress | StatusDone }`.
- [ ] `fn install_github(pkg: &Package, store_root: &Path, target: &str, mut emit: impl FnMut(InstallPhase,Option<u8>,&str)) -> Result<Receipt,String>`: pick asset → download to `staging/` (emit Downloading w/ pct) → (verify if checksum available) → extract to `packages/<name>/` (emit Extracting) → chmod +x the bin → `link_bin` per `pkg.bin` (emit Linking) → write receipt → emit Done. Failure → emit Failed + cleanup staging.
- [ ] `fn install(pkg, store_root, target, emit)` dispatches by `purl::parse(&pkg.source_id).kind`: `"github"` → install_github; others → `Err("source <kind> not yet supported")` (B2 fills npm/pypi/cargo/golang).
- [ ] Test: reuse the Task B1.7 local HTTP server to serve a `.gz` "binary"; a fixture Package pointing at it; `install_github` into temp store → assert receipt + `bin/<name>` symlink + executable. Commit.

## Task B1.10: catalog fetch (network, lazy) + cache

**Files:** `lsp/catalog.rs` (extend)

- [ ] `fn registry_url() -> String` = latest `mason-org/mason-registry` release `registry.json.zip` (use the GitHub "latest" redirect: `https://github.com/mason-org/mason-registry/releases/latest/download/registry.json.zip`). `info.json` sibling for checksum.
- [ ] `fn ensure_catalog(store_root, refresh: bool) -> Result<Vec<Package>,String>`: if cached `registries/registry.json` present and !refresh → parse cache; else download zip → verify (best-effort) → unzip → cache → parse.
- [ ] Test: serve a zip of the fixture from the local HTTP server, `ensure_catalog` with a stubbed URL → parses. (Inject URL via param/env for test.) Commit.

## Task B1.11: backend wiring — catalog/install systems

**Files:** `lsp.rs`, `lsp/install.rs`, `lsp/manager.rs`

- [ ] `ManagerPlugin` (added by `LspPlugin`): inserts `InstallOutbox`; observers for `BinReceive<LspCatalogRequest|LspInstallRequest|LspUninstallRequest|LspUpdateRequest>`; `drain_install_outbox` system emitting `LspInstallProgress`/`LspPkgStatusEvent` to the requesting webview.
- [ ] On `LspCatalogRequest`: spawn thread → `ensure_catalog` + compute per-package status (installed via store / on PATH via resolve / else Available) + filter → push catalog to outbox → drain emits `LspCatalogEvent`.
- [ ] On `LspInstallRequest`: spawn thread → `install(...)` with emit closure pushing progress to outbox; on Done, status Installed.
- [ ] On `LspUninstallRequest`: `store::remove`; emit status Available.
- [ ] Register the receive types via `BinEventEmitterPlugin::<(...)>`. Tests: outbox drains (git-style). Commit.

## Task B1.12: resolution + spawn env (wire store + login-shell PATH into M1)

**Files:** `lsp/manager.rs`, `lsp/registry.rs`

- [ ] `LspManager::open` resolves the server command via `store::resolved_command` (Managed path → spawn that abs path; OnPath → spawn by name; Missing → skip + info log, page can install).
- [ ] `ServerClient::spawn` (or a thin wrapper) sets the child's env `PATH` = login-shell PATH (`vmux_terminal::shell_env::merge_login_shell_env`) with `~/.vmux/lsp/bin` prepended. (Add `vmux_terminal` dep to vmux_editor native, or lift `shell_env` to a shared spot — verify no dep cycle; if cycle, duplicate the tiny capture.)
- [ ] Tests: resolved_command precedence already covered (B1.6); add a manager test that a Managed resolution produces an absolute command. Commit.

## Task B1.13: page_model helpers for the manager page

**Files:** `page_model.rs`

- [ ] Pure helpers (native-testable): `pkg_status_label(status)->&str`, `pkg_status_class(status)->&str` (soft-glass severity colors), `filter_packages` mirror for the frontend if needed, `action_for(status)->Action` (Install/Update/Uninstall/None). Tests. Commit.

## Task B1.14: `vmux://lsp` page + registration

**Files:** `lsp_page.rs`, `lib.rs`, `vmux_server/src/lib.rs`, a `PageManifest`

- [ ] `lsp_page::Page` (Dioxus): on mount emit `LspCatalogRequest`; listen `LspCatalogEvent`/`LspInstallProgress`/`LspPkgStatusEvent`; render search box + filter chips + package list (name, lang/category chips, status badge, action button) + a detail/log pane (streamed progress for the selected/installing package). Soft-glass; SVG `lang_icon`.
- [ ] Buttons emit `LspInstallRequest`/`LspUninstallRequest`/`LspUpdateRequest`.
- [ ] Register `render_lsp: "lsp" => vmux_editor::lsp_page::Page` in `web_pages!`; spawn `PageManifest { host:"lsp", title:"Language Servers", keywords:&["lsp","language","server","install"], icon:"server", command_bar:true }` in `ManagerPlugin`.
- [ ] Verify: `cargo build -p vmux_editor --target wasm32-unknown-unknown` clean. (UI verified at the end by the user.) Commit.

---

# PHASE B2 — additional install sources

## Task B2.1: npm handler
**Files:** `lsp/install.rs`. Detect `npm`; `npm install --prefix packages/<name> <name>@<ver>`; link from `node_modules/.bin/<bin>`. Toolchain-missing → `Err("requires node")` surfaced as `requires:"node"`. Test with a stub `npm` on PATH (fixture script) writing a fake node_modules. Commit.

## Task B2.2: pypi handler
**Files:** `lsp/install.rs`. Detect `python3`; `python3 -m venv packages/<name>/venv` + `venv/bin/pip install <name>==<ver>`; link `venv/bin/<bin>`. requires:"python". Stub-`python3` test. Commit.

## Task B2.3: cargo handler
`cargo install --root packages/<name> --version <ver> <crate>`; bin in `packages/<name>/bin`. requires:"cargo". Stub test. Commit.

## Task B2.4: golang handler
`GOBIN=packages/<name>/bin go install <module>@<ver>`. requires:"go". Stub test. Commit.

## Task B2.5: toolchain detection surfaced in catalog
**Files:** `lsp/manager.rs` catalog status. For a package whose source needs a missing toolchain, set `installable:false, requires:"<tool>"`. Test. Commit.

---

# PHASE B3 — updates / versions / refresh

## Task B3.1: installed version + outdated detection
Receipt stores installed version; compare with catalog's PURL version → `Outdated`. `LspUpdateRequest` = reinstall latest. Tests. Commit.

## Task B3.2: registry refresh action
A `refresh` flag on `LspCatalogRequest` re-downloads the registry; UI "Refresh" button. Test ensure_catalog(refresh=true) re-fetches. Commit.

## Task B3.3: cancel/cleanup + re-detect after install
After install Done, re-emit catalog status so the open editor's `resolve_spec` picks up the new binary (trigger a re-open / re-spawn for matching open files). Test: status transition. Commit.

---

# PHASE B4 — lint-on-open runner

## Task B4.1: linter registry
**Files:** `lsp/registry.rs`. `linter_for(ext) -> Option<LinterSpec{command,args(file),stdout_format}>` for a starter set (ruff→python, eslint→js/ts, shellcheck→sh, golangci-lint→go, clippy via cargo? skip). Only run if `store::resolved_command` finds it. Tests. Commit.

## Task B4.2: lint runner + parsers
**Files:** `lsp/lint.rs`. `run_linters(path, exts) -> Vec<FileDiagnostic>` off-thread: spawn linter, parse stdout (JSON for ruff/eslint, regex for shellcheck) → `FileDiagnostic` (reuse the M1 type + a `source` tag). Push to the M1 diagnostics outbox so they merge with LSP diagnostics in the same `FileDiagnosticsEvent`. Unit-test each parser on captured sample output (fixtures). Commit.

## Task B4.3: trigger on open + merge
**Files:** `lsp/manager.rs`. On text file open (and watcher reload), if a linter is installed for the ext, run it; merge its diagnostics with LSP diagnostics per path before emitting. Dedup/merge test. Commit.

---

# FINALIZATION

## Task F1: fmt + clippy + tests (workspace)
- [ ] `cargo fmt --all` then `git checkout -- patches/`.
- [ ] `cargo clippy --workspace --all-targets` → fix all warnings.
- [ ] `cargo test --workspace` → green. `cargo build -p vmux_editor --target wasm32-unknown-unknown` → clean.
- [ ] Commit fixes.

## Task F2: user runtime test + PR
- [ ] Hand to user for the single end-to-end runtime test (open files, install rust-analyzer from `vmux://lsp`, see diagnostics; check a linter).
- [ ] On confirmation: `gh pr create` (direct, not `-w`), single PR with M1 + manager. Then delete both plan files.

---

## Build/CI notes
- Keep a warm target dir; don't share `CARGO_TARGET_DIR` across worktrees (CEF cmake).
- `cargo fmt` reformats `patches/` — `git checkout -- patches/` before committing.
- `vmux_server/build.rs` already tracks `../vmux_editor/src` → new files trigger wasm rebuilds.
- Tests must not hit the real network: use the local-`TcpListener` HTTP fixture and stub toolchain scripts on a temp PATH.
- Gate the existing `vmux_mock_lsp` bin's wasm build (already done in M1); any new test bins likewise.

## Open items finalized during build
- `~/.vmux/lsp` base path source (`vmux_core::profile` vs `dirs`).
- `shell_env` reuse vs duplication (dep-cycle check `vmux_editor` → `vmux_terminal`).
- Exact lang→mason-package alias table; exact linter starter set + stdout parsers.
- reqwest blocking inside the Bevy process: confirm it needs no extra runtime setup.
