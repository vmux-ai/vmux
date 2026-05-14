# Claude & Codex CLI support — design

Status: draft (2026-05-14)
Owners: Junichi
Related: existing `vmux://vibe/` integration in `crates/vmux_desktop/src/vibe.rs`

## Goal

Add full-parity support for the **Claude Code** and **Codex** CLIs alongside the existing **Vibe** integration. Each CLI should:

1. Spawn fresh in a terminal tab via the command bar (`Claude New`, `Claude New Stack`, `Codex New`, `Codex New Stack`).
2. Be deep-linkable via `vmux://claude/[id]` and `vmux://codex/[id]` URLs (mirror `vmux://vibe/[id]`).
3. Discover its session id from the CLI's on-disk session log after launch and update the tab URL live.
4. Inject the vmux MCP server so the agent can call back into vmux (open panes, run shells, navigate browsers).
5. Run with vibe-equivalent trust defaults (no permission prompts inside cwd).
6. Revert the tab to `vmux://terminal/<pid>` when the agent process exits.

Vibe behavior is preserved exactly — same args, same session-discovery, same exit-detection.

## Why

The vmux abstraction around vibe (AgentProvider registry, session watcher, deep-link URLs, MCP injection) is generic. Two more popular CLIs ship today; users ask for them. Hard-coding three independent modules would triplicate boilerplate; refactoring to a strategy abstraction now keeps the cost of adding a 4th/5th CLI flat.

## Non-goals

- No background-agent / unattended runs. Agents are interactive PTY processes inside terminal tabs.
- No model picker, theme picker, or other CLI-specific UI in vmux. Each CLI manages its own settings.
- No custom auth flow. Users authenticate via each CLI's own login (`claude /login`, `codex login`) outside vmux.
- No first-class shortcuts assigned. Stays consistent with vibe (empty `shortcut` field).

---

## Architecture

### New crate: `crates/vmux_agent/`

```
crates/vmux_agent/
├── Cargo.toml          # bevy, serde, notify, chrono, vmux_core
└── src/
    ├── lib.rs          # re-exports
    ├── kind.rs         # AgentKind { Vibe, Claude, Codex } + executable() / url_scheme() helpers
    ├── strategy.rs     # AgentStrategy trait, AgentStrategies resource
    ├── session.rs      # AgentSession, SessionId, PendingAgentSession components
    │                   # AgentSessionToEntity resource
    │                   # generic systems (track / discover / detect_exit / format_url)
    ├── exec.rs         # find_executable (moved from vibe.rs)
    ├── mcp.rs          # McpServerConfig + sidecar resolution (moved from vibe.rs)
    ├── plugin.rs       # AgentSessionPlugin — registers strategies, watchers, systems
    ├── vibe.rs         # VibeStrategy
    ├── claude.rs       # ClaudeStrategy
    └── codex.rs        # CodexStrategy
```

What stays in `vmux_desktop`:

- `agent.rs` — keeps `AgentPlugin` (MCP→ECS command routing) and the spawn helpers (`spawn_terminal_tab`, `spawn_fresh_agent_tab`, `spawn_agent_resume_tab`). These touch `bevy_cef` and can't move to a generic crate.
- `AgentProvider` / `AgentProviders` registry stays here (its `prepare` returns desktop-specific `TerminalLaunch`).
- Per-kind provider registration is a small helper in `vmux_desktop::agent` that loops over `AgentKind` and registers two `AgentProvider` entries each (`<kind>_new`, `<kind>_new_stack`).

### Core types (in `vmux_agent`)

```rust
// kind.rs
pub enum AgentKind { Vibe, Claude, Codex }

impl AgentKind {
    pub fn executable(self) -> &'static str        // "vibe" | "claude" | "codex"
    pub fn url_scheme(self) -> &'static str        // "vmux://vibe/" | "vmux://claude/" | "vmux://codex/"
    pub fn from_host(host: &str) -> Option<Self>   // "vibe"/"claude"/"codex" → Some(_)
}

// strategy.rs
pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
}

#[derive(Resource)]
pub struct AgentStrategies(HashMap<AgentKind, Box<dyn AgentStrategy>>);

// session.rs (components)
#[derive(Component)] pub struct AgentSession { pub kind: AgentKind }
#[derive(Component)] pub struct SessionId(pub String);
#[derive(Component)] pub struct PendingAgentSession {
    pub kind: AgentKind,
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

// session.rs (resources)
#[derive(Resource, Default)] pub struct AgentSessionToEntity(HashMap<(AgentKind, String), Entity>);
#[derive(Resource, Default)] pub struct AgentSessionDirty(pub bool);
#[derive(Resource)]          pub struct AgentSessionWatchers(HashMap<AgentKind, (RecommendedWatcher, Mutex<Receiver<()>>)>);
```

