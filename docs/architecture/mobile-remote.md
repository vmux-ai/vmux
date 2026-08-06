# Mobile Remote: continue agent chats from a phone

Vmux Remote is a native Dioxus Mobile app for iOS and Android. It lists active agent sessions,
replays their transcripts, streams new output, sends prompts, interrupts turns, and handles tool
approvals. Agents continue running on the Mac; the mobile app is a thin client.

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

Start the desktop and iOS Simulator in separate terminals:

```sh
make
make ios
```

Open the left side sheet in Vmux and enable **Remote** in the space card. Local development uses a
profile-specific loopback endpoint shared by the Mac and iOS Simulator.

The first time, scan the QR code with the phone. It opens Vmux Remote through the
`vmuxremote://pair` deep link, verifies the endpoint, and stores the credentials. After the first
authenticated request, the desktop card switches to **Phone paired**. Use **Pair another** to show
the QR again. In Simulator, copy the pairing URL from the desktop card and paste it into the mobile
app because Simulator cannot scan the desktop QR code.

`vmux remote` remains available as a command-line fallback. `vmux remote --reset` revokes the
previous token.

## Runtime path

The daemon binds an authenticated JSON and server-sent-events API to loopback. The native app uses
`reqwest` and stores the paired endpoint and bearer token in its WebView sandbox.

Each phone connection uses the same daemon registries as the desktop client:

- `AgentSessionManager` for provider-direct page agents.
- `AcpSessionManager` for ACP agents.
- Server-sent events for transcript snapshots, streamed deltas, status, and approvals.
- JSON POST endpoints for prompts, cancellation, and approval decisions.

## Pairing and exposure

The daemon generates a 256-bit bearer token in its profile-specific service directory with mode
`0600`. The QR deep link and manual URL carry the endpoint and token. The native app extracts them,
verifies them against the API, and persists them locally.

The API rejects requests while Remote is off, rejects unauthenticated requests, caps prompt size,
and listens only on loopback. Resetting the token restarts the daemon and invalidates paired phones.

## Reaching a Mac that is not on the network

Loopback reaches the Mac from the iOS Simulator but not from a physical phone, so pairing a real
device goes through a relay. The desktop long-polls it for commands and posts responses back;
neither end listens, so a phone reaches a Mac behind NAT without either opening a port.

`VMUX_REMOTE_RELAY_URL` selects the relay and defaults to `https://relay.vmux.ai`. Point it at your
own to develop against one, or set it **empty** to switch the relay off and pair over loopback,
which is what the Simulator wants. Absent and empty mean opposite things.

Nothing is dialled until Remote is enabled — the daemon checks that first, so the default costs no
traffic on a machine that never turns Remote on.

The `.env` at the repository root feeds `make`, which exports it to the app. A packaged build
started from Finder sees no `.env` and takes the default. The desktop writes the resolved URL into
the profile's service directory, because launchd starts the daemon with no inherited environment
and that file is the daemon's only view of the setting.
