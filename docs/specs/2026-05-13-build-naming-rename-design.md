# Build identity rename — dev/local/release suffix scheme + per-SHA local

**Status:** approved (design)
**Date:** 2026-05-13
**Linear:** [VMX-117](https://linear.app/vmux/issue/VMX-117)

## Goal

Rename the macOS desktop bundle identifier, app filename, service launchd label, and Makefile targets so that:

1. **Dev** builds get a stable `.dev` identity, distinct from local. (Triggered by `make dev`; `VMUX_BUILD_PROFILE=dev`.)
2. **Local** builds get a per-commit identity (`<sha>` = git short SHA from `VMUX_GIT_HASH`) so multiple local builds coexist as separate apps in `~/Applications` / Spotlight / Launchpad.
3. **Release** builds drop the `.release` suffix from the service launchd label, matching the desktop's already-suffix-free release ID.
4. **Makefile targets** standardize on `build-<profile>` for explicit build and `<profile>` for "build + run" (e.g. `dev`, `local`, `release`). `make` defaults to `dev`. Mac is the implicit default OS — when Windows support lands, Windows-specific targets get a `-win` suffix (`dev-win`, `build-local-win`).

The driving use case is local builds: a contributor wants to keep the `vmx-127` build of Vmux installed alongside the `vmx-130` build without one clobbering the other.

## What's already in place

These pieces of the puzzle are already wired up on `main`:

- `crates/vmux_desktop/build.rs` populates `VMUX_GIT_HASH` (7-char short SHA, falls back to `"unknown"`) and `VMUX_BUILD_PROFILE` (`dev` / `local` / `release`).
- `crates/vmux_layout/build.rs` and `crates/vmux_command/build.rs` also populate `VMUX_GIT_HASH`.
- Window title (`crates/vmux_desktop/src/lib.rs:76-81`), CLI version string (`crates/vmux_desktop/src/main.rs:29-34`), and macOS menu app-name (`crates/vmux_macro/src/lib.rs:203-209`) already format as `Vmux (<sha>)` for local and `Vmux Dev (<sha>)` for dev. **Don't touch these.**

This spec only changes the bundle ID, app bundle filename, and launchd label.

## Final naming matrix

### Desktop

| `VMUX_BUILD_PROFILE` | bundle ID | CFBundleName / DisplayName | path |
|---|---|---|---|
| `dev` | `ai.vmux.desktop.dev` | n/a (raw binary, no `.app`) | `target/debug/vmux_desktop` |
| `local` | `ai.vmux.desktop.<sha>` | `Vmux (<sha>)` | `target/release/Vmux (<sha>).app` |
| `release` | `ai.vmux.desktop` | `Vmux` | `target/release/Vmux.app` |

### Service (launchd label)

| `VMUX_BUILD_PROFILE` | label |
|---|---|
| `dev` | `ai.vmux.service.dev` |
| `local` | `ai.vmux.service.<sha>` |
| `release` | `ai.vmux.service` *(no suffix)* |

### Service local files

`~/Library/Application Support/Vmux/services/vmux-<profile>.{sock,pid,identity,log}` keeps the `VMUX_BUILD_PROFILE` literal as the suffix on all profiles (so files become `vmux-dev.sock`, `vmux-local.sock`, `vmux-release.sock`). Per-SHA local files would create file-system clutter for no benefit; the launchd-label SHA is what matters for letting separate `.app` bundles register distinct background services.

## Hash rules

- **Source:** `VMUX_GIT_HASH` (already populated by `build.rs` files via `git rev-parse --short HEAD`). The packaging script computes the same value via `git rev-parse --short HEAD` for `.app` filename + bundle ID, since the script runs before `cargo` and can't read `cargo`-emitted env vars.
- **Same SHA → overwrite.** Rebuilding on the same commit replaces the existing `Vmux (<sha>).app`. No dirty marker, no rebuild counter.
- **Install location:** unchanged. Build artifacts stay in `target/release/`. Users who want Spotlight/Launchpad/Dock visibility manually drag the `.app` into `~/Applications`.

## Tradeoffs accepted

- **Chromium keychain ACL split (dev ↔ local).** Today, debug + local share `ai.vmux.desktop.local`, so Chromium's safe-storage ACL covers both flows from one keychain prompt (per commit `4c5a4bd`). Splitting dev to `.dev` re-introduces the prompt on the first switch. Accepted because `make run-mac` (dev) and `make build-mac-local` (local) serve distinct workflows and shouldn't share login state.
- **Chromium keychain ACL per local build.** Each new SHA → new bundle ID → new Chromium safe-storage entry → first-run keychain prompt. Logins / saved passwords don't carry across local rebuilds on different commits. Accepted as the cost of side-by-side installs.
- **No build cleanup.** Stale `Vmux (<old-sha>).app` directories accumulate in `target/release/` until the user runs `cargo clean` or removes them by hand. Out of scope.

## Implementation surface

### `scripts/package.sh`

Replace the static `local`-profile values with SHA-derived ones. Keep `VMUX_BUILD_PROFILE=local` so the in-binary code that already keys on the literal `"local"` keeps working.

```bash
case "$PROFILE" in
    release)
        PRODUCT_NAME="Vmux"
        BUNDLE_ID="ai.vmux.desktop"
        ;;
    local)
        SHA="$(git -C "$ROOT" rev-parse --short HEAD)"
        PRODUCT_NAME="Vmux ($SHA)"
        BUNDLE_ID="ai.vmux.desktop.$SHA"
        # VMUX_BUILD_PROFILE stays "local" — VMUX_GIT_HASH carries the SHA into the binary.
        ;;
    *)
        echo "Unknown profile: $PROFILE (expected: release, local)" >&2
        exit 1
        ;;
esac
```

Existing `sed` patches (Cargo.toml `product-name` / `identifier` + Info.plist `CFBundleName` / `CFBundleDisplayName` / `CFBundleIdentifier`) work unchanged — they consume `$BUNDLE_ID` and `$PRODUCT_NAME`.

`export VMUX_BUILD_PROFILE="$PROFILE"` (already at line 68) is unchanged.

### `crates/vmux_service/build.rs`

Add `VMUX_GIT_HASH` population so `paths.rs` can read it via `env!()`:

```rust
let hash = Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    .unwrap_or_else(|| "unknown".to_string());
println!("cargo::rustc-env=VMUX_GIT_HASH={hash}");
println!("cargo::rerun-if-changed=../../.git/HEAD");
println!("cargo::rerun-if-changed=../../.git/refs");
```

Mirror what `crates/vmux_desktop/build.rs:5-13` does.

### `crates/vmux_service/src/paths.rs`

Match-on-profile in the launchd-label formula:

```rust
pub fn launchd_label(profile: &str) -> String {
    match profile {
        "release" => "ai.vmux.service".to_string(),
        "local" => format!("ai.vmux.service.{}", env!("VMUX_GIT_HASH")),
        _ => format!("ai.vmux.service.{profile}"),
    }
}
```

Local-file naming (`vmux-<profile>.sock`, `.pid`, `.identity`, `.log`) keeps the `profile` string literally. The launchd-label suffix change is launchd-only.

Note: `env!("VMUX_GIT_HASH")` is a compile-time constant baked into the `vmux_service` binary at build time. Passing `profile = "local"` at any later runtime call site will always resolve to the SHA the binary was built with, not the calling process's current SHA. That's the intended behavior: each `Vmux (<sha>).app/Contents/MacOS/vmux_service` ships with its own immutable launchd label.

### `scripts/sign-debug-mac.sh` → `scripts/sign-dev-mac.sh`

`git mv scripts/sign-debug-mac.sh scripts/sign-dev-mac.sh`. Then change line 9: `APP_IDENTIFIER="ai.vmux.desktop.local"` → `"ai.vmux.desktop.dev"`.

### `Makefile`

**Target naming convention.** Build targets keep the `build-` verb prefix; run targets drop the verb (running is the default action). Convention: `build-<profile>` for explicit build, `<profile>` for "build + run". Profiles: `dev`, `local`, `release`. Mac is the implicit OS — Windows targets (when added) get a `-win` suffix; mac targets stay unsuffixed. Default `make` (no args) aliases to `make dev`.

Rename / removal map:

| old | new |
|---|---|
| `run-mac` | `dev` |
| `build-mac-debug` | **removed** (inlined into `dev`) |
| `run-mac-local` | `local` |
| `build-mac-local` | `build-local` |
| `build-mac-release` | `build-release` |
| — *(new)* | `release` (build + open `Vmux.app`) |
| `package-local-mac` | **removed** (inlined into `build-local`) |
| `package-release-mac` | **removed** (inlined into `build-release` via `build-mac-release.sh`) |
| `sign-mac-debug` | **removed** (inlined into `dev`) |

Default target:

```makefile
.DEFAULT_GOAL := dev
```

`build-local` and `local`:

```makefile
local: build-local
	@sha="$$(git rev-parse --short HEAD)" && \
	open "target/release/Vmux ($$sha).app"

build-local: ensure-run-mac-deps ensure-package-deps ensure-codesign-deps
	./scripts/package.sh local
	@identity="$$(./scripts/ensure-local-codesign-identity.sh)" && \
	sha="$$(git rev-parse --short HEAD)" && \
	APPLE_SIGNING_IDENTITY="$$identity" \
	SKIP_NOTARIZE=1 \
	APP_BUNDLE="target/release/Vmux ($$sha).app" \
	./scripts/sign-and-notarize.sh

dev: ensure-run-mac-deps ensure-codesign-deps install-debug-render-process
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop -p vmux_cli -p vmux_service --features debug
	@identity="$$(./scripts/ensure-local-codesign-identity.sh)" && \
	APPLE_SIGNING_IDENTITY="$$identity" \
	APP_BINARY="target/debug/vmux_desktop" \
	HELPER_BINARY="$(CEF_DEBUG_RENDER)" \
	./scripts/sign-dev-mac.sh
	exec env -u CEF_PATH ./target/debug/vmux_desktop
```

**Asymmetry note:** `dev` only has a run target (no separate `build-dev`) because the dev workflow is iterate-and-launch — there's no use case for "build but don't run". Local and release keep a `build-*` for producing artifacts without launching.

`build-release` keeps its current shape — it already invokes `scripts/build-mac-release.sh` which calls `package.sh` directly (never went through the Makefile `package-release-mac` target):

```makefile
build-release: ensure-run-mac-deps ensure-package-deps
	./scripts/build-mac-release.sh release

release: build-release
	open "target/release/Vmux.app"
```

`release` runs the full release-build pipeline (sign + notarize via `build-mac-release.sh`) then launches the resulting `Vmux.app`. Without `APPLE_CERTIFICATE` env vars set, `build-mac-release.sh` falls back to the user's login keychain — so a local dev with a valid cert can `make release` without CI plumbing.

Update the `.PHONY` line at the top of `Makefile` to reflect new names + dropped targets. Move `ensure-run-mac-deps` + `ensure-package-deps` from the removed `package-*-mac` targets onto the corresponding `build-*` targets (shown above).

**Script rename for consistency:** `scripts/sign-debug-mac.sh` → `scripts/sign-dev-mac.sh` (no behavior change). The hard-coded `APP_IDENTIFIER` inside the script also flips from `ai.vmux.desktop.local` to `ai.vmux.desktop.dev` per the earlier section. The `scripts/build-mac-release.sh` filename is left as-is — it's an internal script, not user-facing, and renaming it would touch more files for no benefit. Same for `scripts/macos.sh`, `scripts/doctor-mac.sh`.

**Documentation/script call sites referencing old names** (must update):

- `scripts/macos.sh:38` — `make -C "$ROOT" build-mac-local` → `make -C "$ROOT" build-local`
- `README.md` — any `make run-mac` / `make build-mac-*` mentions
- `scripts/doctor-mac.sh` — if it suggests targets to the user
- Any `docs/` content that documents the build flow

### Tests

- `crates/vmux_service/src/paths.rs:138-139` — `launchd_label("release")` assertion changes from `"ai.vmux.service.release"` to `"ai.vmux.service"`. `launchd_label("dev")` unchanged. Add `launchd_label("local")` assertion that matches `^ai\.vmux\.service\.[0-9a-f]{7}$` (or accepts the literal `unknown` fallback when git is unavailable).
- `crates/vmux_service/src/launchd.rs:141` — same release-label adjustment.
- `crates/vmux_desktop/tests/release_invariants.rs:72-74` — dev assertions become `ai.vmux.desktop.dev`. Local assertion replaced with regex `^ai\.vmux\.desktop\.[0-9a-f]{7}$` for `BUNDLE_ID`/`APP_IDENTIFIER`.

### Files to grep during implementation, not pre-changed here

- `crates/vmux_desktop/src/profile.rs` — references to literal `"Vmux Local"` strings.
- `scripts/inject-cef.sh` — already env-var-driven (`VMUX_BUNDLE_ID`); should keep working but verify.
- `crates/vmux_desktop/Cargo.toml:53-54` — sed-patched at package time, no source change.
- `packaging/macos/Info.plist` — sed-patched at package time, no source change.
- README and docs — references to `Vmux Local.app` need updating.

## Out of scope

- Auto-install into `~/Applications/` or `/Applications/`.
- Dirty-tree marker (`-dirty` suffix).
- Same-SHA collision detection / refusal.
- Cleanup of stale per-SHA builds.
- Symmetrical release rename (`ai.vmux.desktop` → `ai.vmux`). Not requested.
- Linux-side packaging.

## Verification plan

1. `make` (no args) launches the dev binary (alias for `make dev`).
2. `make dev` builds + signs + runs the debug binary; `codesign -dv target/debug/vmux_desktop` after first run → expect identifier `ai.vmux.desktop.dev`.
3. `make build-local` on a clean tree → expect `target/release/Vmux (<sha>).app` exists, `Info.plist` has `CFBundleIdentifier=ai.vmux.desktop.<sha>` and `CFBundleName=Vmux (<sha>)`.
4. `make build-local`, switch to a different commit, repeat → both `.app` directories coexist in `target/release/`.
5. `make build-release` → unchanged: `Vmux.app` with `ai.vmux.desktop`. `make release` builds + launches it.
6. Launch each build, run `launchctl list | grep ai.vmux.service` → dev shows `.dev`, local shows `.<sha>`, release shows bare `ai.vmux.service`.
7. Per-crate `cargo fmt` / `cargo clippy` / `cargo test` on `vmux_service` and `vmux_desktop`.
