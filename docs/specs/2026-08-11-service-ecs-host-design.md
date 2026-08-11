# Service as ECS host design

## Goal

Make `vmux_service` a headless Bevy app that owns session state *and* workspace shape, and demote the local client to a renderer. A cloud host is then the same service with no local client attached.

## Why now

[Topology](../architecture/topology.md) already states the thesis: one service that owns the work, and clients that render it. The code does not honour it yet.

The service owns sessions. The local client owns the workspace shape — installed agents, the active roster — in a different process. So `ListAgents` and `ListTeam` are not reads of service state; they are commands brokered into the GUI and relayed back, and they answer `NoDesktop` when no window is open (`crates/vmux_wire/src/protocol/shared.rs:215`, `crates/vmux_service/src/remote/quic/dispatch.rs:192`).

That costs two things:

- A paired phone is half-served whenever the Mac's window is closed. Sessions work; the workspace does not.
- A cloud host is impossible. There is never a desktop, so `NoDesktop` is the permanent answer.

The second is the forcing function, but the first is a bug on its own.

## What already holds

Not a rewrite. The pieces are in place and pointing the right way.

- The remote transport is daemon-resident and GUI-free. `remote/quic.rs` and `remote/server.rs` link nothing that renders; `vmux_desktop/src/remote.rs` is a toggle.
- `run_server` is a library function taking a listener (`host/server.rs:147`), so hosting it inside another runtime is a call, not a port.
- Linux builds and tests the whole workspace in CI today (`.github/workflows/ci.yml:46`, `:127`). `objc2` is `cfg(target_os = "macos")` throughout.
- The ECS projection of sessions already exists, keyed by the same `sid` (`vmux_agent/src/plugin/components.rs:12`, `plugin/room.rs:37`). This moves ownership; it does not invent a model.

What is genuinely missing is that the daemon has no `World`, and the state it does own sits in `Arc<Mutex<HashMap>>` — `host/agent.rs:70`, `host/acp.rs:33`, `host/process.rs:1901`. Those maps hold channels, `JoinHandle`s and PTY handles: component payloads, stored in the wrong shape.

## Ownership after

- `vmux_session` — the session and room components. Depends on `bevy_ecs` and nothing that renders.
- `vmux_service` — headless Bevy app. Authority for sessions, ACP, processes, agent roster, team.
- `vmux_agent` — page plumbing for the local client. Subscribes; no longer owns.
- `vmux_host` — `vmux_service` composed for a machine that never has a local client.

## The seam is a crate, not a cfg alias

`host`/`ui`/`web` are target-derived (`crates/build_platform_cfg.rs`), and this split is not a target split — cloud Linux and desktop Linux are the same triple. A cargo feature is the other obvious candidate and is wrong here: features unify per package across an invocation, so `cargo build --workspace` would relink CEF into the headless build and nothing would fail.

So the seam is a crate boundary. `vmux_session` cannot depend on `bevy_cef`, and no feature flag can make it.

`vmux_agent` is the crate to cut. Its CEF use is almost all page IPC — `BinEventEmitterPlugin`, `BinHostEmitEvent`, `BinReceive`, `Browsers` across `plugin/chat/*` and `plugin/agents.rs` — with genuine rendering confined to `attach.rs`, `spawn.rs` and `page_open.rs`. The session model underneath it touches none of that.

CI asserts the seam directly:

```text
cargo tree -p vmux_session -i bevy_cef   # must find nothing
cargo tree -p vmux_host    -i bevy_cef   # must find nothing
```

A test cannot catch a link-level feature leak, so this has to be a job. It also has to be a *new* job: the Linux runs install the CEF framework before building, so nothing in CI currently proves any crate can be built without it. The lean-desktop-features steps (`ci.yml:122`, `:190`) are the precedent to copy.

## Component model

All host state is components on entities. The rule that drives the decomposition:

**Every `Arc<Mutex<T>>` in the current handles exists to share one field between a spawned task and its readers.** The World is that sharing mechanism, so each field becomes a component and the wrapper disappears. `SessionHandle` alone carries four of them.

### Agent session

`SessionHandle` (`host/agent.rs:56`), ten fields, becomes one entity:

| Component | From |
| --- | --- |
| `AgentSession { sid, provider, model, cwd, created_at }` | the descriptor fields; mirrors the component the client already has |
| `AgentMessages(Vec<Message>)` | `messages: Arc<Mutex<Vec<Message>>>`, owned and unwrapped |
| `AgentRunStatus` | `status: Arc<StdMutex<…>>` |
| `PendingApproval(RemoteApproval)` | `approval: Arc<StdMutex<Option<…>>>` |
| `SessionInbox(UnboundedSender<SessionInput>)` | `input_tx` |
| `SessionFanout(broadcast::Sender<ServiceMessage>)` | `stream_tx` |
| `SessionTask(JoinHandle<()>)` | `task` |

`Option` fields become component presence. `PendingApproval` absent means nothing is waiting, and the system that answers approvals queries for it rather than testing a field on a struct it had to lock first.

### ACP session

`AcpHandle` (`host/acp.rs:24`) is the same shape around `Arc<AcpShared>`: `AcpSession`, `AcpInbox`, `AcpTask`, and a relationship to the anchor process in place of the `anchor: ProcessId` field. Component names take the `Inbox` suffix rather than `Input`, because `SessionInput` and `AcpInput` are already the message types they carry.

### Process

`Process` (`host/process.rs:195`) is the god-struct — twenty-five fields spanning five unrelated concerns:

| Component | Fields |
| --- | --- |
| `Process { id, shell, cwd, pid, created_at }` | identity |
| `Pty { master, child, writer, rx }` | the OS handles |
| `TerminalGrid { term, processor, line_hashes, win_hashes }` | the parsed screen |
| `ShellIntegration { osc133, run_marker, last_command_exit, … }` | OSC 133 and run markers |
| `ProcessFanout(broadcast::Sender<ServiceMessage>)` | `patch_tx` |

**Viewport does not belong on the process at all.** `view_top`, `following`, `last_win`, `last_cursor`, `last_passthrough` and `selection` are single-valued on `Process` while `patch_tx` fans out to every attached client, so `handle_scroll_window` (`host/process.rs:1163`) moves the window for all of them. That is latent today because only the desktop renders terminals; it goes live the moment a second client does. Those fields belong on a subscription entity, one per attached client.

Decomposition surfacing that is the argument that this is not tidying.

### Requests in flight

`pending_commands`, `pending_queries` and `pending_tool_calls` are three `HashMap<AgentRequestId, oneshot::Sender<…>>` (`host/agent_broker.rs:13`). Each in-flight request becomes an entity carrying `RequestId`, `Responder` and `Deadline`, related to the session that issued it.

Timeouts stop being a per-request `tokio::time::timeout` and become one system querying `Deadline`. Despawning a session takes its in-flight requests with it.

### Composition over id fields

`ChildOf`/`Children` is already the idiom here — over a thousand call sites. Prefer relationships to stored ids:

```text
ChatRoom ─── RoomMember
AgentSession ─── Request        (in flight, despawned with the session)
             └── Subscription   (one per attached client, owns the viewport)
AcpSession ─── anchor Process
```

## Resources

A `Resource` is for a process-wide singleton with no per-entity identity: the Tokio runtime handle, the wake sender, profile paths. Never a collection of domain state.

`AgentSessionManager`, `AcpSessionManager` and `ProcessManager` therefore do not become resources. **The `HashMap` is the archetype.** What survives each is a thin index — `SessionIndex(HashMap<String, Entity>)`, `ProcessIndex(HashMap<ProcessId, Entity>)` — maintained by observers on spawn and despawn. An index holds entity ids and nothing else. The moment a field would be read off an index rather than looked up through it, that field belongs on a component.

`RemoteState` (`remote/server.rs:38`) dissolves the same way. `token` and `paired` are process-wide and stay; `agents`, `acp` and `client_ops` become queries.

## The headless runner

Bevy is frame-driven and there is no vsync in a container. `ScheduleRunnerPlugin::run_loop(tick)` is the obvious choice and is wrong at both ends: a fast tick burns CPU on a box billed by the second, and a slow one adds latency to every streamed frame.

Use a custom runner via `set_runner` that parks on a wake channel with a timeout. Wake sources are the ones that already exist — PTY output, provider stream frames, an opened QUIC stream, an IPC message. The timeout is a floor for housekeeping, not the pacing mechanism.

This mirrors the desktop rule. `UpdateMode::Continuous` is banned there for 100-200% idle CPU; in a cloud host the same mistake is a bill, and it defeats the idle detection that suspend and scale-to-zero depend on.

**Idle CPU is an acceptance number, not an afterthought.** Measure a host with paired sessions and no traffic.

## What does not move

Topology already draws this line. `vmux_editor`, `vmux_terminal` and `vmux_layout` stay client-side — DOM measurement and tiling, which a headless host and a small screen both have no use for. Compositing, CEF and input routing stay with the local client.

The service gains the work and the shape. It does not gain a view.

## Migration path

Each stage keeps macOS working and CI green.

1. **Cut `vmux_session`.** Move the session and room components out of `vmux_agent`; re-export them so no caller changes. Add the `cargo tree` job. No behaviour change.
2. **Give the daemon a `World`.** `vmux_service` becomes a Bevy app with `MinimalPlugins` and the wake-driven runner. The Tokio runtime becomes a resource; the accept loop stays where it is. No state moves yet. Record idle CPU here — it is the baseline every later stage is judged against.
3. **Spawn entities alongside the maps.** Descriptor components only, written when a session or process is created. The maps stay authoritative and nothing reads the components yet. This proves the model and the `Send + Sync + 'static` bounds at zero risk; `portable_pty`'s master is the one to check first.
4. **Move the handles, delete the managers.** Channels, tasks and grids move onto the entities; readers switch to queries; the three registries and their `Arc<Mutex<…>>` go. In-flight requests become entities with a `Deadline` system.
5. **Split viewport onto subscriptions.** The one behaviour change in the sequence, and the one to flag in review: per-client viewport instead of one window shared by every attached client.
6. **Move workspace shape off the client.** `ListAgents` and `ListTeam` become reads of service state. `AgentBroker` and `SharedFailure::NoDesktop` are deleted. The local client subscribes to what it used to own.
7. **Compose `vmux_host`.** Minimal plugins, session plugins, QUIC remote. No new logic, plus the Linux build job.

Stages 1–4 are reversible refactors. Stage 5 changes behaviour and stage 6 inverts ownership; either is worth landing alone if the diff argues for it.

## Cloud pairing is unresolved

Out of scope here, and the one place a cloud host cannot reuse the desktop design.

The trust anchor is a self-signed certificate whose SHA-256 rides in the pairing QR, persisted so pairing survives restart (`remote/quic.rs:44`). A cloud host has no screen to show a QR. Either a control plane hands the phone the fingerprint — which makes the control plane trusted, a real change to the threat model — or the host gets a CA certificate for a per-tenant name and pinning goes away.

Related: the certificate, key and token would live on the host's persistent volume, making that volume a secret store.

## Dependencies

The relay in `vmux-cloud` still speaks the HTTP/SSE protocol this branch replaced. It has to reach QUIC parity before any host, cloud or desktop, can be served by the deployed relay.
