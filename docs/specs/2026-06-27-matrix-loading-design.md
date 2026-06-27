# Matrix Terminal Agent Loading Screen

Date: 2026-06-27
Status: Approved (design)

## Problem

The agent loading splash (`vmux_terminal/src/page.rs`, shown while an agent CLI boots
inside a terminal pane) is a generic skeleton: favicon tile, fake chat-bubble rows, a
fake input bar — all `animate-pulse` over colored radial glows. It is forgettable.

## Goal

Replace it with a Matrix-style digital-rain loading screen, recolored per agent brand,
fun and on-theme for a terminal product. Keep the agent identity legible.

## Decisions (locked in brainstorm)

- **Composition:** full-screen digital rain *behind* a minimal centered boot console
  (agent name + status line + blinking cursor). The chat-bubble skeleton is removed.
- **Glyphs:** authentic Matrix half-width katakana + digits, with the **agent name woven
  in** — a few "word columns" spell `CLAUDE` / `CODEX` / `VIBE` vertically among the glyphs.
- **Color:** rain + console accent come from the agent brand color (`agent_accent`).
  Head glyph bright (toward white), trail fades in the accent.
- **Rendering:** `web-sys` `<canvas>` + `requestAnimationFrame`, the existing interop
  idiom in these crates (sidebar.rs, event_listener.rs, page.rs ResizeObserver). Not
  CSS-only (static glyphs, heavy DOM) and not `document::eval` (no codebase precedent).

## Architecture

### 1. Accent color → raw RGB (`vmux_ui/src/agent_accent.rs`)

Tailwind classes can't reach a `<canvas>`, so add a raw-RGB field to `AgentAccent`:

```rust
pub rain_rgb: &'static str, // "r g b" for CSS rgb(r g b / a)
```

- claude → `"251 113 133"` (rose-400)
- codex  → `"52 211 153"` (emerald-400)
- vibe/default → `"251 146 60"` (orange-400)

Extend the three existing unit tests to assert `rain_rgb` per branch.

### 2. `MatrixRain` component (new file `vmux_terminal/src/matrix_rain.rs`)

Declared `pub mod matrix_rain;` in `vmux_terminal/src/lib.rs` (filename-module pattern,
no mod.rs).

Props:
- `accent_rgb: String` — `"r g b"` from `agent_accent(...).rain_rgb`.
- `words: Vec<String>` — agent-name tokens to weave (e.g. `["CLAUDE"]`).

Render: a single `<canvas class="absolute inset-0 h-full w-full">`.

Lifecycle (`use_effect` on mount):
1. Resolve the canvas element (via `onmounted` `MountedData` → `web_sys` element, or
   `get_element_by_id` with a unique id) and its `2d` context.
2. Size to the parent box × `devicePixelRatio`; `font = "{fontPx}px monospace"`.
   `cols = floor(cssWidth / fontPx)`; per-column `y` drop initialized to a random
   negative row so columns start staggered.
3. Per frame:
   - Fill the whole canvas with `rgba(30, 30, 46, 0.08)` (term-bg at low alpha) to fade
     prior glyphs into trails.
   - For each column: pick a glyph (random katakana/digit; if the column is a "word
     column", the next letter of its agent word). Draw the trailing glyph in
     `rgb({accent} / ~0.85)`; draw the *head* glyph brightened toward white. Advance the
     drop; when it passes the bottom, reset to top with random probability so columns
     desync.
   - A small set of word columns cycle the agent-name letters in bright accent so the
     name is subtly readable in the rain.
4. Store the `requestAnimationFrame` id; **`use_drop` cancels it** (matches
   `event_listener.rs:195`). A `ResizeObserver` (reuse the `page.rs` pattern) recomputes
   `cols`/canvas size on container resize.

Reduced motion: if `matchMedia("(prefers-reduced-motion: reduce)")` matches, draw **one**
static frame and skip the rAF loop.

### 3. Loading branch rewrite (`vmux_terminal/src/page.rs`, current ~301–328)

Replace the skeleton body. Keep the outer
`div.pointer-events-none.absolute.inset-0.z-40.overflow-hidden.bg-term-bg` and the
`state.map(|(label, segment)| ...)` plumbing + `Favicon`.

```
div (absolute inset-0 z-40 overflow-hidden bg-term-bg)
  MatrixRain { accent_rgb, words }                      // z-0, fills box
  div (relative z-10 flex h-full items-center justify-center)
    div  // boot console: soft-glass panel — translucent, rounded, ring, backdrop-blur
      Favicon tile (unchanged)
      div  agent name "{label}"   (font-semibold, {accent.accent_text})
      div  "> booting…"  +  blinking block cursor  (span animate-pulse {accent.accent_bg})
```

The radial `glow_top`/`glow_bottom` divs are dropped (rain replaces them).

### 4. CPU / lifecycle

The rain only exists while `loading.is_some()`. `loading` is cleared on PTY alt-screen or
the 10s timeout (`plugin.rs` `clear_agent_loading`), which unmounts the branch →
`use_drop` cancels the rAF. Bounded to the boot window; reduced-motion path has no loop.
No idle CPU after the agent takes over.

### 5. Scope

Terminal loading splash only. The agent **setup/install** page
(`vmux_agent/src/vibe/setup/page.rs`, which also uses `agent_accent`) is untouched.

## Data flow

`plugin.rs` emits `TermLoadingEvent { loading, label, segment }` →
`page.rs` listener stores `(label, segment)` in the `loading` signal →
loading branch reads it → `agent_accent(&segment)` gives `rain_rgb` + accent classes →
`MatrixRain` animates; boot console shows `label`.

## Error handling / edge cases

- No 2d context / no window → component renders the bare `<canvas>` and the boot console
  still shows (console is plain DOM, independent of canvas). No panic.
- DPR scaling so the rain is crisp on retina.
- Container resize → ResizeObserver re-inits columns; cleaned up on drop.
- reduced-motion → single static frame.
- Unknown segment → `agent_accent` already falls back to vibe/amber; `words` falls back to
  the `label`.

## Testing

- **Native unit (`agent_accent.rs`):** assert `rain_rgb` for claude/codex/vibe.
- **Source-scrape (`vmux_terminal`):** if `tests/page_source.rs` / `style.rs` `include_str!`
  asserts reference the old skeleton markup, update them to the new structure.
- The canvas/rAF path is wasm-only and not unit-testable natively → covered by the final
  runtime test (user runs the app, boots each agent, confirms recolored rain + name).
- `cargo check --target wasm32-unknown-unknown -p vmux_terminal` to typecheck the page.

## Risks

- `tests/page_source.rs` / `style.rs` text-asserts breaking on the markup change (known
  pattern) — update them in the same change.
- rAF cleanup correctness — verified via `use_drop`; the bounded loading window limits
  blast radius even if a frame leaks.

## Files touched

- `crates/vmux_ui/src/agent_accent.rs` — add `rain_rgb` + tests.
- `crates/vmux_terminal/src/matrix_rain.rs` — **new** component.
- `crates/vmux_terminal/src/lib.rs` — `pub mod matrix_rain;`.
- `crates/vmux_terminal/src/page.rs` — rewrite loading branch.
- `crates/vmux_terminal/tests/page_source.rs` / `src/style.rs` — update asserts if needed.
