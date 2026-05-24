# History Support — Design

**Date:** 2026-05-16
**Status:** Draft (awaiting review)
**Scope:** Real `Visit` spawning from CEF navigation, history page UI, omnibox history search, MCP history tools.
**Out of scope:** Per-tab CEF nav stack serialization across restart, cross-space global history, redirect-chain UI.

## Background

`vmux_history` exists as a POC. The `Visit` marker component, `PageMetadata`, `CreatedAt`, and `LastActivatedAt` are defined in `vmux_core` and persisted via `moonshine-save`. Nothing spawns `Visit` entities at runtime — the POC reads only sample data or entities from prior sessions (none, in practice).

Existing CEF integration surfaces:

- `WebviewChromeStateEvent` — partial events for URL / title / favicon (no main-frame discrimination)
- `WebviewLoadingStateEvent` — loading flag + `can_go_back` / `can_go_forward`
- `BrowserCommand::PrevPage` / `NextPage` — wired to `Cmd+[` / `Cmd+]`, chrome buttons, trackpad swipe

CEF's `cef_transition_type_t` is **not** exposed by the patched `bevy_cef`. Adding it is part of this work.

## Goals

1. Every committed main-frame navigation produces a `Visit` row (modulo back/forward dedup).
2. Dedicated history page at `vmux://history` with day-grouped flat timeline, search, infinite scroll, delete.
3. Omnibox suggestions include history matches, ranked by frecency.
4. MCP tools expose back/forward navigation and history search to agents.
5. 90-day silent prune bounds storage growth.

## Non-Goals

- Persisting per-tab CEF back/forward stack across restarts.
- Aggregating history across spaces (history stays per-space, via the existing moonshine-save space file).
- Redirect chain UI / `from_visit` foreign key.
- Per-profile isolation beyond what per-space already provides.
- Typed-URL-only ranking boosts beyond the `TransitionType` enum below.

## Data Model

Normalized Url + Visit, mirroring Chrome's `urls` + `visits` schema. All new components live in `crates/vmux_core/src/lib.rs`.

```rust
// === Url entity — one per unique URL ===
#[derive(Reflect, Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Url;

#[derive(Reflect, Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitCount(pub u32);

#[derive(Reflect, Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastVisitedAt(pub i64); // millis since epoch

// Url entity also carries existing PageMetadata (latest title/favicon/bg) and CreatedAt.

// === Visit entity — one per navigation event ===
// `Visit` marker already exists in vmux_core. Repurposed here.
#[derive(Reflect, Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Visit;

#[derive(Reflect, Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitedUrl(pub Entity); // FK → Url entity

#[derive(Reflect, Component, Default, Clone, Copy, PartialEq, Eq)]
#[require(Save)]
#[type_path = "vmux_history"]
pub enum TransitionType {
    #[default] Link,
    Typed,
    Reload,
    BackForward,
    Redirect,
    Other,
}

// Visit entity also carries existing CreatedAt (timestamp of this specific visit).
```

`Url` and `Visit` are separate entities. `Visit.VisitedUrl(e)` points to the `Url`. `LastActivatedAt` is **not** used on either — it remains on Tab entities for tab-activation tracking, unchanged.

`PageMetadata` on the `Url` entity is the canonical title/favicon for that URL. It refreshes when later `title_change` / `favicon_change` events arrive for the active tab.

### Persistence

Add new components to the moonshine-save allowlist in `crates/vmux_desktop/src/persistence.rs` (currently at lines 139–154):

- `Url`, `VisitCount`, `LastVisitedAt`, `VisitedUrl`, `TransitionType`

Existing `Visit`, `PageMetadata`, `CreatedAt` already on the allowlist. Existing space files have zero rows of either kind (no runtime spawner exists), so no migration is required.

### Migration: existing `Visit` semantics

The current `Visit` marker in `vmux_core` is meant to tag entities that bundle `(Visit, PageMetadata, CreatedAt, LastActivatedAt)`. In the new model, those bundles become `Url` entities; `Visit` is repurposed for the per-navigation row. Because no Url-like entities exist in any saved space file today, this is a redefinition, not a data migration.

## `bevy_cef` Patch

The patched `bevy_cef_core` lives at `patches/bevy_cef_core-0.5.2/`. Extend it:

1. Add CEF `LoadHandler::OnLoadStart(browser, frame, transition_type)` wiring. This fires after the navigation commits (so a cancelled navigation never produces a Visit) and carries `cef_transition_type_t` directly. The existing `LoadHandler` already has `on_loading_state_change` — extend the same handler implementation.
2. Map the core transition type bits (low 8 bits) to a Rust enum:

