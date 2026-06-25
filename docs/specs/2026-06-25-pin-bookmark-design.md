# Pins + Bookmarks (Dia-style)

Date: 2026-06-25
Status: Approved (design)

## Problem

vmux has no way to save pages for later. Dia/Arc-style browsers keep a left-sidebar
**pin grid** (favicon-only quick access) above a list of **bookmarks** (page entries,
optionally grouped into collapsible folders). We want the same: save the current page
from the command bar, manage saved pages via MCP, and render pins + bookmarks in the
left layout chrome above the open tabs.

Reference (Dia): pin grid on top → a collapsible folder ("Pull Requests") whose
entries are favicon + title + subtitle → divider → open tabs → New Tab.

## Goals

- **Pin**: favicon-only quick-access entry shown in a grid at the top of the left chrome.
- **Bookmark**: a saved page (favicon + title + host subtitle), either loose at the top
  level or nested inside a collapsible **Folder**.
- **Command-bar action**: a bookmark/label SVG icon at the right edge of the command-bar
  input. Click saves the current page as a bookmark; filled when the current URL is
  already bookmarked.
- **Keyboard**: `Cmd+D` toggles the bookmark for the **active** tab's page — adds the
  entry above the divider if absent, removes it if present (entry-centric: the live tab
  stays below the divider; the bookmark entry above appears/disappears). Keyboard twin of
  the command-bar icon. User-rebindable.
- **Pin gesture**: right-click a bookmark entry → Pin (promote into the grid); right-click
  a **tab** in the header/side sheet → Pin or Bookmark.
- **MCP**: list / add / remove / pin / unpin / create-folder.
- **Scope**: per **profile** (one store per `VMUX_PROFILE`, shared across spaces).
- **Composition-first ECS**: pins / bookmarks / folders are entities composed from small
  shared components; markers layer onto shared data. Source of truth is the ECS world,
  exactly like tabs.

## Non-goals (v1)

- Bookmark search / dedupe-on-add beyond pin URL dedupe.
- Drag-and-drop reordering in the UI (MCP `Move` exists; UI DnD is a follow-up).
- Cross-device sync. Import/export. Nested folders inside folders (one folder level).
- Editing a bookmark's title/url inline (rename applies to folders only in v1).

## Composition model

Reuse existing components; add only tiny markers + one id. Markers compose freely onto
the same entity (a pinned bookmark carries both `Pin` and `Bookmark`).

**Add** (`vmux_core`, host-only `#[cfg(not(target_arch = "wasm32"))]`, reflected,
`#[reflect(Component)]`, `#[type_path = "vmux_core"]`):

- `Pin` — marker.
- `Bookmark` — marker.
- `Folder` — marker.
- `Collapsed` — marker (presence = collapsed).
- `Uuid(String)` — stable id for MCP / context-menu targeting (hyphenated UUID v4).

**Reuse**:

- `PageMetadata { title, url, favicon_url, bg_color }` (`vmux_core`) — the data carrier
  (same struct live tabs/stacks use). Not modified.
- `Order(u32)` (`vmux_core`) — sibling ordering.
- `Name` (bevy) — folder display name.
- `ChildOf` / `Children` (bevy, already reflected + registered in `CorePlugin`) — nesting.

### Entity recipes

| Thing            | Components                                                       |
|------------------|-----------------------------------------------------------------|
| Pin (grid)       | `Pin + Uuid + PageMetadata + Order`                             |
| Bookmark (loose) | `Bookmark + Uuid + PageMetadata + Order`                        |
| Bookmark (nested)| `Bookmark + Uuid + PageMetadata + Order + ChildOf(folder)`     |
| Pinned bookmark  | `Bookmark + Pin + Uuid + PageMetadata + Order` (one entity)     |
| Folder           | `Folder + Uuid + Name + Order` (`+ Collapsed` when collapsed)   |

- **Pin / unpin** = add / remove `Pin`.
- **Collapse / expand** = add / remove `Collapsed`.
- **Nest / move** = set / clear `ChildOf` + adjust `Order`.
- A loose bookmark has no `ChildOf`; a nested bookmark is `ChildOf` its folder.
- Folders do not nest inside folders (v1).

Registration: add `Pin / Bookmark / Folder / Collapsed / Uuid` to the `register_type`
chain in `CorePlugin` (`vmux_core/src/lib.rs`).

## Persistence

Per-profile file `profile_dir()/bookmarks.ron` (same directory as `session.ron`;
`VMUX_PROFILE`-isolated, so test sessions never touch the real store).

- **Save set is scoped by marker query**, not by the moonshine `Save` marker:
  `Or<(With<Pin>, With<Bookmark>, With<Folder>)>`. Bookmark entities deliberately do
  **not** carry moonshine `Save`, so they stay disjoint from the `space.ron` /
  `store.ron` set — and space entities stay out of `bookmarks.ron`.
