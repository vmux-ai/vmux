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
| [`layout`](layout.rs) | host | Tabs, a stack within one, and sheets on top |
| [`minimal`](minimal.rs) | host, simulator | The phone, as one plugin |

```sh
cargo run -p vmux_mobile --example layout
cargo run -p vmux_mobile --example minimal --features mobile
```

`-p` is not optional: this workspace names its default members, and without it cargo
looks for the example only among those and reports it missing while listing it.

`layout` needs no Mac, no relay, no pairing and no UIKit. It sends the messages a
phone would and draws the result out of the ECS:

```
a sheet, then a sheet over it
    ╭──────────────────────────────╮
    │ Really?                sheet │
    │ Discard?               sheet │
    │ Edit                  pushed │
    │ Groceries             pushed │
    │ Notes                   root │
    ╰──────────────────────────────╯
      Inbox  [Notes]
```

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
