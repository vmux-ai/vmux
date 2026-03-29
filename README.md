# Vmux

Cargo [workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html) (similar in spirit to [Bevy’s repo layout](https://github.com/bevyengine/bevy/)):

| Crate | Role |
| ----- | ---- |
| **`vmux`** | Binary (`crates/vmux`) |
| **`vmux_app`** | Umbrella plugin composing the crates below (`crates/vmux_app`) |
| **`vmux_core`** | Shared markers (`VmuxWebview`, `VmuxWorldCamera`) + `cef_root_cache_path` |
| **`vmux_input`** | `VmuxInputPlugin` — leafwing quit bindings |
| **`vmux_scene`** | `VmuxScenePlugin` — camera, light, default webview plane |
| **`vmux_webview_layout`** | `VmuxWebviewLayoutPlugin` — `WebviewSize` + plane scale |
| **`vmux_screenshot`** | `VmuxScreenshotPlugin` — Space screenshots, busy cursor |

### Development

See [Makefile](Makefile) for more targets.

```sh
make run-mac
```
