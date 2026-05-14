# Vmux.app as Background Item Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Linear:** [VMX-119](https://linear.app/vmux/issue/VMX-119/ship-vmuxapp-as-signed-background-item-embed-vmux-service-helper)

**Goal:** Ship `Vmux.app` as a signed, notarized background item; embed `vmux_service` inside the bundle as a launchd-supervised helper. Login Items shows "Vmux" with proper icon + Developer ID team. UI process can be Cmd+Q'd / window-closed without killing terminal state. Replaces the current path-based `~/Library/LaunchAgents/ai.vmux.service.*.plist` registration.

**Architecture:**
- `Vmux.app/Contents/MacOS/vmux_desktop` — UI process, hidden from Dock via `LSUIElement=YES` (already set elsewhere — verify), tray-resident when window closed
- `Vmux.app/Contents/MacOS/vmux_service` — daemon binary, supervised by launchd via embedded plist
- `Vmux.app/Contents/Library/LaunchAgents/ai.vmux.service.plist` — embedded plist, uses `BundleProgram` (relative) instead of absolute `ProgramArguments`
- `SMAppService.mainApp.register()` registers the .app as a login item from inside the .app on first run
- `SMAppService.agent(plistName:).register()` registers the embedded daemon
- Dev-mode (cargo run, not bundled) keeps the existing `crates/vmux_service/src/launchd.rs` `ensure_running` path so iteration is fast — SMAppService refuses to register apps outside `/Applications`

**Tech Stack:** Rust, bevy, `objc2` + `objc2-foundation` for Service Management framework FFI, `cargo-packager` for bundling, `codesign` + `notarytool`, `launchctl` (dev fallback only).

**Pattern reference:** OrbStack, Docker Desktop, Tailscale all ship as signed app bundles registered via SMAppService.

---

## File Structure (final state)

```
crates/vmux_service/
├── Cargo.toml                       # +objc2, +objc2-foundation (macos only)
└── src/
    ├── lib.rs                       # cfg-gated `pub mod sm_app_service` + `pub mod bundle`
    ├── launchd.rs                   # unchanged; used as dev-mode fallback
    ├── sm_app_service.rs            # NEW: SMAppService FFI (register/unregister/status, mainApp + agent)
    ├── bundle.rs                    # NEW: detect "running inside .app", resolve bundle paths, embedded plist name
    └── service_registration.rs      # NEW: dispatcher — picks SMAppService when bundled, launchd::ensure_running otherwise

crates/vmux_desktop/
├── Cargo.toml                       # unchanged
└── src/
    ├── lib.rs                       # WindowPlugin: ExitCondition::DontExit; add BackgroundLifecyclePlugin
    ├── background_lifecycle.rs      # NEW: Cmd+Q → hide windows; window close → hide; SMAppService first-launch register
    ├── tray.rs                      # implement: status icon + "Show Vmux" / "Quit Vmux" menu items
    ├── os_menu.rs                   # handle_quit_request → hide instead of AppExit
    └── terminal.rs:608-650          # ensure_service_started → call new dispatcher in vmux_service

packaging/macos/
├── Info.plist                       # +LSUIElement=YES (verify)
└── ai.vmux.service.plist            # NEW: embedded plist template (BundleProgram + KeepAlive)

scripts/
├── package.sh                       # +build vmux_service in --release, copy plist into bundle
└── before-each-package.sh           # ensure plist + helper binary land in bundle before signing
```

---

## Task Group A — UI Lifecycle (Vmux.app survives Cmd+Q)

Goal: Vmux.app stays running in the background after Cmd+Q or window close. Tray menu provides explicit "Quit Vmux" action.

This group is independently mergeable and delivers value on its own (no terminal-state loss on accidental Cmd+Q), even before the bundle/SMAppService work lands.

### Task A1: Block app exit on last-window-close

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs:84-88` (WindowPlugin construction)

- [ ] **Step 1: Write the failing test**

Add to `crates/vmux_desktop/src/lib.rs` test module:

```rust
#[test]
fn window_plugin_keeps_app_alive_after_last_window_closes() {
    use bevy::window::ExitCondition;
    let source = include_str!("lib.rs");
    assert!(
        source.contains("ExitCondition::DontExit"),
        "WindowPlugin must opt out of automatic exit so Vmux.app survives last-window-close"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
env -u CEF_PATH cargo test -p vmux_desktop window_plugin_keeps_app_alive
```

Expected: FAIL — `ExitCondition::DontExit` not in source.

- [ ] **Step 3: Implement**

In `crates/vmux_desktop/src/lib.rs`:

```rust
// Add to imports near line 31:
use bevy::window::{CompositeAlphaMode, ExitCondition, Window as NativeWindow, WindowPlugin};

// Replace lines 84-88:
let window_plugin = WindowPlugin {
    primary_window: Some(primary_window),
    close_when_requested: false,
    exit_condition: ExitCondition::DontExit,
    ..default()
};
```

- [ ] **Step 4: Run test to verify it passes**

```bash
env -u CEF_PATH cargo test -p vmux_desktop window_plugin_keeps_app_alive
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/lib.rs
git commit -m "VMX-119: WindowPlugin opts out of last-window-close exit"
```

---

### Task A2: Cmd+Q hides windows instead of exiting the App

**Files:**
- Modify: `crates/vmux_desktop/src/os_menu.rs:74-92` (`handle_quit_request`)
- Test: `crates/vmux_desktop/src/os_menu.rs` (test module)

- [ ] **Step 1: Write the failing test**

Add to test module in `crates/vmux_desktop/src/os_menu.rs`:

```rust
#[test]
fn quit_menu_event_hides_windows_not_exit() {
    let source = include_str!("os_menu.rs");
    assert!(
        !source.contains("AppExit::Success"),
        "Cmd+Q must hide windows, not exit the app — terminal state must survive"
    );
    assert!(
        source.contains("HideAllWindows") || source.contains("window.visible = false"),
        "handle_quit_request must dispatch a hide action"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
env -u CEF_PATH cargo test -p vmux_desktop quit_menu_event_hides_windows_not_exit
```

Expected: FAIL — source still contains `AppExit::Success`.

- [ ] **Step 3: Define tray-internal lifecycle events (NOT new AppCommand variants)**

`AppCommand` (in `crates/vmux_command/src/command.rs`) uses derive macros (`Message`, `OsMenu`, `CommandBar`, `McpTool`) that require menu metadata for every variant. Adding `HideAllWindows`/`QuitVmux` there would force them into OS menus + command bar where they don't belong.

Instead, introduce a separate Bevy `Message` for lifecycle in the new `background_lifecycle` module:

```rust
// in crates/vmux_desktop/src/background_lifecycle.rs (full file shown in Step 4)
#[derive(bevy::prelude::Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    ShowAllWindows,
    QuitVmux,
}
```

In `crates/vmux_desktop/src/os_menu.rs`, replace `handle_quit_request` body (lines 74-92):

```rust
fn handle_quit_request(world: &mut World) {
    // Cmd+Q hides the UI but keeps Vmux.app + vmux_service alive.
    // Real quit is only available via the tray menu.
    world
        .resource_mut::<Messages<crate::background_lifecycle::LifecycleEvent>>()
        .write(crate::background_lifecycle::LifecycleEvent::HideAllWindows);
}
```

- [ ] **Step 4: Implement BackgroundLifecyclePlugin**

Create `crates/vmux_desktop/src/background_lifecycle.rs`:

```rust
use bevy::prelude::*;
use bevy::window::Window;

#[derive(Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    ShowAllWindows,
    QuitVmux,
}

pub struct BackgroundLifecyclePlugin;

impl Plugin for BackgroundLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LifecycleEvent>();
        app.add_systems(Update, handle_lifecycle_events);
    }
}

