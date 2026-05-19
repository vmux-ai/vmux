# Domain Crate Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract `vmux_settings`, `vmux_command`, `vmux_terminal`, and `vmux_agent` ownership out of `vmux_desktop` so each domain crate exposes a single `Plugin`. Companion: rename `vmux_layout::Tab` (component) → `vmux_layout::Space`, and `PROCESSES_WEBVIEW_URL` → `SERVICES_WEBVIEW_URL` (deduped to one location).

**Architecture:** Mirror VMX-121's message-boundary pattern. Cross-crate communication via Bevy `Message` types and ECS components, never direct function calls. Each owner crate exposes a top-level `Plugin` registered by `VmuxPlugin`.

**Tech Stack:** Rust, Bevy ECS, vmux internal crates.

**Reference spec:** `docs/specs/2026-05-19-domain-crate-extraction-design.md`

**Project conventions (from AGENTS.md):**
- No comments in code; no `mod.rs` (filename + directory pattern).
- After each commit, run fmt + clippy + test on the changed crates only.
- Never `git add -A` / `.` — stage specific paths.
- Never amend commits; create new ones.
- No `Co-Authored-By` trailers.
- Verification template per commit:
  ```bash
  PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
  for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
  for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
  for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
  ```

---

## Phase 0 — Companion cleanups

### Task 1: Rename `PROCESSES_WEBVIEW_URL` → `SERVICES_WEBVIEW_URL`, dedupe to one location

**Files:**
- Modify: `crates/vmux_service/src/webview/event.rs` (rename constant)
- Delete-the-constant from: `crates/vmux_layout/src/event.rs` (the line `pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";`)
- Modify: every caller (discover via `rg`)

- [ ] **Step 1: Find every reference**

```bash
rg -F 'PROCESSES_WEBVIEW_URL' crates/ --type rust
```

Expect hits in: `vmux_service::webview::event` (definition), `vmux_layout::event` (duplicate definition), and ~10 caller sites across `vmux_desktop` and possibly `vmux_command`/`vmux_terminal`.

- [ ] **Step 2: Rename the canonical definition**

In `crates/vmux_service/src/webview/event.rs` change:

```rust
pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";
```

to:

```rust
pub const SERVICES_WEBVIEW_URL: &str = "vmux://services/";
```

Also update the rustdoc on the line above:

```rust
/// URL for the services monitor webview.
```

- [ ] **Step 3: Delete the duplicate**

In `crates/vmux_layout/src/event.rs` delete the line:

```rust
pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";
```

- [ ] **Step 4: Update all importers**

For each caller, change `vmux_service::webview::event::PROCESSES_WEBVIEW_URL` (or `vmux_layout::event::PROCESSES_WEBVIEW_URL`) to `vmux_service::webview::event::SERVICES_WEBVIEW_URL`. Update local rebinds (`use ... as PROCESSES_WEBVIEW_URL`) similarly.

Common pattern in callers:

```rust
// before
use vmux_service::webview::event::PROCESSES_WEBVIEW_URL;
// ...
if url.starts_with(PROCESSES_WEBVIEW_URL) { ... }

// after
use vmux_service::webview::event::SERVICES_WEBVIEW_URL;
// ...
if url.starts_with(SERVICES_WEBVIEW_URL) { ... }
```

Don't touch `PROCESSES_LIST_EVENT`, `PROCESSES_NAVIGATE_EVENT`, `ProcessesMonitor`, `processes_monitor.rs` — those renames are explicitly out of scope.

- [ ] **Step 5: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

```bash
rg -F 'PROCESSES_WEBVIEW_URL' crates/ --type rust
```

Expected: zero hits.

- [ ] **Step 6: Commit**

```bash
git add -u crates/
git commit -m "refactor: rename PROCESSES_WEBVIEW_URL to SERVICES_WEBVIEW_URL and dedupe"
```

---

### Task 2: Rename `vmux_layout::Tab` (component) → `vmux_layout::Space`

**Files:**
- Rename: `crates/vmux_layout/src/tab.rs` → `crates/vmux_layout/src/space.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (swap `mod tab;` → `mod space;`)
- Modify: every caller — `pub use` chains, imports, and body references

- [ ] **Step 1: Find every reference**

```bash
rg -nF 'crate::tab::Tab' crates/vmux_layout/src/
rg -nF 'vmux_layout::tab' crates/
rg -nF '::Tab as SpaceTab' crates/
rg -nw 'Tab' crates/vmux_layout/src/
```

Note the distinction: `vmux_layout::Tab` (the *component* being renamed) vs `vmux_layout::protocol::Tab` (the DTO — leave untouched). The latter does NOT get renamed.

- [ ] **Step 2: Rename the file**

```bash
git mv crates/vmux_layout/src/tab.rs crates/vmux_layout/src/space.rs
```

- [ ] **Step 3: Rename types inside the file**

In `crates/vmux_layout/src/space.rs`:

- `pub struct Tab` → `pub struct Space` (the only `pub struct Tab` defined in that file)
- `pub fn tab_bundle()` → `pub fn space_bundle()`
- `pub struct TabPlugin` → `pub struct SpacePlugin`
- `pub struct TabCommandSet` → `pub struct SpaceCommandSet`
- `fn handle_tab_commands` → `fn handle_space_commands` (and any helpers that reference "tab" as a *space* concept; tab-commands originating from `vmux_command::TabCommand` are pane-tab commands — leave those names alone if they refer to pane-tabs vs space-tabs; inspect each usage)
- Module-level `Tab::default()` → `Space::default()`
- All in-file `Tab` references inside fn bodies → `Space`

**Critical — preserve type_path for save compat:**

The component currently has:

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Tab {
    pub name: String,
}
```

