# Make a change

Start with something you can see and try. Use the build from [Getting started](getting-started.md).

## Try a small visual edit

Open `crates/feature/vmux_terminal/assets/index.css`. Inside the existing `body` rule, add:

```css
font-weight: 700;
```

Rebuild and open your branch:

```sh
vmux build my-vmux
vmux run my-vmux
```

Open Terminal and check that ordinary text is bold. This is a small CSS change with an obvious result; remove the line to undo it.

## Preview shared UI components

Open `vmux://ui/` in the desktop address bar, or search for `storybook` in the command bar.
The component catalog renders the actual `vmux_ui` controls through the same native page
and IPC path as other Vmux pages. Choose a component, try its variants, toggle disabled
state, and inspect the last accepted event and current values. Reset restores the selected
example, including its internal component state. Resize the pane to check smaller layouts;
appearance and language follow the application's settings.

The catalog lives in `crates/feature/vmux_gallery`. Add examples there while keeping reusable
controls in `crates/util/vmux_ui`. It is a desktop feature: it does not require a web server,
a separate renderer, or a remote service.

## Follow a behavior change

Open `crates/feature/vmux_setting/src/ui.rs` and find `Page`, the Settings screen. Its `rsx!` markup is similar to JSX: it renders HTML-like elements and other components. The search input has this callback:

```rust
onsearch: move |query| {
    let _ = send(&SettingsFilterRequest { query });
},
```

The callback sends the search text to Rust code that filters the settings. Search for `SettingsFilterRequest` to find the handler and the state it updates. The page then receives the filtered view and renders it. [How Vmux works](architecture.md) explains this loop.

For another feature, start under `crates/feature/`: `vmux_terminal` for Terminal, `vmux_browser` for Browser, and `vmux_layout` for panes and selection. Find its UI callback or command, follow the request to its handler, then find the state shown by the page.

## Check the result

Try the original behavior and a nearby normal case. For a behavior change, add a test for the outcome you expect and run that feature's tests. For example:

```sh
vmux internal cargo -- test --locked -p vmux_layout --lib
```

For a visual change, check the rendered screen rather than testing a literal CSS class. The push hook checks formatting; CI runs the heavier checks. [Test and share a build](validation-and-release.md) covers explicit diagnostics and packaging.

You can keep the result locally, open a PR in your fork, or propose it upstream. Update the relevant UX guide when user behavior changes. Update the architecture guide only for substantial changes to responsibilities, dependencies or shared ownership/lifecycle boundaries. Routine refactors belong in code, tests and the PR; they do not require architecture edits or migration notes.
