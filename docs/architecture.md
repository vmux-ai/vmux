# Vmux — Architecture

An agent-first workspace that ships with a browser and an IDE. *Designed to co-work with
agents.*

For the experience, see [User Experience](experience.md).

---

## TL;DR

Vmux is a native **Rust** host that embeds Chromium via the **Chromium Embedded Framework
(CEF)** — the browser is a *guest surface*, not the container. That inversion is the whole
thesis: instead of your app living inside a web sandbox (Electron), the web lives inside a
native host that reaches straight to the OS and GPU.

The host is built on **Bevy**, a data-oriented **ECS**: state in components, behavior in
systems, one strongly-typed language end to end. Web views composite on the GPU into a
`tmux`-style tiling tree (Space → Tab → Pane → Stack), and agents drive the whole thing
over an **MCP** server — the workspace is an API.

The payoff: **Chrome-parity CPU**, direct native reach, and a multi-surface workspace that
both humans and agents tile, persist, and reconcile in real time.

## Deep dives

- **[Why Rust + CEF](architecture/why-rust-cef.md)** — the runtime inversion, and
  why a web sandbox is the wrong ceiling for an OS-level workspace.
- **[Rust for React JS developers](architecture/rust-without-the-headaches.md)** — Bevy ECS as an
  in-memory database, lock-free concurrency, and Rust as the universal FFI glue.
- **[ECS, explained](architecture/built-to-scale.md)** — entities, components, and systems
  from the ground up, mapped onto the React model you already know.
- **[Plugins](architecture/plugins.md)** — the `build()` contract, one capability per crate,
  and how the whole app is assembled from the plugin stack.
- **[Co-working with agents](architecture/agent-first.md)** — the MCP surface,
  the persistence daemon, and the scheme-gated security bridge.
- **[The layout model](architecture/layout-model.md)** — Space → Tab → Pane → Stack, the
  selection invariant, and structural persistence.
- **[The render stack](architecture/render-stack.md)** — many CEF surfaces in one window,
  zero-copy interop, Rust-all-the-way-down UI, and the 3D mode.
