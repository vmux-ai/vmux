# Rules

- Do not add comments to code.
- Do not use mod.rs files. Use the filename-based module pattern (e.g. `layout.rs` + `layout/` directory).

## Worktrees

When working on a Linear issue, always use a git worktree for isolation:

1. Create worktree: `git worktree add .worktrees/<short-name> -b <branch-name>`
2. Work inside the worktree directory.
3. When done, merge to main, then remove: `git worktree remove .worktrees/<short-name>`
4. Remember: if the worktree is deleted while your shell is inside it, `cd` back to the repo root — `../..` won't work.

Worktree directory: `.worktrees/` (already in `.gitignore`).

## Documentation

- Save design specs to `docs/specs/YYYY-MM-DD-<topic>-design.md` (not `docs/superpowers/specs/`).
- Save implementation plans to `docs/plans/YYYY-MM-DD-<feature-name>.md` (not `docs/superpowers/plans/`).
- Delete the plan file once the plan is fully implemented.

## Before Pushing

Always run lint and test before pushing to catch CI failures locally:

```sh
make lint  # runs fmt --check + clippy -D warnings
make test  # runs cargo test --workspace
```
