# Agent Loading Screen Redesign

Date: 2026-06-26
Status: Approved (design)

## Problem

When an agent CLI (vibe / claude / codex) terminal first mounts, vmux shows a
plain centered overlay — a muted label, one `animate-pulse` bar, and the text
`starting…` (`crates/vmux_terminal/src/page.rs:294-315`). It is inconsistent
with the recently modernized setup flow (#174) and file navigator: no brand
logo, no per-agent accent, no sense of the chat UI that is about to appear.

We want the loading screen to feel like the agent's chat UI warming up:
soft-glass theme, the real product logo, the agent name, a pulsing chat-
transcript skeleton, and `starting…`. The setup page should adopt the same real
logo so the two screens share one visual language.

## Goals

- Replace the agent loading overlay with a full-bleed **chat transcript
  skeleton** (Direction A) on the terminal background.
- Show the **real product favicon** (Claude / Mistral / OpenAI) in a glass tile.
- Apply a **per-agent accent**: vibe = amber, claude = rose/orange,
  codex = emerald.
- Swap the **setup page** badge to the same glass-tile + real favicon.
- One **shared accent source** in `vmux_ui` so setup and loading stay in sync.
- No new crates. Reuse existing favicon helpers and the `Favicon` component.

## Non-Goals

- The generic pre-snapshot `Loading…` block for non-agent terminals
  (`page.rs:281-292`) — unchanged.
- The 10s arm/clear timeout logic (`AGENT_LOADING_TIMEOUT`,
  `clear_agent_loading`) — unchanged except for the new event field.
- Custom shimmer keyframes — use Tailwind `animate-pulse`. Shimmer is possible
  later as pure polish.
- Matching each agent's exact TUI layout — the skeleton is a generic stylized
  chat, identical structure for all three agents.

## Background / Current State

- The terminal page is a Dioxus/WASM page registered under host `terminal`
  (`crates/vmux_server/src/lib.rs:46`). Its `window.location` is
  `vmux://terminal/...` and therefore **does not** carry the agent kind. The
  loading screen cannot derive the agent from the URL — the agent identity must
  travel on the loading event.
- Loading is driven by `TermLoadingEvent { loading: bool, label: String }`
  (`crates/vmux_core/src/event.rs:788`). `label` is the display name
  (`"Claude"`). Emitted by `arm_agent_loading` / `clear_agent_loading`
  (`crates/vmux_terminal/src/plugin.rs:2679-2732`), keyed off `AgentSession`.
- `AgentKind` (`crates/vmux_core/src/agent.rs:19`) provides
  `as_url_segment()` (`vibe`/`claude`/`codex`) and `display_name()`.
- Favicon resolution already exists in `crates/vmux_ui/src/favicon.rs`:
  - `agent_host(url)` maps `vmux://agent/{kind}/...` → host
    (`claude.ai`, `chat.mistral.ai`, `chatgpt.com`).
  - `favicon_src_for_url(favicon_url, url)` → real favicon URL, falling back to
    Google s2 (`https://www.google.com/s2/favicons?domain=…&sz=64`).
  - `Favicon { favicon_url, url, class, globe_class }` (wasm-only) renders the
    image with a globe-icon fallback on `onerror`.
- Per-agent accents currently live only in the setup page as Tailwind class
  strings (`crates/vmux_agent/src/vibe/setup/page.rs:32-56`): glow blobs, a
  gradient badge, prompt color, and CTA gradient.

## Design

### 1. Carry the agent segment on the loading event

Add one field to `TermLoadingEvent`:

```rust
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,    // display name, e.g. "Claude" (unchanged)
    pub segment: String,  // url segment, e.g. "claude" / "vibe" / "codex"
}
```

Producers set `segment = session.kind.as_url_segment().to_string()` in both
`arm_agent_loading` and `clear_agent_loading`
(`crates/vmux_terminal/src/plugin.rs`). It is additive and rkyv-compatible.

### 2. Shared accent in `vmux_ui`

New module `crates/vmux_ui/src/agent_accent.rs` (filename-based module pattern,
registered in `crates/vmux_ui/src/lib.rs`). Pure, target-agnostic, returns
`&'static str` Tailwind classes so the scanner sees literals:

```rust
pub struct AgentAccent {
    pub glow_top: &'static str,     // radial blur blob, top
    pub glow_bottom: &'static str,  // radial blur blob, bottom
    pub grad: &'static str,         // e.g. "from-orange-400 to-rose-500"
    pub accent_text: &'static str,  // e.g. "text-rose-400"
}

pub fn agent_accent(segment: &str) -> AgentAccent { /* match segment */ }
```

Palette (carried over from the setup page, confirmed in the mockups):

| segment | grad                       | accent_text       | glow            |
|---------|----------------------------|-------------------|-----------------|
| claude  | from-orange-400 to-rose-500| text-rose-400     | rose / orange   |
| codex   | from-emerald-500 to-teal-600| text-emerald-400 | emerald / teal  |
| vibe    | from-orange-500 to-amber-600| text-orange-400  | orange / amber  |

(`vibe` is the default / fallback arm.)

### 3. Loading overlay — Direction A (chat transcript skeleton)

Rewrite the loading block in `crates/vmux_terminal/src/page.rs` (the
`label.map(...)` overlay at lines 294-315). The overlay is full-bleed,
`absolute inset-0 z-40`, on `bg-term-bg`, with the accent glow blobs behind:

- **Header row**: glass tile (`bg-white/[0.06] ring-1 ring-inset ring-white/10
  rounded-xl`) containing `Favicon { favicon_url: "", url:
  format!("vmux://agent/{segment}/cli/") }`; then the agent name (`label`) and a
  small accent dot + `starting…`.
- **Body (ghost chat)**:
  - one right-aligned user bubble — a single `animate-pulse` bar + muted avatar.
  - one assistant row — accent-gradient avatar (`bg-gradient-to-br {grad}`) + 2–3
    `animate-pulse` skeleton bars of varying widths with staggered
    `[animation-delay:120ms]` / `[240ms]` for a typing cascade.
- **Footer**: skeleton input pill (`rounded-xl ring-1 ring-inset ring-white/10`)
  with an accent caret (`animate-pulse`).

The terminal page already calls `use_theme()` and imports `vmux_ui`. It will
import `vmux_ui::favicon::Favicon` and `vmux_ui::agent_accent::agent_accent`.

### 4. Setup page — real logo

In `crates/vmux_agent/src/vibe/setup/page.rs`:

- Replace the gradient `badge` + inline download `Icon` (lines 73-80) with a
  glass tile holding `Favicon { favicon_url: "", url:
  format!("vmux://agent/{segment}/cli/") }` — same treatment as the loading
  header.
- Refactor the local `accent()` struct to build its `glow_*`, `prompt`, and
  `cta` classes from the shared `agent_accent(segment)` (`grad` drives the CTA
  gradient and prompt/`accent_text`). The `tagline()` helper stays local.
- Everything else (install-command box, CTA copy + behavior, footer) unchanged.

## Data Flow

```text
AgentSession terminal becomes PageReady
  └─ arm_agent_loading  →  TermLoadingEvent { loading:true, label:"Claude", segment:"claude" }
        └─ terminal page (host=terminal) receives event
              ├─ Favicon(url = "vmux://agent/claude/cli/")  → favicon_src_for_url → google s2 → claude.ai
              ├─ agent_accent("claude")                     → grad + glow + accent_text
              └─ render Direction-A overlay
  └─ alt-screen entered OR 10s timeout
        └─ clear_agent_loading → TermLoadingEvent { loading:false, … } → overlay removed, TUI shows
```

## Error / Edge Handling

- **Favicon fails to load** (offline / blocked): `Favicon` already falls back to
  the globe icon via `onerror`. The name + accent + skeleton still render.
- **Unknown / missing segment**: `agent_accent` defaults to the `vibe` arm;
  `favicon_src_for_url` returns `None` → globe icon. No panic, no blank screen.
- **Non-agent terminals**: never receive `arm_agent_loading` (it queries
  `AgentSession`), so the overlay never shows — unchanged behavior.

## Tailwind

The new utility classes (`bg-white/[0.06]`, `bg-gradient-to-br`,
`from-* to-*`, `text-rose-400`, `[animation-delay:*]`, etc.) appear as literals
in `vmux_terminal/src`, `vmux_ui/src`, and `vmux_agent/src`. Verify all three
are in the `@source` globs of `crates/vmux_server/assets/index.css` so the
classes are generated (vmux_ui must be present because the shared accent strings
live there). Add any missing `@source` entry. Confirm the WASM page-rebuild
tracking in `crates/vmux_server/build.rs` already covers these crates' `src`.

## Testing

- **Unit (native)**: existing `agent_loading_*` tests in
  `crates/vmux_terminal/src/plugin.rs` stay green; update event construction to
  include `segment` and assert it is the agent's url segment.
- **Unit (native)**: existing `favicon.rs` tests already cover
  `favicon_src_for_url` → host. Add a small test for `agent_accent(segment)`
  returning the expected `grad` per agent and the `vibe` fallback for unknown
  input.
- **rkyv round-trip**: the `TermLoadingEvent` serialize/deserialize test in
  `crates/vmux_core/src/event.rs:1482` updated for the new field.
- **Manual / runtime (one pass at the end, user-run)**: launch each of vibe /
  claude / codex; confirm the loading screen shows the right logo + accent +
  skeleton, then hands off cleanly to the TUI; confirm the setup page shows the
  real logo.

## Files Touched

- `crates/vmux_core/src/event.rs` — add `segment` to `TermLoadingEvent` (+ test).
- `crates/vmux_terminal/src/plugin.rs` — set `segment` in arm/clear (+ tests).
- `crates/vmux_terminal/src/page.rs` — rewrite loading overlay (Direction A).
- `crates/vmux_ui/src/agent_accent.rs` — new shared accent module (+ test).
- `crates/vmux_ui/src/lib.rs` — register module.
- `crates/vmux_agent/src/vibe/setup/page.rs` — glass-tile favicon + use shared accent.
- `crates/vmux_server/assets/index.css` — `@source` entries if missing.
