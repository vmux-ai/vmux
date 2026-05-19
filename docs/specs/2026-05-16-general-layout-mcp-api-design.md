# General Layout MCP API

Replace the ~40 auto-generated layout MCP tools (and 5 read-only query tools) with two declarative tools: `read_layout` and `update_layout`. Tree shape, focus, sizing, zoom, and tab title are all expressed as fields on the layout tree; the agent reads, mutates whatever it wants, and submits the new tree back. Vmux reconciles by id (React-style).

## Motivation

Today every `LayoutCommand` variant (`PaneCommand::SplitV`, `TabCommand::SelectIndex5`, `StackCommand::SwapPrev`, etc.) becomes its own MCP tool via the `McpTool` derive. That puts ~40 narrow, per-action tools in front of an agent on top of 5 read-only query tools (`get_state`, `list_tabs`, `list_spaces`, `list_terminals`, `get_focused`).

Two problems:

1. **Discoverability is poor.** An agent has to learn ~40 names to do anything with the layout.
2. **No tree primitives.** The current tools are "fire this exact UI action against the focused thing." Composite operations (move a tab into a specific pane, swap two arbitrary panes, resize by ratio, equalize a non-root split) are not expressible — agents can only emulate them through brittle sequences of `focus_*` + per-action tools, each of which mutates focus as a side effect.

The goal is to expose a tiny, tree-shaped read/update API that an LLM agent can use without memorizing a vocabulary of imperative ops.

## Scope

In scope (MCP layer only):

- New tool `read_layout` — returns the full layout (spaces + recursive pane tree + focused).
- New tool `update_layout` — accepts the full layout tree; vmux diffs it against current state and reconciles. Tree shape, focus, sizing, zoom, and tab title are all properties on the tree.
- Strip `McpTool` derive from `LayoutCommand`, `PaneCommand`, `TabCommand`, `StackCommand`, `WindowCommand`, `ZenCommand`, `SpaceCommand` so the per-action MCP tools disappear from the agent's surface.
- Delete the `McpQueryTool` enum (`get_state`, `list_tabs`, `list_spaces`, `list_terminals`, `get_focused`) — all subsumed by `read_layout` (terminals appear as tabs with `kind: "terminal"`).

Explicitly out of scope:

- Keyboard shortcuts, OS menus, and the in-app command bar continue to work unchanged. They are driven by the `DefaultShortcuts` / `OsMenu` / `OsSubMenu` / `CommandBar` derives, which are not touched.
- Other MCP tools (`open_command_bar`, `new_terminal_tab`, `run_shell`, `browser_navigate`, `terminal_send`, `select_tab`, `split_and_navigate`) stay as-is. `select_tab` and `split_and_navigate` overlap with the new tools and could be deprecated in a follow-up.
- `SceneCommand` and any other non-layout `AppCommand` variants keep their `McpTool` derive — only the layout subtree is stripped.
- Stale-write detection (version token / CAS). Deferred until it bites; concurrent layout edits between read and write are not protected against in v1.

## Identifier Scheme

Every addressable node gets a self-describing string id of the form `kind:value`:

- `space:<string>` — e.g. `space:work`
- `pane:<u64>` — e.g. `pane:42`
- `split:<u64>` — e.g. `split:17`
- `tab:<string>` — e.g. `tab:abc123`

Self-describing because (a) the agent can validate without re-reading the schema and (b) reconciliation can pick the right entity table per kind during diff.

Internally vmux uses `u64` for pane/split ids and `String` for tab/space ids. Parsing and formatting the prefixed form happens at the MCP boundary in `vmux_mcp`.

## `read_layout`

Returns the full layout. JSON wire format:

```json
{
  "spaces": [
    {
      "id": "space:work",
      "name": "Work",
      "is_active": true,
      "root": {
        "kind": "split",
        "id": "split:1",
        "direction": "row",
        "flex_weights": [1.0, 1.0],
        "children": [
          {
            "kind": "pane",
            "id": "pane:2",
            "is_zoomed": false,
            "tabs": [
              {
                "id": "tab:abc",
                "title": "Vmux",
                "url": "https://vmux.ai",
                "kind": "browser",
                "is_loading": false,
                "favicon_url": "..."
              },
              {
                "id": "tab:def",
                "title": "zsh",
                "url": "vmux://terminal/?cwd=/Users/foo",
                "kind": "terminal"
              }
            ]
          },
          { "kind": "pane", "id": "pane:3", "tabs": [] }
        ]
      }
    }
  ],
  "focused": {
    "space": "space:work",
    "pane": "pane:2",
    "tab": "tab:abc"
  }
}
```

