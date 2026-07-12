# ACP ↔ CLI Session Resume (`/resume`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Do NOT subagent-drive** — CEF builds are huge and long-lived subagents drop the dev socket (see project memory). Implement inline with a warm target dir.

**Goal:** Add a `/` slash menu in the ACP agent composer that lists an agent's past on-disk sessions (`/resume`) and hands the current session off between ACP and CLI runtimes (`/cli`), swapping the session in place on the same page.

**Architecture:** A session is `(kind, sid, cwd)`; runtime (ACP vs CLI) is just how it's opened. Backend scrapes each agent kind's on-disk store into a unified list. A single `SwapStackSession` ECS message tears down the current session on a stack and re-attaches the target (ACP via `attach_acp_agent_to_stack`, CLI via `SpawnAgentInStackRequest`) with explicit cwd. The Dioxus composer renders a slash menu fed by bin-event snapshots and emits typed intents.

**Tech Stack:** Rust, Bevy ECS, Dioxus (WASM page), rkyv bin-ipc, serde. Crates: `vmux_agent` (strategies, chat page, plugin), `vmux_core` (messages).

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

## Phase F — Deferred: Cmd+K entry points

The Cmd+K session browser and "Continue in CLI/ACP" commands are deferred. This PR ships the
ACP composer `/resume` and `/cli` flows only.

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

## Phase H — Prompt-driven resume selector refinement

This phase refines the implemented PR. Keep execution inline; do not dispatch subagents.

### Task H1: Extract testable composer state, filtering, navigation, and edit helpers

**Files:**
- Create: `crates/vmux_agent/src/chat_page/composer.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`

- [ ] **Step 1: Write the failing helper tests.** Create
  `crates/vmux_agent/src/chat_page/composer.rs` with the test module first:

```rust
use super::event::ResumableSessionEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectorMode<'a> {
    None,
    Commands(&'a str),
    Resume(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuDirection {
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PromptEdit<'a> {
    Insert(&'a str),
    Backspace,
    Delete,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(sid: &str, title: &str, cwd: &str) -> ResumableSessionEntry {
        ResumableSessionEntry {
            sid: sid.into(),
            title: title.into(),
            cwd: cwd.into(),
            ..Default::default()
        }
    }

    #[test]
    fn selector_mode_distinguishes_commands_and_resume_arguments() {
        assert_eq!(selector_mode("hello"), SelectorMode::None);
        assert_eq!(selector_mode("/res"), SelectorMode::Commands("res"));
        assert_eq!(selector_mode("/resume"), SelectorMode::Commands("resume"));
        assert_eq!(selector_mode("/resume "), SelectorMode::Resume(""));
        assert_eq!(selector_mode("/resume  SID-9"), SelectorMode::Resume("SID-9"));
        assert_eq!(selector_mode("/unknown arg"), SelectorMode::None);
    }

    #[test]
    fn resume_filter_matches_sid_title_and_cwd_case_insensitively() {
        let sessions = vec![
            session("SID-ABC", "Fix auth", "/work/api"),
            session("sid-def", "Docs", "/work/site"),
        ];
        assert_eq!(filter_sessions(&sessions, "abc")[0].sid, "SID-ABC");
        assert_eq!(filter_sessions(&sessions, "AUTH")[0].sid, "SID-ABC");
        assert_eq!(filter_sessions(&sessions, "SITE")[0].sid, "sid-def");
        assert!(filter_sessions(&sessions, "missing").is_empty());
    }

    #[test]
    fn menu_navigation_wraps_and_empty_stays_zero() {
        assert_eq!(move_selection(0, 3, MenuDirection::Previous), 2);
        assert_eq!(move_selection(2, 3, MenuDirection::Next), 0);
        assert_eq!(move_selection(7, 0, MenuDirection::Next), 0);
        assert_eq!(menu_direction("n", true), Some(MenuDirection::Next));
        assert_eq!(menu_direction("p", true), Some(MenuDirection::Previous));
        assert_eq!(menu_direction("n", false), None);
        assert_eq!(menu_direction("ArrowDown", true), None);
    }

    #[test]
    fn prompt_edits_preserve_utf16_caret_semantics() {
        assert_eq!(
            edit_prompt("abcd", 1, 3, PromptEdit::Insert("X")),
            ("aXd".into(), 2)
        );
        assert_eq!(
            edit_prompt("a🙂b", 3, 3, PromptEdit::Backspace),
            ("ab".into(), 1)
        );
        assert_eq!(
            edit_prompt("a🙂b", 1, 1, PromptEdit::Delete),
            ("ab".into(), 1)
        );
    }
}
```

