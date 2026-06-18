# Agent Layout API Design

## Summary

Give the agent a real, id-addressed layout API by generalizing vmux's existing
command enums with a `target`/`Anchor` parameter — default the focused element,
or a specific `pane:`/`stack:`/`tab:` id — and un-skipping them for MCP. The same
command and the same handler serve the keyboard/menu (Active target) and the
agent (id target): one implementation, no duplicate verbs or messages. Page
placement happens **at spawn time** via the generalized `InPane` command, so the
agent's terminals/pages are born in the right spot instead of popping up
scattered and being rearranged afterward.

## Problem

The agent's only structural layout tool today is `update_layout` (submit a
complete tree, reconciled). Every per-op layout verb a human uses (split, move
stack, equalize, close pane) is `#[mcp(skip)]` on `AppCommand::Layout`
(`command.rs:22`) and is focus-relative + parameterless — `close_pane` closes the
*focused* pane, with no id.

So to arrange anything, the agent must hand-build a complete, correct tree, which
it will not do reliably. Observed: asked to run five parallel commands, the agent
self-split its own pane five times into unreadable slivers, never Z-stacked, never
reused a terminal, and never called `update_layout` to tidy — even with the policy
spelled out in the tool descriptions. Two description-only iterations failed. Root
cause: there are no id-addressed mid-level layout ops, and spawning (`run` /
`open_page`) always splits the agent's own pane.

## Goals

- The agent places each page (terminal or browser) where it wants **at spawn
  time**, so the layout is tidy as pages appear — no flicker-then-rearrange.
- The agent drives layout decisions; no vmux auto-magic or pooling.
- Layout ops are **id-addressed** (`pane:`/`stack:`/`tab:` ids from `read_layout`).
- Keyboard/menu and agent **share one command + one handler** — no duplicated
  logic — via Bevy systems + messages (the existing `AppCommand` message), per the
  project's bevy-way rule.
- Generalizes to the roadmap: the same verbs arrange browser pages (search, docs),
  not just terminals.

## Non-goals

- No vmux-side terminal pool / auto-grouping (rejected: the agent drives).
- No high-level `group_terminals` / `arrange_pages` verb (rejected: spawn-time
  placement replaces post-hoc tidy).
- `update_layout` / `reconcile` stay unchanged (the declarative whole-tree path
  remains the escape hatch).
- No helper-function sharing layer (rejected: share via systems + messages).
- Navigation/selection commands (SelectLeft/Right, rotate, …) are not exposed to
  the agent.

## Terminology

- **Pane** = a spatial tile; **splits** divide a tab on the **X/Y** plane.
- **Stack** = a page (terminal or browser) inside a pane, on the **Z** axis. A
  pane holds several stacks; one is visible, switched via the in-pane stack strip.
- **split** arrangement = N panes (X/Y), one stack each. **stack** arrangement =
  one pane, N stacks (Z).
- **Anchor** (new) = a command's target: `Active` (focused) or a specific id.

## Design

### 1. The `Anchor` target

New type in `vmux_command`:

```rust
enum Anchor { Active, Pane(u64), Stack(u64), Tab(u64) } // default: Active
```

- Keyboard / menu / command-bar construct commands with `Anchor::Active`.
- The agent supplies an id. The MCP wire form is an optional `target` string
  (e.g. `"pane:7"`), parsed via the existing `parse_id`; absent ⇒ `Active`.
- One resolver maps `Anchor` → `Entity`: `Active` → `FocusedStack`; an id → the
  same id↔entity map `read_layout` / `reconcile` already use.

### 2. Generalize the structural layout commands

Add `target: Anchor` to the structural commands the agent needs and remove
`#[mcp(skip)]` from them, so `McpTool` generates id-addressed tools from the same
enum that drives menus and shortcuts:

- `PaneCommand::Close` → close the `target` pane.
- `StackCommand::MoveToPane` → carries a destination-pane `Anchor` (move the
  `target` stack into a pane).
- A new `SetWeights { target: split, weights }` op — absolute weights, which the
  agent can compute, vs. the incremental `Resize*` commands.
- `OpenCommand::InPane` (already fielded with `direction` / `target` / `mode` /
  `url`, `open.rs:54`) → change its `target` from the focus-relative `PaneTarget`
  to an `Anchor` id, and un-skip. This is the **spawn-placement** verb: open a
  page as a `split` (new pane, X/Y) or `stack` (Z) relative to an anchor pane.

Navigation/selection (`SelectLeft/Right/Up/Down`, `Toggle`, rotate) keep
`Anchor::Active` semantics and stay `mcp(skip)` — the agent does not move focus.

### 3. Spawn-time placement (`run` / `open_page`)

- `run` keeps its own tool (it needs blocking + the done-marker + output). It
  gains placement params that map onto the shared split/stack path:
  `beside: <page-id|self>`, `mode: split|stack`, `direction`. The new terminal is
  born at the anchor. `terminal: <id>` reuse stays for sequential steps.
- `open_page` becomes a thin self-relative convenience over the same path
  (`beside: self`); see open question in Risks.
- Spawns return the **landing pane id** (alongside the page/terminal id) so the
  agent can anchor the next page to it (e.g. `beside: <pane>, mode: stack`).

### 4. One handler, target-aware