Notes:

- `root` is recursive: either a `split` (with `direction`, `flex_weights`, `children`) or a `pane` (leaf, with `tabs`).
- Pane carries `is_zoomed` so zoom state remains observable after the per-action `zoom_pane` MCP tool is removed.
- Tabs carry a `kind` discriminator (`browser` | `terminal` | other). Terminals are no longer a separate top-level concept — they are tabs whose `kind` is `terminal`. This is why `list_terminals` can be deleted: an agent that wants the terminal list filters `tabs` for `kind == "terminal"`.
- `flex_weights` is part of the read shape and the write shape; agents change sizing by submitting updated weights through `update_layout`.
- `is_active` is omitted from individual nodes in favor of a single top-level `focused` triple, which is the source of truth for selection. (The triple stays the only mention of focus to avoid two-source-of-truth bugs on writes.)

## `update_layout`

Input is the same JSON shape `read_layout` returns. The agent reads, mutates any combination of fields, and submits the new tree back. Vmux diffs against current state and applies a single atomic update.

```json
// Input schema, abbreviated
{
  "spaces": [ <space>, ... ],
  "focused": { "space": "space:<id>", "pane": "pane:<id>", "tab": "tab:<id>" }
}
```

Returns the resulting tree (same shape as `read_layout`) so the agent learns the ids of any newly-created nodes.

### Reconciliation Rules

Matching is by id. Vmux walks the submitted tree and the current tree in parallel.

| Node in submitted tree | Exists in current tree? | Action |
|---|---|---|
| `id` present | yes | **Match.** Move to the new parent/position if changed. Update mutable props: split `direction` & `flex_weights`; pane `is_zoomed`; tab `title`. |
| `id` present | no | **Error.** Agent referenced a nonexistent id. Reject the whole update. |
| `id` omitted (pane) | n/a | **Create** new pane. Children must include at least one tab (a pane with no tabs is invalid). |
| `id` omitted (tab) | n/a | **Create** new tab. Must include `url` + `kind`. |
| `id` omitted (split) | n/a | **Create** new split. Default `flex_weights` = uniform across `children`. |
| `id` omitted (space) | n/a | **Create** new space. Must include `name` and `root`. |
| In current tree, absent from submitted | n/a | **Close** that node (and its descendants). |

Focus changes when the top-level `focused` triple differs from current. Setting `focused` to a tab id selects that tab; setting only `focused.pane` (with `tab` omitted) selects the pane's active tab; setting only `focused.space` switches space and keeps that space's last-focused pane/tab.

### Atomicity

The whole update is a single transaction:

1. **Validate.** Walk the submitted tree. Reject if any of: duplicate ids; reference to a nonexistent id; new tab without `url`+`kind`; new pane with zero tabs; new space without `name`+`root`; `flex_weights.len()` doesn't match `children.len()`; focus refers to an id not present in the submitted tree.
2. **Apply.** If validation passes, apply all node creations, moves, prop updates, focus changes, and closures in one frame.
3. **Return.** Respond with the resulting tree (which includes ids assigned to newly-created nodes).

If validation fails, return a structured error per the existing `AgentCommandResult::Error(String)` pattern; no mutations applied.

### Examples

**Swap two panes** (just reorder children):

```jsonc
// read_layout returned root.children = [paneA, paneB]
update_layout({
  "spaces": [{ "id": "space:work", "name": "Work", "is_active": true,
    "root": { "kind": "split", "id": "split:1", "direction": "row",
              "flex_weights": [1.0, 1.0],
              "children": [<paneB>, <paneA>] } }],
  "focused": { "space": "space:work", "pane": "pane:A", "tab": "tab:abc" }
})
```

**Split a pane** (replace pane with split-of-two; new pane has new tab):

```jsonc
update_layout({
  "spaces": [{ "id": "space:work", "name": "Work", "is_active": true,
    "root": { "kind": "split", "direction": "row", // no id -> new split
              "children": [
                { "kind": "pane", "id": "pane:2", "tabs": [{ "id": "tab:abc" }] },
                { "kind": "pane", // no id -> new pane
                  "tabs": [{ "url": "https://example.com", "kind": "browser" }] }
              ] } }],
  "focused": { "space": "space:work", "pane": "pane:2", "tab": "tab:abc" }
})
```