- Serialize via a `DynamicScene` built from that query with a component allowlist:
  `Pin, Bookmark, Folder, Collapsed, Uuid, Name, Order, PageMetadata, ChildOf, Children`.
  Mirror the extraction/RON-write shape of
  `vmux_desktop/src/persistence.rs::save_space_to_path`.
- **Load** at startup: read RON → `DynamicScene` → spawn into the world (once, before the
  first broadcast).
- **Autosave**: debounced on change (a `BookmarkCommand` was applied), mirroring the
  space-save debounce. No file is written until the first mutation (no empty-store seed).

## Mutations & data flow

Two command channels, matching the codebase's existing split. `AppCommand` sub-enums are
`Copy`/unit (no data payloads) — the keyboard / OS-menu / command-bar surface. Data-carrying
operations ride the rkyv page-event bus and MCP. Both converge on one host op enum applied
by a single system (per AGENTS.md: typed Bevy messages + systems, no ad-hoc helpers).

**Channel 1 — `AppCommand` (contextual, unit, host-side)** in `vmux_command`. New
`AppCommand::Bookmark(BookmarkCommand)` group, `#[mcp(skip)]` (data ops have dedicated MCP
tools), deriving `OsSubMenuGroup, DefaultShortcuts, CommandBar`. Unit variants:

- `ToggleActive` — `#[shortcut(direct = "Super+d")]` (Cmd+D). Toggle bookmark for the
  active tab's page. `Super+d` is currently unbound.
- `PinActive` — pin the active tab (no default shortcut; menu/command-bar).

A small adapter system reads these `AppCommand`s, resolves the **active** `Stack`'s
`PageMetadata`, and emits the matching `BookmarkOp` (below). The command-bar icon and tab
context menu issue the same effect via Channel 2.

**Channel 2 — `BookmarkOp` (data-carrying, host message enum, `vmux_layout`)**:

- `ToggleForUrl { url, title, favicon_url }` — add if no bookmark matches `url`, else remove.
- `Add { url, title, favicon_url, folder: Option<Uuid> }` — default top level.
- `Remove { uuid }`
- `AddFolder { name }`
- `RemoveFolder { uuid }` — children re-parented to top level.
- `RenameFolder { uuid, name }`
- `ToggleFolder { uuid }` — add/remove `Collapsed`.
- `Pin { uuid }` / `PinUrl { url, title, favicon_url }` (ad-hoc, e.g. a tab) / `Unpin { uuid }`
- `Move { uuid, folder: Option<Uuid>, before: Option<Uuid> }`

Sources of `BookmarkOp`: the Channel-1 adapter; the page (command-bar icon, tab/entry/folder
context menus) via rkyv `BookmarkCommandEvent` → translated host-side; MCP tool dispatch.

Flow:

```
keyboard / menu ── AppCommand::Bookmark(ToggleActive|PinActive) ──▶ adapter (reads active Stack)
command bar / tab/entry/folder menu (wasm) ──rkyv──▶ BookmarkCommandEvent ──▶ translate
MCP tool ── dispatch ─────────────────────────────────────────────────────────────────┐
                                                                                        ▼
                                                                          BookmarkOp (host message)
                                                                                        ▼
                                                       apply_bookmark_ops system
                                       (spawn/despawn entities, add/remove markers,
                                        set PageMetadata/Order/ChildOf) ── mark dirty
                                                                                        ▼
                                       broadcast_bookmarks system ──rkyv──▶ BookmarkHostEvent
                                                                                        ▼
                                                    left chrome + command-bar pages render
```

- **Snapshot DTO** (wasm-safe, defined like `TabRow`): `BookmarkSnapshot { pins: Vec<PinRow>,
  roots: Vec<BookmarkTreeNode> }` where a node is either a `BookmarkRow { uuid, url, title,
  favicon_url }` or a `FolderRow { uuid, name, collapsed, children: Vec<BookmarkRow> }`.
  Derived from ECS each broadcast. The host components are `cfg(not(wasm))`; the DTO is
  shared by host and page.
- Page → host commands ride the existing rkyv bin-event bus (`try_cef_bin_emit_rkyv`),
  translated to `BookmarkOp` host-side — same pattern as `TabsCommandEvent` → tab commands.
- `bookmark_list` MCP is an `AgentQuery` that derives the snapshot from the ECS world and
  returns it in a single response (vibe MCP client requires one-shot results).

## UI

### Command bar (`vmux_layout/src/command_bar/page.rs`)

- Add a bookmark/label SVG icon button at the right edge of the input row (after the
  input-wrap `div`, ~line 386). Label/ribbon glyph (e.g. `M19 21l-7-5-7 5V5a2 2 0 0 1
  2-2h10a2 2 0 0 1 2 2z`), `Icon` from `vmux_ui`.
- Subscribe to `BookmarkHostEvent`; render **filled** when the current URL matches a
  bookmark, outline otherwise.
- On click: emit `BookmarkCommandEvent::ToggleForUrl` for the current page's
  url/title/favicon. `Cmd+D` (`AppCommand::Bookmark(ToggleActive)`) is the keyboard twin —
  same toggle on the active tab.

