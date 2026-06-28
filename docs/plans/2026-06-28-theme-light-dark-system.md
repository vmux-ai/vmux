# Light / Dark / System theme for all `vmux://` pages — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Note (project memory): do NOT subagent-drive this plan — CEF builds are huge and long agents drop sockets. Implement directly with a warm target dir. Defer runtime testing to ONE pass at the end (finish-then-test).

**Goal:** Make all 15 `vmux://` pages — plus editor syntax highlighting and the terminal palette — render correctly in Light, Dark, and System (Device) mode, reacting live to the appearance setting and the OS appearance.

**Architecture:** Hybrid. (1) Chrome (all CSS pages) switches from a hardcoded `.dark` class to `prefers-color-scheme`, which CEF already drives from `AppSettings.appearance.mode` — zero new wire. (2) Editor + terminal generate colors host-side, so a new `ResolvedColorScheme` resource (setting + OS theme) drives their light/dark theme choice and re-push.

**Tech Stack:** Tailwind v4 (CSS-first), Dioxus/WASM pages, Bevy host, CEF (bevy_cef patch), syntect (editor), winit `WindowThemeChanged` (bevy_window patch), objc2/AppKit (macOS OS appearance read).

**Spec:** `docs/specs/2026-06-28-theme-light-dark-system-design.md`

---

## Reference: token mapping (used throughout Phase 2)

The dark theme leans on opacity-modified `white` utilities for its inline glass. The
key trick: **`white` → `foreground`** in any opacity-modified utility. `--foreground`
is near-white in dark and near-black in light, so the tint flips automatically.

| Dark-only literal | Replace with | Why |
|---|---|---|
| `bg-white/[0.04]`, `bg-white/5`, `bg-white/10` | `bg-foreground/[0.04]` … | tint flips with mode |
| `ring-white/10`, `ring-white/[0.06]` | `ring-foreground/10` … | same |
| `border-white/10` | `border-foreground/10` (or `border-glass-border`) | same |
| `text-white` | `text-foreground` | semantic |
| `text-white/60`, `text-zinc-400`, `text-neutral-400` | `text-muted-foreground` | semantic |
| `bg-zinc-900`, `bg-neutral-900`, `bg-black` | `bg-background` | semantic |
| `bg-zinc-800`, `bg-neutral-800` | `bg-card` or `bg-muted` | semantic |
| translucent pane (`bg-white/[x] … backdrop-blur … ring-white/y`) | `glass` utility / `bg-glass` + `border-glass-border` | tokenized |
| `shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]` | `shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]` | heavy black shadow is wrong on light |
| accent literals (`bg-cyan-400`, `text-emerald-400`, `bg-rose-500`, `bg-amber-400`) | keep hue; if on glass as text, add `dark:` pair (e.g. `text-emerald-600 dark:text-emerald-400`) | `-400` is low-contrast on light bg |

Find sites per file with:
`rg -n "white/|bg-zinc|bg-neutral|bg-black|text-white|text-zinc|text-neutral|ring-white|border-white|rgba\(0,0,0" <file>`

---

## Phase 1 — CSS mechanism + light soft-glass tokens

### Task 1: `theme.css` — media-driven dark, light glass tokens, consolidated body override

**Files:**
- Modify: `crates/vmux_ui/assets/theme.css`

- [ ] **Step 1: Switch the dark variant to media-driven.**

Replace line 1:
```css
@custom-variant dark (&:is(.dark *));
```
with:
```css
@custom-variant dark (@media (prefers-color-scheme: dark));
```

- [ ] **Step 2: Add light soft-glass tokens to `:root`.**

In the `:root { … }` block (after the existing `--scrim-strong` line ~37), add:
```css
  --glass: oklch(1 0 0 / 0.55);
  --glass-hover: oklch(1 0 0 / 0.7);
  --glass-active: oklch(1 0 0 / 0.85);
  --glass-border: oklch(0 0 0 / 0.08);
  --cef-surface: oklch(0.98 0 0 / 0.86);
  --cef-surface-hover: oklch(0.94 0 0 / 0.78);
  --cef-surface-active: oklch(0.9 0 0 / 0.82);
  --cef-surface-border: oklch(0 0 0 / 0.1);
```

- [ ] **Step 3: Convert the `.dark { … }` block to a media query.**