Register it beside `event` in `crates/vmux_agent/src/chat_page.rs`:

```rust
pub(crate) mod composer;
pub mod event;
```

- [ ] **Step 2: Run the tests and verify failure.**

Run: `cargo test -p vmux_agent chat_page::composer`

Expected: FAIL because `selector_mode`, `filter_sessions`, `move_selection`,
`menu_direction`, and `edit_prompt` are undefined.

- [ ] **Step 3: Implement the helpers above the test module.**

```rust
pub(crate) fn selector_mode(draft: &str) -> SelectorMode<'_> {
    let Some(token) = draft.strip_prefix('/') else {
        return SelectorMode::None;
    };
    if let Some(rest) = token.strip_prefix("resume")
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return SelectorMode::Resume(rest.trim_start_matches(char::is_whitespace));
    }
    if token.chars().any(char::is_whitespace) {
        SelectorMode::None
    } else {
        SelectorMode::Commands(token)
    }
}

pub(crate) fn filter_sessions(
    sessions: &[ResumableSessionEntry],
    query: &str,
) -> Vec<ResumableSessionEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return sessions.to_vec();
    }
    sessions
        .iter()
        .filter(|session| {
            session.sid.to_lowercase().contains(&query)
                || session.title.to_lowercase().contains(&query)
                || session.cwd.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

pub(crate) fn menu_direction(key: &str, ctrl: bool) -> Option<MenuDirection> {
    match key {
        "ArrowDown" if !ctrl => Some(MenuDirection::Next),
        "ArrowUp" if !ctrl => Some(MenuDirection::Previous),
        "n" | "N" if ctrl => Some(MenuDirection::Next),
        "p" | "P" if ctrl => Some(MenuDirection::Previous),
        _ => None,
    }
}

pub(crate) fn move_selection(
    current: usize,
    len: usize,
    direction: MenuDirection,
) -> usize {
    if len == 0 {
        return 0;
    }
    match direction {
        MenuDirection::Next => (current + 1) % len,
        MenuDirection::Previous => (current + len - 1) % len,
    }
}

pub(crate) fn edit_prompt(
    value: &str,
    selection_start: u32,
    selection_end: u32,
    edit: PromptEdit<'_>,
) -> (String, u32) {
    let start = utf16_to_byte(value, selection_start);
    let end = utf16_to_byte(value, selection_end);
    let (start, end) = if start <= end { (start, end) } else { (end, start) };
    let (replace_start, replace_end, replacement) = match edit {
        PromptEdit::Insert(text) => (start, end, text),
        PromptEdit::Backspace if start != end => (start, end, ""),
        PromptEdit::Backspace => {
            let previous = value[..start]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(start);
            (previous, start, "")
        }
        PromptEdit::Delete if start != end => (start, end, ""),
        PromptEdit::Delete => {
            let next = value[end..]
                .chars()
                .next()
                .map(|character| end + character.len_utf8())
                .unwrap_or(end);
            (end, next, "")
        }
    };
    let mut updated = String::with_capacity(
        value.len() - (replace_end - replace_start) + replacement.len(),
    );
    updated.push_str(&value[..replace_start]);
    updated.push_str(replacement);
    updated.push_str(&value[replace_end..]);
    let caret_byte = replace_start + replacement.len();
    let caret_utf16 = updated[..caret_byte].encode_utf16().count() as u32;
    (updated, caret_utf16)
}

fn utf16_to_byte(value: &str, offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, character) in value.char_indices() {
        if units >= offset {
            return byte;
        }
        units += character.len_utf16() as u32;
    }
    value.len()
}
```

- [ ] **Step 4: Run the helper tests and verify pass.**

Run: `cargo test -p vmux_agent chat_page::composer`

Expected: PASS (4 tests).

- [ ] **Step 5: Commit.**

```bash
git add crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/composer.rs
git commit -m "refactor(agent): add testable composer selector helpers"
```

### Task H2: Limit resume results to the requesting pane's agent kind

**Files:**
- Modify: `crates/vmux_agent/src/chat_page.rs`

