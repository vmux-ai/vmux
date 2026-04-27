# PR Build Artifact

## Goal

Every PR produces a downloadable signed+notarized `.dmg` so reviewers can test the app locally.

## Trigger

- `push` to `main`
- `pull_request` targeting `main`

Same trigger as existing `lint`, `test`, `website` jobs.

## New CI Job: `build-mac`

Runner: `macos-latest`

### Steps

1. **Checkout** — `actions/checkout@v4`
2. **Rust toolchain** — `dtolnay/rust-toolchain@stable` with `wasm32-unknown-unknown` target
3. **Cache** — cargo registry/bin + `target/` (same pattern as existing jobs, key prefix `build`)
4. **Install CEF** — `cargo install export-cef-dir@145.6.1+145.0.28 --force` + `export-cef-dir --force "$HOME/.local/share"`
5. **Install dioxus-cli** — `cargo install dioxus-cli --locked --version 0.7.4`
6. **Install bevy_cef_bundle_app** — needed by `inject-cef.sh`
7. **Install cargo-packager** — `cargo install cargo-packager --locked`
8. **Install bevy_cef_bundle_app** — `cargo install bevy_cef_bundle_app --locked`
9. **Package** — `env -u CEF_PATH cargo packager --release` (produces `.app` + `.dmg`)
10. **Import signing certificate** — decode `APPLE_CERTIFICATE_BASE64` into temp keychain
11. **Sign + notarize** — `scripts/sign-and-notarize.sh` with env vars from secrets
12. **Re-create .dmg** — the `.dmg` from step 9 contains an unsigned `.app` (signing happens after packaging). Re-create the `.dmg` from the now-signed `.app` using `hdiutil`.
13. **Upload artifact** — `actions/upload-artifact@v4` with `.dmg`, 7-day retention

### Signing Certificate Import

Standard pattern for macOS CI:

```yaml
- name: Import signing certificate
  if: env.APPLE_CERTIFICATE_BASE64 != ''
  env:
    APPLE_CERTIFICATE_BASE64: ${{ secrets.APPLE_CERTIFICATE_BASE64 }}
    APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
  run: |
    CERT_FILE=$(mktemp /tmp/cert.XXXXXX.p12)
    echo "$APPLE_CERTIFICATE_BASE64" | base64 --decode > "$CERT_FILE"
    KEYCHAIN="build.keychain-db"
    KEYCHAIN_PASSWORD=$(uuidgen)
    security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security set-keychain-settings -lut 21600 "$KEYCHAIN"
    security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security import "$CERT_FILE" -P "$APPLE_CERTIFICATE_PASSWORD" -A -t cert -f pkcs12 -k "$KEYCHAIN"
    security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')
    rm -f "$CERT_FILE"
```

### Conditional Signing

Signing and notarization steps use `if: env.APPLE_CERTIFICATE_BASE64 != ''` so fork PRs still produce unsigned `.dmg` builds.

### Artifact Upload

```yaml
- name: Upload .dmg
  uses: actions/upload-artifact@v4
  with:
    name: Vmux-${{ github.sha }}.dmg
    path: target/release/*.dmg
    retention-days: 7
```

## Required GitHub Secrets

| Secret | Purpose |
|--------|---------|
| `APPLE_CERTIFICATE_BASE64` | Developer ID cert (.p12) base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the .p12 file |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Junichi Sugiura (4BG7FRMF2G)` |
| `APPLE_ID` | Apple ID email for notarization |
| `APPLE_APP_PASSWORD` | App-specific password from appleid.apple.com |
| `APPLE_TEAM_ID` | 10-character team identifier |

## DMG Re-creation After Signing

`cargo packager` creates the `.dmg` before `sign-and-notarize.sh` runs, so the `.dmg` contains an unsigned `.app`. After signing, we re-create the `.dmg`:

```bash
# Remove the old unsigned .dmg
rm -f target/release/*.dmg

# Create new .dmg from signed .app
hdiutil create -volname "Vmux" \
  -srcfolder target/release/Vmux.app \
  -ov -format UDZO \
  target/release/Vmux.dmg
```

This produces a simpler `.dmg` (no custom background/icon positions) but contains the properly signed `.app`. A future improvement could replicate the full DMG layout from the packager config.

## Files Changed

Only `.github/workflows/ci.yml` — append the `build-mac` job.

## What Does NOT Change

- Makefile
- scripts/sign-and-notarize.sh
- scripts/inject-cef.sh
- Any Rust source code
- Existing CI jobs (lint, test, website)
