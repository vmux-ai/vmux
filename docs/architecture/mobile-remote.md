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

Open the left side sheet in Vmux and enable **Remote** in the space card. Vmux configures a
profile-specific Tailscale Serve endpoint and keeps the setting across desktop restarts.

The first time, scan the QR code with the phone. It opens Vmux Remote through the
`vmuxremote://pair` deep link, verifies the endpoint, and stores the credentials. After the first
authenticated request, the desktop card switches to **Phone paired**. Use **Pair another** to show
the QR again. The HTTPS pairing URL remains visible as a manual fallback.

`vmux remote` remains available as a command-line fallback. `vmux remote --reset` revokes the
previous token.

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
`0600`. The QR deep link and manual URL carry the endpoint and token. The native app extracts them,
verifies them against the API, and persists them locally.

The API rejects unauthenticated requests, caps prompt size, and listens only on loopback. Resetting
the token restarts the daemon and invalidates paired phones. Disabling Remote removes only Vmux's
profile-specific Tailscale Serve HTTPS listener.