- [ ] **Step 1: Write a failing unit test.** Add inside the existing native test module:

```rust
#[test]
fn resume_results_only_include_current_agent_kind() {
    use crate::client::cli::strategy::ResumableSession;
    use std::time::SystemTime;

    let session = |kind, sid: &str| ResumableSession {
        kind,
        sid: sid.into(),
        cwd: "/work".into(),
        mtime: SystemTime::UNIX_EPOCH,
        title: sid.into(),
        cross_runtime: kind_supports_cross_runtime(kind),
    };
    let filtered = sessions_for_kind(
        vec![
            session(AgentKind::Claude, "claude-1"),
            session(AgentKind::Codex, "codex-1"),
        ],
        AgentKind::Claude,
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].sid, "claude-1");
}
```

- [ ] **Step 2: Run and verify failure.**

Run: `cargo test -p vmux_agent resume_results_only_include_current_agent_kind`

Expected: FAIL because `sessions_for_kind` is undefined.

- [ ] **Step 3: Add the filter and resolve kind from the requesting stack.** Import
  `ResumableSession` under the native cfg, then add:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn sessions_for_kind(
    sessions: Vec<crate::client::cli::strategy::ResumableSession>,
    kind: AgentKind,
) -> Vec<crate::client::cli::strategy::ResumableSession> {
    sessions
        .into_iter()
        .filter(|session| session.kind == kind)
        .collect()
}
```

Extend `on_resume_list_request` with `child_of`, `acp_sessions`, and `agent_sessions` queries.
Resolve the stack and kind before spawning the IO task:

```rust
let kind = child_of
    .get(webview)
    .ok()
    .map(ChildOf::parent)
    .and_then(|stack| {
        acp_sessions
            .get(stack)
            .ok()
            .and_then(|acp| AgentKind::from_url_segment(&acp.agent_id))
            .or_else(|| agent_sessions.get(stack).ok().map(|session| session.kind))
    });
```

Inside the task, replace `strategies.list_all_sessions()` with:

```rust
let sessions = kind
    .map(|kind| sessions_for_kind(strategies.list_all_sessions(), kind))
    .unwrap_or_default();
let sessions = sessions
    .into_iter()
    .map(|session| {
        let dir = session
            .cwd
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| session.cwd.to_string_lossy().to_string());
        ResumableSessionEntry {
            kind: session.kind.as_url_segment().to_string(),
            sid: session.sid,
            cwd: session.cwd.to_string_lossy().to_string(),
            title: session.title,
            subtitle: format!("{} · {}", relative_time(session.mtime), dir),
            cross_runtime: session.cross_runtime,
        }
    })
    .collect();
```

- [ ] **Step 4: Run and verify pass.**

Run: `cargo test -p vmux_agent resume_results_only_include_current_agent_kind`

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/vmux_agent/src/chat_page.rs
git commit -m "fix(agent): filter resume sessions by current agent kind"
```

### Task H3: Make `/resume <query>` the selector state and add complete navigation

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/page.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`
- Modify: `crates/vmux_agent/Cargo.toml`

- [ ] **Step 1: Add failing source-integration assertions.** Extend the existing
  `composer_resume_menu_remains_escapeable_when_empty` test:

```rust
#[test]
fn composer_resume_selector_supports_prompt_filter_and_keyboard_navigation() {
    let source = include_str!("chat_page/page.rs");
    assert!(source.contains("SelectorMode::Resume"));
    assert!(source.contains("filter_sessions"));
    assert!(source.contains("No matching sessions"));
    assert!(source.contains("menu_direction"));
    assert!(source.contains("ScrollLogicalPosition::Nearest"));
    assert!(source.contains("agent-selector-item-{i}"));
}
```

- [ ] **Step 2: Run and verify failure.**

Run: `cargo test -p vmux_agent composer_resume_selector_supports_prompt_filter_and_keyboard_navigation`

Expected: FAIL on the new source markers.

- [ ] **Step 3: Add the required WASM imports and features.** In `page.rs` import:

```rust
use crate::chat_page::composer::{
    SelectorMode, filter_sessions, menu_direction, move_selection, selector_mode,
};
```

Add to the WASM `web-sys` feature list in `crates/vmux_agent/Cargo.toml`:

```toml
"ScrollIntoViewOptions",
"ScrollLogicalPosition",
```

- [ ] **Step 4: Replace `resume_mode` with parser-driven state.** Replace the signal with:

```rust
let mut resume_requested = use_signal(|| false);
```

Add this effect after the session listener:

```rust
use_effect(move || {
    let in_resume_selector = matches!(selector_mode(&draft()), SelectorMode::Resume(_));
    if in_resume_selector && !resume_requested() {
        let _ = try_cef_bin_emit_rkyv(&ResumeListRequest);
        resume_requested.set(true);
    } else if !in_resume_selector && resume_requested() {
        resume_requested.set(false);
    }
});
```

Replace the menu derivation with:

```rust
let draft_val = draft();
let selector = selector_mode(&draft_val);
let command_query = match selector {
    SelectorMode::Commands(query) => Some(query),
    _ => None,
};
let resume_query = match selector {
    SelectorMode::Resume(query) => Some(query),
    _ => None,
};
let filtered_cmds: Vec<SlashCommandEntry> = command_query
    .map(|query| {
        let query = query.to_lowercase();
        slash_cmds
            .read()
            .iter()
            .filter(|command| command.name.starts_with(&query))
            .cloned()
            .collect()
    })
    .unwrap_or_default();
