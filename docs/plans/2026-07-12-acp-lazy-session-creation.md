# ACP Lazy Session Creation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Subagents are out of scope.

**Goal:** Delay ACP `session/new` until the first user prompt, preserve eager resume loading, and safely identify existing empty vmux ACP sessions for one-time cleanup.

**Architecture:** Add two small async lifecycle helpers in the ACP driver: one attempts an explicitly requested resume without creating a fallback, and one creates a session only when a user prompt needs it. Keep `AcpSessionCreated` as the persistence boundary. Perform cleanup outside shipped code by correlating vmux-persisted ACP URLs with provider records and requiring zero genuine user prompts.

**Tech Stack:** Rust, Tokio, Agent Client Protocol 1.0, Bevy service messages, Python 3 for the read-only cleanup audit.

---

### Task 1: Specify lazy session lifecycle

**Files:**
- Modify: `crates/vmux_service/src/acp/driver.rs`
- Test: `crates/vmux_service/src/acp/driver.rs`

- [ ] **Step 1: Write failing lifecycle tests**

Add these tests to `driver::tests`:

```rust
#[tokio::test]
async fn requested_resume_loads_only_when_supported() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let loaded = load_requested_session(Some("resume-1".into()), true, |sid| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            assert_eq!(sid.to_string(), "resume-1");
            Ok::<(), ()>(())
        }
    })
    .await;
    assert_eq!(loaded.unwrap().to_string(), "resume-1");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let skipped = load_requested_session(Some("resume-2".into()), false, |_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok::<(), ()>(()) }
    })
    .await;
    assert!(skipped.is_none());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_requested_resume_stays_unassigned() {
    let loaded = load_requested_session(Some("stale".into()), true, |_| async {
        Err::<(), &'static str>("missing")
    })
    .await;
    assert!(loaded.is_none());
}

#[tokio::test]
async fn ensure_session_creates_once_then_reuses_id() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let mut session_id = None;
    let (created_id, created) = ensure_session(&mut session_id, || {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok::<SessionId, ()>(SessionId::new("created")) }
    })
    .await
    .unwrap();
    assert!(created);
    assert_eq!(created_id.to_string(), "created");

    let (reused_id, created) = ensure_session(&mut session_id, || {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok::<SessionId, ()>(SessionId::new("unexpected")) }
    })
    .await
    .unwrap();
    assert!(!created);
    assert_eq!(reused_id.to_string(), "created");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_session_creation_remains_retryable() {
    let mut session_id = None;
    let result = ensure_session(&mut session_id, || async {
        Err::<SessionId, &'static str>("create failed")
    })
    .await;
    assert_eq!(result.unwrap_err(), "create failed");
    assert!(session_id.is_none());
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo test -p vmux_service acp::driver::tests::requested_resume_loads_only_when_supported -- --exact
```

Expected: compilation fails because `load_requested_session` and `ensure_session` do not exist.

- [ ] **Step 3: Add minimal lifecycle helpers**

Add `use std::future::Future;` and place these helpers above `drain_stderr`:

```rust
async fn load_requested_session<F, Fut, E>(
    resume: Option<String>,
    load_supported: bool,
    load: F,
) -> Option<SessionId>
where
    F: FnOnce(SessionId) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    let sid = resume.filter(|_| load_supported).map(SessionId::new)?;
    load(sid.clone()).await.ok()?;
    Some(sid)
}

async fn ensure_session<F, Fut, E>(
    session_id: &mut Option<SessionId>,
    create: F,
) -> Result<(SessionId, bool), E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<SessionId, E>>,
{
    if let Some(sid) = session_id.clone() {
        return Ok((sid, false));
    }
    let sid = create().await?;
    *session_id = Some(sid.clone());
    Ok((sid, true))
}
```

- [ ] **Step 4: Run lifecycle tests**

Run:

```bash
cargo test -p vmux_service acp::driver::tests
```

Expected: all ACP driver unit tests pass.

- [ ] **Step 5: Commit lifecycle helpers**

```bash
git add crates/vmux_service/src/acp/driver.rs
git commit -m "test(agent): specify lazy ACP session lifecycle"
```

### Task 2: Defer ACP creation until the first prompt

**Files:**
- Modify: `crates/vmux_service/src/acp/driver.rs:248-340`
- Test: `crates/vmux_service/src/acp/driver.rs`