Change the selector on line 40 from `.dark {` to:
```css
@media (prefers-color-scheme: dark) {
  :root {
```
and add a matching closing `}` so the whole token block is wrapped in the `@media`. (The dark glass/cef-surface tokens already inside it stay as-is.)

- [ ] **Step 4: Add the consolidated body override (moved out of per-crate CSS).**

After the `.glass { … }` utility (~line 256), add the body-level overrides that today live duplicated in each crate `index.css`. Light is the default; dark is media-gated:
```css
body {
  --glass: oklch(1 0 0 / 0.55);
  --glass-hover: oklch(0 0 0 / 0.04);
  --glass-active: oklch(0 0 0 / 0.07);
  --glass-border: oklch(0 0 0 / 0.1);
}

@media (prefers-color-scheme: dark) {
  body {
    --foreground: oklch(1 0 0);
    --muted-foreground: oklch(0.82 0 0);
    --sidebar-primary-foreground: oklch(1 0 0);
    --glass: oklch(0.18 0 0 / 0.82);
    --glass-hover: oklch(1 0 0 / 0.08);
    --glass-active: oklch(1 0 0 / 0.14);
    --glass-border: oklch(1 0 0 / 0.2);
  }
}
```

- [ ] **Step 5: Build CSS to verify it compiles (deferred to Phase 5 full build).** No per-task build (CEF cost). Move on.

- [ ] **Step 6: Commit.**
```bash
git add crates/vmux_ui/assets/theme.css
git commit -m "feat(theme): media-driven dark + light soft-glass tokens"
```

### Task 2: Remove duplicated `html.dark body` overrides from per-crate `index.css`

**Files (each has the `@layer base` block with `html.dark` + `html.dark body`):**
- Modify: `crates/vmux_server/assets/index.css`
- Modify: `crates/vmux_layout/assets/index.css`
- Modify: `crates/vmux_command/assets/index.css`
- Modify: `crates/vmux_setting/assets/index.css`
- Modify: `crates/vmux_service/assets/index.css`
- Modify: `crates/vmux_space/assets/index.css`
- Modify: `crates/vmux_terminal/assets/index.css`
- Modify: `crates/vmux_history/assets/index.css`

First confirm the exact set: `rg -l "html.dark body" crates/*/assets/index.css`

- [ ] **Step 1: In each file, delete the `html.dark body { … }` block** (the glass override now lives in `theme.css`, Task 1 Step 4). Example for `vmux_server/assets/index.css` — remove lines 102-110:
```css
  html.dark body {
    --foreground: oklch(1 0 0);
    --muted-foreground: oklch(0.82 0 0);
    --sidebar-primary-foreground: oklch(1 0 0);
    --glass: oklch(0.18 0 0 / 0.82);
    --glass-hover: oklch(1 0 0 / 0.08);
    --glass-active: oklch(1 0 0 / 0.14);
    --glass-border: oklch(1 0 0 / 0.20);
  }
```

- [ ] **Step 2: Replace the `html.dark { height; color-scheme: dark }` rule** with a mode-agnostic version. In `vmux_server/assets/index.css` lines 71-74:
```css
  html.dark {
    height: 100%;
    color-scheme: dark;
  }
```
becomes:
```css
  html {
    height: 100%;
    color-scheme: light dark;
  }
```
Apply the equivalent change in each per-crate file (some only have `html.dark { color-scheme: dark }` inline — make it `html { color-scheme: light dark }`).

- [ ] **Step 3: Commit.**
```bash
git add crates/*/assets/index.css
git commit -m "refactor(theme): consolidate body glass override into theme.css"
```

### Task 3: HTML shells — drop static `.dark`, set `color-scheme: light dark`

**Files:**
- Modify: `crates/vmux_server/assets/index.html`
- Check/modify: `crates/vmux_ui/assets/index.html` (standalone gallery shell, if present)

- [ ] **Step 1: `vmux_server/assets/index.html` line 2:**
```html
<html lang="en" class="dark h-full">
```
→
```html
<html lang="en" class="h-full" style="color-scheme: light dark">
```

- [ ] **Step 2: Update the inline `<style>` (lines 11):** replace `html.dark, html.dark body {` with `html, body {`:
```html
    html, body { height: 100%; margin: 0; min-height: 0; }
```

- [ ] **Step 3:** `rg -n "class=\"dark|html.dark" crates/*/assets/index.html` and apply the same two changes anywhere else it appears.

- [ ] **Step 4: Commit.**
```bash
git add crates/*/assets/index.html
git commit -m "feat(theme): drop static dark class from page shell"
```

