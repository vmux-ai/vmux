# Agent Instructions

## Communication Style

Use caveman mode. Terse, direct, no filler. Execute first, talk second. No meta-commentary, no preamble, no postamble. Code speaks.

## Skills

Use superpower. Invoke relevant skills BEFORE any response or action. Even a 1% chance a skill might apply means invoke it.

## Pre-commit Checks

CI runs fmt, clippy, and tests for PRs.

Run targeted tests during the edit loop when they support TDD, debugging, or behavior verification. Run broader local checks only when the user asks.

If a change affects an excluded patched CEF crate, run the appropriate package checks too.

If any check fails, fix the issue before committing. Do not push broken code.

## Debugging

When adding temporary diagnostics to investigate a bug, make logging unconditional (default-on) — never gate it behind an env var or flag the user must set. The user runs the normal build; logs must appear without extra setup. Strip every temporary diagnostic before committing the fix.

## Platform-Specific Code

This project targets macOS (primary) and Linux (CI). When adding imports or code that uses platform-specific APIs (CEF, winit, AppKit), always add appropriate `#[cfg(...)]` gates. Let rustfmt reorder cfg-gated imports.

## Rules

- Do not add comments to code.
- Never add or commit `.claude/*` files. They are local agent config, not project files.
- Do not use mod.rs files. Use the filename-based module pattern (e.g. `layout.rs` + `layout/` directory).
- When configuring a Bevy `App` in plugins or tests, chain consecutive `App` builder calls in one expression (e.g. `app.add_plugins(...).init_resource::<T>().add_systems(...);`) instead of separate `app.*;` statements. Do not chain `app.world()`, `app.world_mut()`, `app.update()`, or control-flow-dependent mutations.
- Prefer Bevy system + message integration over direct helper-function calls for cross-module behavior. Register message types and systems in plugins/tests, send typed messages, run schedules, and assert on resulting ECS state/messages instead of bypassing production flow with ad hoc helpers.
- **Never use `bevy::winit::UpdateMode::Continuous`.** It causes 100-200% idle CPU. Use `UpdateMode::Reactive` or `UpdateMode::reactive_low_power`. If input/scroll/animation lags, the fix is to route the missing wake source through `EventLoopProxy::send_event(WinitUserEvent::WakeUp)` — not to switch to Continuous. The CEF wake throttler (`MessageLoopWakePolicy` + `cef-wake-throttle` thread) already wakes the loop at display refresh rate when CEF schedules pump work. A `no_continuous_update_mode` test in `vmux_desktop` enforces this.

## Linear

When taking a Linear issue (e.g. "take VMX-XX"), immediately move it to **In Progress** before doing anything else — before creating a worktree, before reading code, before drafting a PR.

## Worktrees

**Never edit files on the main worktree.** All changes must happen inside a feature worktree. Before writing any code for a Linear issue:

1. Check if a worktree already exists: `git worktree list`
2. Create worktree if needed: `git worktree add .worktrees/vmx-<number> -b <branch-name>` — always name the worktree directory using the `vmx-<number>` convention matching the Linear issue (e.g., `.worktrees/vmx-88`).
3. `cd` into the worktree directory and make all edits there.
4. When done, merge to main, then remove: `git worktree remove .worktrees/<short-name>`
5. Remember: if the worktree is deleted while your shell is inside it, `cd` back to the repo root — `../..` won't work.

Worktree directory: `.worktrees/` (already in `.gitignore`).

## Merging

Before merging any PR:

1. **Check review comments.** Read all review feedback — CodeRabbit and human reviewers — and address or explicitly triage every item. A green status check is not enough; unresolved review comments must be handled first. **Reply to every CodeRabbit thread** — either reflect the fix in code (cite the commit) or comment a triage reason — so no thread is left dangling, then resolve them (e.g. `@coderabbitai resolve`).
2. **Check CI.** Confirm all required checks are green on the PR's head commit.

After merging, clean up: remove the worktree (`git worktree remove .worktrees/<name>`) and delete the branch (`gh pr merge --delete-branch` for the remote; delete the local branch too if it lingers).

## Documentation

- Save design specs to `docs/specs/YYYY-MM-DD-<topic>-design.md` (not `docs/superpowers/specs/`).
- Save implementation plans to `docs/plans/YYYY-MM-DD-<feature-name>.md` (not `docs/superpowers/plans/`).
- Delete the plan file once the plan is fully implemented.

## Git

Always prefer `git rebase` over `git merge` when updating branches. Use `git push --force-with-lease` after rebasing.
