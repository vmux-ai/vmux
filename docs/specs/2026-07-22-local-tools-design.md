# Local Tools Design

Date: 2026-07-22
Status: Implemented

## Summary

Tools is vmux's profile-agnostic inventory and desired-state layer for local tools. It combines
Homebrew formulae and casks, global npm packages, ACP agents, MCP servers, language tools, and
Stow-style dotfiles in one side-sheet tree and one `vmux://tools/` manager.

Tools delegates package installation to each native package manager. It does not copy Homebrew
or npm packages into a vmux-owned store. ACP and language tools continue using their existing
vmux-managed receipt stores.

## Decisions

- Product name: **Tools**. Existing service, ACP, and LSP registries retain their domain names.
- Source of truth: `~/.vmux/tools/tools.toml`.
- Dotfile source: `~/.vmux/tools/dotfiles/<package>/...`.
- Tools is profile-agnostic. Browser/runtime profiles do not own machine configuration.
- Installed state and desired state remain distinct. Discovered packages are never automatically
  written to the manifest.
- “Manage” explicitly adopts an installed package into `tools.toml`.
- “Import” copies desired state from existing manifests without modifying the source file.
- Tools-managed MCP servers are injected into vmux-launched Claude, Codex, Vibe, and ACP agents.
- Missing desired packages remain visible and installable.
- Scans run asynchronously and only when dirty. External update checks run on explicit refresh,
  avoiding idle polling and package-manager network work during startup.
- No new workspace crate. Shared wire types live in `vmux_core`; manifest and dotfile primitives
  live in the lightweight `vmux_profile` crate.

## Storage

```text
~/.vmux/tools/
  tools.toml
  dotfiles/
    git/
      .gitconfig
    nushell/
      .config/nushell/config.nu
```

Example manifest:

```toml
version = 1

[packages]
homebrew-formula = ["fd", "ripgrep"]
homebrew-cask = ["ghostty"]
npm = ["typescript"]
acp = ["claude-acp"]
lsp = ["rust-analyzer"]

[mcp.servers.docs]
transport = "http"
url = "https://example.com/mcp"

[mcp.servers.local]
transport = "stdio"
command = "npx"
args = ["-y", "local-mcp-server"]

[dotfiles]
packages = ["git", "nushell"]
```

The manifest is created only after an explicit management action. Merely opening Tools never
seeds default or empty configuration.

Existing `~/.vmux/registry/registry.toml` and its dotfiles tree migrate in place to the Tools paths
on first access. Migration stops if both old and new roots exist, avoiding silent overwrites.

## Providers

| Provider | Discovery | Mutation |
| --- | --- | --- |
| Homebrew formulae | `brew list --formula --versions` | `brew install/upgrade/uninstall` |
| Homebrew casks | `brew list --cask --versions` | cask variants of the same commands |
| npm | `npm list --global --depth=0 --json` | global install/update/uninstall |
| ACP | Existing `~/.vmux/agents` receipts and ACP catalog | Existing ACP installer |
| Language tools | Existing `~/.vmux/lsp` receipts and Mason catalog | Existing LSP installer |
| MCP | Claude, Codex, Vibe, and explicit MCP configs | Tools-owned agent injection |
| Dotfiles | Tools package trees and link plans | Native Rust link engine |

Package-manager commands inherit the captured login-shell environment, preserving Finder/launchd
compatibility for Homebrew, Node, and user PATH entries.

## State Model

Every item has a provider-qualified identity, installed version, detail, desired-state flag, status,
and allowed actions.

Statuses:

- `available`: discovered dotfile package not enabled.
- `installed`: installed or fully linked.
- `outdated`: update available after explicit refresh.
- `missing`: declared but absent, or enabled links not yet applied.
- `conflict`: a dotfile target exists and is not the expected link.
- `failed`: reserved for persisted operation failures.

## UI

The side sheet renders a Tools card below Knowledge. The header shows installed, update, and
conflict counts. Categories expand into every discovered item. Selecting an item opens the full
manager in the active pane.

`vmux://tools/` provides:

- Search across all providers.
- Categorized package rows.
- Install, update, uninstall, manage, forget, link, and unlink actions.
- Add-package controls for Homebrew, npm, ACP, and language tools.
- Import controls for Brewfile, package.json, installed ACP/LSP receipts, MCP configs, and existing
  Stow roots.
- Dotfile adoption by package name and path.
- Refresh and declarative Apply actions.

The page reuses the shared manager components used by the language-tool and extension managers.

## Dotfile Engine

Dotfile package layout follows GNU Stow's directory convention, but linking is implemented in Rust.
Each leaf file maps from `dotfiles/<package>/<relative-home-path>` to `$HOME/<relative-home-path>`.
Directories are created as needed; entire shared directories such as `~/.config` are never replaced.

Safety rules:

- Links are relative.
- Existing regular files and foreign symlinks are conflicts.
- Apply plans every enabled package before mutating anything.
- Package apply rolls back links created during a failed operation.
- Multi-package Apply rolls back links created by earlier packages if a later package fails.
- Unlink removes only links that resolve to the expected Tools source.
- Adopt accepts only regular files inside `$HOME`, moves the file into its package tree, creates the
  link, and rolls back if link or manifest persistence fails.
- Package names cannot contain separators, parent components, or hidden-path prefixes.

## CLI

`vmux tools` exposes the filesystem-owned dotfile layer without launching the desktop app:

- `vmux tools status`
- `vmux tools apply`
- `vmux tools adopt <path> --package <name>`
- `vmux tools unlink <package>`

Package-manager actions remain in the desktop backend, where ACP/LSP installers and progress state
already live.

`vmux tools import` supports file-owned imports without launching the desktop:

- `vmux tools import homebrew <Brewfile>`
- `vmux tools import npm <package.json>`
- `vmux tools import mcp [config]`
- `vmux tools import dotfiles [stow-root]`

Import merges into existing desired state. Brewfile formulae and casks retain separate providers;
npm imports runtime, development, and optional dependencies; MCP import normalizes stdio, HTTP,
and SSE definitions from Claude JSON and Codex/Vibe TOML. Dotfile import copies complete package
directories into Tools ownership, rejects symlinks and collisions, and leaves the source tree
untouched.

## Implementation

- `vmux_profile::tools`: manifest, migration, link planning, apply, unlink, adopt.
- `vmux_core::tools`: shared Tools DTOs and bin-event requests.
- `vmux_desktop::tools`: scanners, actions, asynchronous state, page manifest.
- `vmux_layout::tools_page`: full manager page.
- `vmux_layout::page`: side-sheet Tools tree.
- `vmux_cli::commands::tools`: headless dotfile commands.

## Validation

- Manifest round-trip and normalization.
- Dotfile plan, apply, unlink, conflict blocking, adoption, and rollback behavior.
- Homebrew inventory parsing.
- Desired-but-missing package projection.
- Action policy for managed and unmanaged packages.
- Brewfile, package.json, MCP JSON/TOML, and Stow-root import behavior.
- Tools-managed MCP projection into CLI and ACP launch configuration.
- Native desktop build plus wasm page build.
