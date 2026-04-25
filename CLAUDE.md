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
