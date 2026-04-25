# Homebrew Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship vmux as a signed, notarized macOS app distributed via GitHub Releases and Homebrew cask.

**Architecture:** Tag-triggered GitHub Actions workflow builds the app bundle, signs it with Developer ID, notarizes with Apple, packages as DMG, uploads to GitHub Releases, and auto-updates the Homebrew cask formula in `JunichiSugiura/homebrew-vmux`.

**Tech Stack:** GitHub Actions (macOS runner), `codesign`, `xcrun notarytool`, `hdiutil`, Homebrew cask

**Spec:** `docs/specs/2026-04-19-distribution-plan-design.md`

---

### Task 1: Update App Metadata

**Files:**
- Modify: `packaging/macos/Info.plist`
- Modify: `Cargo.toml` (workspace root)
- Modify: `scripts/bundle-macos.sh`

- [ ] **Step 1: Update Info.plist bundle identifier and add icon reference**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>Vmux</string>
	<key>CFBundleExecutable</key>
	<string>Vmux</string>
	<key>CFBundleIdentifier</key>
	<string>ai.vmux.desktop</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Vmux</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>CFBundleIconFile</key>
	<string>Vmux</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
```

- [ ] **Step 2: Update workspace Cargo.toml metadata**

Add these fields to `[workspace.package]`:

```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
description = "Tiling browser with pane multiplexing"
license = "MIT"
homepage = "https://github.com/JunichiSugiura/vmux"
repository = "https://github.com/JunichiSugiura/vmux"
```

- [ ] **Step 3: Update BUNDLE_ID_BASE in bundle-macos.sh**

Change the default value:

```bash
BUNDLE_ID_BASE="${BUNDLE_ID_BASE:-ai.vmux.desktop}"
```

- [ ] **Step 4: Commit**

```bash
git add packaging/macos/Info.plist Cargo.toml scripts/bundle-macos.sh
git commit -m "chore: update bundle ID to ai.vmux.desktop and add workspace metadata"
```

---

### Task 2: Create Entitlements File

**Files:**
- Create: `packaging/macos/Vmux.entitlements`

CEF (Chromium) requires hardened runtime entitlements for its JIT compiler and dynamic library loading.

- [ ] **Step 1: Create entitlements plist**

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

- [ ] **Step 2: Commit**

```bash
git add packaging/macos/Vmux.entitlements
git commit -m "chore: add hardened runtime entitlements for CEF"
```

---

### Task 3: Create Signing and Notarization Script

**Files:**
- Create: `scripts/sign-and-notarize.sh`

This script is called both by CI and locally. It expects environment variables for credentials.

- [ ] **Step 1: Create the script**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Sign and notarize Vmux.app for macOS distribution.
#
# Required environment variables:
#   APPLE_SIGNING_IDENTITY  - "Developer ID Application: Name (TEAM_ID)"
#   APPLE_ID                - Apple ID email for notarytool
#   APPLE_APP_PASSWORD      - App-specific password
#   APPLE_TEAM_ID           - 10-character team identifier
#
# Optional:
#   APP_BUNDLE              - Path to .app (default: build/Vmux.app)
#   SKIP_NOTARIZE           - Set to "1" to skip notarization (local testing)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/build/Vmux.app}"
ENTITLEMENTS="$ROOT/packaging/macos/Vmux.entitlements"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found. Run scripts/bundle-macos.sh first." >&2
    exit 1
fi

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    echo "Error: APPLE_SIGNING_IDENTITY not set." >&2
    echo "  Example: \"Developer ID Application: Your Name (XXXXXXXXXX)\"" >&2
    exit 1
fi

echo "==> Signing $APP_BUNDLE"

# Sign the CEF framework and helper binaries first (inside-out signing)
# Find all Mach-O binaries and .dylib files in Frameworks
find "$APP_BUNDLE/Contents/Frameworks" -type f \( -name "*.dylib" -o -perm +111 \) | while read -r binary; do
    # Skip non-Mach-O files
    file "$binary" | grep -q "Mach-O" || continue
    echo "  Signing: ${binary#$APP_BUNDLE/}"
    codesign --force --verify --verbose \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$binary"
done

# Sign the framework bundle itself
if [ -d "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework" ]; then
    echo "  Signing: Chromium Embedded Framework.framework"
    codesign --force --verify --verbose \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework"
fi

# Sign the main app bundle
echo "  Signing: Vmux.app"
codesign --force --verify --verbose \
    --sign "$APPLE_SIGNING_IDENTITY" \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    "$APP_BUNDLE"

# Verify
echo "==> Verifying signature"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

if [ "${SKIP_NOTARIZE:-}" = "1" ]; then
    echo "==> Skipping notarization (SKIP_NOTARIZE=1)"
    exit 0
fi

# Notarize
if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_APP_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
    echo "Error: APPLE_ID, APPLE_APP_PASSWORD, and APPLE_TEAM_ID must be set for notarization." >&2
    exit 1
fi

echo "==> Creating zip for notarization"
NOTARIZE_ZIP="$ROOT/build/Vmux-notarize.zip"
ditto -c -k --keepParent "$APP_BUNDLE" "$NOTARIZE_ZIP"

echo "==> Submitting for notarization (this may take several minutes)"
xcrun notarytool submit "$NOTARIZE_ZIP" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait

echo "==> Stapling notarization ticket"
xcrun stapler staple "$APP_BUNDLE"

echo "==> Verifying notarization"
spctl --assess --type execute --verbose "$APP_BUNDLE"

rm -f "$NOTARIZE_ZIP"
echo "Done: $APP_BUNDLE is signed and notarized."
```