### Task 4: Update CSS-text regression tests

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/style.rs:334-348`
- Verify: `crates/vmux_layout/tests/page_source.rs` (no change expected)

- [ ] **Step 1: Update the two glass assertions.** The dark glass value now lives in `theme.css` under `@media (prefers-color-scheme: dark) { body { … } }`, not the per-crate `index.css`. Replace both tests (`layout_css_gives_controls_readable_glass_background` and `bundled_layout_css_gives_controls_readable_glass_background`) so they read `theme.css`:
```rust
    #[test]
    fn theme_css_gives_controls_readable_glass_background() {
        let css = include_str!("../../../vmux_ui/assets/theme.css");
        assert!(css.contains("--glass: oklch(0.18 0 0 / 0.82);"));
        assert!(css.contains("prefers-color-scheme: dark"));
        assert!(css.contains("--glass: oklch(1 0 0 / 0.55);"));
    }
```
(Delete the now-stale `include_str!("../../assets/index.css")` / `vmux_server/assets/index.css` glass asserts; those files no longer carry `--glass`.)

- [ ] **Step 2: Run the layout tests.**
Run: `cargo test -p vmux_layout --lib style:: 2>&1 | tail -20`
Expected: PASS (these are native, fast — no CEF).

- [ ] **Step 3: Run page_source tests** (they assert `bg-glass` usage + the theme.css import, which are unchanged):
Run: `cargo test -p vmux_layout --test page_source 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 4: Commit.**
```bash
git add crates/vmux_layout/src/command_bar/style.rs
git commit -m "test(theme): assert glass tokens in theme.css media blocks"
```

---

## Phase 2 — De-hardcode page colors

Apply the **token mapping reference** (top of doc) per file. After each file, run that crate's native page_source/style tests if any (`cargo test -p <crate> --lib`). Defer the full WASM/CEF build to Phase 5.

### Task 5: Shared component library audit — `crates/vmux_ui/src/components/`

**Files:** `crates/vmux_ui/src/components/*.rs`

- [ ] **Step 1: Audit for dark-only literals** (the lib already uses `dark:` pairs in 53 spots, which now work automatically):
Run: `rg -n "white/|bg-zinc|bg-neutral|bg-black|text-white|ring-white|border-white|rgba\(0,0,0" crates/vmux_ui/src/components/`
- [ ] **Step 2:** For each hit, apply the mapping table. Most should already be `dark:`-guarded; fix any bare dark-only literal.
- [ ] **Step 3: Commit.** `git add crates/vmux_ui/src/components && git commit -m "feat(theme): light-mode fixes in shared components"`

### Task 6: Agent accent palette — `crates/vmux_ui/src/agent_accent.rs`

**Files:** `crates/vmux_ui/src/agent_accent.rs` (~33 literals)

- [ ] **Step 1:** Keep each agent's brand hue. For accents used as **text/icon on glass**, add `dark:` pairs so light uses a darker shade: e.g. `text-rose-400` → `text-rose-600 dark:text-rose-400`, `text-emerald-400` → `text-emerald-600 dark:text-emerald-400`.
- [ ] **Step 2:** For **glow blobs** (`bg-rose-500/20 blur-[120px]`), keep as-is (decorative, reads on both) but reduce light intensity if needed: `bg-rose-500/20 dark:bg-rose-500/20` → consider `/10` in light. Verify in Phase 5.
- [ ] **Step 3:** Gradient pills (`from-orange-400 to-rose-500`) are brand — leave unless illegible.
- [ ] **Step 4: Commit.** `git add crates/vmux_ui/src/agent_accent.rs && git commit -m "feat(theme): agent accents legible in light mode"`

### Task 7: Editor pages — `crates/vmux_editor/src/page.rs`, `lsp_page.rs`

**Files:** `crates/vmux_editor/src/page.rs` (~19), `crates/vmux_editor/src/lsp_page.rs` (~11)

- [ ] **Step 1:** Note `PANE_CLASS` (page.rs ~line 111) — the canonical inline glass. Replace its `bg-white/[0.04] … ring-white/… shadow-[…rgba(0,0,0,…)]` with the `glass` utility + `dark:` shadow:
```rust
const PANE_CLASS: &str = "glass rounded-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]";
```
(keep existing layout/radius classes; swap only color/tint/ring/shadow).
- [ ] **Step 2:** Apply the mapping table to remaining hits via the rg command. `ring-cyan-400/10` (focus ring) → keep hue, it reads on both.
- [ ] **Step 3: Commit.** `git add crates/vmux_editor/src/page.rs crates/vmux_editor/src/lsp_page.rs && git commit -m "feat(theme): editor + lsp pages adapt to light mode"`

