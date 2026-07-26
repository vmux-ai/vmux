# Vault and collaborative Spaces end-state design

## Summary

Vmux has two related encrypted systems:

- **Vault** is durable, user-owned state: settings, Knowledge, Tools manifests, Brewfile, dotfile
  sources, and later collaborative artifacts.
- **Space** is a live collaboration boundary: people, devices, and invited agents exchanging
  messages, edits, tool calls, approvals, and files.

Vault follows Obsidian's local-first folder model. Local files remain ordinary plaintext files that
work with editors and command-line tools. Every remote copy is encrypted before leaving the device.
Space follows modern encrypted messaging architecture: every human device and agent runtime is an
independent member of a cryptographic group.

The current Tools PR delivers the first complete personal-Vault slice: one local `~/.vmux` Vault,
automatic encrypted backup, GitHub and cloud-folder remotes, system-key-store caching, passkey
unlock where WebAuthn PRF is available, and a portable Recovery Key for password-manager storage.
Multiple Vaults, device-to-device enrollment, shared Vaults, and MLS group collaboration are later
milestones.

## Product invariants

- Remote providers never receive plaintext Vault content or plaintext encryption keys.
- Local Vault files remain plaintext. Full-device encryption is the operating system's job.
- Creating a remote always enables end-to-end encryption; there is no plaintext sync mode.
- A remote may be public, but public is never the default and still exposes ciphertext metadata.
- Every Vault has an independent random content key and remote.
- Every device and agent has an independent identity. Cryptographic identity is never shared by
  copying another device's state.
- Removing access prevents future decryption. It cannot erase plaintext or keys a former member
  already possessed.
- Recovery never depends solely on Vmux infrastructure.

## Obsidian baseline

Obsidian provides the baseline user model:

- A Vault is an ordinary local folder.
- Multiple Vaults are independent folders selected through a Vault switcher.
- Local files remain unencrypted and usable by other applications.
- Obsidian Sync encrypts remote content on the client.
- A second device selects a remote Vault and proves access locally.

Obsidian derives its remote encryption key from a user-supplied Vault password with scrypt and HKDF.
Vmux instead creates a random Vault key and wraps it for approved recovery mechanisms. This removes
the need to remember a separate password for every Vault while preserving a zero-knowledge remote.

References:

- https://obsidian.md/help/manage-vaults
- https://obsidian.md/help/sync/security
- https://obsidian.md/blog/verify-obsidian-sync-encryption/
- https://obsidian.md/help/sync/migrate

## Personal Vault architecture

### Local layout

```text
~/.vmux/                                  # default Personal Vault, plaintext
  settings.ron
  knowledge/
  tools/
    tools.toml
    Brewfile
    dotfiles/

~/Library/Application Support/Vmux/
  vaults.ron                              # local catalog, no secrets
  vaults/<vault-id>/
    repository/                           # encrypted staging checkout
    state.ron                             # local sync baseline
```

The first release keeps the existing encrypted staging checkout at
`~/Library/Application Support/Vmux/vault`. The end state migrates it into the per-Vault directory
without changing the remote format or Keychain account, because both are already keyed by Vault ID.

### Multiple Vaults

Each catalog entry contains only device-local routing data:

```text
VaultDescriptor {
    id,
    name,
    local_root,
    remote,
    active,
    automatic_sync,
}
```

- `~/.vmux` becomes the default Personal Vault.
- Additional Vault roots may live anywhere.
- Roots may not overlap.
- One Vault is active per window/profile. All connected Vaults may back up in the background.
- Switching reloads settings, Knowledge, Tools, and dotfiles as one unit.
- Removing a Vault from the switcher never deletes its local folder or remote.

### Remote format

```text
vault.ron
index.enc
objects/<opaque-id>
keys/
  devices/<recipient-id>.ron
  recovery/<recipient-id>.ron
  passkeys/<recipient-id>.ron
```

The manifest exposes only format information, a random Vault ID, and key epoch. The encrypted index
contains paths, modes, object references, and content-key envelopes. Objects are immutable
ciphertext.

GitHub uses Git as the transport and history layer. Google Drive, iCloud Drive, Dropbox, OneDrive,
and local folders use an encrypted object-folder backend rather than depending on Git repository
internals. Provider adapters move only the common encrypted format.

## Key hierarchy

### Current personal-Vault format

```text
random 256-bit Vault key
  ├── AES-256-GCM encrypted index
  ├── AES-256-GCM encrypted objects
  ├── wrapped by local system key store
  ├── wrapped by Recovery Key
  └── wrapped by WebAuthn PRF passkeys
```

This format is sufficient for personal backup. The key is stored locally under a stable per-Vault
Keychain account. Remote recipient files contain only wrapped key material and public identifiers.

### End-state envelope encryption

```text
Vault key epoch
  └── wraps per-object data keys
        └── encrypt immutable objects
```

Each object receives a random data-encryption key. Membership changes rotate the Vault wrapping key
and rewrap current object keys without re-encrypting every large object. New content uses new object
keys. A removed member may retain old content already decrypted, but cannot decrypt future objects
or newly wrapped current state.

### Recovery Key

Vmux generates a random 256-bit Recovery Key in the Vault page. It is displayed before any remote
change and intended for Bitwarden or another password manager. After the user confirms it was saved,
Vmux derives a Vault-specific wrapping key and uploads only the wrapped Vault key. The Recovery Key
itself is never uploaded. One Recovery Key can eventually recover multiple Vaults without reusing
their content keys.

