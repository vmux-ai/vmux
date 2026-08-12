# Mobile Remote: continue agent chats from a phone

Vmux Remote is a native Dioxus Mobile app. It lists active agent sessions, replays their
transcripts, streams new output, sends prompts, interrupts turns, and handles tool approvals.
Agents keep running on the desktop; the mobile app is a thin client.

Three roles appear throughout: the **desktop** owns the sessions, the **relay** is a rendezvous
point that cannot read what it carries, and **mobile** is the client. Which operating system runs
each one changes the build, not the design — the platform names below belong to build steps only.

## Build and run

iOS requires Xcode. Build or run the simulator app from the repository root:

```sh
make mobile-ios
make ios
```

Android requires Android Studio, an SDK, an NDK, a JDK, `ANDROID_HOME`, and `ANDROID_NDK_HOME`:

```sh
make mobile-android
make android
```

The build targets only compile the app. The run targets start Dioxus on the selected simulator or
connected device. Pass a device directly through Dioxus when selection is needed:

```sh
dx serve --ios -p vmux_mobile --device "iPhone"
dx serve --android -p vmux_mobile --device "Pixel"
```

## Connect

Start the desktop and the simulator in separate terminals:

```sh
make
make ios
```

Open the left side sheet in Vmux and enable **Remote** in the space card. The desktop registers
with the relay and waits for it to allocate a port; the pairing link cannot be shown until it has,
because the link has to name that port.

The first time, scan the QR code with the phone. It opens Vmux Remote through the
`vmux://pair` deep link, verifies the endpoint, and stores the credentials. After the first
authenticated request, the desktop card switches to **Phone paired**. Use **Pair another** to show
the QR again. In a simulator, copy the pairing URL from the desktop card and paste it in — a
simulator cannot scan the screen behind it.

`vmux remote` remains available as a command-line fallback. `vmux remote --reset` revokes the
previous token.

## Runtime path

The transport is QUIC end to end. There is no HTTP fallback and no loopback listener: a desktop
behind NAT cannot be dialled, so the daemon dials the relay and holds that connection open, and the
relay forwards a client's packets back over it verbatim. Those packets belong to a QUIC session
that terminates on the desktop, so the relay cannot read them.

Each client connection uses the same daemon registries as the local client:

- `AgentSessionManager` for provider-direct page agents.
- `AcpSessionManager` for ACP agents.
- One bidirectional stream per request, and a long-lived stream per subscribed session carrying
  transcript snapshots, streamed deltas, status and approvals.

Every request funnels through `remote/quic/dispatch.rs`, the only place a remote message becomes an
action. Prompt size, replay dedup and attachment confinement are enforced there once rather than
remembered at each of nine handlers.

## Pairing and exposure

The daemon generates a 256-bit bearer token in its profile-specific service directory with mode
`0600`, and a self-signed certificate beside it. The QR deep link and manual URL carry the relay
endpoint, the token, and the certificate's SHA-256 fingerprint.

The client pins that fingerprint and trusts nothing else — narrower than the public CA set. A
pairing link carrying no fingerprint is refused rather than downgraded, because there is no
unpinned transport left to fall back to.

The daemon rejects connections while Remote is off, rejects an unrecognised token, and caps prompt
size. Resetting the token restarts the daemon and invalidates every paired client.

## Choosing a relay

Every pairing goes through one. There is no loopback mode and no way to switch the relay off: a
desktop behind NAT is unreachable without a rendezvous point, and a second code path that only
worked from a simulator was worth less than the confusion it caused.

`VMUX_REMOTE_RELAY_URL` selects it and defaults to `https://relay.vmux.ai`. Point it at a local
stack — `make cloud` in `vmux-cloud` — to develop against that instead. The URL names a host and a
port; the desktop dials UDP on the same port number rather than speaking HTTP to it.

A relay on loopback or a private address is treated as a development stack and its certificate is
not verified, because no public root signs one. A relay on a real hostname is verified by name.

Nothing is dialled until Remote is enabled — the daemon checks that first, so the default costs no
traffic on a machine that never turns Remote on.

The `.env` at the repository root feeds `make`, which exports it to the app. A packaged build
started from the Finder sees no `.env` and takes the default. The desktop writes the resolved URL
into the profile's service directory, because the service manager starts the daemon with no
inherited environment and that file is the daemon's only view of the setting.

That file records the transport alongside the URL — `quic https://relay.vmux.ai`. Builds before
the QUIC cutover wrote a bare URL naming the HTTP port, and dialling that port over UDP reaches
nothing and reports only a timeout, so an untagged value is logged and ignored rather than
inherited. The tag is what is checked and never the port number, so a relay deliberately stood up
somewhere other than the default survives the upgrade. Enabling Remote rewrites the file tagged.

The port the relay allocates is recorded beside it and lives exactly as long as the registration
does: written when the daemon registers, removed when that session ends and again at daemon
startup. Every reader treats a missing port as "not registered yet", so the app offers no pairing
link during that window rather than one naming a port the relay has already freed.
