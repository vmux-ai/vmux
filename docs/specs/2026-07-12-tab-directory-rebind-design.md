# Dynamic Tab Directory Rebinding

> Superseded by `2026-07-13-tab-owned-workspace-design.md`. The later design expands rebinding to
> cross-repository edits and defines the tab-owned workspace contract.

## Problem

When vmux creates a worktree for an agent, it stores that checkout in `Tab.startup_dir` and marks
the tab with `TabWorktree`. If the agent later works from another checkout of the same repository,
vmux continues to display and use the original managed worktree. File previews reflect the new
checkout, but the sidebar path and newly spawned terminals remain stale.

The running agent process cannot reliably provide its current working directory. Codex preserves
its original session directory even when its tool calls use files from another checkout. The
absolute paths already delivered by `FileTouched` are the reliable signal available to vmux.

## Goals

- Keep the tab directory synchronized with the checkout used by the agent's file operations.
- Update the sidebar path and git status through the existing tab-boundary flow.
- Use the rebound directory for terminals and commands spawned later in the tab.
- Preserve the old checkout and its branch; rebinding must not delete worktrees.
- Ignore paths outside the tab's repository.

## Non-goals

- Changing the operating-system working directory of an already running agent process.
- Moving existing terminals to another directory.
- Deleting, pruning, or otherwise managing the checkout that the tab leaves.
- Inferring directory changes from arbitrary process inspection or polling.

## Design

### Repository identity

Add a git helper that resolves the absolute common git directory for a path. Main and linked
worktrees of one repository share this directory, while unrelated repositories do not.

Directory rebinding compares the common git directory of the tab's current `startup_dir` with the
common git directory of the touched file's checkout. Missing paths, non-git paths, and git errors
produce no rebind.

### Observation flow

Introduce a typed layout message carrying a tab entity and an observed absolute file path. The
agent file-touch system already resolves the agent anchor to its pane. It will also resolve the
ancestor tab and send this message for read and edit touches without changing the existing file
preview behavior.

The layout worktree system consumes the message:

1. Resolve the touched path to its checkout root with `git rev-parse --show-toplevel`.
2. Resolve repository identities for the current tab directory and observed checkout.
3. Ignore the observation when the identities differ or either side cannot be resolved.
4. Set `Tab.startup_dir` to the observed checkout root when it differs from the current value.
5. Remove `TabWorktree` when leaving the vmux-managed checkout because its ownership metadata no
   longer describes the active tab directory.

The previous checkout remains registered with git and untouched on disk. Later observations from
another checkout of the same repository may rebind the tab again, including back to the main tree.

### Spawn behavior

New terminal stacks already resolve their startup directory from the tab override. Agent `run`
terminal creation currently prefers the agent's immutable launch directory. Change that path to
resolve the ancestor tab's current directory first, then retain the existing agent-launch and space
fallbacks.

This makes future terminals and agent-issued commands follow the rebound tab without restarting or
mutating existing processes.

Automatic run-terminal reuse excludes terminals whose launch directory differs from the current
tab directory. The old terminal remains visible, while the command opens a new terminal rooted in
the rebound checkout. Explicit requests targeting a particular terminal remain explicit.

### UI and persistence

No new UI event is needed. Mutating `Tab.startup_dir` changes the payload produced by the existing
tab-boundary emitter, which refreshes the displayed path, branch, worktree badge, and git status.

`Tab` is already saved, so the rebound directory persists with the tab. Removing `TabWorktree`
prevents stale managed-worktree metadata from returning after restart.

### Error handling

Rebinding is best-effort. Invalid paths, deleted files, non-git directories, and git command
failures leave tab state unchanged. File preview behavior continues even when rebinding fails.

## Testing

- Git tests prove the same common-directory identity for a repository's main and linked worktrees
  and different identities for unrelated repositories.
- Layout system tests send the typed observation message and verify:
  - a managed worktree rebinds to another checkout of the same repository;
  - `TabWorktree` is removed after leaving the managed checkout;
  - a tab can rebind repeatedly between same-repository checkouts;
  - unrelated repositories, non-git paths, and missing paths are ignored.
- Agent integration tests send `FileTouched` through the registered systems and verify the ancestor
  tab receives a directory observation while file-follow behavior remains intact.
- Agent command tests verify a new `run` terminal uses the rebound tab directory instead of the
  agent's original launch directory.
- Agent command tests verify automatic reuse rejects a terminal launched from the previous tab
  directory.
