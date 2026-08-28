# Examples

`vmux_mobile` exports two things: `MobilePlugin`, the phone's whole runtime, and
`NavPlugin<S>`, the navigation on its own. [`layout`](layout) drives the second.

```sh
make layout-mobile    # from client/, or `make <worktree> layout-mobile` from vmux-cloud
```

It is a real app on a simulator, and the only way to see the parts no test reaches and
CI cannot run: a push that animates, a back-swipe that follows your finger, a tab that
slides under your thumb, a sheet you can drag between detents. Its tabs are canned
rather than reported over QUIC, so it needs no Mac, no relay and no pairing.

It is a crate of its own, and `Dioxus.toml` is why: that file is per-crate, so an
example living inside `vmux_mobile` is bundled under the app's own identifier and
installs *over* the real thing. Its own crate means its own identifier,
`ai.vmux.layout`, and the two sit side by side on the simulator.

Navigation is ECS. A tab is an entity, a pushed level is its child, and a presented one
is a child marked `Presented` — so depth is a walk down the tree, and closing a tab takes
its stack with it rather than leaving a map to tidy up. You write to it with messages,
read it with a query, and declare the screen table in rsx.

A screen is a webview. Every tab root, every pushed level and every presented level is a
`NativePage` of its own that UIKit animates in, and a tab keeps its own navigation
controller — which is what lets two tabs sit side by side and be dragged between.

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

Then declare which component draws which route. Nothing about tabs or depth lives
here — that is the ECS, and this is only the screen table:

```rust
rsx! {
    Stack::<Page> {
        Tabs {
            Screen::<Page> { name: Name("note"), component: &NOTE }
        }
        Screen::<Page> {
            name: Name("alert"),
            component: &ALERT,
            presentation: Presentation::FormSheet,
            detents: &[0.5, 1.0],
        }
    }
}
```

The names come from Expo Router and React Navigation, because that is where most people
meet this shape: `Stack`, `Tabs`, `Screen`, `component`, `presentation`, `ScreenOptions`,
`use_navigation()`, `use_route()`. How a
route arrives is an option on its screen — `Card` pushes, `Modal` and `FormSheet` slide
up, `FullScreenModal` covers — so callers only ever say `navigation.go(route)`.

What is deliberately not borrowed is the file-based routing. `_layout.tsx` works because
a bundler can glob a directory into a route tree; in Rust the nesting is the rsx, which
the compiler can check.

`use_route()` hands a screen the route it was opened for, which is not the same as the
route on top — a pushed level keeps its own title while a sheet sits over it.

`Route::is` is the last piece, and it defaults to equality. Override it when two
routes are the same thing arrived at twice — vmux compares session ids, so a
conversation the phone started stops being its own tab once the Mac reports it.
