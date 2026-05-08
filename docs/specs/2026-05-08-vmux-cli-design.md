# vmux_cli Design

## Goal

Single user-facing binary `vmux` that can launch the Vmux desktop app and run the MCP stdio server. Replaces the standalone `vmux_mcp` binary.

## Scope (v1)

- `vmux` — launch Vmux.app
- `vmux mcp` — run MCP stdio server
- `vmux --help`, `vmux --version` — clap defaults

Out of scope: `mcp install`, deeplink subcommand, session management, status. Reserved for future versions.

## Architecture

### New crate

`crates/vmux_cli/`:

```
crates/vmux_cli/
├── Cargo.toml          # [[bin]] name = "vmux"
└── src/
    ├── main.rs         # clap parser, dispatch
    ├── commands.rs     # Subcommand enum + Cli struct
    └── commands/
        ├── open.rs     # default: launch Vmux.app
        └── mcp.rs      # delegate to vmux_mcp::protocol::run_stdio
```

Module pattern follows the project rule: `commands.rs` + `commands/` directory, no `mod.rs`.

### Changes to `vmux_mcp`

- Remove `[[bin]]` section from `crates/vmux_mcp/Cargo.toml`.
- Delete `crates/vmux_mcp/src/main.rs`.
- Keep `[lib]`, `protocol`, and `tools` modules intact.

`vmux_mcp` becomes a library consumed by `vmux_cli`.

## Command Behavior

### `vmux` (no subcommand)

Launches Vmux.app.

- macOS: `std::process::Command::new("open").arg("-a").arg("Vmux").status()`
- Linux: print `vmux: launching the app is not supported on Linux yet` to stderr and exit with status 1.

Exit code mirrors `open`'s exit code on macOS.

### `vmux mcp`

Calls `vmux_mcp::protocol::run_stdio().await`. Tokio runtime started here (not in `vmux_mcp`), so the lib stays runtime-agnostic.

Behavior identical to today's `vmux_mcp` binary.

### `vmux --help` / `vmux --version`

Clap derive defaults. Version pulled from `CARGO_PKG_VERSION` (matches workspace version).

## Dependencies

`vmux_cli/Cargo.toml`:

```toml
[package]
name = "vmux_cli"
version.workspace = true
edition.workspace = true
description = "Vmux CLI"
publish = false

[[bin]]
name = "vmux"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { workspace = true }
vmux_mcp = { path = "../vmux_mcp" }
```

`clap` added at the workspace level if not already present (verify during impl).

## Testing (TDD)

Write tests first, in this order:

1. **Parser tests** (`tests/cli.rs` using `clap`'s testing helpers or `assert_cmd`):
   - `vmux` parses to `Cli { command: None }`
   - `vmux mcp` parses to `Cli { command: Some(Mcp) }`
   - `vmux --version` prints workspace version
   - Unknown subcommand → non-zero exit
2. **`open` command unit test**: extract a launcher trait so the test can assert the right invocation without spawning `open`. Real impl uses `std::process::Command`; test impl records args.
3. **`mcp` command integration test**: spawn the binary with `assert_cmd`, send a JSON-RPC `initialize` message on stdin, assert valid response on stdout. Mirrors today's `vmux_mcp` behavior end-to-end.

No mocks for the MCP path — it's a thin wrapper, integration test catches regressions.

## Migration / Rollout

Anything currently invoking `vmux_mcp` directly (e.g., MCP client configs pointing at `target/release/vmux_mcp`) must point at `vmux mcp` instead. No code in this repo references the binary by path, so the only impact is documentation / external user configs.

Update README or MCP setup docs if any reference the old binary name.

## Non-Goals

- `vmux mcp install` (auto-register with Claude Desktop) — defer to v2.
- Deeplink handling (`vmux vmux://...`) — defer to v2.
- Linux app launching — defer until desktop crate supports Linux.
- Session/status subcommands — defer to v2+.
