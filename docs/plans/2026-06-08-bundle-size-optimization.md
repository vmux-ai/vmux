# Bundle Size Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce vmux's installed (.app) and download (.dmg) size, and fix a latent build bug that can leak a 108MB debug wasm into locally-built release DMGs.

**Architecture:** vmux ships as a macOS .app whose size is ~entirely the bundled Chromium Embedded Framework (CEF, 368MB uncompressed). The Rust binaries and the dioxus web payload are small in *released* builds. Wins come from (a) stripping symbols off Rust release binaries (incl. the 66MB CEF render-helper), (b) trimming unused CEF locale packs, (c) stronger DMG compression, and (d) making the web-dist rebuild profile-aware so debug artifacts never ship.

**Tech Stack:** Rust (Cargo workspace, Bevy 0.19-rc.2), CEF 148, dioxus-cli (`dx`) 0.7.4, cargo-packager 0.11.8 (vendored patch), `hdiutil`, macOS codesign/notarytool.

---

## Investigation Findings (read before starting)

Measured on 2026-06-08:

| Component | Uncompressed | Notes |
|-----------|-------------|-------|
| CEF framework total | **368 MB** | dominates the bundle |
| → `Chromium Embedded Framework` binary | 204 MB | Chromium itself; immovable |
| → `bevy_cef_debug_render_process` helper | **66 MB** | shipped CEF render subprocess; **unstripped** |
| → `*.lproj` locale packs (~40 langs) | **~50 MB** | Chromium UI strings; en-only is enough |
| → `libvk_swiftshader.dylib` | 16 MB | software GPU fallback (see "Excluded") |
| → `resources.pak` + `icudtl.dat` | 22 MB | required |
| Shipped web wasm (`vmux_server_bg.wasm`) | **1.2 MB** | release build, already stripped |
| Local **debug** web wasm | **108 MB** | `make dev` artifact, DWARF; **not shipped by CI** |
| Shipped DMG `Vmux_0.0.12_aarch64.dmg` | 244 MB | UDZO/zlib-9 |

Key facts established during investigation:

1. **`dx build --release` already strips + wasm-opts** the web payload (108MB debug → 1.2MB release, verified by building). The 108MB is a debug-profile artifact only.
2. **CI release ships the 1.2MB wasm** — `ci.yml:375` runs `build-mac-release.sh` on a fresh checkout, so `crates/vmux_server/dist/` is absent and is always rebuilt in `release` mode.
3. **Latent local bug:** `needs_dist_rebuild()` in `crates/vmux_server/src/build.rs:169` is profile-blind. The dist wasm carries no profile tag, so a debug-built `dist/` can be reused by a later **local** `make build-local`/`build-release` (sources unchanged, no newer release dx output), shipping the 108MB debug wasm into a locally-built DMG. CI is immune.
4. **No bare `[profile.release]`** exists in the root `Cargo.toml` — only per-package `opt-level="z"` entries. Nothing is stripped; `strip` applies to the whole workspace incl. the excluded patched CEF crates, so a single `strip = true` shrinks the 66MB helper too.
5. **DMG compression is `UDZO`** at `patches/cargo-packager-0.11.8/src/package/dmg/mod.rs:314`. `scripts/create-dmg.sh` is **dead code** (referenced nowhere in the build path).
6. `crates/vmux_server/src/build.rs` is compiled into the lib under `#[cfg(feature = "build")]` (see `lib.rs:1-2`), so its logic is unit-testable via `cargo test -p vmux_server --features build`.

### What can be verified locally vs. not

- **Verifiable here:** Task 1 (unit tests), Task 2 (build helper + measure size, workspace build/test).
- **NOT fully verifiable here:** Task 3 (CEF locale trim) and Task 4 (DMG format) require a full signed **and notarized** release build (Apple credentials + network). Implement them, then verify on a real release run. Each task's verification section flags this.

### Excluded levers (deliberate)

