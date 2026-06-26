# Agent Loading Screen Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Repo constraint:** vmux CEF builds are large and long; do NOT subagent-drive this plan. Execute inline/directly with a warm target dir (see memory: "Subagent CEF build fragility", "vmux build workflow").

**Goal:** Replace the plain agent loading overlay (vibe/claude/codex) with a soft-glass chat-transcript skeleton showing the real product logo, agent name, per-agent accent, and "starting…", and switch the setup page to the same real logo — sharing one accent source.

**Architecture:** The terminal page (host `terminal`) can't see the agent kind in its URL, so the kind rides on `TermLoadingEvent` as a new `segment` field. A new pure module `vmux_ui::agent_accent` is the single source of per-agent Tailwind accent tokens, consumed by both the loading overlay and the setup page. The real logo reuses the existing `vmux_ui::favicon::Favicon` component (Google s2 favicon with globe fallback).

**Tech Stack:** Rust, Bevy (host/native), Dioxus + WASM (pages), Tailwind v4, rkyv (page↔host IPC).

---

## File Structure

- `crates/vmux_core/src/event.rs` — add `segment: String` to `TermLoadingEvent`; update rkyv round-trip test.
- `crates/vmux_terminal/src/plugin.rs` — set `segment` at the 3 emit sites (process-exit clear, arm, timeout/alt-screen clear).
- `crates/vmux_ui/src/agent_accent.rs` — **new** pure module: `AgentAccent` struct + `agent_accent(segment)` + tests.
- `crates/vmux_ui/src/lib.rs` — register `pub mod agent_accent;`.
- `crates/vmux_terminal/src/page.rs` — change `loading` signal to carry `(label, segment)`; rewrite the loading overlay as Direction A.
- `crates/vmux_agent/src/vibe/setup/page.rs` — swap gradient badge → glass-tile `Favicon`; drop local `Accent`/`accent()`, consume `vmux_ui::agent_accent`.

