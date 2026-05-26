# Agent Instructions

## Communication Style

Use caveman mode. Terse, direct, no filler. Execute first, talk second. No meta-commentary, no preamble, no postamble. Code speaks.

## Skills

Use superpower. Invoke relevant skills BEFORE any response or action. Even a 1% chance a skill might apply means invoke it.

## Pre-commit Checks

NEVER commit or push without running fmt + clippy + test on the **changed crates only** (not the whole workspace) and confirming they pass.

Only run fmt, clippy, and test at the end of the task, after all intended edits are complete and immediately before committing, pushing, or opening a PR. Do not run cargo fmt, cargo clippy, cargo check, cargo test, or other compiler checks during the edit loop unless the user explicitly asks for an early check.

The `scripts/changed-crates.sh` script computes the set: crates whose files changed, plus crates whose tests `include_str!` from changed paths.

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)

for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Run `make setup-hooks` once to install the pre-push hook that runs these checks automatically.

If a change ripples into a downstream crate that is NOT in the changed set, lint/test that crate too.

The `website/` directory is its own cargo workspace (separate `Cargo.lock`). When `website/**` changes, run fmt + clippy from inside `website/` against `wasm32-unknown-unknown`:

```bash
cd website
cargo fmt -- --check
env -u CEF_PATH cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings
```

There is no host-runnable test target for `vmux_website` (it builds for `wasm32-unknown-unknown`); skip `cargo test` here unless wasm-bindgen-test is wired up.

If any check fails, fix the issue before committing. Do not push broken code.

## Platform-Specific Code

This project targets macOS (primary) and Linux (CI). When adding imports or code that uses platform-specific APIs (CEF, winit, AppKit), always add appropriate `#[cfg(...)]` gates. Run `cargo fmt` after adding cfg-gated imports -- rustfmt reorders them.

## Rules

- Do not add comments to code.
- Do not use mod.rs files. Use the filename-based module pattern (e.g. `layout.rs` + `layout/` directory).
- When configuring a Bevy `App` in plugins or tests, chain consecutive `App` builder calls in one expression (e.g. `app.add_plugins(...).init_resource::<T>().add_systems(...);`) instead of separate `app.*;` statements. Do not chain `app.world()`, `app.world_mut()`, `app.update()`, or control-flow-dependent mutations.
- Prefer Bevy system + message integration over direct helper-function calls for cross-module behavior. Register message types and systems in plugins/tests, send typed messages, run schedules, and assert on resulting ECS state/messages instead of bypassing production flow with ad hoc helpers.

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

## Documentation

- Save design specs to `docs/specs/YYYY-MM-DD-<topic>-design.md` (not `docs/superpowers/specs/`).
- Save implementation plans to `docs/plans/YYYY-MM-DD-<feature-name>.md` (not `docs/superpowers/plans/`).
- Delete the plan file once the plan is fully implemented.

## Git

Always prefer `git rebase` over `git merge` when updating branches. Use `git push --force-with-lease` after rebasing.

## Before Pushing / Opening PRs

**Mandatory**: Run fmt + clippy + test on the **changed crates only** before every `git push` or PR creation. Do not push or open a PR if any check fails. Fix all errors first.

These checks are final-gate checks. Finish edits first, then run them once before push/PR/commit. If they fail, fix the issue and re-run the changed-crate loops.

Use `scripts/changed-crates.sh` (see Pre-commit Checks above) to compute the changed-crate set and run the three loops. The repo-wide `make lint` / `make test` targets still exist (they iterate every workspace package) but are slow and intended for periodic full sweeps, not per-push validation.

```sh
make lint-fix  # auto-fix on every workspace package: runs fmt + clippy --fix
```

If formatting fails, run `make lint-fix` to auto-format, then re-run the changed-crate loops.
