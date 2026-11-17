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

### Register an external feature

Enable `vmux_app`'s `manifest` Cargo feature to use `vmux_app::manifest`. It exposes the existing
`FeaturePlugin<M>`, `FeatureManifestSource`, `FeatureManifest` and frozen `Registry` without a
renderer. A feature's RON source can declare pages, commands, tools, CLI and MCP contributions.
`FeatureManifest::try_parse` decodes an external source with an ordinary error; embedded
sources continue to use `FeatureManifest::parse`.

Pass a decoded declaration to `FeaturePlugin::<Namespace>::from_manifest`. The namespace type
still implements `FeatureManifestSource`, so the existing FeatureId and registry keep one owner.
An external declaration registers once; it cannot replace a previously installed feature.
Repeated registration of the same embedded feature remains idempotent.

An optional `metadata` block declares a stable feature ID and version, versioned dependencies,
provided and required capabilities, requested host-policy permissions, target profiles, SDK/Bevy
compatibility and named lifecycle contracts:

```ron
metadata: Some((
    id: "example.notes",
    version: "1.0.0",
    dependencies: [(id: "example.storage", version: "^1.0")],
    capabilities: ["storage.files"],
    permissions: ["data.write"],
    targets: ["headless", "daemon"],
    sdk: Some(">=0.0.0, <1.0.0"),
    bevy: Some("0.19"),
    lifecycle: (startup: ["Update"], shutdown: ["OnExit"]),
)),
```

Select the application context separately from its feature and runner plugins:

```rust
use vmux_app::manifest::{CompositionPlugin, Profile};

app.add_plugins(CompositionPlugin(
    Profile::new("daemon").permission("data.write"),
));
```

The storage adapter feature supplies `storage.files` through its own manifest. A host may also
declare a capability through `Profile::capability` after providing its actual adapter. Finish and
clean up the app before starting its runner. Composition validation refuses missing or cyclic
dependencies, incompatible versions, duplicate IDs, unsupported target profiles and unavailable
capabilities or host policy. A metadata-bearing feature defaults to the `headless` context when
the application has not selected one.

Permission metadata does not grant OS access, and lifecycle names do not schedule callbacks.
The host's actual per-origin permission checks, feature systems and operation/job cleanup remain
authoritative. Existing manifests without metadata remain valid. The independent
[composition consumer](https://github.com/vmux-ai/vmux/blob/main/examples/framework-consumer/tests/composition.rs)
registers contributions and runs a typed document operation without product adapters.

`vmux_app` provides the builder for enabling features. Your application supplies its window, UI, permissions, and storage locations. The [host-path tests](https://github.com/vmux-ai/vmux/blob/main/examples/host-consumer/tests/host.rs) show how a separate product's name and namespace keep its data and service paths separate.

Use the example's Cargo features and pin all Vmux dependencies to one reviewed main commit or release tag, with its required Bevy version. The SDK currently uses Git dependencies; its public APIs can still change.
