# Cross-Agent Session Handoff Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an ACP chat agent continue any Codex, Claude, or Vibe session shown by `/resume` while preserving visible history and lazily creating the target session on the next user prompt.

**Architecture:** Provider strategies parse local session stores into normalized user/assistant messages. A cross-agent `/resume` selection swaps the stack to a fresh instance of the current ACP agent with imported-history and pending-context components; prompt transport sends private context separately from visible text. A small profile-local sidecar restores imported-history presentation when the target session is resumed later.

**Tech Stack:** Rust, Bevy ECS/messages, rkyv IPC, ACP, Dioxus WASM UI, serde/serde_json.

---

### Task 1: Handoff model and context builder

**Files:**
- Create: `crates/vmux_agent/src/handoff.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_agent/src/components.rs`
- Modify: `crates/vmux_agent/src/client/cli/strategy.rs`
- Modify: `crates/vmux_agent/src/strategy.rs`

- [ ] **Step 1: Write failing context-budget and replay-sanitization tests**

Add tests covering newest-complete-turn retention, chronological order, omission marking, imported-message overlay, and replacement of a replayed private first prompt.

```rust
#[test]
fn context_budget_keeps_newest_complete_messages() {
    let messages = vec![user("old"), assistant("middle"), user("new")];
    let built = build_context(&messages, 80);
    assert!(built.truncated);
    assert!(built.text.contains("new"));
    assert!(built.text.contains(OMITTED_MARKER));
}

#[test]
fn replay_private_prompt_is_replaced_with_display_prompt() {
    let mut messages = vec![user(&wire_prompt("context", "continue")), assistant("done")];
    sanitize_replayed_messages(&mut messages, Some("continue"));
    assert_eq!(messages[0], user("continue"));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p vmux_agent handoff --lib`

Expected: FAIL because `handoff` types and helpers do not exist.

- [ ] **Step 3: Implement focused handoff types and helpers**

Create:

```rust
pub const HANDOFF_PROMPT_PREFIX: &str = "<vmux_handoff_context>";
pub const OMITTED_MARKER: &str = "[Older source turns omitted]";
pub const DEFAULT_CONTEXT_LIMIT: usize = 64 * 1024;

#[derive(Component, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ImportedConversation {
    pub source_agent: String,
    pub source_kind: AgentKind,
    pub source_sid: String,
    pub messages: Vec<Message>,
    pub truncated: bool,
    pub first_prompt: Option<String>,
}

#[derive(Component, Clone, Debug)]
pub struct PendingHandoff {
    pub context: String,
    pub sent: bool,
}

pub struct BuiltContext {
    pub text: String,
    pub truncated: bool,
}
```

Implement `build_context`, `wire_prompt`, `visible_messages`, and `sanitize_replayed_messages`. Only `Message::User` and assistant `Text` blocks enter private context.

Extend `CliAgentStrategy` with:

```rust
fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
    Err(format!("transcript loading unsupported for {session_id}"))
}
```

Add `AgentStrategies::load_transcript(kind, sid)` delegation.

- [ ] **Step 4: Run tests and verify pass**