let filtered_sessions = resume_query
    .map(|query| filter_sessions(&sessions.read(), query))
    .unwrap_or_default();
let cmd_menu_open = command_query.is_some() && !filtered_cmds.is_empty();
let session_menu_open = resume_query.is_some();
```

- [ ] **Step 5: Render filtered sessions, default selection, empty messages, and row ids.**
  Replace both selector blocks with:

```rust
if cmd_menu_open {
    div { class: "absolute bottom-full left-0 z-20 mb-2 w-full overflow-hidden rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
        for (i, command) in filtered_cmds.iter().enumerate() {
            {
                let command = command.clone();
                rsx! {
                    div {
                        key: "sc{i}",
                        id: "agent-selector-item-{i}",
                        class: if i == menu_sel() { "flex cursor-pointer items-baseline gap-3 px-3.5 py-2 text-sm bg-foreground/10" } else { "flex cursor-pointer items-baseline gap-3 px-3.5 py-2 text-sm" },
                        onclick: move |_| run_slash_command(&command.name, draft, menu_sel),
                        span { class: "font-medium text-foreground", "/{command.name}" }
                        span { class: "text-xs text-muted-foreground", "{command.description}" }
                    }
                }
            }
        }
    }
}
if session_menu_open {
    div { class: "absolute bottom-full left-0 z-20 mb-2 max-h-80 w-full overflow-y-auto rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
        if sessions.read().is_empty() {
            div { class: "px-3.5 py-2 text-sm text-muted-foreground", "No resumable sessions found" }
        } else if filtered_sessions.is_empty() {
            div { class: "px-3.5 py-2 text-sm text-muted-foreground", "No matching sessions" }
        } else {
            for (i, session) in filtered_sessions.iter().enumerate() {
                {
                    let session = session.clone();
                    rsx! {
                        div {
                            key: "rs{i}",
                            id: "agent-selector-item-{i}",
                            class: if i == menu_sel() { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2" },
                            onclick: move |_| select_resume_session(&session, draft),
                            span { class: "truncate text-sm text-foreground", "{session.title}" }
                            span { class: "truncate text-xs text-muted-foreground", "{session.subtitle}" }
                        }
                    }
                }
            }
        }
    }
}
```

Change textarea `oninput` so every query change selects the first result:

```rust
oninput: move |event| {
    draft.set(event.value());
    menu_sel.set(0);
},
```

- [ ] **Step 6: Add selected-row scrolling.** Add this effect before `rsx!`:

```rust
use_effect(move || {
    let selected = menu_sel();
    let _ = draft.read();
    let _ = sessions.read().len();
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document.get_element_by_id(&format!("agent-selector-item-{selected}"))
        })
    {
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Nearest);
        element.scroll_into_view_with_scroll_into_view_options(&options);
    }
});
```

- [ ] **Step 7: Replace textarea selector handling.** Derive `cmd_items` and `sess_items` from
  `selector_mode(&draft_now)`. Before normal submit handling, use:

```rust
let (cmd_items, sess_items, session_selector_open) = match selector_mode(&draft_now) {
    SelectorMode::Commands(query) => {
        let query = query.to_lowercase();
        (
            slash_cmds
                .peek()
                .iter()
                .filter(|command| command.name.starts_with(&query))
                .cloned()
                .collect::<Vec<_>>(),
            Vec::new(),
            false,
        )
    }
    SelectorMode::Resume(query) => (
        Vec::new(),
        filter_sessions(&sessions.peek(), query),
        true,
    ),
    SelectorMode::None => (Vec::new(), Vec::new(), false),
};
let selector_open = session_selector_open || !cmd_items.is_empty();
let selector_len = if session_selector_open {
    sess_items.len()
} else {
    cmd_items.len()
};
let key = e.key().to_string();
let command_modifier = e.modifiers().meta() || e.modifiers().ctrl() || e.modifiers().alt();
let direction = if e.modifiers().meta() || e.modifiers().alt() {
    None
} else {
    menu_direction(&key, e.modifiers().ctrl())
};