`handle_pane_commands` (`pane.rs:485`) stops doing focus-relative work inline. It
resolves `target` (Active → focused; id → entity) and performs the op via the
existing helpers (`split_leaf_into_two` / `split_or_extend`, plus the close / move
/ weights bodies and a `stack_into_pane` insert). Both the keyboard
(`AppCommand::Layout { target: Active, … }`) and the agent
(`AppCommand::Layout { target: Id, … }`) flow through it.

The agent path: `dispatch_*` (`tools.rs`) parses the tool + ids and emits
`AppCommand::Layout { target, … }` — i.e. `AgentCommand` routes onto the *same*
`AppCommand` bus the keyboard uses (valid now that the command carries a target).
Spawn placement routes to the InPane/split path.

### 5. Discovery

`read_layout` already returns per-stack `is_self`, `process_id`, `kind`, and
`pane:`/`stack:` ids (`protocol.rs:62-81`) — enough for the agent to find itself
and address targets. (Optional later: an `is_busy` flag for reuse hints.)

## Data flow

```
keyboard/menu ─► AppCommand::Layout{ target: Active, command }
                       │
agent (MCP) ─► tool → AppCommand::Layout{ target: Id, command }
                       ▼
        handle_* system: resolve target → entity → op (shared helpers/messages)
                       ▼
                  ECS layout mutated

spawn: run/open_page → InPane(split|stack, anchor)
                       → split_leaf_into_two / stack_into_pane
```

## Implementation surface

- `vmux_command/src/open.rs` — define `Anchor`; `InPane.target: Anchor`; un-skip
  `InPane`.
- `vmux_command/src/command.rs` — `target: Anchor` on `PaneCommand::Close`,
  `StackCommand::MoveToPane`, new `SetWeights`; remove `#[mcp(skip)]` from these
  (keep it on navigation/selection).
- `vmux_macro` — derive support: default the `Anchor` field to `Active` for
  `OsMenu` / `DefaultShortcuts` / `CommandBar` construction; `McpTool` emits
  `target` as an optional id-string param.
- `vmux_layout/src/pane.rs` — generalize `handle_pane_commands` to resolve
  `Anchor`; reuse `split_leaf_into_two` / `split_or_extend`; add close / move /
  weights bodies and a `stack_into_pane` insert; add the `Anchor`→`Entity`
  resolver (reuse the id map + `parse_id`).
- `vmux_mcp/src/tools.rs` — `run` / `open_page` gain `beside` / `mode` /
  `direction`; dispatch parses ids and emits the generalized commands; drop the
  hand-written layout verbs (now generated from the un-skipped enum); spawns
  return the landing pane id.
- `vmux_agent/src/plugin.rs` — route layout ops to `AppCommand::Layout { target }`
  (bus) and spawn placement to the InPane/split path; return the pane id.
- `vmux_service/src/protocol.rs` — extend `Run` / `OpenBeside` (or an InPane
  variant) with `beside` / `mode` / `direction`; spawn responses carry the pane id.

## Testing (bevy way)

- Send `AppCommand::Layout { target: Id(...), Close / MoveToPane / SetWeights }`
  into a test `App`, run the schedule, assert the resulting ECS layout state.
  Repeat with `target: Active`.
- Spawn placement: emit `InPane(split|stack, anchor)`, assert the new pane/stack
  lands at the anchor and does **not** split the agent's pane.
- dispatch / id-parse tests in `tools.rs` for the new params; tool-presence tests
  for the now-generated layout tools; assert navigation commands keep `mcp(skip)`.
- Macro: a generated tool includes `target`; menu/shortcut paths construct with
  `Active`.

## Risks / open questions

1. **Macro work is the main cost/risk.** The custom derives must handle a
   defaulted `Anchor` field. `InPane` proves that fielded layout commands already
   work across `McpTool` / menu / shortcut (`open.rs:31-64`), but those set fields
   explicitly; defaulting `Anchor::Active` for menu/shortcut/command-bar
   construction may need new macro support. Validate early on one command
   (`Close`) before converting the rest.
2. **`open_page` vs. generalized `InPane`.** Fold `open_page` into
   `InPane(target = self)` or keep it as a thin alias? Lean: keep `open_page` as
   the self-relative convenience, implemented via the same path.
3. **Anchor representation.** `Anchor` enum vs. a `target: Option<String>` field on
   the wire. The enum is clearer; `Option<String>` is derive-simpler. Recommend
   the enum, parsed at the MCP boundary.
4. **Parallel stacking race.** For N concurrent terminals stacked into one pane,
   the first spawn creates the pane and the rest anchor to it
   (`beside: <pane>, mode: stack`). The agent must do first-then-rest, not a blind
   concurrent burst — document this in `run`'s description.
5. **Anchor-kind validation.** A pane command given a `stack:` id (or vice-versa):
   resolve to the containing pane where sensible, otherwise return an error.

## Scope for first implementation

- `Anchor` + resolver; generalize + un-skip `Close` and `InPane`; `run` /
  `open_page` placement params; refactor `handle_pane_commands` to resolve
  `target` for `Close`; tests. Prove the pattern on `Close` + `InPane` first.
- Then: `MoveToPane`, `SetWeights`, and the remaining structural commands.

## Out of scope

- `update_layout` / `reconcile` changes.
- vmux-side pooling / auto-arrange / high-level group verb.
- `is_busy` / idle tracking (later, if reuse needs it).
- Navigation/selection MCP exposure.
