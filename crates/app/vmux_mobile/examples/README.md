# Examples

`vmux_mobile` exports two things: `MobilePlugin`, the phone's whole runtime, and
`NavPlugin<S>`, the navigation on its own. `main.rs` is the first of those and
nothing else.

Navigation is ECS. A tab is an entity, a pushed level is its child, and a sheet is a
child marked as one — so depth is a walk down the tree, and closing a tab takes its
stack with it rather than leaving a map to tidy up. You write to it with messages and
read it with a query.

| Example | Runs on | Shows |
| --- | --- | --- |
| [`tabs`](tabs.rs) | host | Reported tabs, selection, and the phone's own |
| [`stack`](stack.rs) | host | Push, pop, and a swipe UIKit already ran |
| [`modal`](modal.rs) | host | Sheets stacking, and dismissing the top one |
| [`minimal`](minimal.rs) | host, simulator | The phone, as one plugin |

```sh
cargo run --example tabs
cargo run --example minimal --features mobile
```

The first three need no Mac, no relay and no pairing, and no UIKit: they drive the
plugin and print what it made of the messages.

`minimal` opens a window. On the host that is winit and wry alone — push, pop and the
back-swipe are `UINavigationController`, so they only happen on the simulator or a
device.

## Your own screens

`Screen` is what the navigation needs to know about whatever you show:

```rust
#[derive(Clone, PartialEq)]
struct Page(&'static str);

impl Screen for Page {
    fn title(&self) -> String {
        self.0.to_string()
    }
}
```

`is` is the second half, and it defaults to equality. Override it when two screens
are the same thing arrived at twice — vmux compares session ids, so a conversation
the phone started stops being its own tab once the Mac reports it.
