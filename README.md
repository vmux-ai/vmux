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

See [Makefile](Makefile) for more targets.

```sh
make run-mac
```