fn handle_lifecycle_events(
    mut events: MessageReader<LifecycleEvent>,
    mut windows: Query<&mut Window>,
    mut exit: MessageWriter<AppExit>,
) {
    for event in events.read() {
        match event {
            LifecycleEvent::HideAllWindows => {
                for mut window in &mut windows {
                    window.visible = false;
                }
            }
            LifecycleEvent::ShowAllWindows => {
                for mut window in &mut windows {
                    window.visible = true;
                }
            }
            LifecycleEvent::QuitVmux => {
                // Live-terminal confirm dialog is added in Task A5.
                exit.write(AppExit::Success);
            }
        }
    }
}
```

Register in `crates/vmux_desktop/src/lib.rs`:

```rust
mod background_lifecycle;
use background_lifecycle::BackgroundLifecyclePlugin;

// inside VmuxPlugin::build, in the second .add_plugins((...)) tuple (line ~120):
.add_plugins(BackgroundLifecyclePlugin)
```

- [ ] **Step 5: Run test to verify it passes**

```bash
env -u CEF_PATH cargo test -p vmux_desktop quit_menu_event_hides_windows
```

Expected: PASS.

- [ ] **Step 6: Manual smoke test**

```bash
make dev
# In running Vmux: press Cmd+Q. Window should disappear but process should stay alive.
ps aux | grep vmux_desktop  # should still be listed
```

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/src/{os_menu.rs,background_lifecycle.rs,lib.rs}
git commit -m "VMX-119: Cmd+Q hides windows, terminal state survives"
```

---

### Task A3: Window red-button close also hides instead of despawning

**Files:**
- Modify: `crates/vmux_desktop/src/os_menu.rs:97-119` (`close_with_confirmation`)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn window_close_request_hides_window_instead_of_despawning() {
    let source = include_str!("os_menu.rs");
    assert!(
        source.contains("window.visible = false")
            || source.contains("HideAllWindows"),
        "WindowCloseRequested must hide the window so Vmux.app stays in the background"
    );
}
```

- [ ] **Step 2: Verify it fails**

```bash
env -u CEF_PATH cargo test -p vmux_desktop window_close_request_hides_window
```

- [ ] **Step 3: Replace despawn-on-close with hide-on-close**

In `crates/vmux_desktop/src/os_menu.rs`, replace the `for event in closed.read()` block inside `close_with_confirmation` (around lines 109-118):

```rust
for event in closed.read() {
    // We do NOT despawn the window — Vmux.app lives in the background and
    // the same window entity is re-shown when the user reopens from tray.
    if let Ok(mut window) = windows.get_mut(event.window) {
        window.visible = false;
    }
}
```

You'll need to add `mut windows: Query<&mut Window>` to the system signature. Remove the `closing: Query<Entity, With<ClosingWindow>>` despawn loop (lines 105-108) and the `ClosingWindow` insert in `process_pending_window_close` (line 137) — replace with `window.visible = false` similarly.

The terminal-running confirm dialog (lines 110-117) stays — but on confirm we hide instead of despawn.

- [ ] **Step 4: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --all-targets
```

- [ ] **Step 5: Manual smoke test**

```bash
make dev
# Click red button on window. Window disappears, process keeps running.
# (No way to re-show yet without tray — that comes in Task A4.)
```

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/os_menu.rs
git commit -m "VMX-119: window close hides instead of despawning"
```

---

### Task A4: Tray menu with "Show Vmux" + "Quit Vmux"

**Files:**
- Rewrite: `crates/vmux_desktop/src/tray.rs` (currently a placeholder, see lines 1-25)

The `tray-icon` crate (already implied by stub comment) integrates with winit's event loop. We'll use `tray-icon = "0.21"` (latest as of 2026-05; confirm before adding).

- [ ] **Step 1: Add `tray-icon` dependency**

```bash
cd crates/vmux_desktop
cargo add tray-icon@0.21
cd ../..
```

Verify it builds:

```bash
env -u CEF_PATH cargo check -p vmux_desktop
```

- [ ] **Step 2: Write failing test (build-time check)**

In `crates/vmux_desktop/src/tray.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn tray_module_not_a_placeholder() {
        let source = include_str!("tray.rs");
        assert!(
            source.contains("TrayIconBuilder") || source.contains("tray_icon::TrayIcon"),
            "tray.rs must wire tray-icon, not be a stub"
        );
        assert!(
            source.contains("Quit Vmux") || source.contains("QuitVmux"),
            "tray must expose a 'Quit Vmux' menu item"
        );
        assert!(
            source.contains("Show Vmux") || source.contains("ShowVmux"),
            "tray must expose a 'Show Vmux' menu item"
        );
    }
}
```

- [ ] **Step 3: Verify it fails**

```bash
env -u CEF_PATH cargo test -p vmux_desktop tray_module_not_a_placeholder
```

- [ ] **Step 4: Implement tray**

Replace contents of `crates/vmux_desktop/src/tray.rs`:

```rust
use bevy::prelude::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::background_lifecycle::LifecycleEvent;

pub(crate) struct TrayPlugin;

#[derive(Resource)]
struct TrayHandle {
    _tray: TrayIcon,
    show_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tray);
        app.add_systems(Update, drain_menu_events);
    }
}

fn setup_tray(mut commands: Commands) {
    let menu = Menu::new();
    let show = MenuItem::new("Show Vmux", true, None);
    let quit = MenuItem::new("Quit Vmux", true, None);
    let show_id = show.id().clone();
    let quit_id = quit.id().clone();
    if let Err(e) = menu.append_items(&[&show, &quit]) {
        tracing::error!(error = %e, "failed to append tray menu items");
        return;
    }

    let icon = load_tray_icon();
    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Vmux")
        .with_icon(icon)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to build tray icon");
            return;
        }
    };

    commands.insert_resource(TrayHandle {
        _tray: tray,
        show_id,
        quit_id,
    });
}

fn drain_menu_events(
    handle: Option<Res<TrayHandle>>,
    mut events: MessageWriter<LifecycleEvent>,
) {
    let Some(handle) = handle else { return };
    let receiver = MenuEvent::receiver();
    while let Ok(event) = receiver.try_recv() {
        if event.id == handle.show_id {
            events.write(LifecycleEvent::ShowAllWindows);
        } else if event.id == handle.quit_id {
            events.write(LifecycleEvent::QuitVmux);
        }
    }
}

