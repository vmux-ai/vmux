# macOS Distribution Plan

## Overview

Ship vmux as a signed, notarized macOS app for public beta. Users download a DMG from GitHub Releases, drag Vmux.app to Applications, and run it. Homebrew cask provides an alternative install path. GitHub Actions automates the entire build-sign-notarize-release pipeline on each tagged version.

## Scope

- macOS only (Apple Silicon initially, universal binary deferred)
- Signed and notarized via Apple Developer ID
- DMG installer with Applications symlink
- GitHub Actions CI/CD triggered by version tags
- Homebrew cask for `brew install --cask vmux`
- In-app auto-updater via `self_update` crate (checks GitHub Releases)

## Prerequisites

### Apple Developer Account

Enroll in the Apple Developer Program ($99/yr). Required artifacts:

| Artifact | Purpose | Where stored |
|----------|---------|-------------|
| Developer ID Application certificate | Code signing the .app bundle | macOS Keychain / GitHub secret (p12 export) |
| Developer ID Installer certificate | Signing the .pkg if needed later | Same |
| App-specific password | `notarytool` authentication | GitHub secret `APPLE_APP_PASSWORD` |
| Team ID | Identifies the signing team | GitHub secret `APPLE_TEAM_ID` |
| Apple ID email | Notarization account | GitHub secret `APPLE_ID` |

### App Identity

Update the placeholder bundle ID and add missing metadata.

**`packaging/macos/Info.plist`:**
- `CFBundleIdentifier`: `com.yourorg.vmux` -> `ai.vmux.desktop`
- `CFBundleShortVersionString`: sync with `Cargo.toml` workspace version (currently hardcoded `0.1.0`)
- `CFBundleIconFile`: add once `.icns` is created
- Add `NSCameraUsageDescription`, `NSMicrophoneUsageDescription` if CEF may request permissions

**`Cargo.toml` (workspace):**
- Add `description`, `license`, `homepage`, `repository` fields

### App Icon

Create `packaging/macos/Vmux.icns` containing all required sizes (16x16 through 1024x1024). Add `CFBundleIconFile` entry to Info.plist pointing to `Vmux.icns`.

## Build Pipeline

### Trigger

```
git tag v0.1.0
git push --tags
  -> GitHub Actions workflow triggers on tags matching `v*`
```

### Workflow Steps

```
1.  Checkout code
2.  Install Rust toolchain (stable) + wasm32-unknown-unknown target
3.  Install dioxus-cli (dx)
4.  Download/cache CEF framework
5.  cargo install bevy_cef_render_process bevy_cef_bundle_app
6.  Run scripts/bundle-macos.sh -> build/Vmux.app
7.  Code sign
8.  Notarize
9.  Staple
10. Create DMG
11. Create .tar.gz of vmux_desktop binary (for self_update)
12. Upload DMG + .tar.gz to GitHub Release
```

Step 11 packages the signed binary for the auto-updater:

```bash
cd build/Vmux.app/Contents/MacOS
tar czf "../../../../vmux_desktop-v${VERSION}-aarch64-apple-darwin.tar.gz" vmux_desktop
```

### Step Details

#### Code Signing

```bash
# Import certificate from GitHub secret (base64-encoded .p12)
echo "$APPLE_CERTIFICATE" | base64 --decode > cert.p12
security create-keychain -p "" build.keychain
security import cert.p12 -k build.keychain -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -k "" build.keychain
security list-keychains -d user -s build.keychain

# Sign all nested frameworks first (CEF), then the app
codesign --deep --force --verify --verbose \
  --sign "Developer ID Application: <TEAM_NAME> (<TEAM_ID>)" \
  --options runtime \
  --entitlements packaging/macos/Vmux.entitlements \
  build/Vmux.app
```

The `--options runtime` flag enables the hardened runtime, required for notarization.

#### Entitlements

Create `packaging/macos/Vmux.entitlements`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
  <true/>
  <key>com.apple.security.cs.allow-jit</key>
  <true/>
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
  <key>com.apple.security.network.client</key>
  <true/>