Run: `cargo test -p vmux_agent handoff --lib`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/handoff.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/src/components.rs crates/vmux_agent/src/client/cli/strategy.rs crates/vmux_agent/src/strategy.rs
git commit -m "feat(agent): add handoff context model"
```

### Task 2: Provider transcript readers

**Files:**
- Modify: `crates/vmux_agent/src/client/cli/codex.rs`
- Modify: `crates/vmux_agent/src/client/cli/claude.rs`
- Modify: `crates/vmux_agent/src/client/cli/vibe.rs`

- [ ] **Step 1: Write failing Codex transcript tests**

Fixture lines must include `session_meta`, `event_msg/user_message`, `event_msg/agent_message`, reasoning, and tool records. Assert only user and assistant messages survive and malformed lines are skipped.

```rust
assert_eq!(load_codex_transcript(&root, "cx-1").unwrap(), vec![
    Message::User { text: "fix auth".into() },
    Message::Assistant { blocks: vec![AssistantBlock::Text("working".into())] },
]);
```

- [ ] **Step 2: Run Codex test and verify failure**

Run: `cargo test -p vmux_agent codex_transcript --lib`

Expected: FAIL because the loader does not exist.

- [ ] **Step 3: Implement Codex reader**

Find the JSONL whose first `session_meta.payload.id` matches the requested sid. Parse `event_msg` records with `payload.type == user_message|agent_message` and a string `payload.message`. Ignore all other records.

- [ ] **Step 4: Run Codex test and verify pass**

Run: `cargo test -p vmux_agent codex_transcript --lib`

Expected: PASS.

- [ ] **Step 5: Write failing Claude transcript tests**

Cover string user content, text content blocks, assistant text blocks, `isMeta`, thinking, tool-use, and tool-result records.

- [ ] **Step 6: Implement Claude reader and run test**

Read the matching `<sid>.jsonl`; accept non-meta `type=user|assistant` records, extract string content or array items with `type == text`, and ignore empty messages.

Run: `cargo test -p vmux_agent claude_transcript --lib`

Expected: PASS.

- [ ] **Step 7: Write failing Vibe transcript tests**

Cover `messages.jsonl` with top-level `role`, string `content`, `injected`, `reasoning_content`, malformed lines, and unsupported roles.

- [ ] **Step 8: Implement Vibe reader and run test**

Locate the `session_*_<sid>/messages.jsonl` directory, accept non-injected user/assistant string content, and ignore reasoning/tool metadata.

Run: `cargo test -p vmux_agent vibe_transcript --lib`

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_agent/src/client/cli/codex.rs crates/vmux_agent/src/client/cli/claude.rs crates/vmux_agent/src/client/cli/vibe.rs
git commit -m "feat(agent): read provider transcripts for handoff"
```

### Task 3: Private ACP prompt context

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs`
- Modify: `crates/vmux_service/src/server.rs`
- Modify: `crates/vmux_service/src/acp/driver.rs`
- Modify: `crates/vmux_service/src/acp.rs`
- Modify: `crates/vmux_agent/src/client/acp.rs`
- Modify: `crates/vmux_agent/src/client/page/plugin.rs`

- [ ] **Step 1: Write failing protocol and driver tests**

Change `ClientMessage::AgentInput` to carry `context: Option<String>`. Add a driver helper test proving visible text enters the projector while the ACP wire text contains context exactly once.

```rust
let input = AgentInputPayload { text: "continue".into(), context: Some("history".into()) };
assert_eq!(input.display_text(), "continue");
assert!(input.wire_text().contains("history"));
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p vmux_service acp --lib`

Expected: FAIL at the new context assertions and changed protocol shape.

- [ ] **Step 3: Implement protocol transport**

Use:

```rust
AgentInput {
    sid: String,
    text: String,
    context: Option<String>,
}

pub enum AcpInput {
    User { text: String, context: Option<String> },
    // existing variants
}
```

Page-agent senders always set `context: None`. ACP senders use pending handoff context. The ACP driver calls `projector.push_user(text.clone())` and sends `wire_prompt(context, text)` to `PromptRequest`.

- [ ] **Step 4: Implement retry state transitions**

`send_acp_input` marks pending context `sent = true`. ACP status `Idle` removes consumed pending context; `Errored` changes `sent` back to false so the next submitted prompt retries with context.

- [ ] **Step 5: Run tests and verify pass**

Run: `cargo test -p vmux_service acp --lib && cargo test -p vmux_agent client::acp --lib`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_service/src/server.rs crates/vmux_service/src/acp/driver.rs crates/vmux_service/src/acp.rs crates/vmux_agent/src/client/acp.rs crates/vmux_agent/src/client/page/plugin.rs
git commit -m "feat(agent): send private ACP handoff context"
```

### Task 4: Imported-history persistence and rendering

**Files:**
- Modify: `crates/vmux_agent/src/handoff.rs`
- Modify: `crates/vmux_agent/src/plugin.rs`
- Modify: `crates/vmux_agent/src/client/acp.rs`
- Modify: `crates/vmux_agent/src/client/page/plugin.rs`
- Modify: `crates/vmux_agent/src/chat_page/event.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`
- Modify: `crates/vmux_agent/src/chat_page/page.rs`

- [ ] **Step 1: Write failing sidecar round-trip tests**

Use an injected temporary root and hex-encoded path components. Assert save/load round-trip, malformed JSON returns `None`, and missing records are ignored.

- [ ] **Step 2: Write failing snapshot/UI tests**

Assert `snapshot_of` prepends imported messages and emits `handoff_source`/`handoff_truncated`. Add a page source assertion for the `Continued from` marker.

- [ ] **Step 3: Implement sidecar storage**

Store JSON at:

```text
<profile_dir>/handoffs/<hex-agent-id>/<hex-session-id>.json
```

