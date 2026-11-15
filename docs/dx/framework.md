# Reuse Vmux features in another app

Vmux Framework lets another Rust application use parts of Vmux, such as project loading or pane layout. For example, you could use its pane-selection behavior in a new editor and write your own UI around it.

This is optional reading. To change Vmux itself or make a personal build, follow [Make a change](feature-development.md) and [Make your own Vmux](customization.md).

## See a small example

The [pane example](https://github.com/vmux-ai/vmux/blob/main/examples/framework-consumer/tests/pane.rs) creates a Bevy app and enables Vmux's layout behavior:

```rust
use bevy_app::App;
use vmux_app::VmuxPlugin;

let mut app = App::new();
app.add_plugins(VmuxPlugin::builder().layout(true).build());
```

It then creates panes, sends a selection request, and checks which pane became active. It runs as a test and does not open a window. Your application would supply the UI that displays those panes.

The example directory is called `framework-consumer`: it uses Vmux as a dependency. Its [document example](https://github.com/vmux-ai/vmux/blob/main/examples/framework-consumer/src/lib.rs) also shows renaming and saving documents. These are sample code and tests, rather than a finished application template.

## Run the example tests

From the Vmux checkout:

```sh
vmux dx test sdk
```

This tests the examples against your local source. `vmux dx test sdk --git` tests the committed source as Git dependencies instead. Neither command launches Vmux.

## Choose the parts you need

`vmux_app` provides the builder for enabling features. Your application supplies its window, UI, permissions, and storage locations. The [host-path tests](https://github.com/vmux-ai/vmux/blob/main/examples/host-consumer/tests/host.rs) show how a separate product's name and namespace keep its data and service paths separate.

Use the example's Cargo features and pin all Vmux dependencies to one reviewed main commit or release tag, with its required Bevy version. The SDK currently uses Git dependencies; its public APIs can still change.
