# vmux://home — Design

Date: 2026-06-27
Branch: `feat/vmux-home`

## Summary

Add a new internal page `vmux://home` and make it the default `startup_url`. It is a
Google-style launcher: a centered wordmark, one large search/ask input, and the
command-bar entries shown below as quick-launch tiles + a recent list. The text input
is the seed of a future chat interface; for now Enter submits a search/navigate.

## Goals

- New `vmux://home/` Dioxus page, soft-glass design matching recent pages (agent setup, LSP).
- One centered text input ("Search or ask…"), autofocused, Enter = search/navigate.
- Below the input: the same entry set the command bar surfaces (internal pages,
  commands, recent history), rendered as a launcher (always visible, not filtered by typing).
- Selecting an entry / submitting opens **in place** (replaces home in the current tab).
- `vmux://home/` becomes the default `startup_url`.

## Non-goals (YAGNI)

- Chat interface (future; the input is the placeholder for it).
- Live filtering of the entries as you type — the input is submit-only. The Cmd+K
  command bar remains the live, fuzzy launcher.
- File path completion / terminal-spawn entries on home.
- Removing or changing the Cmd+K command bar modal.

## Architecture

### Placement — reuse `vmux_layout`, no new crate

The home page is tightly coupled to the command bar (same entries, same dispatch), so it
lives in `vmux_layout`, which already:
- is a `vmux_server` WASM dependency and hosts the `command-bar` page,
- is in the Tailwind `@source` list (`crates/vmux_server/assets/index.css:7`),
- is tracked for WASM rebuilds (`crates/vmux_server/build.rs:20`).

New files (filename-based module pattern, no `mod.rs`):

- `crates/vmux_layout/src/home.rs` — native side: `PAGE_MANIFEST` + `HomePlugin`. Declares
  `pub mod page;` (`#[cfg(target_arch = "wasm32")]`) and `pub mod event;` (always).
- `crates/vmux_layout/src/home/page.rs` — Dioxus `#[component] pub fn Page()` (`#[cfg(target_arch = "wasm32")]`).
- `crates/vmux_layout/src/home/event.rs` — `HomeDataRequest` / `HomeDataEvent` (host↔page payload).

`pub mod home;` is added to `crates/vmux_layout/src/lib.rs`.

### Page manifest

```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "home",
    title: "Home",
    keywords: &["home", "start", "new tab", "launcher"],
    icon: Some(BuiltinIcon::Sparkles), // reuse existing variant (a dedicated Home glyph is a nice-to-have)
    command_bar: true,                  // also reachable from Cmd+K
};
```

(Field names mirror the existing manifests, e.g. `vmux_agent/src/vibe/setup.rs:17`.)

### Data feed (host → page)

Home is page content, not the Cmd+K modal, so it cannot rely on the modal's
open-time payload. It requests data on load.

New events in `crates/vmux_layout/src/home/event.rs`, reusing the command-bar payload
type `CommandBarPage` from `vmux_command::event` (same derive set as the existing
events: `Clone, Debug, Default, serde::{Serialize,Deserialize}, rkyv::{Archive,Serialize,Deserialize}`):

```rust
pub const HOME_DATA_EVENT: &str = "home-data";

pub struct HomeDataRequest;            // page -> host, on mount

pub struct HomeDataEvent {             // host -> page (host "home")
    pub pages: Vec<CommandBarPage>,
}
```

MVP carries `pages` (internal pages incl. agent pages); "recent" comes from the history
events below. Commands / spaces / open-tabs stay in the Cmd+K command bar and are out of
the home MVP UI — the dispatch path (below) is generic, so adding them later is trivial.

Host observer `on_home_data_request` builds `pages` by reusing the command-bar page
sources already maintained for the modal (`CommandBarPagesSnapshot` + `agent_pages()`) and
emits `HomeDataEvent` to the `home` host.

**Recent history** reuses the existing `HistorySuggestionsRequest` /
`HistorySuggestionsResponse` (`vmux_command::event`, from `vmux_history`). The page emits a
request with an empty query (→ most-recent) and renders the response. The history
suggestions emitter/observer is extended to include the `home` host.

### Dispatch (page → host) — reuse `CommandBarActionEvent`

No new dispatch path. `CommandBarActionEvent { action, value, target: Option<OpenTarget> }`
(`vmux_command::event:121`) already routes everything through the host's
`on_command_bar_action` (`command_bar/handler.rs:1037`). We register the home host on the
existing emitter:

`crates/vmux_layout/src/command_bar/handler.rs:101`
```rust
BinEventEmitterPlugin::<( CommandBarActionEvent, /* … */ )>::for_hosts(&["command-bar", "home"])
```

Mappings emitted from the page:

| Home action (MVP) | CommandBarActionEvent |
|---|---|
| search submit (Enter) | `{ action:"open", value: <query>, target: Some(InPlace) }` |
| page tile click | `{ action:"open", value: <page.url>, target: Some(InPlace) }` |
| recent (history) click | `{ action:"open", value: <url>, target: Some(InPlace) }` |