After rename, keep the `#[type_path = "vmux_desktop::layout::tab"]` attribute UNCHANGED. The Rust type renames to `Space`, but the on-disk reflect path stays as before so existing saved layouts deserialize.

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Space {
    pub name: String,
}
```

- [ ] **Step 4: Swap module declaration**

In `crates/vmux_layout/src/lib.rs`:

```rust
// before
pub mod tab;
```

to:

```rust
pub mod space;
```

If there are `pub use tab::{...}` re-exports in `lib.rs`, update them too:

```rust
// before
pub use tab::{Tab, TabPlugin, tab_bundle};
// after
pub use space::{Space, SpacePlugin, space_bundle};
```

(Check actual re-exports — the codebase may also export `TabCommandSet`.)

- [ ] **Step 5: Update all in-crate references inside vmux_layout**

Replace within `crates/vmux_layout/src/`:

| Old | New |
|---|---|
| `use crate::tab::{...}` | `use crate::space::{...}` |
| `crate::tab::Tab` | `crate::space::Space` |
| `Tab::default()` | `Space::default()` (only where Tab was the component) |
| `Tab { name: ... }` | `Space { name: ... }` |
| `With<Tab>` | `With<Space>` |
| `Query<&Tab>` | `Query<&Space>` |
| `tab_bundle()` | `space_bundle()` |
| `TabPlugin` | `SpacePlugin` |

Use ripgrep per file to catch every reference. Be careful NOT to rename `TabCommand` (a `vmux_command` enum variant) or `protocol::Tab` (the DTO).

- [ ] **Step 6: Update all out-of-crate references**

```bash
rg -nF 'vmux_layout::Tab' crates/
rg -nF 'vmux_layout::tab' crates/
rg -nF 'vmux_layout::tab_bundle' crates/
rg -nF 'vmux_layout::TabPlugin' crates/
rg -nF 'vmux_layout::TabCommandSet' crates/
rg -nF 'Tab as SpaceTab' crates/      # disambiguation aliases from VMX-121
rg -nF 'Tab as ProtoTab' crates/      # any other aliases worth dropping
```

For each hit, rename the import and any body usage. Drop the `as SpaceTab` aliases entirely — they're no longer needed because there's no name collision once the component is `Space`.

Example from `crates/vmux_layout/src/reconcile.rs` (VMX-121 left it like this):

```rust
// before
use crate::tab::{Tab as SpaceTab, tab_bundle};
// ... body uses SpaceTab throughout

// after
use crate::space::{Space, space_bundle};
// ... body uses Space throughout
```

Same shape elsewhere in the workspace.

- [ ] **Step 7: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

```bash
rg -nF 'vmux_layout::Tab' crates/ --type rust
rg -nF 'vmux_layout::tab' crates/ --type rust
rg -nw 'tab_bundle' crates/ --type rust
rg -nw 'TabPlugin' crates/ --type rust
rg -nF 'SpaceTab' crates/ --type rust
```

Expected: zero hits across all five searches.

- [ ] **Step 8: Smoke-test save/load compat**

Run the app once and confirm it loads existing saved layouts without panicking:

```bash
env -u CEF_PATH cargo run -p vmux_desktop 2>&1 | head -30
```

Expected: app starts, loads the persisted space layout, no "type not registered" or "unknown component" errors. Quit after the window appears.

- [ ] **Step 9: Commit**

```bash
git add -u crates/
git commit -m "refactor(layout): rename Tab component to Space"
```

---

## Phase 1 — `vmux_settings` extraction

### Task 3: Move `themes.rs` into `vmux_settings`

**Files:**
- Create: `crates/vmux_settings/src/themes.rs` (copy from desktop)
- Delete: `crates/vmux_desktop/src/themes.rs`
- Modify: `crates/vmux_settings/src/lib.rs` (add `pub mod themes;`)
- Modify: `crates/vmux_desktop/src/lib.rs` (drop `mod themes;`)
- Modify: every caller — change `crate::themes` (from desktop) → `vmux_settings::themes`

- [ ] **Step 1: Read the source file**

```bash
cat crates/vmux_desktop/src/themes.rs | head -20
```

Note its dependencies: which crates does it import from? Most likely just `serde`, `bevy`, and possibly `vmux_settings::event` already.

- [ ] **Step 2: Move the file**

```bash
git mv crates/vmux_desktop/src/themes.rs crates/vmux_settings/src/themes.rs
```

- [ ] **Step 3: Register the module**

In `crates/vmux_settings/src/lib.rs` add (preserving the existing module list, alphabetical or grouped sensibly):

```rust
pub mod themes;
```

- [ ] **Step 4: Drop the desktop module declaration**

In `crates/vmux_desktop/src/lib.rs` delete:

```rust
mod themes;
```

- [ ] **Step 5: Adjust imports inside themes.rs if needed**

The moved file may reference `crate::settings` (when settings is still in desktop) — change to `vmux_settings::...` or leave as `crate::...` if it's referencing other items inside `vmux_settings`. Most likely no change needed if themes.rs is leaf data.

- [ ] **Step 6: Update all callers**

```bash
rg -nF 'crate::themes' crates/vmux_desktop/src/
```

For each hit, change:

```rust
use crate::themes::...;
```

to:

```rust
use vmux_settings::themes::...;
```

- [ ] **Step 7: Verify and commit**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done

git add -u crates/
git commit -m "refactor(settings): move themes.rs into vmux_settings"
```

---

### Task 4: Move `settings.rs` into `vmux_settings`

**Files:**
- Create: `crates/vmux_settings/src/runtime.rs` (the moved `settings.rs` content, renamed for clarity)
- Delete: `crates/vmux_desktop/src/settings.rs`
- Modify: `crates/vmux_settings/src/lib.rs` (add `pub mod runtime;`)
- Modify: `crates/vmux_desktop/src/lib.rs` (drop `mod settings;`)
- Modify: every caller (`crate::settings` → `vmux_settings::runtime` or `vmux_settings`)

- [ ] **Step 1: Read the source file**

```bash
wc -l crates/vmux_desktop/src/settings.rs
head -30 crates/vmux_desktop/src/settings.rs
```

