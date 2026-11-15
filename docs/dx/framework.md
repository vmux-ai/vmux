# Vmux Framework

Vmux Framework is for building another application from the same Rust feature modules. You do not need it to use Vmux as a browser, IDE, or agent workspace.

## Start from an example

Read the [framework consumer](https://github.com/vmux-ai/vmux/tree/main/examples/framework-consumer) and [host consumer](https://github.com/vmux-ai/vmux/tree/main/examples/host-consumer). They use the actual public APIs without depending on the private relay repository.

From the Vmux checkout, run their checks with:

```sh
vmux framework-consumer
vmux framework-consumer -- --git
```

The first uses local path dependencies; the second checks pinned Git dependencies.

## Connect the pieces

`vmux_app` provides the shared builder for choosing features. Each feature's plugin registers its state and handlers. The application supplies its runner, native IO, permissions, and storage locations.

A feature that works without a renderer should stay usable without the browser/window libraries. A custom host uses its own identity and data instead of borrowing Vmux's credentials. [How Vmux works](architecture.md) explains the UI/state split behind this.

## Use one compatible revision

Pin all Vmux dependencies to the same reviewed main commit or release tag and use its required Bevy version. Types from different revisions can be incompatible even when their names match.

The SDK currently uses pinned Git revisions rather than a stable crates.io distribution. Check the example and feature code for platform support. Dynamic Rust plugin loading is not a supported feature.
