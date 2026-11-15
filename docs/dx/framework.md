# Vmux Framework

Vmux Framework reuses the architecture's feature capabilities in another application. It is optional: Vmux's browser, IDE, and ACP harness work with existing projects in any stack.

## Composition

Use `vmux_app` as the public composition facade. Select capabilities and supply the host's runner, IO, permissions, storage, and platform adapters. A renderer-free core should not acquire CEF, windowing, or product UI because Vmux also uses it.

Cargo features choose compiled capabilities; the builder chooses installed plugins. Native plugins are linked Rust crates. The plugin, contracts, manifest, and assets form a feature's public contribution. Dynamic Rust loading and a generic prebuilt product host are not supported capabilities.

## Start from a real consumer

Inspect the independent [framework consumer](https://github.com/vmux-ai/vmux/tree/main/examples/framework-consumer) and [host consumer](https://github.com/vmux-ai/vmux/tree/main/examples/host-consumer). They demonstrate current APIs without inheriting the repository's private environment or relay dependencies.

```sh
vmux framework-consumer
vmux framework-consumer -- --git
```

Path and immutable Git modes run the same behavioral checks. Use the examples and selected capability's public types rather than copy an old builder signature from prose.

## Contracts and boundaries

The host remains authoritative. UI renders selected projections and sends intent. Commands, agents, and tools reach the same feature operations. Adapters preserve contract meaning while translating framing and lifecycle.

A custom host supplies product identity, paths, assets, and native permissions. It does not inherit Vmux credentials or data because a display name matches. Omit or refuse unsupported capabilities at the boundary.

## Compatibility

Pin all Vmux dependencies to the same reviewed main revision or release tag and use its required Bevy version. Matching names across revisions do not make ECS types compatible.

The SDK is consumed from pinned Git revisions, not a stable crates.io distribution. Platform availability is capability-specific. Product compositions do not promise adapters for every target. Follow storage migrations and protocol negotiation when changing persisted or wire contracts.

Read [Architecture](architecture.md) and [Feature development](feature-development.md) for the model and flow to preserve.
