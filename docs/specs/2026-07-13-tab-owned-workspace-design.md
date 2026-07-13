# Tab-Owned Workspace Directory

## Problem

The tab sidebar, agent launch directory, and future run terminals can disagree about where work is
happening. A new tab may have no `Tab.startup_dir`, so the sidebar resolves the current
space/global/default setting while agent commands fall back through a different path. This makes
the visible directory stale or misleading even though commands execute elsewhere.

A tab needs one explicit, persisted workspace directory. Every stack in that tab shares it, and
the sidebar must show the same directory future agents and terminals will use.

## Goals

- Give every new tab an explicit workspace directory at creation time.
- Keep the displayed directory equal to the directory used for future agents and terminals.
- Rebind the tab from agent file activity when that activity strongly indicates a workspace
  switch.
- Preserve the directory across settings changes and application restarts.
- Preserve old worktrees and existing processes when the tab moves.

## Non-goals

- Following the focused stack independently from the rest of the tab.
- Treating a plain terminal `cd` as a tab workspace change.
- Changing the working directory of running agents or terminals.
- Polling process working directories.
- Deleting or pruning worktrees that a tab leaves.

## Directory Ownership

`Tab.startup_dir` is the persisted source of truth for the tab workspace. It is no longer merely an
optional override after tab creation.

When creating a tab, vmux resolves the directory through the existing chain:

1. Explicit directory supplied by the tab-creation request.
2. Active space override.
3. Global terminal startup directory.
4. Per-space default under `~/.vmux/spaces/<space-id>`.

The resolved existing directory is stored immediately in `Tab.startup_dir`. Later settings changes
affect newly created tabs, not existing tabs. Restored legacy tabs without a stored directory are
materialized once from the same chain and persisted.

The sidebar reads the stored tab directory. Agent creation, terminal creation, and automatic
`vmux run` placement also resolve from that same stored value. Existing running processes remain in
their original directories.

## Rebinding Policy

Agent file activity carries a path and a `Read` or `Edit` classification. `Edit` includes writes,
deletes, moves, and patch operations.

| Current tab | Observed activity | Result |
| --- | --- | --- |
| Same Git repository, same checkout | Read or Edit | No change |
| Same Git repository, different checkout/worktree | Read or Edit | Rebind to observed checkout root |
| Different Git repository | Read | No change |
| Different Git repository | Edit | Rebind to observed repository root |
| Current directory is non-Git, observed path is Git | Read | No change |
| Current directory is non-Git, observed path is Git | Edit | Rebind to observed repository root |
| Observed path is non-Git | Read or Edit | No change |

Reads may inspect dependencies or reference repositories, so cross-repository reads are weak
evidence. Editing another repository is strong evidence that the agent moved its workspace.

For same-repository transitions, Git common-directory identity distinguishes linked worktrees while
preventing nested unrelated repositories from being mistaken for the current checkout.

## Data Flow

1. CLI hooks and ACP tool updates produce `FileTouched` with path and touch kind.
2. The agent system resolves the anchor to its ancestor tab and emits `TabDirectoryObserved` with
   the tab entity, path, and kind.
3. The layout rebind system runs before same-frame agent commands.
4. It resolves the observed repository root and compares it with the current tab workspace using
   the policy above.
5. On rebind, it updates `Tab.startup_dir`, removes stale `TabWorktree` metadata, and leaves the old
   checkout untouched.
6. The existing persistence and tab-boundary systems save and display the updated directory.
7. Later agent and terminal spawns use the stored tab directory.

CLI coverage remains tool-dependent: Claude and Vibe report native file reads and edits; Codex CLI
reports native edits but shell-based reads do not produce file-touch events. ACP agents participate
when their tool calls include file locations and a file-affecting tool kind.

## Scheduling

Tab directory materialization and rebinding must complete before systems that launch agents or
terminals from the tab. A `FileTouched` and `Run` received in one frame therefore use the rebound
directory for the run terminal.

## UI and Persistence

The existing tab-boundary event remains the UI contract. Its effective path comes from the stored
tab directory, so the card, Git branch, and worktree state update together.

Changes to `Tab` and `TabWorktree` mark persistence dirty. Rebinding survives restart, and removing
stale managed-worktree metadata prevents the old branch label from returning.

## Error Handling

Directory materialization falls through invalid configured directories to the next valid source.
Rebinding is best-effort: missing paths, non-Git observations, and Git errors leave the tab
unchanged. File preview behavior is independent and continues when rebinding cannot be performed.

## Testing

- Tab creation tests verify explicit, space, global, and default directories are stored.
- A settings-change test verifies an existing tab keeps its frozen directory.
- Legacy restoration tests verify a missing tab directory is materialized and marked dirty.
- Layout tests verify same-checkout no-op and same-repository worktree rebinding for reads and edits.
- Layout tests verify cross-repository reads are ignored and cross-repository edits rebind.
- Layout tests verify non-Git current directories only move on edits into a Git repository.
- Nested linked-worktree and nested unrelated-repository tests preserve Git boundary behavior.
- Same-frame tests verify observation, rebind, then agent run ordering.
- Agent tests verify future run terminals use the tab directory and stale-cwd terminals are not
  automatically reused.
- Persistence tests verify tab-directory changes and `TabWorktree` removal mark the store dirty.
- CLI hook and ACP projector tests verify touch kinds and paths reach the shared observation flow.

