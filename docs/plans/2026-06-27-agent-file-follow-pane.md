# Agent File Follow-Pane Implementation Plan

> **Execution:** Implement **directly** in this worktree (no subagents — CEF builds break long-running agents), with frequent commits. Defer all manual/runtime testing to **one pass at the end** (Task 9). Steps use `- [ ]` for tracking.

**Goal:** When an agent reads/edits a file, instantly open or update a single `file://` follow-pane beside that agent, scrolled to the touched region, ringed in the agent's color — without stealing the human's focus.

**Architecture:** CLI tool hooks (Claude PostToolUse, Vibe `after_tool`, Codex PostToolUse) call a shared `vmux notify-file-touch` notifier → `AgentCommand::FileTouched` over the service socket → `AgentCommandRequest` in ECS → a `handle_agent_file_touch` system that opens/reuses the agent's `file://` pane (mirroring the existing `claim_browser_pane`/`OpenBeside` flow) and emits `ActivatePane{Agent(anchor)}` (no `LastActivatedAt` stamp).

**Tech Stack:** Rust, Bevy ECS, clap, serde_json, rkyv (service protocol), CEF.

**Spec:** `docs/specs/2026-06-27-agent-file-follow-pane-design.md`.

---

## Preconditions

- **Rebased onto `feat/per-profile-active-pane` AFTER the FocusedStack removal lands.** That provides `ActivePanes`, `ActivePanes::local()`, `ProfileId::Agent`, `ActivatePane`, and the per-agent focus-ring rendering. Verify before starting:
  ```bash
  git -C .worktrees/agent-file-follow-pane log --oneline | grep -i focusedstack   # removal present on base
  grep -rn "fn local" crates/vmux_layout/src/active_panes.rs                       # ActivePanes::local exists
  ```
- **Line numbers below are pre-rebase.** Re-confirm each with `grep` after rebasing; the FocusedStack removal touches `vmux_agent/plugin.rs` and others.

## File Structure