The file is ~884 lines. Note the public types it exports: `AppSettings`, `serialize_settings_to_json`, `SettingsLoadError`, `EffectiveStartupUrl` (per the existing usages in command_bar/agent_query/etc.).

Note its external deps: file watcher (`notify` crate), serde, ron, bevy. Confirm they're already in `vmux_settings/Cargo.toml`. If not, add them.

```bash
cat crates/vmux_settings/Cargo.toml
```

If `notify` (or similar file-watcher dep) is missing, add it in step 3.

- [ ] **Step 2: Move the file**

```bash
git mv crates/vmux_desktop/src/settings.rs crates/vmux_settings/src/runtime.rs
```

- [ ] **Step 3: Update `vmux_settings/Cargo.toml`**

If the moved file uses crates not already listed under `[dependencies]` (or `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` if the runtime is non-wasm), add them. Most likely additions:

```toml
notify = { workspace = true }       # if used for file watching
ron = { workspace = true }
```

Don't speculatively add deps — check the actual `use` statements in the file first and add only what's needed.

- [ ] **Step 4: Register the module in vmux_settings**

In `crates/vmux_settings/src/lib.rs`, register the new module and re-export the public API so existing call patterns survive:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;

#[cfg(not(target_arch = "wasm32"))]
pub use runtime::{AppSettings, EffectiveStartupUrl, SettingsPlugin, serialize_settings_to_json};
```

Adjust the re-export list to match what `runtime.rs` actually defines (read the source file's `pub` items).

- [ ] **Step 5: Add `SettingsPlugin` if not present**

If the moved file already defines `SettingsPlugin`, leave it. If not (it likely lives in `settings_view.rs` or is currently a free `add_plugins` call in `vmux_desktop::lib::VmuxPlugin`), define one in `runtime.rs`:

```rust
pub struct SettingsRuntimePlugin;

impl Plugin for SettingsRuntimePlugin {
    fn build(&self, app: &mut App) {
        // Register AppSettings resource, file-watcher system, etc.
        // Pull body from whatever currently runs in vmux_desktop::VmuxPlugin
        // for settings runtime.
    }
}
```

The combined `SettingsPlugin` (covering runtime + view) gets assembled in Task 5 when settings_view moves.

- [ ] **Step 6: Drop the desktop module declaration**

In `crates/vmux_desktop/src/lib.rs` delete:

```rust
mod settings;
```

- [ ] **Step 7: Update all in-desktop callers**

```bash
rg -nF 'crate::settings::' crates/vmux_desktop/src/
```

For each hit, change `crate::settings::...` → `vmux_settings::...` (using the public re-exports from step 4).

Example:

```rust
// before
use crate::settings::AppSettings;
// after
use vmux_settings::AppSettings;
```

- [ ] **Step 8: Verify and commit**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done

git add -u crates/
git add crates/vmux_settings/src/runtime.rs
git commit -m "refactor(settings): move settings runtime into vmux_settings"
```

---

### Task 5: Move `settings_view.rs` into `vmux_settings` and assemble `SettingsPlugin`

**Files:**
- Create: `crates/vmux_settings/src/view.rs`
- Delete: `crates/vmux_desktop/src/settings_view.rs`
- Modify: `crates/vmux_settings/src/lib.rs` (add `pub mod view;`, unify `SettingsPlugin`)
- Modify: `crates/vmux_desktop/src/lib.rs` (drop `mod settings_view;`, drop `SettingsPlugin` from `add_plugins`, add the new combined `vmux_settings::SettingsPlugin`)

- [ ] **Step 1: Read the source file**

```bash
wc -l crates/vmux_desktop/src/settings_view.rs
head -40 crates/vmux_desktop/src/settings_view.rs
```

Identify the existing `SettingsPlugin` (likely defined here) and its `Plugin::build` body. Note systems and observers it registers.

- [ ] **Step 2: Move the file**

```bash
git mv crates/vmux_desktop/src/settings_view.rs crates/vmux_settings/src/view.rs
```

- [ ] **Step 3: Register the module in vmux_settings**

In `crates/vmux_settings/src/lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod view;
```

Re-export the view's public API as needed. The combined `SettingsPlugin` either lives in `view.rs` (renamed `SettingsPlugin` covering both runtime+view by adding `SettingsRuntimePlugin` inside), or in a new top-level file like `crates/vmux_settings/src/plugin.rs` that wraps both.

Choose the simpler option: in `view.rs`, change `SettingsPlugin::build` to ALSO add the runtime plugin from Task 4:

```rust
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::runtime::SettingsRuntimePlugin);
        // ... existing view-side registrations (SettingsView, broadcast, observer)
    }
}
```

Re-export it from `lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use view::SettingsPlugin;
```

(Replaces the runtime-only `SettingsPlugin` re-export from Task 4 if you added one there.)

- [ ] **Step 4: Adjust imports inside view.rs**

The moved file currently imports `crate::settings::*`, `crate::themes::*` (relative to desktop). Update:

```rust
// before (when file was in desktop)
use crate::settings::AppSettings;
use crate::themes::Theme;

// after
use crate::runtime::AppSettings;
use crate::themes::Theme;
```

(Theme stays at `crate::themes` because both files are now in the same crate.)

- [ ] **Step 5: Drop the desktop module declaration**

In `crates/vmux_desktop/src/lib.rs`:

```rust
mod settings_view;
```

→ delete. In the `use { ... }` block at the top of `VmuxPlugin::build`:

```rust
use {
    // ...
    settings_view::SettingsPlugin,  // <-- delete
    // ...
};
```

Replace with:

```rust
use vmux_settings::SettingsPlugin;
```

The `.add_plugins(SettingsPlugin)` call stays the same — only the import path changes.

- [ ] **Step 6: Update all in-desktop callers of settings_view types**

```bash
rg -nF 'crate::settings_view' crates/vmux_desktop/src/
```