</dict>
</plist>
```

CEF (Chromium) requires `allow-unsigned-executable-memory`, `allow-jit`, and `disable-library-validation` for its JIT compiler and dynamic library loading.

#### Notarization

```bash
# Create a zip for notarization submission
ditto -c -k --keepParent build/Vmux.app build/Vmux.zip

# Submit
xcrun notarytool submit build/Vmux.zip \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_APP_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait

# Staple the ticket to the app
xcrun stapler staple build/Vmux.app
```

#### DMG Creation

Using `create-dmg` (pure bash, installed via `brew install create-dmg`):

```bash
create-dmg \
  --volname "Vmux" \
  --volicon "packaging/macos/Vmux.icns" \
  --window-pos 200 120 \
  --window-size 600 400 \
  --icon-size 100 \
  --icon "Vmux.app" 150 190 \
  --app-drop-link 450 190 \
  --hide-extension "Vmux.app" \
  "build/Vmux-${VERSION}-mac.dmg" \
  "build/Vmux.app"
```

Fallback (no external deps):

```bash
hdiutil create -volname "Vmux" -srcfolder build/Vmux.app \
  -ov -format UDZO "build/Vmux-${VERSION}-mac.dmg"
```

#### GitHub Release

```bash
gh release create "v${VERSION}" \
  --title "Vmux v${VERSION}" \
  --generate-notes \
  "build/Vmux-${VERSION}-mac.dmg" \
  "build/vmux_desktop-v${VERSION}-aarch64-apple-darwin.tar.gz"
```

### Version Synchronization

The workflow extracts the version from the git tag and patches Info.plist before building:

```bash
VERSION="${GITHUB_REF_NAME#v}"  # v0.1.0 -> 0.1.0
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${VERSION}" \
  packaging/macos/Info.plist
```

Cargo.toml workspace version must match the tag. The workflow should verify this and fail if they diverge.

## Auto-Updater

### Mechanism

The `self_update` crate checks GitHub Releases for a newer version on app startup. If found, it downloads the new `.tar.gz` asset and replaces the binary inside `Vmux.app`.

### Dependency

```toml
# crates/vmux_desktop/Cargo.toml
[dependencies]
self_update = { version = "0.42", features = ["archive-tar", "compression-flate2", "rustls"] }
```

The `rustls` feature avoids linking OpenSSL. `archive-tar` + `compression-flate2` handle `.tar.gz` assets (the release workflow uploads a `Vmux-{version}-mac.tar.gz` containing the signed app bundle).

### Integration

Add an `UpdatePlugin` in a new file `crates/vmux_desktop/src/updater.rs`:

```rust
use bevy::prelude::*;
use std::thread;

pub struct UpdatePlugin;

impl Plugin for UpdatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, check_for_updates);
    }
}