fn load_tray_icon() -> tray_icon::Icon {
    // 16x16 placeholder; embed real icon bytes when icon design lands.
    let rgba = vec![0u8; 16 * 16 * 4];
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("valid placeholder rgba")
}
```

Register `TrayPlugin` in `lib.rs`:

```rust
.add_plugins(TrayPlugin)
```

- [ ] **Step 5: Verify tests pass**

```bash
env -u CEF_PATH cargo test -p vmux_desktop tray_module_not_a_placeholder
env -u CEF_PATH cargo test -p vmux_desktop --all-targets
```

- [ ] **Step 6: Manual smoke test**

```bash
make dev
# Look at menu bar — should see Vmux tray item
# Cmd+Q the window → still alive → click tray "Show Vmux" → window reappears
# Click tray "Quit Vmux" → confirm dialog → app exits
```

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/src/{tray.rs,lib.rs} crates/vmux_desktop/Cargo.toml Cargo.lock
git commit -m "VMX-119: tray menu with Show / Quit Vmux"
```

---

### Task A5: Confirm-quit dialog now lives only in tray quit path

**Files:**
- Modify: `crates/vmux_desktop/src/background_lifecycle.rs`

Move the live-terminal confirm dialog (currently in `os_menu.rs handle_quit_request`) to the `QuitVmux` branch — Cmd+Q (now "hide") doesn't need confirmation, but tray quit does.

- [ ] **Step 1: Implement**

The confirm-dialog call needs `&mut World` (it has to query terminals + show a modal native dialog on the main thread). Convert `handle_lifecycle_events` into an exclusive system, mirroring the pattern in `os_menu.rs handle_quit_request`:

```rust
fn handle_lifecycle_events(world: &mut World) {
    use crate::terminal::{self, Terminal, PtyExited};

    let drained: Vec<LifecycleEvent> = {
        let mut events = world.resource_mut::<Messages<LifecycleEvent>>();
        events.drain().collect()
    };

    for event in drained {
        match event {
            LifecycleEvent::HideAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) { w.visible = false; }
            }
            LifecycleEvent::ShowAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) { w.visible = true; }
            }
            LifecycleEvent::QuitVmux => {
                let live = {
                    let mut q = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
                    q.iter(world).count()
                };
                if live > 0 && !terminal::confirm_quit_dialog(live) {
                    continue;
                }
                world.resource_mut::<Messages<AppExit>>().write(AppExit::Success);
            }
        }
    }
}
```

Update `BackgroundLifecyclePlugin::build` to register the system as exclusive (`add_systems(Update, handle_lifecycle_events)` works with `&mut World` parameter — bevy infers it).

- [ ] **Step 2: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --all-targets
```

- [ ] **Step 3: Manual smoke test**

```bash
make dev
# Open a terminal that runs something, then click tray "Quit Vmux" → confirm dialog appears
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/background_lifecycle.rs
git commit -m "VMX-119: confirm-quit dialog moves to tray Quit path"
```

---

## Task Group B — Bundle vmux_service inside Vmux.app

Goal: `Vmux.app/Contents/MacOS/vmux_service` ships alongside `vmux_desktop`. Embedded launchd plist at `Vmux.app/Contents/Library/LaunchAgents/ai.vmux.service.plist` uses `BundleProgram` (relative).

### Task B1: Add vmux_service to cargo-packager binaries

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml:60-65` (packager metadata)

- [ ] **Step 1: Write the failing test**

Add to a new file `crates/vmux_desktop/tests/packaging_metadata.rs`:

```rust
//! Verify cargo-packager metadata embeds vmux_service in the bundle.

#[test]
fn packager_binaries_include_vmux_service() {
    let toml = include_str!("../Cargo.toml");
    assert!(
        toml.contains(r#"path = "vmux_service""#),
        "packager metadata must include vmux_service so it lands in Vmux.app/Contents/MacOS/"
    );
}

#[test]
fn before_packaging_command_builds_vmux_service() {
    let toml = include_str!("../Cargo.toml");
    let line = toml.lines()
        .find(|l| l.starts_with("before-packaging-command"))
        .expect("before-packaging-command line present");
    assert!(
        line.contains("vmux_service"),
        "before-packaging-command must build vmux_service: {line}"
    );
}
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --test packaging_metadata
```

Expected: both tests FAIL.

- [ ] **Step 3: Update Cargo.toml**

Edit `crates/vmux_desktop/Cargo.toml`:

```toml
before-packaging-command = "env -u CEF_PATH cargo build -p vmux_desktop -p vmux_cli -p vmux_service -p bevy_cef_debug_render_process --release"
binaries = [
    { path = "vmux_desktop", main = true },
    { path = "vmux" },
    { path = "vmux_service" },
]
```

- [ ] **Step 4: Verify tests pass**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --test packaging_metadata
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/{Cargo.toml,tests/packaging_metadata.rs}
git commit -m "VMX-119: package vmux_service binary inside Vmux.app"
```

---

### Task B2: Author embedded launchd plist template

**Files:**
- Create: `packaging/macos/ai.vmux.service.plist`
- Create: `crates/vmux_service/tests/embedded_plist.rs`

The embedded plist must use `BundleProgram` (relative path, resolved by launchd against the bundle root) instead of `ProgramArguments` with absolute paths.

- [ ] **Step 1: Write failing test**

Create `crates/vmux_service/tests/embedded_plist.rs`:

```rust
#[test]
fn embedded_plist_uses_bundle_program_relative_path() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(xml.contains("<key>BundleProgram</key>"),
        "embedded plist must use BundleProgram, not ProgramArguments");
    assert!(xml.contains("Contents/MacOS/vmux_service"),
        "BundleProgram path must be Contents/MacOS/vmux_service");
    assert!(!xml.contains("/usr/local/") && !xml.contains("$HOME"),
        "embedded plist must not reference absolute paths outside the bundle");
}

#[test]
fn embedded_plist_keeps_alive_on_crash() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(xml.contains("<key>KeepAlive</key>"));
    assert!(xml.contains("<key>Crashed</key>"));
}

#[test]
fn embedded_plist_label_matches_release_profile() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(xml.contains("<string>ai.vmux.service</string>"),
        "release builds must use the suffix-less label");
}

#[test]
fn embedded_plist_sets_build_profile_release() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(xml.contains("<key>VMUX_BUILD_PROFILE</key>"));
    // The packaging script substitutes {{PROFILE}} for local/release; here we assert template var present.
    assert!(xml.contains("{{PROFILE}}") || xml.contains("<string>release</string>"));
}
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_service --test embedded_plist
```

Expected: FAIL — file does not exist.

- [ ] **Step 3: Create plist**

Create `packaging/macos/ai.vmux.service.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>ai.vmux.service</string>
  <key>BundleProgram</key>
  <string>Contents/MacOS/vmux_service</string>
  <key>RunAtLoad</key>
  <false/>
  <key>KeepAlive</key>
  <dict>
    <key>Crashed</key>
    <true/>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>VMUX_BUILD_PROFILE</key>
    <string>{{PROFILE}}</string>
  </dict>
  <key>StandardOutPath</key>
  <string>/tmp/vmux-service.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/vmux-service.log</string>