Rewrite to `vmux_settings::view::...` (or whatever re-exports you set up).

- [ ] **Step 7: Verify and commit**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done

git add -u crates/
git add crates/vmux_settings/src/view.rs
git commit -m "refactor(settings): move settings_view into vmux_settings and unify SettingsPlugin"
```

---

## Phase 2 — Message types for cross-crate dispatch

### Task 6: Add `BrowserNavigateRequest` / `TerminalSendRequest` / `RunShellRequest` messages

**Files:**
- Create: `crates/vmux_layout/src/request.rs` (or extend `crates/vmux_layout/src/lib.rs` — `LayoutSpawnRequest` already lives there; add the new messages alongside)
- Create: `crates/vmux_terminal/src/runtime_messages.rs` (or extend `plugin.rs`)
- Modify: consumers in `vmux_desktop` (browser.rs, agent.rs's BrowserNavigate arm, terminal.rs's TerminalSend handler)

The owner crates produce the message types; consumers stay in their current homes for this task and will move in Tasks 7–9.

- [ ] **Step 1: Add `BrowserNavigateRequest` in `vmux_layout`**

`vmux_layout` is the right home because it already owns `LayoutSpawnRequest::OpenUrl` which is the analogous "open something here" message. Edit `crates/vmux_layout/src/lib.rs` (next to the existing `LayoutSpawnRequest` enum, around line 117):

```rust
#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct BrowserNavigateRequest {
    pub url: String,
    pub pane: Option<String>,
}
```

Register the message in `LayoutPlugin::build`:

```rust
app.add_message::<BrowserNavigateRequest>();
```

(Add alongside the existing `.add_message::<LayoutSpawnRequest>()`.)

- [ ] **Step 2: Add `TerminalSendRequest` and `RunShellRequest` in `vmux_terminal`**

Edit `crates/vmux_terminal/src/plugin.rs` (or create a new submodule under `crates/vmux_terminal/src/`):

```rust
#[derive(Message, Clone)]
pub struct TerminalSendRequest {
    pub text: String,
    pub terminal: Option<String>,
}

#[derive(Message, Clone)]
pub struct RunShellRequest {
    pub command: String,
    pub cwd: String,
    pub mode: vmux_service::protocol::AgentShellMode,
}
```

(`AgentShellMode` may live in `vmux_service::protocol` — adjust import to match its actual location.)

Register them in `TerminalPlugin::build`:

```rust
app.add_message::<TerminalSendRequest>();
app.add_message::<RunShellRequest>();
```

- [ ] **Step 3: Re-route the existing handlers in `vmux_desktop`**

For now, KEEP the consumer logic in `vmux_desktop` (it moves crates in Tasks 7–9). Just change the agent dispatch in `crates/vmux_desktop/src/agent.rs::handle_agent_commands` to write the new messages instead of inline-handling:

```rust
ServiceAgentCommand::BrowserNavigate { url, pane } => {
    browser_nav_writer.write(vmux_layout::BrowserNavigateRequest { url, pane });
    AgentCommandResult::Ok
}
ServiceAgentCommand::TerminalSend { text, terminal } => {
    terminal_send_writer.write(vmux_terminal::TerminalSendRequest { text, terminal });
    AgentCommandResult::Ok
}
ServiceAgentCommand::RunShell { command, cwd, mode } => {
    run_shell_writer.write(vmux_terminal::RunShellRequest { command, cwd, mode });
    AgentCommandResult::Ok
}
```

Add the corresponding `MessageWriter` parameters to `handle_agent_commands`:

```rust
mut browser_nav_writer: MessageWriter<vmux_layout::BrowserNavigateRequest>,
mut terminal_send_writer: MessageWriter<vmux_terminal::TerminalSendRequest>,
mut run_shell_writer: MessageWriter<vmux_terminal::RunShellRequest>,
```

- [ ] **Step 4: Add desktop-side consumer systems**

Move the existing inline dispatch bodies (the ones that previously ran inside `handle_agent_commands::BrowserNavigate`/`TerminalSend`/`RunShell` arms) into new systems in their current locations:

- `vmux_desktop::browser` gets `fn handle_browser_navigate_requests(reader: MessageReader<BrowserNavigateRequest>, ...)` doing what the inline BrowserNavigate arm did.
- `vmux_desktop::terminal` gets `fn handle_terminal_send_requests(...)` and `fn handle_run_shell_requests(...)`.

Register these in their respective plugins (`BrowserPlugin`, `TerminalInputPlugin`).

The body of each handler is verbatim moved from the `agent.rs` arm — same logic, just driven by a message instead of an inline match.

- [ ] **Step 5: Verify and commit**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Manual smoke: launch the app, run an MCP `browser_navigate https://example.com pane=pane:N` (or via the in-app agent prompt), confirm the page loads. Same for `terminal_send` and `run_shell`.

```bash
git add -u crates/
git commit -m "feat: add Browser/Terminal/RunShell request messages for cross-crate dispatch"
```

---

## Phase 3 — `vmux_command` extraction

### Task 7: Move `command_bar.rs` into `vmux_command`

**Files:**
- Create: `crates/vmux_command/src/command_bar.rs` (the moved content)
- Delete: `crates/vmux_desktop/src/command_bar.rs`
- Delete: `crates/vmux_desktop/src/command.rs` (1 line — fold into vmux_command if needed, drop otherwise)
- Modify: `crates/vmux_command/src/lib.rs` (add `pub mod command_bar;`)
- Modify: `crates/vmux_command/Cargo.toml` (add deps: `vmux_agent`, `vmux_terminal`, `vmux_layout` etc. — see step 3)
- Modify: `crates/vmux_desktop/src/lib.rs` (drop `mod command_bar;`, drop `mod command;`)
- Modify: every caller (`crate::command_bar::...` → `vmux_command::command_bar::...`)

- [ ] **Step 1: Inventory the source file's deps**