- **SwiftShader removal (`libvk_swiftshader.dylib`, 16MB):** it is Chromium's last-resort software renderer used when the GPU is blocklisted (old GPUs, VMs, some screen-share paths). 16MB is not worth risking blank webviews in the field. Do **not** remove `libGLESv2.dylib`/`libEGL.dylib` either — those are ANGLE (GL-on-Metal) used in normal hardware rendering.
- **`panic = "abort"`:** removes unwind tables but breaks `std::panic::catch_unwind`, which Bevy's task pools and some plugins rely on. Behavior risk outweighs the small size win. Skip.

---

## File Structure

| File | Responsibility | Tasks |
|------|----------------|-------|
| `crates/vmux_server/src/build.rs` | web-dist build + profile-aware rebuild decision + unit tests | 1 |
| `Cargo.toml` (root) | add bare `[profile.release]` strip + thin LTO | 2 |
| `scripts/inject-cef.sh` | trim non-English CEF locale packs after copy, before sign | 3 |
| `patches/cargo-packager-0.11.8/src/package/dmg/mod.rs` | DMG compression format | 4 |
| `patches/cargo-packager-0.11.8/Cargo.toml`, `Makefile`, `.github/workflows/ci.yml` | version bump so the patched packager change is actually picked up | 4 |

---

## Task 1: Profile-aware web-dist rebuild (fix the 108MB debug leak)

**Files:**
- Modify: `crates/vmux_server/src/build.rs` (helpers + `needs_dist_rebuild` ~169-191 + `build()` dist-copy site ~100-108)
- Test: `crates/vmux_server/src/build.rs` (`#[cfg(test)]` module at end of file)

