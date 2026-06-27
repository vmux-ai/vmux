# vmux://home — Design

Date: 2026-06-27
Branch: `feat/vmux-home`

## Summary

Add a new internal page `vmux://home` and make it the default `startup_url`. It is a
Google-style launcher: a centered wordmark and one large input, with the command bar's
full results below it. The input has **full command-bar parity** — live fuzzy/substring
filtering as you type, file path completion with ghost text, history suggestions, commands,
pages, open tabs, spaces — achieved by **sharing the command bar's composition** (the same
ECS payload, the same `filter_results`, the same dispatch event, the same keyboard module),
not by duplicating it. Today Enter opens/searches in place; the input is also the seed of a
future chat interface.

## Goals

- New `vmux://home/` Dioxus page, soft-glass design matching recent pages (agent setup, LSP).
- One centered input ("Search or ask…"), autofocused.
- **Full command-bar parity**, reusing existing code:
  - live filtering of all entry types as you type (`results.rs::filter_results`),
  - file path completion + ghost text (`PathCompleteRequest`/`Response`),
  - history suggestions (`HistorySuggestionsRequest`/`Response`),
  - pages, commands, open tabs, spaces (the `CommandBarOpenEvent` payload),
  - keyboard nav (arrows / Ctrl-n/p / Tab / Enter / Esc — `keyboard.rs`).
- Selecting an entry / submitting opens **in place** (replaces home in the current tab).
- `vmux://home/` becomes the default `startup_url`.

## Non-goals (YAGNI)

- Chat interface (future; the input is the placeholder for it).
- Removing or changing the Cmd+K command bar modal — it stays; home shares its internals.
- New filtering/dispatch logic — everything routes through the existing command-bar code.

## Architecture

The design principle: **home is the command bar's body, rendered as a centered page
instead of a modal overlay.** The modal-specific parts (prewarm, reveal frames, size
reporting, keyboard-target swap, positioning) stay with the modal. Everything else —
payload, filtering, completion, history, keyboard, dispatch — is shared.

### Placement — reuse `vmux_layout`, no new crate

The home page is the command bar in a different shell, so it lives alongside it in
`vmux_layout`, which already: is a `vmux_server` WASM dependency and hosts `command-bar`;
is in the Tailwind `@source` list (`crates/vmux_server/assets/index.css:7`); is tracked for
WASM rebuilds (`crates/vmux_server/build.rs:20`).

New files (filename-based module pattern, no `mod.rs`):

- `crates/vmux_layout/src/home.rs` — native: `PAGE_MANIFEST` + `HomePlugin`. Declares
  `pub mod page;` (`#[cfg(target_arch = "wasm32")]`) and `pub mod event;` (always).
- `crates/vmux_layout/src/home/page.rs` — Dioxus `#[component] pub fn Page()`: the centered
  hero shell wrapping the shared palette component.
- `crates/vmux_layout/src/home/event.rs` — `HomeDataRequest` (the only new event).

`pub mod home;` is added to `crates/vmux_layout/src/lib.rs`.

### Shared composition — WASM (Dioxus)

Extract the command bar's input+results body from `crates/vmux_layout/src/command_bar/page.rs`
into a shared component, new file `crates/vmux_layout/src/command_bar/palette.rs`:

```rust
#[component]
pub fn CommandPalette(props: PaletteProps) -> Element
// owns: query signal; CommandBarOpenEvent listener -> payload signal;
//       filter_results(payload, query); results list + result rows;
//       keyboard.rs handling; ghost-text path completion
//       (PathCompleteRequest/Response); history (HistorySuggestionsRequest/Response);
//       dispatch via CommandBarActionEvent.
```

`PaletteProps` carries the presentation/behavior differences:
- `variant: PaletteVariant` (`Modal` | `Home`) — drives outer classes only,
- `default_target: OpenTarget` (`InPlace` for Home),
- optional `on_measured` callback (modal uses it to report `CommandBarSizeEvent`; home ignores).

Both `command_bar/page.rs` (modal shell: prewarm/reveal/size as today) and `home/page.rs`
(centered hero shell) render `CommandPalette`. The pure logic — `results.rs::filter_results`,
`looks_like_path`, `completion_query`, and `keyboard.rs` — is reused unchanged.

