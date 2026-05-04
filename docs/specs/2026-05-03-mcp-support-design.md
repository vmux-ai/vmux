# VMX-92: MCP Support Design

## Goal

Expose Vmux workspace controls to coding agents through MCP, starting with Mistral Vibe. The first version lets an agent open the command bar, create tabs, create terminal tabs, and delegate shell execution into visible Vmux terminals instead of running commands inside the agent process.

## Scope

MVP:

- Add a `vmux_mcp` stdio MCP server binary.
- Add service protocol messages for agent-originated workspace commands.
- Route MCP requests through `vmux_service` so the desktop remains the only owner of Bevy ECS state.
- Add command bar entries for launching Vibe when `vibe` is available locally.
- Launch Vibe in a Vmux terminal with MCP config that points to the local `vmux_mcp` binary.
- Implement `run_shell` by creating or selecting a Vmux terminal and sending the command into that PTY.

Out of scope:

- Browser DOM automation.
- MCP resources and prompts.
- Remote MCP transport.
- Generic support for every coding CLI.
- Long-running MCP server lifecycle inside the desktop process.

## Architecture

Vmux uses a separate `vmux_mcp` binary crate. It speaks MCP over stdin/stdout and connects to the existing `vmux_service` Unix socket as a client.

```text
Vibe MCP client
  -> vmux_mcp stdio server
  -> vmux_service IPC
  -> vmux_desktop systems
  -> tabs, panes, command bar, terminals
```

`vmux_mcp` does not link Bevy or mutate ECS state directly. It converts MCP tool calls into typed service protocol messages. The desktop consumes those messages in normal Bevy systems and reuses existing command handlers where possible.

Command metadata lives in `vmux_command`. That crate owns `AppCommand`, the command bar webview app, shortcut primitives, and `CommandPlugin`. `CommandPlugin` registers `AppCommand` messages, command schedule sets, and the command bar webview. Agent-safe zero-argument app commands are exposed with a macro-generated allowlist instead of hand-writing duplicate agent command variants.

This keeps process boundaries clear:

- `vmux_mcp`: protocol adapter between MCP JSON-RPC and Vmux IPC.
- `vmux_service`: broker for service-managed terminal processes and desktop control messages.
- `vmux_desktop`: UI authority for tabs, panes, command bar visibility, keyboard focus, and terminal spawn behavior.

## MCP Tools

The MVP server exposes four tools:

| Tool | Arguments | Behavior |
| --- | --- | --- |
| `open_command_bar` | optional `mode` | Opens the command bar. `mode = "default"`, `"commands"`, or `"path"`. |
| `new_tab` | none | Runs the existing new-tab flow and opens the command bar. |
| `new_terminal_tab` | optional `cwd` | Creates a new terminal tab, using `cwd` when valid. |
| `run_shell` | `command`, optional `cwd`, optional `mode` | Opens a visible terminal target and writes `command` plus newline to its PTY. |

`run_shell.mode` starts as:

- `new_tab`: always create a terminal tab.
- `active`: use the focused terminal if one exists, otherwise create a terminal tab.

The default is `new_tab` so agent shell execution is visible and easy to audit.

## Service Protocol

Add agent control messages to `vmux_service::protocol`:

```rust
pub struct AgentRequestId([u8; 16]);

pub enum AgentCommand {
    AppCommand {
        id: String,
    },
    NewTerminalTab {
        cwd: String,
    },
    RunShell {
        command: String,
        cwd: String,
        mode: AgentShellMode,
    },
}

pub enum ClientMessage {
    SubscribeAgentCommands,
    AgentCommand {
        request_id: AgentRequestId,
        command: AgentCommand,
    },
}

pub enum ServiceMessage {
    AgentCommand {
        request_id: AgentRequestId,
        command: AgentCommand,
    },
    AgentCommandAccepted {
        request_id: AgentRequestId,
    },
}
```

The service owns a broadcast channel for agent commands. The desktop sends `SubscribeAgentCommands` after connecting and receives `ServiceMessage::AgentCommand` through the existing `ServiceClient` polling path. `vmux_mcp` sends `ClientMessage::AgentCommand`; the service validates and broadcasts it, then replies to that MCP connection with `AgentCommandAccepted`.