fn check_for_updates() {
    thread::spawn(|| {
        let result = self_update::backends::github::Update::configure()
            .repo_owner("vmux-ai")
            .repo_name("vmux")
            .bin_name("vmux_desktop")
            .current_version(self_update::cargo_crate_version!())
            .no_confirm(true)
            .show_output(false)
            .build()
            .and_then(|u| u.update());

        match result {
            Ok(status) => {
                if status.updated() {
                    info!("Updated to {}", status.version());
                    // Future: notify user to restart via header UI
                }
            }
            Err(e) => {
                warn!("Update check failed: {e}");
            }
        }
    });
}
```

Key design decisions:

- **Background thread**: The update check runs on a separate thread at startup so it never blocks the Bevy event loop or delays app launch.
- **No confirmation dialog**: For beta, auto-download silently. The update replaces the binary on disk; the user gets the new version on next launch.
- **Graceful failure**: Network errors, rate limits, or offline usage just log a warning. The app continues normally.
- **Version comparison**: `self_update` uses semver. The GitHub Release tag (`v0.2.0`) is compared against `CARGO_PKG_VERSION` compiled into the binary.

### Release Asset Naming

`self_update` expects a predictable asset name pattern. The CI workflow must upload a `.tar.gz` with the naming convention:

```
vmux_desktop-v{VERSION}-{TARGET}.tar.gz
```

For example: `vmux_desktop-v0.2.0-aarch64-apple-darwin.tar.gz`

The `.tar.gz` contains the `vmux_desktop` binary (extracted from `Vmux.app/Contents/MacOS/`). `self_update` replaces the running binary in-place via `self_replace`.

### Limitations

- Replaces only the main binary, not CEF framework or WASM assets bundled in the `.app`. Full app updates (CEF version bumps, new WASM components) require a fresh DMG download.
- No delta updates. Downloads the full binary each time (~50-100 MB).
- No user-facing UI yet. Future work: show "Update available, restart to apply" in the header bar.

### Future Improvements

- Add a "Check for updates" menu item / header button
- Show update notification in the header bar when a new version is downloaded
- Replace the full `.app` bundle instead of just the binary (handles CEF + WASM updates)
- Rate-limit update checks to once per 24 hours (store last check timestamp)

## Distribution Channels

### GitHub Releases (primary)

Every tagged version produces a DMG attached to a GitHub Release with auto-generated release notes.

### Homebrew Cask

The cask formula lives in this repo under `Casks/vmux.rb`:

```ruby
cask "vmux" do
  version "0.1.0"
  sha256 "HASH_OF_DMG"

  url "https://github.com/vmux-ai/vmux/releases/download/v#{version}/Vmux-#{version}-mac.dmg"
  name "Vmux"
  desc "Tiling browser with pane multiplexing"
  homepage "https://github.com/vmux-ai/vmux"

  app "Vmux.app"
end
```

Usage: `brew tap vmux-ai/vmux https://github.com/vmux-ai/vmux && brew install --cask vmux`

The GitHub Actions workflow can auto-update the cask SHA and version after a release.

### Landing Page (optional, later)

A simple static site (GitHub Pages) with download button, screenshot, and install instructions. Not required for initial beta.

## GitHub Actions Secrets

| Secret | Value |
|--------|-------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 Developer ID certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the .p12 file |
| `APPLE_ID` | Apple ID email for notarytool |
| `APPLE_APP_PASSWORD` | App-specific password (generated at appleid.apple.com) |
| `APPLE_TEAM_ID` | 10-character team identifier |

## Files to Create

| File | Purpose |
|------|---------|
| `.github/workflows/release.yml` | CI/CD workflow triggered on `v*` tags |
| `packaging/macos/Vmux.entitlements` | Hardened runtime entitlements for CEF |
| `packaging/macos/Vmux.icns` | App icon (all sizes) |
| `scripts/sign-and-notarize.sh` | Signing + notarization script (called by CI and locally) |
| `crates/vmux_desktop/src/updater.rs` | Auto-update check on startup via self_update |

## Files to Modify

| File | Change |
|------|--------|
| `packaging/macos/Info.plist` | Real bundle ID, icon file reference, version placeholder |
| `Cargo.toml` (workspace) | Add description, license, homepage, repository |
| `crates/vmux_desktop/Cargo.toml` | Add `self_update` dependency |
| `crates/vmux_desktop/src/lib.rs` | Register `UpdatePlugin` |
| `scripts/bundle-macos.sh` | Update BUNDLE_ID_BASE default |

## Deferred

| Item | Reason |
|------|--------|
| Full .app bundle replacement via updater | self_update replaces binary only; CEF/WASM changes need DMG |
| Update notification UI | No header bar indicator yet; future work |
| Universal binary (arm64 + x86_64) | GitHub Actions macOS runners are arm64; Intel build needs cross-compilation or separate runner |
| Linux / Windows builds | macOS only for initial release |
| .pkg installer | DMG is sufficient |
| App Store distribution | Requires sandboxing, which conflicts with CEF |
