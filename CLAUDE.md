# Rules

- Do not add comments to code.
- Do not use mod.rs files. Use the filename-based module pattern (e.g. `layout.rs` + `layout/` directory).
- **Never make changes directly on `main`.** Always use a worktree branch.

## Mandatory: Lint and Test Before Every Commit

**You MUST run these commands and confirm they pass before every `git commit` or `git push`.** Do not skip this step. CI will reject PRs that fail these checks.

```sh
make lint      # cargo fmt --check + clippy -D warnings (excludes vendored patches)
make test      # cargo test --workspace --exclude bevy_cef_core
make lint-fix  # auto-fix: cargo fmt + clippy --fix
```

### What CI checks

| CI Job         | What it runs                                                   |
|----------------|----------------------------------------------------------------|
| **Lint**       | `cargo fmt --check` + `cargo clippy --all-targets -D warnings` per non-patch crate |
| **Test**       | `cargo test --workspace --exclude bevy_cef_core`               |
| **Website**    | `dx build --platform web --release` in `website/`              |

### Fixing failures

- **Auto-fix:** run `make lint-fix` to auto-format and apply clippy suggestions.
- **Manual fix:** if `make lint-fix` can't resolve a clippy warning, fix it by hand.
- **If `make lint` takes too long**, you can target a single crate: `cargo clippy -p <crate> --all-targets -- -D warnings`

### Workflow

1. Make your changes.
2. Run `make lint-fix` — auto-fix formatting and clippy issues.
3. Run `make lint` — confirm everything passes.
4. Run `make test` — fix any failures.
5. Only then `git commit` / `git push`.

**Never commit with known lint or test failures.** If you are unsure whether your changes compile cleanly, run `make lint` again.

## Worktrees

When working on a Linear issue, always use a git worktree for isolation:

1. Create worktree: `git worktree add .worktrees/<short-name> -b <branch-name>`
2. Work inside the worktree directory.
3. When done, merge to main, then remove: `git worktree remove .worktrees/<short-name>`
4. Remember: if the worktree is deleted while your shell is inside it, `cd` back to the repo root — `../..` won't work.

Worktree directory: `.worktrees/` (already in `.gitignore`).
