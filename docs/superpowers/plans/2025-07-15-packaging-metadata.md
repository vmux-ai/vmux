# Packaging Metadata Unification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify all packaging metadata (Cargo, Info.plist, cargo-packager, Homebrew cask, CI workflow) around a canonical set with vmux.ai homepage and AI-native positioning.

**Architecture:** Six independent file updates sharing one canonical metadata table. No code changes — purely configuration and metadata.

**Tech Stack:** TOML (Cargo), XML (Info.plist), Ruby (Homebrew cask), YAML (GitHub Actions)

**Worktree:** `/Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/jun/vmx-15-release-pipeline`
**Homebrew cask:** `Casks/vmux.rb` (in-repo)

---

### Task 1: Update workspace Cargo.toml

**Files:**
- Modify: `Cargo.toml:8-10`

- [ ] **Step 1: Update description and homepage**

In `Cargo.toml`, change:

```toml
description = "Tiling browser with pane multiplexing"
license = "MIT"
homepage = "https://github.com/vmux-ai/vmux"
```

to:

```toml
description = "AI-native workspace combining browser and terminal panes"
license = "MIT"
homepage = "https://vmux.ai"
```

- [ ] **Step 2: Verify workspace resolves**

Run: `cargo metadata --format-version 1 | head -1`
Expected: JSON output (no errors)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: update workspace description and homepage to vmux.ai"
```

---

### Task 2: Update cargo-packager metadata

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml` (packager section)

- [ ] **Step 1: Add category and copyright to packager config**

In `crates/vmux_desktop/Cargo.toml`, add these fields to `[package.metadata.packager]`:

```toml
category = "Productivity"
copyright = "Copyright 2024-2025 Junichi Sugiura. MIT License."
homepage = "https://vmux.ai"
```

- [ ] **Step 2: Verify TOML is valid**

Run: `cargo metadata --format-version 1 -q 2>&1 | head -1`
Expected: no parse errors

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml
git commit -m "chore: add category, copyright, homepage to packager metadata"
```

---

### Task 3: Update Info.plist

**Files:**
- Modify: `packaging/macos/Info.plist`

- [ ] **Step 1: Update LSMinimumSystemVersion from 11.0 to 13.0**

Change:
```xml
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
```
to:
```xml
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
```

- [ ] **Step 2: Add category and copyright entries**

Before the closing `</dict>`, add:

```xml
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.productivity</string>
	<key>NSHumanReadableCopyright</key>
	<string>Copyright 2024-2025 Junichi Sugiura. MIT License.</string>
```

- [ ] **Step 3: Validate plist syntax**

Run: `plutil -lint packaging/macos/Info.plist`
Expected: `packaging/macos/Info.plist: OK`

- [ ] **Step 4: Commit**

```bash
git add packaging/macos/Info.plist
git commit -m "chore: fix min macOS version, add app category and copyright to Info.plist"
```

---

### Task 4: Create root LICENSE file

**Files:**
- Create: `LICENSE`

- [ ] **Step 1: Create MIT license file**

Create `LICENSE` with contents:

```
MIT License

Copyright (c) 2024-2025 Junichi Sugiura

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: Commit**

```bash
git add LICENSE
git commit -m "chore: add MIT license file"
```

---

### Task 5: Update Homebrew cask

**Files:**
- Modify: `Casks/vmux.rb`

- [ ] **Step 1: Update cask with new metadata**

Replace contents of `Casks/vmux.rb` (in this repo) with:

```ruby
cask "vmux" do
  version "0.1.0"
  sha256 "PLACEHOLDER"

  url "https://github.com/vmux-ai/vmux/releases/download/v#{version}/Vmux-#{version}-mac.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai"

  depends_on macos: ">= :ventura"

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
```

- [ ] **Step 2: Validate cask syntax**

Run: `brew audit --cask Casks/vmux.rb 2>&1 || true`
Note: May warn about PLACEHOLDER sha256 — that's expected until first release.

- [ ] **Step 3: Commit**

