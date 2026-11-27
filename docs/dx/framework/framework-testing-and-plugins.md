# Testing and plugin authoring

Because domain logic, UI projections, and IO adapters compose separately in ECS, you can test feature state transitions, permissions, retries, and cancellation deterministically without launching a browser or window.

## Deterministic headless testing

- **Windowed plugins without an OS window**: construct `WindowHostConfig::new("Test").page(&PAGE).headless()` or `PageHostPlugin::headless()`. All page mounting, permission checks, bounds sync, `UiStatePlugin<S>` emissions, and `UiIpcEventPlugin<E>` dispatches run synchronously on `app.update()`.
- **Injecting UI events**: write `UiReceiveRaw::from_event(page, url, authority, &MyEvent { ... })` to exercise permission gating, stale-revision rejection, and `UiIpcBlocked` handling.
- **Deterministic time**: insert `vmux_app::ecs::time::Clock::fake(unix_millis)` so debounce, deadline, retry backoff, and rate-limiter tests never sleep or flake.
- **Controllable IO adapters**: supply fake completion messages (`LoadCompleted`, `WriteResult`, `StoreWriteResult`) against accepted operations to test failure, cancellation, and late/stale responses.

Run the external SDK compatibility suites from a Vmux checkout:

```sh
vmux dx test sdk
vmux dx test sdk --host
vmux dx test sdk --git
```

## Authoring external feature plugins

Enable `vmux_app/manifest` to declare reusable feature plugins with `feature.ron` metadata:

```ron
(
    metadata: Some((
        id: "example.notes",
        version: "1.0.0",
        dependencies: [(id: "example.storage", version: "^1.0")],
        capabilities: ["storage.files"],
        permissions: ["data.write"],
        targets: ["headless", "daemon"],
        sdk: Some(">=0.0.0, <1.0.0"),
        bevy: Some("0.19"),
        lifecycle: (startup: ["Update"], shutdown: ["OnExit"]),
    )),
    pages: [(url: "notes://", title: "Notes", permissions: ["PageReady"])],
    commands: [(id: "notes.rename", label: "Rename", group: "Notes")],
    tools: [(name: "notes.inspect", description: "Inspect notes", input_schema: (type: Object))],
    cli: Some((commands: [(id: "notes.rename", name: "rename")])),
)
```

Register external manifests with `FeaturePlugin::<Namespace>::from_manifest(FeatureManifest::try_parse(source)?)` and declare the host context with `CompositionPlugin(Profile::new("daemon").permission("data.write"))`.

Before Startup, composition validation rejects:
- missing or cyclic dependencies
- incompatible SemVer ranges or Bevy/SDK versions
- duplicate feature IDs
- unsupported target profiles
- unavailable capabilities or host policy permissions

Permission metadata declares intent—it never grants OS access. Actual per-origin `UiEventPermissions` checks and native adapters remain authoritative. See [`examples/manifest/external_feature.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/manifest/external_feature.rs).

## Product identity and isolated paths

Enable `vmux_app/host` (`vmux_profile`) to give an independently branded product its own data, config, credential, and daemon socket directories controlled by `VMUX_HOST_NAME` and `VMUX_HOST_NAMESPACE`:

```dotenv
VMUX_HOST_NAME=My Studio
VMUX_HOST_NAMESPACE=com.example.mystudio
```

See [`tests/sdk/host/tests/host.rs`](https://github.com/vmux-ai/vmux/blob/main/tests/sdk/host/tests/host.rs) for path isolation and credential-boundary guarantees.
