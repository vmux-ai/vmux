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
