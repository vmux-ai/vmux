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
with the relay and waits for it to accept; the pairing link cannot be shown until it has, because
a link offered first would name a pairing the relay would refuse.

The first time, scan the QR code with the phone. It opens Vmux Remote through the
`vmux://pair` deep link, verifies the endpoint, and stores the credentials. After the first
authenticated request, the desktop card switches to **Phone paired**. Use **Pair another** to show
the QR again. In a simulator, copy the pairing URL from the desktop card and paste it in — a
simulator cannot scan the screen behind it.

`vmux remote` remains available as a command-line fallback. `vmux remote --reset` revokes the
previous token.

## Runtime path

The transport is QUIC end to end. There is no HTTP fallback and no loopback listener. What the
relay does in the middle is [Topology](topology.md)'s subject and its internals live in
`vmux-cloud`; three modules on this side of it are what the desktop actually runs.

**`remote/quic/supervisor.rs`** owns the dialer's lifetime, and that ownership *is* the Remote
switch. Off means no dial, no registration and nothing to retry. Gating admission alone would
leave a desktop registered, retrying forever, and advertised as one that refuses everyone —
asking the user to attend to a feature they never turned on. A phone that authenticated before
the switch moved is dropped by the connection it is on, not by never having been dialled for.

**`remote/quic/dialer.rs`** holds the outward connection open and redials on a doubling backoff
from one second to thirty. A registration that stood for a minute restarts the sequence, so a
desktop connected for hours reconnects in a second rather than inheriting the cap from however
many attempts it took to get connected the first time — and a relay that accepts a registration
then tears it down mid-redeploy is not mistaken for a healthy one. The phone's packets arrive as
DATAGRAM frames on that connection and are handed to an inner endpoint that terminates their QUIC
session *here*: same certificate, same `admit()`, same dispatch a phone dialling us directly would
have reached. The inner MTU is floored at 1200 bytes — the smallest packet QUIC can handshake in —
with 64 reserved for tunnel framing.

**`remote/quic/dispatch.rs`** is the only place a remote message becomes an action. Prompt size,
replay dedup and attachment confinement are enforced there once rather than remembered at each of
nine handlers.

Each client connection uses the same daemon registries as the local client:

- `AgentSessionManager` for provider-direct page agents.
- `AcpSessionManager` for ACP agents.
- One bidirectional stream per request, and a long-lived stream per subscribed session carrying
  transcript snapshots, streamed deltas, status and approvals.

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

The device id the relay accepted is recorded beside it and lives exactly as long as the
registration does: written when the relay admits the desktop, removed when that session ends.
It is not the same as the desktop's own id, which exists whether or not anything is registered.
Every reader treats its absence as "not registered yet", so the app offers no pairing link during
that window rather than one the relay would refuse.