```rust
#[derive(Clone, Copy, Debug)]
pub enum CefTransitionCore {
    Link, Explicit, AutoBookmark, AutoSubframe, ManualSubframe,
    Generated, AutoToplevel, FormSubmit, Reload, Keyword, KeywordGenerated,
}
```

3. Plus the qualifier flags (high bits):

```rust
pub struct CefTransitionQualifiers {
    pub forward_back: bool,
    pub from_address_bar: bool,
    pub client_redirect: bool,
    pub server_redirect: bool,
    pub chain_start: bool,
    pub chain_end: bool,
}
```

4. Emit a new Bevy event:

```rust
#[derive(Event, Debug, Clone)]
pub struct WebviewCommittedNavigationEvent {
    pub webview: Entity,
    pub url: String,
    pub is_main_frame: bool,
    pub transition: CefTransitionCore,
    pub qualifiers: CefTransitionQualifiers,
}
```

Fire it from `OnLoadStart`. Subframes set `is_main_frame = false`; consumers filter on `is_main_frame == true`.

## Visit-Spawning System (`vmux_history`)

Replace the existing `push_history_via_host_emit` system with a navigation pipeline. New system runs on `Update`:

```text
WebviewCommittedNavigationEvent
    └─► skip if !is_main_frame
    └─► skip if url starts with "vmux://"
    └─► map CefTransitionCore + qualifiers → TransitionType
    └─► find_or_spawn_url(url):
            if exists: Entity = existing
            else: spawn (Url, PageMetadata { url, ..default }, VisitCount(0),
                         LastVisitedAt(0), CreatedAt(now))
    └─► VisitCount += 1
    └─► LastVisitedAt = now
    └─► if transition != BackForward:
            spawn (Visit, CreatedAt(now), VisitedUrl(url_e), transition)
```

Mapping:

| CEF core type + qualifiers | `TransitionType` |
|---|---|
| any + `forward_back` qualifier | `BackForward` |
| `Reload` | `Reload` |
| any + (`client_redirect` or `server_redirect`) | `Redirect` |
| `Explicit` (typed) or `Generated` (omnibox match) or `Keyword*` | `Typed` |
| `Link` or `FormSubmit` or `AutoBookmark` | `Link` |
| `AutoToplevel`, others | `Other` |

A separate `apply_chrome_state_from_cef`-style system already updates `PageMetadata` on the **Browser/Tab** entity from CEF events (`crates/vmux_layout/src/chrome.rs:41`). Extend it (or add a peer system) so that when a `title_change` / `favicon_change` event lands on a Tab, it also looks up the `Url` entity matching the Tab's current URL and updates that `Url`'s `PageMetadata`. URL match is exact-string. If no `Url` exists for that URL yet (the `OnLoadStart` event has not yet fired), skip silently.

### URL lookup

`find_or_spawn_url` is a linear scan over Url entities in MVP (90-day prune bounds count). If profiling shows this is hot, add a `Resource<UrlIndex(HashMap<String, Entity>)>` updated on spawn/despawn.

### Prune

New system runs once on startup and then every hour via `IntoSystemConfigs::run_if(on_timer(Duration::from_secs(3600)))`:

```text
now = current_millis()
cutoff = now - 90 * 86_400_000
for each Visit with CreatedAt < cutoff: despawn
for each Url with LastVisitedAt < cutoff and no remaining Visit refs: despawn
```

## History Page (`vmux_history`)

Replace `crates/vmux_history/src/app.rs` POC with a Dioxus app.

### Layout

Flat timeline grouped by day, matching Q3 option B:

```text
┌──────────────────────────────────────────────┐
│ ┌──────────────────────────────────────────┐ │
│ │ Search history                       [X] │ │  ← search input
│ └──────────────────────────────────────────┘ │
│                                              │
│ TODAY                                        │
│  14:32  ▢ github.com — vmux/vmux        [×] │
│  14:18  ▢ linear.app — VMX-88           [×] │
│  13:50  ▢ news.ycombinator.com          [×] │
│                                              │
│ YESTERDAY                                    │
│  22:11  ▢ docs.bevy.org — Plugin        [×] │
│                                              │
│ MAY 12, 2026                                 │
│  09:30  ▢ ...                                │
│  …                                           │
│                                              │
│           [Loading more…]                    │  ← infinite scroll sentinel
└──────────────────────────────────────────────┘
```

