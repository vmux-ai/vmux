# Layout: Space → Tab → Pane → Stack

> Part of the [Vmux Architecture](../architecture.md) overview.

The tree the agent reconciles — and the one you tile by hand — is one structure. Ownership
is structural: an element's position in the tree *is* its identity.

```
Window
├─ Header                                      ← shared top bar (the layout)
├─ SideSheet · Left / Right / Bottom           ← space navigator + slide-in panels
└─ Main
   ├─ Space "vmux-ai/vmux" [Active]            ← visible project workspace
   │  ├─ Tab [Active]
   │  │  └─ PaneSplit
   │  │     ├─ Pane [Active] ─ Stack [Active] ─ Browser  (web page · terminal · agent)
   │  │     └─ Pane          ─ Stack          ─ Browser
   │  └─ Tab
   └─ Space "acme/dashboard"                   ← fully alive, hidden
      └─ Tab [Active] ─ …
```

## Node primitives

- **Header / SideSheet** — shared layout surfaces on the Window (top bar; left navigator,
  right/bottom panels), outside the Space tree — drawn by the layout web view.
- **Space** — an Arc-style project container. Exactly one is `Active` and drawn; the rest
  stay fully alive in the background.
- **Tab** — a saved pane arrangement inside a Space.
- **Pane / PaneSplit** — a recursive row/column tree: `tmux`-style tiling.
- **Stack** — multiple leaves stacked in one leaf pane (cycle through like browser tabs).
- **Browser** — the leaf web view, in one of three kinds: a **web page**, a **terminal**,
  or an **agent** (a page agent, or a CLI agent running in a terminal).

## The selection invariant

One rule: **at most one `Active` child per parent.** Focus is found by walking `Active`
down the tree.

To keep it self-maintaining, lightweight `ensure_active_*` systems check topology each
frame (`crates/vmux_layout/src/active.rs`): if a parent has children but no `Active` one,
they tag the most-recently-focused sibling. If a human or agent destroys a node, the gap
heals next frame. And because a tab is owned by its Space *entity* — not a loose string
key — mutations are sandboxed within that Space. Cross-space leaks are impossible by
design.

> **Refactor note:** this structural model replaces a legacy one that tagged tabs with a
> detached `SpaceId` string and computed selection globally — which let creating a space
> corrupt another's panes. Moving ownership into the entity graph eliminated that whole
> class of bug.

## Persistence engine

The live ECS state is snapshotted to `store.ron` via `moonshine-save`. Because ownership
is modeled as structural parent/child relations, the whole workspace round-trips without
degradation. A schema-version sidecar guards startup: on an incompatible version it
hard-resets rather than loading a broken store.