if selector_open && let Some(direction) = direction {
    e.prevent_default();
    menu_sel.set(move_selection(*menu_sel.peek(), selector_len, direction));
    return;
}
if selector_open
    && e.key() == Key::Enter
    && !e.modifiers().shift()
    && !command_modifier
{
    e.prevent_default();
    let selected = *menu_sel.peek();
    if session_selector_open {
        if let Some(session) = sess_items.get(selected) {
            select_resume_session(session, draft);
        }
    } else if let Some(command) = cmd_items.get(selected) {
        run_slash_command(&command.name, draft, menu_sel);
    }
    return;
}
if selector_open && e.key() == Key::Escape && !command_modifier {
    e.prevent_default();
    draft.set(String::new());
    menu_sel.set(0);
    return;
}
if session_selector_open && matches!(e.key(), Key::Enter | Key::Escape) {
    return;
}
```

The `session_selector_open` branch remains true with zero matches. Enter is prevented and does
nothing, so `/resume <query>` is never submitted to the agent.

- [ ] **Step 8: Keep `/resume ` in the prompt.** Replace the command dispatcher and selection
  helper signatures:

```rust
fn run_slash_command(
    name: &str,
    mut draft: Signal<String>,
    mut menu_sel: Signal<usize>,
) {
    match name {
        "resume" => {
            menu_sel.set(0);
            draft.set("/resume ".to_string());
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

fn select_resume_session(session: &ResumableSessionEntry, mut draft: Signal<String>) {
    let _ = try_cef_bin_emit_rkyv(&ResumeSession {
        kind: session.kind.clone(),
        sid: session.sid.clone(),
        cwd: session.cwd.clone(),
    });
    draft.set(String::new());
}
```

The command-row click handler is
`run_slash_command(&command.name, draft, menu_sel)`. The session-row click handler is
`select_resume_session(&session, draft)`, as shown in Step 5.

- [ ] **Step 9: Run tests and WASM check.**

Run:

```bash
cargo test -p vmux_agent chat_page::composer
cargo test -p vmux_agent composer_resume_selector_supports_prompt_filter_and_keyboard_navigation
cargo check -p vmux_agent --target wasm32-unknown-unknown
```

Expected: all PASS.

- [ ] **Step 10: Commit.**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/page.rs
git commit -m "feat(agent): filter resume selector from prompt arguments"
```

### Task H4: Route page-wide typing into the prompt

**Files:**
- Modify: `crates/vmux_agent/Cargo.toml`
- Modify: `crates/vmux_agent/src/chat_page/page.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`

- [ ] **Step 1: Add a failing source-integration assertion.** Add:

```rust
#[test]
fn composer_captures_global_prompt_input_without_stealing_shortcuts() {
    let source = include_str!("chat_page/page.rs");
    assert!(source.contains("install_global_prompt_input"));
    assert!(source.contains("meta_key() || event.ctrl_key() || event.alt_key()"));
    assert!(source.contains("PromptEdit::Backspace"));
    assert!(source.contains("PromptEdit::Delete"));
    assert!(source.contains("dispatch_keyboard_event"));
}
```

- [ ] **Step 2: Run and verify failure.**

Run: `cargo test -p vmux_agent composer_captures_global_prompt_input_without_stealing_shortcuts`

Expected: FAIL on the new source markers.

- [ ] **Step 3: Add WASM dependencies and DOM features.** Add `wasm-bindgen` and extend
  `web-sys`:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = { workspace = true }
web-sys = { version = "0.3", features = [
    "Window",
    "Document",
    "Element",
    "Event",
    "EventInit",
    "EventTarget",
    "HtmlTextAreaElement",
    "KeyboardEvent",
    "KeyboardEventInit",
    "ScrollIntoViewOptions",
    "ScrollLogicalPosition",
    "Selection",
] }
```

- [ ] **Step 4: Add textarea DOM helpers.** Import `Closure`, `JsCast`, `PromptEdit`, and
  `edit_prompt`. Add:

```rust
use wasm_bindgen::{JsCast, closure::Closure};

const PROMPT_ID: &str = "agent-chat-prompt";

fn prompt_textarea() -> Option<web_sys::HtmlTextAreaElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(PROMPT_ID)?
        .dyn_into()
        .ok()
}

fn dispatch_input_event(textarea: &web_sys::HtmlTextAreaElement) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    if let Ok(event) = web_sys::Event::new_with_event_init_dict("input", &init) {
        let _ = textarea.dispatch_event(&event);
    }
}

fn dispatch_keyboard_event(
    textarea: &web_sys::HtmlTextAreaElement,
    source: &web_sys::KeyboardEvent,
) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key(&source.key());
    init.set_code(&source.code());
    init.set_ctrl_key(source.ctrl_key());
    init.set_shift_key(source.shift_key());
    init.set_alt_key(source.alt_key());
    init.set_meta_key(source.meta_key());
    if let Ok(event) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
    {
        let _ = textarea.dispatch_event(&event);
    }
}
```

- [ ] **Step 5: Install the global listener.** Add:

```rust
fn install_global_prompt_input(
    draft: Signal<String>,
    slash_cmds: Signal<Vec<SlashCommandEntry>>,
) {
    let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        let Some(textarea) = prompt_textarea() else {
            return;
        };
        let prompt_focused = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .is_some_and(|element| element.id() == PROMPT_ID);
        if prompt_focused {
            return;
        }

        let selector_open = match selector_mode(&draft.peek()) {
            SelectorMode::Resume(_) => true,
            SelectorMode::Commands(query) => {
                let query = query.to_lowercase();
                slash_cmds
                    .peek()
                    .iter()
                    .any(|command| command.name.starts_with(&query))
            }
            SelectorMode::None => false,
        };
        let key = event.key();
        let direction = if event.meta_key() || event.alt_key() {
            None
        } else {
            menu_direction(&key, event.ctrl_key())
        };
        let plain_invoke_or_close = !event.meta_key()
            && !event.ctrl_key()
            && !event.alt_key()
            && matches!(key.as_str(), "Enter" | "Escape");
        let selector_key = direction.is_some() || plain_invoke_or_close;
        if selector_open && selector_key {
            event.prevent_default();
            event.stop_propagation();
            let _ = textarea.focus();
            dispatch_keyboard_event(&textarea, &event);
            return;
        }

        if event.meta_key() || event.ctrl_key() || event.alt_key() {
            return;
        }
        let edit = match key.as_str() {
            "Backspace" => PromptEdit::Backspace,
            "Delete" => PromptEdit::Delete,
            _ if key.chars().count() == 1 => PromptEdit::Insert(&key),
            _ => return,
        };
        event.prevent_default();
        event.stop_propagation();
        let start = textarea
            .selection_start()
            .ok()
            .flatten()
            .unwrap_or_else(|| textarea.value().encode_utf16().count() as u32);
        let end = textarea.selection_end().ok().flatten().unwrap_or(start);
        let (value, caret) = edit_prompt(&textarea.value(), start, end, edit);
        let _ = textarea.focus();
        textarea.set_value(&value);
        let _ = textarea.set_selection_range(caret, caret);
        dispatch_input_event(&textarea);
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback(
            "keydown",
            closure.as_ref().unchecked_ref(),
        );
    }
    closure.forget();
}
```

Install it once in `Page()`:

```rust
use_effect(move || install_global_prompt_input(draft, slash_cmds));
```

Set `id: PROMPT_ID` on the textarea. Existing textarea-targeted events remain owned by the
Dioxus handler, so no key is inserted twice. Selector Ctrl+N/P is rerouted before the general
modifier exclusion; other Cmd/Ctrl/Alt shortcuts remain untouched.

- [ ] **Step 6: Run tests and WASM check.**

Run:

```bash
cargo test -p vmux_agent chat_page::composer
cargo test -p vmux_agent composer_captures_global_prompt_input_without_stealing_shortcuts
cargo check -p vmux_agent --target wasm32-unknown-unknown
```

Expected: all PASS.

- [ ] **Step 7: Commit.**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/page.rs
git commit -m "feat(agent): route page-wide typing into chat prompt"
```