</dict>
</plist>
```

(Log path will be rewritten by SMAppService runtime side later — for now `/tmp` is fine because launchd only reads `BundleProgram` and `EnvironmentVariables`; the daemon itself opens a real log via `log_path()`.)

- [ ] **Step 4: Verify tests pass**

```bash
env -u CEF_PATH cargo test -p vmux_service --test embedded_plist
```

- [ ] **Step 5: Commit**

```bash
git add packaging/macos/ai.vmux.service.plist crates/vmux_service/tests/embedded_plist.rs
git commit -m "VMX-119: embedded launchd plist with BundleProgram"
```

---

### Task B3: Copy plist into bundle during packaging

**Files:**
- Modify: `scripts/package.sh` (post-cargo-packager copy for local builds)
- Modify: `scripts/before-each-package.sh` (pre-sign copy for release builds)

**Ordering reference** (from reading `scripts/before-each-package.sh`):
- `cargo packager --formats app` builds the .app skeleton
- For `release`: `cargo packager` continues to dmg pass, which runs `before-each-package.sh` → `inject-cef.sh` → `sign-and-notarize.sh` → wraps .app into dmg
- For `local`: `package.sh` calls `inject-cef.sh` manually after the app pass; `make build-local` then calls `sign-and-notarize.sh` separately

The plist must land BEFORE `sign-and-notarize.sh` runs so the signature covers it. Two insertion points needed: (a) inside `before-each-package.sh` between `inject-cef.sh` and `sign-and-notarize.sh` for the release dmg pass, and (b) at the end of `package.sh` (after manual `inject-cef.sh`) for local builds — but BEFORE `make build-local` invokes signing.

- [ ] **Step 2: Write failing integration check**

Create `scripts/test-bundle-layout.sh`:

```bash
#!/usr/bin/env bash
# Asserts the .app bundle has the expected layout. Exits non-zero on failure.
set -euo pipefail
APP="${1:?usage: $0 <path-to-Vmux.app>}"

REQUIRED=(
    "Contents/MacOS/vmux_desktop"
    "Contents/MacOS/vmux"
    "Contents/MacOS/vmux_service"
    "Contents/Library/LaunchAgents/ai.vmux.service.plist"
    "Contents/Info.plist"
    "Contents/Resources/Vmux.icns"
)

for path in "${REQUIRED[@]}"; do
    if [[ ! -e "$APP/$path" ]]; then
        echo "MISSING: $APP/$path" >&2
        exit 1
    fi
done

# Verify embedded plist substitution happened
if grep -q '{{PROFILE}}' "$APP/Contents/Library/LaunchAgents/ai.vmux.service.plist"; then
    echo "Plist still has {{PROFILE}} placeholder — substitution did not run" >&2
    exit 1
fi

echo "OK: bundle layout correct"
```

```bash
chmod +x scripts/test-bundle-layout.sh
```

- [ ] **Step 3: Verify failure (no bundle yet)**

```bash
make build-local 2>&1 | tail -5
sha="$(git rev-parse --short HEAD)"
./scripts/test-bundle-layout.sh "target/release/Vmux ($sha).app"
```

Expected: FAIL — `vmux_service` and embedded plist are missing.

- [ ] **Step 4: Add plist-embed helper script (shared by both flows)**

Create `scripts/embed-launch-agent-plist.sh`:

```bash
#!/usr/bin/env bash
# Embed packaging/macos/ai.vmux.service.plist inside Vmux.app, substituting
# the build profile (and per-SHA label for local builds).
#
# Required env: VMUX_APP_BUNDLE, VMUX_BUILD_PROFILE
# Optional env: VMUX_GIT_HASH (required when VMUX_BUILD_PROFILE=local)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

: "${VMUX_APP_BUNDLE:?VMUX_APP_BUNDLE not set}"
: "${VMUX_BUILD_PROFILE:?VMUX_BUILD_PROFILE not set}"

PLIST_SRC="$ROOT/packaging/macos/ai.vmux.service.plist"
PLIST_DST="$VMUX_APP_BUNDLE/Contents/Library/LaunchAgents/ai.vmux.service.plist"

LABEL="ai.vmux.service"
if [[ "$VMUX_BUILD_PROFILE" == "local" ]]; then
    : "${VMUX_GIT_HASH:?VMUX_GIT_HASH must be set for local builds}"
    LABEL="ai.vmux.service.$VMUX_GIT_HASH"
fi

mkdir -p "$(dirname "$PLIST_DST")"
sed -e "s|{{PROFILE}}|$VMUX_BUILD_PROFILE|g" \
    -e "s|<string>ai.vmux.service</string>|<string>$LABEL</string>|" \
    "$PLIST_SRC" > "$PLIST_DST"
echo "==> Embedded launchd plist (label=$LABEL, profile=$VMUX_BUILD_PROFILE)"
```

```bash
chmod +x scripts/embed-launch-agent-plist.sh
```

- [ ] **Step 5: Wire into `before-each-package.sh` (release dmg pass)**

Edit `scripts/before-each-package.sh` to embed the plist after `inject-cef.sh` but before `sign-and-notarize.sh`:

```bash
"$ROOT/scripts/inject-cef.sh"

if [[ "${CARGO_PACKAGER_FORMAT:-}" == "dmg" ]]; then
    APP_BUNDLE="${VMUX_APP_BUNDLE:-$ROOT/target/release/Vmux.app}" \
        VMUX_APP_BUNDLE="${VMUX_APP_BUNDLE:-$ROOT/target/release/Vmux.app}" \
        "$ROOT/scripts/embed-launch-agent-plist.sh"
    APP_BUNDLE="${VMUX_APP_BUNDLE:-$ROOT/target/release/Vmux.app}" \
        "$ROOT/scripts/sign-and-notarize.sh"
fi
```

- [ ] **Step 6: Wire into `package.sh` (local pass)**

In `scripts/package.sh`, find the existing `if [[ "$PROFILE" == "local" && -d "$VMUX_APP_BUNDLE" ]]; then` block (right after `inject-cef.sh`) and append:

```bash
if [[ "$PROFILE" == "local" && -d "$VMUX_APP_BUNDLE" ]]; then
    echo "==> Injecting CEF into .app (local build)"
    CARGO_PACKAGER_FORMAT=dmg bash "$ROOT/scripts/inject-cef.sh"

    echo "==> Embedding launchd plist (local build)"
    VMUX_GIT_HASH="$SHA" "$ROOT/scripts/embed-launch-agent-plist.sh"
