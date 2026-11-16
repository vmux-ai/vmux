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
and IPC path as other Vmux pages. Choose a component, try its presets, edit the disabled
property in Controls, and inspect the last accepted event and current values. The sidebar and content
scroll independently, using the same workspace layout as Tools and Git. Search accepts
component names such as `ComposerFrame` and `IdentityOrb`. Each family has a direct URL:
`vmux://ui/composer` opens the composer, and the shared breadcrumb and browser history
follow the selected family. The Default preset restores the example and its local state.
Resize the pane to check smaller layouts; appearance and language follow application settings.

The sidebar groups stories into Primitives, Forms, Composition, Navigation, Feedback,
and Icons. The breadcrumb includes the selected category. The preview appears before
the inspector; wider panes place Controls beside it, and Events expand when needed.
Open `vmux://ui/icon` for the searchable icon library. Line, builtin and Git icons are
enumerated from their declared variants, alongside file types, favicons and agent logos.
Change the icon size to compare them at the sizes used in feature pages.

Keep a component's metadata and conditions next to its implementation:

```text
crates/util/vmux_ui/src/components/button.rs
crates/util/vmux_ui/src/components/button.stories.ron
```

The RON file describes its category, title, Controls, defaults and named presets. `catalog.rs` in
`crates/feature/vmux_gallery` connects that file to a shared Rust renderer with one
`#[story(metadata = "...", component = ...)]` entry. The registration attribute generates
names, component URLs, search metadata and dispatch, and type-checks preset fields.
Gallery Rust code stays focused on shared previews and Controls; it does not need a new
adapter file for every component. Add an executable example before integrating a new
shared component into a feature page.

Args can change several conditions at once, including when the preview is disabled.
Named presets replace previous conditions and reset component-local state. Composer,
menu and directory callbacks update isolated sample state. The catalog is a desktop feature.

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