- [ ] **Step 2: Make executable**

```bash
chmod +x scripts/sign-and-notarize.sh
```

- [ ] **Step 3: Commit**

```bash
git add scripts/sign-and-notarize.sh
git commit -m "feat: add signing and notarization script"
```

---

### Task 4: Create DMG Packaging Script

**Files:**
- Create: `scripts/create-dmg.sh`

- [ ] **Step 1: Create the script**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Create a DMG from Vmux.app.
#
# Optional:
#   APP_BUNDLE  - Path to .app (default: build/Vmux.app)
#   VERSION     - Version string (default: read from Info.plist)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/build/Vmux.app}"
VERSION="${VERSION:-$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP_BUNDLE/Contents/Info.plist")}"
DMG_NAME="Vmux-${VERSION}-mac.dmg"
DMG_PATH="$ROOT/build/$DMG_NAME"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found." >&2
    exit 1
fi

echo "==> Creating DMG: $DMG_NAME"

# Use create-dmg if available (prettier result), otherwise fall back to hdiutil
if command -v create-dmg >/dev/null 2>&1; then
    # Remove existing DMG (create-dmg fails if it exists)
    rm -f "$DMG_PATH"

    ICON_ARGS=()
    if [ -f "$ROOT/packaging/macos/Vmux.icns" ]; then
        ICON_ARGS=(--volicon "$ROOT/packaging/macos/Vmux.icns")
    fi

    create-dmg \
        --volname "Vmux" \
        "${ICON_ARGS[@]}" \
        --window-pos 200 120 \
        --window-size 600 400 \
        --icon-size 100 \
        --icon "Vmux.app" 150 190 \
        --app-drop-link 450 190 \
        --hide-extension "Vmux.app" \
        "$DMG_PATH" \
        "$APP_BUNDLE"
else
    echo "  (create-dmg not found, using hdiutil fallback)"
    rm -f "$DMG_PATH"
    hdiutil create -volname "Vmux" -srcfolder "$APP_BUNDLE" \
        -ov -format UDZO "$DMG_PATH"
fi

echo "Done: $DMG_PATH"
```

- [ ] **Step 2: Make executable**

```bash
chmod +x scripts/create-dmg.sh
```

- [ ] **Step 3: Commit**

```bash
git add scripts/create-dmg.sh
git commit -m "feat: add DMG creation script"
```

---

### Task 5: Create GitHub Actions Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`

This workflow triggers on `v*` tags, builds the app, signs, notarizes, creates DMG, and uploads to GitHub Releases.

**Required GitHub Secrets (set manually in repo settings):**
- `APPLE_CERTIFICATE` - Base64-encoded .p12 Developer ID certificate
- `APPLE_CERTIFICATE_PASSWORD` - Password for the .p12
- `APPLE_ID` - Apple ID email
- `APPLE_APP_PASSWORD` - App-specific password from appleid.apple.com
- `APPLE_TEAM_ID` - 10-character team ID
- `APPLE_SIGNING_IDENTITY` - Full identity string, e.g. "Developer ID Application: Name (TEAM_ID)"