fi
```

(The `$SHA` variable is only set in the `local` branch of the case statement at the top of package.sh — it's already in scope here.)

- [ ] **Step 7: Verify bundle layout test passes**

```bash
make build-local
sha="$(git rev-parse --short HEAD)"
./scripts/test-bundle-layout.sh "target/release/Vmux ($sha).app"
```

Expected: `OK: bundle layout correct`.

- [ ] **Step 8: Verify codesign chain still passes**

The existing loop in `scripts/sign-and-notarize.sh:76` signs everything in `Contents/MacOS/`, so `vmux_service` will be signed automatically. No script change needed there.

```bash
make build-local
codesign --verify --deep --strict --verbose=2 "target/release/Vmux ($(git rev-parse --short HEAD)).app"
```

Expected: `Vmux ($SHA).app: valid on disk` and `satisfies its Designated Requirement`.

- [ ] **Step 9: Commit**

```bash
git add scripts/{package.sh,before-each-package.sh,embed-launch-agent-plist.sh,test-bundle-layout.sh}
git commit -m "VMX-119: embed vmux_service + plist into Vmux.app"
```

---

## Task Group C — SMAppService FFI

Goal: Rust wrapper around `SMAppService` Objective-C class to register/unregister `mainApp` and `agent` services.

API surface needed:
- `SMAppService.mainApp.register()` / `.unregister()` / `.status`
- `SMAppService.agent(plistName:).register()` / `.unregister()` / `.status`

We'll use `objc2` raw `msg_send!` macros — no dedicated `objc2-service-management` crate exists at time of writing (verify in Step 1).

### Task C1: Add objc2 dependencies and stub module

**Files:**
- Modify: `crates/vmux_service/Cargo.toml`
- Create: `crates/vmux_service/src/sm_app_service.rs`

- [ ] **Step 1: Verify no `objc2-service-management` crate exists**

```bash
cargo search objc2-service-management
```

If a maintained crate exists, prefer it and adjust the implementation steps accordingly. If not, proceed with raw `objc2`.

- [ ] **Step 2: Add deps**

```bash
cd crates/vmux_service
cargo add --target 'cfg(target_os = "macos")' objc2@0.6
cargo add --target 'cfg(target_os = "macos")' objc2-foundation@0.3
cd ../..
```

- [ ] **Step 3: Write failing test**

Create `crates/vmux_service/tests/sm_app_service_module.rs`:

```rust
#[cfg(target_os = "macos")]
#[test]
fn sm_app_service_module_exposes_register_main_app() {
    // Compile-time check via fn pointer: forces the type to exist.
    let _: fn() -> Result<(), vmux_service::sm_app_service::SmError> =
        vmux_service::sm_app_service::register_main_app;
}
```

- [ ] **Step 4: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_service --test sm_app_service_module
```

Expected: FAIL — module does not exist.

- [ ] **Step 5: Create stub module**

Create `crates/vmux_service/src/sm_app_service.rs`:

```rust
#![cfg(target_os = "macos")]

use std::fmt;

#[derive(Debug)]
pub enum SmError {
    NotEnabled,
    NotRegistered,
    RequiresApproval,
    Other(String),
}

impl fmt::Display for SmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnabled => write!(f, "SMAppService not enabled"),
            Self::NotRegistered => write!(f, "SMAppService not registered"),
            Self::RequiresApproval => write!(f, "SMAppService requires user approval"),
            Self::Other(s) => write!(f, "SMAppService: {s}"),
        }
    }
}

impl std::error::Error for SmError {}

pub fn register_main_app() -> Result<(), SmError> {
    Err(SmError::Other("not yet implemented".into()))
}

pub fn unregister_main_app() -> Result<(), SmError> {
    Err(SmError::Other("not yet implemented".into()))
}

pub fn register_agent(plist_name: &str) -> Result<(), SmError> {
    let _ = plist_name;
    Err(SmError::Other("not yet implemented".into()))
}

pub fn unregister_agent(plist_name: &str) -> Result<(), SmError> {
    let _ = plist_name;
    Err(SmError::Other("not yet implemented".into()))
}

pub enum Status {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

pub fn main_app_status() -> Status {
    Status::NotRegistered
}

pub fn agent_status(_plist_name: &str) -> Status {
    Status::NotRegistered
}
```

Add to `crates/vmux_service/src/lib.rs`:

```rust
#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod sm_app_service;
```

- [ ] **Step 6: Verify test passes**

```bash
env -u CEF_PATH cargo test -p vmux_service --test sm_app_service_module
```

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_service/Cargo.toml crates/vmux_service/src/{lib.rs,sm_app_service.rs} crates/vmux_service/tests/sm_app_service_module.rs Cargo.lock
git commit -m "VMX-119: SMAppService module skeleton + stub API"
```

---

### Task C2: Implement register_main_app via objc2 msg_send

**Files:**
- Modify: `crates/vmux_service/src/sm_app_service.rs`

`SMAppService.mainApp` is a class method returning the singleton for the currently-running app bundle. `.register()` signature: `func register() throws`.

- [ ] **Step 1: Write the failing integration test (cfg-gated, ignored by default — needs a real bundle)**

```rust
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the test binary to run from inside a signed .app in /Applications"]
fn register_main_app_returns_status() {
    use vmux_service::sm_app_service::{register_main_app, main_app_status, Status};
    let _ = register_main_app();
    assert!(matches!(
        main_app_status(),
        Status::Enabled | Status::RequiresApproval
    ));
}
```

This is `#[ignore]` because it can only pass inside the bundle. It documents intent.

- [ ] **Step 2: Implement `register_main_app`**

Replace the stub in `sm_app_service.rs`:

```rust
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSError, NSString};

fn sm_app_service_class() -> &'static AnyClass {
    AnyClass::get(c"SMAppService").expect("SMAppService class available; ensure ServiceManagement framework is linked")
}

fn map_ns_error(err: *mut NSError) -> SmError {
    if err.is_null() {
        return SmError::Other("nil NSError".into());
    }
    let msg: *mut NSString = unsafe { msg_send![err, localizedDescription] };
    if msg.is_null() {
        return SmError::Other("nil localizedDescription".into());
    }
    let s = unsafe { (*msg).to_string() };
    SmError::Other(s)
}

pub fn register_main_app() -> Result<(), SmError> {
    let cls = sm_app_service_class();
    let instance: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
    if instance.is_null() {
        return Err(SmError::Other("mainAppService returned nil".into()));
    }
    let mut error: *mut NSError = std::ptr::null_mut();
    let ok: bool = unsafe {
        msg_send![instance, registerAndReturnError: &mut error]
    };
    if ok { Ok(()) } else { Err(map_ns_error(error)) }
}

pub fn unregister_main_app() -> Result<(), SmError> {
    let cls = sm_app_service_class();
    let instance: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
    let mut error: *mut NSError = std::ptr::null_mut();
    let ok: bool = unsafe {
        msg_send![instance, unregisterAndReturnError: &mut error]
    };
    if ok { Ok(()) } else { Err(map_ns_error(error)) }
}
```

- [ ] **Step 3: Link ServiceManagement framework**

Create `crates/vmux_service/build.rs` (or extend if one exists):

```rust
fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
    }
}
```