Top-right of header: "Clear all" button — confirms via modal before despawning all Url + Visit entities (use existing confirm-close modal pattern from `docs/specs/2026-04-27-confirm-close-design.md`).

### Query protocol

Bevy ↔ webview uses the existing `BinHostEmitEvent` / `BinJsEmitEventPlugin` rkyv pattern:

```rust
// Webview → Bevy
#[derive(Event, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct HistoryQueryRequest {
    pub query: Option<String>, // None = no search, recency-ordered
    pub offset: u32,
    pub limit: u32,            // always 50 from the UI
    pub request_id: u64,
}

// Bevy → webview
#[derive(Event, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct HistoryQueryResponse {
    pub request_id: u64,
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

pub struct HistoryEntry {
    pub url_entity_bits: u64,  // for delete / open dispatch
    pub url: String,
    pub title: String,
    pub favicon_url: String,
    pub visit_created_at: i64,
    pub visit_count: u32,
    pub last_visited_at: i64,
}
```

Other webview → Bevy events:

- `HistoryDeleteRequest { url_entity_bits: u64 }`
- `HistoryClearAllRequest`
- `HistoryOpenRequest { url: String, in_new_stack: bool }`

### Ranking

- **No query:** order Visit entities by `CreatedAt desc`. One row per Visit. (Same URL can appear multiple times in the timeline — matches Chrome.)
- **With query:** order Url entities by `frecency × match_strength desc`. Show one row per Url with `last_visited_at` as the displayed timestamp.

Frecency:

```text
age_hours = (now - last_visited_at) / 3_600_000
decay     = 1.0 / (1.0 + age_hours / 24.0)
frecency  = (visit_count as f32) * decay
```

Hybrid match (URL + title, case-insensitive):

```text
score = 0
if url.starts_with(query)   → score += 3.0
if title.starts_with(query) → score += 2.0
if url.contains(query)      → score += 1.0
if title.contains(query)    → score += 1.0
```

Final ranking key: `frecency × score` (filtered to rows with `score > 0`).

### Infinite scroll

Dioxus app keeps a `Vec<HistoryEntry>` in signal state. An `IntersectionObserver` at the list bottom triggers `HistoryQueryRequest { offset: entries.len(), limit: 50, ... }`. On `HistoryQueryResponse` with matching `request_id`, append and update `has_more`.

Search input debounced 100ms — on change, reset offset to 0, clear entries, dispatch new request.

### Click → new stack

Clicking a row emits `HistoryOpenRequest { url, in_new_stack: true }`. Bevy handler dispatches a new `AgentCommand::OpenInNewStack { url }` (or reuses an existing pane-spawn path if one fits). The destination is a new stack adjacent to the current stack in the active space.

### Delete UX

Hover row → `[×]` button appears (CSS `:hover` + `opacity`). Click despawns the matching `Url` and cascades despawn its Visits (system iterates `Query<(Entity, &VisitedUrl)>` and despawns matches). No undo.

"Clear all" header button → confirm modal → despawn all `Url` and `Visit` entities. No undo.

## Omnibox Integration (`vmux_command`)

Add a new variant to `CommandBarResultItem` in `crates/vmux_command/src/results.rs`:

```rust
pub enum CommandBarResultItem {
    Terminal { ... },
    Stack { ... },
    Space { ... },
    Command { ... },
    Navigate { ... },
    History {                       // NEW
        url: String,
        title: String,
        favicon_url: String,
        visit_count: u32,
        last_visited_at: i64,
    },
}
```

Extend `filter_results()` (currently `results.rs:100-209`) to query history when the input is non-empty and does not start with a command/path/space prefix. History rows are inserted between `Stack` and `Command` sections, capped at **5**, ranked by the same hybrid frecency formula as the history page.

Query path: the command bar's existing webview-to-host pattern. Add `HistorySuggestionsRequest { query: String, limit: u32 }` and `HistorySuggestionsResponse { entries }`. The webview fetches suggestions on input change (debounce reuses existing 100ms path-completion timing).

Enter on a `History` row reuses the existing `"navigate"` action: same code path as typing the URL manually. No new dispatch logic needed.

Rendering: globe favicon + title (primary) + URL (muted secondary). Matches the existing `Navigate` row aesthetic.

## MCP Tools (`vmux_mcp`)

