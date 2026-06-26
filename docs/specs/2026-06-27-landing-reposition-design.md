# Landing reposition — "the browser that gets sh*t done"

Date: 2026-06-27

## Goal

Broaden the landing page positioning from a dev-only message ("the browser that
ships code") to a general agentic one ("get anything done with agents"), without
losing the dev/IDE story that differentiates vmux from generic "AI browsers".

Three canonical asks anchor the breadth: a flight booking, a restaurant website,
and a self-referential `vmux` theme-support PR.

## Decisions (validated via visual brainstorm)

1. **Hero tone — edgy.** Real profanity ships on the public page.
2. **Scope — hero + proof.** Reframe the hero, add ONE new "Ask for anything"
   showcase section, keep the browser → ⌘K → IDE climax, soften the single
   over-dev line in `Visit`.
3. **Showcase format — one cycling space.** A single real vmux space (agent pane
   priority + browser + terminal) that cycles through the three asks with a dot
   indicator. Cinematic, one outcome at a time.

## Copy

**Hero** (`hero.rs`)
- Kicker: `The browser`
- Punch: `that gets sh*t done.`
- Subhead: `Book a flight, build a website, ship a PR — just ask your agents.`

**Showcase** (`showcase.rs`)
- Eyebrow: `ASK FOR ANYTHING`
- Lead: `One prompt.`
- Punch: `Anything, done.`
- Subhead: `Travel, a website, a pull request — your agents do the work while you watch.`

Three asks (ask text + working pane + status):
| Ask | Working pane | Status label |
|-----|--------------|--------------|
| `Find me a flight to Tokyo from Paris next month` | browser → flight results | `tokyo` |
| `Make me a website for my new restaurant` | preview pane (Osteria Lina) | `lina` |
| `Add theme support to vmux. Open a PR.` | editor diff + terminal (`PR #182`) | `theme` |

## Section-by-section changes

### `website/src/landing/hero.rs`
Replace the two headline spans + subhead `p` text only. Keep video bg, aurora
blobs, `InstallCard`, download button, scroll cue untouched.

### `website/src/landing/showcase.rs` (new)
New `Showcase` component, `data-tone="dark"` (sits between the light `Visit` and
dark `Ide`, gives the arc rhythm). Structure:

- `headline("ASK FOR ANYTHING", "One prompt.", "Anything, done.")` + subhead `p`.
- A vmux window frame: titlebar (3 dots) + tmux tab strip + body + tmux status
  line, reusing the pane aesthetic from `parts.rs`
  (agent `accent`, browser/preview `aurora-cyan`, terminal `aurora-violet`).
- **Three scene layers** stacked absolutely inside the body, cross-faded by a
  pure-CSS keyframe cycle. Agent pane (priority, widest) shows the ask bubble +
  agent confirmation; the working pane + terminal + tab strip + status label
  change per scene.
- A three-dot progress indicator synced to the cycle.

### `website/src/landing.rs`
- `mod showcase;` + `use showcase::Showcase;`
- Insert `Showcase {}` between `Visit {}` and `Ide {}`.

### `website/src/landing/visit.rs`
- Soften the dev-only chat: bot line `"Tests pass — ship it?"` → `"All done — ship it?"`.
  Keep the `"Ship it."` reply.

### `website/tailwind.input.css`
- Add a `--animate-showcase` cycle keyframe (~12s, three ~33% phases of opacity)
  plus the matching `@keyframes`. Scene layers use `animation-delay` offsets of
  `0s` / `-4s` / `-8s`; dots reuse the same timing.
- `motion-reduce`: layers fall back to `animate-none`; the three scenes render
  stacked and static so no content is lost.

## Animation / accessibility

- Pure CSS, no new JS (consistent with `animate-aurora` / `animate-slide`).
- All cycling elements carry `motion-reduce:animate-none`.
- Reduced-motion fallback: scenes stack vertically, all visible, dots static.

## Responsive

- Showcase window: agent pane left, working pane(s) right on `sm+`; on narrow
  widths panes stack vertically. Window max-width matches the `Ide` section
  (`max-w-4xl`-ish) for visual continuity.

## Out of scope

- No changes to `browser.rs`, `coworking.rs`, `platform.rs`, `cta.rs`.
- No new crates, no JS framework changes, no asset/video changes.
- No rewrite of the IDE or co-working narrative (that was option C, rejected).

## Verification

- `cargo check --target wasm32-unknown-unknown -p vmux_website` (typecheck pages).
- `cargo fmt` (crates only; restore `patches/` if touched).
- Runtime: user runs the site, confirms hero copy, the showcase cycles through
  all three asks, reduced-motion shows the static stack, and mobile stacks panes.
