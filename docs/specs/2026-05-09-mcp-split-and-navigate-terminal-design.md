# MCP `split_and_navigate` vmux:// URL Support — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended)

## Goal

Extend the `split_and_navigate` MCP tool to recognize the three established `vmux://` protocol URLs and spawn the matching internal entity instead of a generic browser webview. No new MCP tool — same composite, just smarter routing.

## Why

Agents need to "open a terminal on the right" with the same atomic / no-focus-race guarantee that `split_and_navigate` provides for URLs. The vmux protocol already uses dedicated `vmux://` URLs for internal views (terminal, sessions, processes/services). Reusing those conventions lets agents request the right view via a familiar URL form without adding three more MCP tools.

## Approach

In `vmux_desktop::agent::handle_agent_commands::SplitAndNavigate` arm, after `split_pane_in_two` returns the new `pane2`, dispatch by URL prefix:

| URL prefix             | Spawn helper                                            |
|------------------------|---------------------------------------------------------|
| `vmux://terminal/`     | `spawn_terminal_tab(pane2, None, None, ...)`            |
| `vmux://sessions/`     | new `spawn_sessions_tab(pane2, ...)` (mirrors terminal) |
| `vmux://services/`     | new `spawn_processes_tab(pane2, ...)` (mirrors terminal)|
| Unknown `vmux://...`   | `AgentCommandResult::Error("split_and_navigate: unknown vmux URL '<url>'")` |
| Anything else          | existing `spawn_browser_tab(pane2, url, ...)` path      |

Two new spawn helpers in `agent.rs` mirror the existing `spawn_terminal_tab` / `spawn_browser_tab` pattern.

Update the `McpParamTool::SplitAndNavigate` description so agents know about the special URL forms.

## Changes

### `vmux_desktop::agent::handle_agent_commands::SplitAndNavigate` arm

Replace the unconditional `spawn_browser_tab(pane2, url, ...)` call (added in the previous wave) with prefix-based dispatch:

```rust
const VMUX_PREFIX: &str = "vmux://";
const VMUX_TERMINAL: &str = "vmux://terminal/";
const VMUX_SESSIONS: &str = "vmux://sessions/";
const VMUX_SERVICES: &str = "vmux://services/";

if url.starts_with(VMUX_TERMINAL) {
    spawn_terminal_tab(
        pane2, None, None,
        &mut commands, &mut meshes, &mut webview_mt, &settings,
    );
    AgentCommandResult::Ok
} else if url.starts_with(VMUX_SESSIONS) {
    spawn_sessions_tab(pane2, &mut commands, &mut meshes, &mut webview_mt);
    AgentCommandResult::Ok
} else if url.starts_with(VMUX_SERVICES) {
    spawn_processes_tab(pane2, &mut commands, &mut meshes, &mut webview_mt);
    AgentCommandResult::Ok
} else if url.starts_with(VMUX_PREFIX) {
    AgentCommandResult::Error(format!(
        "split_and_navigate: unknown vmux URL '{url}'"
    ))
} else {
    spawn_browser_tab(pane2, url, &mut commands, &mut meshes, &mut webview_mt);
    AgentCommandResult::Ok
}
```

Constants can also reference the existing `vmux_layout::event::TERMINAL_WEBVIEW_URL` / `vmux_session::event::SESSIONS_WEBVIEW_URL` / `vmux_layout::event::PROCESSES_WEBVIEW_URL` rather than hard-coded strings — pick whichever is cleanest.

### New spawn helpers in `vmux_desktop::agent`

```rust
pub(crate) fn spawn_sessions_tab(
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((crate::layout::tab::tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: vmux_session::event::SESSIONS_WEBVIEW_URL.to_string(),
        title: "Sessions".to_string(),
        ..default()
    });
    commands.spawn((
        crate::sessions::SessionsView::new(meshes, webview_mt),
        ChildOf(tab),
    ));
    tab
}

pub(crate) fn spawn_processes_tab(
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((crate::layout::tab::tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: vmux_layout::event::PROCESSES_WEBVIEW_URL.to_string(),
        title: "Background Services".to_string(),
        ..default()
    });
    commands.spawn((
        crate::processes_monitor::ProcessesMonitor::new(meshes, webview_mt),
        ChildOf(tab),
    ));
    tab
}
```

Both follow the existing `spawn_terminal_tab` / `spawn_browser_tab` pattern. Constructors `SessionsView::new` and `ProcessesMonitor::new` already exist with the same `(meshes, webview_mt)` signature. Verify exact import paths during implementation.

### `vmux_mcp::tools::McpParamTool::SplitAndNavigate` description

Current: `"Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom."`

New:

```
Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom. URLs starting with 'vmux://terminal/' open a terminal, 'vmux://sessions/' opens the sessions view, 'vmux://services/' opens the processes monitor; other 'vmux://' URLs are rejected; everything else opens as a browser.
```

### Tests

`vmux_desktop::agent::tests`:

- `split_and_navigate_with_terminal_url_spawns_terminal` — focused pane + `SplitAndNavigate { direction: "right", url: "vmux://terminal/" }`. Assert PaneSplit on original pane and a `Terminal` entity exists.
- `split_and_navigate_with_sessions_url_spawns_sessions_view` — same shape, asserts `SessionsView` entity.
- `split_and_navigate_with_processes_url_spawns_processes_monitor` — same shape, asserts `ProcessesMonitor` entity.
- `split_and_navigate_with_unknown_vmux_url_errors` — send `vmux://nonsense/`, assert command result is Error.
- Existing `split_and_navigate_creates_split_and_browser_tab` (from prior wave) still passes for non-`vmux://` URLs.

## Out of Scope

- Passing custom args via vmux:// query strings (`vmux://terminal/?cwd=/path`). YAGNI.
- A separate `split_and_terminal` named tool. Decided against — vmux:// URLs cover it.
- Recognizing vmux:// in other tools (e.g. `browser_navigate`). Future ticket if useful.

## Risks

- **URL normalization**: We use `starts_with` against the full `vmux://terminal/` prefix (with trailing slash). Strict — `vmux://terminal` (no slash) currently goes to the unknown-vmux branch and errors. Alternative: relax to `starts_with("vmux://terminal")` for ergonomics. Pick the relaxed version in the implementation.
- **Helper proliferation in `agent.rs`**: two new spawn helpers (`spawn_sessions_tab`, `spawn_processes_tab`) plus the existing `spawn_terminal_tab` / `spawn_browser_tab`. Clean grouping; if this list grows further, extract into a `spawn_tab_for_url(url, ...)` dispatcher.
- **Cross-crate dependency surface**: `agent.rs` will reference `vmux_session::event::SESSIONS_WEBVIEW_URL` and `crate::sessions::SessionsView`, plus `vmux_layout::event::PROCESSES_WEBVIEW_URL` and `crate::processes_monitor::ProcessesMonitor`. All paths already exist; just need imports added.

## File Map

- **Modify** `crates/vmux_desktop/src/agent.rs` — add `spawn_sessions_tab` and `spawn_processes_tab` helpers; update `SplitAndNavigate` arm with vmux:// dispatch; add 4 new tests; update existing browser test if needed.
- **Modify** `crates/vmux_mcp/src/tools.rs` — update the `SplitAndNavigate` `#[mcp(description = ...)]` string.