### Task H5: Targeted verification and push

**Files:**
- Modify only files required by formatter or compiler findings from H1-H4.

- [ ] **Step 1: Format.**

Run: `cargo fmt --all`

Expected: PASS. Restore unrelated formatting under `patches/` if Cargo fmt touches it.

- [ ] **Step 2: Run targeted native tests.**

Run: `cargo test -p vmux_agent`

Expected: PASS.

- [ ] **Step 3: Run targeted clippy.**

Run: `cargo clippy -p vmux_agent --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 4: Compile the WASM page.**

Run: `cargo check -p vmux_agent --target wasm32-unknown-unknown`

Expected: PASS.

- [ ] **Step 5: Inspect the final diff and commit formatter/compiler fixes if present.**

Run: `git diff --check` and `git status --short`.

If tracked fixes remain:

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/
git commit -m "fix(agent): finish resume selector refinement"
```

- [ ] **Step 6: Push the branch.**

Run: `git push origin feat/acp-cli-resume`

Expected: PR #241 updates to the new head.

---

## Self-review

**Spec coverage:**
- Runtime-agnostic identity → A1 (`ResumableSession`), B1 (`for_session`). ✓
- Backend lister (3 kinds, on-demand, sorted) → A2/A3/A4. ✓
- Composer slash menu (`/resume`, `/cli`, extensible, unknown→prompt) → D1, E2. ✓
- Prompt-driven `/resume <query>` parser + case-insensitive sid/title/cwd filtering → H1, H3. ✓
- Current-agent-kind-only results → H2. ✓
- Immediate selection, Arrow/Ctrl+N/P wraparound, nearest scrolling, empty result state → H1, H3. ✓
- Page-wide prompt typing, Backspace/Delete, caret preservation, shortcut exclusions → H1, H4. ✓
- In-place swap + cwd carry → B1/B2, E1. ✓
- Cross-runtime handoff gated by `cross_runtime` (claude) → E1/E2, `kind_supports_cross_runtime`. ✓
- Cmd+K browser and runtime-switch commands → deferred from this PR. ✓
- Edge cases: already-open focus (existing `AgentSessionToEntity` path in page-open — the swap intentionally forces a fresh attach; add focus-existing only if desired — noted), stale sid (ACP falls back to new; CLI warn), cwd carried, missing binary (existing setup path). ✓
- Tests: parsers (A2/A3), url round-trip (B1), swap ECS (B2), runtime-switch ECS (E1), rkyv (C1), source-scrape (D1). ✓

