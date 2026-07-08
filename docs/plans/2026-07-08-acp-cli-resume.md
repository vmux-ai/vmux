# ACP ↔ CLI Session Resume (`/resume`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Do NOT subagent-drive** — CEF builds are huge and long-lived subagents drop the dev socket (see project memory). Implement inline with a warm target dir.

**Goal:** Add a `/` slash menu in the ACP agent composer that lists an agent's past on-disk sessions (`/resume`) and hands the current session off between ACP and CLI runtimes (`/cli`), swapping the session in place on the same page.

**Architecture:** A session is `(kind, sid, cwd)`; runtime (ACP vs CLI) is just how it's opened. Backend scrapes each agent kind's on-disk store into a unified list. A single `SwapStackSession` ECS message tears down the current session on a stack and re-attaches the target (ACP via `attach_acp_agent_to_stack`, CLI via `SpawnAgentInStackRequest`) with explicit cwd. The Dioxus composer renders a slash menu fed by bin-event snapshots and emits typed intents.

**Tech Stack:** Rust, Bevy ECS, Dioxus (WASM page), rkyv bin-ipc, serde. Crates: `vmux_agent` (strategies, chat page, plugin), `vmux_core` (messages), `vmux_command`/`vmux_layout` (Cmd+K).

**Verified facts baked in:** claude-code-acp ≥0.12 shares session ids + the `~/.claude/projects/<cwd>/<uuid>.jsonl` store with the `claude` CLI, so a claude `acp_session_id` is a valid `claude --resume` target both directions. Resume is cwd-keyed — cwd MUST travel with the session. Codex/Vibe id-sharing is unverified → `cross_runtime = false` for them (list + same-runtime resume only).

**Spec:** `docs/specs/2026-07-08-acp-cli-resume-design.md`

---

## Phase A — Backend session model + lister

### Task A1: `ResumableSession` type + `list_sessions` trait method

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/strategy.rs`

- [ ] **Step 1: Add the type + trait method (with a default so kinds compile before impl).**

In `crates/vmux_agent/src/client/cli/strategy.rs`, add above the trait:

```rust
use vmux_core::agent::AgentKind;

/// A resumable agent session discovered on disk. Runtime-agnostic: `(kind, sid, cwd)`
/// identifies the conversation; how it is opened (ACP vs CLI) is a separate choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumableSession {
    pub kind: AgentKind,
    pub sid: String,
    pub cwd: PathBuf,
    pub mtime: SystemTime,
    /// First user message / summary, or a short sid fallback.
    pub title: String,
    /// True when this kind's ACP and CLI runtimes share the session id (Claude only, for now).
    pub cross_runtime: bool,
}
```

Add to the `CliAgentStrategy` trait (default returns empty so unimplemented kinds still compile):

```rust
    /// List this kind's resumable sessions from its on-disk store, newest first is not
    /// required (the collector sorts). Default: none.
    fn list_sessions(&self) -> Vec<ResumableSession> {
        Vec::new()
    }
```

- [ ] **Step 2: Compile.** Run: `cargo check -p vmux_agent` — Expected: PASS (no impls yet).

- [ ] **Step 3: Commit.**
```bash
git add crates/vmux_agent/src/client/cli/strategy.rs
git commit -m "feat(agent): add ResumableSession + list_sessions trait hook"
```

### Task A2: Claude `list_sessions` (cross_runtime = true)

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/claude.rs`

- [ ] **Step 1: Write failing tests.** Append to the `tests` mod in `claude.rs`:

```rust
    #[test]
    fn list_sessions_reads_sid_cwd_and_title_from_jsonl() {
        let tmp = unique_tmp("claude-list");
        let proj = tmp.join("-Users-me-proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("11111111-2222.jsonl"),
            b"{\"type\":\"user\",\"cwd\":\"/Users/me/proj\",\"message\":{\"role\":\"user\",\"content\":\"fix the auth bug\"}}\n",
        )
        .unwrap();
        // agent-* internal files are excluded.
        std::fs::write(proj.join("agent-log.jsonl"), b"{}\n").unwrap();

        let out = list_claude_sessions(&tmp);
        assert_eq!(out.len(), 1, "agent-* excluded, one real session");
        let s = &out[0];
        assert_eq!(s.sid, "11111111-2222");
        assert_eq!(s.cwd, PathBuf::from("/Users/me/proj"));
        assert_eq!(s.title, "fix the auth bug");
        assert!(s.cross_runtime, "claude shares ids across ACP/CLI");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn list_sessions_title_falls_back_to_short_sid() {
        let tmp = unique_tmp("claude-list-fallback");
        let proj = tmp.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("abcdef01-9999.jsonl"), b"{\"type\":\"summary\"}\n").unwrap();
        let out = list_claude_sessions(&tmp);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "abcdef01");
        let _ = std::fs::remove_dir_all(&tmp);
    }
```

- [ ] **Step 2: Run — verify fail.** Run: `cargo test -p vmux_agent list_sessions -- --nocapture` — Expected: FAIL (`list_claude_sessions` not found).

- [ ] **Step 3: Implement.** Add to `claude.rs` (below `discover_claude_session_id`), and wire the trait method:

