# Light / Dark / System theme for all `vmux://` pages

Date: 2026-06-28
Branch: `feat/theme-light-dark-system`

## Problem

The appearance *setting* is already wired end-to-end, but the pages don't render
light. Specifically:

- `ColorScheme` (`Light` / `Dark` / `Device`) exists in `AppSettings.appearance`,
  persists sparsely to `settings.ron`, has a Select widget on the Settings page,
  and `sync_appearance_to_cef` already pushes it to CEF's emulated
  `prefers-color-scheme` for every webview on change (Device = real OS).
- But every page is styled **dark-only**: `<html class="dark">` is hardcoded and
  static, Tailwind v4's `dark:` variant keys off the `.dark` class
  (`@custom-variant dark (&:is(.dark *))`), the glass tokens are defined only in
  `.dark`, and ~94 hardcoded palette utilities + ~150 inline `bg-white/[…]`
  glass utilities across the page crates have no light fallback.
- Two surfaces render colors **outside CSS**: editor syntax highlighting is
  hardcoded to `base16-ocean.dark` (`vmux_editor/src/highlight.rs`), and the
  terminal pushes concrete RGB from its own named theme (`TermThemeEvent`).

Goal: all 15 `vmux://` pages — plus editor syntax highlighting and the terminal
palette — render correctly in Light, Dark, and System (Device) mode, reacting
live when the setting or the OS appearance changes.

## Scope

In scope (decided):

- All 15 registered pages (CSS chrome): `layout`, `command-bar`, `debug`,
  `error`, `terminal`, `services`, `history`, `spaces`, `team`, `settings`,
  `agent`, `files`, `lsp`, `extensions`, `start`.
- Editor syntax highlighting follows app appearance.
- Terminal palette follows app appearance (explicit user-set term theme still
  wins).
- Light palette is a designed **soft-glass light** aesthetic mirroring the dark
  one (translucent light panes), not flat stock shadcn.

Out of scope: redesigning the dark theme; per-page visual redesign beyond making
light legible/cohesive; a manual in-page theme toggle separate from the existing
Settings Select.

## Approach — Hybrid

Two mechanisms, each matched to how the surface gets its colors:

1. **Chrome (all 15 CSS pages) → CSS `prefers-color-scheme`.** CEF already sets
   the emulated `prefers-color-scheme` from `appearance.mode` (Device lets the
   real OS through). So switching the CSS dark mechanism from the `.dark` class
   to `prefers-color-scheme` makes every chrome page track the setting
   automatically and reactively, with **no new wire protocol and no host-side OS
   read**. The remaining work is purely making light *look right*.

2. **Editor + terminal (host-generated colors) → host-resolved appearance.**
   These colors are produced in Rust (syntect spans; terminal RGB) and pushed
   over existing events, so CSS media queries can't reach them. Add one
   host-side resolved-appearance source, use it to pick the light/dark
   syntect/terminal theme, and re-push on change.

### Rejected alternative

Push a resolved theme to *every* page and toggle `.dark` from JS. More wiring
(extend `ThemeEvent`, make `use_theme` mutate `documentElement`, add a
re-broadcast system), abandons CEF's native System handling, and buys nothing —
no chrome page needs the theme value in JS. Only editor/terminal need the
resolved value, and they already have their own event channels.

## Design

### 1. CSS theme mechanism — `crates/vmux_ui/assets/theme.css`

- Change the dark variant to media-driven:
  `@custom-variant dark (@media (prefers-color-scheme: dark));`
  This makes the existing 53 `dark:` variants in the shared component library
  respond to `prefers-color-scheme` with no per-component changes.
- Move the `.dark { … }` token block into
  `@media (prefers-color-scheme: dark) { :root { … } }`.
- Keep the existing light tokens in `:root`, and **add the missing light
  soft-glass tokens** there (today glass/cef-surface exist only in dark):
  - `--glass: oklch(1 0 0 / 0.55)`
  - `--glass-hover: oklch(1 0 0 / 0.7)`
  - `--glass-active: oklch(1 0 0 / 0.85)`
  - `--glass-border: oklch(0 0 0 / 0.08)`
  - light `--scrim*` and `--cef-surface*` variants
  - retain `backdrop-filter` blur in the `.glass` utility (unchanged).
- Final light/dark glass values are tuned during implementation against the real
  backdrop; the values above are the starting point.

### 2. Consolidate the per-crate body override

The effective dark glass value (`--glass: oklch(0.18 0 0 / 0.82)`, etc.) is set
by a duplicated `html.dark body { … }` block copy-pasted into ~8 per-crate
`index.css` files (`vmux_server`, `vmux_layout`, `vmux_command`, `vmux_setting`,
`vmux_service`, `vmux_space`, `vmux_terminal`, `vmux_history`). Consolidate into
`theme.css` as a single light `body { … }` default plus
`@media (prefers-color-scheme: dark) { body { … } }`, and delete the duplicated
blocks from the per-crate files. Single source of truth; removes drift.

If a per-crate file genuinely needs a unique override, it stays local but is
media-gated the same way.

### 3. HTML shell

In the served shell `index.html` (vmux_server; and the standalone `vmux_ui`
gallery shell):

- Drop `class="dark"`, keep `h-full`.
- Set `color-scheme: light dark` on `<html>` so native form controls,
  scrollbars, and the canvas background follow the resolved scheme.
- Remove the static `html.dark { color-scheme: dark }` rules (now handled by the
  media query / `color-scheme: light dark`).

### 4. De-hardcode page-level colors

The shared component primitives mostly use tokens / `dark:` pairs already and
need no change once the mechanism is media-driven. The work is the **page-level
dark-only literals**:

- Replace literal palette utilities (`bg-cyan-400`, `bg-white/[0.04]`,
  `ring-white/…`, `shadow-[… rgba(0,0,0,…)]`, etc.) with semantic tokens
  (`bg-glass`, `bg-glass-hover`, `text-foreground`, `text-muted-foreground`,
  `border-glass-border`, `bg-background`) or explicit `dark:` pairs where a token
  doesn't fit.
- Agent brand accents (`vmux_ui/src/agent_accent.rs`, ~33 literals): keep each
  agent's brand hue but ensure the accent, glow, and text variants are legible on
  both light and dark backgrounds (mid-tone or `dark:`-paired).
- Highest-density files: `vmux_editor/src/page.rs` + `lsp_page.rs`,
  `vmux_ui/src/agent_accent.rs`, `vmux_service/src/page.rs`,
  `vmux_team/src/page.rs`, `vmux_layout/src/{extensions_page,command_bar/page}.rs`,
  `vmux_space/src/page.rs`, `vmux_agent/src/vibe/setup/page.rs`. `start` reuses
  the command bar, so it inherits command-bar fixes.

This is a page-by-page sweep; each page is independently verifiable.

### 5. Host appearance resolution (new shared piece)

Add a single host-side source of the *resolved* scheme, used only by the
host-generated surfaces (editor, terminal):

- `ResolvedColorScheme(Light | Dark)` Bevy resource.
- Computed from `AppSettings.appearance.mode`:
  - `Light` → Light, `Dark` → Dark.
  - `Device` → OS theme via winit (`WindowThemeChanged` message +
    initial `Window::window_theme`, already plumbed in the
    `patches/bevy_window` patch but unused). Fallback to Dark when the OS theme
    is unknown (e.g. Linux/CI), preserving today's behavior.
- Emit a `ColorSchemeChanged` message when the resolved value changes (on
  settings change or OS theme change). Per project rules, this is Bevy message
  integration: register the message + systems in the owning plugin; editor and
  terminal subscribe.

Chrome needs nothing from this — CEF + CSS already handle it. In Device mode the
OS read here and CEF's internal System resolution observe the same OS appearance,
so chrome and generated colors stay consistent.

### 6. Editor syntax highlighting — `crates/vmux_editor/src/highlight.rs`

- Select the syntect theme by resolved appearance: `base16-ocean.dark` (dark) ↔
  `base16-ocean.light` (light) — both in `ThemeSet::load_defaults()`.
- Thread the resolved appearance into the highlight call sites (the highlighter
  currently hardcodes the theme name in two places).
- On `ColorSchemeChanged`, re-highlight the open file(s) and re-push so the
  editor recolors live without a reload.

### 7. Terminal palette

- When following app appearance, resolve a light vs dark default terminal theme
  and populate `TermThemeEvent` (foreground/background/cursor/ansi) accordingly.
- Add a light default terminal palette to pair with the existing dark default.
- A terminal theme the user has explicitly configured in settings still takes
  precedence over the appearance-derived default.
- On `ColorSchemeChanged`, re-push `TermThemeEvent` so open terminals recolor
  live.

## Data flow

```
Settings page (Select)
  └─intent─> on_settings_command ─> AppSettings.appearance.mode
       ├─> sync_appearance_to_cef ─> CEF prefers-color-scheme ─> CSS @media ─> all chrome pages (live)
       └─> ResolvedColorScheme (mode, or winit OS theme if Device)
              └─ ColorSchemeChanged ─┬─> editor: re-pick syntect theme, re-highlight, re-push
                                     └─> terminal: re-pick palette, re-push TermThemeEvent

OS appearance change (Device mode)
  ├─> CEF System ─> CSS @media ─> all chrome pages (live)
  └─> winit WindowThemeChanged ─> ResolvedColorScheme ─> ColorSchemeChanged ─> editor + terminal
```

## Error handling / edge cases

- Unknown OS theme (Linux/CI, or before the first `WindowThemeChanged`): resolve
  Device → Dark (current behavior); never panic.
- CEF emulation already unit-tested for the mode→CefColorMode mapping; no change
  there. If CEF emulation ever fails to apply, chrome falls back to the OS
  `prefers-color-scheme`, which is an acceptable degradation.
- Glass/accent values that fail contrast in light are tuning, not structural;
  iterate during the per-page sweep.

## Testing

- Unit: `ResolvedColorScheme` mapping (mode × OS theme → resolved), including the
  Device fallback. Existing rkyv roundtrip tests for `ThemeEvent`,
  `TermThemeEvent`, `FileThemeEvent` stay green.
- Update regression tests that pin CSS text: the `--glass` assertion in
  `crates/vmux_layout/src/command_bar/style.rs` and the `include_str!`
  source-scrape asserts in `style.rs` + `tests/page_source.rs` (these only fail
  under native `cargo test -p vmux_layout`).
- `cargo fmt` / `clippy` clean; `cargo test --workspace` before push.
- Runtime (user-driven, final pass): each page in Light / Dark / Device; toggle
  the setting live and confirm chrome flips without reload; flip macOS appearance
  while in Device and confirm chrome + editor + terminal all recolor.

## Boundaries

- `theme.css` is the single source of truth for design tokens (light + dark).
- CEF's `prefers-color-scheme` is the mechanism for all chrome; no per-page JS.
- `ResolvedColorScheme` is the single host-side truth for generated colors
  (editor, terminal) and the only place that reads the OS appearance.
- Each page's de-hardcoding edit is isolated and independently testable.

## Build order

1. `theme.css` mechanism + light soft-glass tokens; consolidate body override;
   HTML shell. (Chrome switches to media-driven dark.)
2. De-hardcode pages, page by page.
3. Host `ResolvedColorScheme` + `ColorSchemeChanged` + editor light syntect theme.
4. Terminal light palette.
5. Fix regression tests; workspace test; final runtime pass.
