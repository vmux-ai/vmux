# Background Service: work that outlives the window

> Part of the [Vmux Architecture](../architecture.md) overview.

Vmux runs as two processes. The one you see is the app window — the Bevy host compositing CEF
surfaces. Behind it sits a long-lived background daemon (`crates/vmux_service`) that owns the
actual work: the PTYs behind every terminal, and the agent sessions. The window is just a client
of that daemon, so closing it doesn't stop anything that's running.

## launchd keeps it alive

A per-profile **LaunchAgent** (`ai.vmux.service.<profile>`) supervises the daemon:

- **`KeepAlive` on crash, not on exit** — launchd relaunches the daemon if it crashes, but lets a
  clean shutdown stay down.
- **`RunAtLoad` is false** — the daemon is *not* a login item; it never starts on boot. The app
  starts it on demand the first time it's needed (`ensure_running`, which is idempotent and even
  rewrites the plist when the binary path drifts between builds or worktrees).
- **Two backends** — a packaged `.app` registers an embedded agent through `SMAppService`; a dev
  binary is wired up with `launchctl bootstrap`/`kickstart`.

On start the daemon boots a Tokio runtime, binds a **Unix-domain socket**, and runs an IPC
server. The app — and the standalone [MCP server](agent-first.md) — connect to that socket as
clients and exchange typed protocol messages.

## Terminals and agents register by id

The daemon holds two in-memory registries; "registering" is just inserting into one of them.

- **Terminals.** `ProcessManager` keeps a `HashMap<ProcessId, Process>`. Creating a terminal
  (`create_process`) opens a real PTY (`portable_pty`), spawns the child shell or command, and
  starts a dedicated reader thread that pumps PTY bytes through an `alacritty` VTE parser into an
  in-memory grid. The daemon owns the master PTY, the child process, and that grid.
- **Agent sessions.** `AgentSessionManager` keeps a `HashMap<String, SessionHandle>` keyed by a
  session id (`sid`). Spawning one (idempotent per `sid`) starts a Tokio task that drives the
  provider stream and retains the running message history.

Each registered thing exposes the same two surfaces: a **broadcast channel** of updates, and a
point-in-time **snapshot**.

## Attach, detach, re-attach

Clients don't hold state — they subscribe to it.

- To show a terminal, the window `subscribe()`s and receives live **viewport patches** (only the
  changed lines), after painting an initial `snapshot()` of the current screen. Agents work the
  same way: `subscribe(sid)` streams deltas; `snapshot(sid)` replays the full transcript.
- Because the PTY, child, and tasks live in the daemon, a client can leave at any moment — close
  the window, quit the app — without disturbing them. The shell keeps reading, the build keeps
  building, the agent keeps streaming.
- On reopen, the app reconnects to the socket, re-subscribes, and replays snapshots. Whatever ran
  while it was gone is already in the daemon's grid and history, so it's right there when you
  return.

## What it buys you

An agent can kick off a long build, you can quit Vmux, and the output is waiting when it reopens.
Terminals and agent runs survive app restarts; a daemon crash is healed by launchd; and many
agents can work in their own [spaces](layout-model.md) at once — all because the work lives in a
process that outlives the window.
