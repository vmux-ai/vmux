# Mobile Remote: continue agent chats from a phone

Vmux Remote is an installable Dioxus web app served by the background service. It lists active
agent sessions, replays their transcripts, streams new output, sends prompts, interrupts turns,
and handles tool approvals. The agent still runs on the Mac; the phone is a thin client.

## Connect

Run:

```sh
vmux remote
```

The command starts the Vmux service, configures Tailscale Serve when available, and prints a
pairing URL. Open that URL on the phone and use Add to Home Screen. `vmux remote --local` prints a
localhost URL without configuring Tailscale. `vmux remote --reset` revokes the previous pairing
token.

## Runtime path

The daemon binds the remote HTTP server to loopback. Tailscale Serve provides the encrypted HTTPS
endpoint and tailnet access. The Dioxus page is part of the existing `vmux_server` WASM bundle, so
the desktop webviews and phone app share the Rust UI toolchain and packaged assets.

Each phone connection uses the same daemon registries as the desktop client:

- `AgentSessionManager` for provider-direct page agents.
- `AcpSessionManager` for ACP agents.
- Server-sent events for transcript snapshots, streamed deltas, status, and approvals.
- JSON POST endpoints for prompts, cancellation, and approval decisions.

## Pairing and exposure

The daemon generates a 256-bit bearer token in its profile-specific service directory with mode
`0600`. The pairing URL carries it in the URL fragment, which is not sent to the HTTP server. The
app exchanges it once for an HttpOnly, SameSite cookie and removes the fragment from browser
history.

The API is same-origin, rejects unauthenticated requests, caps prompt size, applies a restrictive
content security policy, and never listens on a LAN interface. Resetting the token restarts the
daemon and invalidates paired phones.
