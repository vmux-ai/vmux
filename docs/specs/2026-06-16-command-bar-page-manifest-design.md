# Command Bar Page Manifest — Design

Date: 2026-06-16

## Problem

Command-bar page suggestions are hardcoded. Only **Spaces** and **Settings** show up, via:

- Duplicated URL constants (`SPACES_PAGE_URL`, `SETTINGS_PAGE_URL`) in
  `vmux_layout/src/command_bar/results.rs`, copied from the source-of-truth
  consts in `vmux_core`/`vmux_setting`.
- Bespoke string-match helpers (`space_query`, `settings_query`,
  `spaces_page_matches`, `settings_page_matches`) in `filter_results`.
- Per-page special-case rendering in the WASM command bar (`command_bar/page.rs`),
  matching `url == SPACES_PAGE_URL` / `== SETTINGS_PAGE_URL`.

Other internal pages (terminal, history, services, agent) are never suggested as
pages. Adding a page means editing the command bar in several places.

## Goals

1. Kill the hardcoded per-page special-casing — one declarative source per page.
2. Surface all user-facing pages: Settings, Spaces, History, Services, Agent,
   Terminal. (`layout`, `command-bar` stay hidden — infrastructure.)
3. Per-page opt-in: each page declares whether/how it appears in the command bar.
4. Consistent rendering: every page row is `icon + title + url`, driven by data.

Non-goals: changing how pages are opened/navigated, changing the spaces
sub-list expansion behavior, redesigning the icon system.

## Approach

Enrich the existing `PageManifest` (already declared as a `PAGE_MANIFEST` const
per crate and spawned by each plugin) with display metadata. Collect the
command-bar pages into one snapshot, ship them in `CommandBarOpenEvent`, and
match/render them generically.

Rejected alternatives:

- **Separate `PageCard` component** — second const + spawn per crate, two things
  to keep in sync.
- **Rename live `PageMetadata` → `PageState`** to free the name — churns
  `handler.rs`, `vmux_header`, save/load; the static descriptor is really the
  manifest, so extend that instead.

## Design

### 1. `PageManifest` (vmux_core/src/page.rs)

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageManifest {
    pub host: &'static str,
    pub title: &'static str,
    pub keywords: &'static [&'static str],
    pub icon: &'static str,   // id mapped to an SVG in the WASM command bar
    pub command_bar: bool,    // opt-in to command-bar suggestions
}

impl PageManifest {
    pub fn url(&self) -> String {
        let host = self.host.trim().trim_matches('/');
        format!("vmux://{host}/")
    }
    // existing embedded_host / bundle_root unchanged
}
```

Stays `Copy + Eq` (`&'static [&'static str]` is both). Every `PAGE_MANIFEST`
const and the two test literals in `page.rs` are updated to the new shape.

Per-page values:

| host        | title    | icon      | keywords                      | command_bar |
|-------------|----------|-----------|-------------------------------|-------------|
| settings    | Settings | settings  | preferences, config           | true        |
| spaces      | Spaces   | layers    | space                         | true        |
| history     | History  | clock     | recent, visited               | true        |
| services    | Services | activity  | processes, monitor            | true        |
| agent       | Agent    | sparkles  | ai, chat, assistant           | true        |
| terminal    | Terminal | terminal  | shell, console                | true        |
| layout      | Layout   | (n/a)     | —                             | false       |
| command-bar | —        | (n/a)     | —                             | false       |

(Keyword for spaces is "space", never "workspace" — project terminology.)

### 2. Transport (vmux_command)

`event.rs` — new wire type + field:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct CommandBarPage {
    pub url: String,
    pub title: String,
    pub keywords: Vec<String>,
    pub icon: String,
}

// in CommandBarOpenEvent:
#[serde(default)]
pub pages: Vec<CommandBarPage>,
```

`snapshot.rs` — new resource reusing the wire type:

```rust
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarPagesSnapshot {
    pub pages: Vec<CommandBarPage>,
}
```

Collector system (lives in `vmux_command`, registered in `CommandPlugin` under
`WriteCommandBarSnapshots`; `vmux_command` already depends on `vmux_core`):

```rust
fn update_pages_snapshot(
    manifests: Query<&PageManifest>,
    mut snap: ResMut<CommandBarPagesSnapshot>,
) {
    if !snap.pages.is_empty() { return; } // manifests are static, spawned at build
    snap.pages = manifests
        .iter()
        .filter(|m| m.command_bar)
        .map(|m| CommandBarPage {
            url: m.url(),
            title: m.title.to_string(),
            keywords: m.keywords.iter().map(|k| k.to_string()).collect(),
            icon: m.icon.to_string(),
        })
        .collect();
}
```

`CommandPlugin` gains `init_resource::<CommandBarPagesSnapshot>()` and the
system. No per-crate page-url updaters.

### 3. Handler (vmux_layout/src/command_bar/handler.rs)