### Component lifecycle on a terminal entity

```
spawn fresh:        + AgentSession { kind } + PendingAgentSession { kind, spawn_time, cwd }
discovery success:  - PendingAgentSession + SessionId(id)              → format_url writes vmux://<scheme>/<id>
discovery timeout:  - PendingAgentSession                                (URL stays vmux://<scheme>/)
process exits:      + ProcessExited (already inserted by terminal subsystem)
exit detected:      - AgentSession - SessionId                          → URL reverts to vmux://terminal/<pid>
```

### Systems (all in `vmux_agent::session`, run in `Update`)

| System | Trigger | Action |
|---|---|---|
| `track_session_id_inserts` | `Added<SessionId>` on `AgentSession` entity | Insert into `AgentSessionToEntity` keyed `(kind, id)` |
| `track_session_id_removals` | `RemovedComponents<SessionId>` | Remove from `AgentSessionToEntity` |
| `mark_dirty_on_fs_change` | Drains all watcher channels | Sets `AgentSessionDirty = true` |
| `mark_dirty_on_pending_added` | `Added<PendingAgentSession>` or `Added<SessionId>` | Sets `AgentSessionDirty = true` |
| `discover_pending_agent_sessions` | Run-condition `AgentSessionDirty` | For each `PendingAgentSession`, dispatch to strategy. Insert `SessionId` on hit, remove pending; remove pending after 30 s timeout |
| `detect_agent_session_exit_on_change` | Run-condition `AgentSessionDirty` | For each `(AgentSession, SessionId)`: if `ProcessExited` present OR `strategy.detect_end_time(id)` true, strip `AgentSession`+`SessionId`, rewrite URL to `vmux://terminal/<pid>` |
| `clear_agent_session_dirty` | After discovery+exit | Sets `AgentSessionDirty = false` |
| `format_agent_url` | `Or<(Changed<SessionId>, Added<AgentSession>)>` | Write `format!("{}{}", strategy.url_scheme(), session_id_or_empty)` into `PageMetadata.url` |

Same shape and ordering as the current `vibe::session` plugin — generalized over kind.

### Watchers

One `notify::RecommendedWatcher` per strategy. Created in `AgentSessionPlugin::build` `Startup` system; each watches its strategy's `sessions_root()` recursively and writes into its own `mpsc::channel()`. `mark_dirty_on_fs_change` drains every per-kind receiver into the single `AgentSessionDirty` flag.

3 watchers on macOS FSEvents is trivial overhead.

---

## Per-CLI behavior

### Vibe (preserved)

- **Executable**: `vibe`
- **URL scheme**: `vmux://vibe/`
- **Sessions root**: `$VIBE_HOME/logs/session/` or `~/.vibe/logs/session/`
- **Args**: `["--trust"]` + optional `["--resume", <id>]`
- **Env**: `VIBE_MCP_SERVERS` = JSON array `[{name:"vmux", transport:"stdio", command, args, cwd?}]`
- **Discovery**: read `*/meta.json`, parse `{session_id, start_time, environment.working_directory}`. Match by normalized cwd, `start_time ≥ spawn_time`, skip claimed, return earliest.
- **End-time detection**: parse `meta.json`, return true if `end_time` field present.

### Claude

- **Executable**: `claude` (Claude Code v2.x)
- **URL scheme**: `vmux://claude/`
- **Sessions root**: `~/.claude/projects/`
- **Args**:
  ```
  ["--permission-mode", "bypassPermissions",
   "--mcp-config", <json>,
   "--strict-mcp-config"]
  + sid.map(|id| ["--resume", id]).unwrap_or_default()
  ```
  where `<json>` is `{"mcpServers":{"vmux":{"command":<cmd>,"args":<args>,"cwd":<cwd?>}}}` (single object literal passed inline; `--mcp-config` accepts JSON strings).
- **Env**: `[]`
- **Discovery**:
  1. Encode cwd to project-dir name. Algorithm: replace each `/` with `-`. Verified against existing dirs: `/Users/junichi.sugiura/...` → `-Users-junichi-sugiura-...`. Final algorithm to be confirmed during implementation by reverse-engineering 2–3 dirs in `~/.claude/projects/`.
  2. List `~/.claude/projects/<encoded>/*.jsonl`.
  3. Filename stem (sans `.jsonl`) is the session UUID.
  4. Filter by `metadata().created()` (or first-line JSON `timestamp`) `≥ spawn_time`.
  5. Skip claimed; return earliest.