- [ ] **Step 1: Create the workflow file**

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  release-macos:
    runs-on: macos-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Extract version from tag
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          echo "VERSION=$VERSION" >> "$GITHUB_ENV"
          echo "Building version: $VERSION"

      - name: Verify Cargo.toml version matches tag
        run: |
          CARGO_VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
          if [ "$CARGO_VERSION" != "$VERSION" ]; then
            echo "Error: Cargo.toml version ($CARGO_VERSION) does not match tag ($VERSION)" >&2
            exit 1
          fi

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Cache cargo registry and build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-

      - name: Install dioxus-cli
        run: cargo install dioxus-cli --locked --version 0.7.4

      - name: Install CEF framework
        run: |
          cargo install export-cef-dir@145.6.1+145.0.28 --force
          export-cef-dir --force "$HOME/.local/share"

      - name: Install bevy_cef tools
        run: |
          cargo install bevy_cef_render_process
          cargo install bevy_cef_bundle_app

      - name: Patch Info.plist version
        run: |
          /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" \
            packaging/macos/Info.plist
          /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" \
            packaging/macos/Info.plist

      - name: Build and bundle app
        run: ./scripts/bundle-macos.sh

      - name: Import signing certificate
        env:
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
        run: |
          echo "$APPLE_CERTIFICATE" | base64 --decode > cert.p12
          security create-keychain -p "" build.keychain
          security default-keychain -s build.keychain
          security unlock-keychain -p "" build.keychain
          security import cert.p12 -k build.keychain \
            -P "$APPLE_CERTIFICATE_PASSWORD" \
            -T /usr/bin/codesign
          security set-key-partition-list -S apple-tool:,apple: \
            -k "" build.keychain
          security list-keychains -d user -s build.keychain
          rm cert.p12

      - name: Sign and notarize
        env:
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_APP_PASSWORD: ${{ secrets.APPLE_APP_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
        run: ./scripts/sign-and-notarize.sh

      - name: Create DMG
        run: |
          brew install create-dmg
          VERSION="$VERSION" ./scripts/create-dmg.sh

      - name: Create binary tarball for auto-updater
        run: |
          cd build/Vmux.app/Contents/MacOS
          tar czf "../../../../vmux_desktop-v${VERSION}-aarch64-apple-darwin.tar.gz" vmux_desktop

      - name: Create GitHub Release
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release create "$GITHUB_REF_NAME" \
            --title "Vmux v${VERSION}" \
            --generate-notes \
            "build/Vmux-${VERSION}-mac.dmg" \
            "build/vmux_desktop-v${VERSION}-aarch64-apple-darwin.tar.gz"

      - name: Update Homebrew cask
        env:
          GH_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
        run: |
          DMG_SHA=$(shasum -a 256 "build/Vmux-${VERSION}-mac.dmg" | awk '{print $1}')

          # Clone the tap repo
          git clone "https://x-access-token:${GH_TOKEN}@github.com/JunichiSugiura/homebrew-vmux.git" /tmp/homebrew-vmux

          cat > /tmp/homebrew-vmux/Casks/vmux.rb << EOF
          cask "vmux" do
            version "${VERSION}"
            sha256 "${DMG_SHA}"

            url "https://github.com/JunichiSugiura/vmux/releases/download/v#{version}/Vmux-#{version}-mac.dmg"
            name "Vmux"
            desc "Tiling browser with pane multiplexing"
            homepage "https://github.com/JunichiSugiura/vmux"

            app "Vmux.app"
          end
          EOF

          cd /tmp/homebrew-vmux
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Casks/vmux.rb
          git commit -m "Update vmux to ${VERSION}"
          git push
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "feat: add GitHub Actions release workflow"
```

---

### Task 6: Create Homebrew Tap Repository

This task is done outside the vmux repo. You need to create the `JunichiSugiura/homebrew-vmux` repository on GitHub with an initial cask formula.

- [ ] **Step 1: Create the repo on GitHub**

Go to https://github.com/new and create `homebrew-vmux` (public repo).

- [ ] **Step 2: Create initial cask formula**

Clone the repo and create `Casks/vmux.rb`:

```ruby
cask "vmux" do
  version "0.1.0"
  sha256 "PLACEHOLDER"

  url "https://github.com/JunichiSugiura/vmux/releases/download/v#{version}/Vmux-#{version}-mac.dmg"
  name "Vmux"
  desc "Tiling browser with pane multiplexing"
  homepage "https://github.com/JunichiSugiura/vmux"

  app "Vmux.app"
end
```

- [ ] **Step 3: Create a GitHub Personal Access Token for tap updates**

Go to https://github.com/settings/tokens and create a fine-grained token with:
- Repository access: `JunichiSugiura/homebrew-vmux` only
- Permissions: Contents (read & write)

Add this token as `HOMEBREW_TAP_TOKEN` secret in the `vmux` repository settings.

- [ ] **Step 4: Push initial cask and verify tap works**

```bash
cd homebrew-vmux
git add Casks/vmux.rb
git commit -m "Initial cask formula"
git push

# Verify
brew tap JunichiSugiura/vmux
brew info --cask vmux
```

---

### Task 7: Set Up GitHub Secrets

This is a manual step. Add these secrets in the vmux repo at Settings > Secrets and variables > Actions:

- [ ] **Step 1: Export Developer ID certificate as .p12**

Open Keychain Access, find "Developer ID Application" certificate, right-click > Export. Save as `cert.p12` with a password.

```bash
# Base64 encode it
base64 -i cert.p12 -o cert.b64
# Copy contents of cert.b64
cat cert.b64 | pbcopy
```

- [ ] **Step 2: Add all required secrets**

| Secret Name | Value |
|-------------|-------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 content |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting .p12 |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAM_ID)` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_APP_PASSWORD` | Generate at https://appleid.apple.com/account/manage > App-Specific Passwords |
| `APPLE_TEAM_ID` | 10-character team ID from developer.apple.com/account |
| `HOMEBREW_TAP_TOKEN` | GitHub PAT from Task 6 Step 3 |

---

### Task 8: Local Test Run

Before pushing a tag to trigger CI, verify the scripts work locally.

- [ ] **Step 1: Build the app bundle**

```bash
make bundle-mac
```

Expected: `build/Vmux.app` exists with CEF framework inside.

- [ ] **Step 2: Test signing locally (without notarization)**

```bash
APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAM_ID)" \
SKIP_NOTARIZE=1 \
./scripts/sign-and-notarize.sh
```

Expected: "Done: build/Vmux.app is signed" (no notarization errors).

- [ ] **Step 3: Verify signature**

```bash
codesign --verify --deep --strict --verbose=2 build/Vmux.app
```

Expected: "valid on disk" and "satisfies its Designated Requirement".

- [ ] **Step 4: Test DMG creation**

```bash
VERSION=0.1.0 ./scripts/create-dmg.sh
```

Expected: `build/Vmux-0.1.0-mac.dmg` exists. Mount it and verify Vmux.app is inside with Applications symlink.

- [ ] **Step 5: Test full pipeline including notarization**

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAM_ID)"
export APPLE_ID="your@email.com"
export APPLE_APP_PASSWORD="xxxx-xxxx-xxxx-xxxx"
export APPLE_TEAM_ID="XXXXXXXXXX"
./scripts/sign-and-notarize.sh
```

Expected: Notarization succeeds, staple applied, `spctl --assess` passes.

---

### Task 9: First Release

- [ ] **Step 1: Tag and push**

```bash
git tag v0.1.0
git push origin v0.1.0
```

- [ ] **Step 2: Monitor GitHub Actions**

Go to https://github.com/JunichiSugiura/vmux/actions and watch the Release workflow.

Expected: All steps pass. GitHub Release created with DMG and .tar.gz assets.

- [ ] **Step 3: Verify Homebrew cask was updated**

```bash
brew tap JunichiSugiura/vmux
brew install --cask vmux
```

Expected: Vmux.app installed to /Applications.

- [ ] **Step 4: Verify Gatekeeper passes**

```bash
open /Applications/Vmux.app
```

Expected: App opens without Gatekeeper warning.