`handle_open_command_bar` already runs near the system-param ceiling, so read
the new snapshot through the existing `snapshot_params` `ParamSet` (add a member)
rather than a new top-level param. Clone `pages` early and pass them into
`command_bar_open_payload`, which gains a `pages: Vec<CommandBarPage>` argument
and sets `CommandBarOpenEvent.pages`.

### 4. Generic matching (results.rs)

- **Delete** the duplicated `SPACES_PAGE_URL` / `SETTINGS_PAGE_URL` consts and
  the `space_query` / `settings_query` / `spaces_page_matches` /
  `settings_page_matches` helpers.
- New result variant:

  ```rust
  Page { url: String, title: String, icon: String },
  ```

- `filter_results` gains `pages: &[CommandBarPage]`. A page matches when the
  search is empty *for the typed-page case* or when `title` / any keyword / `url`
  contains the (lowercased) search. Matching pages become `ResultItem::Page`.
- **Parity preserved:**
  - Empty query (plain Cmd+K) is unchanged: search + new-tab terminal + tabs +
    commands. No page rows. (Pages appear once the user types.)
  - Typing `vmux://spaces` / `vmux://spaces/` still lists the Spaces page entry
    **plus** the full per-space list. Only the spaces page expands to sub-items;
    this stays a small spaces-specific branch keyed off the spaces host. All
    other pages render as a single row.

### 5. Rendering (vmux_layout/src/command_bar/page.rs, WASM)

- Handle `ResultItem::Page` in: `execute` (emit `open` with the current
  `open_target`), `nav_mode` display text (title), the input-row icon detection
  (page → page icon), and the results list row.
- Row layout: `page_icon(icon)` + title (primary) + url (secondary) + trailing
  "New tab" badge — matching today's Spaces/Settings rows.
- New `fn page_icon(icon: &str) -> Element`: a `match` returning inline lucide
  SVG paths for `settings`, `layers`, `clock`, `activity`, `sparkles`,
  `terminal`; default is a generic file/page glyph. Follows the existing
  inline-SVG style already used in this file.
- Remove the `url == SPACES_PAGE_URL` / `== SETTINGS_PAGE_URL` branches (and the
  now-unused `SETTINGS_PAGE_URL` / `SPACES_PAGE_URL` imports) from the
  `Navigate` arm.

### 6. Cleanup

- Remove `CommandBarSettingsSnapshot`, its `init_resource`, and
  `vmux_setting/src/snapshot_updater.rs` — it carried only `settings_page_url`,
  which becomes redundant. (`on_command_bar_action`'s `resource_params.p1()`
  appears unused; confirm during implementation before deleting. If it turns out
  to be used, keep the resource and only drop the URL field.)
- Keep `CommandBarSpacesSnapshot` and `CommandBarTerminalsSnapshot` — they carry
  other data (space list / active ids, terminal pid maps). `spaces_page_url`
  stays as the prefill source for the Space::Open command.

## Testing

- **vmux_core:** `PageManifest::url()` derivation; a manifest carries
  `command_bar` / `icon` / `keywords`.
- **vmux_command snapshot:** `update_pages_snapshot` includes only
  `command_bar == true` manifests and excludes `layout` / `command-bar`.
- **vmux_command event:** `CommandBarOpenEvent` with `pages` survives an rkyv
  round-trip.
- **results.rs:** page matches by title, by keyword, and by url substring;
  Settings reachable by name; Spaces typed-url still expands the space list;
  empty query shows no page rows.
- **handler:** `command_bar_open_payload` propagates `pages`.

## Files touched

- `crates/vmux_core/src/page.rs` — extend `PageManifest`, add `url()`, update tests.
- `crates/vmux_terminal/src/lib.rs`, `vmux_setting/src/lib.rs`,
  `vmux_space/src/lib.rs`, `vmux_history/src/lib.rs`, `vmux_service/src/lib.rs`,
  `vmux_vibe_setup/src/lib.rs`, `vmux_layout/src/lib.rs` — enrich `PAGE_MANIFEST`
  consts (`layout` + `command-bar` set `command_bar: false`).
- `crates/vmux_command/src/event.rs` — `CommandBarPage` + `pages` field.
- `crates/vmux_command/src/snapshot.rs` — `CommandBarPagesSnapshot`; remove
  `CommandBarSettingsSnapshot`.
- `crates/vmux_command/src/plugin.rs` — init + register `update_pages_snapshot`;
  drop settings snapshot init.
- `crates/vmux_layout/src/command_bar/handler.rs` — read snapshot, pass `pages`.
- `crates/vmux_layout/src/command_bar/results.rs` — generic page matching;
  delete dup consts/helpers; add `Page` variant.
- `crates/vmux_layout/src/command_bar/page.rs` — render `Page`, `page_icon`,
  remove special-cases.
- `crates/vmux_setting/src/snapshot_updater.rs` — delete; unregister in
  `vmux_setting/src/plugin.rs`.
