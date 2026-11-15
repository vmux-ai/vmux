# Architecture

Vmux composes a browser, IDE, agent harness, service, and mobile client from feature plugins. A new capability should have one owner and one integration model across those surfaces.

## Applications as plugin graphs

A plugin installs a capability's components, messages, systems, schedules, manifests, and boundary adapters into an ECS world. Application roots select plugins and platform adapters; feature crates own their behavior.

```text
feature = state + systems + UI + commands + tools + contracts
runtime = feature plugins + platform adapters
```

Cargo features decide what can be compiled. Runtime composition decides which compiled plugins are installed. `vmux_app` supplies the shared facade; desktop, mobile, daemon, CLI, and MCP hosts provide their entry points, runners, and IO.

A feature does not install a sibling as a hidden dependency. Shared composition belongs at the application root. Split plugins for independent configuration, platform, reuse, or lifecycle boundaries.

## Design vocabulary

| Primitive | Responsibility |
|---|---|
| Crate | Own a capability and its contracts. |
| Plugin | Install the capability into a runtime. |
| Entity | Give independently addressed or removable work an identity. |
| Component | Store typed state or capabilities on that identity. |
| System | Perform an operation through declared ECS inputs and outputs. |
| Event or message | Carry transient intent or facts without calling another owner's internals. |
| UiEvent | Carry permission-checked UI intent to the host. |
| UiState | Project host-owned state into a page. |

The same type can cross UI, IPC, remote, CLI, or tool boundaries when it represents the same contract. Adapters change framing, not the domain model.

## Runtime and trust boundaries

The desktop owns visible layout, input, native integration, and its host ECS. The background service owns processes and sessions that outlive a view. Native pages render projections and emit intent; arbitrary web pages remain untrusted browser content.

Mobile is a remote client of supported host capabilities. Both endpoints connect outward through an opaque relay, and the inner encrypted session ends at the host. The public client owns this contract; relay admission, registry, and deployment internals belong to the relay repository.

Platform credentials stay at their designated native boundary. Children, pages, and tools receive scoped capabilities, not an unrestricted key broker or the host's state.

## State and ownership

Canonical product state lives on entities and components. Resources hold truly global facts. Reuse shared identity, name, activity, order, and relationship primitives before creating aggregates that repeat them.

Systems own selection, reconciliation, retries, and commits. They stay private to their registering plugin. Features communicate through typed data and explicit schedule ordering. UI snapshots and read indexes are derived from canonical state and do not become a second mutation authority.

Long work has identity and an observable lifecycle. Capture accepted inputs before IO, perform bounded blocking work outside the schedule, and adopt results only while their owner, generation, and operation remain current. Worker completion is not a domain commit. Cancellation and replacement prevent late results from changing a new owner.

### ECS ownership boundaries

An adapter translates an external format or performs bounded IO. It names its interface, canonical input/output, scope, lifetime, and cancellation or eviction. Handles, bytes, codec scratch, and read-only presentation caches may remain there; product policy and normalized mutable state may not.

`ecs-policy.ron` records item-specific contracts and frozen migration debt. DX policy checks validate declarations, production ownership references, and adapter provenance. Changed legacy code gains a real owner or boundary; the frozen baseline is not refreshed to approve it.

Repository checks, build adapters, and static website rendering run before or outside the product world. Their boundaries describe source analysis, build inputs, or presentation, not another product registry or workflow engine.

## Persisted and wire contracts

A feature owns storage semantics and migrations. Shared infrastructure supplies versioned records, atomic IO, and receipts; owning systems decide when current state commits. Stable identities and stored keys survive implementation changes. Incompatible data is migrated or explicitly refused before acceptance.

Local and remote use the same typed contracts where meaning is shared. Wire layout changes require the protocol's compatibility/version mechanism. Secrets and browser storage follow their designated credential/encryption owner.

## Platforms and crate layout

- `crates/app/`: product entry points and runtime adapters.
- `crates/feature/`: cohesive user capabilities, including state, UI, commands, IO, and contracts.
- `crates/util/`: reusable infrastructure and composition support.

Platform imports and plugins are gated at their boundary. Renderer-free cores remain usable without native browser/window dependencies. The `host` cfg alias describes compiled code, not a crate's directory.

## Invariants

Keep one feature owner, one canonical state model, and one shared contract per boundary. Compose runtimes from those owners. Treat web content as untrusted, make long work observable, and apply results only to the operation that accepted them.

Use [Feature development](feature-development.md) to route a change and [Vmux Framework](framework.md) to reuse a capability. Current type layouts and modules remain in code and tests.
