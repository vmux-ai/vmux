# ECS: Design to scale

> Part of the [Vmux Architecture](../architecture.md) overview.

The same property that makes Vmux pleasant to write makes it compound as it grows:
**composition over inheritance**. An entity is just an id, defined entirely by the
components attached to it — no base classes, no inheritance trees:

- A web view *becomes* a shell by adding a `Terminal` component.
- A web view switches to a native surface via `WebviewWindowed`.
- Systems query for component sets (e.g. the `Active` tag), so behavior is opt-in — you
  add a capability, you don't inherit one.

## State in components, behavior in systems

Vmux holds no object graph. Every pane, stack, space, and surface is an **entity**; its
state lives in **components** (plain data), and every behavior is a **system** — a function
that queries for a set of components and runs over each matching entity. The `World` ties
them together as the one source of truth: an in-memory database the systems read and write.

Because components are stored contiguously by type, a system touches only the data it asks
for — cache-friendly by construction. Bevy then schedules systems with non-overlapping
queries across cores in **parallel** on its own: you declare *what* data you need, the
engine decides *when* it runs.

## What compounds as you grow

- **More behavior → more systems.** A capability is a system over a query; independent
  systems run in parallel, so adding one rarely costs the others.
- **More surfaces → more entities.** Panes, stacks, and spaces are just entities; the ECS
  stores them contiguously and iterates them at speed.
- **More agents → more spaces.** Each agent is anchored to its own Space subtree, so many
  can work concurrently without touching the space you're in.
- **Heavier pages → the GPU.** Web surfaces composite on the GPU while the host loop stays
  reactive — load goes up, idle cost doesn't.

## Built to sit idle

Most game engines pump the render loop continuously, pinning a core even on a static
scene. Vmux forces `winit` into a strictly **reactive** update mode and uses a CEF wake
throttler to tick the loop at the monitor's refresh rate *only* when there's work — render
changes, scroll, cursor animation, terminal output. Static workspace, quiet machine.
