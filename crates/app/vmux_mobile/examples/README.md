# Examples

`vmux_mobile` exports two things: `MobilePlugin`, the phone's whole runtime, and
`NavPlugin<S>`, the navigation on its own. `main.rs` is the first of those and
nothing else.

Navigation is ECS. A tab is an entity, a pushed level is its child, and a sheet is a
child marked as one — so depth is a walk down the tree, and closing a tab takes its
stack with it rather than leaving a map to tidy up. You write to it with messages,
read it with a query, and declare which page draws what in rsx.

A level is a webview. The shell draws the selected tab's root, and every pushed level
and every sheet is a `NativePage` of its own that UIKit animates in — so a modal is a
real modal with its own document, not the one webview redrawn behind a still.

| Example | Shows |
| --- | --- |
| [`layout`](../../../../examples/layout) | Tabs, a stack within one, and sheets on top |
| [`minimal`](minimal.rs) | The phone, as one plugin |

```sh
make layout-mobile    # from client/, or `make <worktree> layout-mobile` from vmux-cloud
```

`layout` is a real app on a simulator, and the only way to see the parts no test
reaches and CI cannot run: a push that animates, a back-swipe that follows your
finger, a sheet you can drag down. Its tabs are canned rather than reported over
QUIC, so it needs no Mac, no relay and no pairing.

It is a crate of its own, in `examples/layout/`, and `Dioxus.toml` is why: that file
is per-crate, so an example living inside `vmux_mobile` is bundled under the app's
identifier and installs *over* the real thing. Its own crate means its own
identifier, `ai.vmux.layout`, and the two sit side by side on the simulator.

`minimal` is the app itself, and wants a paired Mac to be worth looking at:

```sh
cargo run -p vmux_mobile --example minimal --features mobile
```

## Your own screens

`Route` is what the navigation needs to know about whatever you show — a title, and
a name to declare it under:

```rust
#[derive(Clone, PartialEq)]
struct Page(&'static str);

#[derive(Clone, Copy, PartialEq)]
struct Name(&'static str);

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        Name(self.0)
    }

    fn title(&self) -> String {
        self.0.to_string()
    }
}
```

Then declare what draws what, the way React Navigation does. Nothing about tabs or
depth lives here — that is the ECS, and this is only the screen table:

```rust
rsx! {
    NavigationContainer::<Page> {
        TabNavigator {
            Screen::<Page> { name: Name("note"), draws: &NOTE }
            Sheet::<Page> { name: Name("alert"), draws: &ALERT }
        }
    }
}
```

`Screen` pushes and `Sheet` presents, so the declaration is what decides how a route
arrives — callers only ever say `navigation.go(route)`. Alongside it, `go_back` and
`navigate` do what `useNavigation()` does.

`Route::is` is the last piece, and it defaults to equality. Override it when two
routes are the same thing arrived at twice — vmux compares session ids, so a
conversation the phone started stops being its own tab once the Mac reports it.
