# Resume Session Prefetch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Subagents are out of scope.

**Goal:** Start loading resumable sessions when `/resume` is the only filtered slash-command option, before the command is executed.

**Architecture:** Put the prefetch decision in the shared composer model so native tests can verify command filtering. Reuse the page's existing `resume_requested` and `resume_loading` state; prefetch changes neither the draft nor the visible command selector.

**Tech Stack:** Rust, Dioxus signals, Bevy native tests, wasm32 compile check.

---

### Task 1: Define the prefetch decision

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/composer.rs`
- Test: `crates/vmux_agent/src/chat_page/composer.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn resume_prefetch_starts_only_for_resume_as_the_sole_match() {
    let commands = vec![
        SlashCommandEntry {
            name: "resume".into(),
            ..Default::default()
        },
        SlashCommandEntry {
            name: "cli".into(),
            ..Default::default()
        },
    ];
    assert!(should_fetch_resume("/r", &commands));
    assert!(should_fetch_resume("/resume", &commands));
    assert!(should_fetch_resume("/resume ", &commands));
    assert!(!should_fetch_resume("/", &commands));
    assert!(!should_fetch_resume("/c", &commands));
    assert!(!should_fetch_resume("hello", &commands));
}
```

- [ ] **Step 2: Verify the test fails**

```bash
cargo test -p vmux_agent chat_page::composer::tests::resume_prefetch_starts_only_for_resume_as_the_sole_match -- --exact
```

Expected: compilation fails because `should_fetch_resume` does not exist.

- [ ] **Step 3: Implement the decision helper**

Import `SlashCommandEntry` beside `ResumableSessionEntry`, then add:

```rust
pub(crate) fn should_fetch_resume(draft: &str, commands: &[SlashCommandEntry]) -> bool {
    match selector_mode(draft) {
        SelectorMode::Resume(_) => true,
        SelectorMode::Commands(query) => {
            let query = query.to_lowercase();
            let mut matches = commands
                .iter()
                .filter(|command| command.name.starts_with(&query));
            matches.next().is_some_and(|command| command.name == "resume")
                && matches.next().is_none()
        }
        SelectorMode::None => false,
    }
}
```

- [ ] **Step 4: Verify composer tests**

```bash
cargo test -p vmux_agent chat_page::composer::tests
```

Expected: all composer tests pass.

### Task 2: Trigger prefetch from the page

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/page.rs:3-235`

- [ ] **Step 1: Wire the helper into the request effect**

Import `should_fetch_resume`. Replace the resume-selector-only condition with:

```rust
let should_fetch = should_fetch_resume(&draft(), &slash_cmds.read());
if should_fetch && !resume_requested() {
    resume_loading.set(true);
    if try_cef_bin_emit_rkyv(&ResumeListRequest).is_err() {
        resume_loading.set(false);
    }
    resume_requested.set(true);
} else if !should_fetch && resume_requested() {
    resume_requested.set(false);
    resume_loading.set(false);
}
```

This keeps the existing request alive when the draft transitions from the unique `/resume` command match to `SelectorMode::Resume`.

- [ ] **Step 2: Verify native and wasm builds**

```bash
cargo test -p vmux_agent chat_page::composer::tests
cargo check -p vmux_agent --target wasm32-unknown-unknown
cargo clippy -p vmux_agent --all-targets -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/chat_page/composer.rs crates/vmux_agent/src/chat_page/page.rs docs/plans/2026-07-13-resume-session-prefetch.md
git commit -m "fix(agent): prefetch unique resume command"
```

### Task 3: Finish branch state

**Files:**
- Delete: `docs/plans/2026-07-13-resume-session-prefetch.md`

- [ ] **Step 1: Remove the completed plan with `apply_patch`**

- [ ] **Step 2: Commit plan removal**

```bash
git add docs/plans/2026-07-13-resume-session-prefetch.md
git commit -m "docs(agent): remove completed resume prefetch plan"
```

- [ ] **Step 3: Push the existing PR branch**

```bash
git push
```

Expected: PR #241 updates to the new branch head.
