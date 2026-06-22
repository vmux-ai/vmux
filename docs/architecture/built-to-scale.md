# ECS: Build to scale

> Part of the [Vmux Architecture](../architecture.md) overview.

The same property that makes Vmux pleasant to write makes it compound as it grows:
**composition over inheritance**. An entity is defined entirely by the components attached
to it — no base classes, no inheritance trees:

- A web view *becomes* a shell by adding a `Terminal` component.
- A web view switches to a native surface via `WebviewWindowed`.
- Systems query for component sets (e.g. the `Active` tag), so behavior is opt-in — you
  add a capability, you don't inherit one.

## Modular extensibility via plugins

A Bevy `Plugin` is the unit of modularity: it bundles the components, systems, resources,
and messages for one domain and registers them into the `App` in a single call. Vmux is
assembled by stacking them:

```
crates/vmux_desktop/src/lib.rs
├── LayoutPlugin
├── BrowserPlugin
├── TerminalPlugin
├── AgentPlugin
├── ServicePlugin
└── SpacePlugin        (…and a dozen more, one per crate)
```

Adding a feature means adding a plugin; each stays decoupled in its own crate and
independently testable. The same composition that builds an entity from components builds
the *app* from plugins.

## What compounds as you grow

- **More features → more plugins.** Each domain is an isolated plugin in its own crate;
  adding one doesn't touch the rest.
- **More surfaces → more entities.** Panes, stacks, and spaces are just entities; the ECS
  stores them contiguously and runs systems over them in parallel.
- **More agents → more spaces.** Each agent is anchored to its own Space subtree, so many
  can work concurrently without touching the space you're in.
- **Heavier pages → the GPU.** Web surfaces composite on the GPU while the host loop stays
  reactive — load goes up, idle cost doesn't.

## Built to sit idle

Most game engines pump the render loop continuously, pinning a core even on a static
scene. Vmux forces `winit` into a strictly **reactive** update mode and uses a CEF wake
throttler to tick the loop at the monitor's refresh rate *only* when there's work — render
changes, scroll, cursor animation, terminal output. Static workspace, quiet machine.
