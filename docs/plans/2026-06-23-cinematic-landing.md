# Cinematic Landing Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the `website/` landing page as one cinematic, scroll-driven story (browser → agent → IDE → co-working → platform → install) with single-line headlines, dramatic parallax, a light→dark-neon→light "light bookends" tonal arc, and a dual liquid-glass theme.

**Architecture:** Act-based Dioxus components composed by `landing.rs`. Each act renders inside a `data-tone="light|dark"` wrapper; Tailwind v4 theme tokens are re-scoped per tone so the same components adapt color/contrast. Scroll choreography reuses the existing wasm `scroll.rs` (`--p` per `[data-scene]`, `--sy`, `[data-tilt]`), extended to scrub a hero `<video>` and to cross-fade tonal-transition bands. Shared atoms live in `landing/parts.rs`.

**Tech Stack:** Rust, Dioxus 0.7 (`fullstack`, web/wasm), Tailwind CSS v4 (`tailwindcss` CLI), `web-sys` for scroll JS. Build/serve via `make website`.

---

## Verification model (read first)

This is a visual feature with **no unit-test harness** in `website/`. Each task verifies via:

1. **CSS build:** `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify` (or `make build-website-css`) — must exit 0.
2. **Compile gate (host):** `cd website && cargo check` — checks all non-wasm component code.
3. **Wasm/runtime gate:** `cd website && dx build --platform web` (compiles the wasm bundle incl. `cfg(target_arch="wasm32")` `scroll.rs`). For live checks use `make website` then open the served URL.
4. **Manual browser check:** explicit "scroll to X, see Y" per task. Per project rule, visual correctness MUST be runtime-verified in a browser; do not claim a task done on compile alone.

RSX/CSS in this plan is complete, compiling code, but exact visual values (blur radii, glow opacity, translateZ depths, color stops) are **starting points to tune live in the browser** — that tuning is expected iteration, not a placeholder.

All work happens in the worktree `.worktrees/website-cinematic-landing` (branch `website/cinematic-landing`). Run commands from there.

---

## File structure

**Create:**
- `website/src/landing/parts.rs` — shared atoms (icons, `browser_frame`, `tab`, `nav_icon`, avatars, `website_pane`/`editor_pane`/`terminal_pane`) + new helpers (`nav_pill`, `scroll_cue`, `install_card`, `headline`).
- `website/src/landing/browser.rs` — Act 1 (light).
- `website/src/landing/visit.rs` — Act 2 (light, agent morph).
- `website/src/landing/ide.rs` — Act 3 (dark neon, pane split climax).

**Modify:**
- `website/tailwind.input.css` — per-tone token scopes, `.glass` utility, aurora keyframes.
- `website/Cargo.toml` — add `web-sys` features `HtmlMediaElement`, `HtmlVideoElement`.
- `website/src/landing.rs` — compose acts + `data-tone` wrappers + fixed aurora bg + tonal-transition bands + floating nav pill.
- `website/src/landing/hero.rs` — Act 0 (light, full-bleed, video slot).
- `website/src/landing/coworking.rs` — Act 4 (dark; merge co-working + talk/type prompting).
- `website/src/landing/platform.rs` — Act 5 (dark, condensed).
- `website/src/landing/cta.rs` — Act 6 (light).
- `website/src/landing/scroll.rs` — hero video scrub.

**Delete (after their content is lifted/folded):**
- `website/src/landing/pillars.rs` (atoms → `parts.rs`; copy points fold into acts).
- `website/src/landing/scenes.rs` (panes → `parts.rs`; `LayoutScene`→`ide.rs`, `InputScene`→`coworking.rs`).
- `website/src/landing/agents.rs` (folds into `visit.rs`).

---

## Task 1: CSS foundation — tone tokens, glass utility, aurora keyframes

**Files:**
- Modify: `website/tailwind.input.css`

- [ ] **Step 1: Add per-tone token scopes + glass utility + aurora keyframes**

In `website/tailwind.input.css`, add the aurora animation token inside the existing `@theme { … }` block (next to `--animate-float`):

```css
    --animate-aurora: aurora 22s ease-in-out infinite;

    @keyframes aurora {
        0%, 100% { transform: translate3d(0, 0, 0) scale(1); }
        33% { transform: translate3d(4%, -3%, 0) scale(1.08); }
        66% { transform: translate3d(-3%, 4%, 0) scale(1.05); }
    }
```

Then append two new `@layer` blocks at the end of the file:

```css
@layer base {
    [data-tone="dark"] {
        --color-bg: #0a0a0a;
        --color-surface: #151515;
        --color-text: #e0e0e0;
        --color-text-muted: #888;
        --glass-bg: rgba(255, 255, 255, 0.06);
        --glass-border: rgba(255, 255, 255, 0.12);
        --glass-shadow: 0 40px 120px -30px rgba(0, 0, 0, 0.9);
    }
    [data-tone="light"] {
        --color-bg: #eef1f7;
        --color-surface: #ffffff;
        --color-text: #10142a;
        --color-text-muted: #565b73;
        --glass-bg: rgba(255, 255, 255, 0.55);
        --glass-border: rgba(255, 255, 255, 0.75);
        --glass-shadow: 0 30px 90px -35px rgba(40, 50, 90, 0.45);
    }
}

@layer components {
    .glass {
        background: var(--glass-bg);
        border: 1px solid var(--glass-border);
        backdrop-filter: blur(20px) saturate(1.2);
        -webkit-backdrop-filter: blur(20px) saturate(1.2);
        box-shadow: var(--glass-shadow);
    }
}
```

Note: Tailwind v4 emits theme tokens as CSS vars and utilities reference them (e.g. `.text-text { color: var(--color-text) }`), so re-scoping `--color-*` under `[data-tone]` makes `bg-bg` / `bg-surface` / `text-text` / `text-text-muted` adapt automatically inside each act wrapper.

- [ ] **Step 2: Build CSS**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify`
Expected: exits 0, `public/style.css` regenerated.

- [ ] **Step 3: Sanity-check existing site still renders (dark default)**

Run: `cd website && cargo check`
Expected: compiles (no RSX touched yet).

- [ ] **Step 4: Commit**

```bash
git add website/tailwind.input.css website/public/style.css
git commit -m "style(website): per-tone tokens, glass utility, aurora keyframes"
```

---

## Task 2: Extract shared atoms into `parts.rs`

Move reusable atoms out of `pillars.rs` and `scenes.rs` so every act can use them, and add new cinematic helpers. This is a refactor: behavior unchanged until acts are wired.

**Files:**
- Create: `website/src/landing/parts.rs`
- Modify: `website/src/landing.rs` (add `mod parts;`)

- [ ] **Step 1: Create `parts.rs` with moved atoms + new helpers**

Create `website/src/landing/parts.rs`. Move these **verbatim** from their current homes and make them `pub`:

- From `scenes.rs`: `website_pane`, `editor_pane`, `terminal_pane` (the three `fn … -> Element`). Make each `pub fn`.
- From `pillars.rs`: `svg_icon`, `icon_globe`, `icon_term`, `icon_search`, `icon_mic`, `icon_person`, `icon_bot`, `avatar_you`, `avatar_bot`, `nav_icon`, `tab`, `browser_frame`. Make each `pub fn`.

Then add these **new** helpers at the bottom of `parts.rs`:

```rust
use dioxus::prelude::*;

pub fn headline(eyebrow: &str, lead: &str, punch: &str) -> Element {
    rsx! {
        div { class: "reveal",
            if !eyebrow.is_empty() {
                p { class: "text-sm uppercase tracking-[0.25em] text-accent mb-4", "{eyebrow}" }
            }
            h2 { class: "font-bold tracking-tight leading-[1.05]",
                span { class: "block text-2xl sm:text-3xl text-text-muted", "{lead}" }
                span { class: "block text-4xl sm:text-7xl text-text", "{punch}" }
            }
        }
    }
}

pub fn scroll_cue() -> Element {
    rsx! {
        div { class: "mt-12 flex justify-center",
            span { class: "inline-flex h-9 w-6 items-start justify-center rounded-full border border-text-muted/40 p-1.5",
                span { class: "h-2 w-1 rounded-full bg-text-muted/70 animate-cue motion-reduce:animate-none" }
            }
        }
    }
}
```

`nav_pill` and `install_card` are added in Task 9 (they need hooks/route context wired in `landing.rs` / acts); leave them out here.

- [ ] **Step 2: Remove the moved items from `scenes.rs` and `pillars.rs`**

In `scenes.rs`, delete the three pane `fn`s and replace their call sites in `LayoutScene` with `crate::landing::parts::website_pane()` etc. (Temporary — `scenes.rs` is replaced in Tasks 6–8, but it must compile now.)

In `pillars.rs`, delete the moved atoms and update `Pillars`' `art(...)` to call `crate::landing::parts::{browser_frame, tab, icon_*, avatar_*}`. (Temporary — `pillars.rs` is deleted in Task 9.)

- [ ] **Step 3: Register the module**

In `website/src/landing.rs`, add to the module list (keep alphabetical-ish with the others):

```rust
mod parts;
```

- [ ] **Step 4: Compile gate**

Run: `cd website && cargo check`
Expected: compiles. Unused-warning noise is acceptable.

- [ ] **Step 5: Commit**

```bash
git add website/src/landing/parts.rs website/src/landing/scenes.rs website/src/landing/pillars.rs website/src/landing.rs
git commit -m "refactor(website): lift shared landing atoms into parts.rs"
```

---

## Task 3: Hero video scrub in `scroll.rs` + web-sys features

**Files:**
- Modify: `website/Cargo.toml`
- Modify: `website/src/landing/scroll.rs`

- [ ] **Step 1: Add web-sys media features**

In `website/Cargo.toml`, under the `[target.'cfg(target_arch = "wasm32")'.dependencies]` `web-sys` `features = [ … ]` list, add:

```toml
    "HtmlMediaElement",
    "HtmlVideoElement",
