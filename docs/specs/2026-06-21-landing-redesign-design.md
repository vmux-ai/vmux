# Landing Page Redesign — Design Spec

Date: 2026-06-21
Branch: `feat/landing-redesign`
Scope: `website/` only (the `vmux.ai` landing page / `Home` route). Docs site untouched.

## Goal

Redesign the vmux.ai landing page to be more modern and to tell the story from
`docs/experience.md`, using Tailwind animation and scroll parallax. A cinematic,
scroll-driven page with a sticky install banner.

## Constraints (from current code)

- **Stack:** Dioxus `=0.7.4` with `fullstack` + `router`. The site is SSG —
  pages are pre-rendered (`ServeConfig` + `IncrementalRendererConfig`) then
  hydrated. `default = ["web"]`; `server` feature for the SSR build.
- **Styling:** Tailwind v4 (`@import "tailwindcss"`), tokens in `@theme`, built
  to `/style.css`. Theme: bg `#0a0a0a`, accent (periwinkle) `#7c8aff`.
- **JS interop:** `web-sys` is available only under `cfg(target_arch = "wasm32")`
  and must run inside `use_effect` (see existing docs scroll-spy `mod spy`).
- **Routing:** `/` → `Home`, `/_home` → `HomeStatic` (identical body today),
  `/docs` + `/docs/:slug` under `DocsLayout`.

### Why this drives the technical approach

Because the page is server-rendered then hydrated, animation must work without
client JS at first paint. **Pure-CSS scroll-driven animations** are the chosen
mechanism (decided with the user): declarative, SSR-safe, no VDOM churn, no
custom scroll engine.