### Left chrome (`vmux_layout/src/page.rs`)

Insert a pins+bookmarks section **above** the tab list:

1. **Pin grid** — rounded favicon squares, 3 per row (reuse `Favicon` /
   `vmux_ui::favicon`). Click opens the URL.
2. **Bookmark roots** — folders (collapsible: chevron + folder icon + `Name`) and loose
   bookmark rows (favicon + title + host subtitle). Folder children render when not
   `Collapsed`.
3. Divider, then the existing tab list, then New Tab (unchanged).

### Context menus (`vmux_ui::ContextMenu`, not yet used on tabs)

- **Pin square**: Open · Unpin.
- **Bookmark row**: Open · Pin · Remove · Move to folder ▸.
- **Folder header**: Rename · New bookmark · Remove · Collapse/Expand.
- **Tab** (wrap `fn Tab`, `page.rs:404`): existing Close + **Pin** + **Bookmark** (emit
  `BookmarkCommandEvent` from the tab's url/title/favicon, available client-side).

## MCP tools (`vmux_mcp/src/tools.rs`)

Map to `BookmarkOp`; `bookmark_list` is a query.

- `bookmark_list` → snapshot (pins + tree), one response.
- `bookmark_add { url, title?, favicon_url?, folder? }`
- `bookmark_remove { uuid }`
- `bookmark_pin { uuid? | url, title?, favicon_url? }`
- `bookmark_unpin { uuid }`
- `bookmark_folder_create { name }`

(`folder_remove` / `folder_rename` / `move` are optional follow-ups.)

## File placement (no new crate)

- **Components + registration** → `vmux_core/src/lib.rs` (`Pin, Bookmark, Folder,
  Collapsed, Uuid`; `CorePlugin` register chain).
- **`AppCommand::Bookmark(BookmarkCommand)` unit enum + `Super+d` default shortcut** →
  `vmux_command/src/command.rs` (derive `DefaultShortcuts` etc., `#[shortcut(direct =
  "Super+d")]` on `ToggleActive`).
- **`BookmarkOp` host messages, Channel-1 adapter, `BookmarkCommandEvent` /
  `BookmarkHostEvent`, snapshot DTO, apply + broadcast systems, plugin** → `vmux_layout`
  (new module `bookmark.rs` + `bookmark/` per the no-`mod.rs` rule).
- **UI** → `vmux_layout/src/command_bar/page.rs` + `vmux_layout/src/page.rs`.
- **Persistence (save/load scene)** → `vmux_desktop/src/persistence.rs` (sibling of the
  space pipeline) + `profile_dir()/bookmarks.ron` via `vmux_core::profile`.
- **MCP tools** → `vmux_mcp/src/tools.rs`.

`vmux_layout` is already tracked by `vmux_server/build.rs`, so the wasm page rebuilds —
no `build.rs` change.

## Testing

- **Components**: reflection registration test (extend the `CorePlugin` test in
  `vmux_core`).
- **Mutations** (ECS-first, message + system per AGENTS.md): build an `App`, send each
  `BookmarkOp`, run the schedule, assert ECS state (entity counts, markers, `ChildOf`,
  `Order`) and the emitted `BookmarkHostEvent` snapshot. Cover: add bookmark (top-level +
  into folder), remove, add/remove/rename/toggle folder, pin/unpin (marker add/remove on
  an existing bookmark and ad-hoc), move, remove-folder re-parents children,
  `ToggleForUrl` add-then-remove idempotence.
- **Shortcut**: `AppCommand::default_shortcuts()` includes `Super+d` → `ToggleActive`
  (extend the `command.rs` default-shortcuts tests); the Channel-1 adapter, given an
  active `Stack` with `PageMetadata`, emits the matching `BookmarkOp` and toggles the
  bookmark entity.
- **Snapshot DTO**: serde round-trip; derives correct tree from a seeded world.
- **Persistence**: save → load round-trip rebuilds the same entities; disjointness —
  saving `space.ron` does not include bookmark entities and vice-versa.
- **MCP**: `mcp_smoke` covers tool definitions present, dispatch maps to commands, and
  `bookmark_list` returns a populated snapshot in one response.
- **Source-scrape note**: if `page.rs` / `command_bar` markup is asserted via
  `include_str!` text tests (`style.rs`, `tests/page_source.rs`), update those — only
  native `cargo test -p vmux_layout` catches them.

## Risks / open questions

- **Persistence disjointness** is the main correctness risk: confirm the space pipeline
  selects entities by the moonshine `Save` marker (so non-`Save` bookmark entities are
  excluded) before mirroring it. If the space pipeline saves by a broader query, scope
  the bookmark scene explicitly by the marker query above.
- **Folder targeting in MCP** is by `Uuid`; `bookmark_add { folder }` may also accept a
  folder name for ergonomics (resolve name → uuid host-side, first match).
- **Bookmark subtitle** = derived host of `PageMetadata.url`; no extra storage.
- One folder level only in v1 (no nested folders); the `ChildOf` model permits deeper
  nesting later without a data change.
