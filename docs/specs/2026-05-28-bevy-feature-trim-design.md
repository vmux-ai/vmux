# Bevy Feature Trim Design

## Goal

Reduce iterative build cost by replacing Bevy's default feature set with an explicit feature list that supports vmux's desktop, service, UI, CEF, and rendering paths.

## Approach

Use an aggressive root-level trim first. The workspace `bevy` dependency will set `default-features = false` and enumerate the features vmux needs. Keep feature ownership centralized in the root `Cargo.toml` so all crates share one Bevy capability set and Cargo feature unification stays predictable.

The first pass will not change patched `bevy_cef` default features. That avoids mixing Bevy root trimming with CEF patch surgery. If build verification shows `bevy_cef` still re-enables too much, trim the patch in a second pass.

## Feature Set

Keep app/runtime basics: `std`, `multi_threaded`, `async_executor`, `bevy_asset`, `bevy_log`, `bevy_state`, `bevy_scene`, `reflect_auto_register`, `custom_cursor`, `default_font`, and `https`.

Keep desktop and rendering stack: `bevy_winit`, `bevy_window`, `bevy_render`, `bevy_core_pipeline`, `bevy_pbr`, `bevy_sprite`, `bevy_ui`, `bevy_text`, `bevy_image`, and `png`.

Keep interaction and current vmux-specific needs: `bevy_input_focus`, `bevy_picking`, `mesh_picking`, `ui_picking`, `bevy_camera_controller`, and `free_camera`.

Drop obvious unused defaults: audio, Vorbis, gamepad/Gilrs, glTF, animation, morph animation, KTX2, SMAA LUTs, tonemapping LUTs, sysinfo plugin, WebGL, and Android platform features.

Linux windowing features are a verification item. If Linux checks fail due missing platform support, add back only the needed platform feature instead of restoring `default_platform`.

## Verification

Add a release invariant test that parses the root `Cargo.toml` and asserts the workspace Bevy dependency has `default-features = false`. The test will also reject obvious heavy root features such as `audio`, `gamepad`, `bevy_gltf`, `gltf_animation`, `vorbis`, `ktx2`, `smaa_luts`, and `tonemapping_luts`.

Run compile verification with `env -u CEF_PATH cargo check -p vmux_desktop --features dev`. If feature errors appear, add only the smallest missing Bevy feature. Do not use `bevy/default` or `default_platform`.

Before committing implementation changes, run changed-crate final gates: `cargo fmt`, `cargo clippy`, and `cargo test` for the packages reported by `BASE=origin/main ./scripts/changed-crates.sh`.
