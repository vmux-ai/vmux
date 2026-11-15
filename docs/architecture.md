# Vmux architecture

The guides are now organized by [UX and DX](README.md).

Read [How Vmux works](dx/architecture.md) for composition, ownership, and runtime boundaries, [Make a change](dx/feature-development.md) to route a change, or [Getting started](dx/getting-started.md) for the development workflow.

Plugin types use descriptive names ending in `Plugin`, including private, platform, DX and test
plugins. Type aliases and re-exports preserve that naming. The DX policy tests enforce it in
the pre-push hook and CI across crates, examples, website and scripts, resolving framework and
type aliases. Vendored types keep their upstream names.

### ECS ownership boundaries

The [DX architecture guide](dx/architecture.md#ecs-ownership-boundaries) describes the repository policy's bounded adapter contracts and canonical ownership model.

Main publishes dependency caches and workspace compiler artifacts for ordinary PRs to reuse.
PR-only release profiles retain isolated branch caches. Cache keys include compiler, flags,
target, profile, and manifest identity; packaged apps and distribution archives are excluded.
Source timestamps are restored only when tracked contents, modes, and directory membership
match. Changed inputs and Git build identity are invalidated, tracked symlinks invalidate
compiler fingerprints, and cache paths reject symlink parents. Each cache keeps its own
restore receipt so another target cannot lower the timestamps saved for its artifacts.
Snapshots from the earlier format are invalidated.
Action state retains cache inputs and the receipt until successful job teardown, which
records the source snapshot before cache upload. Retained main jobs warm their compiler
profiles; ordinary PRs do not run native builds or need a separate macOS check profile.

Cargo execution owns its environment preparation, optional compiler-cache probes, process, and seed publication in registered ECS stages. Probe errors select the uncached path; cancellation still drains the process before returning. Seed inputs are canonical file entities, ordered for Git blob hashing so existing cache keys remain compatible. Successful non-CI build commands publish the seed marker through an exclusively created staging file and rename while retaining the target lease. Failed commands publish no seed, and every exit restores the accepted incremental setting.

Compiler-cache selection respects the command's incremental mode. VM helpers contribute their hypervisor entitlements to signature inputs and participate in readiness verification. Bundle copies replace staged files atomically, so an already-open executable retains its original contents through rebuilding.

Provider snapshots open the source once, validate that handle, and copy at most 64 MiB with cancellation checks. A private staging file is published only after copying succeeds. Failed or canceled copies remove partial files before their target and temporary loans are released.

Development command admission captures configuration before target selection and tooling preparation. Registered systems own phase changes and native operation ordering; their query capabilities perform synchronous lookup, selection, and scoped state mutation. Native format conversion produces plist bytes from accepted inputs without running a workflow. IO routing discriminators belong to their canonical operation components, and the unchanged compatibility schema remains migration debt until its last callers are removed.

Packaging waits for process observations, publication journals, and keychain restoration to drain. The reactive CLI sleeps while those children are pending and resumes from their completion wakes. Bundle verification requires the VM helper as well as the desktop, service, and CLI binaries before publication.

`vmux_feature` owns global boolean release decisions. Provider roots compose environment `Name`,
`Revision`, generation and lease facts. Published values are individual children; definitions
project `Current` and emit typed `Changed` messages. `Receive` owns a temporary versioned JSON
document, which is materialized before consumers evaluate it. Missing, expired and failed input
uses the definition's default; Account cloud linking declares false. Malformed input cannot
replace accepted content, and older revisions or context completions cannot replace newer state.
Configuration content is immutable within a revision; lease renewal may change its timestamps.
Disabling the default `ecs` Cargo feature exposes only the serialized snapshot contract, so
Cloud readers can compile it without Bevy, Profile, page or native UI dependencies.

The optional `HttpPlugin` composes `FeaturePlugin`, native `ClockPlugin` sampling and the shared bounded job lane.
`Endpoint` selects HTTPS `/v1/flags/snapshot`; loopback HTTP exists for integration tests.
Requests capture provider generation and one conditional validator. Registered systems own
admission, cancellation, validation and jittered polling. Only accepted content installs its
ETag, and a 304 preserves the existing validity deadline. Failure closes admission and requests
a fresh document on retry. The HTTP adapter limits requests to five seconds and responses to
64 KiB. A provider-owned native timer binds the host's `JobWake` for polling and lease expiry;
provider removal cancels timers and invalidates outstanding jobs. Bevy fixtures drive production
plugins without launching a GUI. This foundation does not migrate manifests or enable a product
capability; application composition and the private Cloud Control remain separate consumers.

Native extension bridge initialization translates already-granted WebExtension permission
strings into immutable bridge registrations before CEF starts. Host-pattern parsing preserves
those grants; the catalog remains responsible for granting and persisting access.
