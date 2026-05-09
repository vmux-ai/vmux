# MCP vmux:// URL Support in `split_and_navigate` and `browser_navigate` — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended)

## Goal

Recognize the three established `vmux://` protocol URLs (terminal, sessions, services) in BOTH `split_and_navigate` AND `browser_navigate` MCP tools, spawning the matching internal entity instead of a generic browser webview. Support `?cwd=…` query parameter for `vmux://terminal/`. No new MCP tool — same composites, smarter routing.

## Why

Agents need to "open a terminal on the right" with the same atomic / no-focus-race guarantee that `split_and_navigate` provides for URLs. They also need to open a terminal in the focused pane (or a target pane) without splitting first — `browser_navigate(url='vmux://terminal/')` is the natural API for that. The vmux protocol already uses dedicated `vmux://` URLs for internal views (terminal, sessions, processes/services). Reusing those conventions in both navigation tools lets agents request the right view via a familiar URL form without adding more MCP tools.

## Approach

In `vmux_desktop::agent::handle_agent_commands::SplitAndNavigate` arm, after `split_pane_in_two` returns the new `pane2`, parse the URL with `url::Url::parse` and dispatch by host (the part after `vmux://`):

| Parsed `vmux://` host | Spawn helper                                                                |
|-----------------------|-----------------------------------------------------------------------------|
| `terminal`            | `spawn_terminal_tab(pane2, cwd_from_query, None, ...)` — `?cwd=...` extracted |
| `sessions`            | new `spawn_sessions_tab(pane2, ...)` (mirrors terminal)                     |
| `services`            | new `spawn_processes_tab(pane2, ...)` (mirrors terminal)                    |
| anything else         | `AgentCommandResult::Error("split_and_navigate: unknown vmux URL '<url>'")` |

Non-`vmux://` URLs continue to call `spawn_browser_tab(pane2, url, ...)` (existing behavior).

Two new spawn helpers in `agent.rs` mirror the existing `spawn_terminal_tab` / `spawn_browser_tab` pattern.

`vmux://terminal/?cwd=/Users/foo` extracts `cwd = "/Users/foo"` and passes it as `Some(Path::new("/Users/foo"))` to `spawn_terminal_tab`. Existing `valid_cwd` validation (used by `NewTerminalTab` handler) is reused — invalid path → Error.

`vmux://sessions/` and `vmux://services/` accept no query parameters today; any provided are silently ignored. (Future tools can add specific parameters as needed without breaking the URL convention.)

Update the `McpParamTool::SplitAndNavigate` description so agents know about the special URL forms.

## Changes

### Shared helper: `spawn_vmux_tab`

A single private dispatcher in `agent.rs` handles vmux:// URL spawning for ANY pane. Both `SplitAndNavigate` and `BrowserNavigate` call it.

```rust
fn spawn_vmux_tab(
    url: &str,
    pane: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<(), String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("invalid vmux URL '{url}': {e}"))?;
    let host = parsed.host_str().unwrap_or("");

    match host {
        "terminal" => {
            let cwd_param = parsed
                .query_pairs()
                .find(|(k, _)| k == "cwd")
                .map(|(_, v)| v.into_owned());
            let cwd_path = if let Some(c) = cwd_param.as_deref() {
                match valid_cwd(c) {
                    Ok(p) => p,
                    Err(message) => return Err(message),
                }
            } else {
                None
            };
            spawn_terminal_tab(
                pane,
                cwd_path.as_deref(),
                None,
                commands,
                meshes,
                webview_mt,
                settings,
            );
            Ok(())
        }
        "sessions" => {
            spawn_sessions_tab(pane, commands, meshes, webview_mt);
            Ok(())
        }
        "services" => {
            spawn_processes_tab(pane, commands, meshes, webview_mt);
            Ok(())
        }
        other => Err(format!(
            "unknown vmux URL host '{other}' in '{url}'"
        )),
    }
}
```

Error messages are generic ("invalid vmux URL", "unknown vmux URL host") — callers prepend their tool name (e.g. `format!("split_and_navigate: {message}")`) for context.

`url::Url` is already a workspace dependency (`url = "2"` in the root `Cargo.toml`). Add `url = { workspace = true }` to `crates/vmux_desktop/Cargo.toml` if not already present.

`valid_cwd` is the existing helper in `agent.rs` used by `NewTerminalTab`. Reused for consistency.

