# Mobile Remote: continue agent chats from a phone

Vmux Remote is a native Dioxus Mobile app for iOS and Android. It lists active agent sessions,
replays their transcripts, streams new output, sends prompts, interrupts turns, and handles tool
approvals. Agents continue running on the Mac; the mobile app is a thin client.

## Build and run

iOS requires Xcode. Build or run the simulator app from the repository root:

```sh
make mobile-ios
make mobile-ios-run
```

Android requires Android Studio, an SDK, an NDK, a JDK, `ANDROID_HOME`, and `ANDROID_NDK_HOME`:

```sh
make mobile-android
make mobile-android-run
```

The build targets only compile the app. The run targets start Dioxus on the selected simulator or
connected device. Pass a device directly through Dioxus when selection is needed:

```sh
dx serve --ios -p vmux_mobile --device "iPhone"
dx serve --android -p vmux_mobile --device "Pixel"
```

## Connect

Start the Mac endpoint:

```sh
vmux remote
```

The command starts the Vmux service, configures Tailscale Serve when available, and prints a pairing
URL. Paste that URL into the native app. `vmux remote --reset` revokes the previous token.

## Runtime path

The daemon binds an authenticated JSON and server-sent-events API to loopback. Tailscale Serve
provides the encrypted HTTPS endpoint and tailnet access. The native app uses `reqwest` and stores
the paired endpoint and bearer token in its WebView sandbox.

Each phone connection uses the same daemon registries as the desktop client:

- `AgentSessionManager` for provider-direct page agents.
- `AcpSessionManager` for ACP agents.
- Server-sent events for transcript snapshots, streamed deltas, status, and approvals.
- JSON POST endpoints for prompts, cancellation, and approval decisions.

## Pairing and exposure

The daemon generates a 256-bit bearer token in its profile-specific service directory with mode
`0600`. The pairing URL carries it in the URL fragment. The native app extracts the endpoint and
token, verifies them against the API, and persists them locally.

The API rejects unauthenticated requests, caps prompt size, and listens only on loopback. Resetting
the token restarts the daemon and invalidates paired phones.
