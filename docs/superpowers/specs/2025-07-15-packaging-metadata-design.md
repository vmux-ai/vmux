# Packaging Metadata Unification

Unify all packaging surfaces (Cargo, Info.plist, cargo-packager, Homebrew cask, CI) around a single canonical metadata set.

## Canonical Metadata

| Field | Value |
|---|---|
| Product name | Vmux |
| Short description | AI-native workspace combining browser and terminal panes |
| Identifier | ai.vmux.desktop |
| Category | public.app-category.productivity |
| Min macOS | 13.0 (Ventura) |
| Copyright | Copyright 2024-2025 Junichi Sugiura. MIT License. |
| License | MIT |
| Homepage | https://vmux.ai |
| Version | 0.1.0 (from workspace) |

## Changes

### 1. Workspace Cargo.toml

Update `[workspace.package].description` to the canonical short description.

### 2. crates/vmux_desktop/Cargo.toml

Add to `[package.metadata.packager]`:
- `category = "Productivity"`
- `copyright = "Copyright 2024-2025 Junichi Sugiura. MIT License."`

### 3. packaging/macos/Info.plist

- `LSMinimumSystemVersion` → `13.0` (was `11.0`)
- Add `LSApplicationCategoryType` → `public.app-category.productivity`
- Add `NSHumanReadableCopyright` → `Copyright 2024-2025 Junichi Sugiura. MIT License.`

### 4. Root LICENSE file

Create standard MIT license file with copyright holder "Junichi Sugiura".

### 5. Homebrew cask (in-repo)

Update `Casks/vmux.rb` in this repo:

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

### 6. CI workflow (.github/workflows/release.yml)

Replace shell script invocations with cargo-packager:
- Build step: `cargo packager --release`
- Sign step: `APP_BUNDLE=target/release/Vmux.app ./scripts/sign-and-notarize.sh`
- DMG artifact: `target/release/Vmux_0.1.0_aarch64.dmg` (cargo-packager naming)
- Homebrew update: update `Casks/vmux.rb` in-repo with sha256 of DMG

## Out of scope

- App Store submission
- Universal binary (x86_64 + aarch64)
- Sparkle auto-update framework
- Custom DMG background image (using solid color)