```bash
wc -l crates/vmux_desktop/src/command_bar.rs
head -50 crates/vmux_desktop/src/command_bar.rs
rg -nF 'use crate::' crates/vmux_desktop/src/command_bar.rs
```

Catalog every `use crate::X::Y` import — these become `use vmux_X::Y` or stay as desktop-internal after the move (the latter implies a cycle and means a different decoupling is needed).

Expected cross-crate deps after move:
- `vmux_agent::strategy::AgentStrategies`
- `vmux_terminal::Terminal`
- `vmux_settings::AppSettings`
- `vmux_layout::{pane::*, stack::*, space::*}` (Space after Task 2)
- `vmux_desktop::browser::Browser` (← problem: this would make `vmux_command → vmux_desktop`, a cycle)
- `vmux_desktop::spaces::ActiveSpace` (← same cycle concern)
- `vmux_desktop::processes_monitor::ProcessesMonitor`

- [ ] **Step 2: Resolve the cycle hazard**

`vmux_command` cannot depend on `vmux_desktop`. For each desktop-resident type that `command_bar.rs` references, choose ONE of:

(a) **Component-only access** — query the entity via its layout component (e.g., query the `Stack` and inspect its children for `Browser`/`Terminal` markers without needing the `Browser` type itself).
(b) **Re-route via message** — instead of directly invoking a desktop-side function, write a message that the desktop consumer handles.
(c) **Move the desktop type to a shared crate** — usually overkill for this PR.

For `Browser`: command_bar likely just CHECKS `With<Browser>` in queries. The marker can be a unit struct re-exported from `vmux_layout::chrome::Browser` (which already exists for the chrome webview) — verify whether it suits or whether a new shared `Browser` marker should land in `vmux_layout` (probably not in this PR).

If marker check is the only use: leave the Browser type in desktop AND have command_bar's queries use a different marker (e.g., add `pub struct ContentBrowser;` in `vmux_layout` and tag content browsers when they spawn — too invasive for this PR).

**Pragmatic compromise:** for the duration of this PR, move command_bar BUT keep the few systems that reference desktop-only types in `vmux_desktop`. Concretely:

- Move the bulk of command_bar.rs (the UI, the modal management, the payload broadcast) to `vmux_command`.
- Identify the ~3-5 functions that touch `Browser`/`Terminal`/`ProcessesMonitor` directly and either:
  - replace direct type access with message writes consumed in desktop, OR
  - leave those specific helpers in a thin `vmux_desktop::command_bar_glue.rs` file.

If the cycle resists clean resolution, STOP and report BLOCKED with the specific function(s) — do not work around with a bidirectional dep edge.

- [ ] **Step 3: Update `vmux_command/Cargo.toml`**

After the dep audit above, add the needed `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` entries:

```toml
vmux_agent = { path = "../vmux_agent" }
vmux_terminal = { path = "../vmux_terminal" }
vmux_settings = { path = "../vmux_settings" }
vmux_layout = { path = "../vmux_layout" }
```

Verify no cycle:

```bash
cargo tree -p vmux_command -e=no-build,no-dev 2>&1 | grep -i 'vmux_desktop'
```

Expected: no `vmux_desktop` in the output.

- [ ] **Step 4: Move the file**

```bash
git mv crates/vmux_desktop/src/command_bar.rs crates/vmux_command/src/command_bar.rs
```

If `crates/vmux_desktop/src/command.rs` is empty / a stub:

```bash
git rm crates/vmux_desktop/src/command.rs
```

- [ ] **Step 5: Register the module**

In `crates/vmux_command/src/lib.rs` (under the existing `cfg(not(target_arch = "wasm32"))` block):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod command_bar;
```

Re-export the plugin:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use command_bar::CommandBarPlugin;
```

(Rename `CommandBarInputPlugin` to `CommandBarPlugin` if the existing name has "Input" in it — the plugin owns more than just input now.)

- [ ] **Step 6: Rewrite imports inside the moved command_bar.rs**

For every `use crate::X::Y` line, replace with `use vmux_X::Y` (settings → `vmux_settings`, terminal → `vmux_terminal`, agent → `vmux_agent`, layout → `vmux_layout`). For desktop-resident types that survived step 2's cycle resolution, change to the chosen alternative (component query, message write, or leave a glue function in desktop).

- [ ] **Step 7: Update desktop callers**

```bash
rg -nF 'crate::command_bar' crates/vmux_desktop/src/
rg -nF 'crate::command::' crates/vmux_desktop/src/
```

Rewrite to `vmux_command::command_bar::...` or `vmux_command::...`.

In `crates/vmux_desktop/src/lib.rs`:

```rust
mod command_bar;       // <-- delete
mod command;           // <-- delete (if vmux_desktop::command was the 1-line stub)
```

Add `vmux_command::CommandBarPlugin` to the `add_plugins` list (or update if `CommandBarInputPlugin` was renamed).

- [ ] **Step 8: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Manual smoke: launch the app, cmd+k (or whatever opens the command bar), confirm the bar opens, type a command, confirm it dispatches.

- [ ] **Step 9: Commit**

```bash
git add -u crates/
git add crates/vmux_command/src/command_bar.rs
git commit -m "refactor(command): move command_bar into vmux_command"
```

---

## Phase 4 — `vmux_terminal` extraction

### Task 8: Move `terminal.rs` + `terminal/launch.rs` + `terminal/pid.rs` into `vmux_terminal`

**Files:**
- Create: `crates/vmux_terminal/src/runtime.rs` (the moved `terminal.rs`)
- Create: `crates/vmux_terminal/src/runtime/launch.rs` (the moved `terminal/launch.rs`)
- Create: `crates/vmux_terminal/src/runtime/pid.rs` (the moved `terminal/pid.rs`)
- Delete: `crates/vmux_desktop/src/terminal.rs`, `crates/vmux_desktop/src/terminal/launch.rs`, `crates/vmux_desktop/src/terminal/pid.rs`
- Modify: `crates/vmux_terminal/src/lib.rs` (register `runtime` and submodules)
- Modify: `crates/vmux_terminal/Cargo.toml` (add deps: `vmux_settings`, `vmux_layout`, `vmux_service`, `bevy_cef`, `rfd`, etc.)
- Modify: every caller

