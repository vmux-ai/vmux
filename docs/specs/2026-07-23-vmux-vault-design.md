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
- installed agents, extensions, and LSP servers
- logs, services, generated shell integration, downloads, and staging

Managed worktrees remain under `~/.vmux/worktrees` and are Git-ignored. Moving linked worktrees requires repairing Git's absolute administrative paths and is deferred.

## Git workflow

The side sheet and dedicated Vault page expose setup and status.

Setup is a progressive three-step flow:

1. Pick remote storage: GitHub, Google Drive, Dropbox, or OneDrive.
2. Connect the account. GitHub opens GitHub.com device authorization in a new vmux stack, copies and displays the one-time code, then waits for approval through `gh`; cloud folders use the provider's signed-in desktop sync client.
3. Create a new remote location or choose an existing repository/folder.

- Create a GitHub repository. Default name: `vmux-vault`.
- New repositories are private by default; public is an explicit option with a warning.
- Connect an existing GitHub repository or Git URL.
- Sync stages and commits Vault changes, fetches, rebases, and pushes.
- Rebase conflicts abort without modifying remote history.
- Existing Vault repositories become the base and local files are replayed on top.
- Unrelated repositories that do not match the Vault layout are rejected rather than overwritten.

## Multiple Vaults

vmux follows the folder-and-switcher model: every Vault is an independent local folder with its
own settings, Knowledge, tool manifest, dotfiles, encryption key, and remote. A local Vault connects
to at most one Git repository or cloud folder. The same remote Vault can be opened on many devices.

The Vault switcher stores only local paths and the active Vault identifier in Application Support.
Removing a Vault from the switcher never deletes its local folder or remote. New local Vaults default
to `~/.vmux/vaults/<name>/`, but users can open any folder as a Vault. Encrypted staging repositories
remain under `~/Library/Application Support/Vmux/vaults/<id>/repository` and never mix with plaintext
Vault content.

Switching Vaults reloads settings, Knowledge, tools, and dotfiles as one unit. The first version may
restart vmux to guarantee that no state from the previous Vault survives. Existing single-Vault
installs migrate their authored `~/.vmux` content into a named Vault without moving runtime data.

## Encryption and device unlock

Vault files use a random 256-bit master key and AES-256-GCM. The master key is cached in the local
system key store and can also be wrapped by one or more passkeys using the WebAuthn PRF extension.
Each passkey derives a credential-specific wrapping key; the passkey private key and PRF output are
never written to Git.

The bundled Vault page redirects to the locally served `https://vault.vmux.ai` origin so Chromium
and passkey providers can run a standards-compliant WebAuthn ceremony without loading remote page
code. A fetched Vault with no local key remains connected but locked. Selecting a registered
passkey unwraps the master key, validates the encrypted snapshot, stores the key locally, and then
materializes `~/.vmux`.

Remote layout:

```text
vault.ron
index.enc
objects/<path-hmac>
keys/passkeys/<credential-hash>.ron
```

Passkey recipient files contain only a public credential identifier and an AES-GCM-wrapped master
key. Multiple Bitwarden, Apple Passwords, Google Password Manager, or other PRF-capable passkeys can
wrap the same master key. Providers without PRF support are rejected for encryption rather than
silently creating an unusable recipient.

GitHub authentication and repository creation use the installed `gh` CLI with a vmux-owned `GH_CONFIG_DIR` in Application Support. The browser authorization is separate from the user's global `gh` login. Git credentials remain outside the Vault.

## Safety

- vmux maintains required ignores for retired/generated directories that may remain during migration.
- Public repository creation rejects literal MCP credential fields known to contain secrets.
- Browser profiles, cookies, logs, recordings, and installed packages are outside the Vault.
- vmux never force-pushes or merges histories.

## Migration

Startup merges legacy generated directories from `~/.vmux` into the active build's Application Support directory without overwriting existing files. Managed worktrees are excluded.