### `vmux_desktop::agent::handle_agent_commands::SplitAndNavigate` arm

Replace the unconditional `spawn_browser_tab(pane2, url, ...)` call (added in the previous wave) with vmux:// detection:

```rust
if url.starts_with("vmux://") {
    match spawn_vmux_tab(
        url,
        pane2,
        &mut commands,
        &mut meshes,
        &mut webview_mt,
        &settings,
    ) {
        Ok(()) => AgentCommandResult::Ok,
        Err(message) => AgentCommandResult::Error(format!("split_and_navigate: {message}")),
    }
} else {
    spawn_browser_tab(pane2, url, &mut commands, &mut meshes, &mut webview_mt);
    AgentCommandResult::Ok
}
```

### `vmux_desktop::agent::handle_agent_commands::BrowserNavigate` arm

The current handler (added in the MTD wave) has three branches:
1. Explicit pane provided → spawn_browser_tab in target pane.
2. No pane + active webview → trigger `RequestNavigate` (in-place URL change).
3. No pane + no webview but focused pane exists → spawn_browser_tab in focused pane.

Add vmux:// handling at the top: if URL starts with `vmux://`, ALWAYS create a new tab (the active-webview navigate path doesn't apply — vmux:// URLs need the right entity type, can't be navigated into a Browser entity). Restructure:

```rust
ServiceAgentCommand::BrowserNavigate { url, pane } => {
    let target_pane = if let Some(s) = pane.as_deref() {
        match parse_pane_target(s, &panes) {
            Some(target) => Some(target),
            None => {
                return AgentCommandResult::Error(format!(
                    "browser_navigate: invalid pane id '{s}'"
                ));
            }
        }
    } else {
        focus.pane.filter(|p| panes.contains(*p))
    };

    if url.starts_with("vmux://") {
        if let Some(pane_entity) = target_pane {
            return match spawn_vmux_tab(
                url,
                pane_entity,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
                &settings,
            ) {
                Ok(()) => AgentCommandResult::Ok,
                Err(message) => AgentCommandResult::Error(format!("browser_navigate: {message}")),
            };
        }
        return AgentCommandResult::Error(
            "browser_navigate: no focused pane for vmux URL".to_string(),
        );
    }

    if let Some(s) = pane.as_deref() {
        if let Some(target) = parse_pane_target(s, &panes) {
            spawn_browser_tab(target, url, &mut commands, &mut meshes, &mut webview_mt);
            AgentCommandResult::Ok
        } else {
            AgentCommandResult::Error(format!("browser_navigate: invalid pane id '{s}'"))
        }
    } else if let Some(webview) = active_webview_for_tab(focus.tab, &browsers, &terminals) {
        commands.trigger(RequestNavigate { webview, url: url.clone() });
        AgentCommandResult::Ok
    } else if let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) {
        spawn_browser_tab(pane, url, &mut commands, &mut meshes, &mut webview_mt);
        AgentCommandResult::Ok
    } else {
        AgentCommandResult::Error("browser_navigate: no focused pane".to_string())
    }
}
```

Note: the early-return `match parse_pane_target(...)` at the top duplicates work with the later branch. Refactor opportunity exists but the structure as written is correct. The implementer can collapse the duplicate `parse_pane_target` call if a clean factoring presents itself.

Behaviour:
- `browser_navigate(url='vmux://terminal/?cwd=/tmp', pane=<id>)` → spawn terminal in target pane.
- `browser_navigate(url='vmux://sessions/')` → spawn sessions view in focused pane.
- `browser_navigate(url='https://...', pane=<id>)` → existing behaviour, browser tab in target.
- `browser_navigate(url='https://...')` → existing behaviour (active webview navigate or auto-spawn).

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

### `vmux_mcp::tools::McpParamTool` description updates

**`SplitAndNavigate`** — replace description with:

```
Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom. URLs starting with 'vmux://terminal/' open a terminal (use '?cwd=/path' to set working dir), 'vmux://sessions/' opens the sessions view, 'vmux://services/' opens the processes monitor; other 'vmux://' URLs are rejected; everything else opens as a browser.
```

**`BrowserNavigate`** — replace description with:

```
Navigate the active webview to a URL, or open a URL in a target pane. URLs starting with 'vmux://terminal/' open a terminal (use '?cwd=/path' to set working dir), 'vmux://sessions/' opens the sessions view, 'vmux://services/' opens the processes monitor; other 'vmux://' URLs are rejected; everything else opens as a browser. With 'vmux://' URLs, a new tab is always created (the target pane is required, defaulting to the focused pane).
```

