# Auto Type-Stack Spiral Placement

Date: 2026-06-23
Status: Approved (design)

## Problem

When an agent opens a page via MCP (`open_page` / `open_file` / `run`), placement
is manual: the caller must pass a `direction`, and pages of the same kind scatter
across panes. There is no automatic grouping and no automatic split geometry.

We want agent-opened pages to self-organize:

- Pages group into **one stack per page type** (file, browser, terminal, agent).
- A new type-stack claims space by **splitting the most-recently-created leaf
  along its longer side**, producing a Fibonacci/spiral layout (v, h, v, h, ...).
- Already-open pages are reused instead of duplicated.
- The agent pane stays the priority panel and is never split by this flow
  (preserves PR #123, `fix/open-beside-placement`).

## Goals

- Deterministic, automatic placement for MCP opens with no `direction` given.
- Bounded pane count: spiral depth <= number of distinct types (<= 4).
- Reuse-by-URL across the whole space.
- Preserve the agent pane as the protected priority panel.

## Non-Goals

- No change to `update_layout` (remains the full manual escape hatch).
- No change to in-app (command-bar / keyboard) open paths beyond what falls out
  of a shared resolver.
- No URL normalization for reuse (exact string match only).
- No per-agent / per-space ownership model — placement keys purely on type.

## Locked Decisions

| Question | Decision |
| --- | --- |
| Grouping scope | No ownership scope. Pages key purely on type; same type -> same stack. Spiral grows in the current tab's layout. |
| Split anchor | Most-recently-created leaf (creation order, not activation). |
| Type buckets | Core 4: file, browser, terminal, agent. Agent split out of browser. Internal pages (spaces/services/debug) fold into browser. |
| MCP API | Auto by default; explicit `direction` overrides (current path). |
| Reuse match | Exact URL string, all types. |
| Reuse range | Whole space. Cross-tab hit switches tab + focuses. |
| Agent pane | Protected (PR #123). Never a split anchor. Agent pages always stack into it. Spiral runs only on non-agent leaves. Agent pane split at most once, to birth the non-agent region, and keeps the priority share. |

## Spiral Illustration (protected agent)

```
agent running        opens file           opens URL (new type)    runs terminal (new type)
+-----------+        +------+----+         +------+------+         +------+------------+
|           |        |      |    |         |      | FILE |         |      |   FILE     |
|   AGENT   |   ->   |AGENT |FILE|   ->    |AGENT +------+   ->    |AGENT +-----+------+
|           |        |      |    |         |      |BROWSE|         |      |BROWSE| TERM |
+-----------+        +------+----+         +------+------+         +------+-----+------+
 priority pane        split agent ONCE      split FILE (tall        split BROWSE (wide
 (agent stack)        (only b/c no non-     -> Column / horiz)      -> Row / vert)
                      agent leaf exists)
```

After all four types exist, every later open adds a tab to the matching stack
(another file -> FILE, another agent -> AGENT). The agent pane is never split
again.

## Architecture

Approach A (centralized resolver, minimal new state). The ECS entity tree remains
the single source of truth. Type-stack lookup and exact-URL reuse are derived by
querying `PageMetadata` across the space. The only new persisted state is a
per-leaf creation-order stamp, which is not otherwise derivable.

### 1. Page-type classifier — `PageKind`

Replace the stringly `stack_kind_for_url` (`crates/vmux_layout/src/snapshot.rs:134`)
with an enum:

```rust
pub enum PageKind { Agent, Terminal, File, Browser }
```

Mapping:

- `vmux://agent/` -> `Agent` (new; today this collapses into browser)
- `vmux://terminal/` -> `Terminal`
- `file:` -> `File`
- everything else (incl. `vmux://spaces|services|debug`) -> `Browser`

The protocol boundary keeps the existing kind strings (`"terminal"` / `"files"` /
`"browser"`) and adds a new `"agent"` string emitted for `vmux://agent/` stacks
(today these report `"browser"`). Consumers that switch on kind treat unknown
strings as browser-like: the reconciler (`reconcile.rs`) dispatches
`LayoutSpawnRequest::Terminal` only for `"terminal"` and falls through to
`PageOpenRequest` otherwise, which is correct for agent URLs, so adding `"agent"`
is non-breaking.

### 2. Placement resolver (core)

`resolve_placement(space, current_tab, url) -> Placement`, evaluated in order:

1. **Reuse.** Exact-URL match in any stack across the space ->
   `Focus { tab, stack }`. Switches tab if the hit is cross-tab.
2. **Type-stack hit.** A stack of `PageKind(url)` exists in the current tab ->
   `AddTab { stack }`. New page becomes the active tab unless `focus: false`.
3. **New type.** -> `Spiral` (see section 3).

`PageKind::Agent` special-cases steps 2/3. Agent pages always live in exactly one
pane, the agent pane, and join it as tabs. They never split an existing agent pane
and never grow the spiral. If no agent pane exists yet:

- no leaves at all -> the agent page is the root leaf (root becomes the agent pane);
- non-agent leaves exist -> split the most-recently-created non-agent leaf once
  (longer side) to birth the agent pane, which then becomes the protected priority
  pane.

This is symmetric with the non-agent bootstrap (see section 3): each region may
split the other exactly once to come into existence; thereafter the agent pane is
protected and the non-agent spiral only touches non-agent leaves.

```
Placement =
  | Focus  { tab, stack }
  | AddTab { stack }
  | Spiral { anchor, axis }
```

### 3. Spiral mechanics

- **Anchor** = the leaf with the max `SpawnSeq` among **non-agent** leaves in the
  current tab.
- If no non-agent leaf exists -> split the agent pane **once** along its longer
  side; the agent pane retains the priority `flex_grow` share.
- **Axis** from anchor `ComputedNode.size`: `w >= h` -> `Row` (vertical divider);
  else `Column`. Square ties -> `Row`.
- Reuse `split_leaf_into_two` / `split_or_extend`
  (`crates/vmux_layout/src/pane.rs:820` / `:856`): the anchor keeps its existing
  stack; the new leaf hosts the new type-stack and is stamped with the next
  `SpawnSeq`.

### 4. Reuse semantics

- Exact URL string match (no normalization).
- Whole-space search over every tab's stacks (`PageMetadata.url`,
  `crates/vmux_core/src/lib.rs:67`).
- Cross-tab hit -> activate that tab, activate the stack, focus it.
- Honors the existing `focus` parameter.

### 5. MCP API surface

`open_page` / `open_file` / `run` (`crates/vmux_mcp/src/tools.rs:403` dispatch):

- `direction` becomes **optional**.
  - Present -> current explicit split path (override; unchanged behavior).
  - Absent -> resolver.
- `run`'s `PlacementMode::Auto` -> resolver. `Split` / `Stack` unchanged.
- `update_layout` untouched.

### 6. New state and change sites

- **`SpawnSeq(u64)`** component (`#[require(Save)]`) in `pane.rs`, backed by a
  monotonic `Resource` counter. The counter is reseeded from the max existing
  `SpawnSeq` on load (after `moonshine_save` restore) so the spiral resumes
  correctly across restarts. Stamp it in `leaf_pane_bundle` and at both split
  primitives' new-leaf creation.
- **Resolver** in a new `crates/vmux_layout/src/placement.rs`.
- **Wiring**:
  - `handle_open_beside_requests` (`pane.rs:889`) — `open_page` / `open_file`.
  - `handle_agent_self_commands` (`plugin.rs:671`) — `run`.
  - tools dispatch (`tools.rs:403`) — make `direction` optional, route to
    resolver when absent.

## Edge Cases

- **Empty tab, first open.** No leaves -> the first page occupies the root leaf as
  its type-stack (no split). If it is an agent page, that root is the agent pane.
- **Only the agent pane exists, non-agent page opens.** Unavoidable single split
  of the agent pane; agent retains priority share. Subsequent non-agent opens
  spiral within the non-agent region only.
- **Agent page opens with non-agent leaves present but no agent pane.** Split the
  most-recently-created non-agent leaf once (longer side) to birth the protected
  agent pane; thereafter agent pages tab into it.
- **Type-stack was closed, reopened later.** Treated as a new type again -> spirals
  the current most-recently-created non-agent leaf. Self-heals via tree query.
- **Square anchor.** Ties resolve to `Row`.
- **Reuse hit while page is in a background tab.** Switch to that tab and focus.

## Testing

Bevy system + message tests per AGENTS.md (register message types and systems,
send typed messages, run schedules, assert ECS state/messages):

- Spiral axis alternation v / h / v / h across four sequential new types.
- Agent pane is never the split anchor when a non-agent leaf exists.
- Agent pane is split exactly once when it is the only pane.
- Exact-URL reuse, same-tab and cross-tab (asserts tab switch + focus).
- Same-type open adds a tab, does not split.
- Explicit `direction` overrides the resolver.
- Four-types-then-tabs cap: fifth+ opens add tabs, no further splits.

Run native `cargo test -p vmux_layout` (source-scrape / include_str! tests only
trip under the native runner).