### Task 8: Layout pages — command bar, extensions, debug, error, start

**Files:** `crates/vmux_layout/src/command_bar/page.rs`, `extensions_page.rs`, `debug_page.rs`, `error_page.rs`, `start/page.rs`

- [ ] **Step 1:** Apply mapping per file (rg command). `start` reuses command-bar styling — fix command_bar first, then start inherits.
- [ ] **Step 2:** After editing `command_bar/page.rs`, re-run its source-scrape tests (they assert content/structure, may catch class-string changes):
Run: `cargo test -p vmux_layout --lib command_bar 2>&1 | tail -20`
Expected: PASS — if a test asserts a now-changed class string, update the assertion to the new token.
- [ ] **Step 3: Commit.** `git add crates/vmux_layout/src && git commit -m "feat(theme): layout pages adapt to light mode"`

### Task 9: Service / team / space / history pages

**Files:** `crates/vmux_service/src/page.rs`, `crates/vmux_team/src/page.rs`, `crates/vmux_space/src/page.rs`, `crates/vmux_history/src/page.rs`

- [ ] **Step 1:** Apply mapping per file (rg command each).
- [ ] **Step 2: Commit.** `git add crates/vmux_service crates/vmux_team crates/vmux_space crates/vmux_history && git commit -m "feat(theme): service/team/space/history pages adapt to light mode"`

### Task 10: Settings + agent setup pages

**Files:** `crates/vmux_setting/src/page.rs`, `crates/vmux_agent/src/vibe/setup/page.rs`

- [ ] **Step 1:** Apply mapping per file. `vmux_agent` is **not** in `vmux_server/assets/index.css` `@source` list — confirm it is (it was reported present at line 4); if a new file is added ensure `@source "../../vmux_agent/src"` exists (it does).
- [ ] **Step 2:** `vmux_team` is missing from the `@source` list (latent gap). Add `@source "../../vmux_team/src";` to `crates/vmux_server/assets/index.css` so team page utilities compile.
- [ ] **Step 3: Commit.** `git add crates/vmux_setting crates/vmux_agent crates/vmux_server/assets/index.css && git commit -m "feat(theme): settings/agent pages adapt to light mode; add vmux_team @source"`

---

## Phase 3 — Host appearance resolution

### Task 11: `ResolvedColorScheme` resource + `ColorSchemeChanged` message

**Files:**
- Create: `crates/vmux_setting/src/appearance.rs`
- Modify: `crates/vmux_setting/src/lib.rs` (module decl + re-exports)
- Modify: `crates/vmux_setting/src/plugin.rs` (register)

- [ ] **Step 1: Write the failing unit test for the resolver.** Create `crates/vmux_setting/src/appearance.rs`:
```rust
use crate::ColorScheme;
use bevy::prelude::*;

/// Concrete light/dark choice after resolving `ColorScheme` against the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedScheme {
    Light,
    Dark,
}

/// The OS appearance as last observed by the host. `None` = not yet known.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SystemAppearance(pub Option<ResolvedScheme>);

/// The resolved app scheme driving host-generated colors (editor, terminal).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedColorScheme(pub ResolvedScheme);

impl Default for ResolvedColorScheme {
    fn default() -> Self {
        Self(ResolvedScheme::Dark)
    }
}

/// Sent whenever the resolved scheme changes.
#[derive(Message, Clone, Copy, Debug)]
pub struct ColorSchemeChanged(pub ResolvedScheme);

/// Pure resolution: explicit Light/Dark win; Device follows the OS, defaulting
/// to Dark when the OS appearance is unknown.
pub fn resolve(mode: ColorScheme, system: Option<ResolvedScheme>) -> ResolvedScheme {
    match mode {
        ColorScheme::Light => ResolvedScheme::Light,
        ColorScheme::Dark => ResolvedScheme::Dark,
        ColorScheme::Device => system.unwrap_or(ResolvedScheme::Dark),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_modes_ignore_os() {
        assert_eq!(resolve(ColorScheme::Light, Some(ResolvedScheme::Dark)), ResolvedScheme::Light);
        assert_eq!(resolve(ColorScheme::Dark, Some(ResolvedScheme::Light)), ResolvedScheme::Dark);
    }

    #[test]
    fn device_follows_os_and_defaults_dark() {
        assert_eq!(resolve(ColorScheme::Device, Some(ResolvedScheme::Light)), ResolvedScheme::Light);
        assert_eq!(resolve(ColorScheme::Device, Some(ResolvedScheme::Dark)), ResolvedScheme::Dark);
        assert_eq!(resolve(ColorScheme::Device, None), ResolvedScheme::Dark);
    }
}
```