### Tests

`vmux_desktop::agent::tests` (split_and_navigate paths):

- `split_and_navigate_with_terminal_url_spawns_terminal` — `SplitAndNavigate { direction: "right", url: "vmux://terminal/" }`. Assert PaneSplit on original pane and a `Terminal` entity exists.
- `split_and_navigate_with_terminal_url_and_cwd_query_uses_cwd` — pass `vmux://terminal/?cwd=<existing-dir>`. Use `std::env::current_dir()` for a guaranteed-existing dir; assert no Error and a Terminal entity exists.
- `split_and_navigate_with_terminal_url_and_invalid_cwd_errors` — `vmux://terminal/?cwd=/this/does/not/exist`. Assert command result is Error.
- `split_and_navigate_with_sessions_url_spawns_sessions_view` — assert `SessionsView` entity.
- `split_and_navigate_with_processes_url_spawns_processes_monitor` — assert `ProcessesMonitor` entity.
- `split_and_navigate_with_unknown_vmux_url_errors` — send `vmux://nonsense/`. Assert Error.
- Existing `split_and_navigate_creates_split_and_browser_tab` (from prior wave) still passes for non-`vmux://` URLs.

`vmux_desktop::agent::tests` (browser_navigate vmux:// paths):

- `browser_navigate_with_terminal_url_spawns_terminal_in_focused_pane` — focused pane (no tabs); `BrowserNavigate { url: "vmux://terminal/", pane: None }`. Assert Terminal entity exists in focused pane.
- `browser_navigate_with_terminal_url_and_target_pane_uses_target` — two panes; focused = A, target = B (via `pane: Some(B.bits)`); URL `vmux://terminal/`. Assert Terminal in pane B, none in pane A.
- `browser_navigate_with_unknown_vmux_url_errors` — `vmux://nonsense/`. Assert Error.
- Existing `browser_navigate_*` tests (from earlier waves) still pass for non-`vmux://` URLs.

## Out of Scope

- A separate `split_and_terminal` named tool. Decided against — vmux:// URLs cover it.
- Recognizing vmux:// in tools beyond `split_and_navigate` and `browser_navigate` (e.g. `terminal_send` doesn't take URLs). Future ticket if useful.

## Risks

- **URL parser strictness**: `url::Url::parse` requires a valid URL. `vmux://terminal` (no slash) — does the parser accept it? Per RFC 3986, host-only URLs are valid. A quick check: `Url::parse("vmux://terminal")` returns Ok with `host_str() = Some("terminal")` and `path() = ""`. So both `vmux://terminal` and `vmux://terminal/` work via the host-match approach.
- **Helper proliferation in `agent.rs`**: two new spawn helpers (`spawn_sessions_tab`, `spawn_processes_tab`) plus the existing `spawn_terminal_tab` / `spawn_browser_tab` plus the new `handle_vmux_split_url` dispatcher. Clean grouping; if this list grows further, extract into a `spawn_tab_for_url(url, ...)` dispatcher used everywhere.
- **Cross-crate dependency surface**: `agent.rs` will reference `vmux_session::event::SESSIONS_WEBVIEW_URL` and `crate::sessions::SessionsView`, plus `vmux_layout::event::PROCESSES_WEBVIEW_URL` and `crate::processes_monitor::ProcessesMonitor`, plus `url::Url`. All paths already exist or are workspace deps; just need imports added.
- **`url` crate dep**: existing workspace dep; one more crate (`vmux_desktop`) adds it. Slight build-time overhead, negligible.

## File Map

- **Modify** `crates/vmux_desktop/Cargo.toml` — add `url = { workspace = true }` if not already present.
- **Modify** `crates/vmux_desktop/src/agent.rs` — add `spawn_sessions_tab` and `spawn_processes_tab` helpers; add private `spawn_vmux_tab` dispatcher with `?cwd=` query parsing for terminals; update `SplitAndNavigate` arm to delegate; restructure `BrowserNavigate` arm to handle vmux:// URLs by always creating a new tab in the resolved target/focused pane; add 9 new tests.
- **Modify** `crates/vmux_mcp/src/tools.rs` — update the `SplitAndNavigate` and `BrowserNavigate` `#[mcp(description = ...)]` strings.
