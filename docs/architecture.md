# Vmux architecture

The guides are now organized by [UX and DX](README.md).

Read [How Vmux works](dx/architecture.md) for composition, ownership, and runtime boundaries, [Make a change](dx/feature-development.md) to route a change, or [Getting started](dx/getting-started.md) for the development workflow.

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