```

- [ ] **Step 2: Scrub the hero video from scroll progress**

In `website/src/landing/scroll.rs`, inside the `update` closure (the one that sets `--sy` and iterates `[data-scene]`), after the `for_each_html(&d, "[data-scene]", …)` block, append:

```rust
            if let Some(v) = d
                .query_selector("[data-hero-video]")
                .ok()
                .flatten()
                .and_then(|e| e.dyn_into::<web_sys::HtmlMediaElement>().ok())
            {
                let dur = v.duration();
                if dur.is_finite() && dur > 0.0 {
                    let p = (sy / (vh.max(1.0))).clamp(0.0, 1.0);
                    v.set_current_time(p * dur);
                }
            }
```

This scrubs the hero clip across the first viewport of scroll. The reduced-motion branch already returns early before this code, so scrubbing is disabled there.

- [ ] **Step 3: Wasm compile gate**

Run: `cd website && dx build --platform web`
Expected: wasm bundle builds; no `HtmlMediaElement` feature errors.

- [ ] **Step 4: Commit**

```bash
git add website/Cargo.toml website/Cargo.lock website/src/landing/scroll.rs
git commit -m "feat(website): scroll-scrub the hero video via scroll.rs"
```

---

## Task 4: Act 0 — Hero (light glass, full-bleed, video slot)

**Files:**
- Modify: `website/src/landing/hero.rs`

- [ ] **Step 1: Rewrite `hero.rs` as a light, full-bleed cinematic hero**

Replace the body of `Hero` in `website/src/landing/hero.rs`. Keep the existing hooks (`use_toast`, `use_is_mac`, `use_clipboard_copy`, `use_dmg_download`) and imports. New RSX:

```rust
    rsx! {
        section {
            "data-tone": "light",
            class: "relative isolate min-h-screen overflow-hidden flex flex-col items-center justify-center px-6 text-center bg-bg text-text",
            // full-bleed background: video clip + light aurora fallback
            div { class: "pointer-events-none absolute inset-0 -z-10",
                style: "transform: translateY(calc(var(--sy, 0) * -0.04px))",
                video {
                    "data-hero-video": "1",
                    class: "absolute inset-0 h-full w-full object-cover opacity-60 mix-blend-screen motion-reduce:hidden",
                    autoplay: true,
                    muted: true,
                    "loop": true,
                    "playsinline": true,
                    poster: "/aurora-poster.jpg",
                }
                div { class: "absolute left-1/2 top-1/4 h-[34rem] w-[34rem] -translate-x-1/2 rounded-full bg-accent/25 blur-[130px] animate-aurora motion-reduce:animate-none" }
                div { class: "absolute left-[18%] top-1/3 h-80 w-80 rounded-full bg-aurora-cyan/30 blur-[110px] animate-aurora [animation-delay:-7s] motion-reduce:animate-none" }
                div { class: "absolute right-[16%] top-1/4 h-80 w-80 rounded-full bg-aurora-violet/25 blur-[110px] animate-aurora [animation-delay:-13s] motion-reduce:animate-none" }
            }
            div { class: "relative mx-auto max-w-3xl reveal",
                img { src: ICON, alt: "Vmux icon", class: "w-20 h-20 mb-8 inline-block rounded-3xl shadow-2xl shadow-accent/20" }
                h1 { class: "font-bold tracking-tight leading-[1.02] mb-6",
                    span { class: "block text-2xl sm:text-3xl text-text-muted", "It starts as" }
                    span { class: "block text-6xl sm:text-8xl text-text", "just a browser." }
                }
                p { class: "text-lg sm:text-2xl text-text-muted mb-10 max-w-xl mx-auto",
                    "The browser that bridges chat and IDE."
                }
                InstallCard {}
                div { class: "mt-6 flex justify-center",
                    button {
                        class: "inline-flex items-center px-7 py-3.5 rounded-xl text-base font-semibold bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
                        onclick: move |_| {
                            if is_mac { download(()); }
                            else { toast_api.info("Not supported".to_string(), ToastOptions::new().description("Windows/Linux not supported yet — see GitHub Releases")); }
                        },
                        "Download .dmg"
                    }
                }
                {crate::landing::parts::scroll_cue()}
            }
        }
    }
