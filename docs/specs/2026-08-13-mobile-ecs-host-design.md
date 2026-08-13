# Mobile ECS host design

## Goal

Give the phone a client-side Bevy `World` that owns device state and projects service state, and make `PageHost` its only door. Pages then render on the phone the way they render on the desktop, and a phone feature stops costing a wire change.

## Why now

[Service as ECS host](2026-08-11-service-ecs-host-design.md) made the service own the work and demoted the local client to a renderer. The phone is the other client, and it is not a renderer of anything — it is a second implementation.

`crates/app/vmux_mobile/src/main.rs` is 2275 lines. `AppBody` (`:698`) is 787 of them and declares thirty `use_signal`s in its first forty lines: auth, pairing, sessions, agents, the open session, the transcript, the composer draft, staged attachments, media queries, three generation counters used as stale-guards. One component holding routing, connection state, chat state and the composer, with no owner for any of it.

The cost compounds. Every feature the phone wants needs a `SharedAgentCommand` variant, a payload mirrored into `vmux_wire`, an answering system and a dispatch arm — the five-step change written out in VMX-150 for one struct.

## What already holds

More than the issue assumed.

- **The page/host seam is two methods.** `PageHost` (`vmux_ui/src/transport.rs:27`) is `send(id, bytes)` and `listen(id, on_bytes)`. `transport/cef.rs` answers it for the desktop, `vmux_mobile/src/page_host.rs:30` for the phone. A page does not know which it has.
- **Pages already compile for iOS.** `ui` means wasm *or* iOS (`crates/build_platform_cfg.rs:31`), so `vmux_chat::page` and its leaves are already built on the phone.
- **The push channel exists.** `quic_api.rs:238` opens the stream client-side and writes once, because the relay only routes streams the client opened. The constraint is real and the workaround is built.
- **The session model is render-free and phone-linkable.** VMX-140 landed `vmux_session`, whose whole dependency list is `bevy_app`, `bevy_ecs`, `bevy_reflect`, `serde`, `vmux_wire`. `room.rs` is 485 lines of it.

## The duplicate projection is a missing dependency, not a missing ECS

This issue claimed 1078 lines the phone "cannot call because it has no ECS". That was wrong twice, and the correction changes what the ECS is for.

`vmux_service::chat` is 729 lines of transcript grouping — `group_turns_tail`, `group_turns_before`, `grouped_item_count` — declared `pub mod chat` unconditionally at `vmux_service/src/lib.rs:4`, outside the `#[cfg(host)]` block below it. It imports `crate::message`, and `vmux_service/src/message.rs` is one line re-exporting `vmux_wire::room::{AssistantBlock, Message, PlanStep, SubagentBlock}`. Pure functions over wire types, unit-tested, no Bevy anywhere near them.

The phone re-implements them at `main.rs:1743`, `:1787` and `:1809` because `vmux_mobile` does not list `vmux_service` as a dependency. Nothing else stops it.

So deleting the duplicate is a manifest line and a call-site change. **It should land first and alone, and it does not need any of the rest of this design.**

That removes the strongest-sounding argument for the World, so the honest justification is what remains:

- **Device state has no owner.** Three global queues drained by polling futures: `OPENED_URLS` (`main.rs:49`, polled at `:854`), `qr_scanner::RESULTS` (`qr_scanner.rs:43`, polled at `main.rs:867`) and `RESUMED` (`main.rs:104`). Each is a thread-local or a `Mutex` plus a loop, invented once per capability.
- **`PageHost::send` has nothing to send to.** It returns `Unsupported` for every id (`page_host.rs:40`) because there is no local state a page could address. `listen` serves exactly `TEAM_EVENT`, by re-reading the Mac every three seconds (`page_host.rs:28`).
- **Thirty signals in one component** is the shape state takes when nothing owns it.

The World is for those. The projection is a dependency line.

### Where the grouping should live

`vmux_session`, not `vmux_service`. It is pure, three surfaces consume it, and `host/` means "runtime that runs with no UI and owns state" — a fold over a slice is not that. Move it beside `room.rs` and re-export from `vmux_service::chat` so no caller changes, exactly as stage 1 of the sibling spec moved the components.

## Ownership after

- `vmux_session` — session and room components, plus transcript grouping. No `App`, no CEF, no target gate.
- `app/vmux_mobile` — the `World`, the device capability plugins, the QUIC forwarder, and `MobileHost`.

**No new crate.** One consumer, and the seam that would justify one is already enforced by the target (below). A `vmux_mobile_host` crate earns its place when a second thing links it.

## The seam is the target, not a crate

The sibling spec needed a crate boundary because cloud Linux and desktop Linux are the same triple, so no cfg could separate them. Here the discriminator is `aarch64-apple-ios`, and `build_platform_cfg.rs` already resolves it to `ui`.

The risk is therefore inverted, and worth stating because it reads backwards. `vmux_agent` and `vmux_service` both gate their Bevy halves with `cfg(not(any(target_arch = "wasm32", target_os = "ios")))`. That is correct — it is what keeps CEF off the phone — but it also means **those crates compile without Bevy on iOS**. The phone cannot reuse their plugins. It links `vmux_session` and writes its own.

CI asserts the direction that can still break:

```text
cargo tree --target aarch64-apple-ios -p vmux_mobile -i bevy_cef   # must find nothing
```