**Why:** A debug-built `dist/` must never be reused for a release bundle. We tag `dist/` with the profile that produced it and force a rebuild on mismatch.

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_server/src/build.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_tag_maps_release_flag() {
        assert_eq!(profile_tag(true), "release");
        assert_eq!(profile_tag(false), "debug");
    }

    #[test]
    fn mismatch_true_when_marker_missing() {
        assert!(dist_profile_mismatch(None, true));
        assert!(dist_profile_mismatch(None, false));
    }

    #[test]
    fn mismatch_true_when_marker_differs() {
        assert!(dist_profile_mismatch(Some("debug"), true));
        assert!(dist_profile_mismatch(Some("release"), false));
    }

    #[test]
    fn mismatch_false_when_marker_matches() {
        assert!(!dist_profile_mismatch(Some("release"), true));
        assert!(!dist_profile_mismatch(Some("debug"), false));
        assert!(!dist_profile_mismatch(Some("release\n"), true));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `env -u CEF_PATH cargo test -p vmux_server --features build profile_tag mismatch -- --nocapture`
Expected: FAIL — `cannot find function profile_tag` / `dist_profile_mismatch`.

- [ ] **Step 3: Add the pure helpers**

Near the top of the file's function section in `crates/vmux_server/src/build.rs` (e.g. just above `dx_web_public_dir`), add:

```rust
pub const DIST_PROFILE_MARKER: &str = ".dx-profile";

pub fn profile_tag(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

pub fn dist_profile_mismatch(existing_marker: Option<&str>, release: bool) -> bool {
    existing_marker.map(str::trim) != Some(profile_tag(release))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `env -u CEF_PATH cargo test -p vmux_server --features build profile_tag mismatch`
Expected: PASS (4 tests).

- [ ] **Step 5: Write the profile marker when dist is (re)built**

In `build()`, immediately after the existing dist-copy block (`crates/vmux_server/src/build.rs` ~104-108):

```rust
        if self.needs_dist_rebuild(release, &workspace_root) {
            run_dx_web_bundle(
                &workspace_root,
                self.dx_package,
                release,
                self.dx_extra_args,
            );
            let public = dx_web_public_dir(&workspace_root, self.dx_bin, release);
            copy_dx_public_to_dist(&public, &dist);
            let _ = fs::write(dist.join(DIST_PROFILE_MARKER), profile_tag(release));
        }
```

- [ ] **Step 6: Consult the marker in `needs_dist_rebuild`**

In `needs_dist_rebuild` (`crates/vmux_server/src/build.rs` ~169), after the existing `let Some(wasm_mtime) = newest_bg_wasm_mtime(&dist) else { return true; };` line, insert:

```rust
        let marker = fs::read_to_string(dist.join(DIST_PROFILE_MARKER)).ok();
        if dist_profile_mismatch(marker.as_deref(), release) {
            return true;
        }
```

- [ ] **Step 7: Run unit tests again**

Run: `env -u CEF_PATH cargo test -p vmux_server --features build`
Expected: PASS (no regressions).

- [ ] **Step 8: Manual integration check (proves the leak is closed)**

Run from the worktree root:

```bash
# simulate a debug-built dist
rm -rf crates/vmux_server/dist && mkdir -p crates/vmux_server/dist/wasm
: > crates/vmux_server/dist/index.html
head -c 1000000 /dev/zero > crates/vmux_server/dist/wasm/vmux_server_bg.wasm
printf debug > crates/vmux_server/dist/.dx-profile
# release build must NOT reuse the debug dist
env -u CEF_PATH cargo build -p vmux_server --release 2>&1 | tail -5
ls -lh crates/vmux_server/dist/wasm/*.wasm
cat crates/vmux_server/dist/.dx-profile
```

Expected: build rebuilds the web dist, the wasm is the real release wasm (~1–2MB, not the 1MB stub), and `.dx-profile` reads `release`.

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_server/src/build.rs
git commit -m "fix(build): make web-dist rebuild profile-aware to stop debug wasm leaking into release bundles"
```

---

## Task 2: Strip + thin-LTO the Rust release profile (shrinks the 66MB CEF helper + app binaries)

**Files:**
- Modify: `Cargo.toml` (root) — add a bare `[profile.release]` table

**Why:** Nothing in release builds is stripped. `strip = true` removes symbols from every workspace binary including the **excluded** `bevy_cef_debug_render_process` (66MB) and the main `Vmux`/`vmux`/`Vmux Service` binaries. `lto = "thin"` gives cross-crate dead-code elimination at modest build-time cost (full `fat` LTO is too slow with CEF/wgpu in the graph). Global `codegen-units` is left at default to avoid ballooning CEF/Bevy build times; the existing per-package `codegen-units = 1` entries stay.

- [ ] **Step 1: Record the baseline helper size**

```bash
env -u CEF_PATH cargo build -p bevy_cef_debug_render_process --release
ls -lh target/release/bevy_cef_debug_render_process | awk '{print "BEFORE:", $5}'
```

Note the number (expected ~60–66MB).

- [ ] **Step 2: Add the release profile table**

In the root `Cargo.toml`, immediately before the first `[profile.release.package.*]` block (the `[profile.release.package.vmux_history]` entry), add:

```toml
[profile.release]
strip = true
lto = "thin"
```

- [ ] **Step 3: Rebuild the helper and measure**

```bash
env -u CEF_PATH cargo build -p bevy_cef_debug_render_process --release
ls -lh target/release/bevy_cef_debug_render_process | awk '{print "AFTER:", $5}'
```

Expected: AFTER is substantially smaller than BEFORE (symbols removed; typically 30–50% reduction). If it is not smaller, STOP — the profile table was not applied; recheck placement.

- [ ] **Step 4: Verify the workspace still builds and tests pass**

Run:
```bash
env -u CEF_PATH cargo test --workspace --exclude bevy_cef --exclude bevy_cef_core --exclude bevy_cef_debug_render_process
```
Expected: PASS. (This is also a final-gate check; it is appropriate to run here because Step 5 commits.)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml
git commit -m "perf(build): strip symbols and enable thin LTO for release binaries"
```

---

## Task 3 (OPTIONAL — biggest CEF win, needs notarized verification): Trim non-English CEF locales

**Files:**
- Modify: `scripts/inject-cef.sh` — add a trim step after `bevy_cef_bundle_app` copies the framework into the .app, **before** the app is signed/notarized.

**Why:** Chromium ships ~40 `*.lproj` locale packs (~50MB) inside the CEF framework Resources. vmux is English-only; non-English Chromium UI strings (error pages, context menus, PDF viewer) can fall back to English. Removing all but `en`/`en_GB` saves ~50MB installed and a proportional amount of the DMG.

**Risk / verification:** The CEF framework is upstream-signed; deleting files invalidates that signature, so the trim **must** run before `sign-and-notarize.sh` re-signs the app deeply with the hardened runtime. This cannot be fully verified in this environment (notarization needs Apple credentials + network). After implementing, you MUST run a real release build and confirm the app launches, webviews render, and notarization succeeds.

- [ ] **Step 1: Add the locale-trim step to `inject-cef.sh`**

In `scripts/inject-cef.sh`, immediately after the `bevy_cef_bundle_app ... --no-sign` invocation and before the "Copy app icon" block, insert:

```bash
# Trim non-English Chromium locale packs to cut bundle size. Must run before
# the app is (re)signed/notarized below, since editing the framework
# invalidates its upstream signature.
CEF_RESOURCES="$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework/Resources"
if [[ -d "$CEF_RESOURCES" ]]; then
    keep_locales=("en.lproj" "en_GB.lproj" "Base.lproj")
    removed=0
    while IFS= read -r -d '' lproj; do
        base="$(basename "$lproj")"
        keep=0
        for k in "${keep_locales[@]}"; do
            [[ "$base" == "$k" ]] && keep=1 && break
        done
        if [[ "$keep" -eq 0 ]]; then
            rm -rf "$lproj"
            removed=$((removed + 1))
        fi
    done < <(find "$CEF_RESOURCES" -maxdepth 1 -name "*.lproj" -print0)
    echo "==> inject-cef: trimmed $removed non-English locale packs"
fi
```

- [ ] **Step 2: Local smoke build (ad-hoc signed, no notarization)**

Run: `make build-local`
Expected: build completes; console shows `inject-cef: trimmed N non-English locale packs` (N ≈ 40).

- [ ] **Step 3: Measure the savings**

```bash
app="$(ls -d "target/release/Vmux ("*").app" 2>/dev/null | head -1)"
du -sh "$app"
du -sh "$app/Contents/Frameworks/Chromium Embedded Framework.framework/Resources"
```
Expected: framework Resources ~50MB smaller than the 75MB baseline.

- [ ] **Step 4: Launch the local build and verify rendering**

```bash
open "$app"
```
Manually confirm: app launches, a browse-mode webview renders a page, terminal works. (Hardened-runtime/notarization is verified later on a real release run.)

- [ ] **Step 5: Commit**

```bash
git add scripts/inject-cef.sh
git commit -m "perf(packaging): trim non-English CEF locale packs (~50MB) before signing"
```

- [ ] **Step 6: Release-build verification gate (run on CI / a notarizing machine)**

Trigger a real release build (the `ci.yml` `build-mac` path or `make build-release` with Apple creds). Confirm: DMG builds, `spctl`/notarytool accepts it, and the installed app launches on a clean machine. Do not consider Task 3 done until this passes.

---

## Task 4 (OPTIONAL — download-size only): Stronger DMG compression

**Files:**
- Modify: `patches/cargo-packager-0.11.8/src/package/dmg/mod.rs:306-322` — change DMG format `UDZO` → `ULFO` (LZFSE).
- Modify: `patches/cargo-packager-0.11.8/Cargo.toml:15`, `Makefile:12`, `.github/workflows/ci.yml:320` and `:358` — bump the vendored packager version so the change is actually rebuilt/installed.

**Why:** The shipped DMG uses `UDZO` (zlib). `ULFO` (LZFSE, macOS 10.11+; our floor is 13.0) gives a better ratio with fast decompression — a smaller download for the same .app. The vendored packager is only reinstalled when its version string changes (`ci.yml:358` greps for `0.11.8`; the cargo-bin cache key at `:320` also pins `cp0.11.8`), so a code-only edit would be ignored — the version must bump.

- [ ] **Step 1: Switch the DMG format to ULFO**

In `patches/cargo-packager-0.11.8/src/package/dmg/mod.rs`, replace the final-create args (lines ~309-319):

```rust
        .args([
            "-fs",
            "HFS+",
            "-volname",
            &config.product_name,
            "-format",
            "ULFO",
            "-ov",
        ])
```

(Drop the `-imagekey zlib-level=9` pair — it only applies to zlib/UDZO.) Update the `tracing::debug!` on line ~305 to say `ULFO`.

- [ ] **Step 2: Bump the vendored packager version**

- `patches/cargo-packager-0.11.8/Cargo.toml:15`: `version = "0.11.8"` → `version = "0.11.9"`.
- `Makefile:12`: `CARGO_PACKAGER_VERSION := 0.11.8` → `0.11.9`.
- `Makefile:142`: `make ensure-package-deps` currently installs from crates.io by version, which has no `0.11.9`. Replace that line so it installs the **vendored** crate:
  - From: `"$(CARGO_BIN)" install cargo-packager --locked --version "$(CARGO_PACKAGER_VERSION)";`
  - To:   `"$(CARGO_BIN)" install --path patches/cargo-packager-0.11.8 --locked --force;`
  (`--force` makes the bump reinstall over an already-present 0.11.8; the surrounding `!=` version guard still short-circuits once 0.11.9 is installed.)
- `.github/workflows/ci.yml:358`: `grep -q '0.11.8'` → `grep -q '0.11.9'`.
- `.github/workflows/ci.yml:320`: cache key `...cp0.11.8...` → `...cp0.11.9...` (busts the stale cached binary).

- [ ] **Step 3: Build the vendored packager locally to confirm it compiles**

```bash
cargo build --manifest-path patches/cargo-packager-0.11.8/Cargo.toml
```
Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add patches/cargo-packager-0.11.8/src/package/dmg/mod.rs patches/cargo-packager-0.11.8/Cargo.toml Makefile .github/workflows/ci.yml
git commit -m "perf(packaging): compress release DMG with LFSE (ULFO) instead of zlib"
```

- [ ] **Step 5: Release-build verification gate (run on CI)**

On a real release build, confirm the DMG mounts on macOS 13+, the app installs, and the asset is smaller than the previous UDZO DMG. Do not consider Task 4 done until this passes.

---

## Final Gate (before PR)

Run the full workspace checks from the worktree root (per AGENTS.md):

```bash
cargo fmt --all -- --check
env -u CEF_PATH cargo clippy --workspace --exclude bevy_cef --exclude bevy_cef_core --exclude bevy_cef_debug_render_process --all-targets -- -D warnings
env -u CEF_PATH cargo test --workspace --exclude bevy_cef --exclude bevy_cef_core --exclude bevy_cef_debug_render_process
```

If Task 4 touched the vendored packager, also: `cargo build --manifest-path patches/cargo-packager-0.11.8/Cargo.toml`.

Fix any failures, re-run, then open the PR.

## Measurement appendix

```bash
# installed .app size + framework breakdown
app="$(ls -d target/release/Vmux*.app 2>/dev/null | head -1)"
du -sh "$app"
du -sh "$app"/Contents/Frameworks/*

# helper binary size
ls -lh target/release/bevy_cef_debug_render_process

# shipped DMG size (from a release build)
ls -lh target/release/Vmux_*_aarch64.dmg
```

## Expected impact summary

| Lever | Installed | Download | Risk | Verifiable here |
|-------|-----------|----------|------|-----------------|
| Task 1 dist leak fix | prevents +107MB regression (local DMGs) | same | low | yes |
| Task 2 strip + thin LTO | ~30–50MB (helper + binaries) | proportional | low | yes |
| Task 3 CEF locale trim | ~50MB | proportional | medium (sign/notarize) | partial |
| Task 4 ULFO DMG | none | several % smaller | low | no (needs CI) |