```rust
use crate::client::cli::strategy::ResumableSession;

pub(crate) fn list_claude_sessions(root: &Path) -> Vec<ResumableSession> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return out;
    };
    for proj in projects.flatten() {
        let Ok(files) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.starts_with("agent-") {
                continue;
            }
            let mtime = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let (cwd, title) = claude_cwd_and_title(&path, stem);
            out.push(ResumableSession {
                kind: AgentKind::Claude,
                sid: stem.to_string(),
                cwd,
                mtime,
                title,
                cross_runtime: true,
            });
        }
    }
    out
}

/// Read the first lines of a claude `.jsonl` to recover the working dir and a title.
/// `cwd` is taken from the first line carrying a string `cwd`; `title` from the first
/// user message text. Both fall back gracefully (cwd → the project dir, title → short sid).
fn claude_cwd_and_title(path: &Path, stem: &str) -> (PathBuf, String) {
    use std::io::{BufRead, BufReader};
    let mut cwd: Option<PathBuf> = None;
    let mut title: Option<String> = None;
    if let Ok(file) = std::fs::File::open(path) {
        for line in BufReader::new(file).lines().map_while(Result::ok).take(40) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if cwd.is_none()
                && let Some(c) = v.get("cwd").and_then(|c| c.as_str())
            {
                cwd = Some(PathBuf::from(c));
            }
            if title.is_none()
                && v.get("type").and_then(|t| t.as_str()) == Some("user")
                && let Some(text) = user_message_text(&v)
            {
                title = Some(text);
            }
            if cwd.is_some() && title.is_some() {
                break;
            }
        }
    }
    let cwd = cwd.unwrap_or_else(|| path.parent().map(Path::to_path_buf).unwrap_or_default());
    let title = title.unwrap_or_else(|| stem.split('-').next().unwrap_or(stem).to_string());
    (cwd, title)
}

/// Extract plain text from a claude `message.content` (string, or an array of `{type:text,text}`).
fn user_message_text(v: &Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(80).collect())
}
```

Then in `impl CliAgentStrategy for ClaudeStrategy` add:

```rust
    fn list_sessions(&self) -> Vec<ResumableSession> {
        list_claude_sessions(&self.sessions_root())
    }
```

- [ ] **Step 4: Run — verify pass.** Run: `cargo test -p vmux_agent list_sessions` — Expected: PASS.

- [ ] **Step 5: Commit.**
```bash
git add crates/vmux_agent/src/client/cli/claude.rs
git commit -m "feat(agent): claude list_sessions (sid/cwd/title from jsonl)"
```

### Task A3: Codex + Vibe `list_sessions` (cross_runtime = false)

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/codex.rs`, `crates/vmux_agent/src/client/cli/vibe.rs`

- [ ] **Step 1: Read the existing discovery parsers first.** Read `codex.rs` (`discover_codex_session_id`, `walk_jsonl`, the `session_meta` line: `payload.id`, `payload.cwd`) and `vibe.rs` (`discover_vibe_session_id`, `meta.json` → `environment.working_directory`, dir name `session_*`). Mirror those exact field paths.

- [ ] **Step 2: Write failing tests** — one per kind, mirroring A2 but with each kind's on-disk shape:
  - codex: a `<root>/2026/07/sess.jsonl` whose first line is `{"type":"session_meta","payload":{"id":"cx-1","cwd":"/w/x"}}`; assert `sid=="cx-1"`, `cwd=="/w/x"`, `!cross_runtime`.
  - vibe: a `<root>/session_vb-1/meta.json` = `{"environment":{"working_directory":"/w/y"}}`; assert `sid=="vb-1"`, `cwd=="/w/y"`, `!cross_runtime`.

```rust
// codex.rs tests
#[test]
fn list_sessions_reads_session_meta() {
    let tmp = unique_tmp("codex-list");
    let day = tmp.join("2026/07");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join("sess.jsonl"),
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"cx-1\",\"cwd\":\"/w/x\"}}\n",
    ).unwrap();
    let out = list_codex_sessions(&tmp);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].sid, "cx-1");
    assert_eq!(out[0].cwd, PathBuf::from("/w/x"));
    assert!(!out[0].cross_runtime);
    let _ = std::fs::remove_dir_all(&tmp);
}
```
```rust
// vibe.rs tests
#[test]
fn list_sessions_reads_meta_json() {
    let tmp = unique_tmp("vibe-list");
    let sdir = tmp.join("session_vb-1");
    std::fs::create_dir_all(&sdir).unwrap();
    std::fs::write(
        sdir.join("meta.json"),
        b"{\"environment\":{\"working_directory\":\"/w/y\"}}",
    ).unwrap();
    let out = list_vibe_sessions(&tmp);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].sid, "vb-1");
    assert_eq!(out[0].cwd, PathBuf::from("/w/y"));
    assert!(!out[0].cross_runtime);
    let _ = std::fs::remove_dir_all(&tmp);
}
```

- [ ] **Step 3: Run — verify fail.** Run: `cargo test -p vmux_agent list_sessions` — Expected: FAIL (`list_codex_sessions`/`list_vibe_sessions` missing).

- [ ] **Step 4: Implement** `list_codex_sessions(root)` and `list_vibe_sessions(root)` mirroring each kind's discovery parser (reuse `walk_jsonl` for codex; iterate `session_*` dirs for vibe). Title: codex → first user turn if cheap, else short sid; vibe → short sid (or a `meta.json` summary field if present). Set `kind`, `cross_runtime: false`. Wire each into the kind's `impl CliAgentStrategy` `list_sessions`.

- [ ] **Step 5: Run — verify pass.** Run: `cargo test -p vmux_agent list_sessions` — Expected: PASS.

- [ ] **Step 6: Commit.**
```bash
git add crates/vmux_agent/src/client/cli/codex.rs crates/vmux_agent/src/client/cli/vibe.rs
git commit -m "feat(agent): codex+vibe list_sessions (cross_runtime=false)"
```

### Task A4: Unified collector

**Files:**
- Modify: `crates/vmux_agent/src/strategy.rs`

- [ ] **Step 1: Write failing test.** In `strategy.rs` tests, register the three strategies and assert `list_all_sessions` unions + sorts newest-first + dedups `(kind, sid)`. (Use a temp-dir override if `sessions_root` can be pointed at fixtures; otherwise assert it returns without panicking and preserves ordering given a hand-built `Vec` via a small `sort_sessions` helper — test `sort_sessions` directly.)

```rust
#[test]
fn sort_sessions_is_newest_first_and_deduped() {
    use crate::client::cli::strategy::ResumableSession;
    use std::time::{Duration, SystemTime};
    let mk = |sid: &str, secs: u64| ResumableSession {
        kind: AgentKind::Claude, sid: sid.into(), cwd: "/w".into(),
        mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
        title: sid.into(), cross_runtime: true,
    };
    let got = sort_sessions(vec![mk("a", 10), mk("b", 30), mk("a", 20)]);
    assert_eq!(got.iter().map(|s| s.sid.as_str()).collect::<Vec<_>>(), vec!["b", "a"]);
}
```

- [ ] **Step 2: Run — verify fail.** Run: `cargo test -p vmux_agent sort_sessions` — Expected: FAIL.

- [ ] **Step 3: Implement** in `strategy.rs`:

```rust
use crate::client::cli::strategy::ResumableSession;

