# cargo-packager Signing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `scripts/sign-and-notarize.sh` with cargo-packager's built-in codesigning and notarization, adding a verification script as safety net.

**Architecture:** Configure `signing-identity` and `entitlements` in `Cargo.toml` so cargo-packager handles signing/notarization during packaging. Add `scripts/verify-signature.sh` for post-packaging verification. Rename `.env` var `APPLE_APP_PASSWORD` to `APPLE_PASSWORD` to match cargo-packager's expected name.

**Tech Stack:** cargo-packager, macOS codesign, xcrun notarytool

---

### Task 1: Update Cargo.toml with signing config

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml:60-61`

- [ ] **Step 1: Add signing-identity and entitlements to macos config**

In `crates/vmux_desktop/Cargo.toml`, replace:

```toml
[package.metadata.packager.macos]
minimum-system-version = "13.0"
```

with:

```toml
[package.metadata.packager.macos]
minimum-system-version = "13.0"
signing-identity = "Developer ID Application: Junichi Sugiura (4BG7FRMF2G)"
entitlements = "../../packaging/macos/Vmux.entitlements"
```

- [ ] **Step 2: Verify Cargo.toml parses correctly**

Run: `cd crates/vmux_desktop && cargo metadata --format-version 1 --no-deps 2>&1 | head -5`
Expected: JSON output without errors

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml
git commit -m "feat: configure cargo-packager signing identity and entitlements"
```

---

### Task 2: Update .env for cargo-packager variable names

**Files:**
- Modify: `.env`

- [ ] **Step 1: Rename APPLE_APP_PASSWORD to APPLE_PASSWORD**

In `.env`, replace:

```
APPLE_APP_PASSWORD=rsfz-rfqr-xqnn-qtec
```

with:

```
APPLE_PASSWORD=rsfz-rfqr-xqnn-qtec
```

cargo-packager reads `APPLE_PASSWORD`, not `APPLE_APP_PASSWORD`.

The `APPLE_SIGNING_IDENTITY` env var can be removed since the identity is now in `Cargo.toml`. But keeping it is harmless -- cargo-packager uses the config value.

- [ ] **Step 2: Verify .env loads correctly**

Run: `bash -c "source .env && echo APPLE_PASSWORD=\${APPLE_PASSWORD:+SET}"`
Expected: `APPLE_PASSWORD=SET`

Note: `.env` is gitignored, no commit needed.

---

### Task 3: Create verify-signature.sh

**Files:**
- Create: `scripts/verify-signature.sh`

- [ ] **Step 1: Write the verification script**

Create `scripts/verify-signature.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Verify codesigning and notarization after cargo-packager.
# Runs as a safety net to catch any signing gaps (e.g. CEF framework hardened runtime).

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/target/release/Vmux.app}"

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "verify-signature: $APP_BUNDLE not found" >&2
    exit 1
fi

echo "==> Verifying .app codesign"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

echo "==> Gatekeeper assessment"
if ! spctl --assess --type execute --verbose "$APP_BUNDLE" 2>&1; then
    echo "WARNING: spctl assessment failed. App may not be notarized." >&2
    exit 1
fi

# Find and verify DMG if it exists
DMG_PATH=$(find "$ROOT/target/release" -name "Vmux_*.dmg" -maxdepth 1 | head -1)
if [[ -n "$DMG_PATH" && -f "$DMG_PATH" ]]; then
    echo "==> Verifying .dmg codesign"
    codesign --verify --verbose=2 "$DMG_PATH" || {
        echo "WARNING: DMG is not signed." >&2
    }
fi

echo "==> Signature verification passed"
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/verify-signature.sh`

- [ ] **Step 3: Commit**

```bash
git add scripts/verify-signature.sh
git commit -m "feat: add post-packaging signature verification script"
```

---

### Task 4: Update Makefile and delete sign-and-notarize.sh

**Files:**
- Modify: `Makefile:33-35`
- Delete: `scripts/sign-and-notarize.sh`

- [ ] **Step 1: Update build-local-mac target**

In `Makefile`, replace:

```makefile
build-local-mac: package-mac
	@echo "Signing..."
	SKIP_NOTARIZE=1 ./scripts/sign-and-notarize.sh
```

with:

```makefile
build-local-mac: package-mac
	@echo "Verifying signature..."
	bash scripts/verify-signature.sh
```

- [ ] **Step 2: Delete sign-and-notarize.sh**

Run: `rm scripts/sign-and-notarize.sh`

- [ ] **Step 3: Commit**

```bash
git add Makefile
git rm scripts/sign-and-notarize.sh
git commit -m "feat: replace sign-and-notarize.sh with cargo-packager signing"
```

---

### Task 5: Test end-to-end

- [ ] **Step 1: Run package-mac to test signing + notarization**

Run: `make package-mac`

Expected: cargo-packager output includes signing and notarization steps. Look for:
- `Codesigning` messages during .app packaging
- `Notarizing` messages after .app is built
- `Packaging Vmux_0.1.0_aarch64.dmg` completes without error

- [ ] **Step 2: Run verification**

Run: `bash scripts/verify-signature.sh`

Expected:
```
==> Verifying .app codesign
target/release/Vmux.app: valid on disk
==> Gatekeeper assessment
target/release/Vmux.app: accepted
==> Signature verification passed
```

- [ ] **Step 3: Open and test the app**

Run: `open target/release/Vmux.app`

Expected: App launches without Gatekeeper warnings.

- [ ] **Step 4: If verification fails (CEF hardened runtime issue)**

If `spctl --assess` fails, the CEF framework signing gap is real. In that case:
1. Do NOT proceed with deleting `sign-and-notarize.sh`
2. Revert Tasks 1 and 4
3. Document the failure in the spec as a known limitation