- [ ] **Step 1: Inventory deps in the source files**

```bash
wc -l crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/terminal/launch.rs crates/vmux_desktop/src/terminal/pid.rs
rg -nF 'use crate::' crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/terminal/*.rs
rg -nF 'use vmux_' crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/terminal/*.rs
```

Expected cross-crate deps:
- `vmux_settings::AppSettings` (now lives in vmux_settings after Phase 1)
- `vmux_layout::{LayoutSpawnRequest, CloseRequiresConfirmation, window::WEBVIEW_MESH_DEPTH_BIAS}`
- `vmux_service::{client::{ServiceHandle, ServiceWake}, protocol::{ClientMessage, ProcessId, ServiceMessage}}`
- `vmux_history::LastActivatedAt`
- `vmux_webview_app::UiReady`
- `vmux_agent::session::AgentSession`, `vmux_agent::strategy::AgentStrategies`, `vmux_agent::AgentKind`
- `vmux_desktop::browser::Browser` (← potential cycle, mostly used inside `spawn_url_into_stack`)
- `vmux_desktop::spaces::ActiveSpace` (← potential cycle)
- `vmux_desktop::processes_monitor::ProcessesMonitor` (← potential cycle)

- [ ] **Step 2: Resolve the cycle hazard for `spawn_url_into_stack`**

The function dispatches on URL prefix to spawn `Terminal` / `Agent` / `Browser` / `ProcessesMonitor` / `SpacesView`. After the move:

- Terminal branch: stays in `vmux_terminal`.
- Agent branch: needs `vmux_agent` (no cycle).
- Browser branch: needs `vmux_desktop::browser::Browser` → cycle.
- ProcessesMonitor branch: needs `vmux_desktop::processes_monitor::ProcessesMonitor` → cycle.
- SpacesView branch: needs `vmux_desktop::spaces::SpacesView` → cycle.

**Resolution:** split `spawn_url_into_stack` along the cycle boundary. `vmux_terminal::spawn_url_into_stack` handles ONLY `vmux://terminal/` URLs. Everything else gets routed via a new message:

```rust
#[derive(Message, Clone)]
pub struct SpawnNonTerminalUrlRequest {
    pub stack: Entity,
    pub url: String,
}
```

…which `vmux_desktop` consumes in a new system that does the remaining dispatch (Browser / Agent / ProcessesMonitor / SpacesView).