/// Sort newest-first and drop duplicate `(kind, sid)` keeping the newest.
pub fn sort_sessions(mut sessions: Vec<ResumableSession>) -> Vec<ResumableSession> {
    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    let mut seen = std::collections::HashSet::new();
    sessions.retain(|s| seen.insert((s.kind, s.sid.clone())));
    sessions
}

impl AgentStrategies {
    /// All resumable sessions across every registered CLI strategy, newest-first.
    pub fn list_all_sessions(&self) -> Vec<ResumableSession> {
        let all = self
            .cli_strategies()
            .flat_map(|s| s.list_sessions())
            .collect();
        sort_sessions(all)
    }
}
```

(If `cli_strategies()` returns values, not the trait objects, adapt to iterate `self.get_cli(kind)` over `AgentKind::all()`.)

- [ ] **Step 4: Run — verify pass.** Run: `cargo test -p vmux_agent sort_sessions` — Expected: PASS.

- [ ] **Step 5: Commit.**
```bash
git add crates/vmux_agent/src/strategy.rs
git commit -m "feat(agent): list_all_sessions collector (sorted, deduped)"
```

---

## Phase B — `SwapStackSession` message + handler

### Task B1: Define the message + target-URL resolver

**Files:**
- Modify: `crates/vmux_core/src/agent.rs`
- Modify: `crates/vmux_agent/src/url.rs`

- [ ] **Step 1: Add the message (native-only).** In `crates/vmux_core/src/agent.rs`, near `SpawnAgentInStackRequest`, add (respect the existing `#[cfg(not(target_arch = "wasm32"))]` gating used for Bevy messages there — see `vmux_core::event` wasm memory):

```rust
/// Swap the agent session shown on `stack` in place: tear down the current session and
/// re-attach `target` (ACP or CLI) with the given `cwd`. Same tab position.
#[cfg(not(target_arch = "wasm32"))]
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct SwapStackSession {
    pub stack: bevy::prelude::Entity,
    /// Formatted agent url of the target runtime+session (see `AgentUrl::format`).
    pub target_url: String,
    pub cwd: std::path::PathBuf,
}
```