Add three variants to `McpParamTool` in `crates/vmux_mcp/src/tools.rs`:

```rust
#[mcp(description = "Navigate the active or specified browser pane back one page in history.")]
BrowserGoBack {
    pane: Option<PaneRef>,
},
#[mcp(description = "Navigate the active or specified browser pane forward one page in history.")]
BrowserGoForward {
    pane: Option<PaneRef>,
},
#[mcp(description = "Search vmux browsing history. Returns up to `limit` entries ranked by frecency.")]
BrowserHistorySearch {
    query: String,
    limit: Option<u32>, // default 20, max 100
},
```

`to_agent_command()` mappings:

```rust
McpParamTool::BrowserGoBack { pane } =>
    Ok(AgentCommand::BrowserGoBack { pane }),
McpParamTool::BrowserGoForward { pane } =>
    Ok(AgentCommand::BrowserGoForward { pane }),
McpParamTool::BrowserHistorySearch { query, limit } => {
    if query.trim().is_empty() {
        return Err("browser_history_search.query is empty".into());
    }
    let limit = limit.unwrap_or(20).min(100);
    Ok(AgentCommand::BrowserHistorySearch { query, limit })
}
```

New `AgentCommand` variants:

- `BrowserGoBack { pane: Option<PaneRef> }` — dispatches `BrowserCommand::PrevPage` to the resolved pane (skip if pane is a terminal, mirroring `browser.rs:921`).
- `BrowserGoForward { pane: Option<PaneRef> }` — dispatches `BrowserCommand::NextPage`.
- `BrowserHistorySearch { query: String, limit: u32 }` — runs the same ranking system as the history page, returns a JSON array of `{ url, title, favicon_url, visit_count, last_visited_at }` to the MCP caller.

## Crate Boundaries

- **`vmux_core`** — new components only.
- **`patches/bevy_cef_core-0.5.2`** — transition-type exposure + `WebviewCommittedNavigationEvent`.
- **`vmux_history`** — visit-spawning, prune, history-page Dioxus app, query protocol, open/delete handlers. The crate balloons; split `plugin.rs` into `plugin.rs` (Bevy systems) + `query.rs` (ranking helpers) + `app.rs` (webview).
- **`vmux_command`** — new `CommandBarResultItem::History` + history suggestions request/response.
- **`vmux_mcp`** — new tool variants + `AgentCommand` mappings.
- **`vmux_desktop`** — extend `persistence.rs` allowlist, add `AgentCommand` arms for new variants.

## Testing

Per-crate unit tests:

- **`vmux_history`** — ranking math (`frecency × match_strength` ordering), transition mapping table, prune cutoff math. CEF event → spawn pipeline tested via fake `WebviewCommittedNavigationEvent` writer.
- **`vmux_command`** — `filter_results` includes history rows; cap at 5; section ordering.
- **`vmux_mcp`** — `to_agent_command` arms; empty-query rejection; `limit` clamping.
- **`vmux_desktop`** — allowlist round-trip: spawn Url + Visit, save, load, verify components survive.

Manual smoke checks (UI is not unit-testable):

- Navigate through 5 URLs, open `vmux://history`, see all 5 in correct day group.
- Type partial URL in command bar, see history rows ranked by frecency, Enter opens.
- Press `Cmd+[`/`Cmd+]` — confirms back/forward does NOT create new Visit rows.
- MCP: invoke `browser_go_back`, `browser_history_search` from a connected agent.

## Risks

1. **CEF event timing.** `OnLoadStart` fires after commit but before page content arrives, so title/favicon are not yet known at spawn time. The spawn-then-update model handles this: `Url.PageMetadata` starts URL-only and is refreshed by later `title_change` / `favicon_change` events. The history page must render gracefully when title is empty (fall back to URL).
2. **URL lookup cost.** Linear scan over Url entities is O(N). 90-day bound and typical use should keep N < 10k. If needed, add `UrlIndex` resource.
3. **Webview ↔ Bevy round-trip latency.** Infinite scroll and search both depend on it. The existing path-completion uses the same pattern with no reports of lag; expect similar perf.
4. **Search ranking quality.** The simple hybrid formula is not as good as Chrome's. Acceptable for MVP; tunable later.

## Open Questions

None blocking. The following are explicit deferrals, not unknowns:

- Cross-space global history (item 3) — deferred.
- Per-tab CEF nav stack persistence (item 2) — deferred.
- Linear ticket(s) and PR-split strategy — to be decided at start of implementation.
