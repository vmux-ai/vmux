# Vmux

Cargo [workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html) (similar in spirit to [Bevy’s repo layout](https://github.com/bevyengine/bevy/)):

| Crate | Role |
| ----- | ---- |
| **`vmux_core`** | `VmuxWorldCamera`, `CAMERA_DISTANCE` — shared by `vmux` + `vmux_webview` |
| **`vmux`** | Library + binary `vmux`: `VmuxPlugin`, `core` (re-exports), `cef_root_cache_path` |
| **`vmux_webview`** | `VmuxWebviewPlugin` — CEF plane, layout |
| **`vmux_input`** | `VmuxInputPlugin` — leafwing quit bindings |
| **`vmux_screenshot`** | `VmuxScreenshotPlugin` — Space screenshots, busy cursor |

### Development

**Dioxus web UIs** (`vmux_status_bar`, `vmux_history`): native builds run each crate’s `build.rs`. You need the **`wasm-bindgen` CLI** (see those crates’ READMEs / `Cargo.lock`), e.g. `cargo install wasm-bindgen-cli --version 0.2.115 --locked`. Optional: `VMUX_HISTORY_USE_DX=1` uses the **`dx`** binary for history only.

See [Makefile](Makefile) for more targets.

```sh
make run-mac
```