Persist `ImportedConversation` after `AcpSessionCreated` once `first_prompt` exists. Load it in `attach_acp_agent_to_stack` when opening a known target ACP session.

- [ ] **Step 4: Implement imported-history overlay**

Extend `ChatSnapshot`:

```rust
pub handoff_source: String,
pub handoff_truncated: bool,
```

`snapshot_of` serializes imported messages followed by live target messages. The page renders a muted `Continued from {source}` divider and an omission note when truncated.

- [ ] **Step 5: Sanitize later ACP replay**

When an imported conversation is attached, replace the first replayed user message beginning with `HANDOFF_PROMPT_PREFIX` with the persisted `first_prompt`. Leave later replayed target turns unchanged.

- [ ] **Step 6: Run tests**

Run: `cargo test -p vmux_agent handoff --lib && cargo test -p vmux_agent chat_page --lib`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_agent/src/handoff.rs crates/vmux_agent/src/plugin.rs crates/vmux_agent/src/client/acp.rs crates/vmux_agent/src/client/page/plugin.rs crates/vmux_agent/src/chat_page/event.rs crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/page.rs
git commit -m "feat(agent): persist and render imported handoff history"
```

### Task 5: Cross-agent resume selection

**Files:**
- Modify: `crates/vmux_core/src/agent.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`
- Modify: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Write failing resume-list tests**

Replace the current-kind filtering assertion with a test that all kinds remain sorted and that source labels use the active profile only for matching-kind rows, otherwise `AgentKind::display_name()`.

- [ ] **Step 2: Write failing selection tests**

Test same-kind selection produces the existing native resume target. Test foreign selection asynchronously loads the transcript and produces a fresh target ACP URL plus handoff payload and source cwd.

- [ ] **Step 3: Extend swap payload**

Add a core-only serialized handoff payload to `SwapStackSession`:

```rust
pub struct StackSessionHandoff {
    pub source_agent: String,
    pub source_kind: AgentKind,
    pub source_sid: String,
    pub messages_json: String,
    pub context: String,
    pub truncated: bool,
}

pub struct SwapStackSession {
    pub stack: Entity,
    pub target_url: String,
    pub cwd: PathBuf,
    pub handoff: Option<StackSessionHandoff>,
}
```

Set `handoff: None` at existing native-resume and runtime-switch call sites.

- [ ] **Step 4: Return all sessions and correct row labels**

Remove `sessions_for_kind`. Map every `list_all_sessions()` entry. Matching-kind rows use the active ACP profile name; foreign rows use their source kind display name.

- [ ] **Step 5: Add asynchronous handoff preparation**

For a foreign source kind, keep the current ACP agent id as target, load the source transcript on `IoTaskPool`, build bounded context, and emit `SwapStackSession` targeting `vmux://agent/<current-id>` with `handoff: Some(...)`. On load failure, leave the pane intact and set an inline error state.

- [ ] **Step 6: Attach handoff components during swap**

`handle_swap_stack_session` removes stale handoff components, attaches the fresh target ACP agent, deserializes imported messages, and inserts `ImportedConversation` plus `PendingHandoff`.

- [ ] **Step 7: Run tests**

Run: `cargo test -p vmux_core agent --lib && cargo test -p vmux_agent chat_page --lib && cargo test -p vmux_agent plugin --lib`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_core/src/agent.rs crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): hand off foreign resume sessions"
```

### Task 6: Final integration verification

**Files:**
- Modify: affected test modules only if verification finds gaps
- Delete: `docs/plans/2026-07-13-cross-agent-handoff.md`

- [ ] **Step 1: Run focused package tests**

Run:

```bash
cargo test -p vmux_service acp --lib
cargo test -p vmux_agent --lib
cargo test -p vmux_core --lib
```

Expected: PASS.

- [ ] **Step 2: Run formatting and focused checks**

Run:

```bash
cargo fmt --all -- --check
cargo check -p vmux_service -p vmux_agent -p vmux_core
```

Expected: PASS.

- [ ] **Step 3: Run vmux_agent WASM compile check**

Run: `cargo check -p vmux_agent --target wasm32-unknown-unknown`

Expected: PASS.

- [ ] **Step 4: Delete completed plan**

Use `apply_patch` to remove `docs/plans/2026-07-13-cross-agent-handoff.md`.

- [ ] **Step 5: Commit final verification cleanup**

```bash
git add -A
git commit -m "docs(agent): remove completed handoff plan"
```