## Topology: one host, two legs

```text
page ↔ mobile host ↔ relay ↔ desktop or cloud host
                   ↔ device (camera, photos, biometrics, notifications)
```

A page talks to exactly one host, because `PageHost` is a singleton by construction: `install_host` writes into a `thread_local!` holding one `Option<Rc<dyn PageHost>>` (`transport.rs:42`, `:100`) and `Host::with_installed` (`:63`) resolves every call through it. Giving a page two hosts means a registry and a rule in each page about which id belongs to which — topology knowledge in the one place the trait exists to keep it out of.

It also matches the desktop, where a page likewise talks to one host that fans out: CEF → desktop ECS → `vmux_service`.

The phone's World is therefore not purely a projection. Sessions project from the service — a PTY is never going to run on a phone — but device state is authoritative here and nowhere else. That is what makes it a host.

## Where the World lives, and who pumps it

The desktop is Bevy hosting a webview. The phone is the inverse: Dioxus and tao own the run loop (`main.rs:106`), and UIKit requires the main thread. Bevy is a guest.

**Reject a worker thread.** It looks like the service's headless runner and buys nothing here. `PageHost` is `Rc<dyn PageHost>` and `BytesListener` is `Box<dyn FnMut(&[u8])>` — neither is `Send`, so listeners run on the Dioxus thread whatever the World does. Every device capability is a UIKit call that must reach the main thread anyway. A worker thread adds two marshalling hops to buy parallelism the phone has no work for.

**Take the main thread, pumped by a wake-driven future.** Same wake discipline as the service runner, for a different reason: there an idle tick is a bill, here it is a blocked UI thread. Wake sources are the ones that already exist — a frame decoded off the QUIC subscription, a `PageHost::send`, a capability result, `Event::Resumed` (`main.rs:124`).

The budget is what makes this safe. The phone's World has no PTYs, no terminal grid, no compositing and no tiling. One update is a transcript fold over the tail.

**Idle CPU and battery are acceptance numbers.** Measure a paired phone with an open session and no traffic, before and after.

### Schedules

One pass per wake, matching the service so the two read the same:

- `PreUpdate` — drain the tokio channels and the capability results into components. The only place the outside world is read.
- `Update` — decide. Fold the transcript, answer page requests, forward what belongs to the Mac.
- `PostUpdate` — fan out. Push payloads to the listeners `MobileHost` registered.

## Routing is registration, not a match

`MobileHost` decides nothing. `send` queues the `(id, bytes)` into the World; `listen` records the callback. Whichever plugin claimed that id answers it, and forwarding to the Mac is itself just the plugin that owns `Api`.

So adding a capability is adding a plugin, not editing a central `match`, and `Unsupported` becomes its honest meaning — no plugin claimed this id — which is what `event_listener.rs:25` already documents it as.

A device capability owns its slice whole, per the by-feature split rule: `CameraPlugin` holds permission state, the `AVCaptureSession` and the scan result together. Because the World is on the main thread, its systems call UIKit directly — the `dispatch2` hop `qr_scanner.rs` needs today goes away along with `RESULTS`, `ACTIVE` and `REQUESTING`.

## What does not move

Sessions execute on the Mac or a cloud host. The phone's World owns device state and a projection; it never owns a PTY, an ACP process or a provider stream.

Native navigation — the `UINavigationController` stack, the back gesture, per-screen state — is VMX-161 and is deliberately not here. This design decides where state lives; that one decides how screens are presented.

## Migration path

Each stage ships on its own.

1. **Delete the duplicate projection.** Move the grouping to `vmux_session`, re-export from `vmux_service::chat`, depend on it from `vmux_mobile`, delete `main.rs:1743-1861`. No ECS, no behaviour change. Worth landing whether or not the rest happens.
2. **Give the phone a `World`.** `MinimalPlugins`, the wake-driven pump, nothing in it. Record idle CPU and battery — the baseline every later stage is judged against.
3. **Move the QUIC client in.** `Api` becomes a resource, the subscribe loop at `main.rs:1888` becomes a system draining into components, and `AppBody`'s session signals become reads.
4. **Make `PageHost` real.** `send` queues, `listen` registers, `PostUpdate` pushes. `TEAM_EVENT` stops polling and `POLL_INTERVAL_MS` goes.
5. **Camera as a capability.** Port the QR scanner onto `CameraPlugin`, preserving VMX-132's denied-permission behaviour.
6. **Retire the rest of `AppBody`.** Composer, media, pairing.

Stages 1 and 2 are independent of each other. Stage 3 is where behaviour can actually regress, and is the one to land alone.

## Open

- **Backgrounding.** iOS suspends the process; the QUIC connection dies and the World stops being pumped. Today `RESUMED` (`main.rs:104`) triggers a re-read. What the World does across suspend — reconnect, refetch, or restore — is undecided.
- **Persistence.** The desktop persists ECS state through `moonshine-save`. Whether the phone persists a transcript or refetches it is open, and depends on the above.

## Dependencies

**VMX-151** blocks anything still `cfg(web)` — the palette and the start page cannot mount on the phone until the command-bar input is controlled. **VMX-127** is QUIC parity on the deployed relay, which the forwarding leg rides on. **VMX-132** overlaps stage 5 and lands first. **VMX-161** owns navigation.