- Chrome/Edge 115+ and **Safari 26+**: full support (vmux's macOS audience).
- Firefox: implemented behind a flag → **graceful static fallback** (page fully
  readable, no scroll motion). Accepted by the user.
- Everything is wrapped in `@supports (animation-timeline: view())` with sane
  default (visible / end) states, plus a `prefers-reduced-motion` guard.

## Visual direction

**Punctuated aurora.** Most sections stay deep-dark (`bg`/`surface`) with glass
cards; aurora gradient blooms (periwinkle + violet + cyan) appear only at the
Hero, scene transitions, and the final CTA. A subtle global film-grain overlay.
Periwinkle `#7c8aff` stays the primary accent.

New theme tokens (added to `tailwind.input.css`):
- `--color-aurora-violet: #c264ff`
- `--color-aurora-cyan: #36d6e7`

## Page structure (scroll order)

A sticky banner rides the whole page. Sections in order:

1. **Banner** (sticky top) — logo · GitHub (external) · Docs (`Link` to
   `DocsIndex`) · **Install** (anchors to `#install`). Backdrop-blur fades in
   once scrolled past the hero.
2. **Hero** — aurora bloom + grain; "Vibe Multiplexer — an agent-first workspace
   with a browser and IDE built in"; the experience.md hook ("bridges chat and
   IDE"); curl-copy + `.dmg` button; "Requires macOS 13.0+". Background parallax,
   content fade-up.
3. **Three Pillars** — Co-working · Known by heart · IDE power (glass cards,
   staggered reveal + depth-offset parallax). Copy adapted from experience.md and
   the existing `Features` cards.
4. **Co-working** — human + agent in one shared space; pairing ⇄ autonomy
   metaphor; parallax accents.
5. **Layout** *(pinned, scrubbed)* — tall track + sticky stage; one browser pane
   splits and tiles into browser + terminal as you scroll. "Browser simplicity,
   tmux power." Includes the 3D-flip nod.
6. **Input** *(pinned, scrubbed)* — talk → type → click priority builds across
   the scrub; prompt bar morphs voice → keys → cursor.
7. **Platform** — "More OS than app"; desktop / phone / AR silhouettes drift in
   at different parallax depths; "macOS + Linux today."
8. **CTA** (`id="install"`) — large final install block (curl + `.dmg`), aurora
   bloom.
9. **Footer** — GitHub · MIT (existing).

## Architecture / files

Extract the landing out of `main.rs` (which currently mixes routing, landing, and
the docs shell) into a focused module tree. Filename-based modules only (no
`mod.rs`).

- `website/src/main.rs` — keep routing, `App`, `DocsLayout` + docs helpers.
  Remove `Hero`/`Features`/`Footer`; `Home` and `HomeStatic` both render
  `landing::Landing {}` (de-duplicated).
- `website/src/landing.rs` — `Landing` root (assembles banner + sections),
  shared consts (`ICON`, `GITHUB_URL`, `INSTALL_CMD`), `Banner`, `Footer`.
  Declares the submodules below.
- `website/src/landing/hero.rs`
- `website/src/landing/pillars.rs`
- `website/src/landing/coworking.rs`
- `website/src/landing/scenes.rs` — the two pinned scenes (Layout, Input).
- `website/src/landing/platform.rs`
- `website/src/landing/cta.rs`
- `website/src/hooks.rs` — unchanged; `use_dmg_download` / `use_clipboard_copy` /
  `use_is_mac` reused by Hero + CTA.
- `website/tailwind.input.css` — kept minimal. Only what utilities can't express:
  aurora color tokens, and `@theme` `--animate-*` + `@keyframes` (fade-up,
  parallax-y, float, split, morph) so they're usable as `animate-*` utilities.
  No bespoke component classes for layout/glass/aurora — those are utilities (see
  below). At most a single `.grain` helper if the noise data-URI is too unwieldy
  inline.

Const note: `ICON`/`GITHUB_URL`/`INSTALL_CMD` move to `landing.rs`; if
`main.rs`/docs still need any, re-export from there.

## Animation mechanics (Tailwind-first)

Maximize Tailwind: express animation through utilities, arbitrary properties, and
variants in the RSX `class:` attributes. Custom CSS is limited to `@theme`
keyframes/tokens (above). Patterns:

- **Keyframes as utilities:** define `--animate-fade-up` etc. in `@theme`, apply
  with `animate-fade-up` / `animate-float`.
- **Bind to scroll via arbitrary properties + `supports-*` variant** (Tailwind
  emits the `@supports` wrapper, so the animation only engages where supported):
  - Reveal: `supports-[animation-timeline:view()]:[animation-timeline:view()]`
    `[animation-range:entry_0%_cover_30%]`.
  - Parallax: same `view()` timeline on a `translate3d` keyframe layer.
- **Default = end state via utilities:** `opacity-100 translate-y-0` so
  unsupported browsers (and SSR first paint) show the finished layout; the
  animation only overrides under the `supports-*` variant.
- **Reduced motion:** the built-in `motion-reduce:` variant —
  `motion-reduce:animate-none motion-reduce:[animation-timeline:none]`. No custom
  media query needed.
- **Pinned scenes:** track `min-h-[300vh] [scroll-timeline-name:--scene]`; sticky
  stage `sticky top-0 h-screen`; scrubbed children
  `[animation-timeline:--scene]` + `animate-*`. Sticky is universal; named
  timelines need Safari 26+ (accepted).
- **Glass / aurora / grain via utilities:** glass = `bg-white/5 backdrop-blur
  border border-white/10`; aurora blobs = positioned `blur-3xl` divs with
  `bg-aurora-violet`/`bg-aurora-cyan`/`bg-accent` + `animate-float`; bloom
  backdrops = `bg-[radial-gradient(...)]` arbitrary values.

## Error handling

- Download already surfaces failures via toast / no-op on non-mac; unchanged.
- Pure CSS has no runtime error surface. Unsupported browsers fall back to the
  static end-state (content fully visible) by construction.

## Testing / verification

- `dx build` (or `cargo check`) of `website/` must pass; `cargo fmt` + `clippy`
  clean. **Do not** run a full-repo cargo build — it pulls CEF (huge); the
  `website/` crate is a separate workspace and builds on its own.
- No meaningful unit tests for WASM scroll visuals. Behavior is verified
  **manually in the browser** via `dx serve`:
  - Golden path: hero → scroll through all scenes; pinned scenes scrub; parallax
    + reveals fire; sticky banner + install anchor work.
  - Cross-browser: Chrome (full) and Safari 26+ (full); Firefox shows static
    fallback.
  - `prefers-reduced-motion: reduce`: animations off, everything readable.
  - Will not claim success without a runtime check by the user.

## Out of scope

- Docs site (`docs.rs`, `markdown.rs`, `DocsLayout`, scroll-spy).
- New product screenshots/assets — scenes are built from styled DOM, not images.
- Markdown/content changes to `docs/experience.md`.

## Workflow

Worktree `.worktrees/landing-redesign` (branch `feat/landing-redesign`, off
`origin/main`). PR to `main` via `gh pr create`.
