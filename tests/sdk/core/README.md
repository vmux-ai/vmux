# SDK core compatibility tests

This independent Cargo workspace exercises the public Vmux SDK without the Vmux renderer,
Profile, or the repository's Cargo patches. It uses real ECS operations with controllable
adapters for failed writes, cancellation, stale completions, independent roots, pane selection,
and layout. Project archive inputs use the same public contract as the product.

From a checkout of `vmux-ai/vmux`:

```sh
vmux dx test sdk
vmux dx test sdk --git
```

The second command copies this workspace to a temporary directory and replaces its Vmux path
dependencies with Git dependencies pinned to the current committed revision. It runs the
consumer tests and dependency/revision checks; the local command also runs the core package
tests. Commit the candidate first; Git mode intentionally tests committed source.
The verification runs through the Rust builder with Git and Cargo.

For another application, select the required `vmux_app` Cargo features and use
`VmuxPlugin::builder()` to install them. Public feature types live under `vmux_app::ecs`,
`project`, `flex`, `layout`, and `lsp`. `extension-core` exposes the renderer-independent `extension`
namespace; `extension` adds its native product adapters.
Use one reviewed main commit or release tag for every Vmux dependency. Bevy 0.19 is the current
supported minor. Keep `vmux_ecs` directly available for its schema macros, and select the
owning crate's optional store/adapter features explicitly.

`VmuxPlugin::builder().layout(true)` installs the renderer-free Layout core. Native Vmux
composition selects `layout-runtime` separately; enabling its Cargo feature makes the adapter
available without adding it to a core-only app. Layout owns its pane state and selection.

An application providing Project IO supplies `LoadCompleted` and `WriteResult` against the
accepted tickets, or opts into the filesystem adapter. It does not need Editor or a Vmux
Profile. Native plugins added to Vmux use the same components and operations, with their
presentation and platform adapters composed at the host boundary.

The LSP consumer uses the same public server, document, request, and diagnostic lifecycles as
Editor with a controllable transport. The local SDK command and CI also run `vmux_lsp`'s
library tests with default features disabled. Protocol diagnostics without a document version
cannot identify which edit produced them.

The supported surface and compatibility rules are in the [framework guide](../../../docs/dx/framework.md).