- Modify `crates/vmux_service/src/protocol.rs` — add `FileTouchKind` + `AgentCommand::FileTouched`.
- Create `crates/vmux_cli/src/commands/notify_file_touch.rs` — the hook notifier + stdin parsing.
- Modify `crates/vmux_cli/src/commands.rs` + `crates/vmux_cli/src/main.rs` — register the subcommand.
- Modify `crates/vmux_agent/src/plugin.rs` — `handle_agent_file_touch` system + registration.
- Modify `crates/vmux_agent/src/client/cli/{claude,codex,vibe}.rs` — per-agent hook injection.
- Modify `crates/vmux_agent/src/mcp.rs` (or new `hooks.rs`) — resolve the `vmux` binary path + build hook config (reuse `resolve`'s sidecar logic).
- Modify the embedded `settings.ron` + `AppSettings` `agent` section — `follow_files: bool`.

---

### Task 1: `AgentCommand::FileTouched` protocol variant

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs:55` (enum `AgentCommand`), `:283` (validation match)

- [ ] **Step 1: Add the variant + kind enum**

In `protocol.rs`, near `AgentCommand` (after the `Notify` variant at `:141`), and add `FileTouchKind` next to `AgentPaneDirection`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum FileTouchKind {
    Read,
    Edit,
}

// inside `pub enum AgentCommand { ... }`
    FileTouched {
        anchor: ProcessId,
        path: String,
        line: Option<u32>,
        kind: FileTouchKind,
    },
```

- [ ] **Step 2: Handle the validation match + any exhaustive matches**

At `protocol.rs:283` the validation rejects empty-URL `OpenBeside`. Add an arm so an empty `path` is rejected the same way:

```rust
        AgentCommand::FileTouched { path, .. } if path.trim().is_empty() => {
            return Err(AgentCommandValidationError::Empty);   // match the existing error variant name
        }
```

Run `cargo build -p vmux_service`; fix every other non-exhaustive `match` the compiler flags over `AgentCommand` with a `FileTouched { .. } => { /* routed in ECS */ }` arm (or `_` where appropriate).

- [ ] **Step 3: Roundtrip test**

```rust
#[test]
fn file_touched_roundtrips() {
    let cmd = AgentCommand::FileTouched {
        anchor: ProcessId::new(),
        path: "/abs/x.rs".into(),
        line: Some(42),
        kind: FileTouchKind::Edit,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
    let back: AgentCommand = rkyv::from_bytes::<_, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(cmd, back);
}
```

- [ ] **Step 4: Run** `cargo test -p vmux_service file_touched_roundtrips` → PASS
- [ ] **Step 5: Commit** `feat(protocol): add AgentCommand::FileTouched`

---

### Task 2: `vmux notify-file-touch` CLI subcommand

**Files:**
- Create: `crates/vmux_cli/src/commands/notify_file_touch.rs`
- Modify: `crates/vmux_cli/src/commands.rs:3` (mod), `:16` (Command enum); `crates/vmux_cli/src/main.rs` (dispatch)

- [ ] **Step 1: Pure parser + failing test** (in the new file)

```rust
use vmux_service::protocol::FileTouchKind;

/// Parse a tool-hook JSON payload into a file touch. `None` if it isn't a
/// file read/edit or carries no absolute path.
pub fn parse_touch(v: &serde_json::Value) -> Option<(String, Option<u32>, FileTouchKind)> {
    let tool = v.get("tool_name").and_then(|t| t.as_str()).unwrap_or("");
    let input = v.get("tool_input")?;
    let path = input.get("file_path").and_then(|p| p.as_str())?;
    if !path.starts_with('/') {
        return None;
    }
    let kind = match tool {
        "Read" => FileTouchKind::Read,
        "Edit" | "Write" | "MultiEdit" | "apply_patch" => FileTouchKind::Edit,
        // vibe lowercases tool names
        "read" => FileTouchKind::Read,
        "edit" | "write" => FileTouchKind::Edit,
        _ => return None,
    };
    let line = input
        .get("offset")
        .and_then(|o| o.as_u64())
        .map(|o| o as u32);
    Some((path.to_string(), line, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_read_with_offset() {
        let v = serde_json::json!({
            "tool_name": "Read",
            "tool_input": { "file_path": "/a/b.rs", "offset": 120 }
        });
        assert_eq!(parse_touch(&v), Some(("/a/b.rs".into(), Some(120), FileTouchKind::Read)));
    }

    #[test]
    fn claude_edit_no_offset() {
        let v = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": "/a/b.rs", "old_string": "x", "new_string": "y" }
        });
        assert_eq!(parse_touch(&v), Some(("/a/b.rs".into(), None, FileTouchKind::Edit)));
    }

    #[test]
    fn codex_apply_patch() {
        let v = serde_json::json!({
            "tool_name": "apply_patch",
            "tool_input": { "file_path": "/a/b.rs" }
        });
        assert_eq!(parse_touch(&v).unwrap().2, FileTouchKind::Edit);
    }

    #[test]
    fn relative_path_skipped() {
        let v = serde_json::json!({ "tool_name": "Read", "tool_input": { "file_path": "b.rs" } });
        assert_eq!(parse_touch(&v), None);
    }

    #[test]
    fn non_file_tool_skipped() {
        let v = serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
        assert_eq!(parse_touch(&v), None);
    }
}
```

- [ ] **Step 2: Run** `cargo test -p vmux_cli parse_touch` (and the module tests) → expect FAIL (module not wired) then PASS once compiled.

- [ ] **Step 3: `run()` — read stdin, resolve anchor, send** (mirror `notify.rs` exactly for connection + anchor + timeout)

```rust
use std::io::{self, Read};
use vmux_service::client::ServiceConnection;
use vmux_service::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentRequestId, ClientMessage, ProcessId, ServiceMessage,
};

pub async fn run(anchor: Option<String>) -> io::Result<()> {
    let anchor = match anchor {
        Some(raw) => raw.parse::<ProcessId>().ok(),
        None => std::env::var("VMUX_ANCHOR").ok().and_then(|s| s.parse().ok()),
    };
    let Some(anchor) = anchor else { return Ok(()) }; // no anchor → not under vmux, no-op

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&buf) else { return Ok(()) };
    let Some((path, line, kind)) = parse_touch(&v) else { return Ok(()) };

    let Ok(connection) = ServiceConnection::connect().await else { return Ok(()) };
    let request_id = AgentRequestId::new();
    let _ = connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor: Some(anchor),
            command: AgentCommand::FileTouched { anchor, path, line, kind },
        })
        .await;
    let _ = tokio::time::timeout(AGENT_COMMAND_TIMEOUT, async {
        while let Ok(Some(m)) = connection.recv().await {
            if let ServiceMessage::AgentCommandResult { request_id: r, .. } = m {
                if r == request_id { break; }
            }
        }
    })
    .await;
    Ok(())
}
```

- [ ] **Step 4: Register the subcommand**

`commands.rs`: add `pub mod notify_file_touch;` and the variant:
```rust
    NotifyFileTouch {
        #[arg(long)]
        anchor: Option<String>,
    },
```
`main.rs`: add the dispatch arm `Command::NotifyFileTouch { anchor } => commands::notify_file_touch::run(anchor).await?,` (match the existing async dispatch style).

- [ ] **Step 5: Run** `cargo test -p vmux_cli` → PASS; `cargo build -p vmux_cli` → builds.
- [ ] **Step 6: Commit** `feat(cli): vmux notify-file-touch (hook notifier)`

---

### Task 3 + 4: `handle_agent_file_touch` follow-pane system (core)

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (new SystemParam + system, mirror `AgentBrowserResolve`/`claim_browser_pane` at `:549-620` and `handle_agent_self_commands` at `:1011`)

- [ ] **Step 1: SystemParam to resolve/reuse the agent's file pane**

Mirror `browser_pane_for` (`:566`) but filter siblings to the editor host (`file://` pages / `PageKind::File`). Use the editor's page marker component (confirm name in `vmux_editor`; the page host is `"files"`). Sketch:

```rust
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentFileResolve<'w, 's> {
    activate: MessageWriter<'w, vmux_layout::active_panes::ActivatePane>,
    open_beside: MessageWriter<'w, vmux_layout::OpenBesideRequest>,
    file_open: MessageWriter<'w, vmux_editor::FileOpenEvent>, // confirm exported name
    agent_terms: Query<'w, 's, (&'static vmux_service::protocol::ProcessId, &'static ChildOf), With<AgentSession>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    file_stacks: Query<'w, 's, (Entity, &'static ChildOf), With<vmux_editor::FileView>>, // sibling file panes
}
```

`file_pane_for(agent_pane)`: copy `browser_pane_for` verbatim, swapping `browser_stacks` for `file_stacks` (same "sibling leaf under the same parent split" walk).

- [ ] **Step 2: The system**

```rust
fn handle_agent_file_touch(
    mut reader: MessageReader<AgentCommandRequest>,
    mut resolve: AgentFileResolve,
    settings: Res<AppSettings>,
) {
    if !settings.agent.follow_files { for _ in reader.read() {} return; }
    for request in reader.read() {
        let ServiceAgentCommand::FileTouched { anchor, path, line, .. } = &request.command else { continue };
        let url = format!("file://{path}");
        let Some(agent_pane) = resolve.agent_pane(*anchor) else { continue };
        match resolve.file_pane_for(agent_pane) {
            None => {
                resolve.open_beside.write(vmux_layout::OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,                 // placement: File kind → own leaf, never splits agent
                    url,
                    request_id: request.request_id.0,
                    focus: false,
                });
                // first-open lands at top; a subsequent touch scrolls (MVP).
            }
            Some(file_pane) => {
                resolve.file_open.write(/* FileOpenEvent { stack/pane, path, top_line: line } */);
                resolve.activate.write(vmux_layout::active_panes::ActivatePane {
                    profile: vmux_layout::active_panes::ProfileId::Agent(format!("{anchor:?}")),
                    active: vmux_layout::active_panes::ActiveStack {
                        tab: None, pane: Some(file_pane), stack: None,
                    },
                });
            }
        }
    }
}
```

Confirm post-rebase: the exact `FileOpenEvent` shape + how to set `FileViewport.top_line = line` (editor `:562` viewport-scroll path / `:696`). Use `format!("{anchor:?}")` for the `ProfileId::Agent` key — **identical** to `claim_browser_pane` so an agent's browser pane and file pane share one profile/ring.

- [ ] **Step 3: Register the system** in `AgentPlugin::build` near `handle_agent_self_commands` (`:180`), and ensure existing `AgentCommandRequest` readers ignore `FileTouched` (add `FileTouched { .. } => continue` / `_` arms the compiler flags). Per AGENTS.md, chain the `App` builder calls.

- [ ] **Step 4: Integration tests** (Bevy, in `plugin.rs` tests — mirror the existing `claim_browser_pane`/agent-pane tests at `:2472+`)

```rust
#[test]
fn file_touch_opens_pane_beside_agent_without_stealing_local_focus() {
    // spawn agent term+pane; write AgentCommandRequest{FileTouched{anchor, "/a.rs", None, Read}};
    // app.update();
    // assert: an OpenBesideRequest was emitted with focus:false and url "file:///a.rs";
    //         ActivePanes[Local] unchanged; ActivePanes[Agent(anchor)] set.
}

#[test]
fn second_touch_reuses_same_file_pane() {
    // with an existing file pane sibling: touch a different path →
    // a FileOpenEvent (not a second OpenBesideRequest); same file_pane entity.
}

#[test]
fn despawned_agent_is_noop() {
    // FileTouched for an unknown anchor → no messages emitted.
}
```

- [ ] **Step 5: Run** `cargo test -p vmux_agent file_touch` → PASS
- [ ] **Step 6: Commit** `feat(agent): file follow-pane on AgentCommand::FileTouched`

---

### Task 5: `agent.follow_files` setting

**Files:**
- Modify: embedded `settings.ron` (agent section), `AppSettings` agent struct (per [[feedback_no_config_autoseed]] / [[feedback_rebase_before_fixing]]: default lives in embedded `settings.ron`, not `serde(default_*)`).

- [ ] **Step 1:** Add `follow_files: bool` to the `agent` settings struct; set `follow_files: true` in the embedded default `settings.ron`. Absent key ⇒ on.
- [ ] **Step 2: Test** that with `follow_files=false`, `handle_agent_file_touch` drains the reader and emits nothing (extend Task 4's test harness with the setting off).
- [ ] **Step 3:** `cargo test -p vmux_agent` → PASS
- [ ] **Step 4: Commit** `feat(settings): agent.follow_files toggle (default on)`

---

### Task 6: Claude hook injection

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/claude.rs:34` (`build_args`); reuse `mcp.rs` binary resolution.

- [ ] **Step 1:** When `settings.agent.follow_files`, append `--settings` with inline JSON (build with `serde_json` so the `vmux` path is escaped). The `vmux` path + anchor come from the same resolution `mcp::resolve` already uses:

```json
{"hooks":{"PostToolUse":[{"matcher":"Read|Edit|Write|MultiEdit",
  "hooks":[{"type":"command","command":"<vmux-abs-path>",
    "args":["notify-file-touch","--anchor","<anchor>"],"async":true}]}]}}
```

- [ ] **Step 2: Test** (mirror existing `claude.rs` `build_args` tests): assert the args contain `--settings`, the matcher string, `notify-file-touch`, and the anchor; and that with `follow_files=false` they do **not**.
- [ ] **Step 3:** `cargo test -p vmux_agent claude` → PASS
- [ ] **Step 4: Commit** `feat(agent): inject Claude PostToolUse file-touch hook`

---

### Task 7: Vibe hook injection

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/vibe.rs:35` (`build_args`/`build_env`); add a managed `~/.vibe/hooks.toml` writer.

- [ ] **Step 1:** In `build_env`, set `VIBE_ENABLE_EXPERIMENTAL_HOOKS=true` when `follow_files`.
- [ ] **Step 2:** Idempotently write a vmux-managed `~/.vibe/hooks.toml` block (an `after_tool` hook, `match` = read/edit tools, `command` = `<vmux> notify-file-touch`). The hook no-ops without `VMUX_ANCHOR` (vibe inherits the launch env, which carries `VMUX_ANCHOR` from `launch.rs:21`). Never clobber user-authored hooks — write only a delimited managed section.
- [ ] **Step 3: Test** the writer: produces the expected block; running twice is idempotent; preserves surrounding content.
- [ ] **Step 4:** `cargo test -p vmux_agent vibe` → PASS
- [ ] **Step 5: Commit** `feat(agent): inject Vibe after_tool file-touch hook`

---

### Task 8: Codex hook injection (edits-only)

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/codex.rs:30` (`build_args`).

- [ ] **Step 1:** When `follow_files`, add `-c features.hooks=true` and a hooks config (inline `-c` table or a written `hooks.json` referenced by `-c`) with a `PostToolUse` matcher `apply_patch|Edit|Write` → `<vmux> notify-file-touch --anchor <id>`. **Edits only** (Codex has no structured read tool).
- [ ] **Step 2: Test:** args contain `features.hooks=true` + the matcher + `notify-file-touch`.
- [ ] **Step 3:** `cargo test -p vmux_agent codex` → PASS
- [ ] **Step 4: Commit** `feat(agent): inject Codex apply_patch file-touch hook (edits-only)`

---

### Task 9: Verification pass (one runtime test at the end)

- [ ] **Step 1:** `cargo fmt`, then `git checkout -- patches/` (per [[feedback_cargo_fmt_patches]]); `cargo clippy --workspace`; `cargo test --workspace` (per [[feedback_workspace_test_before_push]]).
- [ ] **Step 2 (user runtime-tests — [[feedback_finish_then_test]] / [[feedback_verify_observable_behavior]]):**
  - Claude agent: "read crates/vmux_desktop/src/main.rs" → file appears in a `file://` pane **beside** the agent; agent keeps input focus; the follow-pane shows the **agent's color** ring.
  - Ask it to edit a file → follow-pane scrolls to the change.
  - Vibe agent: same read/edit → follow-pane.
  - Codex agent: edit a file → follow-pane (read won't follow — expected).
  - Two agents at once → two follow-panes, each its own colored ring; neither moves the human's ring.
- [ ] **Step 3:** Push with `--no-verify` if the pre-push hook pollutes the tree (per [[feedback_prepush_hook_pollution]]); open PR (per [[feedback_create_pr_directly]]) noting it stacks on the per-profile PR.

---

## Self-Review

- **Spec coverage:** detection (Tasks 2,6,7,8) · `FileTouched` transport (1) · follow-pane open/reuse + scroll + `ActivatePane` (3/4) · per-agent ring (free via `ActivatePane`) · toggle (5) · multi-agent (4 test) · Codex edits-only (8). ✔
- **Confirm post-rebase (not placeholders — real APIs that shift):** `FileOpenEvent` exported name/shape + `FileViewport.top_line` scroll wiring (`vmux_editor`); the `FileView`/editor page marker used in `file_stacks`; exact `AppSettings.agent` struct path; non-exhaustive `match` arms the compiler flags after adding `FileTouched`.
- **Type consistency:** `ProfileId::Agent(format!("{anchor:?}"))` matches `claim_browser_pane`; `OpenBesideRequest{pane,direction,url,request_id,focus}` matches `pane.rs`; `FileTouchKind`/`FileTouched` match Task 1.
- **First-open scroll gap (known):** first `OpenBeside` lands at top; scroll-to-line applies on the reuse path. If first-open-scroll matters, add a follow-up: stash `(path,line)` and scroll when the `FileView` for `path` first exists. Deferred — not worth blocking MVP.