(Use the same import style as the surrounding messages in that file. `target_url` is a String so `vmux_core` needn't depend on `vmux_agent::AgentUrl`.)

- [ ] **Step 2: Add the resolver + tests in `url.rs`.**

```rust
impl AgentUrl {
    /// The url that opens `(kind, sid)` in the given runtime. ACP is only addressable when the
    /// kind's segment is a configured ACP id (claude, codex); otherwise falls back to CLI so
    /// the url is always openable.
    pub fn for_session(kind: AgentKind, sid: &str, prefer_acp: bool, acp_ids: &[String]) -> Self {
        let seg = kind.as_url_segment();
        if prefer_acp && acp_ids.iter().any(|id| id == seg) {
            AgentUrl::Acp { id: seg.to_string(), sid: Some(sid.to_string()) }
        } else {
            AgentUrl::Cli { kind, sid: sid.to_string() }
        }
    }
}
```

```rust
#[test]
fn for_session_prefers_acp_when_configured() {
    let ids = vec!["claude".to_string(), "codex".to_string()];
    assert_eq!(
        AgentUrl::for_session(AgentKind::Claude, "s1", true, &ids),
        AgentUrl::Acp { id: "claude".into(), sid: Some("s1".into()) }
    );
    // vibe has no matching acp id → CLI even when prefer_acp.
    assert_eq!(
        AgentUrl::for_session(AgentKind::Vibe, "s2", true, &ids),
        AgentUrl::Cli { kind: AgentKind::Vibe, sid: "s2".into() }
    );
    // prefer_acp=false → always CLI.
    assert_eq!(
        AgentUrl::for_session(AgentKind::Claude, "s3", false, &ids),
        AgentUrl::Cli { kind: AgentKind::Claude, sid: "s3".into() }
    );
}
```

- [ ] **Step 3: Run.** `cargo test -p vmux_agent for_session` and `cargo check -p vmux_core` — Expected: PASS.

- [ ] **Step 4: Commit.**
```bash
git add crates/vmux_core/src/agent.rs crates/vmux_agent/src/url.rs
git commit -m "feat(agent): SwapStackSession message + AgentUrl::for_session resolver"
```

### Task B2: The swap handler

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Read the acp-config source.** In `plugin.rs`, find the caller of `handle_agent_page_open_task` and note how it obtains `acp_configs: &[vmux_setting::AcpAgentConfig]` and `catalog` from `AppSettings`/resources. The swap handler needs the same two.

- [ ] **Step 2: Add the handler.** Insert near `handle_spawn_agent_requests`:

```rust
fn handle_swap_stack_session(
    mut reader: MessageReader<vmux_core::agent::SwapStackSession>,
    settings: Res<AppSettings>,
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
    children_q: Query<&Children>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for ev in reader.read() {
        // Tear down stack-level agent components. Removing AcpSession fires
        // close_acp_session_on_remove → the daemon session is closed. Children (the
        // Browser/terminal pane) are despawned; a CLI terminal despawn kills its PTY.
        commands
            .entity(ev.stack)
            .remove::<crate::client::acp::AcpSession>()
            .remove::<crate::components::AgentSession>()
            .remove::<crate::AgentMessages>()
            .remove::<crate::AgentApprovalPolicy>()
            .remove::<crate::AgentRunState>()
            .remove::<vmux_core::AgentWorkingDir>()
            .remove::<vmux_core::team::Agent>()
            .remove::<vmux_core::team::Profile>();
        clear_stack_children(ev.stack, &children_q, &mut commands);

        match crate::AgentUrl::parse(&ev.target_url) {
            Some(crate::AgentUrl::Cli { kind, sid }) => {
                let session_id = (sid != crate::url::CLI_FRESH_SID).then_some(sid);
                spawn_agent.write(SpawnAgentInStackRequest {
                    kind,
                    cwd: ev.cwd.clone(),
                    session_id,
                    stack: ev.stack,
                    initial_prompt: None,
                });
            }
            Some(crate::AgentUrl::Acp { id, sid }) => {
                let Some(cfg) = settings.agent.acp.iter().find(|c| c.id == id) else {
                    bevy::log::warn!("swap: no ACP agent configured for '{id}'");
                    continue;
                };
                let routing_sid = uuid::Uuid::new_v4().to_string();
                let icon = acp_icon_for_id(catalog.as_deref(), &cfg.id);
                attach_acp_agent_to_stack(
                    ev.stack, &cfg.id, &cfg.name, &routing_sid, &ev.cwd,
                    icon.as_deref(), sid.as_deref(),
                    &mut commands, &mut meshes, &mut webview_mt,
                );
            }
            other => bevy::log::warn!("swap: unsupported target url {:?}", other),
        }
    }
}
```

(Adjust `settings.agent.acp` to the real accessor found in Step 1.)

- [ ] **Step 3: Register the message + system** in the plugin `build()` (chain builder calls per AGENTS.md):

```rust
app.add_message::<vmux_core::agent::SwapStackSession>()
    .add_systems(Update, handle_swap_stack_session);
```

- [ ] **Step 4: Write an ECS behavior test.** New file `crates/vmux_agent/tests/swap_session.rs` (or an inline `#[cfg(test)]` module) that builds a minimal `App`, spawns a stack with a fake `AcpSession`, sends `SwapStackSession` targeting `vmux://agent/claude/cli/s-1` with a cwd, runs `Update`, and asserts a `SpawnAgentInStackRequest { kind: Claude, session_id: Some("s-1"), cwd, stack }` was written and `AcpSession` was removed from the stack. Follow the "register written messages in build()" rule (workspace-test memory).

```rust
// Sketch — adapt resource/asset setup to what handle_swap_stack_session needs.
#[test]
fn swap_to_cli_writes_spawn_request_and_removes_acp() {
    // app.add_plugins(MinimalPlugins)...
    // spawn stack with AcpSession{..}; send SwapStackSession{ stack, target_url: cli url, cwd };
    // app.update();
    // assert SpawnAgentInStackRequest captured has session_id Some("s-1") + cwd;
    // assert app.world().get::<AcpSession>(stack).is_none();
}
```

- [ ] **Step 5: Run.** `cargo test -p vmux_agent swap` — Expected: PASS. `cargo check -p vmux_agent` — PASS.

- [ ] **Step 6: Commit.**
```bash
git add crates/vmux_agent/src/plugin.rs crates/vmux_agent/tests/swap_session.rs
git commit -m "feat(agent): SwapStackSession handler (teardown + re-attach with cwd)"
```

---

## Phase C — Wire protocol (page ↔ native)

### Task C1: Resume + slash events

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/event.rs`

- [ ] **Step 1: Add payloads + a rkyv round-trip test.** Follow the existing derive pattern (`ChatSnapshot`/`ChatSubmit`).

```rust
/// Bin-event id: native → page, the resumable-session list (answer to ResumeListRequest).
pub const RESUMABLE_SESSIONS_EVENT: &str = "resumable_sessions";
/// Bin-event id: native → page, the slash commands available for this pane.
pub const SLASH_COMMANDS_EVENT: &str = "slash_commands";

/// One row in the `/resume` picker. Strings only (frontend is dumb).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ResumableSessionEntry {
    pub kind: String,        // AgentKind::as_url_segment
    pub sid: String,
    pub cwd: String,
    pub title: String,
    pub subtitle: String,    // "2h ago · proj" (native-formatted)
    pub cross_runtime: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ResumableSessions {
    pub sessions: Vec<ResumableSessionEntry>,
}

/// One slash command entry (native → page).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SlashCommandEntry {
    pub name: String,        // "resume", "cli"
    pub description: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SlashCommands {
    pub commands: Vec<SlashCommandEntry>,
}

/// Page → native: open the `/resume` picker (native replies with ResumableSessions).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ResumeListRequest;

/// Page → native: resume a specific past session on this stack (current runtime).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ResumeSession {
    pub kind: String,
    pub sid: String,
    pub cwd: String,
}

