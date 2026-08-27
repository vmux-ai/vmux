# Examples

`vmux_mobile` exports `MobilePlugin`, the phone's whole runtime. `main.rs` is that
plugin and nothing else.

| Example | Runs on | Shows |
| --- | --- | --- |
| [`minimal`](minimal.rs) | host, simulator | The phone, as one plugin |
| [`navigation`](navigation.rs) | host | A Mac tab flattened into the phone's entries |

```sh
cargo run --example navigation
cargo run --example minimal --features mobile
```

`navigation` needs no Mac, no relay and no pairing — it feeds a layout in by hand,
which is how the model can be exercised away from a paired desktop.

`minimal` opens a window. On the host that is winit and wry alone: the navigation
stack is `UINavigationController`, so push, pop and the back-swipe only exist on the
simulator or a device.
