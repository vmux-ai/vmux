# Dev Dynamic Linking Design

## Goal

Speed up local `make dev` rebuilds by enabling Bevy dynamic linking only for the desktop development build.

## Scope

This change affects local development builds only. Release, local packaging, CI release artifacts, and website builds stay statically linked.

## Design

Rename the `vmux_desktop` feature `debug` to `dev` and make it enable both CEF debug mode and Bevy dynamic linking:

```toml
[features]
dev = ["bevy_cef/debug", "bevy/dynamic_linking"]
```

Update `make dev` to build `vmux_service` and `vmux_cli` without `--features dev`, then build only `vmux_desktop` with `--features dev`. Cargo unifies package features across a single build invocation, so keeping the service/CLI build separate prevents the launchd-managed service binary from inheriting Bevy dynamic linking.

The dev run command must provide Rust's target library directory and Cargo's debug deps directory to dyld:

```bash
DYLD_LIBRARY_PATH="$(rustc --print target-libdir):target/debug/deps" ./target/debug/vmux_desktop
```

Bevy dynamic linking makes the desktop binary load Rust `libstd` dynamically via `@rpath`. `make dev` executes `target/debug/vmux_desktop` directly instead of through `cargo run`, so Cargo does not populate the dynamic library path for us. Cargo rustflags must not be used here because Cargo passes them into build-script environments, and `vmux_ui` build scripts invoke `dx build` for `wasm32-unknown-unknown`. `install_name_tool -add_rpath` is also unsuitable because the debug binary does not reserve enough load-command padding.

Keep `install-debug-render-process` using `--features debug` for `bevy_cef_debug_render_process`; that feature belongs to the patched CEF subprocess crate, not `vmux_desktop`.

## Runtime Handling

The first implementation should rely on Cargo and Bevy's standard dynamic-linking behavior plus a dev-only `DYLD_LIBRARY_PATH`. Dev codesigning uses `VmuxDev.entitlements`, which adds `com.apple.security.cs.allow-dyld-environment-variables` to the release entitlements. Release packaging stays on `Vmux.entitlements`.

## CI

Do not enable dynamic linking in default CI. Clean CI builds gain little from dynamic linking, and release/package validation should keep matching production linkage. Benchmark jobs are out of scope for this change.

## Testing

Build verification is enough for this configuration change:

```bash
env -u CEF_PATH cargo build -p vmux_service -p vmux_cli
env -u CEF_PATH cargo build -p vmux_desktop --features dev
env -u CEF_PATH DYLD_LIBRARY_PATH="$(rustc --print target-libdir):target/debug/deps" ./target/debug/vmux_desktop
```

In sandboxed environments without GPU access, the run command may stop after dynamic loading with Bevy's "Unable to find a GPU" panic. That still verifies the dyld failure is gone.