On a new device:

1. Connect the remote Vault.
2. Paste the Recovery Key from the password manager.
3. Decrypt and validate the wrapped Vault key and encrypted snapshot.
4. Store the Vault key in the new device's secure store.
5. Clear the Recovery Key from memory.

There is no additional Vmux password. The password manager and operating-system biometric policy
protect their respective local secrets.

### Passkeys

WebAuthn PRF derives a wrapping key without exposing the passkey private key. PRF-capable passkeys
remain a convenient recovery recipient, not the only recovery mechanism. Providers that return no
PRF output cannot unlock encrypted Vaults and must fail explicitly.

### Device enrollment

The end state uses a Signal-style trusted-device flow:

1. A new device generates a per-Vault keypair in its system secure store.
2. It displays a QR code containing the Vault ID, public key, nonce, and request identifier.
3. An unlocked device verifies and approves the request.
4. The approving device wraps the Vault key to the new public key and uploads the envelope.
5. The new device proves possession by decrypting the challenge and materializes the Vault.

Recovery Key and passkey recovery create a fresh device recipient immediately after unlock. They do
not turn the recovery credential into a permanently loaded content key.

## Automatic backup and conflict behavior

After initial remote confirmation, backup is automatic:

- Watch only authored Vault paths.
- Debounce bursts of Knowledge edits, Tools reconciliation, settings writes, and dotfile changes.
- Skip generated/runtime roots and local Vault metadata.
- Fetch and rebase before every upload.
- Abort on conflicting local and remote edits.
- Never merge, force-push, or silently choose one side.
- Keep the explicit Sync action as retry and immediate-sync control.

The remote remains a backup and synchronization target, not the live plaintext filesystem.

## Collaborative Space architecture

Vault encryption protects durable content. Live collaboration uses a separate group protocol.

```text
Space
  ├── MLS group
  │     ├── each human device
  │     ├── each local agent runtime
  │     └── each hosted agent runtime
  ├── encrypted append-only event log
  │     ├── chat messages
  │     ├── tool calls and results
  │     ├── approvals
  │     ├── membership changes
  │     └── collaborative edit operations
  └── encrypted Vault objects and snapshots
```

### MLS membership

Messaging Layer Security, RFC 9420, is the group primitive. Signal's Double Ratchet is appropriate
for two-party sessions; MLS handles dynamic asynchronous groups efficiently.

- Every device is an MLS client, even when devices belong to the same user.
- Every agent is a distinct, short-lived member with an explicit capability scope and expiry.
- Local ACP agents may be represented by a Vmux-owned virtual client.
- Hosted agents terminate E2EE at their runtime. The provider can read content intentionally sent to
  that agent, and the UI must state that trust boundary.
- Invite and removal are MLS commits that advance the group epoch.
- Removed members cannot decrypt future Space events.

OpenMLS is the preferred Rust implementation and lives in an existing service crate. Vmux does not
implement group cryptography itself.

References:

- https://www.rfc-editor.org/rfc/rfc9420
- https://www.rfc-editor.org/rfc/rfc9750
- https://github.com/openmls/openmls
- https://signal.org/docs/specifications/doubleratchet/

### Events and collaborative artifacts

The Agent page evolves into a Space conversation renderer. Messages, tool activity, approvals, and
edit operations become typed `SpaceEvent` records with actor, device, agent, epoch, ordering, and
causality identifiers.

MLS application messages protect the event log. Large files are not encrypted directly with chat
message keys. An MLS exporter-derived Space wrapping key protects Vault object-key envelopes. A
CRDT or operation log handles simultaneous edits; MLS handles confidentiality and membership, not
merge semantics.

New members receive current artifacts only after explicit approval. Historical chat access is a
separate policy and is not implied by receiving the current Vault snapshot.

### Relay and identity services

MLS does not define account identity, message delivery, storage, or collaboration semantics. Vmux
therefore needs:

- an untrusted ciphertext relay for real-time and offline Space events;
- an authentication directory mapping accounts to device credentials;
- key transparency so the service cannot silently substitute device keys;
- encrypted durable snapshots in the selected Vault remote;
- self-hosted relay support for organizations that do not use Vmux infrastructure.

The relay routes ciphertext and metadata. It does not receive Vault keys or MLS plaintext.

## Delivery stages

### Current Tools PR

- Single Personal Vault rooted at `~/.vmux`.
- Knowledge, settings, Tools manifests, Brewfile, and dotfile sources.
- Always-encrypted GitHub and cloud-folder backup.
- Automatic backup after local authored changes.
- System-key-store caching.
- Password-manager Recovery Key.
- Optional WebAuthn PRF passkey recovery.
- Manual Sync remains available.

### Multi-Vault

- `vaults.ron` catalog and Vault switcher.
- Per-Vault staging directories and sync workers.
- Existing single-Vault migration.
- Add-device approval across selected Vaults.

### Collaborative Spaces

- Typed append-only `SpaceEvent` model.
- Real-time ciphertext relay.
- OpenMLS membership for users, devices, and agents.
- Per-object envelope encryption and membership-driven key epochs.
- Encrypted CRDT edits, current-state sharing, and explicit history policy.

### Hardening

- Key transparency.
- Device and Recovery Key rotation.
- Historical Git/object-store compaction after key compromise.
- Hardware-backed local keys where platform APIs permit.
- Self-hosted authentication and relay services.