**Completeness scan:** Every Phase H code change has concrete types, signatures, assertions,
commands, and expected results. Earlier context-reading steps remain deliberate prerequisites for
their exact upstream field paths and accessors.

**Type consistency:** `ResumableSession` (native, PathBuf/SystemTime) vs `ResumableSessionEntry` (wire, String) kept distinct and mapped in E1. `SwapStackSession.target_url: String` (built via `AgentUrl::format`, parsed in the handler) avoids a `vmux_core → vmux_agent` dep. `kind_supports_cross_runtime` is the single source for the `cross_runtime` flag (A2/A3 refactored in E2.2). Slash command names (`resume`/`cli`/`acp`) match between D1 `run_slash_command`, E2 `slash_commands_for`, and E1 `on_runtime_switch_request`.

Phase H keeps `SelectorMode`, `MenuDirection`, and UTF-16-aware `PromptEdit` in a native/WASM
shared module. DOM-only code remains in `page.rs`. `ResumeListRequest` stays payload-free because
native ECS resolves the requesting stack's `AcpSession`/`AgentSession` kind before scanning.

**Known follow-ups (not blockers):** Cmd+K session-list browser + runtime-switch commands; ACP `availableCommands` surfacing; codex/vibe cross-runtime after verification; per-row runtime pick; focus-existing-tab on resume instead of re-attach.