Define the message in `vmux_layout::lib.rs` (alongside `LayoutSpawnRequest`) so both `vmux_terminal` (producer when it doesn't recognize the URL) and `vmux_desktop` (consumer) can see it without new edges.

Update `spawn_layout_requested_content`'s `LayoutSpawnRequest::OpenUrl` arm to detect terminal URLs locally and write `SpawnNonTerminalUrlRequest` for everything else.

If the cycle resists this resolution too, STOP and report BLOCKED — do not introduce a bidirectional dep.

- [ ] **Step 3: Update `vmux_terminal/Cargo.toml`**

Add only the deps actually referenced after the move. Expected additions (verify against actual usage):

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
vmux_settings = { path = "../vmux_settings" }
vmux_layout = { path = "../vmux_layout" }
vmux_service = { path = "../vmux_service" }
vmux_history = { path = "../vmux_history" }
vmux_agent = { path = "../vmux_agent" }
bevy_cef = { workspace = true }
rfd = { workspace = true }
notify = { workspace = true }
libc = "0.2"
```

Verify no cycle:

```bash
cargo tree -p vmux_terminal -e=no-build,no-dev 2>&1 | grep -i 'vmux_desktop'
```

Expected: no `vmux_desktop` in the output.

- [ ] **Step 4: Move the files**

```bash
mkdir -p crates/vmux_terminal/src/runtime
git mv crates/vmux_desktop/src/terminal.rs crates/vmux_terminal/src/runtime.rs
git mv crates/vmux_desktop/src/terminal/launch.rs crates/vmux_terminal/src/runtime/launch.rs
git mv crates/vmux_desktop/src/terminal/pid.rs crates/vmux_terminal/src/runtime/pid.rs
rmdir crates/vmux_desktop/src/terminal 2>/dev/null || true
```

- [ ] **Step 5: Register modules in vmux_terminal**

In `crates/vmux_terminal/src/lib.rs` add (inside the non-wasm cfg block alongside the existing `plugin.rs` include):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
```

In `crates/vmux_terminal/src/runtime.rs` add (near the top):

```rust
pub mod launch;
pub mod pid;
```

Re-export the public API from `lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use runtime::{
    ServiceClient, Terminal, TerminalInputPlugin, TerminalRuntimePlugin,
    // ... whatever vmux_desktop currently re-exports/uses
};
```

If a top-level umbrella plugin exists or needs to: define `TerminalRuntimePlugin` (combines TerminalInputPlugin + the existing TerminalPlugin webview registration).

- [ ] **Step 6: Rewrite imports inside the moved files**

For every `use crate::X::Y` in `runtime.rs`/`runtime/launch.rs`/`runtime/pid.rs`:
- `crate::settings::...` → `vmux_settings::...`
- `crate::browser::Browser` → either gone (replaced by message dispatch from step 2) or use the upcoming `vmux_layout::chrome::Browser` if applicable
- `crate::layout::...` → `vmux_layout::...`
- `crate::spaces::ActiveSpace` → if still needed inside vmux_terminal, replace with a message-based handoff (probably not — terminal shouldn't care about ActiveSpace except for `space_dir(active.record.id)` in cwd resolution; resolve via a `Res<ActiveSpace>` reads-only and have desktop pass it through a shared accessor, OR move ActiveSpace into vmux_layout / vmux_core if needed)
- `crate::command::...` → `vmux_command::...`
- `crate::agent::...` → `vmux_agent::...`
- `crate::terminal::pid::*` → `crate::runtime::pid::*` (sibling module)
- `crate::terminal::launch::*` → `crate::runtime::launch::*`

- [ ] **Step 7: Update desktop callers**

```bash
rg -nF 'crate::terminal' crates/vmux_desktop/src/
```

Rewrite to `vmux_terminal::runtime::...` (or whatever was re-exported from vmux_terminal::lib).

In `crates/vmux_desktop/src/lib.rs`:

```rust
mod terminal;          // <-- delete
```

Add `vmux_terminal::TerminalRuntimePlugin` to the plugin list (if not already added via the existing `TerminalPlugin` umbrella).

- [ ] **Step 8: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Manual smoke: launch the app, open a terminal pane, type, confirm input flows. Quit confirmation dialog still works. Process exit on shell `exit` still closes the pane.

- [ ] **Step 9: Commit**

```bash
git add -u crates/
git add crates/vmux_terminal/src/runtime.rs crates/vmux_terminal/src/runtime/launch.rs crates/vmux_terminal/src/runtime/pid.rs
git commit -m "refactor(terminal): move terminal runtime into vmux_terminal"
```

---

## Phase 5 — `vmux_agent` extraction

### Task 9: Move `agent.rs` and the `GetSettings`/`ReadLayout` query arms into `vmux_agent`

**Files:**
- Create: `crates/vmux_agent/src/desktop_dispatch.rs` (the moved `agent.rs` content)
- Modify: `crates/vmux_settings/src/runtime.rs` or `view.rs` (to consume the `AgentQuery::GetSettings` request — see below)
- Delete: `crates/vmux_desktop/src/agent.rs`
- Delete: `crates/vmux_desktop/src/agent_query.rs`
- Modify: `crates/vmux_agent/src/lib.rs` (register the new module + plugin)
- Modify: `crates/vmux_agent/Cargo.toml` (add deps: `vmux_layout`, `vmux_settings`, `vmux_terminal`, `vmux_command`, etc.)
- Modify: `crates/vmux_desktop/src/lib.rs` (drop `mod agent;`, `mod agent_query;`, register the new `vmux_agent::AgentDispatchPlugin`)

- [ ] **Step 1: Inventory deps in agent.rs + agent_query.rs**

```bash
wc -l crates/vmux_desktop/src/agent.rs crates/vmux_desktop/src/agent_query.rs
rg -nF 'use crate::' crates/vmux_desktop/src/agent.rs crates/vmux_desktop/src/agent_query.rs
rg -nF 'use vmux_' crates/vmux_desktop/src/agent.rs crates/vmux_desktop/src/agent_query.rs
```

Expected: heavy cross-crate references. After the move, the `crate::` becomes `vmux_<owner>::` for each.

- [ ] **Step 2: Split `agent_query.rs`**

The file currently handles two `AgentQuery` variants:
- `ReadLayout` — already routed via `LayoutSnapshotRequest` after VMX-121 (T13).
- `GetSettings` — still inline; should move to `vmux_settings` since it owns the settings.

Move the `GetSettings` arm into a new system in `crates/vmux_settings/src/view.rs` (or a dedicated `query.rs`):

```rust
pub(crate) fn handle_get_settings_query(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<vmux_terminal::ServiceClient>>,
    settings: Res<AppSettings>,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        if matches!(request.query, AgentQuery::GetSettings) {
            let result = AgentQueryResult::Settings(serialize_settings_to_json(&settings));
            service.0.send(ClientMessage::AgentQueryResponse {
                request_id: request.request_id,
                result,
            });
        }
    }
}
```

Register in `SettingsPlugin::build`. Note this requires `vmux_settings` to depend on `vmux_terminal` (for `ServiceClient`) and `vmux_service` (for protocol types) and `vmux_agent` (for `AgentQueryRequest`). Verify no cycle:

```bash
cargo tree -p vmux_settings -e=no-build,no-dev 2>&1 | grep -i 'vmux_settings'
```

(Should only print `vmux_settings v0.0.x` once.)

If the cycle is unavoidable (e.g., `vmux_agent → vmux_settings → vmux_agent`), instead move `AgentQueryRequest` / `AgentCommandRequest` into `vmux_service::protocol` so the dep is `vmux_settings → vmux_service` (no cycle, since `vmux_service` doesn't depend on `vmux_agent`). Pick this fallback if direct vmux_settings → vmux_agent introduces a cycle.

- [ ] **Step 3: Move `agent.rs` into `vmux_agent`**

```bash
git mv crates/vmux_desktop/src/agent.rs crates/vmux_agent/src/desktop_dispatch.rs
```

- [ ] **Step 4: Rewrite imports inside `desktop_dispatch.rs`**

Mechanical rewrite of every `use crate::X::Y`:
- `crate::command::...` → `vmux_command::...`
- `crate::layout::*` → `vmux_layout::*`
- `crate::settings::AppSettings` → `vmux_settings::AppSettings`
- `crate::terminal::*` → `vmux_terminal::*`
- `crate::browser::Browser` → (if still referenced directly) use a message; otherwise drop.
- `crate::spaces::ActiveSpace` → consider passing via message or moving ActiveSpace to a shared crate. If desktop is the only ActiveSpace producer, the agent can read it via `Res<ActiveSpace>` only if `vmux_agent` depends on a crate that exports it — likely cleanest to move `ActiveSpace` into `vmux_space` as part of this task (small move).
- `crate::agent_layout::*` → deleted in VMX-121 (no-op).

For sibling references inside the now-relocated file: `use crate::session::*`, `use crate::strategy::*` etc. work as-is because `vmux_agent` is the new `crate`.

- [ ] **Step 5: Register the new plugin**

The moved file likely defines `AgentPlugin`. Re-export it from `crates/vmux_agent/src/lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod desktop_dispatch;

#[cfg(not(target_arch = "wasm32"))]
pub use desktop_dispatch::AgentDispatchPlugin;  // or whatever the plugin is named
```

Rename `AgentPlugin` if there's a collision with existing `AgentSessionPlugin` / `AppAgentPlugin`. Pick a name that conveys "desktop-side dispatch of agent commands" — e.g., `AgentDispatchPlugin`.

- [ ] **Step 6: Update `vmux_agent/Cargo.toml`**

Add deps actually referenced (verify against `use` statements):

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
vmux_command = { path = "../vmux_command" }
vmux_layout = { path = "../vmux_layout" }
vmux_settings = { path = "../vmux_settings" }
vmux_terminal = { path = "../vmux_terminal" }
vmux_service = { path = "../vmux_service" }
vmux_history = { path = "../vmux_history" }
bevy_cef = { workspace = true }
```

Verify no cycle:

```bash
cargo tree -p vmux_agent -e=no-build,no-dev 2>&1 | grep -i 'vmux_agent'
```

(`vmux_agent` should appear exactly once — at the root.)

- [ ] **Step 7: Drop agent_query.rs (now redundant)**

After step 2 moved the `GetSettings` arm and VMX-121's `LayoutSnapshotRequest` handles `ReadLayout`, `agent_query.rs` has nothing left. Delete:

```bash
git rm crates/vmux_desktop/src/agent_query.rs
```

In `crates/vmux_desktop/src/lib.rs` delete `mod agent_query;`.

- [ ] **Step 8: Update desktop wiring**

In `crates/vmux_desktop/src/lib.rs`:

```rust
mod agent;             // <-- delete
mod agent_query;       // <-- delete (done in step 7)
```

Add `vmux_agent::AgentDispatchPlugin` to the plugin list. Drop any `AgentPlugin` import from `crate::agent`.

- [ ] **Step 9: Re-evaluate `layout_response.rs`**

`crates/vmux_desktop/src/layout_response.rs` was added in VMX-121 to forward `LayoutApplyResponse`/`LayoutSnapshotResponse` to the service client. After agent moves to its own crate, the forwarder is the only desktop-side bridge for layout responses.

Decision: keep `layout_response.rs` in desktop for now (it's the right home if desktop is the "outer shell" coordinating IPC). Skip this step if no changes are needed.

If logic naturally fits in `vmux_agent::desktop_dispatch.rs`, fold it in and delete `layout_response.rs`.

- [ ] **Step 10: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Manual smoke: launch the app, open the Vibe agent, run `read_layout` and `update_layout` via MCP, confirm the layout updates. Try `get_settings` and `update_settings`.

- [ ] **Step 11: Commit**

```bash
git add -u crates/
git add crates/vmux_agent/src/desktop_dispatch.rs
git commit -m "refactor(agent): move agent dispatch into vmux_agent"
```

---

## Phase 6 — Wrap-up

### Task 10: Final verification + PR description update

**Files:**
- Delete: `docs/plans/2026-05-19-domain-crate-extraction.md` (per AGENTS.md — delete plan once implemented)

- [ ] **Step 1: Full workspace check**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 2: Verify `vmux_desktop` shrank**

```bash
wc -l crates/vmux_desktop/src/*.rs
ls crates/vmux_desktop/src/
```

Expected modules remaining in `vmux_desktop/src/`: `lib.rs`, `main.rs`, `browser.rs`, `spaces.rs`, `processes_monitor.rs`, `persistence.rs`, `background_lifecycle.rs`, `os_menu.rs`, `tray.rs`, `updater.rs`, `clipboard.rs`, `shortcut.rs`, `profile.rs`, `scene.rs`, `layout_response.rs` (if kept). No `agent*.rs`, no `command*.rs`, no `settings*.rs`, no `terminal*.rs`, no `themes.rs`.

- [ ] **Step 3: Manual end-to-end smoke**

```bash
env -u CEF_PATH cargo run -p vmux_desktop 2>&1 | tee /tmp/vmx122.log
```

Walk through:
- App launches; existing saved layout loads (Space rename didn't break persistence).
- Open command bar (cmd+k or whatever the shortcut is). Type a command. Dispatch.
- Open a terminal pane. Type. Verify input.
- Open the Vibe agent. Trigger `read_layout` and `update_layout` via the agent UI.
- Change a setting via the settings view. Confirm it persists.
- Quit.

Scan `/tmp/vmx122.log` for any `WARN`/`ERROR` lines that didn't exist before this PR.

- [ ] **Step 4: Push and update PR description**

```bash
git push
```

Update PR #50's description (or create a new PR if a different branch was set up for VMX-122). New description sections:
- Final dep graph (paste from `cargo tree` output).
- Crate ownership table (which crate owns which file group).
- Reference to both companion cleanups (Tab→Space, PROCESSES→SERVICES).
- Test plan checklist.

(If using a tool like `gh pr edit 50 --body ...`, paste the assembled description in.)

- [ ] **Step 5: Delete the plan file**

Per AGENTS.md: "Delete the plan file once the plan is fully implemented."

```bash
git rm docs/plans/2026-05-19-domain-crate-extraction.md
git commit -m "chore: remove implemented plan"
git push
```

---

## Done

After this plan executes:
- `vmux_desktop` is a thin desktop-shell crate.
- `vmux_settings`, `vmux_command`, `vmux_terminal`, `vmux_agent` each own their domain logic behind a single `Plugin`.
- Cross-crate communication is via Bevy `Message` types (mirrors VMX-121's pattern).
- `vmux_layout::Tab` (component) is renamed to `vmux_layout::Space` — the `Tab` (window-equivalent) level is reserved for a future PR.
- `PROCESSES_WEBVIEW_URL` → `SERVICES_WEBVIEW_URL`, defined once in `vmux_service::webview::event`.