- [ ] **Step 2: Run the test (will fail to compile until module is wired).**
Run: `cargo test -p vmux_setting appearance:: 2>&1 | tail -20`
Expected: FAIL (unresolved module).

- [ ] **Step 3: Wire the module + re-exports** in `crates/vmux_setting/src/lib.rs`:
```rust
mod appearance;
pub use appearance::{
    ColorSchemeChanged, ResolvedColorScheme, ResolvedScheme, SystemAppearance,
};
```

- [ ] **Step 4: Add the systems** to `appearance.rs`:
```rust
/// Track winit OS theme changes into `SystemAppearance`.
pub fn track_window_theme(
    mut reader: MessageReader<bevy::window::WindowThemeChanged>,
    mut system: ResMut<SystemAppearance>,
) {
    for ev in reader.read() {
        let scheme = match ev.theme {
            bevy::window::WindowTheme::Light => ResolvedScheme::Light,
            bevy::window::WindowTheme::Dark => ResolvedScheme::Dark,
        };
        if system.0 != Some(scheme) {
            system.0 = Some(scheme);
        }
    }
}

/// Recompute `ResolvedColorScheme` from the setting + OS; emit on change.
pub fn update_resolved_color_scheme(
    settings: Res<crate::AppSettings>,
    system: Res<SystemAppearance>,
    mut resolved: ResMut<ResolvedColorScheme>,
    mut changed: MessageWriter<ColorSchemeChanged>,
) {
    if !settings.is_changed() && !system.is_changed() {
        return;
    }
    let next = resolve(settings.appearance.mode, system.0);
    if resolved.0 != next {
        resolved.0 = next;
        changed.write(ColorSchemeChanged(next));
    }
}
```

- [ ] **Step 5: Register in `SettingsPlugin::build`** (`crates/vmux_setting/src/plugin.rs`, chain onto the existing `app` builder expression):
```rust
            .init_resource::<crate::appearance::SystemAppearance>()
            .init_resource::<crate::appearance::ResolvedColorScheme>()
            .add_message::<crate::appearance::ColorSchemeChanged>()
            .add_systems(
                Update,
                (
                    crate::appearance::track_window_theme,
                    crate::appearance::update_resolved_color_scheme,
                )
                    .chain(),
            )
```

- [ ] **Step 6: Run the test.**
Run: `cargo test -p vmux_setting appearance:: 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 7: Commit.**
```bash
git add crates/vmux_setting/src/appearance.rs crates/vmux_setting/src/lib.rs crates/vmux_setting/src/plugin.rs
git commit -m "feat(theme): ResolvedColorScheme resource + ColorSchemeChanged"
```

### Task 12: macOS initial OS appearance read

**Files:**
- Modify: `crates/vmux_desktop/src/` (add a small startup system; place near other macOS setup, e.g. a new `appearance.rs` registered by the desktop plugin)

- [ ] **Step 1: Add a macOS read** mirroring the objc2 pattern in `crates/vmux_desktop/src/glass.rs`. Create `crates/vmux_desktop/src/appearance.rs`:
```rust
use bevy::prelude::*;
use vmux_setting::{ResolvedScheme, SystemAppearance};

#[cfg(target_os = "macos")]
fn read_system_appearance() -> Option<ResolvedScheme> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAppearanceNameDarkAqua, NSApplication};
    let _mtm = MainThreadMarker::new()?;
    let app = unsafe { NSApplication::sharedApplication(_mtm) };
    let appearance = unsafe { app.effectiveAppearance() };
    let names = unsafe { objc2_foundation::NSArray::from_slice(&[NSAppearanceNameDarkAqua]) };
    let best = unsafe { appearance.bestMatchFromAppearancesWithNames(&names) };
    match best {
        Some(name) if &*name == unsafe { NSAppearanceNameDarkAqua } => Some(ResolvedScheme::Dark),
        _ => Some(ResolvedScheme::Light),
    }
}

