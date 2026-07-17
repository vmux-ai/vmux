# Shell Integration — Plan 1: OSC 133 parse foundation

**Goal:** Make `vmux_service` detect OSC 133 command-lifecycle markers (`C` = command start, `D;<exit>` = command end) in PTY output and broadcast them as a new `ServiceMessage::CommandLifecycle`, without changing any existing behavior.

**Architecture:** The service parses PTY bytes with `alacritty_terminal`'s vte, whose high-level `Handler` ignores OSC 133. Rather than hand-roll a byte scanner, run a second, tiny `vte::Parser` per `Process` whose `Perform` only inspects `osc_dispatch` for `133`. vte handles ESC/OSC framing, `ST` vs `BEL`, and chunk-split reassembly. The drain loop feeds this scanner the same bytes it feeds alacritty and broadcasts a `CommandLifecycle` for each event. Nothing consumes the new message yet (additive, safe to ship).

**Tech Stack:** Rust, `vte` 0.15 (already a transitive dep via `alacritty_terminal`), `tokio::sync::broadcast`, `rkyv` (wire serialization for `ServiceMessage`).

**Spec:** `docs/specs/2026-06-21-shell-integration-command-lifecycle-design.md`

---

## Task 1: Add the `vte` dependency

**Files:**
- Modify: `crates/vmux_service/Cargo.toml`

- [ ] **Step 1: Add the dependency**

In `crates/vmux_service/Cargo.toml`, under `[dependencies]` (next to `alacritty_terminal`), add:

```toml
vte = "0.15"
```

- [ ] **Step 2: Verify it resolves to the same version alacritty uses**

Run: `cargo tree -p vmux_service -i vte`
Expected: a single `vte v0.15.x` (no duplicate versions). If a second version appears, pin `vte` to the exact version printed under `alacritty_terminal`.

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p vmux_service`
Expected: builds with no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_service/Cargo.toml Cargo.lock
git commit -m "build(service): add vte dependency for OSC 133 parsing"
```

---

## Task 2: Add the `CommandLifecycle` message

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (the `ServiceMessage` enum begins at line 433; `ProcessTitle` at 460 is the template)

- [ ] **Step 1: Add the kind enum**

Immediately above `pub enum ServiceMessage` (line 433), add a new enum with the same derives:

```rust
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum CommandLifecycleKind {
    /// OSC 133;C — a command began executing.
    Started,
    /// OSC 133;D;<exit> — a command finished; `exit_code` is None if the shell
    /// omitted it.
    Ended { exit_code: Option<i32> },
}
```

- [ ] **Step 2: Add the message variant**

Inside `pub enum ServiceMessage`, after the `ProcessTitle { .. }` variant (ends line 463), add:

```rust
    CommandLifecycle {
        process_id: ProcessId,
        kind: CommandLifecycleKind,
    },
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p vmux_service`
Expected: builds. (Any `match` on `ServiceMessage` that is non-exhaustive will fail to compile — if so, add a `ServiceMessage::CommandLifecycle { .. } => {}` arm wherever the compiler points, since no existing code consumes it yet.)

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(service): add CommandLifecycle service message"
```

---

## Task 3: OSC 133 scanner (pure, unit-tested)

**Files:**
- Create: `crates/vmux_service/src/osc133.rs`
- Modify: `crates/vmux_service/src/lib.rs` (add `mod osc133;` — check the file for how sibling modules like `process` and `protocol` are declared and match that style)

- [ ] **Step 1: Declare the module**

In `crates/vmux_service/src/lib.rs`, add alongside the other `mod` declarations:

```rust
mod osc133;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/vmux_service/src/osc133.rs` with the test module only (the types it references don't exist yet, so it won't compile — that is the "failing" state for this pure unit):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn esc(seq: &str) -> Vec<u8> {
        seq.replace("\\e", "\u{1b}")
            .replace("\\a", "\u{07}")
            .into_bytes()
    }

    #[test]
    fn detects_command_start() {
        let mut s = Osc133Scanner::new();
        assert_eq!(s.feed(&esc("\\e]133;C\\a")), vec![Osc133Event::CommandStart]);
    }

    #[test]
    fn detects_command_end_with_exit_code() {
        let mut s = Osc133Scanner::new();
        assert_eq!(
            s.feed(&esc("\\e]133;D;0\\a")),
            vec![Osc133Event::CommandEnd(Some(0))]
        );
        assert_eq!(
            s.feed(&esc("\\e]133;D;130\\a")),
            vec![Osc133Event::CommandEnd(Some(130))]
        );
    }

    #[test]
    fn command_end_without_code_is_none() {
        let mut s = Osc133Scanner::new();
        assert_eq!(
            s.feed(&esc("\\e]133;D\\a")),
            vec![Osc133Event::CommandEnd(None)]
        );
    }

    #[test]
    fn accepts_st_terminator() {
        let mut s = Osc133Scanner::new();
        assert_eq!(
            s.feed(&esc("\\e]133;D;0\\e\\")),
            vec![Osc133Event::CommandEnd(Some(0))]
        );
    }

    #[test]
    fn reassembles_sequence_split_across_feeds() {
        let mut s = Osc133Scanner::new();
        assert_eq!(s.feed(&esc("\\e]133;D")), vec![]);
        assert_eq!(s.feed(&esc(";0\\a")), vec![Osc133Event::CommandEnd(Some(0))]);
    }

    #[test]
    fn ignores_other_osc_and_plain_text() {
        let mut s = Osc133Scanner::new();
        // OSC 0 (title) + normal text must yield nothing.
        assert_eq!(s.feed(&esc("\\e]0;my title\\ahello world\n")), vec![]);
    }

    #[test]
    fn emits_start_then_end_in_order() {
        let mut s = Osc133Scanner::new();
        assert_eq!(
            s.feed(&esc("\\e]133;C\\als -la\n\\e]133;D;0\\a")),
            vec![Osc133Event::CommandStart, Osc133Event::CommandEnd(Some(0))]
        );
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p vmux_service osc133 2>&1 | head -20`
Expected: compile error — `Osc133Scanner`/`Osc133Event` not found.

- [ ] **Step 4: Write the implementation**

At the **top** of `crates/vmux_service/src/osc133.rs` (above the `#[cfg(test)] mod tests`), add:

```rust
use vte::{Parser, Perform};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc133Event {
    /// OSC 133;C
    CommandStart,
    /// OSC 133;D;<exit>
    CommandEnd(Option<i32>),
}

/// Watches a PTY byte stream for OSC 133 semantic-prompt markers, reusing vte's
/// state machine so split chunks and `ST`/`BEL` terminators are handled for us.
pub struct Osc133Scanner {
    parser: Parser,
}

impl Osc133Scanner {
    pub fn new() -> Self {
        Self { parser: Parser::new() }
    }

    /// Feed a chunk of PTY output; returns any OSC 133 events completed by it.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Osc133Event> {
        let mut collector = Collector::default();
        self.parser.advance(&mut collector, bytes);
        collector.events
    }
}

#[derive(Default)]
struct Collector {
    events: Vec<Osc133Event>,
}

impl Perform for Collector {
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.first().copied() != Some(b"133".as_slice()) {
            return;
        }
        let kind = params.get(1).copied();
        if kind == Some(b"C".as_slice()) {
            self.events.push(Osc133Event::CommandStart);
        } else if kind == Some(b"D".as_slice()) {
            let exit = params
                .get(2)
                .and_then(|p| std::str::from_utf8(p).ok())
                .and_then(|s| s.trim().parse::<i32>().ok());
            self.events.push(Osc133Event::CommandEnd(exit));
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p vmux_service osc133`
Expected: all 7 tests pass, output pristine.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/src/osc133.rs crates/vmux_service/src/lib.rs
git commit -m "feat(service): OSC 133 scanner over a parallel vte parser"
```

---

## Task 4: Wire the scanner into `Process` and broadcast lifecycle events

**Files:**
- Modify: `crates/vmux_service/src/process.rs` (struct at line 69, `new_with_wake` init around line 304-327, drain `poll()` at 1173-1198)

- [ ] **Step 1: Write the failing integration test**

Add this test to the existing `#[cfg(test)] mod tests` in `crates/vmux_service/src/process.rs` (use the existing `drain_process_output` helper at line 1717 and the `subscribe()` pattern from `proxy_broadcasts_process_title_on_term_title_event`):