**Rename a tab + change focus + resize** (all in one call):

```jsonc
update_layout({
  "spaces": [{ "id": "space:work", "name": "Work", "is_active": true,
    "root": { "kind": "split", "id": "split:1", "direction": "row",
              "flex_weights": [2.0, 1.0],  // was [1.0, 1.0]
              "children": [
                { "kind": "pane", "id": "pane:2",
                  "tabs": [{ "id": "tab:abc", "title": "Renamed" }] },
                { "kind": "pane", "id": "pane:3", "tabs": [...] }
              ] } }],
  "focused": { "space": "space:work", "pane": "pane:3", "tab": "tab:xyz" }
})
```

Note: LayoutNode children inside a split use `"kind": "split" | "pane"` as a node-type discriminator. Tab objects in a `tabs` array are unambiguously tabs (by position), so they have no node-type discriminator; their `kind` field (when present) is the tab's webview kind — `"browser" | "terminal" | ...`.

## Protocol Changes

In `vmux_service/src/protocol.rs`:

- Add `AgentCommand::UpdateLayout { layout: LayoutSnapshot }`.
- Add `AgentQuery::ReadLayout`.
- Add `AgentCommandResult::Layout(LayoutSnapshot)` (or extend `AgentCommandResult` to carry an optional layout payload on success) so `update_layout` can return the resulting tree.
- Add `AgentQueryResult::Layout(LayoutSnapshot)` carrying:
  - `LayoutSnapshot { spaces: Vec<SpaceDto>, focused: FocusDto }`
  - `SpaceDto { id, name, is_active, root: LayoutNodeDto }`
  - `LayoutNodeDto::Split { id: Option<String>, direction, flex_weights, children }` / `Pane { id: Option<String>, is_zoomed, tabs }`
  - `TabDto { id: Option<String>, title, url, kind, is_loading, favicon_url }` (all properties optional on write; required on read)
  - `FocusDto { space: Option<String>, pane: Option<String>, tab: Option<String> }`

  `id` is `Option<String>` so the same type works as both the read response (always populated) and the write payload (omitted to signal create).

Removals from the protocol (after confirming no other consumer with `grep`):

- `AgentQuery::{GetState, ListTabs, ListSpaces, ListTerminals, GetFocused}`
- `AgentQueryResult::{State, Tabs, Spaces, Terminals, Focused}`
- `StateSnapshot`, `FocusedInfo`, `PaneInfo`, `SpaceInfo`, `TabInfo`, `TerminalInfo`

We intentionally do not reuse `vmux_command::event::LayoutNode` (used by the in-process layout webview). That type lives in a different crate, is internal to the renderer, and uses raw `u64` ids without the kind prefix. The protocol type is on-the-wire and stable independent of the renderer.

## Implementation in `vmux_mcp`

In `vmux_mcp/src/tools.rs`:

- Delete `McpQueryTool` enum and its impl.
- Add `read_layout` and `update_layout` as hand-built `ToolDefinition` entries pushed alongside the macro-derived ones. (The `McpTool` derive macro only handles flat enums with `String`/integer/bool/`Option` fields — it cannot emit the recursive object schema we need. Rather than extend the macro, we bypass it for these two tools.)
- The `update_layout` input schema is a hand-written recursive JSON Schema. To keep the schema readable and the agent's introspection cheap, reference each node type by `$ref` from a `$defs` table: `Space`, `LayoutNode` (oneOf Split | Pane), `Tab`.
- Add a `parse_layout(args: Value) -> Result<LayoutSnapshot, String>` helper that deserializes into the protocol type and returns a structured error on shape failures (missing required field, unknown kind, malformed id).
- Update `dispatch_from_tool_call()` to route `read_layout` → `AgentQuery::ReadLayout` and `update_layout` → `AgentCommand::UpdateLayout`.

In `vmux_command/src/command.rs`:

- Remove `McpTool` from the `derive` list of `LayoutCommand`, `PaneCommand`, `TabCommand`, `StackCommand`, `WindowCommand`, `ZenCommand`, `SpaceCommand`. Keep all other derives.
- Update the existing `mcp_lookup_resolves_every_command_id` test — it iterates `mcp_tool_entries()`, so it should still pass; there's just less to iterate. Add an assertion that `from_mcp_id("split_v")` returns `None` (and similar for a representative sample of removed ids).

