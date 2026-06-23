# Cinematic Landing Redesign — Design Spec

- Date: 2026-06-23
- Status: Approved (pending spec review)
- Scope: `website/` landing page only (Dioxus + Tailwind). Docs/routes untouched.

## Goal

Rebuild the landing page as a single, cinematic, scroll-driven story with strong
single-line headlines and dramatic parallax. The page tells one narrative arc:

> It starts as **just a browser** → hit ⌘K, **visit an agent** → it **splits into
> an IDE** → where **people and agents work side by side** → **more OS than app** →
> install.

The brightness of the page is part of the story: it opens **light**, dives into a
**dark neon** core where the IDE power lives, then returns to **light** for the
finale. Everything is **liquid glass** — translucent frosted panes — in a light
variant up top and a dark neon variant in the core.

## Reference & style language

Reference: high-end "cinematic marketing site" genre (Viktor Oddy tutorial,
Gemini + Seedance). Defining traits to adopt:

- Full-bleed cinematic background, high-contrast, borderless.
- Huge mixed-weight single-line headlines: small muted lead-in + heavy punch line
  (e.g. reference "A New Way" / "to Manage Your Digital Wealth").
- Floating glass UI: rounded nav pill + floating glass cards.
- Scroll-tied motion: the visual reacts continuously to scroll (video scrubs,
  panes move, tone shifts) rather than discrete fade-ins.
- Seamless transitions between sections (no hard breaks).

## Locked decisions

- **Visual engine: hybrid.** Code-generated aurora motion everywhere (reuse the
  `aurora-cyan` / `aurora-violet` / `accent` palette + the 3D pane scene), plus one
  short looping cinematic clip behind the hero only.
- **Scope: full cinematic rewrite.** Replace the current section spine; fold/trim
  existing sections into the arc.
- **Theme: liquid glass throughout**, in two variants (light / dark neon).
- **Tonal arc: light bookends** (see table).

### Tonal arc (light bookends)

| Act | Beat            | Tone               |
|-----|-----------------|--------------------|
| 0   | Hero            | ☀ light glass      |
| 1   | Browser         | ☀ light glass      |
| 2   | Visit an agent  | ☀ light glass      |
| 3   | IDE climax      | ● dark neon glass  |
| 4   | Co-working      | ● dark neon glass  |
| 5   | Platform        | ● dark neon glass  |
| 6   | CTA finale      | ☀ light glass      |

Two cinematic tone flips, both scroll-tied:

- **light → dark neon** at the Act 2 → 3 boundary — the "power revealed" moment.
- **dark neon → light** at the Act 5 → 6 boundary — emerge into the invite.

## Liquid glass system

Two glass variants, selected per-act by a `data-tone` wrapper (see Technical):

- **Light glass** (`data-tone="light"`): bright airy surfaces — translucent white
  panes (`bg-white/40–60` + `backdrop-blur-xl`), soft white borders
  (`border-white/60`), subtle top highlight, soft diffuse shadow. Dark text on
  light. Accent/cyan/violet used as colored pills and glows at low intensity.
- **Dark neon glass** (`data-tone="dark"`): the current vmux look intensified —
  near-black translucent panes (`bg-white/5–10` + `backdrop-blur-xl`), **neon**
  borders (`border-aurora-*/40`) and glow shadows (`shadow-aurora-*/30`), light
  text, brighter aurora bloom.

Both share the same frosted-glass geometry (rounded, blurred, layered highlight)
so panes feel continuous through a tone flip — only color/contrast changes.

## Color language (tokens)

