# Framework consumer

This independent Cargo workspace exercises the public Vmux SDK without the Vmux renderer,
Profile, or the repository's Cargo patches. It uses real ECS operations with controllable
adapters for failed writes, cancellation, stale completions, independent roots, pane selection,
and layout. Project archive inputs use the same public contract as the product.

From a checkout of `vmux-ai/vmux`:

```sh
bash scripts/test-framework-consumer.sh
bash scripts/test-framework-consumer.sh --git
```

The second command copies this workspace to a temporary directory and replaces its Vmux path
dependencies with Git dependencies pinned to the current committed revision. It runs the same
tests and rejects native dependencies or mixed SDK revisions. Commit the candidate first; this
mode intentionally tests committed source.
The verification runs through the Rust builder with Git and Cargo.

For another application, select the required `vmux_app` Cargo features and use
`VmuxPlugin::builder()` to install them. Public feature types live under `vmux_app::ecs`,
`project`, `flex`, and `layout`. `extension-core` exposes the renderer-independent `extension`
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

The supported surface and compatibility rules are in `docs/architecture.md`.