```rust
    #[test]
    fn poll_broadcasts_command_lifecycle_from_osc133() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![
                "-c".to_string(),
                // Emit an OSC 133;D;0 sequence, then exit.
                "printf '\\033]133;D;0\\007'".to_string(),
            ],
            String::new(),
            vec![],
            80,
            24,
            wake_tx,
        )
        .expect("spawn");
        let mut rx = process.subscribe();

        drain_process_output(&mut process, std::time::Duration::from_secs(2));

        let mut saw_end = false;
        while let Ok(msg) = rx.try_recv() {
            if let ServiceMessage::CommandLifecycle {
                kind: crate::protocol::CommandLifecycleKind::Ended { exit_code },
                ..
            } = msg
            {
                assert_eq!(exit_code, Some(0));
                saw_end = true;
            }
        }
        assert!(saw_end, "expected a CommandLifecycle Ended broadcast from OSC 133;D;0");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p vmux_service poll_broadcasts_command_lifecycle_from_osc133 -- --nocapture`
Expected: FAIL — no `CommandLifecycle` is ever broadcast (and `Process` has no scanner field yet, so this may be a compile error first; either failure is acceptable for RED).

- [ ] **Step 3: Add the scanner field to `Process`**

In the `Process` struct (line 69), after the `processor: Processor,` field (line 78), add:

```rust
    osc133: crate::osc133::Osc133Scanner,
```

- [ ] **Step 4: Initialize it in `new_with_wake`**

In the struct literal returned by `new_with_wake` (the block initializing `processor: Processor::new(),` around line 322), add:

```rust
            osc133: crate::osc133::Osc133Scanner::new(),
```

- [ ] **Step 5: Feed the scanner + broadcast in `poll()`**

In `poll()` (line 1173), replace the drain body that currently reads:

```rust
            self.processor.advance(&mut self.term, &data);
            got_data = true;
```

with:

```rust
            self.processor.advance(&mut self.term, &data);
            for event in self.osc133.feed(&data) {
                let kind = match event {
                    crate::osc133::Osc133Event::CommandStart => {
                        crate::protocol::CommandLifecycleKind::Started
                    }
                    crate::osc133::Osc133Event::CommandEnd(exit_code) => {
                        crate::protocol::CommandLifecycleKind::Ended { exit_code }
                    }
                };
                let _ = self.patch_tx.send(ServiceMessage::CommandLifecycle {
                    process_id: self.id,
                    kind,
                });
            }
            got_data = true;
```

- [ ] **Step 6: Run the integration test to verify it passes**

Run: `cargo test -p vmux_service poll_broadcasts_command_lifecycle_from_osc133`
Expected: PASS.

- [ ] **Step 7: Run the full crate test suite to check for regressions**

Run: `cargo test -p vmux_service`
Expected: all pass, output pristine.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_service/src/process.rs
git commit -m "feat(service): broadcast CommandLifecycle from PTY OSC 133 markers"
```

---

## Out of scope (later plans)

- **Plan 2 — Emit + inject:** per-shell OSC 133 snippets (bash/zsh/fish/nu) shipped in the app bundle, injected at PTY spawn via each shell's native init (`--rcfile`, `ZDOTDIR`, `--init-command`, nu config) with no dotfile edits.
- **Plan 3 — Consume + cleanup:** rewire MCP `run` (`vmux_mcp/src/protocol.rs`) to wait on `CommandLifecycle::Ended` instead of scraping `__VMUX_DONE_`; delete the `run_command_line` wrapper (`vmux_agent/src/plugin.rs:582`) and the sentinel parser; add the unknown-shell fallback.

These depend on Plan 1's `CommandLifecycle` API and are written once Plan 1 lands.