- `aurora-cyan` (#36d6e7) = browser / web.
- `accent` (#7c8aff, indigo) = agent / IDE / editor.
- `aurora-violet` (#c264ff) = terminal.
- New **light-surface tokens** for light acts (bg/surface/text/text-muted/glass),
  applied via `data-tone="light"` scope. Dark tokens remain today's defaults.

## Narrative acts (top → bottom)

Each act is full-viewport with a single mixed-weight headline, a cinematic aurora
background in its tone, and liquid-glass panes. Headlines are proposed copy (final
wording tuned during implementation).

### Act 0 — Hero · ☀ light glass

- Full-bleed light background: hero `<video>` clip blended over a light aurora
  flow (soft cyan/violet/accent on near-white), with a code-gen fallback.
- Floating **light** glass nav pill (top center), replacing the sticky banner:
  logo · GitHub · Docs · Install.
- Headline: muted "It starts as" / huge **"just a browser."**
- Subline: "The browser that bridges chat and IDE."
- Floating light-glass install card (install command + copy) + Download .dmg.
- Scroll cue.
- Parallax: background translateY on `--sy`; headline slight counter-parallax.

### Act 1 — Browser · ☀ light glass

- One clean, large **light-glass** browser frame (cyan accents), reusing a
  light-toned `browser_frame`.
- Headline: muted "Familiar on the surface." / **"You already know how."**
- Sub: looks and acts like a standard web browser — zero learning curve.
- Parallax: frame rises + scales on enter; soft cyan glow drifts.

### Act 2 — Visit an agent · ☀ light glass (pivot)

- Address bar is the hero of this beat. As `--p` progresses, scroll scrubs: the
  address bar "types" `⌘K → vmux://agent/…`, and the frame body morphs from a web
  page into an agent chat. Cyan accents warm toward accent/indigo (still on light).
- Headline: **"Hit ⌘K. Visit an agent."**
- Sub: every agent, terminal, and space lives at its own address — ready to share
  or jump back to.
- Parallax: frame tilts/zooms on `--p`; `[data-tilt]` mouse parallax.
- **End of act: the light → dark neon flip begins** (background cross-fades into the
  dark core as the IDE section approaches).

### Act 3 — IDE climax · ● dark neon glass (peak)

- The single pane **splits**: editor + terminal panes fly in on Z-depth driven by
  `--p` (extends the current `LayoutScene`), forming the tmux-style layout, agent
  now driving via MCP. Full dark neon: glowing aurora edges, deep shadows.
- Headline: muted "Then it" / huge **"splits into an IDE."** Secondary:
  "Browser simplicity, tmux power."
- Biggest parallax moment: 3D depth on `--p`, mouse tilt on `--rx`/`--ry`, neon
  cyan (browser) + accent (editor) + violet (terminal).
- MCP tool chips (`vmux_browser_navigate`, `vmux_run`, …) glow in as it settles.

### Act 4 — Co-working + prompting · ● dark neon glass

Folds the current Coworking and InputScene into one beat.

- Headline: **"People and agents, side by side."**
- you↔agent autonomy slider (reuse Coworking art, dark neon).
- **Prompt your agents — talk or type.** Talk and Type are explicitly the two ways
  you *prompt the agent*:
  - **Talk** — speak your prompt; direct the whole space hands-free.
  - **Type** — type your prompt; plus tmux-style `<leader>` commands for layout.
  - **Click** — grounded, predictable browser control when you want it.
- Reuse `talk_art` / `type_art` / `click_art`, relabeled so talk/type read as
  prompting an agent, not generic "input".

### Act 5 — Platform · ● dark neon glass

- Condensed Platform section.
- Headline: **"More OS than app."**
- Floating device mockups (desktop / phone / AR-VR) on `animate-float`.
- **End of act: the dark neon → light flip begins** into the finale.

### Act 6 — CTA finale · ☀ light glass

- Headline: **"Install vmux."** (large finale).
- Light aurora finale + light-glass install card + Download .dmg.
- Footer (GitHub · license).

## Shared cinematic mechanics

- **Floating glass nav pill**: restyle `Banner` into a centered `rounded-full`
  backdrop-blur pill, light-toned, like the reference.
- **Aurora background engine**: code-gen flowing aurora via layered blurred radial
  gradients animated with new Tailwind keyframes; light palette in light acts, neon
  palette in dark acts. CSS-driven, no per-frame JS.
- **Tonal transitions**: at each light↔dark boundary the incoming act's full-bleed
  background cross-fades in over the previous one, scroll-tied (opacity from `--p`
  of a transition band), so the flip reads as a smooth cinematic shift while glass
  panes keep their frost.
- **Hero clip scroll-scrub** (signature reference effect): as the hero scrolls
  away, set `video.currentTime = p * duration` so the clip scrubs with scroll.
- **Staged scenes**: extend `[data-scene]` so a single tall sticky section drives
  multiple sub-stages off `--p` thresholds (Act 2 morph, Act 3 split).
- **Reduced motion**: `prefers-reduced-motion: reduce` keeps each act at its final
  tone (no animated flip), reveals shown, scenes pinned at a representative `--p`
  (set by `scroll.rs`'s early-return branch), aurora/reveal animations disabled
  via `motion-reduce:*` utilities, and the hero `<video>` hidden via
  `motion-reduce:hidden`.

## Old → new mapping (fold / cut)

- `Hero` → Act 0 (restyled light, full-bleed + floating card).
- `Pillars` → dissolved; its three points distribute into Acts 1 / 3 / 4.
- `Coworking` → Act 4 (slider art reused).
- `Agents` → Act 2 (address-bar morph; MCP chips move to Act 3).
- `LayoutScene` → Act 3 (becomes the climax).
- `InputScene` → Act 4 (relabeled "prompt your agents — talk or type").
- `Platform` → Act 5 (condensed).
- `Cta` → Act 6 (now light).
- `Footer` → unchanged.

## Technical architecture

- **Modules**: replace `landing/{hero,pillars,coworking,agents,scenes,platform,
  cta}.rs` with act-based modules under `landing/` (e.g. `hero.rs`, `browser.rs`,
  `visit.rs`, `ide.rs`, `coworking.rs`, `platform.rs`, `cta.rs`). Rewrite
  `landing.rs` to compose the acts in order. Filename-based module pattern (no
  `mod.rs`).
- **Tone scoping**: each act renders inside a wrapper with `data-tone="light"` or
  `data-tone="dark"`. `tailwind.input.css` defines token overrides under
  `[data-tone="light"]` (light bg/surface/text/glass vars) and `[data-tone="dark"]`
  (today's dark/neon values). Components read tokens, so the same pane adapts to
  either tone.
- **Reusable components**: lift shared atoms into `landing/parts.rs` —
  `browser_frame`, `tab`, `nav_icon`, avatars/icons (from `pillars.rs`), and
  `website_pane` / `editor_pane` / `terminal_pane` (from `scenes.rs`) — tone-aware
  via tokens.
- **`scroll.rs`**: extend the wasm scroll module to (a) scrub the hero video
  `currentTime`, (b) support staged `--p` thresholds, (c) drive the tonal-transition
  bands' cross-fade. Keep the `.reveal` / `--p` / `--sy` / `[data-tilt]` machinery
  and the reduced-motion branch.
- **Tailwind**: add aurora-flow keyframes and light-surface tokens to
  `tailwind.input.css` `@theme`. Prefer utilities / arbitrary props over
  hand-written CSS; keep the input file minimal.
- **Hero clip**: wire a `<video muted playsinline>` slot (no `autoplay`/`loop` —
  playback is scroll-scrubbed via `currentTime`, so natural playback would fight
  the seek) with a light code-gen aurora fallback as default. Hidden under
  reduced motion via `motion-reduce:hidden`. The actual cinematic mp4
  (Seedance-generated) drops into `website/assets/` later; an optional `poster`
  can be added with it.

## Terminology

Honor vmux terms: "space" (not "workspace"), "the layout" (UI) and "page" (web
content); never "chrome" or "shell".

## Testing / verification

- Run `make website` (tailwind watch + `dx serve --platform web`); verify each act
  renders and scrolls.
- Manually scroll-test in a real browser: hero clip scrub, Act 2 address-bar morph,
  both tonal flips (2→3, 5→6), Act 3 pane split, tilt parallax (golden path + fast
  scroll).
- Verify **light-mode legibility**: text/contrast on light glass, accent intensity.
- Toggle `prefers-reduced-motion`; confirm static per-act tones and fallbacks.
- Verify observable behavior in the browser, not just that it compiles.

## Out of scope / open items

- The actual cinematic hero mp4 (sourced/generated separately). Ship the code-gen
  light fallback as default.
- No changes to `/docs` routes, markdown, or non-landing pages.
- No new dependencies unless a lightweight WebGL helper proves necessary for the
  aurora engine (default is pure CSS).
