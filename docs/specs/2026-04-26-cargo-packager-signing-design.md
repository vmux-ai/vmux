# Replace sign-and-notarize.sh with cargo-packager Signing

## Summary

Remove the custom `scripts/sign-and-notarize.sh` and delegate codesigning + notarization to cargo-packager's built-in support. Add a post-packaging verification script as a safety net.

## Motivation

`sign-and-notarize.sh` duplicates functionality already available in cargo-packager. Consolidating reduces maintenance surface and lets cargo-packager sign both the `.app` and `.dmg` in a single pass.

## Current State

```
cargo packager --release   (unsigned)
  -> sign-and-notarize.sh  (codesign + notarize + staple + verify)
```

`sign-and-notarize.sh` does:
1. Inside-out codesigning (frameworks, helpers, main app) with hardened runtime + entitlements
2. Post-sign verification (`codesign --verify --deep --strict`)
3. Notarization via `xcrun notarytool submit --wait`
4. Stapling via `xcrun stapler staple`
5. Gatekeeper check (`spctl --assess`)

Environment variables used:
- `APPLE_SIGNING_IDENTITY`
- `APPLE_ID`, `APPLE_APP_PASSWORD`, `APPLE_TEAM_ID`
- `SKIP_NOTARIZE` (local dev escape hatch)

## New State

```
cargo packager --release   (signs + notarizes if env vars present)
  -> verify-signature.sh   (safety net verification)
```

### Cargo.toml Config

```toml
[package.metadata.packager.macos]
minimum-system-version = "13.0"
signing-identity = "Developer ID Application: Junichi Sugiura (TEAM_ID)"
entitlements = "../../packaging/macos/Vmux.entitlements"
```

`signing-identity` can also be set via the `APPLE_SIGNING_IDENTITY` env var if preferred over hardcoding in the config. The value in config is used when present; otherwise cargo-packager falls back to the system keychain.

### Environment Variables

| Variable | Purpose | Where |
|---|---|---|
| `APPLE_ID` | Apple ID email | `.env` / GH secrets |
| `APPLE_PASSWORD` | App-specific password | `.env` / GH secrets |
| `APPLE_TEAM_ID` | 10-char team ID | `.env` / GH secrets |

Note: cargo-packager uses `APPLE_PASSWORD`, not `APPLE_APP_PASSWORD` (the old script's name). Update `.env` accordingly.

For CI, `APPLE_CERTIFICATE` (base64 p12) and `APPLE_CERTIFICATE_PASSWORD` can be used instead of relying on a pre-installed keychain identity.

### Makefile

```makefile
package-mac:
	env -u CEF_PATH cargo packager --release

build-local-mac: package-mac
	@echo "Verifying signature..."
	bash scripts/verify-signature.sh
```

Local dev without signing: unset `APPLE_*` vars or don't load `.env`. cargo-packager skips signing when no identity is configured.

### verify-signature.sh (new, replaces sign-and-notarize.sh)

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"
DMG_PATH="${DMG_PATH:-$ROOT/target/release/Vmux_0.1.0_aarch64.dmg}"

echo "==> Verifying .app signature"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

echo "==> Gatekeeper assessment"
spctl --assess --type execute --verbose "$APP_BUNDLE" 2>&1 || {
    echo "WARNING: spctl assessment failed. App may not be notarized."
    exit 1
}

if [[ -f "$DMG_PATH" ]]; then
    echo "==> Verifying .dmg signature"
    codesign --verify --verbose=2 "$DMG_PATH" 2>&1 || {
        echo "WARNING: DMG is not signed."
    }
fi

echo "==> Signature verification passed"
```

## Risk: CEF Hardened Runtime

cargo-packager signs framework bundles without `--options runtime` (hardened runtime). Individual Mach-O files inside frameworks DO get hardened runtime. Apple notarization requires hardened runtime on all executable code.

**Mitigation**: The `verify-signature.sh` step will catch this. If notarization fails:
1. `spctl --assess` will report the failure
2. We can either patch cargo-packager upstream or add a pre-signing step that signs CEF frameworks with `--options runtime` before cargo-packager runs

## Files Changed

| File | Action |
|---|---|
| `crates/vmux_desktop/Cargo.toml` | Add `signing-identity`, `entitlements` to `[package.metadata.packager.macos]` |
| `scripts/verify-signature.sh` | New file |
| `scripts/sign-and-notarize.sh` | Delete |
| `Makefile` | Update `build-local-mac` to use `verify-signature.sh` |
| `.env` | Rename `APPLE_APP_PASSWORD` to `APPLE_PASSWORD` |

## Rollback

If cargo-packager signing proves insufficient for CEF, restore `sign-and-notarize.sh` from git history and revert the Makefile change.