> Refactor caution: source-scrape tests assert on `command_bar/page.rs` text via
> `include_str!` (`command_bar/style.rs`, `crates/vmux_layout/tests/page_source.rs`). Moving
> markup into `palette.rs` will break them; update those tests to scan the new file
> (or assert against `palette.rs`). Run `cargo test -p vmux_layout` to catch it.

### Shared composition — host (ECS)

The payload the modal sends (`CommandBarOpenEvent`: spaces, tabs, commands, pages, …) is
built inside `handle_open_command_bar`. Extract the data-gathering into reusable helpers in
`command_bar/handler.rs`, called by both the modal system and the new home observer:

- `gather_command_bar_tabs(active_tab, &queries…) -> Vec<CommandBarTab>` — the pane-tree
  walk currently inline at `handler.rs:803-844`.
- `build_command_bar_open_payload(pages_snap, spaces_snap, agents_snap, tabs, target) ->
  CommandBarOpenEvent` — wraps the existing `command_bar_open_payload(...)` (`handler.rs:961`)
  with the snapshot→Vec mapping for pages/commands/spaces.

The modal path keeps its reveal/target/`open_id` wrapping; it just calls these helpers for
the data.

### Data feed (host → page) — reuse `CommandBarOpenEvent`

Home is page content, not the modal, so it requests its payload on load:

- New event `crates/vmux_layout/src/home/event.rs`:
  ```rust
  pub const HOME_DATA_REQUEST_EVENT: &str = "home-data-request";
  pub struct HomeDataRequest;   // page -> host, on mount
  // derives: Clone, Debug, Default, serde::{Serialize,Deserialize}, rkyv::{Archive,Serialize,Deserialize}
  ```
- Host observer `on_home_data_request` reads the **requesting browser entity** from the
  trigger, builds the payload via the shared helpers (`target = InPlace`), and emits it back
  to that entity exactly as the modal does:
  ```rust
  let event = BinHostEmitEvent::from_rkyv(requesting_entity, COMMAND_BAR_OPEN_EVENT, &payload);
  commands.trigger(event);
  ```
- The home page listens for `CommandBarOpenEvent` (reused) and feeds `filter_results`.

Path completion and history reuse their existing request/response events and observers; the
responses already target the requesting entity, so they work for home once the home page is
allowed to emit the requests (see Registration).

### Dispatch (page → host) — reuse `CommandBarActionEvent`

No new dispatch path. The palette emits `CommandBarActionEvent { action, value, target }`
(`vmux_command::event:121`) → the host's existing `on_command_bar_action`
(`command_bar/handler.rs:1037`) handles open / command / space / terminal / switch_tab.
Home selections use `target: Some(OpenTarget::InPlace)` (the enum default,
`open_target.rs:77-78`), replacing the current stack's content (home). URL-vs-search
normalization is already host-side.

### Registration

- Inbound emitters (page→host) — add `"home"` to the host list so the home page may emit
  these. Today: `BinEventEmitterPlugin::<( CommandBarActionEvent, PathCompleteRequest, … )>
  ::for_hosts(&["command-bar"])` (`handler.rs:95-101`). Add `"home"`, and register
  `HomeDataRequest` (+ history request host wiring) for `"home"`.
- WASM router: add to `web_pages!` (`crates/vmux_server/src/lib.rs:41-56`):
  `render_home: "home" => vmux_layout::home::page::Page,`
- Desktop app: add `HomePlugin` to the plugin tuple in `crates/vmux_desktop/src/lib.rs`
  (~lines 101-144). `HomePlugin::build` spawns `PAGE_MANIFEST`, registers the home host on the
  emitters, and adds `on_home_data_request`; per AGENTS.md it registers any message types it
  writes in `build()` (idempotent).

### Page manifest

```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "home",
    title: "Home",
    keywords: &["home", "start", "new tab", "launcher"],
    icon: Some(BuiltinIcon::Sparkles), // reuse existing variant (dedicated Home glyph optional)
    command_bar: true,                  // also reachable from Cmd+K
};
```
(Field names/types mirror existing manifests, e.g. `vmux_agent/src/vibe/setup.rs:17`.)

### UI (Dioxus, soft-glass)

`home/page.rs` is a thin centered shell around `CommandPalette`:
- `use_theme()` first; emit `HomeDataRequest` on mount (the palette owns the rest).
- Root: `main` full-screen centered column on `bg-background text-foreground`, subtle
  radial-gradient glow.