For MVP, tool completion means "command accepted by Vmux", not "shell process finished".

## Desktop Handling

Desktop systems map agent messages onto existing command infrastructure:

- `AgentCommand::AppCommand` uses generated `AppCommand::from_agent_id` and writes the returned command to `Messages<AppCommand>`.
- `AgentCommand::NewTerminalTab` creates a terminal tab through the same spawn helpers used by command bar terminal actions.
- `AgentCommand::RunShell` creates or selects a terminal, then sends `ClientMessage::ProcessInput` with the command bytes.

The first exposed app command ids are `browser_open_command_bar`, `browser_open_commands`, `browser_open_path_bar`, and `tab_new`. New exposed app commands require an explicit macro attribute on the `AppCommand` leaf variant.

Terminal command injection happens after a service process exists. If terminal creation is asynchronous, the desktop stores a pending agent shell command keyed by the terminal entity, then flushes it when `ProcessCreated` updates `ServiceProcessHandle`.

The injected bytes are:

```text
<command>\n
```

No shell escaping is applied because the command is already a shell command chosen by the agent. `cwd` is applied by spawning the terminal with `Terminal::new_with_cwd`; if `mode = "active"`, cwd is ignored when reusing an existing terminal.

## Vibe Launch

Add visible command bar entries only when a local `vibe` executable is found:

- `Vibe New`
- `Vibe New Tab`

Detection uses a small runtime command lookup equivalent to `command -v vibe`. If unavailable, Vibe commands are omitted from the command bar.

Selecting a Vibe command opens a Vmux terminal and runs Vibe with a session-scoped `VIBE_HOME`. Vmux builds that directory by copying the user's existing Vibe config files when present, then appending the Vmux MCP server entry:

```toml
[[mcp_servers]]
name = "vmux"
transport = "stdio"
command = "<path-to-vmux_mcp>"
```

The terminal command starts Vibe with:

```sh
VIBE_HOME=<session-vibe-home> vibe --trust --workdir <cwd>
```

Using `VIBE_HOME` keeps the user's default Vibe config intact while preserving their copied providers, models, keys, and other settings for the launched session. The session directory lives under Vmux's app support directory and can be cleaned up when the terminal exits.

## Error Handling

`vmux_mcp` returns MCP tool errors when:

- It cannot connect to `vmux_service`.
- The desktop is not connected to the service.
- The command is rejected by validation.
- `run_shell.command` is empty.
- The service accepts the request but no desktop has subscribed to agent commands.

The desktop rejects:

- Nonexistent cwd values for new terminal creation.
- `run_shell` requests without an available pane.
- Unknown agent app command ids or shell modes.

Failures should include terse human-readable messages suitable for agent context.

## Testing

Unit tests:

- MCP JSON-RPC initialize/list-tools/call-tool parsing in `vmux_mcp`.
- Service protocol serialization for new agent command messages.
- Command lookup hides Vibe entries when executable lookup fails.
- Macro-generated agent command lookup exposes only allowlisted `AppCommand` variants.
- `run_shell` pending-command queue flushes after `ProcessCreated`.

Integration-style tests:

- Fake service connection accepts MCP `run_shell` and receives `AgentRunShell`.
- Desktop system maps agent messages to `AppCommand` values.
- Terminal spawn with pending command sends expected `ProcessInput` bytes.

Manual verification:

- Launch Vmux.
- Open command bar and confirm Vibe entries appear only when `vibe` exists.
- Start Vibe from command bar.
- Ask Vibe to run a shell command.
- Confirm command appears and runs in a visible Vmux terminal.

## Implementation Notes

Keep code organized by responsibility:

- `crates/vmux_mcp/src/main.rs`: stdio entrypoint.
- `crates/vmux_mcp/src/protocol.rs`: MCP request/response types.
- `crates/vmux_mcp/src/tools.rs`: tool schema and dispatch.
- `crates/vmux_command/src/command.rs`: `AppCommand` definitions and macro-exposed agent ids.
- `crates/vmux_service/src/protocol.rs`: agent command messages.
- `crates/vmux_desktop/src/agent.rs`: desktop-side command handling.

Do not use `mod.rs`. Use filename modules and directories.

Do not add browser automation hooks in this issue. The MCP surface should be small and auditable first.
