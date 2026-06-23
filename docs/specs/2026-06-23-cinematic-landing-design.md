# Cinematic Landing Redesign — Design Spec

- Date: 2026-06-23
- Status: Approved (pending spec review)
- Scope: `website/` landing page only (Dioxus + Tailwind). Docs/routes untouched.

## Goal

Rebuild the landing page as a single, cinematic, scroll-driven story with strong
single-line headlines and dramatic parallax. The page tells one narrative arc:

> It starts as **just a browser** → hit ⌘L, **visit an agent** → it **splits into
> an IDE** → where **people and agents work side by side** → **more OS than app** →
> install.

## Reference & style language

Reference: high-end "cinematic marketing site" genre (Viktor Oddy tutorial,
Gemini + Seedance). Defining traits to adopt:

- Full-bleed cinematic background, dark, high-contrast, borderless.
- Huge mixed-weight single-line headlines: small muted lead-in + heavy white punch
  line (e.g. reference "A New Way" / "to Manage Your Digital Wealth").
- Floating glass UI: rounded nav pill + floating glass cards.
- Scroll-tied motion: the visual reacts continuously to scroll (video scrubs,
  panes move) rather than discrete fade-ins.
- Seamless transitions between sections (no hard breaks).

## Locked decisions

- **Visual engine: hybrid.** Code-generated aurora motion everywhere (reuse the
  existing `aurora-cyan` / `aurora-violet` / `accent` palette + the 3D pane scene),
  plus one short looping cinematic clip behind the hero only.
- **Scope: full cinematic rewrite.** Replace the current section spine; fold/trim
  existing sections into the arc.

## Color language (existing tokens, reused)