```

Update hero's hooks: keep `use_toast`, `use_is_mac`, `use_dmg_download` (the Download button needs them); **remove** `let copy = use_clipboard_copy();` (the card owns copy now). Add to the top of `hero.rs`:

```rust
use crate::landing::parts::InstallCard;
```

- [ ] **Step 2: Add `InstallCard` component to `parts.rs`**

`use_clipboard_copy()` returns `Callback<String>` and `use_dmg_download()` returns `Callback<()>` (see `website/src/hooks.rs`). To avoid passing/naming those types, make the card a self-contained component that calls the hooks itself. Append to `website/src/landing/parts.rs`:

```rust
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::use_clipboard_copy;
use crate::landing::INSTALL_CMD;

#[component]
pub fn InstallCard() -> Element {
    let toast = use_toast();
    let copy = use_clipboard_copy();
    rsx! {
        div { class: "glass inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 rounded-xl px-4 py-3 text-sm sm:text-base",
            code { class: "font-mono text-accent", "{INSTALL_CMD}" }
            button {
                class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                onclick: move |_| {
                    copy(INSTALL_CMD.to_string());
                    toast.success("Copied!".to_string(), ToastOptions::new());
                },
                "Copy"
            }
        }
    }
}
```

Then render `InstallCard {}` in both Act 0 (hero) and Act 6 (cta), replacing their hand-rolled copy-command blocks.

- [ ] **Step 3: Compile + CSS build**

Run: `cd website && cargo check && tailwindcss -i tailwind.input.css -o public/style.css --minify`
Expected: compiles, CSS built.

- [ ] **Step 4: Manual browser check**

Run: `make website`, open the served URL.
Expected: hero fills the viewport, **light** background, big "It starts as / just a browser." headline, glass install card, scroll cue. Aurora blobs drift. (Video falls back to aurora until an mp4 exists.)

- [ ] **Step 5: Commit**

```bash
git add website/src/landing/hero.rs website/src/landing/parts.rs
git commit -m "feat(website): light full-bleed hero with video slot + glass card"
```

---

## Task 5: Act 1 — Browser (light glass)

**Files:**
- Create: `website/src/landing/browser.rs`

- [ ] **Step 1: Create `browser.rs`**

```rust
use dioxus::prelude::*;

use crate::landing::parts::{browser_frame, headline, icon_globe, icon_mic, icon_search, tab};

#[component]
pub fn Browser() -> Element {
    rsx! {
        section {
            "data-tone": "light",
            class: "relative isolate min-h-screen overflow-hidden flex flex-col items-center justify-center px-6 bg-bg text-text",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-[30rem] w-[30rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-cyan/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
            }
            div { class: "mx-auto max-w-2xl text-center mb-12",
                {headline("Familiar on the surface", "You already", "know how.")}
                p { class: "mt-5 text-base sm:text-lg text-text-muted reveal",
                    "It looks and acts like a standard web browser. No learning curve — everyone already knows how to use it."
                }
            }
            div {
                class: "w-full max-w-3xl reveal",
                style: "transition-delay: 120ms; transform: translateY(calc((var(--sy, 0)) * -0.02px))",
                {browser_frame(
                    "glass border-aurora-cyan/30",
                    rsx! {
                        {tab(icon_globe("h-3 w-3 text-aurora-cyan"), "New Tab", true)}
                        {tab(icon_globe("h-3 w-3"), "docs", false)}
                    },
                    "Search or enter address",
                    rsx! {
                        div { class: "flex h-full flex-col items-center justify-center p-6",
                            div { class: "flex w-full items-center gap-3 rounded-full border border-text-muted/25 bg-surface/70 px-5 py-3.5",
                                {icon_search("h-4 w-4 shrink-0 text-text-muted")}
                                span { class: "flex-1 text-left text-[13px] text-text-muted", "Search the web" }
                                {icon_mic("h-4 w-4 shrink-0 text-aurora-cyan")}
                            }
                        }
                    },
                )}
            }
        }
    }
}
```

Note: `browser_frame` currently hardcodes `h-64`. For these big hero frames, edit `browser_frame` in `parts.rs` to accept the height via the `frame` class string (remove the hardcoded `h-64`, add `min-h-[16rem]`) so callers control size, or add an explicit `aspect-video` on the outer wrapper here. Pick one and keep it consistent across acts.

- [ ] **Step 2: Register + compile**

Add `mod browser;` and `use browser::Browser;` to `landing.rs`. (Not composed into the page yet — wired in Task 9.)

Run: `cd website && cargo check`
Expected: compiles (unused `Browser` warning OK).

- [ ] **Step 3: Commit**

```bash
git add website/src/landing/browser.rs website/src/landing/parts.rs website/src/landing.rs
git commit -m "feat(website): Act 1 browser (light glass)"
```

---

## Task 6: Act 2 — Visit an agent (light glass, scroll morph)

Folds `agents.rs`. A tall `[data-scene]` whose `--p` scrubs the address bar from a web URL to an agent URL and morphs the body from search → agent chat.

**Files:**
- Create: `website/src/landing/visit.rs`

- [ ] **Step 1: Create `visit.rs`**

```rust
use dioxus::prelude::*;

