# Managed Tab Worktrees

## Problem

Worktree isolation currently depends on an agent calling `create_worktree` after it has already
started. The running agent keeps its original working directory, sibling tool calls can run before
the worktree exists, and deleted checkouts silently lose their `TabWorktree` metadata on restart.
Worktrees are also created inside the repository, which requires mutating Git's local exclude file.

## Goals

- Give each Git-backed agent tab at most one managed worktree.
- Create the worktree before project file or terminal work begins.
- Reuse the same worktree for later agents, terminals, restarts, and MCP calls in the tab.
- Preserve the original project directory separately from the mutable execution directory.
- Recover a missing managed checkout from its persisted branch when possible.
- Accept ACP workspace-change metadata from agents that create or restore their own worktree.
- Allow agents to start immediately without a startup directory.
- Let the agent request a workspace only when the task needs project files.
- Preserve the same chat session and webview while selecting and activating a workspace.
- Never silently delete dirty or committed work.

## Lifecycle

Tabs remain cheap until an agent page opens. Tabs with a configured directory are prepared before
the agent page-open handler runs:

1. A tab already carrying `TabWorktree` keeps that checkout.
2. A tab already rooted in an external linked worktree keeps that checkout and records its project
   identity without creating another worktree.
3. A tab rooted in a normal Git checkout gets one vmux-managed worktree.
4. A non-Git tab continues in its existing directory.

A tab without a configured directory starts the agent in the user's home directory without
binding that directory to the tab. General questions and terminal tasks can proceed immediately.

The original directory is persisted in `TabWorkspace`. `Tab.startup_dir` remains the execution
directory used by new agents and terminals. `TabWorktree` remains the attached checkout metadata.

## Workspace Selection

When neither the active space nor global settings define a startup directory, a new tab keeps
`Tab.startup_dir` empty and starts its agent immediately with the user's home directory as a
temporary runtime cwd. The home directory is not persisted as the tab workspace.

The agent calls `choose_workspace` only when the request requires project files. The chat shows an
inline workspace-selection card above the composer instead of opening the native folder picker
immediately. The picker opens only after the user clicks `Choose folder`, keeping the agent's
request visible and making the system dialog intentional. Non-Git selections become `TabWorkspace`
directly. Git selections remain pending while the agent asks the user which branch to create. The
agent then calls `create_worktree` with the exact user-selected branch. Generated names such as
`Tab 2` are replaced by the selected folder name, and the branch name is sanitized only for the
managed checkout directory slug.

While a tab is unbound, vmux adds private host context to ACP prompts requiring `choose_workspace`
before any existing-project operation. The context explicitly forbids repository discovery under
the user's home directory and manual `git worktree add`, while still allowing general questions and
self-contained terminal demonstrations. A selected Git project receives a pending-worktree policy
until `create_worktree` completes.

Workspace activation mutates the existing tab and ACP session in place. The chat entity, webview,
transcript, routing session id, and agent process remain unchanged. Tab cwd and host-side ACP file
scope move to the selected directory or managed worktree.

## Storage

New managed worktrees live under:

```text
~/.vmux/worktrees/<repository-name>-<git-common-dir-hash>/<tab-slug>[-N]
```

The shared Git common directory provides stable repository identity across linked checkouts. A
global root avoids repository pollution and local `.git/info/exclude` mutations. Existing
repository-local worktrees remain valid and are not moved.

Automatically prepared managed worktrees keep dedicated `vmux/<slug>[-N]` branches. Conversational
workspace setup uses the exact branch name supplied by the user. Branch ownership stays one
worktree at a time. The persisted tab checkout path and branch make creation idempotent.

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

Configured Git tabs still activate before agent page opening. Empty tabs start immediately and
activate later through agent tools. Agent command batches process `choose_workspace` and
`create_worktree` before sibling commands; commands are skipped when worktree activation fails
rather than running in the project checkout.

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
- Opening the first agent page for a configured Git tab creates and uses one worktree; later agent
  pages reuse it.
- An agent tab without a configured startup directory starts immediately in the user's home
  directory and dispatches its first prompt.
- General prompts do not open the workspace picker.
- Selecting a Git workspace asks for a branch, creates a slugged checkout on that exact branch,
  and updates the existing tab and ACP session.
- Workspace activation preserves the original chat view, transcript, and routing session.
- Non-Git tabs do not create worktrees.
- Missing managed checkouts recover from their branch; invalid external checkouts remain marked.
- `create_worktree` is idempotent and ordered before sibling run commands.
- ACP worktree metadata validates, crosses the service protocol, and rebinds only the matching tab.
- Persistence saves project identity and worktree ownership without resetting existing stores.