## Service-side Reconciler (`vmux_desktop`)

The service crate that handles `AgentCommand` / `AgentQuery` (locate exact module during implementation) gains:

- A handler for `AgentQuery::ReadLayout` that walks the ECS pane/stack/tab graph and emits the `LayoutSnapshot`. Reuses existing focus-walk helpers (`FocusedStack`, `collect_leaf_panes`) and the split tree traversal already used by the in-process layout webview.
- A handler for `AgentCommand::UpdateLayout { layout }` that runs the reconciler. The reconciler:
  1. Builds an id-keyed map of all nodes currently in the world (spaces, panes, splits, tabs).
  2. Walks `layout.spaces` recursively, classifying each node as Match / Create / Error.
  3. Computes the set of currently-existing ids not referenced in `layout` — those are Close candidates.
  4. Validates the full plan (duplicate ids, reference errors, missing required fields on creates, focus refers to a present id, `flex_weights.len()` == `children.len()` on splits).
  5. On validation success, applies in this order: Creates → Moves/Prop updates → Closes → Focus. (Closes last so that creates and moves can reference closing nodes' positions; focus last so it lands on settled state.)
  6. Returns the resulting tree by re-running the read walk.

For most operations the reconciler can lean on existing internal primitives:

- Move pane/tab → existing reparenting paths used by `SideSheetDragCommand` (generalize source to "any node by id").
- Split creation/collapse → existing `PaneSplit` machinery.
- Sizing → mutate `PaneSplit::flex_weights` directly.
- Tab create → existing `LayoutSpawnRequest` flow (parametrized by `url` + `kind`).
- Tab close, pane close → existing `*Command::Close` handlers.
- Focus → existing focus-set systems.
- Rename → existing `TabCommand::Rename` plumbing (extend if it doesn't already accept a title arg from the reconciler path).

Where no existing primitive suffices (e.g., closing multiple panes simultaneously while preserving in-flight processes' shutdown order), add the missing helper inside the relevant layout module rather than expanding the command enums (those are for the UI keyboard/menu path).

These additions are layout-internal; they do not change the menu/keyboard surface.

## Testing

Unit tests in `vmux_mcp`:

- `LayoutSnapshot` serde round-trips for representative trees (empty, single pane, deep nest, multiple spaces).
- `update_layout` validation rejects: duplicate ids; reference to nonexistent id; new tab without `url`+`kind`; new pane with empty `tabs`; `flex_weights` length mismatch; focus pointing to absent id.
- `tool_definitions()` includes `read_layout` and `update_layout` and does not include the removed per-action layout tools.

Unit tests in `vmux_command`:

- Update `mcp_lookup_resolves_every_command_id` (still iterates `mcp_tool_entries()`, just shorter).
- Add an assertion that `from_mcp_id("split_v")` returns `None` and similar for a representative sample of removed ids.

Reconciler tests in `vmux_desktop` (or wherever the service handler lives):

- Move a tab between panes → tab id preserved at new location, old position empty.
- Swap two panes → both panes preserved, positions swapped, child tabs untouched.
- Close a pane → pane and its tabs removed; parent split collapses if it had two children and now has one.
- Create a new pane with one new tab inside an existing split → split now has one more child, new ids returned.
- Update `flex_weights` only → no structural change, only sizing.
- Combined update (rename + resize + focus shift) in one call → all applied atomically.
- Validation failure (e.g., reference to nonexistent pane id) → no mutation, error returned.

Update `vmux_cli/tests/mcp_smoke.rs` to drop expectations for the removed tool names and add smoke coverage for `read_layout` + `update_layout` round trip.

Integration test: drive `read_layout → mutate locally → update_layout → read_layout` and assert the second read matches the submitted tree (after id assignment for new nodes).

## Crates Touched

`vmux_command`, `vmux_mcp`, `vmux_service`, `vmux_desktop`, `vmux_cli` (tests). `vmux_macro` is not touched — we bypass the macro for the two new tools rather than extend it to support recursive schemas.

## Migration / Compatibility

Breaking change for any external MCP client that depends on the auto-generated layout tools or the five query tools. The acceptance is that the new surface is small enough that re-targeting is cheap, and the legacy auto-generated tools were never advertised as a stable API.