- `vmux` wordmark (+ sparkle), large, `font-semibold tracking-tight`.
- The palette renders the big input pill (`rounded-2xl bg-white/[0.04] px-5 py-4 text-base
  ring-1 ring-inset ring-white/10 backdrop-blur-2xl focus:ring-white/20`, autofocused) and,
  below it, the live results list (max-height, scroll) using the shared result rows.
- Empty query shows the default set (pages / open tabs / recent); typing filters live; a
  path-like query shows file completions + ghost text; `>` enters command mode — all from the
  shared logic.

Icons via `Icon { path { d:"…" } }` / `PageIconView` only — no Nerd Font glyphs.

### Startup default swap

- `crates/vmux_setting/src/settings.ron:1-4` → `startup_url: "vmux://home/"`.
- `default_browser_startup_url()` (`vmux_setting/src/plugin/runtime.rs:306`) → `"vmux://home/"`.
- Legacy coercion in `resolve_startup_url` (`runtime.rs:149-160`) mapping `vmux://agent/` /
  `vmux://agent` to the old default is retargeted to `"vmux://home/"`.
- Update `vmux_setting` tests asserting the google default
  (`parse_settings_empty_uses_embedded_defaults`; the `resolve_startup_url_*` suite,
  `runtime.rs` ~1066-1138, ~1378-1382).

`StartupUrlOrPrompt` already opens `EffectiveStartupUrl` when non-empty
(`vmux_layout/src/window.rs:560-578`), so home opens on a fresh start with no other change.
Clearing the field still falls back to the command-bar prompt.

## File-by-file change list

New:
- `crates/vmux_layout/src/home.rs`
- `crates/vmux_layout/src/home/page.rs`
- `crates/vmux_layout/src/home/event.rs`
- `crates/vmux_layout/src/command_bar/palette.rs` (shared `CommandPalette` body)

Modified:
- `crates/vmux_layout/src/lib.rs` — `pub mod home;`
- `crates/vmux_layout/src/command_bar.rs` — `pub mod palette;`
- `crates/vmux_layout/src/command_bar/page.rs` — render shared `CommandPalette` (modal shell only).
- `crates/vmux_layout/src/command_bar/handler.rs` — extract `gather_command_bar_tabs` /
  `build_command_bar_open_payload`; add `"home"` to the emitter host lists; add
  `on_home_data_request`.
- `crates/vmux_layout/src/command_bar/style.rs` + `crates/vmux_layout/tests/page_source.rs` —
  update source-scrape targets to `palette.rs`.
- `crates/vmux_server/src/lib.rs` — `web_pages!` home row.
- `crates/vmux_desktop/src/lib.rs` — add `HomePlugin`.
- `crates/vmux_setting/src/settings.ron` — default `startup_url`.
- `crates/vmux_setting/src/plugin/runtime.rs` — default fn + legacy coercion + tests.

## Testing

- `vmux_layout` native unit tests: `gather_command_bar_tabs` (pure-ish; via a small ECS app
  per AGENTS.md message+system pattern) and `build_command_bar_open_payload` (snapshot→payload);
  `HomePlugin` registers its written messages (assert via app build).
- Update the source-scrape tests for the `palette.rs` move; confirm `filter_results` tests
  still pass unchanged.
- `vmux_setting` tests updated for the new default and coercion.
- `cargo test -p vmux_layout -p vmux_setting` during the edit loop; `cargo test --workspace`
  before push.
- Final manual runtime test by the user: home opens on launch; type → live filter; path →
  completion + ghost text; history shows; Enter/click opens in place; Cmd+K modal still works.

## Risks / notes

- **Source-scrape fragility** — moving markup out of `command_bar/page.rs` breaks the
  `include_str!` text-assert tests; update them in the same change. `cargo test -p vmux_layout`
  is the gate.
- **Entity-targeted emit** — `CommandBarOpenEvent` is sent to a specific CEF browser entity.
  `on_home_data_request` must reply to the requesting entity (from the trigger), not a fixed
  modal entity.
- **Host-list parity** — `"home"` must be added to every inbound emitter the palette uses
  (`CommandBarActionEvent`, `PathCompleteRequest`, history request) or those features silently
  fail on home while working in the modal.
- **Keyboard target** — home is normal focused content (no modal keyboard-target swap), so
  typing works natively; verify `on_command_bar_action`'s post-action focus restore behaves on
  the non-modal home stack.
- Per-worktree CEF build: use this worktree's own `target/`; do not share `CARGO_TARGET_DIR`.