(Verify `crates/vmux_service` doesn't already have a `build.rs` — it might, given webview build steps. If it does, append the println.)

- [ ] **Step 4: Verify it compiles**

```bash
env -u CEF_PATH cargo build -p vmux_service
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/{Cargo.toml,src/sm_app_service.rs,build.rs,tests/sm_app_service_module.rs}
git commit -m "VMX-119: implement SMAppService.mainApp register/unregister"
```

---

### Task C3: Implement register_agent + status query

**Files:**
- Modify: `crates/vmux_service/src/sm_app_service.rs`

`SMAppService.agent(plistName:)` returns an instance scoped to a specific embedded plist. The plist must live at `Contents/Library/LaunchAgents/<plistName>` inside the running .app.

- [ ] **Step 1: Implement `register_agent` / `unregister_agent`**

```rust
fn agent_instance(plist_name: &str) -> Result<*mut AnyObject, SmError> {
    let cls = sm_app_service_class();
    let ns_name = NSString::from_str(plist_name);
    let instance: *mut AnyObject = unsafe {
        msg_send![cls, agentServiceWithPlistName: &*ns_name]
    };
    if instance.is_null() {
        Err(SmError::Other(format!("no agent for plist {plist_name}")))
    } else {
        Ok(instance)
    }
}

pub fn register_agent(plist_name: &str) -> Result<(), SmError> {
    let instance = agent_instance(plist_name)?;
    let mut error: *mut NSError = std::ptr::null_mut();
    let ok: bool = unsafe {
        msg_send![instance, registerAndReturnError: &mut error]
    };
    if ok { Ok(()) } else { Err(map_ns_error(error)) }
}

pub fn unregister_agent(plist_name: &str) -> Result<(), SmError> {
    let instance = agent_instance(plist_name)?;
    let mut error: *mut NSError = std::ptr::null_mut();
    let ok: bool = unsafe {
        msg_send![instance, unregisterAndReturnError: &mut error]
    };
    if ok { Ok(()) } else { Err(map_ns_error(error)) }
}
```

- [ ] **Step 2: Implement status queries**

```rust
fn raw_status(instance: *mut AnyObject) -> Status {
    // SMAppServiceStatus enum:
    //   0 = NotRegistered, 1 = Enabled, 2 = RequiresApproval, 3 = NotFound
    let raw: i64 = unsafe { msg_send![instance, status] };
    match raw {
        0 => Status::NotRegistered,
        1 => Status::Enabled,
        2 => Status::RequiresApproval,
        _ => Status::NotFound,
    }
}

pub fn main_app_status() -> Status {
    let cls = sm_app_service_class();
    let instance: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
    if instance.is_null() { Status::NotFound } else { raw_status(instance) }
}

pub fn agent_status(plist_name: &str) -> Status {
    match agent_instance(plist_name) {
        Ok(instance) => raw_status(instance),
        Err(_) => Status::NotFound,
    }
}
```

- [ ] **Step 3: Verify build + clippy**

```bash
env -u CEF_PATH cargo build -p vmux_service
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_service/src/sm_app_service.rs
git commit -m "VMX-119: SMAppService agent register + status queries"
```

---

## Task Group D — Wire SMAppService at Runtime

Goal: `Vmux.app` registers itself on first launch. `ensure_service_started` dispatches to SMAppService when bundled, falls back to existing `launchd::ensure_running` for `cargo run` dev mode.

### Task D1: Bundle-detection helper

**Files:**
- Create: `crates/vmux_service/src/bundle.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/vmux_service/tests/bundle_detection.rs`:

```rust
use std::path::PathBuf;
use vmux_service::bundle;

#[test]
fn detects_bundled_when_exe_inside_app_macos() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
    assert!(bundle::is_bundled_path(&exe));
}

#[test]
fn detects_not_bundled_when_target_debug() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");
    assert!(!bundle::is_bundled_path(&exe));
}

#[test]
fn bundle_root_resolves_app_path() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
    assert_eq!(
        bundle::bundle_root_for(&exe).unwrap(),
        PathBuf::from("/Applications/Vmux.app")
    );
}

#[test]
fn bundle_root_none_when_not_bundled() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");
    assert!(bundle::bundle_root_for(&exe).is_none());
}
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_service --test bundle_detection
```

- [ ] **Step 3: Implement**

Create `crates/vmux_service/src/bundle.rs`:

```rust
use std::path::{Path, PathBuf};

/// True if `exe` lives at `<X>.app/Contents/MacOS/<binary>`.
pub fn is_bundled_path(exe: &Path) -> bool {
    bundle_root_for(exe).is_some()
}

/// Returns the `.app` root if `exe` lives inside a macOS bundle.
pub fn bundle_root_for(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?; // …/Contents/MacOS
    if parent.file_name()?.to_str()? != "MacOS" { return None; }
    let contents = parent.parent()?; // …/Contents
    if contents.file_name()?.to_str()? != "Contents" { return None; }
    let app = contents.parent()?; // …/X.app
    if app.extension()?.to_str()? == "app" { Some(app.to_path_buf()) } else { None }
}

/// Resolve bundle root for the currently-running executable.
pub fn current_bundle_root() -> Option<PathBuf> {
    bundle_root_for(&std::env::current_exe().ok()?)
}

/// True if the running process lives inside a `.app` bundle.
pub fn is_bundled() -> bool {
    current_bundle_root().is_some()
}

/// Plist filename of the embedded launchd agent (matches packaging/macos/ai.vmux.service.plist).
pub const EMBEDDED_AGENT_PLIST: &str = "ai.vmux.service.plist";
```

Register in `crates/vmux_service/src/lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;
```

- [ ] **Step 4: Verify tests pass**

```bash
env -u CEF_PATH cargo test -p vmux_service --test bundle_detection
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/{lib.rs,bundle.rs} crates/vmux_service/tests/bundle_detection.rs
git commit -m "VMX-119: bundle-detection helpers"
```

---

### Task D2: Service-registration dispatcher

**Files:**
- Create: `crates/vmux_service/src/service_registration.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs:608-650` (`ensure_service_started`)

- [ ] **Step 1: Write failing test**

Create `crates/vmux_service/tests/service_registration_dispatch.rs`:

```rust
use std::path::PathBuf;
use vmux_service::service_registration::{Backend, choose_backend};

#[test]
fn bundled_path_chooses_sm_app_service() {
    let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/vmux_service");
    assert!(matches!(choose_backend(&exe), Backend::SmAppService { .. }));
}

#[test]
fn unbundled_path_chooses_launchctl() {
    let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_service");
    assert!(matches!(choose_backend(&exe), Backend::Launchctl));
}
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_service --test service_registration_dispatch
```

- [ ] **Step 3: Implement dispatcher**

Create `crates/vmux_service/src/service_registration.rs`:

```rust
use std::path::{Path, PathBuf};

use crate::bundle;

#[derive(Debug)]
pub enum Backend {
    /// Bundled: register Vmux.app + agent via SMAppService.
    SmAppService { bundle_root: PathBuf },
    /// Not bundled: write per-profile plist into ~/Library/LaunchAgents and launchctl bootstrap.
    Launchctl,
}

pub fn choose_backend(exe: &Path) -> Backend {
    if let Some(root) = bundle::bundle_root_for(exe) {
        Backend::SmAppService { bundle_root: root }
    } else {
        Backend::Launchctl
    }
}

#[derive(Debug)]
pub enum RegistrationError {
    Io(std::io::Error),
    #[cfg(target_os = "macos")]
    SmAppService(crate::sm_app_service::SmError),
}

impl From<std::io::Error> for RegistrationError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}

#[cfg(target_os = "macos")]
impl From<crate::sm_app_service::SmError> for RegistrationError {
    fn from(e: crate::sm_app_service::SmError) -> Self { Self::SmAppService(e) }
}

/// Idempotent: ensure the daemon is registered and running.
/// Dispatches to the SMAppService path when bundled, launchctl otherwise.
pub fn ensure_running(profile: &str, exe: &Path) -> Result<(), RegistrationError> {
    match choose_backend(exe) {
        Backend::SmAppService { .. } => {
            #[cfg(target_os = "macos")]
            {
                crate::sm_app_service::register_main_app()?;
                crate::sm_app_service::register_agent(bundle::EMBEDDED_AGENT_PLIST)?;
                Ok(())
            }
            #[cfg(not(target_os = "macos"))]
            { Ok(()) }
        }
        Backend::Launchctl => {
            #[cfg(target_os = "macos")]
            { crate::launchd::ensure_running(profile, exe)?; Ok(()) }
            #[cfg(not(target_os = "macos"))]
            { let _ = (profile, exe); Ok(()) }
        }
    }
}
```

Add to `crates/vmux_service/src/lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod service_registration;
```

- [ ] **Step 4: Update call site in vmux_desktop**

In `crates/vmux_desktop/src/terminal.rs:619-626`, replace:

```rust
#[cfg(target_os = "macos")]
{
    let profile = vmux_service::current_profile();
    if let Err(e) = vmux_service::launchd::ensure_running(profile, &binary) {
        tracing::error!(error = %e, "launchd ensure_running failed");
    }
}
```

with:

```rust
#[cfg(target_os = "macos")]
{
    let profile = vmux_service::current_profile();
    if let Err(e) = vmux_service::service_registration::ensure_running(profile, &binary) {
        tracing::error!(error = ?e, "service registration failed");
    }
}
```

- [ ] **Step 5: Verify tests pass + workspace builds**

```bash
env -u CEF_PATH cargo test -p vmux_service --test service_registration_dispatch
env -u CEF_PATH cargo build -p vmux_desktop
env -u CEF_PATH cargo clippy -p vmux_service -p vmux_desktop --all-targets -- -D warnings
```

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/src/{lib.rs,service_registration.rs} crates/vmux_service/tests/service_registration_dispatch.rs crates/vmux_desktop/src/terminal.rs
git commit -m "VMX-119: dispatch SMAppService when bundled, launchctl otherwise"
```

---

## Task Group E — Migration of Legacy Plists

Goal: First launch of new Vmux.app removes any pre-existing `~/Library/LaunchAgents/ai.vmux.service.*.plist` and unloads them from launchd. Idempotent.

### Task E1: Legacy plist scanner + removal

**Files:**
- Create: `crates/vmux_service/src/legacy_plist_cleanup.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/vmux_service/tests/legacy_plist_cleanup.rs`:

```rust
use std::fs;
use vmux_service::legacy_plist_cleanup;

#[test]
fn finds_and_lists_legacy_plists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("ai.vmux.service.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("ai.vmux.service.dev.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("ai.vmux.service.abc1234.plist"), "<plist/>").unwrap();
    fs::write(dir.path().join("com.unrelated.app.plist"), "<plist/>").unwrap();

    let found = legacy_plist_cleanup::find_legacy_plists_in(dir.path()).unwrap();
    assert_eq!(found.len(), 3, "should find 3 vmux plists, ignoring unrelated: {found:?}");
}