- [ ] **Step 1: Replace eager startup creation with optional load**

After initialization, replace the current `SessionId` match and unconditional `AcpSessionCreated` emission with:

```rust
let mut session_id = load_requested_session(
    resume,
    init_resp.agent_capabilities.load_session,
    |sid| {
        let mut load = LoadSessionRequest::new(sid, main_shared.cwd.clone());
        load.mcp_servers = mcp_servers.clone();
        async { cx.send_request(load).block_task().await.map(|_| ()) }
    },
)
.await;
if let Some(sid) = &session_id {
    main_shared.emit(ServiceMessage::AcpSessionCreated {
        sid: main_shared.sid.clone(),
        acp_session_id: sid.to_string(),
    });
}
main_shared.emit_status(AgentRunStatus::Idle);
```

- [ ] **Step 2: Create a session inside the user-input branch**

After projecting the user message and emitting `Streaming`, ensure a session before spawning the prompt:

```rust
let ensured = ensure_session(&mut session_id, || {
    let mut new_session = NewSessionRequest::new(main_shared.cwd.clone());
    new_session.mcp_servers = mcp_servers.clone();
    async {
        cx.send_request(new_session)
            .block_task()
            .await
            .map(|response| response.session_id)
    }
})
.await;
let (active_session_id, created) = match ensured {
    Ok(value) => value,
    Err(err) => {
        main_shared.emit_status(AgentRunStatus::Errored(format!(
            "acp session/new failed: {err}"
        )));
        continue;
    }
};
if created {
    main_shared.emit(ServiceMessage::AcpSessionCreated {
        sid: main_shared.sid.clone(),
        acp_session_id: active_session_id.to_string(),
    });
}
```

Pass `active_session_id` into `PromptRequest` instead of cloning a startup-created ID.

- [ ] **Step 3: Make cancel and close safe before assignment**

Replace unconditional cancellation notifications with:

```rust
if let Some(sid) = &session_id {
    let _ = cx.send_notification(CancelNotification::new(sid.clone()));
}
```

Keep permission denial and loop termination behavior unchanged.

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test -p vmux_service acp::driver::tests
```

Expected: all tests pass.

- [ ] **Step 5: Commit deferred creation**

```bash
git add crates/vmux_service/src/acp/driver.rs
git commit -m "fix(agent): create ACP sessions on first prompt"
```

### Task 3: Verify the implementation

**Files:**
- Verify: `crates/vmux_service/src/acp/driver.rs`
- Verify: `crates/vmux_agent/src/client/acp.rs`

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt --all -- --check
```

Expected: exit 0.

- [ ] **Step 2: Run package tests**

Run:

```bash
cargo test -p vmux_service
```

Expected: all `vmux_service` tests pass.

- [ ] **Step 3: Run package clippy**

Run:

```bash
cargo clippy -p vmux_service --all-targets -- -D warnings
```

Expected: exit 0 with no warnings.

- [ ] **Step 4: Verify the diff**

Run:

```bash
git diff HEAD~2 --check
git status --short
```

Expected: no whitespace errors; only the committed design, plan, and intended driver changes are present.

### Task 4: Audit existing empty vmux ACP sessions

**Files:**
- Read: `~/Library/Application Support/Vmux/*/*.ron`
- Read: `~/.claude/projects/**/*.jsonl`
- Read: `~/.codex/sessions/**/*.jsonl`
- Read: `~/.vibe/logs/session/*/{meta.json,messages.jsonl}`
- Create temporarily: `/tmp/vmux-empty-acp-dry-run.py`

- [ ] **Step 1: Build a read-only candidate set**

The scanner must:

1. Extract non-CLI `vmux://agent/<provider>/<session-id>` URLs from every vmux RON store.
2. Match Claude IDs against JSONL filenames or top-level `sessionId` values.
3. Match Codex IDs against rollout filenames or `session_meta.payload.id`.
4. Match Vibe IDs against `meta.json.session_id`.
5. Classify genuine prompts conservatively:
   - Claude: non-meta `type == "user"`, `promptSource == "sdk"`, and textual content that is not a command, hook, tool result, or injected system block.
   - Codex: `response_item.payload.type == "message"`, `role == "user"`, and at least one non-empty `input_text` item after excluding injected context records.
   - Vibe: `role == "user"`, `injected == false`, and non-empty textual `content`.
