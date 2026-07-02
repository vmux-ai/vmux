# ACP Registry Integration + Runtime Bootstrap — Design

**Date:** 2026-07-01
**Status:** Draft (for review)
**Ships in:** PR #216 (same PR, per direction).

## Problem

vmux is a general-audience browser; assuming the user has `npx`/Node is a developer assumption.
Today the ACP defaults spawn `npx -y <pkg>@latest`, which **silently fails without Node**. We also
hardcode only 3 agents in `settings.agent.acp`, with no discovery, no icons, no updates.

## Approach — adopt the official ACP Registry as the source of truth

The [ACP Registry](https://agentclientprotocol.com/rfds/acp-agent-registry) is a standardized,
client-agnostic catalog: one `registry.json` at
`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` (currently **37 agents**),
each entry carrying id/name/description/icon/version + a `distribution` telling clients how to
install and run it. Upstream auto-updates versions hourly. This replaces our hand-rolled per-agent
table, our hardcoded settings defaults, **and** the favicon hack — the registry provides all of it.

vmux becomes an ACP-registry client (like Zed/JetBrains): fetch the catalog, install on demand,
download+manage a Node or Python runtime **only when an agent needs one**, keep versions current.

### Registry schema (verified against live registry)

```json
{ "version": "1.0.0", "agents": [ {
  "id": "claude-acp", "name": "Claude Agent", "version": "…", "description": "…",
  "icon": "https://…/claude-acp.svg", "repository": "…", "license": "…",
  "distribution": {
    "binary": { "darwin-aarch64": { "archive": "https://…", "cmd": "./exe", "args": [], "env": {} }, … },
    "npx":    { "package": "@scope/pkg", "args": ["--acp"] },
    "uvx":    { "package": "pkg", "args": ["serve"] }
  } } ] }
```

- Distribution types: **binary** (per-platform archive → download/extract/run), **npx** (Node),
  **uvx** (PyPI via uv). Targets: `darwin-aarch64|darwin-x86_64|linux-aarch64|linux-x86_64|windows-*`.
- Archives: `.zip .tar.gz .tgz .tar.bz2 .tbz2` or raw.
- **Reality check:** `claude-acp`, `codex-acp`, `gemini` = `npx` (need Node); `mistral-vibe`,
  `cursor`, `goose`, `opencode`, `kimi`, … = `binary` (no runtime); `fast-agent`, `minion-code`
  = `uvx` (need Python via uv). So Node is needed for the common case; uv for a few; binary needs
  nothing.

## Architecture (reuses the Mason installer, `crates/vmux_editor/src/lsp/`)

### 1. Registry catalog
Fetch + cache `registry.json` (mirror `lsp/catalog.rs`, which already caches a `registry.json`).
Parse into `RegistryAgent { id, name, version, description, icon, distribution }`. Cache at
`~/.vmux/agents/registry.json`. Refresh on launch + TTL (daily) + manual; offline → use cache.

### 2. Distribution installer (reuse Mason download/extract/receipts)
- **binary** → `target::host_target` mapped to ACP targets → `install_from_url` + `archive::extract`
  (add `tar.bz2`) → run `cmd args env`. Zero runtime.
- **npx** → ensure managed **Node**, then install/run the package via managed npm/npx.
- **uvx** → ensure managed **uv** (Astral single binary), then `uvx package args` (uv fetches Python).

### 3. Managed runtimes (`~/.vmux/runtime/`, lazy, shared)
- **Node**: pinned tarball from nodejs.org via `install_from_url`; map `host_target` → Node naming
  (`darwin-arm64`…). Downloaded on first `npx` agent; shared by all.
- **uv**: single static binary from Astral GitHub releases; downloaded on first `uvx` agent. uv
  manages Python itself, so we don't.
- State via `store::{Receipt, is_installed, resolved_command, server_path_env}`.

### 4. Install trigger — before spawn (GUI)
`vmux_agent/src/client/acp.rs` `spawn_acp_session_on_add`, before `ClientMessage::SpawnAcpAgent`:
resolve the agent's distribution from the cached registry → ensure runtime + agent installed on a
bg thread (progress) → send `SpawnAcpAgent` with `command/args/env` from the manifest + a PATH that
includes the managed `bin/` (`store::server_path_env`). Daemon driver also injects that PATH
(defense-in-depth) via the existing `env` field.

### 5. Progress UI + agent manager page
- First open shows an **Installing…** state on the chat page (new `AgentRunState::Installing
  { phase, pct, message }`), reusing the bin-ipc `InstallPhase`/`LspInstallProgress` +
  `ManagerOutbox → drain → BinHostEmitEvent` pattern.
- New **`vmux://agents`** manager page (mirror `vmux://lsp`): browse the 37 registry agents, search,
  install/update/uninstall, see which need Node/Python, blue-dot when an update is available.

### 6. Catalog-driven launcher + icons
The command bar + start page list **registry** agents (not a hardcoded list). Each entry's icon is
the registry `icon` SVG URL — this **replaces** the `agent_host` favicon hack from the CLI-discovery
batch. (`AcpAgentConfig`/`settings.agent.acp` become per-agent *overrides*: enabled set, env for
auth, version pin, plus a `System` escape hatch for custom agents not in the registry.)

### 7. Auto-update
Re-fetch `registry.json` on the TTL; compare installed `Receipt.version` vs registry `version`;
surface an update in the manager page (and refresh on next launch). No manual version bumps.

### 8. Auth (unchanged, agent-handled)
Each agent does Agent Auth (OAuth, opens browser) or Terminal Auth on first prompt; vmux passes env
from settings overrides. Not vmux's concern beyond env plumbing.

## Scope / sequencing (this is large — flagged)

This turns #216 from "ACP host" into "ACP host + registry-driven agent management." Suggested order
inside the PR so it stays reviewable:
1. Registry fetch/cache/parse + `RegistryAgent` model.
2. Binary distribution install + spawn rewrite (no runtime) — proves the path end-to-end.
3. Managed Node + npx distribution (covers claude/codex/gemini).
4. Install-progress on the chat page.
5. Catalog-driven launcher + registry icons (supersede `agent_host`).
6. `vmux://agents` manager page + auto-update.
7. uvx + managed uv (can trail).

## Out of scope
- Bundling any runtime in the base app (stays lazy-download).
- Non-ACP concerns; agent auth internals.

## Testing
- Unit: registry parse; `host_target` → ACP target + Node/uv asset naming; distribution → resolved
  command/args/env; receipt install/outdated.
- Manual (fresh machine, no Node/Python): open a **binary** agent (mistral-vibe) → runs, no runtime;
  open an **npx** agent (claude) → Node downloads once → runs; open a **uvx** agent → uv downloads →
  runs; re-open → no re-download; airplane-mode first open → clear "network required" error.

## Open questions
1. Node + uv pin/refresh policy.
2. Registry TTL + whether to prompt vs auto-apply agent updates.
3. `settings.agent.acp` migration: keep the 3 hardcoded entries as overrides, or drop them entirely
   in favor of the registry catalog + an enabled-set?