/// Page → native: hand the current session to the other runtime. `to`: "cli" | "acp".
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct RuntimeSwitchRequest {
    pub to: String,
}
```

Add a test mirroring `chat_snapshot_rkyv_roundtrip` for `ResumableSessions` (one entry) — assert round-trip of `sessions[0].sid` + `cross_runtime`.

- [ ] **Step 2: Run.** `cargo test -p vmux_agent -- resumable` — Expected: PASS. Also confirm wasm still typechecks: `cargo check -p vmux_agent --target wasm32-unknown-unknown` — Expected: PASS (these payloads are Bevy-free).

- [ ] **Step 3: Commit.**
```bash
git add crates/vmux_agent/src/chat_page/event.rs
git commit -m "feat(agent): resume + slash bin-event payloads"
```

---

## Phase D — Composer slash menu (frontend)

### Task D1: Slash detection + command/session menu

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/page.rs`

- [ ] **Step 1: Import the new payloads.** Extend the `use crate::chat_page::event::{...}` at the top with `RESUMABLE_SESSIONS_EVENT, ResumableSessions, ResumableSessionEntry, SLASH_COMMANDS_EVENT, SlashCommands, SlashCommandEntry, ResumeListRequest, ResumeSession, RuntimeSwitchRequest`.

- [ ] **Step 2: Add signals + listeners** inside `Page()` (next to the other `use_signal`/`use_bin_event_listener` calls):

```rust
let mut slash_cmds = use_signal(Vec::<SlashCommandEntry>::new);
let mut sessions = use_signal(Vec::<ResumableSessionEntry>::new);
let mut menu_sel = use_signal(|| 0usize);
// Menu mode: None = closed, Some(false) = command list, Some(true) = session list.
let mut resume_mode = use_signal(|| false);

let _cmds = use_bin_event_listener::<SlashCommands, _>(SLASH_COMMANDS_EVENT, move |s| {
    slash_cmds.set(s.commands.clone());
});
let _sess = use_bin_event_listener::<ResumableSessions, _>(RESUMABLE_SESSIONS_EVENT, move |s| {
    sessions.set(s.sessions.clone());
    menu_sel.set(0);
});
```

- [ ] **Step 3: Derive the menu-open state + filtered items** from `draft`. Add helpers above `Page` and compute inside the component:

```rust
/// A draft is in slash mode when it's a single `/token` (no spaces yet).
fn slash_query(draft: &str) -> Option<&str> {
    let d = draft.strip_prefix('/')?;
    if d.contains(char::is_whitespace) { None } else { Some(d) }
}
```

```rust
let menu_open = slash_query(&draft()).is_some() && !resume_mode();
let session_menu_open = resume_mode() && !sessions.read().is_empty();
let filtered_cmds: Vec<SlashCommandEntry> = {
    let q = slash_query(&draft()).unwrap_or("").to_lowercase();
    slash_cmds.read().iter().filter(|c| c.name.starts_with(&q)).cloned().collect()
};
```

- [ ] **Step 4: Render the drop-up menu** directly above the `div { class: "flex items-end gap-2", ... }` textarea row, inside the input container. Use `z-20` (above chat rows — see terminal overlay z-index memory):