use crate::landing::parts::{avatar_bot, avatar_you, browser_frame, headline, icon_bot, icon_globe, tab};

const TOOLS: &[&str] = &[
    "vmux_browser_navigate",
    "vmux_run",
    "vmux_read_layout",
    "vmux_update_layout",
];

#[component]
pub fn Visit() -> Element {
    rsx! {
        section { class: "relative min-h-[240vh]", "data-scene": "1", "data-tone": "light",
            div { class: "sticky top-0 h-screen overflow-hidden flex flex-col items-center justify-center px-6 bg-bg text-text",
                div { class: "pointer-events-none absolute inset-0 -z-10",
                    div { class: "absolute left-1/2 top-1/3 h-[30rem] w-[30rem] -translate-x-1/2 rounded-full bg-accent/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
                }
                div { class: "mx-auto max-w-2xl text-center mb-10",
                    {headline("The pivot", "Hit ⌘L.", "Visit an agent.")}
                    p { class: "mt-5 text-base sm:text-lg text-text-muted reveal",
                        "Every agent, terminal, and space lives at its own address — ready to share or jump back to."
                    }
                }
                div {
                    class: "w-full max-w-3xl",
                    style: "transform: perspective(1600px) rotateX(calc((var(--p,0) - 0.5) * -6deg)) scale(calc(0.96 + var(--p,0) * 0.04))",
                    "data-tilt": "1",
                    {browser_frame(
                        "glass border-accent/30",
                        rsx! {
                            {tab(icon_bot("h-3 w-3 text-accent"), "vibe", true)}
                            {tab(icon_globe("h-3 w-3"), "example.com", false)}
                        },
                        "vmux://agent/vibe/2c80e7…",
                        rsx! {
                            // search layer fades out as --p grows
                            div { class: "relative h-full",
                                div {
                                    class: "absolute inset-0 flex items-center justify-center p-6",
                                    style: "opacity: calc(1 - min(var(--p,0) * 2.2, 1))",
                                    div { class: "flex w-full items-center gap-3 rounded-full border border-text-muted/25 bg-surface/70 px-5 py-3.5",
                                        span { class: "flex-1 text-left text-[13px] text-text-muted", "Search the web" }
                                    }
                                }
                                // agent chat layer fades in
                                div {
                                    class: "absolute inset-0 flex flex-col justify-center gap-3 p-6",
                                    style: "opacity: calc(max(var(--p,0) * 2.2 - 1, 0))",
                                    div { class: "flex items-end gap-2",
                                        {avatar_bot()}
                                        div { class: "rounded-xl rounded-bl-sm bg-surface/80 px-3 py-2 text-[12px] text-text", "Tests pass — ship it?" }
                                    }
                                    div { class: "flex items-end justify-end gap-2",
                                        div { class: "rounded-xl rounded-br-sm border border-accent/30 bg-accent/20 px-3 py-2 text-[12px] text-text", "Ship it." }
                                        {avatar_you()}
                                    }
                                }
                            }
                        },
                    )}
                }
                div { class: "mt-6 flex flex-wrap justify-center gap-2 reveal",
                    for t in TOOLS.iter() {
                        span { key: "{t}", class: "font-mono text-[11px] px-2 py-1 rounded-md border border-text-muted/20 bg-surface/60 text-text-muted", "{t}" }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Register + compile**

Add `mod visit;` and `use visit::Visit;` to `landing.rs`.

Run: `cd website && cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add website/src/landing/visit.rs website/src/landing.rs
git commit -m "feat(website): Act 2 visit-an-agent with scroll morph (light)"
```

---

## Task 7: Act 3 — IDE climax (dark neon glass, pane split)

Replaces `scenes::LayoutScene`. Dark neon. The pane split is driven by `--p`.

**Files:**
- Create: `website/src/landing/ide.rs`

- [ ] **Step 1: Create `ide.rs`**

Use the existing `LayoutScene` 3D structure as the base, retoned to dark neon and using `parts` panes. Full module:

```rust
use dioxus::prelude::*;

use crate::landing::parts::{editor_pane, headline, terminal_pane, website_pane};

const TOOLS: &[&str] = &["vmux_browser_navigate", "vmux_run", "vmux_read_layout", "vmux_update_layout", "vmux_create_space"];

#[component]
pub fn Ide() -> Element {
    rsx! {
        section { class: "relative min-h-[300vh]", "data-scene": "1", "data-tone": "dark",
            div { class: "sticky top-0 h-screen overflow-hidden flex flex-col items-center justify-center px-6 bg-bg text-text",
                div { class: "pointer-events-none absolute inset-0 -z-10",
                    div { class: "absolute left-1/2 top-1/2 h-[36rem] w-[36rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/25 blur-[140px] animate-aurora motion-reduce:animate-none" }
                    div { class: "absolute left-[20%] top-1/3 h-72 w-72 rounded-full bg-aurora-cyan/20 blur-[120px] animate-aurora [animation-delay:-6s] motion-reduce:animate-none" }
                    div { class: "absolute right-[18%] bottom-1/4 h-72 w-72 rounded-full bg-aurora-violet/25 blur-[120px] animate-aurora [animation-delay:-11s] motion-reduce:animate-none" }
                }
                div { class: "max-w-2xl text-center mb-8",
                    {headline("The reveal", "Then it", "splits into an IDE.")}
                    p { class: "mt-4 text-text-muted reveal", "Browser simplicity, tmux power." }
                }
                div { class: "w-full max-w-4xl", style: "perspective: 1600px",
                    div {
                        "data-tilt": "1",
                        class: "relative w-full aspect-video rounded-xl glass [transform-style:preserve-3d] will-change-transform",
                        style: "transform: rotateX(calc(var(--rx, 0) * -7deg)) rotateY(calc(var(--ry, 0) * 11deg + (var(--p, 0) - 0.4) * 16deg))",
                        div { class: "flex h-8 items-center gap-1.5 border-b border-white/10 px-3",
                            span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                            span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                            span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                        }
                        div { class: "flex h-[calc(100%-2rem)] gap-2 p-2 [transform-style:preserve-3d]",
                            div { class: "flex-[1.5]", style: "transform: translateZ(calc(var(--p, 0) * 30px))",
                                {website_pane()}
                            }
                            div { class: "flex-1 flex flex-col gap-2 [transform-style:preserve-3d]",
                                div { class: "flex-1", style: "transform: translateZ(calc(var(--p, 0) * 60px))",
                                    {editor_pane()}
                                }
                                div { class: "flex-1", style: "transform: translateZ(calc(var(--p, 0) * 90px))",
                                    {terminal_pane()}
                                }
                            }
                        }
                    }
                }
                div { class: "mt-8 flex flex-wrap justify-center gap-2 reveal",
                    for t in TOOLS.iter() {
                        span { key: "{t}", class: "font-mono text-[11px] px-2 py-1 rounded-md border border-accent/25 bg-accent/10 text-text-muted shadow-lg shadow-accent/10", "{t}" }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Register + compile**

Add `mod ide;` and `use ide::Ide;` to `landing.rs`.

Run: `cd website && cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add website/src/landing/ide.rs website/src/landing.rs
git commit -m "feat(website): Act 3 IDE climax pane-split (dark neon)"
```

---

## Task 8: Act 4 — Co-working + prompting (dark neon); Act 5 — Platform (dark)

Rework `coworking.rs` to merge co-working + talk/type-as-prompting (from `InputScene`), dark-toned. Condense `platform.rs`, dark-toned.

**Files:**
- Modify: `website/src/landing/coworking.rs`
- Modify: `website/src/landing/platform.rs`

- [ ] **Step 1: Rewrite `coworking.rs`**

Replace the file with a dark-tone act: headline, the existing you↔agent slider markup (kept), then a three-up Talk/Type/Click block reframed as prompting. Move the `talk_art`/`type_art`/`click_art`/`bar`/`cap` helpers **verbatim** from `scenes.rs` into `coworking.rs` (private fns). Structure:

```rust
use dioxus::prelude::*;

use crate::landing::parts::headline;

#[component]
pub fn Coworking() -> Element {
    rsx! {
        section { "data-tone": "dark",
            class: "relative isolate overflow-hidden px-6 py-28 sm:py-36 bg-bg text-text",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/4 h-96 w-96 -translate-x-1/2 rounded-full bg-aurora-violet/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
            }
            div { class: "mx-auto max-w-5xl",
                div { class: "text-center mb-16",
                    {headline("Co-working", "People and agents,", "side by side.")}
                }
                // you<->agent autonomy slider (reuse existing markup, swap bg-white/5 -> glass)
                // … keep the current Coworking slider card here, wrapped in `glass rounded-2xl p-8 reveal` …

                div { class: "mt-24 text-center mb-12 reveal",
                    h3 { class: "text-2xl sm:text-3xl font-bold tracking-tight", "Prompt your agents — talk or type." }
                    p { class: "mt-3 text-text-muted max-w-2xl mx-auto", "Talk and type are how you prompt an agent. Click stays grounded in plain browser control." }
                }
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-6",
                    {prompt_tier("01", "Talk", "Speak your prompt — direct the whole space hands-free.", talk_art())}
                    {prompt_tier("02", "Type", "Type your prompt, plus tmux-style <leader> commands for layout.", type_art())}
                    {prompt_tier("03", "Click", "Plain, predictable point-and-click browser control.", click_art())}
                }
            }
        }
    }
}

fn prompt_tier(rank: &str, title: &str, body: &str, art: Element) -> Element {
    rsx! {
        div { class: "reveal flex flex-col gap-4",
            {art}
            div { class: "flex items-baseline gap-3",
                span { class: "font-mono text-sm text-accent", "{rank}" }
                h4 { class: "text-xl font-bold tracking-tight", "{title}" }
            }
            p { class: "text-text-muted leading-relaxed", "{body}" }
        }
    }
}
```

Paste the moved `talk_art`, `type_art`, `click_art`, `bar`, `cap`, and the `InputArt`-free versions (they no longer need the enum) below. For the slider comment block, paste the current `Coworking` slider markup (the `div` with the you/agent avatars and `animate-slide` bar), changing its outer `bg-white/5` to `glass`.

- [ ] **Step 2: Rewrite `platform.rs` (dark, condensed)**

Replace `Platform` body: add `"data-tone": "dark"`, swap device cards `bg-white/5` → `glass`, keep `animate-float`, use `headline("Platform", "More", "OS than app.")` for the heading, drop to a single supporting line.

- [ ] **Step 3: Compile gate**

Run: `cd website && cargo check`
Expected: compiles. (`scenes.rs` still exists but its `InputScene`/`LayoutScene` may now be unused — fine until Task 9 deletes it.)

- [ ] **Step 4: Commit**

```bash
git add website/src/landing/coworking.rs website/src/landing/platform.rs
git commit -m "feat(website): Act 4 co-working+prompting & Act 5 platform (dark neon)"
```

---

## Task 9: Act 6 CTA (light) + orchestrator wiring + nav pill + delete old files

**Files:**
- Modify: `website/src/landing/cta.rs`
- Modify: `website/src/landing.rs`
- Modify: `website/src/landing/parts.rs` (add `nav_pill`)
- Delete: `website/src/landing/pillars.rs`, `website/src/landing/scenes.rs`, `website/src/landing/agents.rs`

- [ ] **Step 1: Retone `cta.rs` to light**

In `cta.rs`, add `"data-tone": "light"` to the `section`, swap the background block to `glass`, and **replace the hand-rolled copy-command block with `InstallCard {}`** (add `use crate::landing::parts::InstallCard;`). Keep the Download button (still uses `use_is_mac`/`use_dmg_download`/`use_toast`) + macOS line. Headline becomes `h2 { class: "text-5xl sm:text-7xl font-bold tracking-tight mb-8", "Install vmux." }`. Keep `id: "install"` and `scroll-mt-20`. Remove the now-unused `use_clipboard_copy` import from `cta.rs`.

- [ ] **Step 2: Add `nav_pill` to `parts.rs`**

```rust
pub fn nav_pill() -> Element {
    rsx! {
        header { class: "fixed top-4 inset-x-0 z-50 flex justify-center px-4",
            nav { class: "glass flex items-center gap-2 sm:gap-4 rounded-full px-4 py-2 text-sm",
                a { class: "flex items-center gap-2 font-bold tracking-tight text-text no-underline hover:text-accent", href: "#top",
                    img { src: crate::landing::ICON, alt: "Vmux", class: "w-6 h-6 rounded-md" }
                    "Vmux"
                }
                a { class: "no-underline text-text-muted hover:text-text px-2 py-1", href: crate::landing::GITHUB_URL, target: "_blank", rel: "noopener noreferrer", "GitHub" }
                Link { class: "no-underline text-text-muted hover:text-text px-2 py-1", to: crate::Route::DocsIndex {}, "Docs" }
                a { class: "no-underline bg-accent text-black font-semibold rounded-full px-4 py-1.5 hover:bg-accent-hover", href: "#install", "Install" }
            }
        }
    }
}
```

- [ ] **Step 3: Rewrite `landing.rs` to compose the acts**

Replace the module list and `Landing` component. Remove `mod pillars; mod agents; mod scenes;` and the old `Banner`. New body:

```rust
mod browser;
mod coworking;
mod cta;
mod hero;
mod ide;
mod parts;
mod platform;
mod visit;
#[cfg(target_arch = "wasm32")]
mod scroll;

use browser::Browser;
use coworking::Coworking;
use cta::Cta;
use dioxus::prelude::*;
use hero::Hero;
use ide::Ide;
use platform::Platform;
use visit::Visit;

pub const ICON: Asset = asset!("/assets/icon.png");
pub const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
pub const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

#[component]
pub fn Landing() -> Element {
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        scroll::init();
    });
    rsx! {
        div { id: "top", class: "bg-bg",
            {parts::nav_pill()}
            Hero {}
            Browser {}
            Visit {}
            Ide {}
            Coworking {}
            Platform {}
            Cta {}
            Footer {}
        }
    }
}
```

Keep the existing `Footer` component (move it below, unchanged).

- [ ] **Step 4: Delete folded files**

```bash
git rm website/src/landing/pillars.rs website/src/landing/scenes.rs website/src/landing/agents.rs
```

- [ ] **Step 5: Compile + CSS + wasm build**

Run: `cd website && cargo check && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: all succeed. Fix any leftover references to deleted modules.

- [ ] **Step 6: Commit**

```bash
git add -A website/src website/public/style.css
git commit -m "feat(website): compose cinematic act flow + floating nav pill; drop old sections"
```

---

## Task 10: Full runtime verification pass

**Files:** none (verification + tuning only)

- [ ] **Step 1: Serve and walk the whole story**

Run: `make website`, open the URL. Scroll top → bottom and confirm:
- Hero: light, full-bleed, headline, glass card, scroll cue; clip scrubs as you scroll out (once an mp4 is present).
- Browser act: light, cyan glass frame.
- Visit act: address bar morphs web → `vmux://agent/…`, search fades to agent chat across the sticky scroll; frame tilts.
- **Tone flip** light → dark neon entering the IDE act.
- IDE act: panes split outward in 3D on scroll; mouse tilt works; MCP chips visible.
- Co-working: slider animates; Talk/Type/Click read as prompting; Platform device float.
- **Tone flip** dark → light entering CTA.
- CTA: light, "Install vmux.", copy + download work.

- [ ] **Step 2: Reduced-motion pass**

Enable OS "Reduce motion" (or DevTools emulate `prefers-reduced-motion: reduce`), reload.
Expected: each act sits at its final tone, reveals shown, scenes pinned (no animated flip), video hidden/poster, no aurora animation. Page is fully readable.

- [ ] **Step 3: Tone-boundary cross-fade (spec "smooth flip")**

v1 uses adjacent full-bleed tone sections; the large blurred aurora softens each boundary. If the Visit→IDE (light→dark) or Platform→CTA (dark→light) boundary reads as a hard cut, add a top gradient bridge to the **incoming** section so the previous tone bleeds in. The flip is scroll-tied for free because the bridge scrolls with its section.

Into the top of `Ide`'s sticky inner `div` (first child), add a bridge from light:

```rust
                div { class: "pointer-events-none absolute inset-x-0 top-0 -z-10 h-48 bg-gradient-to-b from-[#eef1f7] to-transparent motion-reduce:hidden" }
```

Into the top of `Cta`'s `section`, add a bridge from dark:

```rust
            div { class: "pointer-events-none absolute inset-x-0 top-0 -z-10 h-48 bg-gradient-to-b from-[#0a0a0a] to-transparent motion-reduce:hidden" }
```

Rebuild CSS, re-check both flips in the browser. Tune `h-48` / hex stops to match the actual `--color-bg` values if they were adjusted in Task 1.

- [ ] **Step 4: Light-mode legibility + responsive**

Check text contrast on light glass (hero/browser/visit/cta) at desktop and mobile widths. Tune `--color-text-muted` / glass opacity in `tailwind.input.css` if anything is hard to read; rebuild CSS; re-commit.

- [ ] **Step 5: Commit any tuning**

```bash
git add -A website
git commit -m "polish(website): tune cinematic landing contrast + motion"
```

---

## Self-review notes (coverage)

- Spec acts 0–6, tones, both tone flips, liquid-glass dual variants, talk/type-as-prompt, hero video slot+scrub, terminology, reduced-motion → Tasks 1–10.
- Open item (real mp4) stays out of scope; code-gen fallback ships (Task 4).
- After full implementation + merge, delete this plan file per project docs rule.
