# Auto Update Support

**Issue:** VMX-84
**Date:** 2026-04-26
**Status:** Design approved

## Overview

Add silent automatic updates to Vmux. The app checks for new versions via the GitHub Releases API, downloads the full signed `.app` bundle in the background, and applies the update on next launch. Users never see a prompt or dialog.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Update UX | Silent + apply on restart | No user interaction needed |
| Tooling | Pure Rust (no Sparkle/ObjC) | Simpler dependency chain |
| Payload | Full `.app` bundle | Preserves code signing and notarization |
| Update source | GitHub Releases API | Already produces release artifacts, no extra infra |
| Apply strategy | Stage + swap on launch | No helper process needed |
| Opt-out | `auto_update` field in `settings.ron` | Consistent with existing settings pattern |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Vmux App Launch                           │
│                                                             │
│  ┌──────────────┐    ┌──────────────────┐                   │
│  │ Check staged  │───→│ Apply update     │──→ Re-exec Vmux  │
│  │ update exists?│yes │ (swap .app)      │                   │
│  └──────┬───────┘    └──────────────────┘                   │
│         │ no                                                 │
│         ▼                                                    │
│  ┌──────────────┐    ┌──────────────────┐    ┌────────────┐ │
│  │ Normal app   │    │ Background task:  │    │ Stage to   │ │
│  │ startup      │───→│ Check GitHub API  │───→│ cache dir  │ │
│  └──────────────┘    │ for new version   │    └────────────┘ │
│                      └──────────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

### Module Structure

```
crates/vmux_desktop/src/
├── updater.rs          # UpdatePlugin, Bevy integration, re-exports
├── updater/
│   ├── github.rs       # GitHub Releases API client
│   ├── download.rs     # Streaming download + checksum verification
│   ├── stage.rs        # Extract to staging dir, write update-meta.json
│   └── apply.rs        # Swap .app bundle, re-exec
```

`updater.rs` is the module root, declares submodules, and contains the `UpdatePlugin` with Bevy timer-based polling.

## Update Check & Download

### GitHub API

- Endpoint: `GET https://api.github.com/repos/vmux-ai/vmux/releases/latest`
- No auth required (public repo). Rate limit: 60 req/hr unauthenticated.
- Parse `tag_name` (e.g., `v0.2.0`), extract semver, compare against `env!("CARGO_PKG_VERSION")` using the `semver` crate.
- Look for asset matching pattern: `Vmux-v{VERSION}-aarch64-apple-darwin.app.tar.gz`

### Staging Directory

```
~/Library/Caches/ai.vmux.desktop/updates/
├── downloading/          # Active download (partial file)
│   └── Vmux-v0.2.0.app.tar.gz
├── staged/               # Ready to apply
│   └── Vmux.app/         # Extracted, verified
└── update-meta.json      # Version, download timestamp, SHA-256 checksum
```

- Stream download with `reqwest` to `downloading/`
- Verify SHA-256 checksum against companion `.sha256` file (uploaded as a separate release asset by CI)
- Extract `.tar.gz` to `staged/Vmux.app`
- Write `update-meta.json` with version + timestamp + checksum

### Timing

- First check: ~5 seconds after startup (avoid contending with initial rendering)
- Periodic polling: every 1 hour while the app is running
- Implemented as a Bevy system with a repeating `Timer` resource (1hr interval)
- If an update is already staged, skip the check entirely

## Update Application (Swap on Launch)

Runs in `main()` before Bevy app initialization:

1. Check `~/Library/Caches/ai.vmux.desktop/updates/staged/Vmux.app` exists
2. Read `update-meta.json` for expected version
3. Resolve current `.app` bundle path via `std::env::current_exe()` (e.g., `/Applications/Vmux.app/Contents/MacOS/Vmux` -> derive `/Applications/Vmux.app`)
4. Atomic swap:
   a. Rename current bundle to `/Applications/Vmux.app.old`
   b. Move staged bundle to `/Applications/Vmux.app`
   c. Remove `.old` bundle
5. Clean up staging dir + `update-meta.json`
6. Re-exec via `std::os::unix::process::exec()` (replaces current process, preserves PID)

### Failure Recovery

- If step 4b fails: rename `.old` back to `Vmux.app`, log error, continue with current version
- If step 4c fails (can't remove old): non-fatal, log warning, clean up on next launch
- If re-exec fails: swap already succeeded, user just needs to manually relaunch

### Permissions

The app must have write access to its own bundle directory. If installed in `/Applications/` by the user (not via `sudo`), this is typically fine. No elevated privileges are ever requested. If permission denied, auto-update is silently disabled for that install.

## Settings Integration

Add `auto_update` field to `AppSettings`:

```rust
pub struct AppSettings {
    // ... existing fields ...
    pub auto_update: bool,  // default: true
}
```

In `settings.ron`:

```ron
(
    // ... existing settings ...
    auto_update: true,
)
```

The `UpdatePlugin` reads this on each poll cycle. Setting `auto_update: false` skips both the check and download.

## Dependencies

Added to `crates/vmux_desktop/Cargo.toml`:

| Crate | Purpose |
|---|---|
| `reqwest` (features: rustls-tls, stream) | HTTP client for GitHub API + download |
| `semver` | Version parsing and comparison |
| `serde_json` | Parse GitHub API response + update-meta.json |
| `flate2` | Gzip decompression |
| `tar` | Tarball extraction |
| `sha2` | SHA-256 checksum verification |

`serde` and `tracing` are already workspace dependencies.

## CI Pipeline Changes

### New step in `release.yml`

After signing/notarization, before GitHub Release creation:

```yaml
- name: Create app bundle tarball
  run: |
    cd target/release
    tar czf "Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz" Vmux.app
    shasum -a 256 "Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz" \
      | awk '{print $1}' > "Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz.sha256"
```

### Updated release upload

```yaml
gh release create "v${VERSION}" \
  --title "Vmux v${VERSION}" \
  --generate-notes \
  "$DMG_PATH" \
  "target/release/vmux-v${VERSION}-aarch64-apple-darwin.tar.gz" \
  "target/release/Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz" \
  "target/release/Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz.sha256"
```

Existing binary tarball and DMG remain unchanged.

## Error Handling

| Scenario | Behavior |
|---|---|
| No internet / GitHub API unreachable | Silent skip, retry on next poll (1hr) |
| GitHub rate limited (HTTP 403) | Silent skip, retry next poll |
| Partial download (app quit mid-download) | `downloading/` dir cleaned up on next attempt |
| Corrupted download (checksum mismatch) | Delete staged file, log warning, retry next poll |
| Permission denied on `.app` swap | Log warning, skip update silently |
| Staged version equals current version | Skip apply, clean up stale staging dir |
| Staged version is older than current | Skip apply, clean up |
| Re-exec fails after successful swap | Non-fatal, swap already done, user relaunches manually |
| `.app.tar.gz` asset missing from release | Silent skip |
| Disk full during download | Download fails, partial file cleaned up on next attempt |

All update failures are silent to the user. Logged at `debug`/`warn` level via `tracing`.
