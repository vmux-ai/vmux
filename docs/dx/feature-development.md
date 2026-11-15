# Make a change

Start with one behavior you can reproduce. For example: a terminal shortcut uses the wrong pane, or a page refresh loses the current selection. Build Vmux first using [Getting started](getting-started.md).

## Find the feature

Look under `crates/feature/`. Terminal code is in `vmux_terminal`; browser code is in `vmux_browser`; pane and selection behavior is in `vmux_layout`.

For a Terminal change, start with `crates/feature/vmux_terminal/src/lib.rs`. It points to the UI, event types, and host code. Look for the plugin and the handlers it registers. A search can help locate the behavior:

```sh
rg -n 'focus' crates/feature/vmux_terminal/src
```

## Trace the request

Follow the UI callback or command to its event type, then find the Rust handler that receives it. Check which entity/components it reads and what state it changes. Finally, find the view data the page receives.

This is the same debugging habit as following a React callback into a reducer and back to rendered props. The state and handler are separate from the component, so closing or recreating a page does not have to recreate the work.

For IO, also follow the returned result. A result arriving after navigation or cancellation should not change a different page or task.

## Change the behavior and test it

Keep the fix in the feature that already handles the behavior. If it needs another feature, use the existing request or shared data rather than call its private handler directly.

Find a nearby behavioral test and add a case for the problem you reproduced. Run the relevant crate's test. For example:

```sh
vmux internal cargo -- test --locked -p vmux_layout --lib
```

For an ECS test, add the feature's production plugin to the test app and send the real request. Check the resulting state. For UI work, try the interaction and inspect what the page displays. The expected user outcome matters more than the exact internal function arrangement.

## Try the result

```sh
vmux build -b my-vmux
vmux run -b my-vmux
```

Check the original problem and a nearby normal case. Then run the checks in [Validation and release](validation-and-release.md).

Keep the result in your own branch, share a PR in your fork, or propose a general improvement upstream. Explain what was wrong, what now happens, and how you checked it. Update a guide only if the way someone uses or understands the feature changed.
