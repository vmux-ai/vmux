# Vmux Framework

Build pure-Rust desktop, headless, CLI, daemon, and MCP applications from typed Bevy ECS feature plugins and Dioxus pages.

Start with the [Framework quickstart](framework/framework-quickstart.md) to run a CEF-free native desktop window app or a headless ECS example in minutes. To modify Vmux itself or build a personal fork, see [Make a change](feature-development.md) and [Keep your own version](customization.md).

## Guide map

- [Framework quickstart](framework/framework-quickstart.md): run a native counter window, a split workspace, or a headless ECS app.
- [Core concepts](framework/framework-core-concepts.md): the eight primitives, entity vocabulary, operation policies, async tasks, channels, timers, and autosave stores.
- [Windows, pages, and menus](framework/framework-windows-and-pages.md): `WindowHostPlugin`, `PageHostPlugin`, `UiStatePlugin`, `UiIpcEventPlugin`, `vmux_ui`, declarative menus, and `WebHostPlugin`.
- [Testing and plugin authoring](framework/framework-testing-and-plugins.md): deterministic headless testing, `feature.ron` composition, capabilities, and isolated product namespaces.
- [Crates and macros reference](framework/framework-reference.md): complete `vmux_app` feature matrix, attribute macro cheat sheet, and target support matrix.

## Run the examples

Runnable Bevy-style examples live in [`examples/`](https://github.com/vmux-ai/vmux/blob/main/examples/README.md):

```sh
cargo run -p vmux_examples --example panes
cargo run -p vmux_examples --example async_channel
cargo run -p vmux_examples --features window --example window_counter
```

Pass `VMUX_EXAMPLE_HEADLESS=1` to smoke-test any windowed example without opening an OS window.

## Validate external SDK consumers

Independent Cargo workspace compatibility suites live in `tests/sdk/core` and `tests/sdk/host`:

```sh
vmux dx test sdk
vmux dx test sdk --host
vmux dx test sdk --git
```

Neither command launches Vmux. `--git` copies the consumer workspace and verifies pinned immutable Git revisions and dependency isolation without private cloud crates or `bevy_cef`.

## Pin a compatible SDK

Use one reviewed `main` commit or release tag for every Vmux dependency. Mixing revisions can produce incompatible ECS types even when their names match. The supported Bevy minor is `0.19`. During `0.0.x`, pin an exact revision; crates.io publication is a separate release action.

Store schemas belong to their features. Keep reflected names stable, version persisted contracts, and provide a migration or explicit refusal before accepting incompatible data.