#[cfg(not(target_os = "macos"))]
fn read_system_appearance() -> Option<ResolvedScheme> {
    None
}

/// Seed `SystemAppearance` once at startup so Device mode resolves correctly on
/// the first frame (winit only reports *changes* afterward).
pub fn seed_system_appearance(mut system: ResMut<SystemAppearance>) {
    if system.0.is_none() {
        if let Some(scheme) = read_system_appearance() {
            system.0 = Some(scheme);
        }
    }
}
```
> Note: verify the exact `objc2-app-kit` 0.3 symbol names (`NSAppearanceNameDarkAqua`, `effectiveAppearance`, `bestMatchFromAppearancesWithNames`) against the installed crate; adjust if the API differs. Add any missing `objc2-app-kit` feature flags (`NSAppearance`, `NSApplication`) to `crates/vmux_desktop/Cargo.toml`.

- [ ] **Step 2: Register** the startup system in the desktop plugin (after `SettingsLoadSet` so the resource exists):
```rust
            .add_systems(Startup, crate::appearance::seed_system_appearance)
```
and add `mod appearance;` where the desktop modules are declared.

- [ ] **Step 3: Build-check the desktop crate** (native, but pulls CEF — run once, expect slow):
Run: `cargo check -p vmux_desktop 2>&1 | tail -20`
Expected: compiles. Fix objc2 symbol mismatches if any.

- [ ] **Step 4: Commit.**
```bash
git add crates/vmux_desktop/src/appearance.rs crates/vmux_desktop/Cargo.toml crates/vmux_desktop/src/*.rs
git commit -m "feat(theme): seed macOS system appearance for Device mode"
```

---

## Phase 4 — Editor + terminal generated colors

### Task 13: Editor syntax theme follows resolved scheme

**Files:**
- Modify: `crates/vmux_editor/src/highlight.rs`
- Modify: `crates/vmux_editor/src/plugin.rs` (pass scheme + re-highlight on `ColorSchemeChanged`)

- [ ] **Step 1: Make theme selection scheme-aware** in `highlight.rs`. Add a theme-name helper and thread it through `default_theme`, `highlight_snippet`, and `Highlighter::highlight`:
```rust
use vmux_setting::ResolvedScheme;

fn theme_name(scheme: ResolvedScheme) -> &'static str {
    match scheme {
        ResolvedScheme::Dark => "base16-ocean.dark",
        ResolvedScheme::Light => "base16-ocean.light",
    }
}

pub fn default_theme(scheme: ResolvedScheme) -> syntect::highlighting::Theme {
    ThemeSet::load_defaults().themes[theme_name(scheme)].clone()
}
```
Change `highlight_snippet` to take `scheme: ResolvedScheme` and use `default_theme(scheme)`. Change `Highlighter::highlight` to take `scheme: ResolvedScheme` and select `&self.themes.themes[theme_name(scheme)]`. Update the in-file tests to pass `ResolvedScheme::Dark`.
> `vmux_editor` already depends on `vmux_setting` (uses `AppSettings`), so the import is available. Verify `base16-ocean.light` exists in `ThemeSet::load_defaults()` (it does in syntect defaults).

- [ ] **Step 2: Thread the scheme at call sites** in `plugin.rs`. Where files are highlighted/pushed, read `Res<ResolvedColorScheme>` and pass `.0` to the highlighter.

- [ ] **Step 3: Re-highlight + re-push on change.** Add a system reacting to `ColorSchemeChanged` that re-highlights open file views and re-emits their content (mirror the existing file-content push path / `FileThemeSent` gating):
```rust
fn rehighlight_on_scheme_change(
    mut reader: MessageReader<vmux_setting::ColorSchemeChanged>,
    /* file view query + Highlighter + Browsers + Commands as in the existing push system */
) {
    if reader.read().last().is_none() { return; }
    // re-run the same highlight + BinHostEmitEvent push used on initial load,
    // for every open FileView.
}
```
Register it in the editor plugin's `Update` systems.

- [ ] **Step 4: Run editor unit tests.**
Run: `cargo test -p vmux_editor highlight 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit.**
```bash
git add crates/vmux_editor/src/highlight.rs crates/vmux_editor/src/plugin.rs
git commit -m "feat(theme): editor syntax highlighting follows app appearance"
```