```bash
git add Casks/vmux.rb
git commit -m "chore: update cask metadata — description, homepage, macOS dep, zap stanza"
```

---

### Task 6: Update CI release workflow

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Install patched cargo-packager in CI**

After the "Install bevy_cef tools" step, add:

```yaml
      - name: Install cargo-packager (patched for macOS Tahoe)
        run: cargo install --path patches/cargo-packager-0.11.8
```

- [ ] **Step 2: Replace build + DMG steps with cargo-packager**

Remove these steps:
- "Build and bundle app" (`./scripts/bundle-macos.sh`)
- "Create DMG" (`brew install create-dmg` + `./scripts/create-dmg.sh`)

Replace with:

```yaml
      - name: Package app and DMG
        run: env -u CEF_PATH cargo packager --release
```

- [ ] **Step 3: Update sign-and-notarize step to use new output path**

Change the "Sign and notarize" step to:

```yaml
      - name: Sign and notarize
        env:
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_APP_PASSWORD: ${{ secrets.APPLE_APP_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
        run: APP_BUNDLE=target/release/Vmux.app ./scripts/sign-and-notarize.sh
```

- [ ] **Step 4: Update artifact paths for release + tarball**

The DMG filename from cargo-packager is `Vmux_0.1.0_aarch64.dmg` (format: `{product}_{version}_{arch}.dmg`). Update the tarball step:

```yaml
      - name: Create binary tarball
        run: |
          cd target/release/Vmux.app/Contents/MacOS
          tar czf "$GITHUB_WORKSPACE/target/release/vmux-v${VERSION}-aarch64-apple-darwin.tar.gz" Vmux
```

Update the release step:

```yaml
      - name: Create GitHub Release
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          DMG_PATH=$(ls target/release/Vmux_*.dmg)
          gh release create "$GITHUB_REF_NAME" \
            --title "Vmux v${VERSION}" \
            --generate-notes \
            "$DMG_PATH" \
            "target/release/vmux-v${VERSION}-aarch64-apple-darwin.tar.gz"
```

- [ ] **Step 5: Update Homebrew cask update step**

Update the cask generation to use new description, homepage, and DMG path:

```yaml
      - name: Update Homebrew cask
        env:
          GH_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
        run: |
          DMG_PATH=$(ls target/release/Vmux_*.dmg)
          DMG_NAME=$(basename "$DMG_PATH")
          DMG_SHA=$(shasum -a 256 "$DMG_PATH" | awk '{print $1}')

          cat > Casks/vmux.rb << 'CASKEOF'
          cask "vmux" do
            version "VERSION_PLACEHOLDER"
            sha256 "SHA_PLACEHOLDER"

            url "https://github.com/vmux-ai/vmux/releases/download/v#{version}/DMG_PLACEHOLDER"
            name "Vmux"
            desc "AI-native workspace combining browser and terminal panes"
            homepage "https://vmux.ai"

            depends_on macos: ">= :ventura"

            app "Vmux.app"

            zap trash: [
              "~/Library/Application Support/ai.vmux.desktop",
              "~/Library/Caches/ai.vmux.desktop",
              "~/Library/Preferences/ai.vmux.desktop.plist",
            ]
          end
          CASKEOF

          sed -i '' "s/VERSION_PLACEHOLDER/${VERSION}/" Casks/vmux.rb
          sed -i '' "s/SHA_PLACEHOLDER/${DMG_SHA}/" Casks/vmux.rb
          sed -i '' "s/DMG_PLACEHOLDER/${DMG_NAME}/" Casks/vmux.rb

          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Casks/vmux.rb
          git diff --cached --quiet || git commit -m "chore: update Homebrew cask to ${VERSION}"
          git push
```

- [ ] **Step 6: Remove the Patch Info.plist version step**

Remove the "Patch Info.plist version" step — cargo-packager generates Info.plist from metadata, and the `info-plist-path` merge handles overrides. The version comes from `Cargo.toml`.

- [ ] **Step 7: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "chore: update release workflow for cargo-packager and new metadata"
```
