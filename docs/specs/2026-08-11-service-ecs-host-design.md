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

- `vmux_session` — session and room components. Bevy-free of everything but `bevy_ecs`. No CEF, no render.
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

## Runtime model

```text
World (vmux_service)
├── AgentSession        sid, provider, model  ── AgentSessionHandle (task, channels)
├── AcpSession          agent_id, sid, cwd    ── AcpHandle
├── Process             ProcessId             ── PtyHandle (master, child, grid)
├── ChatRoom            RoomId                ── RoomProjection
├── InstalledAgent      roster entry
└── TeamMember
```

The registries become archetypes. `RemoteState` stops holding `Arc<Mutex<…>>` and sends typed messages into the world, which is the integration pattern the repo already mandates.

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
2. **Give the daemon a `World`.** `vmux_service` becomes a Bevy app with `MinimalPlugins` and the wake-driven runner. The Tokio runtime becomes a resource; the accept loop stays where it is. Nothing moves into ECS yet. Record idle CPU here — it is the baseline every later stage is judged against.
3. **Move handles onto entities.** Replace the three registries with archetypes. `Send + Sync + 'static` is the constraint to watch; `portable_pty`'s master is the one to check first.
4. **Move workspace shape off the client.** `ListAgents` and `ListTeam` become reads of service state. `AgentBroker` and `SharedFailure::NoDesktop` are deleted. The local client subscribes to what it used to own.
5. **Compose `vmux_host`.** Minimal plugins, session plugins, QUIC remote. No new logic, plus the Linux build job.

Stages 1–3 are reversible. Stage 4 inverts ownership and is the one to land alone if the diff argues for it.

## Cloud pairing is unresolved

Out of scope here, and the one place a cloud host cannot reuse the desktop design.

The trust anchor is a self-signed certificate whose SHA-256 rides in the pairing QR, persisted so pairing survives restart (`remote/quic.rs:44`). A cloud host has no screen to show a QR. Either a control plane hands the phone the fingerprint — which makes the control plane trusted, a real change to the threat model — or the host gets a CA certificate for a per-tenant name and pinning goes away.

Related: the certificate, key and token would live on the host's persistent volume, making that volume a secret store.

## Dependencies

The relay in `vmux-cloud` still speaks the HTTP/SSE protocol this branch replaced. It has to reach QUIC parity before any host, cloud or desktop, can be served by the deployed relay.
