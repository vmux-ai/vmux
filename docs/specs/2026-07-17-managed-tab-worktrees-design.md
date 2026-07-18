# Managed Tab Worktrees

## Problem

Worktree isolation currently depends on an agent calling `create_worktree` after it has already
started. The running agent keeps its original working directory, sibling tool calls can run before
the worktree exists, and deleted checkouts silently lose their `TabWorktree` metadata on restart.
Worktrees are also created inside the repository, which requires mutating Git's local exclude file.

## Goals

- Give each Git-backed agent tab at most one managed worktree.
- Create the worktree before launching the first agent in that tab.
- Reuse the same worktree for later agents, terminals, restarts, and MCP calls in the tab.
- Preserve the original project directory separately from the mutable execution directory.
- Recover a missing managed checkout from its persisted branch when possible.
- Accept ACP workspace-change metadata from agents that create or restore their own worktree.
- Allow tabs to exist without a startup directory and request a workspace inside the chat UI.
- Never silently delete dirty or committed work.

## Lifecycle

Tabs remain cheap until an agent page opens. Before the agent page-open handler runs, vmux checks
the tab directory:

1. A tab already carrying `TabWorktree` keeps that checkout.
2. A tab already rooted in an external linked worktree keeps that checkout and records its project
   identity without creating another worktree.
3. A tab rooted in a normal Git checkout gets one vmux-managed worktree.
4. A non-Git tab continues in its existing directory.

The original directory is persisted in `TabWorkspace`. `Tab.startup_dir` remains the execution
directory used by new agents and terminals. `TabWorktree` remains the attached checkout metadata.

## Workspace Selection

When neither the active space nor global settings define a startup directory, a new tab keeps
`Tab.startup_dir` empty. Browser-only tabs still open normally. Opening an agent page attaches the
chat UI in a workspace-required state and does not start an agent process.

The chat page asks the user to choose a folder. Canceling leaves the picker state intact. A valid
selection becomes `TabWorkspace`; Git-backed folders create the managed worktree before the
original agent URL is reopened, while non-Git folders run directly from the selected directory.
Generated names such as `Tab 2` are replaced by the folder name, producing branches such as
`vmux/dashboard` and collision suffixes such as `vmux/dashboard-2`.

## Storage

New managed worktrees live under:

```text
~/.vmux/worktrees/<repository-name>-<git-common-dir-hash>/<tab-slug>[-N]
```

The shared Git common directory provides stable repository identity across linked checkouts. A
global root avoids repository pollution and local `.git/info/exclude` mutations. Existing
repository-local worktrees remain valid and are not moved.

Managed worktrees keep dedicated `vmux/<slug>[-N]` branches. Branch ownership stays one worktree at
a time. The persisted tab checkout path and branch make creation idempotent.

## Recovery

On restore, vmux validates that the persisted execution directory is a checkout of the expected
repository and branch. If a vmux-managed checkout is missing but its source repository and branch
still exist, vmux recreates the checkout at the same path. Failed recovery keeps the metadata and
marks the tab unavailable instead of silently forgetting ownership.

Recovery only writes beneath the repository's hashed managed directory or to the exact stale path
already registered by Git for that branch. A branch registered to another worktree is never forced
into a second checkout. Restored tabs are reconciled one per frame, while validated runtime state
avoids repeating Git subprocesses for every later agent open.

## Ordering

Host-side creation occurs before agent page opening, so the agent process starts in the isolated
directory. Agent command batches process `create_worktree` before sibling commands; commands are
skipped when activation fails rather than running in the project checkout.

## ACP Workspace Updates

ACP `session_info_update` metadata may contain:

```json
{
  "worktree": {
    "name": "quiet-amber-wolf",
    "branch": "vibe/quiet-amber-wolf",
    "cwd": "/worktrees/quiet-amber-wolf",
    "workspaceCwd": "/repo"
  }
}
```

The service and GUI both require `cwd` to be a linked worktree of the same repository as
`workspaceCwd`, on the reported branch. The matching ACP session and ancestor tab update their
execution directory while preserving `workspaceCwd` as project identity. Canonical containment
checks prevent nested project symlinks from escaping the linked checkout.

## Cleanup

This change never automatically deletes a worktree containing uncommitted files or commits beyond
its base ref. Clean unreferenced retention cleanup can be added on top of the persisted ownership
model. Existing orphaned worktrees are left untouched.

## Testing

- Managed roots are global, repository-specific, and collision-safe.
- Nested project directories preserve their relative path in the new checkout.
- Opening the first agent page creates and uses one worktree; later agent pages reuse it.
- An agent tab without a configured startup directory shows the workspace picker and emits no
  agent spawn before selection.
- Selecting a Git workspace creates a slugged worktree, updates the tab, and resumes the original
  agent page open.
- Non-Git tabs do not create worktrees.
- Missing managed checkouts recover from their branch; invalid external checkouts remain marked.
- `create_worktree` is idempotent and ordered before sibling run commands.
- ACP worktree metadata validates, crosses the service protocol, and rebinds only the matching tab.
- Persistence saves project identity and worktree ownership without resetting existing stores.
