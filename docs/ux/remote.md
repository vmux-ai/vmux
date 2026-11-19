# Remote access

Use the iPhone companion to return to supported workspace pages and ongoing Sessions on your Mac.

## Connect your phone

Enable Remote on the Mac and follow the pairing flow with the phone. Keep the Mac available while you want to reach its work.

In development and local builds, open **Live** in the header. Turn it on and choose **Connect device** to show the pairing QR code. Pairing details hide after two minutes; **Show QR** displays them again.

A rollout or operational flag can make Live unavailable. The header control is hidden while availability is off, and the requested preference is preserved. Enabling from an older view cannot override an unavailable decision. Pairing and the connection phase remain separate from availability.

Pairing gives the phone access to that host. You can revoke a paired device or disable Remote when you want to end access.

## Pick up a task

Open the relevant Project or Session to review a result, ask a follow-up, or continue the conversation. Reconnecting returns to existing host work, including supported terminal processes, rather than starting a duplicate task.

The available pages depend on the phone app and host capabilities. For a trip, you can return to the research Session; for software, you can follow the agent's result and PR discussion.

## Know where the work lives

Your Mac runs the work. The connection to it is encrypted; the relay forwards traffic and cannot read the workspace contents. Task permissions still apply at the host.

See [Agents](agents.md) and [Projects and Sessions](workspace.md).

## Select Profile metadata to synchronize

When Profile transfer is available, connect your Account from the Account section and explicitly enable synchronization for the current Profile. Signing in alone uploads nothing. This slice synchronizes the Profile name and identity; it does not copy browser data, local files, credentials, or running Sessions.

Remote Profiles appear in the same section. Choose **Import and synchronize** to create a local Profile with that identity and a new local directory. This enables name synchronization for that Profile. Each device keeps its own files.

Offline edits remain local and wait for the connection to return. If another device changed the same Profile, choose **Keep local** or **Use remote** after reviewing the conflict. A remotely deleted Profile keeps its local files and stops synchronizing.
