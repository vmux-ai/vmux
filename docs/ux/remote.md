# Remote access

The iPhone companion lets you reconnect to supported workspace pages and ongoing conversations on your host.

## Pair with the host

Enable Remote on the host, then use the supported pairing flow on the phone. Pairing establishes trust in that host. Both sides connect outward through the relay, so an inbound port is not required.

Keep the host available for the remote work you want to reach. Remote is an explicit lifecycle switch; disabling it stops registration/retry. Revoking one client does not revoke every paired client.

## Return to existing work

Remote pages use the host's supported capabilities. Reconnecting returns to existing processes and Sessions instead of launching duplicate work. Use the Project and Session context to review results, follow up, or continue a task.

The phone is a remote client, not a second local host. Available capabilities depend on the supported mobile pages and the connected host.

## Privacy and permissions

The inner connection is encrypted through to the host. The relay sees connection metadata and forwards packets, but cannot read workspace contents. Native capabilities and task permissions remain scoped at the host.

See [Agents](agents.md) and [Projects, Spaces, and Sessions](workspace.md) for organizing the work you reconnect to.
