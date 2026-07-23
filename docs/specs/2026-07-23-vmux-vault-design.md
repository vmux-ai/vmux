# vmux Vault

## Summary

The vmux Vault is the portable, Git-backed `~/.vmux` directory. It contains user-owned settings, Knowledge, tool manifests, Brewfile, and dotfile sources. Runtime state and installed artifacts live in Application Support and are never committed.

## Storage boundary

Vault:

- `~/.vmux/settings.ron`
- `~/.vmux/knowledge/`
- `~/.vmux/tools/`
- user locale overrides and other explicitly authored configuration

Application Support:

- profiles, recordings, browser data, sessions, and layout state
- installed agents, extensions, and language tools
- logs, services, generated shell integration, downloads, and staging

Managed worktrees remain under `~/.vmux/worktrees` and are Git-ignored. Moving linked worktrees requires repairing Git's absolute administrative paths and is deferred.

## Git workflow

The side sheet and dedicated Vault page expose setup and status.

- Create a GitHub repository. Default name: `vmux-vault`.
- New repositories are private by default; public is an explicit option with a warning.
- Connect an existing GitHub repository or Git URL.
- Sync stages and commits Vault changes, fetches, rebases, and pushes.
- Rebase conflicts abort without modifying remote history.
- Existing Vault repositories become the base and local files are replayed on top.
- Unrelated repositories that do not match the Vault layout are rejected rather than overwritten.

GitHub authentication and repository creation use the installed `gh` CLI. Git credentials remain outside the Vault.

## Safety

- vmux maintains required ignores for retired/generated directories that may remain during migration.
- Public repository creation rejects literal MCP credential fields known to contain secrets.
- Browser profiles, cookies, logs, recordings, and installed packages are outside the Vault.
- vmux never force-pushes or merges histories.

## Migration

Startup merges legacy generated directories from `~/.vmux` into the active build's Application Support directory without overwriting existing files. Managed worktrees are excluded.