#[test]
fn extracts_label_from_filename() {
    assert_eq!(
        legacy_plist_cleanup::label_from_filename("ai.vmux.service.dev.plist"),
        Some("ai.vmux.service.dev")
    );
    assert_eq!(
        legacy_plist_cleanup::label_from_filename("ai.vmux.service.plist"),
        Some("ai.vmux.service")
    );
    assert_eq!(
        legacy_plist_cleanup::label_from_filename("com.other.plist"),
        None
    );
}

#[test]
fn cleanup_removes_files() {
    let dir = tempfile::tempdir().unwrap();
    let plist = dir.path().join("ai.vmux.service.dev.plist");
    fs::write(&plist, "<plist/>").unwrap();
    assert!(plist.exists());

    legacy_plist_cleanup::remove_plist_files(&[plist.clone()]).unwrap();
    assert!(!plist.exists());
}

#[test]
fn cleanup_is_idempotent_when_no_files_present() {
    let dir = tempfile::tempdir().unwrap();
    let found = legacy_plist_cleanup::find_legacy_plists_in(dir.path()).unwrap();
    assert!(found.is_empty());
}
```

Add `tempfile` to dev-dependencies if not present:

```bash
cd crates/vmux_service
cargo add --dev tempfile
cd ../..
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_service --test legacy_plist_cleanup
```

- [ ] **Step 3: Implement module**

Create `crates/vmux_service/src/legacy_plist_cleanup.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

const LABEL_PREFIX: &str = "ai.vmux.service";

pub fn launch_agents_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Library/LaunchAgents"))
}

pub fn label_from_filename(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".plist")?;
    if stem == LABEL_PREFIX || stem.starts_with(&format!("{LABEL_PREFIX}.")) {
        Some(stem)
    } else {
        None
    }
}

pub fn find_legacy_plists_in(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() { return Ok(out); }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
        if label_from_filename(name).is_some() {
            out.push(path);
        }
    }
    Ok(out)
}