No new crates. `@source` globs already include `vmux_core`? (no — it's native), `vmux_ui/src`, `vmux_terminal/src`, `vmux_agent/src` — all present in `crates/vmux_server/assets/index.css`, so no CSS change needed.

---

## Task 1: Carry agent `segment` on the loading event

**Files:**
- Modify: `crates/vmux_core/src/event.rs:788-791` (struct), `crates/vmux_core/src/event.rs:1480-1490` (test)
- Modify: `crates/vmux_terminal/src/plugin.rs:1317`, `:2693`, `:2725` (3 emit sites)

- [ ] **Step 1: Update the rkyv round-trip test to require `segment` (RED — won't compile)**

In `crates/vmux_core/src/event.rs`, replace the test body at lines 1480-1490:

```rust
    #[test]
    fn term_loading_event_rkyv_roundtrip() {
        let original = TermLoadingEvent {
            loading: true,
            label: "Vibe".to_string(),
            segment: "vibe".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TermLoadingEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
```

- [ ] **Step 2: Run the test to verify it fails to compile**

Run: `cargo test -p vmux_core term_loading_event_rkyv_roundtrip`
Expected: FAIL — `struct TermLoadingEvent has no field named segment`.

- [ ] **Step 3: Add the `segment` field to the struct**

In `crates/vmux_core/src/event.rs`, replace the struct at lines 788-791:

```rust
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,
    pub segment: String,
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p vmux_core term_loading_event_rkyv_roundtrip`
Expected: PASS.

- [ ] **Step 5: Set `segment` at the process-exit clear site**

In `crates/vmux_terminal/src/plugin.rs`, replace the struct literal at lines 1317-1320:

```rust
                                &crate::event::TermLoadingEvent {
                                    loading: false,
                                    label: session.kind.display_name().to_string(),
                                    segment: session.kind.as_url_segment().to_string(),
                                },
```

- [ ] **Step 6: Set `segment` at the arm site**

In `crates/vmux_terminal/src/plugin.rs`, replace the struct literal at lines 2693-2696:

```rust
            &crate::event::TermLoadingEvent {
                loading: true,
                label: session.kind.display_name().to_string(),
                segment: session.kind.as_url_segment().to_string(),
            },
```

- [ ] **Step 7: Set `segment` at the timeout/alt-screen clear site**

In `crates/vmux_terminal/src/plugin.rs`, replace the struct literal at lines 2725-2728:

```rust
                &crate::event::TermLoadingEvent {
                    loading: false,
                    label: session.kind.display_name().to_string(),
                    segment: session.kind.as_url_segment().to_string(),
                },
```

- [ ] **Step 8: Run the terminal plugin loading tests to verify nothing broke**

Run: `cargo test -p vmux_terminal agent_loading`
Expected: PASS — `agent_terminal_armed_loading_on_page_ready`, `agent_loading_cleared_when_alt_screen_active`, `agent_loading_cleared_after_timeout`, `agent_loading_retained_while_starting` all green.

- [ ] **Step 9: Format and commit**

```bash
cargo fmt -p vmux_core -p vmux_terminal
git add crates/vmux_core/src/event.rs crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): carry agent segment on TermLoadingEvent"
```

---

## Task 2: Shared per-agent accent module in `vmux_ui`

**Files:**
- Create: `crates/vmux_ui/src/agent_accent.rs`
- Modify: `crates/vmux_ui/src/lib.rs:8-12` (add module)

- [ ] **Step 1: Write the new module with the accent table and tests (RED — module not registered)**

Create `crates/vmux_ui/src/agent_accent.rs`:

```rust
pub struct AgentAccent {
    pub glow_top: &'static str,
    pub glow_bottom: &'static str,
    pub grad: &'static str,
    pub accent_text: &'static str,
    pub accent_bg: &'static str,
    pub cta_shadow: &'static str,
}

pub fn agent_accent(segment: &str) -> AgentAccent {
    match segment {
        "claude" => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-rose-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-orange-400/10 blur-[120px]",
            grad: "from-orange-400 to-rose-500",
            accent_text: "text-rose-400",
            accent_bg: "bg-rose-400",
            cta_shadow: "shadow-lg shadow-rose-500/25 hover:shadow-rose-500/40",
        },
        "codex" => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-emerald-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-teal-400/10 blur-[120px]",
            grad: "from-emerald-500 to-teal-600",
            accent_text: "text-emerald-400",
            accent_bg: "bg-emerald-400",
            cta_shadow: "shadow-lg shadow-emerald-500/25 hover:shadow-emerald-500/40",
        },
        _ => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-orange-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-amber-400/10 blur-[120px]",
            grad: "from-orange-500 to-amber-600",
            accent_text: "text-orange-400",
            accent_bg: "bg-orange-400",
            cta_shadow: "shadow-lg shadow-orange-500/25 hover:shadow-orange-500/40",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_uses_rose_orange() {
        let a = agent_accent("claude");
        assert_eq!(a.grad, "from-orange-400 to-rose-500");
        assert_eq!(a.accent_text, "text-rose-400");
        assert_eq!(a.accent_bg, "bg-rose-400");
    }

    #[test]
    fn codex_uses_emerald_teal() {
        let a = agent_accent("codex");
        assert_eq!(a.grad, "from-emerald-500 to-teal-600");
        assert_eq!(a.accent_text, "text-emerald-400");
    }

    #[test]
    fn unknown_falls_back_to_vibe_amber() {
        let a = agent_accent("nope");
        assert_eq!(a.grad, "from-orange-500 to-amber-600");
        assert_eq!(a.grad, agent_accent("vibe").grad);
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/vmux_ui/src/lib.rs`, add after line 8 (`pub mod favicon;`):

```rust
pub mod agent_accent;
```

- [ ] **Step 3: Run the accent tests to verify they pass**

Run: `cargo test -p vmux_ui agent_accent`
Expected: PASS — `claude_uses_rose_orange`, `codex_uses_emerald_teal`, `unknown_falls_back_to_vibe_amber`.

- [ ] **Step 4: Format and commit**

```bash
cargo fmt -p vmux_ui
git add crates/vmux_ui/src/agent_accent.rs crates/vmux_ui/src/lib.rs
git commit -m "feat(ui): shared per-agent accent tokens"
```

---

## Task 3: Redesign the loading overlay (Direction A) in the terminal page

**Files:**
- Modify: `crates/vmux_terminal/src/page.rs:12` (imports), `:32` (signal type), `:114-117` (listener), `:294-315` (overlay)

No native unit test — this is WASM/Dioxus render code, type-checked via `cargo check --target wasm32` and visually verified by the user at the end.

- [ ] **Step 1: Add imports**

In `crates/vmux_terminal/src/page.rs`, replace line 12:

```rust
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
```

- [ ] **Step 2: Change the `loading` signal to carry `(label, segment)`**

In `crates/vmux_terminal/src/page.rs`, replace line 32:

```rust
    let mut loading = use_signal(|| None::<(String, String)>);
```

- [ ] **Step 3: Update the loading listener to store both fields**

In `crates/vmux_terminal/src/page.rs`, replace the listener body at lines 114-117:

```rust
    let _loading_listener =
        use_bin_event_listener::<TermLoadingEvent, _>(TERM_LOADING_EVENT, move |evt| {
            loading.set(if evt.loading {
                Some((evt.label, evt.segment))
            } else {
                None
            });
        });
```

- [ ] **Step 4: Rewrite the overlay block as Direction A**

In `crates/vmux_terminal/src/page.rs`, replace the entire overlay block at lines 294-315 (the `let label = loading.read().clone(); label.map(...)` block):

```rust
            {
                let state = loading.read().clone();
                state.map(|(label, segment)| {
                    let accent = agent_accent(&segment);
                    let favicon_url = format!("vmux://agent/{segment}/cli/");
                    rsx! {
                        div {
                            class: "pointer-events-none absolute inset-0 z-40 overflow-hidden bg-term-bg",
                            div { class: "{accent.glow_top}" }
                            div { class: "{accent.glow_bottom}" }
                            div {
                                class: "relative flex h-full w-full flex-col",
                                div {
                                    class: "flex items-center gap-3 px-5 py-4",
                                    div {
                                        class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-white/[0.06] ring-1 ring-inset ring-white/10",
                                        Favicon {
                                            favicon_url: "".to_string(),
                                            url: favicon_url.clone(),
                                            class: "h-5 w-5 shrink-0 rounded object-contain".to_string(),
                                            globe_class: "h-5 w-5 text-muted-foreground".to_string(),
                                        }
                                    }
                                    div {
                                        div { class: "text-sm font-semibold text-foreground", "{label}" }
                                        div {
                                            class: "flex items-center gap-1.5 text-xs text-muted-foreground",
                                            span { class: "h-1.5 w-1.5 rounded-full animate-pulse {accent.accent_bg}" }
                                            "starting…"
                                        }
                                    }
                                }
                                div {
                                    class: "flex flex-1 flex-col gap-4 px-5 py-3",
                                    div {
                                        class: "flex justify-end gap-2.5",
                                        div {
                                            class: "flex max-w-[60%] flex-col items-end gap-2",
                                            div { class: "h-2.5 w-40 rounded-md bg-white/10 animate-pulse" }
                                        }
                                        div { class: "h-6 w-6 shrink-0 rounded-lg bg-white/10" }
                                    }
                                    div {
                                        class: "flex gap-2.5",
                                        div { class: "h-6 w-6 shrink-0 rounded-lg bg-gradient-to-br {accent.grad}" }
                                        div {
                                            class: "flex flex-1 flex-col gap-2",
                                            div { class: "h-2.5 w-[92%] rounded-md bg-white/10 animate-pulse" }
                                            div { class: "h-2.5 w-[80%] rounded-md bg-white/10 animate-pulse [animation-delay:120ms]" }
                                            div { class: "h-2.5 w-[45%] rounded-md bg-white/10 animate-pulse [animation-delay:240ms]" }
                                        }
                                    }
                                }
                                div {
                                    class: "mx-4 mb-4 flex items-center gap-2 rounded-xl bg-white/[0.03] px-3 py-3 ring-1 ring-inset ring-white/10",
                                    span { class: "h-4 w-0.5 animate-pulse {accent.accent_bg}" }
                                    div { class: "h-2 w-32 rounded bg-white/10 animate-pulse" }
                                }
                            }
                        }
                    }
                })
            }
```

- [ ] **Step 5: Type-check the WASM page build**

Run: `cargo check -p vmux_server --target wasm32-unknown-unknown --features web`
Expected: PASS — no type errors in `vmux_terminal::page`.

- [ ] **Step 6: Format and commit**

```bash
cargo fmt -p vmux_terminal
git add crates/vmux_terminal/src/page.rs
git commit -m "feat(terminal): chat-skeleton agent loading screen"
```

---

## Task 4: Setup page — real favicon + shared accent

**Files:**
- Modify: `crates/vmux_agent/src/vibe/setup/page.rs` — remove `struct Accent` (lines 8-14) and `fn accent()` (lines 32-56); add `Favicon` import; rebuild header badge + accent usage in `Page()`.

- [ ] **Step 1: Replace the imports and delete the local `Accent` struct**

In `crates/vmux_agent/src/vibe/setup/page.rs`, replace lines 1-14 (top of file through the end of `struct Accent`):

```rust
#![allow(non_snake_case)]

use crate::vibe::setup::event::AgentInstallRunRequest;
use dioxus::prelude::*;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};
```

- [ ] **Step 2: Delete the local `accent()` function**

In `crates/vmux_agent/src/vibe/setup/page.rs`, delete the entire `fn accent(segment: &str) -> Accent { ... }` block (originally lines 32-56). Leave `fn current_agent_segment()` and `fn tagline()` intact.

- [ ] **Step 3: Build accent classes from the shared module in `Page()`**

In `crates/vmux_agent/src/vibe/setup/page.rs`, replace the lines in `Page()` from `let accent = accent(&segment);` through `let emit_segment = segment.clone();` with:

```rust
    let accent = agent_accent(&segment);
    let prompt_class = format!("select-none font-mono text-sm {}", accent.accent_text);
    let cta_class = format!(
        "group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br {} px-4 py-2.5 text-sm font-medium text-white {} transition-all hover:brightness-110 active:scale-[0.99]",
        accent.grad, accent.cta_shadow
    );
    let emit_segment = segment.clone();
```

- [ ] **Step 4: Swap the gradient badge for a glass-tile favicon**

In `crates/vmux_agent/src/vibe/setup/page.rs`, replace the badge `div` (originally lines 74-80, the `div { class: "{accent.badge}", Icon { ... download paths ... } }`) with:

```rust
                    div { class: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-white/[0.06] ring-1 ring-inset ring-white/10",
                        Favicon {
                            favicon_url: "".to_string(),
                            url: format!("vmux://agent/{segment}/cli/"),
                            class: "h-7 w-7 shrink-0 rounded-lg object-contain".to_string(),
                            globe_class: "h-7 w-7 text-muted-foreground".to_string(),
                        }
                    }
```

- [ ] **Step 5: Point the prompt and CTA at the composed classes**

In `crates/vmux_agent/src/vibe/setup/page.rs`, change the two consumers:

- the `$` prompt span — replace `class: "{accent.prompt}"` with:

```rust
                    span { class: "{prompt_class}", "$" }
```

- the CTA button — replace `class: "{accent.cta}"` with:

```rust
                button {
                    class: "{cta_class}",
```

(The `accent.glow_top` / `accent.glow_bottom` usages at the top of `main` are unchanged — the field names match the shared struct.)

- [ ] **Step 6: Type-check the WASM page build**

Run: `cargo check -p vmux_server --target wasm32-unknown-unknown --features web`
Expected: PASS — no type errors in `vmux_agent::vibe::setup::page` (no remaining references to `Accent`, `accent.badge`, `accent.prompt`, `accent.cta`).

- [ ] **Step 7: Format and commit**

```bash
cargo fmt -p vmux_agent
git add crates/vmux_agent/src/vibe/setup/page.rs
git commit -m "feat(agent): real product logo + shared accent on setup page"
```

---

## Task 5: Final verification + runtime hand-off

**Files:** none (verification only).

- [ ] **Step 1: Run all affected native tests**

Run: `cargo test -p vmux_core -p vmux_ui -p vmux_terminal`
Expected: PASS — rkyv round-trip, `agent_accent` tests, all `agent_loading_*` and `favicon` tests.

- [ ] **Step 2: Clippy on the touched crates**

Run: `cargo clippy -p vmux_core -p vmux_ui -p vmux_terminal -p vmux_agent --all-targets`
Expected: no warnings. Fix any before proceeding.

- [ ] **Step 3: Workspace fmt check, protecting vendored patches**

Run: `cargo fmt --all`
Then: `git checkout -- patches/` (cargo fmt reformats vendored `patches/` crates — discard those; see memory "cargo fmt patches").
Then: `git status` — only `crates/` files should be modified. Commit any residual fmt-only changes:

```bash
git add crates/
git commit -m "style: cargo fmt" || true
```

- [ ] **Step 4: WASM page type-check (final)**

Run: `cargo check -p vmux_server --target wasm32-unknown-unknown --features web`
Expected: PASS.

- [ ] **Step 5: Hand off to the user for runtime verification**

Do NOT launch the app yourself (memory: "No unbounded make dev"; the user always runtime-tests). Ask the user to run vmux and confirm, for each of vibe / claude / codex:
- the loading screen shows the correct product logo, agent name, per-agent accent (vibe amber / claude rose-orange / codex emerald), the pulsing chat skeleton + input pill, and "starting…";
- it hands off cleanly to the TUI once the agent is ready;
- the setup page (uninstalled CLI) shows the real product logo in the glass tile with the matching accent.

---

## Self-Review

**Spec coverage:**
- Direction A chat skeleton → Task 3. ✓
- Real favicon in glass tile (loading + setup) → Task 3 (header tile), Task 4 (setup tile). ✓
- Per-agent accent (vibe amber / claude rose-orange / codex emerald) → Task 2 table, consumed in Tasks 3 & 4. ✓
- Single shared accent source → Task 2 (`vmux_ui::agent_accent`), setup's local `Accent`/`accent()` deleted in Task 4. ✓
- Agent identity on the event (terminal URL lacks kind) → Task 1 (`segment`), set at all 3 emit sites. ✓
- No new crates; reuse `Favicon`/`favicon_src_for_url` → Tasks 3 & 4 use `vmux_ui::favicon::Favicon`. ✓
- Tailwind `@source` already covers all touched crates → no CSS task needed. ✓
- Animation = Tailwind `animate-pulse` + `[animation-delay:*]` → Task 3. ✓
- Out of scope (generic pre-snapshot `Loading…`, 10s timeout logic) → untouched. ✓

**Placeholder scan:** none — every code step shows complete, comment-free code.

**Type consistency:** `AgentAccent` fields (`glow_top`, `glow_bottom`, `grad`, `accent_text`, `accent_bg`, `cta_shadow`) defined in Task 2 and referenced identically in Tasks 3 (`grad`, `accent_bg`, `glow_*`) and 4 (`grad`, `accent_text`, `cta_shadow`, `glow_*`). `TermLoadingEvent.segment` defined in Task 1, consumed in Task 3. `Favicon` props (`favicon_url`, `url`, `class`, `globe_class`) match `crates/vmux_ui/src/favicon.rs`. `agent_accent(&str)` signature consistent across all call sites.