- **End-time detection**: returns `false`. Exit handled via `ProcessExited`.

### Codex

- **Executable**: `codex` (Codex CLI v0.13x)
- **URL scheme**: `vmux://codex/`
- **Sessions root**: `~/.codex/sessions/` (nested `YYYY/MM/DD/`)
- **Args** (fresh):
  ```
  ["-s", "workspace-write",
   "-a", "never",
   "-c", format!("mcp_servers.vmux.command={}", quote_toml(&mcp.command)),
   "-c", format!("mcp_servers.vmux.args={}",    toml_array(&mcp.args))]
  + mcp.cwd.map(|c| ["-c", format!("mcp_servers.vmux.cwd={}", quote_toml(c))]).unwrap_or_default()
  ```
- **Args** (resume): `["resume", <id>]` is a subcommand. Global `-c` flags must precede the subcommand. Final arg shape to be verified during implementation:
  ```
  [<global flags as above>, "resume", <id>]
  ```
  If codex requires the subcommand first, reorder accordingly. Cover with an integration test.
- `quote_toml(s)` and `toml_array(strs)` are helper fns in `vmux_agent::codex` that emit TOML-safe scalars / arrays for `-c` overrides (escape `"` and `\`, wrap in `"..."`).
- **Env**: `[]`
- **Discovery**:
  1. Walk `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. Pre-filter by file mtime `≥ spawn_time` (cheap).
  2. Read first line of each candidate, parse JSON, extract `payload.id`, `payload.cwd`, `payload.timestamp`.
  3. Match by normalized cwd; skip claimed; return earliest.
- **End-time detection**: returns `false`. Exit handled via `ProcessExited`.

---

## Routing & spawning (in `vmux_desktop`)

### `spawn_vmux_tab` (in `agent.rs`) extension

Add `"vibe" | "claude" | "codex"` arms that dispatch through generic helpers:

```rust
host @ ("vibe" | "claude" | "codex") => {
    let kind = AgentKind::from_host(host).unwrap();
    let cwd  = std::env::current_dir().unwrap_or(PathBuf::from("/"));
    let path = parsed.path().trim_start_matches('/');
    if path.is_empty() {
        spawn_fresh_agent_tab(kind, pane, cwd, ...).map(|_| ())
    } else {
        let id = path.to_string();
        if let Some(map) = agent_to_entity
            && let Some(&entity) = map.0.get(&(kind, id.clone()))
        {
            focus_pane_entity(entity, commands, child_of_q);
            return Ok(());
        }
        spawn_agent_resume_tab(kind, pane, cwd, id, ...).map(|_| ())
    }
}
```

`vibe_to_entity: Option<Res<VibeSessionToEntity>>` is replaced by `agent_to_entity: Option<Res<AgentSessionToEntity>>` everywhere it appears (`agent.rs`, `command_bar.rs`).

### Generic spawn helpers (in `vmux_desktop::agent`)

```rust
fn spawn_fresh_agent_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    strategies: &AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<Entity, String>;

fn spawn_agent_resume_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    session_id: String,
    /* ... */
) -> Result<Entity, String>;
```

These replace `spawn_fresh_vibe_tab` / `spawn_vibe_resume_tab` — same insert pattern, kind-aware. A stack-flavored sibling `spawn_agent_into_stack(kind, stack, cwd, session_id, ...)` mirrors today's `spawn_vibe_into_stack` (in `terminal.rs`) and is consumed by command-bar URL handling and persistence restore.

### `TerminalKind`

Existing enum (in `crates/vmux_desktop/src/terminal/launch.rs`):

```rust
pub enum TerminalKind { Plain, Vibe }
```

Becomes:

```rust
pub enum TerminalKind { Plain, Vibe, Claude, Codex }
```

Pure additive change. `Reflect` + `Serialize` data persisted with `Plain` or `Vibe` continues to deserialize. New variants are mapped from `AgentKind` via `AgentKind::into_terminal_kind()`.

### Persistence

`crates/vmux_desktop/src/persistence.rs` reads persisted `vmux://vibe/<id>` URLs and re-spawns via `spawn_vibe_into_stack`. Change to dispatch by URL scheme:

```rust
match url scheme {
    "vibe" | "claude" | "codex" => spawn_agent_into_stack(kind, ...),
    _ => /* existing */
}
```

`spawn_agent_into_stack` is the stack-flavored sibling of `spawn_fresh_agent_tab` / `spawn_agent_resume_tab` (already present today as `spawn_vibe_into_stack` in `terminal.rs`; generalize).

`TerminalLaunch` is `Reflect+Serialize`, so resumed tabs reconstitute the right args from disk on startup.

### Settings

`crates/vmux_desktop/src/settings.rs` line 66 default `startup_url = "vmux://vibe/"` is unchanged. Vibe stays the blessed default. Document that users can override to `vmux://claude/` or `vmux://codex/`.

---

## Command bar / AgentProvider registration

```rust
// vmux_desktop/src/agent.rs (in AgentPlugin::build)
fn register_agent_providers(providers: &mut AgentProviders) {
    use vmux_agent::{vibe, claude, codex};
    vibe::register_providers(providers);
    claude::register_providers(providers);
    codex::register_providers(providers);
}
```

Each strategy module exposes a `register_providers` helper that adds its two `AgentProvider` entries (`<kind>_new`, `<kind>_new_stack`) using free `fn` items (cannot capture, since `AgentProvider` fields are `fn` pointers — see "Open issues" below):

```rust
// vmux_agent/src/claude.rs
pub fn claude_available() -> bool { exec::find_executable("claude").is_some() }
pub fn claude_prepare(cwd: &Path) -> Result<PreparedAgentLaunch, String> { /* build_agent_launch(AgentKind::Claude, cwd, None) */ }

pub fn register_providers(providers: &mut AgentProviders) {
    providers.register(AgentProvider {
        id: "claude_new", name: "Claude New", shortcut: "",
        executable: "claude", available: claude_available, prepare: claude_prepare,
    });
    providers.register(AgentProvider {
        id: "claude_new_stack", name: "Claude New Stack", shortcut: "",
        executable: "claude", available: claude_available, prepare: claude_prepare,
    });
}
```

`AgentProviders.command_entries()` already filters by `available`, so Claude/Codex entries are hidden when the binary isn't installed. No extra wiring.

`PreparedAgentLaunch` may need to live in `vmux_agent` (or a third crate, or a small interface module shared between `vmux_agent` and `vmux_desktop::agent`). Resolved during implementation.

---

## Migration of existing files

Files **deleted**:
- `crates/vmux_desktop/src/vibe.rs`
- `crates/vmux_desktop/src/vibe/session.rs`
- `crates/vmux_desktop/src/vibe/` (directory)

Files **edited** (imports rewritten to `vmux_agent`):
- `crates/vmux_desktop/src/lib.rs` — `vibe::VibePlugin` → `vmux_agent::AgentSessionPlugin`
- `crates/vmux_desktop/src/agent.rs` — generic spawn helpers, generalized URL routing
- `crates/vmux_desktop/src/command_bar.rs` (lines 813, 832, 920–1057) — URL-prefix matching loop generalized over all schemes
- `crates/vmux_desktop/src/persistence.rs` (lines 325–357) — dispatch by URL scheme
- `crates/vmux_desktop/src/terminal.rs` (line 2668 + `spawn_vibe_into_stack`) — generalize to `spawn_agent_into_stack`
- `crates/vmux_desktop/src/settings.rs` — `crate::vibe::*` references retargeted (the default value stays `vmux://vibe/`)
- `crates/vmux_desktop/tests/release_invariants.rs` — adjust if it references old types

All `crate::vibe::session::Vibe` marker uses become `AgentSession { kind: AgentKind::Vibe }` (or filter on kind where the existing code only cared about vibe specifically).

All `VIBE_WEBVIEW_URL` constant references become `AgentKind::Vibe.url_scheme()` (or a generic helper that loops kinds).

---

## Tests

### `vmux_agent` unit tests

| File | Test | Coverage |
|---|---|---|
| `vibe::tests` | `discover_picks_session_matching_cwd_and_after_spawn_time` | port from existing |
| `vibe::tests` | `discover_skips_already_claimed_sessions` | port from existing |
| `vibe::tests` | `detect_end_time_returns_true_when_meta_has_end_time` | new |
| `claude::tests` | `discover_picks_jsonl_under_encoded_cwd_dir` | fixture: temp `~/.claude/projects/-tmp-foo/<uuid>.jsonl` |
| `claude::tests` | `discover_skips_files_older_than_spawn_time` | fixture |
| `claude::tests` | `build_args_includes_mcp_config_and_strict` | pure |
| `claude::tests` | `build_args_resume_appends_resume_flag` | pure |
| `claude::tests` | `detect_end_time_always_false` | pure |
| `codex::tests` | `discover_walks_yyyy_mm_dd_dirs` | fixture |
| `codex::tests` | `discover_reads_first_line_session_meta` | fixture |
| `codex::tests` | `build_args_uses_dash_c_overrides_for_mcp` | pure |
| `codex::tests` | `build_args_resume_uses_resume_subcommand` | pure |
| `codex::tests` | `detect_end_time_always_false` | pure |
| `session::tests` | `format_agent_url_emits_scheme_per_kind` | pure |
| `session::tests` | `pending_session_inserted_then_session_id_added` | ECS smoke |
| `exec::tests` | `find_executable_*` | port from existing |
| `mcp::tests` | `mcp_falls_back_to_cargo_run_when_sidecar_missing` | port from existing |

### `vmux_desktop` integration tests (in `agent.rs`)

| Test | Coverage |
|---|---|
| `spawn_fresh_agent_tab_inserts_pending_session_for_each_kind` | parameterized |
| `spawn_agent_resume_tab_inserts_session_id` | per kind |
| `vmux_claude_url_routes_to_claude_strategy` | URL routing |
| `vmux_codex_url_routes_to_codex_strategy` | URL routing |
| `deep_link_focuses_existing_agent_tab` | per kind |

Existing vibe tests in `agent.rs` keep passing — they exercise `spawn_vmux_tab` for the `vibe` host through the same generic helpers.

---

## Open issues / risks

1. **Claude project-dir encoding**. Algorithm assumed: replace `/` with `-`, no leading double-dash. Confirm by inspecting 2–3 existing dirs in `~/.claude/projects/` during implementation. If the algorithm differs (e.g. URL-encoding for non-ASCII), encode that into `claude::project_dir_name(&Path)` with a unit test against real fixtures.
2. **Codex resume subcommand placement**. Need to confirm whether global `-c` flags can precede the `resume` subcommand or must follow it. Cover with an integration test that actually shells out to `codex resume --help` (or builds args and pipes to a dry-run).
3. **`AgentProvider` `fn`-pointer constraint**. Cannot close over `AgentKind`, so each strategy ships free-fn pairs for `available` / `prepare`. Acceptable boilerplate (6 functions). Alternative: refactor `AgentProvider` to `Box<dyn Fn>` — out of scope for this design.
4. **Watcher count**. 3 strategies → 3 `RecommendedWatcher` instances on macOS FSEvents. Trivial overhead; if it ever matters, consolidate into a single watcher with multiple `watch()` calls.
5. **Session-ID collision across CLIs**. Codex uses UUIDv7 in `session_meta.id`; claude uses UUIDv4 filename; vibe uses its own UUIDv5-ish. Collisions astronomically unlikely, but `AgentSessionToEntity` is keyed on `(AgentKind, String)` to be safe.
6. **MCP config validation**. Wrong JSON for claude `--mcp-config` or wrong TOML for codex `-c mcp_servers.*` will silently disable the vmux MCP server. Add an integration test that checks the constructed args against a hand-rolled known-good fixture.
7. **`startup_url` default**. Stays `vmux://vibe/`. Adding a setting like `default_agent: AgentKind` could come later — out of scope here.

---

## Done means

- New crate `vmux_agent` exists, compiles, has unit tests (all passing).
- `vmux_desktop::vibe::*` is gone; all callers use `vmux_agent` types and generic spawn helpers.
- `Claude New`, `Claude New Stack`, `Codex New`, `Codex New Stack` show up in the command bar when the respective binaries are on `$PATH`.
- `vmux://claude/`, `vmux://claude/<id>`, `vmux://codex/`, `vmux://codex/<id>` all route correctly.
- A claude/codex tab transitions `vmux://<scheme>/` → `vmux://<scheme>/<id>` within a few seconds of launch (filesystem watcher discovers the session).
- A claude/codex tab reverts to `vmux://terminal/<pid>` when the agent exits.
- The agent process can call back into vmux via the injected `vmux` MCP server (verified: launch claude, ask it to "split the pane and run echo hi", confirm the side pane appears).
- Vibe behavior is unchanged (existing vibe tests still pass; manual smoke confirms `vmux://vibe/` flow).
- Pre-commit checks pass on all changed crates (`vmux_agent`, `vmux_desktop`).