### Task 14: Terminal palette follows resolved scheme

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs` (`sync_terminal_theme` + add a `ColorSchemeChanged` trigger)

- [ ] **Step 1: Map app scheme → a terminal color scheme when the terminal uses the built-in default.** In `sync_terminal_theme`, after resolving `theme`, choose the color scheme by app appearance if the terminal theme's `color_scheme` is the built-in default (`"catppuccin-mocha"` / `"default"`); otherwise honor the user's explicit choice:
```rust
let resolved = scheme.map(|s| s.0).unwrap_or(vmux_setting::ResolvedScheme::Dark);
let scheme_name = if is_builtin_default(&theme.color_scheme) {
    match resolved {
        vmux_setting::ResolvedScheme::Light => "catppuccin-latte",
        vmux_setting::ResolvedScheme::Dark => "catppuccin-mocha",
    }
} else {
    theme.color_scheme.as_str()
};
let colors = vmux_setting::themes::resolve_theme(scheme_name, &terminal_settings.custom_themes);
```
Add `scheme: Option<Res<vmux_setting::ResolvedColorScheme>>` to the system params and a small `is_builtin_default` helper. (`catppuccin-latte` and `solarized-light` are confirmed builtins.)

- [ ] **Step 2: Fold the resolved scheme into `theme_signature`** so a scheme change re-pushes: hash `resolved` (or `scheme_name`) into the signature.

- [ ] **Step 3: Wake the system on `ColorSchemeChanged`.** `sync_terminal_theme` already re-pushes when the signature changes; ensure it runs when the scheme changes. Add `.run_if(...)` is unnecessary — it runs each Update and compares the hash, which now includes the scheme. Confirm it's registered in `Update`.

- [ ] **Step 4: Run terminal unit tests.**
Run: `cargo test -p vmux_terminal theme 2>&1 | tail -20`
Expected: PASS (update `theme_signature_changes_with_font_size`-style tests if the signature shape changed).

- [ ] **Step 5: Commit.**
```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(theme): terminal palette follows app appearance for default theme"
```

---

## Phase 5 — Verify

### Task 15: Workspace checks + runtime pass

- [ ] **Step 1: Format + lint.**
Run: `cargo fmt && git checkout -- patches/ && cargo clippy --workspace 2>&1 | tail -30`
(`git checkout -- patches/` per project memory: `cargo fmt` reformats vendored patches; keep only `crates/` fmt changes.)
Expected: no warnings/errors.

- [ ] **Step 2: Workspace tests.**
Run: `cargo test --workspace 2>&1 | tail -40`
Expected: PASS. (Project memory: run `--workspace`, not just `-p`, before push; check commit authors after.)

- [ ] **Step 3: Warm + build the app** (CEF). Background-build first per project workflow, then run:
Run: `cargo build -p vmux_desktop 2>&1 | tail -20`
Expected: builds.

- [ ] **Step 4: Runtime pass (USER drives — single final pass).** Verify per the checklist below. Do not claim success without the user's runtime confirmation (project memory: user always runtime-tests observable behavior).

**Runtime checklist:**
- Settings → Appearance: switch Light / Dark / Device; every page reflows colors live (no reload).
- Each page legible in Light and Dark: layout shell, command-bar, start, settings, services, history, spaces, team, agent, files (editor), lsp, extensions, debug, error, terminal.
- Glass panes: translucent + readable in both modes; focus rings visible.
- Device mode: flip macOS System Settings → Appearance; chrome + editor highlighting + terminal recolor without restart.
- Editor: code syntax colors switch (dark ↔ light theme).
- Terminal: palette switches when on the default theme; an explicitly-chosen terminal theme is preserved.

- [ ] **Step 5: Delete the plan file** (project rule: remove the plan once fully implemented) and open the PR.
```bash
git rm docs/plans/2026-06-28-theme-light-dark-system.md
git commit -m "chore: remove completed theme plan"
```

---

## Self-review notes
- Spec coverage: §1 mechanism→T1-3; §2 body consolidation→T1-2; §3 shell→T3; §4 de-hardcode→T5-10; §5 host resolution→T11-12; §6 editor→T13; §7 terminal→T14; testing→T4,T15. All covered.
- The `vmux_team` `@source` gap (spec note) is handled in T10 Step 2.
- Initial Device correctness depends on T12 (macOS read); non-macOS falls back to Dark until first `WindowThemeChanged`, which is acceptable per spec error-handling.