- `aurora-cyan` (#36d6e7) = browser / web.
- `accent` (#7c8aff, indigo) = agent / IDE / editor.
- `aurora-violet` (#c264ff) = terminal.

## Narrative acts (top → bottom)

Each act is full-viewport, dark, with a single mixed-weight headline and a
cinematic aurora background. Headlines below are proposed copy (final wording may
be tuned during implementation).

### Act 0 — Hero

- Full-bleed background: hero `<video>` clip + code-gen aurora flow fallback
  layered behind. Dark, high-contrast.
- Floating glass nav pill (top center), replacing the current sticky banner:
  logo · GitHub · Docs · Install.
- Headline (mixed weight): muted "It starts as" / huge white **"just a browser."**
- Subline: "The browser that bridges chat and IDE."
- Floating glass install card (install command + copy) + Download .dmg.
- Scroll cue.
- Parallax: background translateY on `--sy` (slow); headline slight counter-parallax.

### Act 1 — Browser (cyan)

- One clean, large browser frame (cyan), reusing `browser_frame`.
- Headline: muted "Familiar on the surface." / **"You already know how."**
- Sub: looks and acts like a standard web browser — zero learning curve.
- Parallax: frame rises + scales on enter; aurora-cyan glow drifts.

### Act 2 — Visit an agent (cyan → accent) — the pivot

- The address bar is the hero of this beat. As `--p` progresses, scroll scrubs:
  the address bar "types" `⌘L → vmux://agent/…`, and the frame body morphs from a
  web page into an agent chat. Color shifts cyan → accent.
- Headline: **"Hit ⌘L. Visit an agent."**
- Sub: every agent, terminal, and space lives at its own address — ready to share
  or jump back to. (Distills the current Agents section.)
- Parallax: frame tilts/zooms on `--p`; `[data-tilt]` mouse parallax.

### Act 3 — IDE climax (full aurora) — the peak

- The single pane **splits**: editor + terminal panes fly in on Z-depth driven by
  `--p` (extends the current `LayoutScene`), forming the tmux-style layout, with the
  agent now driving it via MCP.
- Headline: muted "Then it" / huge **"splits into an IDE."** Secondary line:
  "Browser simplicity, tmux power."
- This is the biggest parallax moment: 3D depth on `--p`, mouse tilt on
  `--rx`/`--ry`, full aurora (cyan browser + accent editor + violet terminal).
- Optional MCP tool chips (`vmux_browser_navigate`, `vmux_run`, …) as the layout
  settles.

### Act 4 — Co-working + prompting (accent / violet)

Folds the current Coworking and InputScene into one beat.

- Headline: **"People and agents, side by side."**
- you↔agent autonomy slider (reuse current Coworking art).
- **Prompt your agents — talk or type.** Frame Talk and Type explicitly as the two
  ways you *prompt the agent*:
  - **Talk** — speak your prompt; direct the whole space hands-free.
  - **Type** — type your prompt; plus tmux-style `<leader>` commands for layout.
  - **Click** — grounded, predictable browser control when you want it.
- Keep the existing `talk_art` / `type_art` / `click_art` visuals; relabel so
  talk/type read as prompting an agent, not generic "input".

### Act 5 — Platform (violet)

- Condensed version of the current Platform section.
- Headline: **"More OS than app."**
- Floating device mockups (desktop / phone / AR-VR) on `animate-float`.

### Act 6 — CTA finale (accent)

- Headline: **"Install vmux."** (large finale).
- Aurora finale background + install command card + Download .dmg.
- Footer (GitHub · license).

## Shared cinematic mechanics

- **Floating glass nav pill**: restyle `Banner` into a centered `rounded-full`
  backdrop-blur pill (`border-white/10`, translucent), like the reference.
- **Aurora background engine**: code-gen flowing aurora via layered blurred radial
  gradients animated with new Tailwind keyframes (extends today's hero blobs).
  No per-frame JS; CSS-driven.
- **Hero clip scroll-scrub** (signature reference effect): as the hero scrolls
  away, set `video.currentTime = p * duration` so the clip scrubs with scroll.
- **Staged scenes**: extend `[data-scene]` so a single tall sticky section can
  drive multiple sub-stages off `--p` thresholds (used by Act 2 morph and Act 3
  split).
- **Reduced motion**: `prefers-reduced-motion: reduce` keeps the existing
  behavior — reveals shown, scenes pinned at a representative `--p`, video paused
  on a poster frame, aurora animations disabled.

## Old → new mapping (fold / cut)

- `Hero` → Act 0 (restyled, full-bleed + floating card).
- `Pillars` → dissolved; its three points distribute into Acts 1 / 3 / 4.
- `Coworking` → Act 4 (slider art reused).
- `Agents` → Act 2 (address-bar morph; MCP chips move to Act 3).
- `LayoutScene` → Act 3 (becomes the climax).
- `InputScene` → Act 4 (relabeled as "prompt your agents — talk or type").
- `Platform` → Act 5 (condensed).
- `Cta` → Act 6.
- `Footer` → unchanged.

## Technical architecture

- **Modules**: replace `landing/{hero,pillars,coworking,agents,scenes,platform,
  cta}.rs` with act-based modules under `landing/` (e.g. `hero.rs`, `browser.rs`,
  `visit.rs`, `ide.rs`, `coworking.rs`, `platform.rs`, `cta.rs`). Rewrite
  `landing.rs` to compose the acts in order. Keep filename-based module pattern
  (no `mod.rs`).
- **Reusable components**: keep and lift shared atoms — `browser_frame`, `tab`,
  `nav_icon`, avatars/icons (from `pillars.rs`), and `website_pane` /
  `editor_pane` / `terminal_pane` (from `scenes.rs`) — into a shared
  `landing/parts.rs` so multiple acts use them.
- **`scroll.rs`**: extend the existing wasm scroll module to (a) scrub the hero
  video `currentTime` from scroll progress, (b) support staged `--p` thresholds.
  Keep the existing `.reveal`/`--p`/`--sy`/`[data-tilt]` machinery and the
  reduced-motion branch.
- **Tailwind**: add aurora-flow keyframes to `tailwind.input.css` `@theme`
  (alongside `float`/`slide`/`cue`). Prefer utilities/arbitrary props over
  hand-written CSS.
- **Hero clip**: wire a `<video muted loop playsinline>` slot with a rich
  code-gen aurora/WebGL fallback as the default. The actual cinematic mp4 (e.g.
  Seedance-generated) is dropped into `website/assets/` later; until then the
  fallback renders. Poster frame used for reduced-motion.

## Terminology

Honor vmux terms: "space" (not "workspace"), "the layout" (UI) and "page" (web
content); never "chrome" or "shell".

## Testing / verification

- Run `make website` (tailwind watch + `dx serve --platform web`); verify each act
  renders and scrolls.
- Manually scroll-test: hero clip scrub, Act 2 address-bar morph, Act 3 pane split,
  tilt parallax. Verify on a real browser (golden path + fast scroll).
- Toggle `prefers-reduced-motion` and confirm static fallbacks.
- Verify observable behavior in the browser, not just that it compiles.

## Out of scope / open items

- The actual cinematic hero mp4 (sourced/generated separately). Ship the code-gen
  fallback as default.
- No changes to `/docs` routes, markdown, or non-landing pages.
- No new dependencies unless a lightweight WebGL helper proves necessary for the
  aurora engine (default is pure CSS).