```rust
if menu_open && !filtered_cmds.is_empty() {
    div { class: "absolute bottom-full left-0 z-20 mb-2 w-full max-w-3xl overflow-hidden rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
        for (i, c) in filtered_cmds.iter().enumerate() {
            div {
                key: "sc{i}",
                class: if i == menu_sel() { "flex items-baseline gap-3 px-3.5 py-2 text-sm bg-foreground/10" } else { "flex items-baseline gap-3 px-3.5 py-2 text-sm" },
                span { class: "font-medium text-foreground", "/{c.name}" }
                span { class: "text-xs text-muted-foreground", "{c.description}" }
            }
        }
    }
}
if session_menu_open {
    div { class: "absolute bottom-full left-0 z-20 mb-2 max-h-80 w-full max-w-3xl overflow-y-auto rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
        for (i, s) in sessions.read().iter().enumerate() {
            div {
                key: "rs{i}",
                class: if i == menu_sel() { "flex flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex flex-col gap-0.5 px-3.5 py-2" },
                span { class: "truncate text-sm text-foreground", "{s.title}" }
                span { class: "truncate text-xs text-muted-foreground", "{s.subtitle}" }
            }
        }
    }
}
```

Ensure the wrapping input container is `relative` (it already sits under a `relative z-10` parent; add `relative` to the `div { class: "mx-auto flex max-w-3xl flex-col gap-2", ... }` if needed for absolute anchoring).

- [ ] **Step 5: Keyboard handling** — extend the textarea `onkeydown`. Before the existing Enter branch, intercept when a menu is open:

```rust
let menu_is_open = menu_open && !filtered_cmds.is_empty();
let sess_is_open = session_menu_open;
if (menu_is_open || sess_is_open)
    && matches!(e.key(), Key::ArrowDown | Key::ArrowUp)
{
    e.prevent_default();
    let len = if sess_is_open { sessions.read().len() } else { filtered_cmds.len() };
    let cur = menu_sel();
    let next = match e.key() {
        Key::ArrowDown => (cur + 1) % len.max(1),
        _ => (cur + len.saturating_sub(1)) % len.max(1),
    };
    menu_sel.set(next);
    return;
}
if e.key() == Key::Enter && !e.modifiers().shift() && (menu_is_open || sess_is_open) {
    e.prevent_default();
    if sess_is_open {
        if let Some(s) = sessions.read().get(menu_sel()) {
            let _ = try_cef_bin_emit_rkyv(&ResumeSession {
                kind: s.kind.clone(), sid: s.sid.clone(), cwd: s.cwd.clone(),
            });
        }
        resume_mode.set(false);
        draft.set(String::new());
    } else if let Some(c) = filtered_cmds.get(menu_sel()) {
        run_slash_command(&c.name, resume_mode, draft);
    }
    return;
}
if e.key() == Key::Escape && (menu_is_open || sess_is_open) {
    e.prevent_default();
    resume_mode.set(false);
    draft.set(String::new());
    return;
}
```

Add the dispatcher near `do_submit`:

```rust
/// Run a selected vmux slash command. `resume` opens the session picker; `cli`/`acp`
/// hand the current session to the other runtime. Unknown names are ignored (the raw
/// text still submits via the normal Enter path).
fn run_slash_command(name: &str, mut resume_mode: Signal<bool>, mut draft: Signal<String>) {
    match name {
        "resume" => {
            let _ = try_cef_bin_emit_rkyv(&ResumeListRequest);
            resume_mode.set(true);
        }
        "cli" => {
            let _ = try_cef_bin_emit_rkyv(&RuntimeSwitchRequest { to: "cli".into() });
            draft.set(String::new());
        }
        "acp" => {
            let _ = try_cef_bin_emit_rkyv(&RuntimeSwitchRequest { to: "acp".into() });
            draft.set(String::new());
        }
        _ => {}
    }
}
```

- [ ] **Step 6: Guard `do_submit`** so a `/resume`-style draft with the menu open doesn't get sent as a prompt (the Enter branch above already `return`s when a menu is open; `do_submit` stays for the no-menu case). No change needed beyond Step 5's early returns.

- [ ] **Step 7: Typecheck wasm.** Run: `cargo check -p vmux_agent --target wasm32-unknown-unknown` — Expected: PASS.

- [ ] **Step 8: Source-scrape test.** If `vmux_layout`/`vmux_agent` has `include_str!` page-source assertions (see page.rs source-scrape memory), add/adjust an assertion that `page.rs` contains the slash menu markers (e.g. `"/{c.name}"`). Run the owning crate's native test.

- [ ] **Step 9: Commit.**
```bash
git add crates/vmux_agent/src/chat_page/page.rs
git commit -m "feat(agent): composer slash menu (/resume, /cli) with keyboard nav"
```

---

## Phase E — Backend handlers wiring events → lister/swap

### Task E1: Register page→native events + handlers

**Files:**
- Modify: `crates/vmux_agent/src/chat_page.rs`

- [ ] **Step 1: Read how `ChatSubmit` is registered** as a native listener/observer in `chat_page.rs` (the CEF bin bridge → ECS message/observer for `on_chat_submit`). Mirror that exact registration for `ResumeListRequest`, `ResumeSession`, `RuntimeSwitchRequest`.

- [ ] **Step 2: `on_resume_list_request`** — scan + push snapshot to the requesting pane. It needs `AgentStrategies` + the pane to emit back to (same emit path `ChatSnapshot` uses). Format `subtitle` native-side:

```rust
fn relative_time(mtime: std::time::SystemTime) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}
```

Build `ResumableSessionEntry { kind: s.kind.as_url_segment().into(), sid, cwd: s.cwd.to_string_lossy().into(), title, subtitle: format!("{} · {}", relative_time(s.mtime), s.cwd.file_name()...), cross_runtime }` from `strategies.list_all_sessions()`, then emit `ResumableSessions` on `RESUMABLE_SESSIONS_EVENT` to the pane (mirror the `ChatSnapshot` emit helper).