pub fn remove_plist_files(paths: &[PathBuf]) -> std::io::Result<()> {
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn bootout_label(label: &str) {
    let uid = unsafe { libc::getuid() };
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{label}")])
        .status();
}

/// Run once on first launch of new Vmux.app: bootout + delete every legacy plist.
pub fn cleanup_legacy_registrations() -> std::io::Result<usize> {
    let Some(dir) = launch_agents_dir() else { return Ok(0); };
    let paths = find_legacy_plists_in(&dir)?;
    #[cfg(target_os = "macos")]
    for path in &paths {
        if let Some(name) = path.file_name().and_then(|s| s.to_str())
            && let Some(label) = label_from_filename(name)
        {
            bootout_label(label);
        }
    }
    let count = paths.len();
    remove_plist_files(&paths)?;
    Ok(count)
}
```

Register in `crates/vmux_service/src/lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod legacy_plist_cleanup;
```

- [ ] **Step 4: Verify tests pass**

```bash
env -u CEF_PATH cargo test -p vmux_service --test legacy_plist_cleanup
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/{Cargo.toml,src/{lib.rs,legacy_plist_cleanup.rs}} crates/vmux_service/tests/legacy_plist_cleanup.rs Cargo.lock
git commit -m "VMX-119: legacy plist scanner + cleanup helpers"
```

---

### Task E2: Trigger cleanup on first launch of bundled Vmux.app

**Files:**
- Modify: `crates/vmux_service/src/service_registration.rs`

- [ ] **Step 1: Write the test**

Add to `crates/vmux_service/tests/service_registration_dispatch.rs`:

```rust
#[test]
fn ensure_running_calls_legacy_cleanup_for_sm_app_service_path() {
    // Compile-time check: cleanup_legacy_registrations symbol referenced from service_registration.
    let source = include_str!("../src/service_registration.rs");
    assert!(
        source.contains("cleanup_legacy_registrations"),
        "SmAppService branch must invoke legacy cleanup"
    );
}
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_service --test service_registration_dispatch ensure_running_calls_legacy_cleanup
```

- [ ] **Step 3: Wire cleanup into the SmAppService branch**

In `service_registration.rs`, expand the `Backend::SmAppService` arm:

```rust
Backend::SmAppService { .. } => {
    #[cfg(target_os = "macos")]
    {
        match crate::legacy_plist_cleanup::cleanup_legacy_registrations() {
            Ok(0) => {}
            Ok(n) => tracing::info!(removed = n, "removed legacy launchd plists"),
            Err(e) => tracing::warn!(error = %e, "legacy plist cleanup failed (continuing)"),
        }
        crate::sm_app_service::register_main_app()?;
        crate::sm_app_service::register_agent(bundle::EMBEDDED_AGENT_PLIST)?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    { Ok(()) }
}
```

- [ ] **Step 4: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_service --all-targets
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/service_registration.rs crates/vmux_service/tests/service_registration_dispatch.rs
git commit -m "VMX-119: clean up legacy plists on first SMAppService run"
```

---

## Task Group F — Verification & Polish

### Task F1: Add LSUIElement to Info.plist

**Files:**
- Modify: `packaging/macos/Info.plist`

`LSUIElement=YES` removes the Dock icon — Vmux lives in the menu bar tray, not the Dock. Without this, the user sees a Dock icon for an app they're "supposed" to be running in the background.

- [ ] **Step 1: Write the failing test**

Add to a new file `crates/vmux_desktop/tests/info_plist.rs`:

```rust
#[test]
fn info_plist_marks_app_as_ui_element() {
    let xml = include_str!("../../../packaging/macos/Info.plist");
    assert!(xml.contains("<key>LSUIElement</key>"));
    let after_key = xml.split("<key>LSUIElement</key>").nth(1).unwrap();
    assert!(
        after_key.trim_start().starts_with("<true/>"),
        "LSUIElement must be true so Vmux runs as menu-bar-only"
    );
}
```

- [ ] **Step 2: Verify failure**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --test info_plist
```

- [ ] **Step 3: Edit Info.plist**

Add to `packaging/macos/Info.plist` before `</dict>`:

```xml
	<key>LSUIElement</key>
	<true/>
```

- [ ] **Step 4: Verify test passes**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --test info_plist
```

- [ ] **Step 5: Manual smoke test**

```bash
make build-local
sha="$(git rev-parse --short HEAD)"
open "target/release/Vmux ($sha).app"
# No Dock icon should appear; tray icon visible in menu bar
```

- [ ] **Step 6: Commit**

```bash
git add packaging/macos/Info.plist crates/vmux_desktop/tests/info_plist.rs
git commit -m "VMX-119: LSUIElement=YES, run as menu-bar app"
```

---

### Task F2: Manual verification checklist

These are not automated — they must be performed against a real signed+notarized release build before merging.

- [ ] **Step 1: Full release build**

```bash
make build-release
```

- [ ] **Step 2: Install to /Applications**

```bash
cp -R target/release/Vmux.app /Applications/
```

- [ ] **Step 3: First launch**

```bash
open /Applications/Vmux.app
```

Expected:
- No Dock icon
- Tray icon appears in menu bar
- macOS may prompt: "Vmux added itself to your Login Items" (SMAppService UX)

- [ ] **Step 4: Check Login Items UI**

Open *System Settings → General → Login Items & Extensions*.

Expected:
- Under "App Background Activity": **Vmux** with proper icon (not "vmux_service" + blank document)
- No "Item from unidentified developer" string

- [ ] **Step 5: Verify daemon is running under launchd**

```bash
launchctl list | grep vmux
```

Expected: line containing `ai.vmux.service` with a non-zero PID and exit code `0` or `-`.

- [ ] **Step 6: Verify Cmd+Q does not kill the daemon**

```bash
# From inside Vmux: Cmd+Q
# Then:
ps aux | grep vmux_service | grep -v grep
```

Expected: `vmux_service` still running.

- [ ] **Step 7: Verify killing UI does not kill terminals**

```bash
killall vmux_desktop
ps aux | grep vmux_service | grep -v grep
launchctl list | grep vmux
```

Expected: `vmux_service` still alive (launchd KeepAlive), terminal state preserved.

- [ ] **Step 8: Reopen UI from tray, verify reattach**

Click tray "Show Vmux" → window appears → existing terminals visible.

- [ ] **Step 9: Quit cleanly via tray**

Click tray "Quit Vmux" → confirm dialog (if terminals running) → both `vmux_desktop` and `vmux_service` exit.

- [ ] **Step 10: Reboot test**

Reboot the Mac. After login, `Vmux.app` should auto-launch (because mainApp registration). Verify tray appears.

- [ ] **Step 11: Verify legacy plist cleanup**

```bash
ls ~/Library/LaunchAgents/ai.vmux.service.*.plist 2>/dev/null
```

Expected: no output (legacy plists removed by cleanup migration).

---

### Task F3: Update Homebrew cask reference

**Files:**
- Update vmux Homebrew tap (separate repo, out of this worktree) — note in PR description.

- [ ] **Step 1: Check current cask**

```bash
gh api repos/vmux-ai/homebrew-tap/contents/Casks/vmux.rb --jq '.content' | base64 -d | head -40
```

- [ ] **Step 2: Confirm cask installs Vmux.app to /Applications**

If the cask is already shipping the .app to `/Applications/`, no changes needed. If it's installing a bare binary, file a follow-up issue to convert to a `.app` cask. Either way, document in PR description.

- [ ] **Step 3: Note in PR body**

Add a "Distribution" section to the PR template summarizing: cask still installs to `/Applications/Vmux.app`, no Brewfile/cask change required.

---

### Task F4: Lint + test sweep on changed crates

- [ ] **Step 1: Run pre-commit checks per AGENTS.md**

```bash
BASE="main"
ROOT="$(git rev-parse --show-toplevel)"
CHANGED_PKGS=$(
  cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.manifest_path | test("patches") | not) | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
  | while IFS=$'\t' read -r name dir; do
      rel="${dir#"$ROOT"/}"; [ -z "$rel" ] && rel="."
      if ! git diff --quiet "$BASE" -- "$rel"; then echo "$name"; fi
    done
)

for pkg in $CHANGED_PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $CHANGED_PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $CHANGED_PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all green.

- [ ] **Step 2: Open PR**

```bash
linear issue pr
```

PR title: `VMX-119: Vmux.app as signed background item; embed vmux_service helper`

PR body should include:
- Link to Linear issue
- Architecture summary (copy from this plan's header)
- Distribution note (Task F3)
- Manual verification checklist results (Task F2)
- Known follow-ups: TCC entitlements for Microphone / Process Tap (separate ticket once this lands)

---

## Open Questions / Future Work

These are explicitly **out of scope** for this plan. File separate Linear issues if needed.

1. **Microphone / Screen Recording / Process Tap entitlements** — required for the planned transcript / meeting-notes feature. Add `NSMicrophoneUsageDescription` to Info.plist, declare entitlements, request user consent. Separate ticket.
2. **Settings UI for Login Items toggle** — let user disable Vmux from auto-launching at login via in-app Settings (calls `unregister_main_app()`). Today they can toggle it from System Settings.
3. **Linux equivalent** — systemd user unit instead of launchd. Out of scope for macOS-only ticket.
4. **`local` build SMAppService UX** — `cargo-packager` produces `Vmux ($SHA).app` at `target/release/`. SMAppService refuses to register apps outside `/Applications`. For local testing, either copy to `/Applications/` manually or accept that local builds use the launchctl fallback. Document in `make local` output.
5. **CEF helper bundle ID** — CEF Helper apps inside the bundle have their own bundle IDs (e.g. `org.cef.helper`). TCC permissions for screen capture etc. may need separate consideration when transcript feature lands.
