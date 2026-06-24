# `vmux://lsp` — Mason-style language-tool manager

Date: 2026-06-24
Branch: `editor-lsp` (design only; implementation is a follow-on — see "Relationship to M1")
Status: design (umbrella)

## Motivation

Milestone 1 (the editor-lsp client core + diagnostics) makes diagnostics work, but
only if the user already has a language server on `PATH`. There is no way to see
whether a server is present, and no path to get one. On a packaged `.app` (launched
by Finder/launchd) the process even gets a minimal `PATH` that excludes Homebrew and
cargo, so installed servers are invisible.

The goal is a **`vmux://lsp` manager page** — vmux's equivalent of `:Mason` — to
discover, install, update, and remove language tools, **self-contained** so LSP "just
works" without the user hand-wrangling `brew`/`rustup`/`npm`. The reference is
[Mason.nvim](https://github.com/mason-org/mason.nvim); we reuse its registry and
on-disk conventions rather than reinventing them.

### Reference: how the user's Neovim does it (validated 2026-06-24)

- Mason manages **LSP servers only**: `rust_analyzer`, `solang`, `eslint`, `ts_ls`
  (`mason-lspconfig`, `automatic_installation = true`).
- **Formatting rides on the LSP** (`vim.lsp.buf.format()` on save). No `conform`,
  `none-ls`, or `nvim-lint` — i.e. **no standalone linters/formatters**.
- Mason on disk: `~/.local/share/nvim/mason/` → `bin/<tool>` **symlinks** into
  `packages/<name>/`; each package has a `mason-receipt.json`; the catalog is a
  **579-package `registry.json`** (514 KB) downloaded from `mason-org/mason-registry`
  and checksum-verified. Packages are identified by **PURL**
  (`pkg:github/…`, `pkg:npm/…`, `pkg:pypi/…`, `pkg:cargo/…`, `pkg:golang/…`) with a
  per-target asset map.
- `mason/bin` is on the user's `PATH`, which is why M1's PATH detection already finds
  rust-analyzer for *this* user. The manager primarily helps machines without that.

Two consequences shape the design:

1. **The install engine is category-agnostic.** Installing a server, linter, or
   formatter is the same operation (resolve PURL → fetch → extract → link). The
   catalog already tags every package by category. So a "full Mason" *manager*
   (browse/install/update/remove *any* package) is one coherent build. What differs
   per category is the **consumer** that *runs* the tool — and only the **LSP client
   consumer exists** (M1). Linter/formatter *runners* are separate later work (and
   format-on-save needs an editing surface the editor does not yet have; note the
   reference user formats via the LSP anyway).

2. **Reuse the catalog.** We consume `mason-org/mason-registry`'s `registry.json`
   directly (download + checksum-verify + parse), so there is zero catalog
   maintenance and instant 579-package parity.

## Goals

- A `vmux://lsp` page: browse the full catalog (search + filter by language and
  category), see per-package status, install / update / uninstall, with streamed
  install progress.
- **Self-contained installs** into a vmux-managed dir (`~/.vmux/lsp/`,
  Mason-compatible layout) — no dependency on system package managers for
  GitHub-sourced prebuilt binaries.
- **Reuse mason-registry** as the catalog (checksum-verified).
- **Category-agnostic install engine** (servers + linters + formatters all
  installable). LSP servers are immediately usable (wired into M1's spawn path);
  linters/formatters install but are not executed in v1.
- **Respect existing installs**: if a tool already resolves on `PATH` (e.g. the
  user's `mason/bin`), show it as "on PATH" and do not double-install.
- **Login-shell `PATH`** when spawning servers, so the packaged `.app` finds tools,
  not just `make dev`.

## Non-goals (deferred)

- **Running** linters/formatters: no lint-runner; format-on-save needs the editing
  surface. Installable in v1, executed in a later phase (P4) with its own design.
- DAP/debuggers, and managing language *runtimes* themselves (node/python/cargo/go)
  — for toolchain-dependent sources we **detect and guide**, we do not install the
  runtime.
- A bespoke catalog (we reuse mason-registry).

## Architecture

Same host/page split as other `vmux://` pages. A new page host **`lsp`** maps to
`vmux_editor::lsp_page::Page` (registered in `vmux_server`'s `web_pages!`), with a
backend `PageManifest { host: "lsp", title: "Language Servers", command_bar: true }`
so it is command-bar discoverable. **No new crate** — all of it lives in
`vmux_editor`, alongside the M1 LSP backend. Transport is the existing rkyv
bin-event channel; long-running installs use the `vmux_git`-style
spawn→thread→outbox→drain→emit pattern (no tokio). Downloads use a **blocking** HTTP
client on a worker thread.

### Modules (native, `cfg(not(wasm32))`, in `vmux_editor/src/lsp/`)

- `catalog.rs` — fetch/verify/parse `mason-registry/registry.json`; the PURL + asset
  model; search/filter queries.
- `install.rs` — the install engine: resolve target asset → download → verify →
  extract → link bin → write receipt; per-source handlers (`github`, `npm`, `pypi`,
  `cargo`, `golang`); uninstall; update.
- `store.rs` — managed-dir layout (`~/.vmux/lsp/`), receipts, installed-state
  queries, bin/PATH resolution, "respect existing PATH" logic.
- `manager.rs` (existing, extended) — `resolve_spec` consults the managed store
  first, then `PATH`; spawn servers with managed `bin/` + login-shell `PATH`.
- `lsp_page.rs` (wasm) — the page UI; pure helpers in `page_model.rs`.
- Event contract added to `vmux_core/src/event.rs` (reuse, no new crate).

### Catalog (reuse mason-registry)

- **Source**: the `registry.json.zip` asset from the latest `mason-org/mason-registry`
  GitHub release, plus its `checksums`/`info.json`. Download, verify sha256, unzip,
  cache under `~/.vmux/lsp/registries/`.
- **Refresh**: on demand (a "Refresh" action) and a periodic latest-release check
  (e.g. once/day); fall back to the cached copy when offline.
- **Parse**: each entry → `{ name, description, homepage, licenses, languages[],
  categories[] (LSP/Linter/Formatter/DAP/Runtime), source: Purl, bin: map }`. The
  PURL scheme selects the install handler.

### Install engine (by PURL source)

- **`github`** (toolchain-free; covers rust-analyzer, gopls, clangd, lua-language-
  server, zls, taplo, marksman, …): pick the asset by `(os, arch)` using Mason's
  target ids (`darwin_arm64`, `darwin_x64`, `linux_x64_gnu`, …); download; verify;
  extract (`.gz` → single binary, `.zip`/`.tar.gz` → dir); `chmod +x`; create
  `bin/` symlinks per the package's bin map; write a Mason-compatible receipt.
- **`npm`** (needs `npm`): `npm install --prefix packages/<name> <pkg>@<ver>`; link
  from `node_modules/.bin`.
- **`pypi`** (needs `python`): venv in `packages/<name>`, `pip install`; link bin.
- **`cargo`** (needs `cargo`): `cargo install --root packages/<name> <crate>`.
- **`golang`** (needs `go`): `GOBIN=packages/<name>/bin go install <module>@<ver>`.
- **Toolchain detection**: when `npm`/`python`/`cargo`/`go` is absent, the package's
  action is disabled with a "requires X" hint — we never install the runtime.
- Installs run off the main thread; results/progress flow through an outbox drained
  each frame. Work happens in a `staging/` dir and is atomically moved into
  `packages/` on success; failures clean up.

### Managed store + resolution

- `~/.vmux/lsp/{ bin/, packages/<name>/, registries/, staging/ }` (Mason-compatible).
- `resolve_spec` (M1) gains a lookup order: (1) managed store `bin/` if installed,
  (2) `executable_on_path` (finds the user's mason/system installs), (3) not
  available → the page offers install.
- **Spawn env**: merge the login-shell `PATH` (reuse
  `vmux_terminal::shell_env::merge_login_shell_env`) and prepend
  `~/.vmux/lsp/bin`. This fixes the packaged-app PATH problem for both managed and
  system installs.
- **language → package bridge**: M1's registry maps `ext → server command`; the
  catalog maps `package → bin`. For most servers the command *is* the mason package
  name (`rust-analyzer`, `gopls`, `clangd`, `typescript-language-server`,
  `vscode-json-language-server`, …), so the bridge is "command name ≈ package name"
  plus a small alias table for exceptions. Finalized in the plan.

### Page UX (`vmux://lsp`)

- **Layout**: a search box + filter chips (language, category, "installed only");
  a package list (icon, name, language + category chips, status badge, action
  button); a detail/log pane showing the selected package and streamed install
  output. Installed packages grouped/pinned.
- **Status badges**: Available / On PATH / Installing (with progress) / Installed
  (vX) / Update available / Running.
- **Actions**: Install / Update / Uninstall; toolchain-dependent sources missing
  their runtime show a disabled action + "Requires node/python/…".
- **Style**: soft-glass (translucent rounded panes, accent pills, SVG `lang_icon`
  glyphs), keyboard column navigation — consistent with the other vmux pages.

### Data / event contract (`vmux_core/src/event.rs`, rkyv + serde)

- `LspCatalogRequest { query: String, filters }` → `LspCatalogEvent { packages:
  Vec<LspPackage> }` where `LspPackage { name, description, languages, categories,
  status, version, installable, requires: Option<String> }`.
- `LspInstallRequest { name }`, `LspUpdateRequest { name }`, `LspUninstallRequest
  { name }`.
- `LspInstallProgress { name, phase (Resolving/Downloading/Extracting/Linking/
  Done/Failed), pct: Option<u8>, message }`.
- `LspPackageStatusEvent { name, status, version }`.
- Event-name consts alongside the existing `FILE_*` ones.

### Error handling

- Network/registry download failure → surfaced in the page with retry; offline →
  use cached registry.
- Checksum mismatch → abort, error badge.
- Missing toolchain (npm/pip/…) → disabled action + guidance, never a silent fail.
- Extract/permission/partial-install failures → staging cleanup, error surfaced.
- All install work is fault-isolated on its worker thread; a failure never crashes
  the app.

### Testing

- **catalog**: parse a `registry.json` fixture → PURL/assets/categories; target
  selection (`os/arch → asset`).
- **install engine**: a `github` install served by a local HTTP fixture (tiny
  `.gz`/`.zip`) → installs into a temp store, asserts the `bin/` symlink + receipt;
  checksum verify path.
- **store/resolution**: managed-store-first vs PATH fallback; "respect existing
  PATH".
- **page_model**: search/filter + status-badge mapping (native-testable pure fns).
- **e2e**: install a tiny fake package from a local fixture → shows Installed →
  `resolve_spec` finds it → (with the M1 mock server) diagnostics appear.

### Dependencies (new external; all small, no tokio)

- Blocking HTTP for downloads, sha256 for verification, and zip/gzip/tar for
  extraction. **Before adding crates, check what the workspace already has** — e.g.
  `vmux_desktop/src/updater.rs` already downloads release artifacts; reuse its HTTP
  stack if suitable. Likely additions: `sha2`, and an archive crate set
  (`zip`, `flate2`, `tar`). Justify each against CEF build weight in the plan.

## Phasing (each phase = its own implementation plan)

- **P1 — servers, self-installable, page**: catalog (reuse mason-registry) + `github`
  install engine + managed store + resolution + login-shell PATH + the `vmux://lsp`
  page (browse/search/filter, install/uninstall, streamed progress) for LSP servers.
  **~20-24 tasks.** Outcome: one-click rust-analyzer (no brew/rustup), diagnostics
  light up; servers visible and manageable.
- **P2 — more sources**: `npm`/`pypi`/`cargo`/`golang` handlers + toolchain
  detection. ~10-12 tasks.
- **P3 — updates/versions**: registry refresh, outdated detection, update/pin. ~6-8
  tasks.
- **P4 — linter/formatter consumers**: a lint-on-open runner; format-on-save once an
  editing surface exists. Separate brainstorm (the manager already installs these;
  this phase makes them *run*).

## Relationship to M1

This builds on Milestone 1 (`editor-lsp` branch: client core + diagnostics). M1 should
be runtime-verified and merged first; the manager is a follow-on that **extends**
M1's `resolve_spec`/spawn. Implementation should start on its own worktree/branch off
`origin/main` after M1 lands, not pile onto the M1 PR.

## Open items for the P1 plan

- The `language/server → mason-package` bridge (alias table for the exceptions).
- Registry refresh cadence + where "check for updates" lives.
- Exact page layout (a mockup) and the install-flow states.
- Confirm/choose the HTTP + archive dependency set (reuse updater's stack if viable).