- [ ] **Step 3: `on_resume_session`** — map the payload to a `SwapStackSession` on the pane's stack. Resolve runtime = current pane runtime (ACP pane → prefer_acp true). Build target via `AgentUrl::for_session(kind, sid, prefer_acp, acp_ids).format()`:

```rust
// kind from segment:
let kind = AgentKind::from_url_segment(&ev.kind);
// acp_ids from settings.agent.acp ids
// prefer_acp = pane currently has AcpSession
swap.write(vmux_core::agent::SwapStackSession {
    stack,
    target_url: AgentUrl::for_session(kind, &ev.sid, prefer_acp, &acp_ids).format(),
    cwd: PathBuf::from(&ev.cwd),
});
```

- [ ] **Step 4: `on_runtime_switch_request`** — the `/cli` fallback. Read the pane's live `AcpSession { agent_id, resume, cwd }` (or CLI `AgentSession`+`SessionId`) to get the authoritative `(kind, sid, cwd)`, then:
  - `to == "cli"`: `target = AgentUrl::Cli { kind, sid }`. Only proceed if the kind's session is `cross_runtime` (claude) — else `warn!` + no-op.
  - `to == "acp"`: `target = AgentUrl::for_session(kind, sid, true, &acp_ids)`.
  - Emit `SwapStackSession { stack, target_url, cwd }`.

  For an ACP claude pane, `kind = AgentKind::Claude` (map `agent_id "claude"` → `AgentKind::from_url_segment`), `sid = AcpSession.resume` (the acp_session_id == claude session uuid), `cwd = AcpSession.cwd`.

- [ ] **Step 5: Register** all three handlers + the `SwapStackSession` writer in the plugin `build()` (chained).

- [ ] **Step 6: Test.** ECS test: insert a stack with `AcpSession { agent_id:"claude", resume:Some("sid-9"), cwd:"/w" }`, send the `RuntimeSwitchRequest{to:"cli"}`-equivalent ECS message, run update, assert a `SwapStackSession { target_url: "vmux://agent/claude/cli/sid-9", cwd:"/w" }` was written. Run: `cargo test -p vmux_agent runtime_switch` — Expected: PASS.

- [ ] **Step 7: Commit.**
```bash
git add crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): wire resume-list, resume-session, runtime-switch handlers"
```

### Task E2: Push the slash-command list to each ACP pane

**Files:**
- Modify: `crates/vmux_agent/src/chat_page.rs` (or `client/acp.rs`)

- [ ] **Step 1: Emit `SlashCommands` when an ACP pane is ready.** Add a system (or extend the snapshot emitter) that, for a pane with `AcpSession`, sends `SlashCommands { commands }` where `commands = [resume]` always, plus `cli` when the pane's kind is `cross_runtime` (claude). Emit on `AcpSession` add and whenever the run-state snapshot is pushed (cheap, idempotent).

```rust
fn slash_commands_for(cross_runtime: bool) -> Vec<SlashCommandEntry> {
    let mut v = vec![SlashCommandEntry { name: "resume".into(), description: "Resume a past session".into() }];
    if cross_runtime {
        v.push(SlashCommandEntry { name: "cli".into(), description: "Continue this session in the CLI".into() });
    }
    v
}
```

  Determine `cross_runtime` from the pane's kind: map `AcpSession.agent_id` → `AgentKind::from_url_segment` → check against the known-shared set (claude). Reuse the `ResumableSession.cross_runtime` rule (a small `fn kind_cross_runtime(kind) -> bool { matches!(kind, AgentKind::Claude) }`, defined once and reused by A2/A3 too — DRY).

- [ ] **Step 2: Refactor `cross_runtime` to one source.** Define `pub fn kind_supports_cross_runtime(kind: AgentKind) -> bool` in `strategy.rs`; use it in claude/codex/vibe `list_sessions` and here. Update A2/A3 to call it (replace the literal `true`/`false`).

- [ ] **Step 3: Run.** `cargo check -p vmux_agent` + `cargo test -p vmux_agent` — Expected: PASS.

- [ ] **Step 4: Commit.**
```bash
git add crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/strategy.rs crates/vmux_agent/src/client/cli/*.rs
git commit -m "feat(agent): advertise /resume + /cli slash commands per ACP pane"
```

---

## Phase F — Cmd+K runtime-switch commands (secondary entry)

> Scope note: the composer slash menu (Phase D) fully delivers `/resume` browsing. Phase F adds the Cmd+K runtime-switch commands (small). The full Cmd+K *session-list* browser is deferred (spec §Deferred) — call this out to the user at review.

### Task F1: "Continue in CLI" / "Continue in ACP" AppCommands

**Files:**
- Modify: `crates/vmux_command/src/command.rs`
- Modify: `crates/vmux_layout/src/command_bar/handler.rs`

- [ ] **Step 1: Add command variants.** In the appropriate sub-enum (e.g. a new `AgentCommand` or the existing agent-related group), add two `#[menu(...)]` leaves: `ContinueInCli`, `ContinueInAcp`. They auto-appear in the `>` list via the `CommandBar` derive.

