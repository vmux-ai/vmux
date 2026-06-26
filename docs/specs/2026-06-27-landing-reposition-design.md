# Landing reposition — "the browser that gets sh*t done"

Date: 2026-06-27

## Goal

Broaden the landing page positioning from a dev-only message ("the browser that
ships code") to a general agentic one ("get anything done with agents"), without
losing the dev/IDE story that differentiates vmux from generic "AI browsers".

Three canonical asks anchor the breadth: a flight booking, a restaurant website,
and a self-referential `vmux` theme-support PR.

## Decisions (validated via visual brainstorm)

1. **Tone — edgy.** Real profanity ships on the public page.
2. **Banner — the showcase IS the hero.** The cycling vmux demo replaces the
   old text-only banner. Headline leads with "Anything, done."; "browser" is
   reminded in the subhead; the dev/IDE climax downstream is unchanged.
3. **Showcase format — one cycling space.** A single real vmux space (agent pane
   priority + browser + terminal) that cycles through the three asks with a dot
   indicator. Cinematic, one outcome at a time.

## Copy

**Hero / banner** (`hero.rs`)

- Eyebrow: `ASK FOR ANYTHING`
- Lead: `One prompt.`
- Punch: `Anything, done.`
- Subhead: `The browser that gets sh*t done — a flight, a website, a PR, handled by your agents while you watch.`
- CTA: `InstallCard` (curl) + `Download .dmg`, below the demo.

Three asks (ask text + working pane + status):

| Ask | Working pane | Status label |
|-----|--------------|--------------|
| `Find me a flight to Tokyo from Paris next month` | browser → flight results | `tokyo` |
| `Make me a website for my new restaurant` | preview pane (Osteria Lina) | `lina` |
| `Add theme support to vmux. Open a PR.` | editor diff + terminal (`PR #182`) | `theme` |

## Section-by-section changes

### `website/src/landing/hero.rs`
Rewritten as the banner: `data-tone="dark"`, aurora blobs, the eyebrow/lead/punch
headline, the browser-reminder subhead, the cycling demo (`showcase::vmux_demo()`),
then `InstallCard` + `Download .dmg` + scroll cue. The old `[data-hero-video]`
element is dropped (its use in `scroll.rs` is guarded).

### `website/src/landing/showcase.rs` (new)
Exports `pub fn vmux_demo() -> Element` — the cycling demo only (no section
wrapper), consumed by the hero:

- A vmux window frame: titlebar (3 dots) + tmux tab strip + body + tmux status
  line (agent `accent`, browser/preview `aurora-cyan`, terminal `aurora-violet`).
- **Three scene layers** stacked absolutely, cross-faded by a pure-CSS keyframe
  cycle. Agent pane (priority) shows the ask + confirmation; working pane +
  terminal + tabs + status label change per scene.
- A three-dot progress indicator synced to the cycle.

### `website/src/landing.rs`
- `mod showcase;` only — no standalone section component; the demo is rendered by
  the hero. No separate section is inserted into the arc.

### `website/src/landing/visit.rs`
- Soften the dev-only chat: bot line `"Tests pass — ship it?"` → `"All done — ship it?"`.
  Keep the `"Ship it."` reply.

### `website/tailwind.input.css`
- `scene` / `scenedot` keyframes (~12s, three phases) + `.scene-cycle` /
  `.scene-dot` classes. Scene layers use `animation-delay` `0s` / `-4s` / `-8s`;
  dots reuse the same timing.
- `motion-reduce`: `.scene-stack` becomes a static vertical stack, layers stop
  animating and show, dots hide — no content lost.

## Animation / accessibility

- Pure CSS, no new JS (consistent with `animate-aurora` / `animate-slide`).
- All cycling elements carry `motion-reduce:animate-none`.
- Reduced-motion fallback: scenes stack vertically, all visible, dots static.

## Responsive

- Demo panes stack (`flex-col`) on narrow screens and sit side-by-side
  (`sm:flex-row`) on `sm+`. Window max-width `max-w-4xl`.

## Out of scope

- No changes to `browser.rs`, `coworking.rs`, `platform.rs`, `cta.rs`.
- No new crates, no JS framework changes.
- No rewrite of the IDE or co-working narrative (that was option C, rejected).

## Verification

- `cd website && cargo check --target wasm32-unknown-unknown` (standalone crate).
- `cd website && cargo fmt`.
- Runtime: the banner demo cycles all three asks, reduced-motion shows the static
  stack, mobile stacks panes, and the install CTA works.