6. Print only matched vmux ACP sessions with zero genuine prompts. Print unmatched and ambiguous IDs separately; never classify them as deletable.

- [ ] **Step 2: Run the dry-run**

Run:

```bash
python3 /tmp/vmux-empty-acp-dry-run.py
```

Expected: provider, session ID, exact owned paths, match evidence, and prompt count for every candidate. No files are changed.

- [ ] **Step 3: Request deletion approval**

Present the exact candidate list and total count. Do not delete anything until the user explicitly approves that list.

- [ ] **Step 4: Delete only approved paths**

Use `rm` only for exact approved files or directories. Do not use globs. Re-run the dry-run afterward; expected candidate count is zero.

### Task 5: Show resume loading state while fetching

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/composer.rs`
- Modify: `crates/vmux_agent/src/chat_page/page.rs`
- Test: `crates/vmux_agent/src/chat_page/composer.rs`

- [ ] **Step 1: Write the failing menu-state test**

Add:

```rust
#[test]
fn resume_menu_distinguishes_loading_from_loaded_empty() {
    assert_eq!(
        resume_menu_state(false, false, 0, 0),
        ResumeMenuState::Loading
    );
    assert_eq!(
        resume_menu_state(true, true, 0, 0),
        ResumeMenuState::Loading
    );
    assert_eq!(
        resume_menu_state(true, false, 0, 0),
        ResumeMenuState::Empty
    );
    assert_eq!(
        resume_menu_state(true, false, 2, 0),
        ResumeMenuState::NoMatch
    );
    assert_eq!(
        resume_menu_state(true, false, 2, 1),
        ResumeMenuState::Results
    );
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p vmux_agent chat_page::composer::tests::resume_menu_distinguishes_loading_from_loaded_empty -- --exact
```

Expected: compilation fails because `resume_menu_state` and `ResumeMenuState` do not exist.

- [ ] **Step 3: Add the menu-state model**

Add to `composer.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResumeMenuState {
    Loading,
    Empty,
    NoMatch,
    Results,
}

pub(crate) fn resume_menu_state(
    requested: bool,
    loading: bool,
    session_count: usize,
    filtered_count: usize,
) -> ResumeMenuState {
    if !requested || loading {
        ResumeMenuState::Loading
    } else if session_count == 0 {
        ResumeMenuState::Empty
    } else if filtered_count == 0 {
        ResumeMenuState::NoMatch
    } else {
        ResumeMenuState::Results
    }
}
```

- [ ] **Step 4: Wire request state into the page**

Import `ResumeMenuState` and `resume_menu_state`. Add `resume_loading`, set it before emitting `ResumeListRequest`, clear it when `ResumableSessions` arrives, and clear it when emitting fails. Derive menu state with:

```rust
let resume_state = resume_query.map(|_| {
    resume_menu_state(
        resume_requested(),
        resume_loading(),
        sessions.read().len(),
        filtered_sessions.len(),
    )
});
```

Render `Loading sessions…` for `ResumeMenuState::Loading`, then the existing empty, no-match, and result branches.

- [ ] **Step 5: Run targeted tests**

```bash
cargo test -p vmux_agent chat_page::composer::tests
```

Expected: all composer tests pass.

- [ ] **Step 6: Commit**

```bash
git add docs/specs/2026-07-12-acp-lazy-session-creation-design.md docs/plans/2026-07-12-acp-lazy-session-creation.md crates/vmux_agent/src/chat_page/composer.rs crates/vmux_agent/src/chat_page/page.rs
git commit -m "fix(agent): show resume loading state"
```

### Task 6: Finish branch state

**Files:**
- Delete: `docs/plans/2026-07-12-acp-lazy-session-creation.md`

- [ ] **Step 1: Remove completed plan**

Use `apply_patch` to delete this plan after implementation and verification succeed.

- [ ] **Step 2: Commit plan removal**

```bash
git add docs/plans/2026-07-12-acp-lazy-session-creation.md
git commit -m "docs(agent): remove completed ACP session plan"
```

- [ ] **Step 3: Push the branch**

```bash
git push
```

Expected: `feat/acp-cli-resume` updates PR #241.