- [ ] **Step 2: Handle them.** In `on_command_bar_action` (`handler.rs`), for these commands, resolve the focused agent stack and emit the same ECS message `on_runtime_switch_request` uses (send an ECS `RuntimeSwitch`-equivalent, or directly `SwapStackSession` built from the focused pane's live session). Reuse the E1/E4 resolution helper — extract it into a shared `fn` so the composer path and Cmd+K path share one code path (message integration per AGENTS.md).

- [ ] **Step 3: Test.** Native `cargo test -p vmux_command` (command enumeration) + `cargo test -p vmux_layout` (handler). Add an assertion that the new command ids appear in `AppCommand::command_bar_entries()`.

- [ ] **Step 4: Commit.**
```bash
git add crates/vmux_command/src/command.rs crates/vmux_layout/src/command_bar/handler.rs
git commit -m "feat(command): Continue in CLI/ACP runtime-switch commands"
```

---

## Phase G — Verification pass (end)

> Per project memory (finish-then-test): defer manual/runtime testing to ONE pass here.

### Task G1: Workspace checks

- [ ] **Step 1: fmt.** Run: `cargo fmt --all`. Then `git checkout -- patches/` (fmt touches vendored patches — see cargo-fmt-patches memory). Stage only `crates/` changes.
- [ ] **Step 2: clippy.** Run: `cargo clippy --workspace --all-targets -- -D warnings` — fix any findings.
- [ ] **Step 3: tests.** Run: `cargo test --workspace` — Expected: PASS. (Register any plugin-written messages in `build()`, not per-test — workspace-test memory.)
- [ ] **Step 4: wasm typecheck.** Run: `cargo check -p vmux_agent --target wasm32-unknown-unknown` — Expected: PASS.
- [ ] **Step 5: Commit any fixes.**
```bash
git add -A -- crates/ && git commit -m "chore(agent): fmt/clippy/test fixes for resume feature"
```

### Task G2: Runtime smoke (user-driven)

- [ ] **Step 1: Build + run** the dev app (warm CEF target first, then incremental — vmux build-workflow memory). Do NOT launch unbounded builds/loops yourself; let the user runtime-test (no-unbounded-make-dev memory).
- [ ] **Step 2: Verify the flows** (user):
  1. Open `vmux://agent/claude` (ACP). Send a prompt so a session id is assigned (URL rewrites to `/claude/<sid>`).
  2. Type `/` → menu shows `resume` + `cli`. Type `/cli` → same page swaps to a CLI `claude --resume <sid>` PTY continuing the conversation, same cwd.
  3. In an ACP pane, `/resume` → session list appears → pick one → page swaps to that session (history replays).
  4. From a codex/vibe ACP pane, `/cli` is absent (gated); `/resume` still lists + resumes in-runtime.
- [ ] **Step 3: Read the app's own logs** in `~/Library/Application Support/Vmux/dev/logs/…` for warnings (never ask the user to paste logs — read them yourself).

### Task G3: Open PR

- [ ] **Step 1:** `cargo test --workspace` green, worktree clean of `.claude/*`.
- [ ] **Step 2:** Push `feat/acp-cli-resume`, open PR via `gh pr create` (not `-w`; create-pr-directly memory). Delete this plan file once fully merged (AGENTS.md).

---

## Self-review

**Spec coverage:**
- Runtime-agnostic identity → A1 (`ResumableSession`), B1 (`for_session`). ✓
- Backend lister (3 kinds, on-demand, sorted) → A2/A3/A4. ✓
- Composer slash menu (`/resume`, `/cli`, extensible, unknown→prompt) → D1, E2. ✓
- In-place swap + cwd carry → B1/B2, E1. ✓
- Cross-runtime handoff gated by `cross_runtime` (claude) → E1/E2, `kind_supports_cross_runtime`. ✓
- Cmd+K secondary → F1 (runtime-switch); **session-list-in-palette deferred** (flagged). ✓ (partial by design)
- Edge cases: already-open focus (existing `AgentSessionToEntity` path in page-open — the swap intentionally forces a fresh attach; add focus-existing only if desired — noted), stale sid (ACP falls back to new; CLI warn), cwd carried, missing binary (existing setup path). ✓
- Tests: parsers (A2/A3), url round-trip (B1), swap ECS (B2), runtime-switch ECS (E1), rkyv (C1), source-scrape (D1). ✓

**Placeholder scan:** No TBD/TODO in code steps; the two "read the existing X first" steps (A3.1, B2.1, E1.1) are deliberate context-gathering before code that depends on exact upstream field paths/accessors, not deferred implementation.

**Type consistency:** `ResumableSession` (native, PathBuf/SystemTime) vs `ResumableSessionEntry` (wire, String) kept distinct and mapped in E1. `SwapStackSession.target_url: String` (built via `AgentUrl::format`, parsed in the handler) avoids a `vmux_core → vmux_agent` dep. `kind_supports_cross_runtime` is the single source for the `cross_runtime` flag (A2/A3 refactored in E2.2). Slash command names (`resume`/`cli`/`acp`) match between D1 `run_slash_command`, E2 `slash_commands_for`, and E1 `on_runtime_switch_request`.

**Known follow-ups (not blockers):** Cmd+K session-list browser; ACP `availableCommands` surfacing; codex/vibe cross-runtime after verification; per-row runtime pick; focus-existing-tab on resume instead of re-attach.
