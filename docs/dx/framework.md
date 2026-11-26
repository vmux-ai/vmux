# Reuse Vmux features in another app

Vmux Framework lets another Rust application use parts of Vmux, such as project loading or pane layout. For example, you could use its pane-selection behavior in a new editor and write your own UI around it.

This is optional reading. To change Vmux itself or make a personal build, follow [Make a change](feature-development.md) and [Make your own Vmux](customization.md).

## See a small example

The [pane example](https://github.com/vmux-ai/vmux/blob/main/tests/sdk/core/tests/pane.rs) creates a Bevy app and enables Vmux's layout behavior:

```rust
use bevy_app::App;
use vmux_app::VmuxPlugin;

let mut app = App::new();
app.add_plugins(VmuxPlugin::builder().layout(true).build());
```

It then creates panes, sends a selection request, and checks which pane became active. It runs as a test and does not open a window. Your application would supply the UI that displays those panes.

The independent SDK core test workspace lives in `tests/sdk/core`: it uses Vmux as a dependency. Its [document example](https://github.com/vmux-ai/vmux/blob/main/tests/sdk/core/src/lib.rs) also shows renaming and saving documents. These are compatibility tests, rather than a finished application template.

## Run the example tests

From the Vmux checkout:

```sh
vmux dx test sdk
```

This tests the examples against your local source. `vmux dx test sdk --git` tests the committed source as Git dependencies instead. Neither command launches Vmux.

## Choose the parts you need

### Load a web application package

Enable `vmux_app/hosting` for `vmux_app::manifest::hosting`. The versioned package contract
accepts an existing HTTP(S) application without changing its presentation:

```ron
(
    schema_version: 1,
    sdk_revision: "<the host's full 40-character SDK commit>",
    entry: "https://example.test/app/",
    manifest: (
        metadata: Some((id: "example.web", version: "1.0.0")),
        pages: [(
            url: "https://example.test/app/",
            title: "Example",
            startup: true,
            permissions: ["PageReady"],
        )],
    ),
)
```

Decode the bounded envelope with `Package::try_parse`, then add
`WebHostPlugin::new(Contract::new(verified_sdk_revision), package)` to the selected application
composition. The contract comes from the host's verified build identity. It must match the
package revision. A registered observer validates the schema, the declared startup page and
the host's typed page-event surface before Startup. The same canonical feature entity receives
`HostedApplication`, `EffectiveStartupUrl` and `Ready`; `HostedApplicationReady` publishes its
entity once during Startup. Renderer adapters read that entity's current startup URL.

Web packages declare pages and compatibility metadata. They cannot claim embedded asset hosts,
provide native capabilities or register native commands/tools/CLI/MCP handlers. Native code
comes from statically linked feature plugins: name them in metadata dependencies and compose
their actual adapters. `Contract::event::<Request>()` exposes an already-supported typed page
event. The page's permissions and the host's per-origin/OS checks remain explicit. The
[SDK core hosting tests](https://github.com/vmux-ai/vmux/blob/main/tests/sdk/core/tests/hosting.rs)
keeps the same URL while adding a linked native feature and its authoritative Rust request.

This defines the SDK package/host boundary. The application still supplies its renderer,
platform adapters and runner. Generic prebuilt binary distribution and the five-minute
bootstrap journey belong to VMX-222; no binary or release is published by this contract.

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
[SDK core composition tests](https://github.com/vmux-ai/vmux/blob/main/tests/sdk/core/tests/composition.rs)
registers contributions and runs a typed document operation without product adapters.

`vmux_app` provides the builder for enabling features. Your application supplies its window, UI, permissions, and storage locations. The [host-path tests](https://github.com/vmux-ai/vmux/blob/main/tests/sdk/host/tests/host.rs) show how a separate product's name and namespace keep its data and service paths separate.

Select Cargo features to make a capability available, then enable it through `VmuxPlugin::builder()`.
For example, `layout` makes the renderer-independent layout core available; `layout-runtime`
adds the native product adapter. Compiling an adapter does not install it in a core-only app.

| Public namespace | Cargo feature | Available behavior |
|---|---|---|
| `vmux_app::ecs` | `ecs`; optionally `jobs` and `contracts` | Shared components, scoped identity, operations and owned jobs. Store remains opt-in on its owning crate. |
| `vmux_app::project` | `project` | Project registration, archive and persistence requests; optional filesystem adapter. |
| `vmux_app::flex` | `flex` | Renderer-independent geometry; an explicit window viewport adapter. |
| `vmux_app::layout` | `layout` | Owner-scoped pane topology and selection; native adapters require `layout-runtime`. |
| `vmux_app::menu` | `menu`; optionally `menu-native` | Declarative ECS menu bars, submenus, entries and context menus; `menu-native` adds the OS menu adapter. |
| `vmux_app::extension` | `extension-core` | Extension catalog and installation lifecycle; `extension` adds native product adapters. |
| `vmux_app::lsp` | `lsp` | Shared server, document, request and diagnostics lifecycles; the application supplies transport. |
| `vmux_app::host` | `host` | Compiled product identity and isolated data, credential and service paths. |

The independent consumer checks the core namespaces without Editor, Profile, CEF, Winit or
product UI. A separate Host consumer validates product namespaces. The same consumer suites
run from local paths and immutable Git dependencies without the private cloud repository.

## Pin a compatible SDK

Use one reviewed main commit or release tag for every Vmux dependency. Mixing revisions can
produce incompatible ECS types even when their names match. The supported Bevy minor is 0.19;
a Bevy minor change or breaking public API change requires a compatibility review and the
consumer gate. The SDK currently uses Git dependencies rather than a supported set of
crates.io packages. Publishing is a separate release action.

During 0.0.x, pin an exact revision. Floating revisions have no compatibility guarantee.
Release notes identify breaking changes and replacement APIs; compatible aliases remain
where practical, with no fixed deprecation window before a stable SDK version.

Store schemas belong to their features. Keep reflected names stable, version persisted
contracts, and provide a migration or explicit refusal before accepting incompatible data.
Wire compatibility follows the protocol's versioned handshake. Unversioned LSP diagnostics
cannot identify which document edit produced them.

## Package a plugin and choose a target

Native plugins are Rust crates linked at build time. A plugin owns its components, typed
requests and feature manifest, and packages its pages and assets with the host build. The
host supplies the renderer, IO, permissions and runner. An embedded web surface can keep
its presentation while moving behavior into typed native plugins through the existing UI
contracts; native permissions remain explicit.

The facade provides desktop, mobile, daemon, CLI and MCP composition presets. macOS desktop
and iOS mobile have product hosts; Linux exercises headless and shared-core paths in CI.
This does not guarantee that every SDK capability has an adapter on every target. Windows,
Android, web, game and spatial SDK hosts remain unverified. Dynamic Rust loading and a
prebuilt generic product host are not supported SDK capabilities.