`OpenTarget::InPlace` is the enum default (`open_target.rs:77-78`); the host's `"open"`
handler already normalizes URL-vs-search and applies the target, replacing the current
stack's content (home). The same event also supports `"command"` / `"space"` for a later
pass, but those are not surfaced in the home MVP UI.

### UI (Dioxus, soft-glass)

Follows the conventions in `vmux_agent/src/vibe/setup/page.rs` and
`vmux_editor/src/lsp_page.rs`:

- `use_theme()` first; register `HomeDataEvent` + `HistorySuggestionsResponse` listeners;
  `use_effect` to emit `HomeDataRequest` + history request on mount.
- Root: `main` full-screen centered column on `bg-background text-foreground`, subtle
  radial-gradient glow (inline `style` or a glow div).
- Wordmark: `vmux` (+ sparkle), large, `font-semibold tracking-tight`.
- Search input: large pill —
  `w-full max-w-xl rounded-2xl bg-white/[0.04] px-5 py-4 text-base ring-1 ring-inset
  ring-white/10 backdrop-blur-2xl outline-none focus:ring-white/20`, placeholder
  "Search or ask…", autofocused, Enter submits non-empty query.
- Quick-launch tiles: internal pages from `HomeDataEvent.pages` as glass chips with
  `PageIconView` / `builtin_icon`.
- Recent: a "Recent" section listing history rows (favicon + title), top ~6, clickable.
- Empty/loading: render nothing (or a faint skeleton) until first payload arrives.

Icons via `Icon { path { d:"…" } }` / `PageIconView` only — no Nerd Font glyphs.

### Startup default swap

- `crates/vmux_setting/src/settings.ron:1-4` → `startup_url: "vmux://home/"`.
- `default_browser_startup_url()` (`vmux_setting/src/plugin/runtime.rs:306`) → `"vmux://home/"`.
- Legacy coercion in `resolve_startup_url` (`runtime.rs:149-160`) that maps `vmux://agent/`
  / `vmux://agent` to the old default is retargeted to `"vmux://home/"`.
- Update `vmux_setting` tests asserting the google default
  (`parse_settings_empty_uses_embedded_defaults`, the `resolve_startup_url_*` suite,
  `runtime.rs` ~1066-1138, ~1378-1382).

`StartupUrlOrPrompt` already opens `EffectiveStartupUrl` when non-empty
(`vmux_layout/src/window.rs:560-578`), so home opens on a fresh start with no other
change. Clearing the field still falls back to the command-bar prompt.

### Registration

- WASM router: add to `web_pages!` (`crates/vmux_server/src/lib.rs:41-56`):
  `render_home: "home" => vmux_layout::home::page::Page,`
- Desktop app: add `HomePlugin` to the plugin tuple in `crates/vmux_desktop/src/lib.rs`
  (near the other page plugins, ~lines 101-144).
- `HomePlugin::build` spawns `PAGE_MANIFEST`, registers `BinEventEmitterPlugin`/observer for
  `HomeDataRequest`, the `HomeDataEvent` emit path, and the home-host history wiring; per
  AGENTS.md it registers any message types it writes in `build()` (idempotent).

## File-by-file change list

New:
- `crates/vmux_layout/src/home.rs`
- `crates/vmux_layout/src/home/page.rs`
- `crates/vmux_layout/src/home/event.rs`

Modified:
- `crates/vmux_layout/src/lib.rs` — `pub mod home;`
- `crates/vmux_layout/src/command_bar/handler.rs` — add `"home"` to `CommandBarActionEvent`
  `for_hosts`; extend history-suggestions host wiring for `home`.
- `crates/vmux_server/src/lib.rs` — `web_pages!` home row.
- `crates/vmux_desktop/src/lib.rs` — add `HomePlugin`.
- `crates/vmux_setting/src/settings.ron` — default `startup_url`.
- `crates/vmux_setting/src/plugin/runtime.rs` — default fn + legacy coercion + tests.

## Testing

- `vmux_layout` native unit tests: the page→action mapping (pure fn from a result item to
  `CommandBarActionEvent`) and the `HomeDataEvent` payload builder; `HomePlugin` registers
  its written messages (assert via app build, per AGENTS.md message+system pattern).
- `vmux_setting` tests updated for the new default and coercion.
- `cargo test -p vmux_layout -p vmux_setting` during the edit loop; `cargo test --workspace`
  before push.
- Final manual runtime test by the user (launch → home opens; search/select opens in place;
  tiles/recent work).

## Risks / notes

- Per-worktree CEF build: use this worktree's own `target/`; do not share `CARGO_TARGET_DIR`.
- `home` host must be added to **both** the emitter `for_hosts` (page→host actions) and any
  host→page emitter (`HomeDataEvent`, history) or events silently won't cross.
- Keep `home` in the Tailwind `@source` coverage (already covered via `vmux_layout/src`).
