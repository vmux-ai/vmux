# Landing Page Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the vmux.ai landing page (`website/` `Home` route) as a modern, cinematic, scroll-driven page that tells the `docs/experience.md` story, using pure-CSS scroll-driven animation expressed through Tailwind utilities.

**Architecture:** Extract the landing out of `website/src/main.rs` into a focused `landing` module tree (one file per section). Animation is pure CSS via Tailwind utilities + arbitrary properties (`[animation-timeline:view()]`, named `scroll-timeline`) gated by the `supports-[...]` variant, with `motion-reduce:` fallbacks. The site is Dioxus fullstack SSG, so every section renders correctly server-side with the finished layout as the default (no-JS) state.

**Tech Stack:** Dioxus `=0.7.4` (fullstack + router, SSG), Tailwind CSS v4.2.4, `dioxus-cli` (`dx`).

---

## Conventions & commands (read first)

- **No tests for this work.** The `website/` crate has no test harness and RSX scroll visuals aren't unit-testable. Each task verifies by **compiling** + **building CSS** + **manual browser check**. Do not invent a test framework.
- **Compile check (fast):** `cd website && dx build --platform web` — expect `cargo`/`dx` to finish with no errors. (Do NOT run a full-repo `cargo build`; it pulls CEF. `website/` is its own workspace.)
- **Rebuild Tailwind after adding/removing classes:** `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify` (Tailwind scans `@source "./src/**/*.rs"`, so new utility classes only appear after a rebuild). `make build-website-css` does the same.
- **Dev server (manual verify):** `make website` (runs `tailwindcss --watch` + `dx serve --platform web`), open the printed `localhost` URL.
- **Format/lint before commit:** `cd website && cargo fmt && cargo clippy --all-targets -- -D warnings`. Fix anything that fails.
- **No code comments** (project rule). **Filename-based modules only** (no `mod.rs`).
- Commit after every task with the message shown.

## File structure (locked in)

- `website/tailwind.input.css` — **modify**: aurora tokens + `@theme` `--animate-*` + `@keyframes`. Keep minimal; no bespoke component classes.
- `website/src/main.rs` — **modify**: drop `Hero`/`Features`/`Footer` + their consts/imports; add `mod landing;`; `Home`/`HomeStatic` render `landing::Landing {}`.
- `website/src/landing.rs` — **create**: `Landing` root, shared consts, `Banner`, `Footer`; declares section submodules.
- `website/src/landing/hero.rs` — **create**: `Hero`.
- `website/src/landing/pillars.rs` — **create**: `Pillars`.
- `website/src/landing/coworking.rs` — **create**: `Coworking`.
- `website/src/landing/scenes.rs` — **create**: `LayoutScene`, `InputScene` (pinned/scrubbed).
- `website/src/landing/platform.rs` — **create**: `Platform`.
- `website/src/landing/cta.rs` — **create**: `Cta` (`id="install"`).
- `website/src/hooks.rs` — **unchanged**; reused via `crate::hooks::{...}`.

`crate::Route` is private in the crate root but Rust makes it visible to descendant modules, so `landing` can reference `crate::Route::DocsIndex {}` without a visibility change.

---

## Task 1: Tailwind theme — aurora tokens, keyframes, animation utilities

**Files:**
- Modify: `website/tailwind.input.css`

- [ ] **Step 1: Add tokens + keyframes to the `@theme` block**

In `website/tailwind.input.css`, replace the existing `@theme { ... }` block with this (adds two aurora colors, the `--animate-*` definitions, and their `@keyframes`; keeps every existing token):

```css
@theme {
    --color-bg: #0a0a0a;
    --color-surface: #151515;
    --color-border: #2a2a2a;
    --color-text: #e0e0e0;
    --color-text-muted: #888;
    --color-accent: #7c8aff;
    --color-accent-hover: #9aa4ff;
    --color-code-bg: #1a1a2e;
    --color-aurora-violet: #c264ff;
    --color-aurora-cyan: #36d6e7;
    --font-sans:
        -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    --font-mono: "SF Mono", "Fira Code", "Cascadia Code", monospace;

    --animate-fade-up: fade-up 0.7s ease-out both;
    --animate-float: float 14s ease-in-out infinite;

    @keyframes fade-up {
        from { opacity: 0; transform: translateY(24px); }
        to { opacity: 1; transform: translateY(0); }
    }
    @keyframes float {
        0%, 100% { transform: translate3d(0, 0, 0); }
        50% { transform: translate3d(0, -24px, 0); }
    }
    @keyframes parallax-up {
        from { transform: translate3d(0, 60px, 0); }
        to { transform: translate3d(0, -60px, 0); }
    }
    @keyframes scene-split {
        from { transform: translate3d(0, 0, 0); opacity: 0.85; }
        to { transform: translate3d(0, 0, 0); opacity: 1; }
    }
}
```

- [ ] **Step 2: Build the CSS**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify`
Expected: completes with no error; `public/style.css` is regenerated.

- [ ] **Step 3: Commit**

```bash
git add website/tailwind.input.css website/public/style.css
git commit -m "feat(website): add aurora tokens and animation keyframes"
```

---

## Task 2: Extract landing into a module (pure refactor, no visual change)

Move the existing `Hero`/`Features`/`Footer` + consts out of `main.rs` into `landing.rs` so `Home`/`HomeStatic` render `landing::Landing {}`. The page must look identical after this task.

**Files:**
- Create: `website/src/landing.rs`
- Modify: `website/src/main.rs`

- [ ] **Step 1: Create `website/src/landing.rs`** with the moved code (verbatim from the current `main.rs`, made `pub`, imports localized):

```rust
mod coworking;
mod cta;
mod hero;
mod pillars;
mod platform;
mod scenes;

use dioxus::prelude::*;

pub const ICON: Asset = asset!("/assets/icon.png");
pub const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
pub const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

#[component]
pub fn Landing() -> Element {
    rsx! {
        Hero {}
        Features {}
        Footer {}
    }
}

#[component]
fn Hero() -> Element {
    use dioxus_primitives::toast::{ToastOptions, use_toast};
    use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};

    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section { class: "text-center max-w-3xl mx-auto pt-16 pb-12 px-6 sm:pt-24 sm:pb-16 sm:px-8",
            img {
                src: ICON,
                alt: "Vmux icon",
                class: "w-32 h-32 mb-6 inline-block rounded-3xl",
            }
            h1 { class: "text-4xl sm:text-5xl font-bold mb-2 tracking-tight", "Vmux" }
            p { class: "text-base sm:text-xl text-text-muted mb-10 max-w-xl mx-auto",
                "Vibe Multiplexer — an agent-first workspace with a browser and IDE built in."
            }
            div { class: "flex flex-wrap justify-center gap-3 mb-6",
                button {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold border border-transparent bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        if is_mac {
                            download(());
                        } else {
                            toast_api
                                .info(
                                    "Not supported".to_string(),
                                    ToastOptions::new()
                                        .description("Windows/Linux not supported yet — see GitHub Releases"),
                                );
                        }
                    },
                    "Download .dmg"
                }
                a {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                    href: GITHUB_URL,
                    target: "_blank",
                    "GitHub"
                }
                Link {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                    to: crate::Route::DocsIndex {},
                    "Docs"
                }
            }
            div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-4",
                code { class: "font-mono text-accent", "{INSTALL_CMD}" }
                button {
                    class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        copy(INSTALL_CMD.to_string());
                        toast_api.success("Copied!".to_string(), ToastOptions::new());
                    },
                    "Copy"
                }
            }
            p { class: "text-sm text-text-muted", "Requires macOS 13.0 (Ventura) or later." }
        }
    }
}

#[component]
fn Features() -> Element {
    let features = [
        (
            "Co-work with agents",
            "People and agents build side by side in one shared space — from hands-on pairing to full autonomy, you set the balance.",
        ),
        (
            "Browser simplicity, tmux power",
            "Looks like the browser you already know; split, stack, and tile panes like tmux underneath.",
        ),
        (
            "IDE power underneath",
            "Keyboard-driven workflows and deep environment control — and agents drive the whole workspace over MCP.",
        ),
        (
            "3D workspace",
            "Powered by Bevy. Flip your panes into a live, GPU-rendered 3D scene — same workspace, still interactive.",
        ),
    ];

    rsx! {
        section { class: "max-w-3xl mx-auto py-12 px-8",
            h2 { class: "text-center text-3xl mb-8", "Features" }
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-5",
                for (title , desc) in features {
                    div { class: "bg-surface border border-border rounded-xl p-6",
                        h3 { class: "text-base mb-2 text-accent", "{title}" }
                        p { class: "text-sm text-text-muted leading-relaxed", "{desc}" }
                    }
                }
            }
        }
    }
}

#[component]
fn Footer() -> Element {
    rsx! {
        footer { class: "text-center py-12 px-8 text-text-muted text-sm",
            p {
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: GITHUB_URL,
                    target: "_blank",
                    "GitHub"
                }
                " · "
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: "https://github.com/vmux-ai/vmux/blob/main/LICENSE",
                    target: "_blank",
                    "MIT License"
                }
            }
        }
    }
}
```

- [ ] **Step 2: Create empty section module files** so `landing.rs`'s `mod` declarations compile. Each file gets one placeholder component that renders nothing (replaced in later tasks):

`website/src/landing/hero.rs`, `pillars.rs`, `coworking.rs`, `scenes.rs`, `platform.rs`, `cta.rs` — for now put this in **each** file (this keeps the crate compiling; later tasks overwrite these):

```rust
use dioxus::prelude::*;

#[allow(dead_code)]
pub fn placeholder() -> Element {
    rsx! {}
}
```

> Note: `landing.rs` above declares the submodules but the real `Hero`/`Features`/`Footer` live inline in `landing.rs` for this task. Later tasks move `Hero` into `landing/hero.rs` etc. and update `landing.rs`'s `Landing` to call them. Keeping them inline now means visual parity is trivial to confirm.

- [ ] **Step 3: Edit `website/src/main.rs`**

Add the module declaration alongside the others (line 1-3 area):

```rust
mod docs;
mod hooks;
mod landing;
mod markdown;
```

Remove these now-unused top-of-file items from `main.rs`:
- `use hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};`
- `const ICON: Asset = asset!("/assets/icon.png");`
- `const GITHUB_URL: &str = "...";`
- `const INSTALL_CMD: &str = "...";`

Change the toast import (keep only what `App` uses):

```rust
use dioxus_primitives::toast::ToastProvider;
```

Delete the `Hero`, `Features`, and `Footer` functions from `main.rs` entirely (they now live in `landing.rs`).

Update both home components:

```rust
#[component]
fn Home() -> Element {
    rsx! {
        landing::Landing {}
    }
}

#[component]
fn HomeStatic() -> Element {
    rsx! {
        landing::Landing {}
    }
}
```

- [ ] **Step 4: Compile**

Run: `cd website && dx build --platform web`
Expected: builds with no errors. If `crate::Route` is unresolved from `landing.rs`, confirm the `Route` enum is defined in `main.rs` (crate root) — descendant modules can see it.

- [ ] **Step 5: Visual parity check**

Run: `make website`, open the URL. The home page must look **identical** to before (same hero, 4 feature cards, footer).

- [ ] **Step 6: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/main.rs website/src/landing.rs website/src/landing
git commit -m "refactor(website): extract landing into its own module"
```

---

## Task 3: Sticky banner + page shell

Add a sticky top banner and wrap the landing in a shell. The banner is always present with a translucent blurred background.

**Files:**
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Add `Banner` and update `Landing` in `landing.rs`**

Add this `Banner` component and replace the `Landing` body. (Sections beyond Hero/Footer are wired in later tasks; for now Landing = Banner + Hero + Features + Footer.)

```rust
#[component]
fn Banner() -> Element {
    rsx! {
        header { class: "sticky top-0 z-50 backdrop-blur-md bg-bg/70 border-b border-border/60",
            nav { class: "max-w-5xl mx-auto flex items-center justify-between px-5 py-3",
                a {
                    class: "flex items-center gap-2 font-bold tracking-tight text-text no-underline hover:text-accent",
                    href: "#top",
                    img { src: ICON, alt: "Vmux", class: "w-6 h-6 rounded-md" }
                    "Vmux"
                }
                div { class: "flex items-center gap-2 sm:gap-3 text-sm",
                    a {
                        class: "no-underline text-text-muted hover:text-text px-2 py-1",
                        href: GITHUB_URL,
                        target: "_blank",
                        "GitHub"
                    }
                    Link {
                        class: "no-underline text-text-muted hover:text-text px-2 py-1",
                        to: crate::Route::DocsIndex {},
                        "Docs"
                    }
                    a {
                        class: "no-underline bg-accent text-black font-semibold rounded-lg px-4 py-1.5 hover:bg-accent-hover",
                        href: "#install",
                        "Install"
                    }
                }
            }
        }
    }
}

#[component]
pub fn Landing() -> Element {
    rsx! {
        div { id: "top",
            Banner {}
            Hero {}
            Features {}
            Footer {}
        }
    }
}
```

- [ ] **Step 2: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 3: Manual check**

`make website` → banner stays pinned on scroll, blur shows over content, `Install` link jumps toward the (future) `#install` anchor (no-op until Task 10), `Docs` routes to `/docs`.

- [ ] **Step 4: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/public/style.css
git commit -m "feat(website): add sticky install banner"
```

---

## Task 4: Hero redesign (aurora bloom, grain, parallax, fade-up)

Move `Hero` into `landing/hero.rs` and rebuild it: aurora bloom backdrop, grain, fade-up reveal, parallax bloom. Keep the existing download/copy behavior.

**Files:**
- Create (overwrite placeholder): `website/src/landing/hero.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Write `website/src/landing/hero.rs`**

```rust
use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};
use crate::landing::{GITHUB_URL, ICON, INSTALL_CMD};

#[component]
pub fn Hero() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section { class: "relative overflow-hidden text-center px-6 pt-24 pb-28 sm:pt-32 sm:pb-36",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-24 h-[28rem] w-[28rem] -translate-x-1/2 rounded-full bg-accent/30 blur-[120px] animate-float supports-[animation-timeline:scroll()]:[animation-timeline:scroll()] motion-reduce:animate-none" }
                div { class: "absolute left-[20%] top-40 h-72 w-72 rounded-full bg-aurora-violet/25 blur-[100px] animate-float [animation-delay:-4s] motion-reduce:animate-none" }
                div { class: "absolute right-[18%] top-32 h-72 w-72 rounded-full bg-aurora-cyan/20 blur-[100px] animate-float [animation-delay:-8s] motion-reduce:animate-none" }
            }
            div { class: "relative mx-auto max-w-3xl animate-fade-up motion-reduce:animate-none",
                img {
                    src: ICON,
                    alt: "Vmux icon",
                    class: "w-24 h-24 sm:w-28 sm:h-28 mb-6 inline-block rounded-3xl shadow-2xl shadow-accent/20",
                }
                h1 { class: "text-5xl sm:text-7xl font-bold tracking-tight mb-4",
                    "Vmux"
                }
                p { class: "text-lg sm:text-2xl text-text mb-3 max-w-2xl mx-auto",
                    "The workspace that bridges chat and IDE."
                }
                p { class: "text-base sm:text-lg text-text-muted mb-10 max-w-xl mx-auto",
                    "An agent-first workspace with a browser and IDE built in — co-work with agents in one shared space."
                }
                div { class: "flex flex-wrap justify-center gap-3 mb-6",
                    button {
                        class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold border border-transparent bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
                        onclick: move |_| {
                            if is_mac {
                                download(());
                            } else {
                                toast_api
                                    .info(
                                        "Not supported".to_string(),
                                        ToastOptions::new()
                                            .description("Windows/Linux not supported yet — see GitHub Releases"),
                                    );
                            }
                        },
                        "Download .dmg"
                    }
                    a {
                        class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                        href: GITHUB_URL,
                        target: "_blank",
                        "GitHub"
                    }
                }
                div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg/80 backdrop-blur border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-4",
                    code { class: "font-mono text-accent", "{INSTALL_CMD}" }
                    button {
                        class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                        onclick: move |_| {
                            copy(INSTALL_CMD.to_string());
                            toast_api.success("Copied!".to_string(), ToastOptions::new());
                        },
                        "Copy"
                    }
                }
                p { class: "text-sm text-text-muted", "Requires macOS 13.0 (Ventura) or later." }
            }
        }
    }
}
```

- [ ] **Step 2: Update `landing.rs`** — delete the inline `Hero` fn; bring the module one into scope; use it in `Landing`.

Add near the top of `landing.rs` (after the `use dioxus::prelude::*;`):

```rust
use hero::Hero;
```

Delete the entire inline `fn Hero()` previously in `landing.rs`. `Landing` already calls `Hero {}`.

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check**

`make website` → hero shows aurora blooms (periwinkle/violet/cyan), content fades up on load, download/copy still work, blooms drift (float) and parallax-shift on scroll in Chrome/Safari 26+.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/hero.rs website/public/style.css
git commit -m "feat(website): redesign hero with aurora bloom and parallax"
```

---

## Task 5: Three Pillars section

Replace the old `Features` grid with the experience.md pillars: Co-working, Known by heart, IDE power. Glass cards, staggered fade-up.

**Files:**
- Create (overwrite placeholder): `website/src/landing/pillars.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Write `website/src/landing/pillars.rs`**

```rust
use dioxus::prelude::*;

struct Pillar {
    title: &'static str,
    body: &'static str,
}

const PILLARS: &[Pillar] = &[
    Pillar {
        title: "Co-working",
        body: "People and agents work in one shared space — from hands-on pairing to full autonomy. Watch a run and grab the keyboard, or turn agents loose.",
    },
    Pillar {
        title: "Known by heart",
        body: "It looks and acts like a standard web browser. No learning curve — everyone already knows how to use it.",
    },
    Pillar {
        title: "IDE power",
        body: "Beneath the surface: advanced tools, keyboard-driven workflows, and deep environment control for when you want it.",
    },
];

#[component]
pub fn Pillars() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            p { class: "text-center text-sm uppercase tracking-[0.2em] text-accent mb-3",
                "Two worlds, one workspace"
            }
            h2 { class: "text-center text-3xl sm:text-4xl font-bold tracking-tight mb-14 max-w-2xl mx-auto",
                "Vmux bridges chat-first tools and developer IDEs."
            }
            div { class: "grid grid-cols-1 md:grid-cols-3 gap-5",
                for (i , p) in PILLARS.iter().enumerate() {
                    div {
                        class: "rounded-2xl border border-white/10 bg-white/5 backdrop-blur p-7 animate-fade-up supports-[animation-timeline:view()]:[animation-timeline:view()] supports-[animation-timeline:view()]:[animation-range:entry_0%_cover_35%] motion-reduce:animate-none",
                        style: "animation-delay: {i * 120}ms",
                        h3 { class: "text-lg font-semibold text-accent mb-2", "{p.title}" }
                        p { class: "text-sm text-text-muted leading-relaxed", "{p.body}" }
                    }
                }
            }
        }
    }
}
```

> Note on `animation-delay` + scroll timelines: when `animation-timeline: view()` engages, `animation-delay` (time-based) is ignored by the browser, so the stagger applies on the initial load/non-supporting path and the scroll-driven path simply reveals per-card as it enters. This is acceptable and needs no extra code.

- [ ] **Step 2: Wire into `landing.rs`** — delete the inline `fn Features()`, import and use `Pillars`.

Add to imports in `landing.rs`:

```rust
use pillars::Pillars;
```

Replace the `Landing` body so it uses `Pillars` instead of `Features`:

```rust
#[component]
pub fn Landing() -> Element {
    rsx! {
        div { id: "top",
            Banner {}
            Hero {}
            Pillars {}
            Footer {}
        }
    }
}
```

Delete the inline `fn Features()` from `landing.rs`.

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check**

`make website` → three glass pillar cards reveal as they scroll into view; readable with motion reduced.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/pillars.rs website/public/style.css
git commit -m "feat(website): add three-pillars section"
```

---

## Task 6: Co-working section

A wider narrative beat on human+agent collaboration with a pairing⇄autonomy visual.

**Files:**
- Create (overwrite placeholder): `website/src/landing/coworking.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Write `website/src/landing/coworking.rs`**

```rust
use dioxus::prelude::*;

#[component]
pub fn Coworking() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-10 items-center",
                div { class: "animate-fade-up supports-[animation-timeline:view()]:[animation-timeline:view()] supports-[animation-timeline:view()]:[animation-range:entry_0%_cover_40%] motion-reduce:animate-none",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Co-working" }
                    h2 { class: "text-3xl sm:text-4xl font-bold tracking-tight mb-4",
                        "Build alongside your agents."
                    }
                    p { class: "text-text-muted leading-relaxed mb-4",
                        "People and agents work, build, and orchestrate tasks side by side, in real time, in one shared space."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "Find your own balance — and let it shift as you trust agents more."
                    }
                }
                div { class: "rounded-2xl border border-white/10 bg-white/5 backdrop-blur p-8",
                    div { class: "flex items-center justify-between text-xs text-text-muted mb-3",
                        span { "Hands-on pairing" }
                        span { "Full autonomy" }
                    }
                    div { class: "relative h-2 rounded-full bg-border overflow-hidden",
                        div { class: "absolute inset-y-0 left-0 w-1/2 rounded-full bg-gradient-to-r from-accent to-aurora-violet" }
                        div { class: "absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 h-5 w-5 rounded-full bg-accent shadow-lg shadow-accent/40 animate-float motion-reduce:animate-none" }
                    }
                    p { class: "mt-4 text-sm text-text-muted",
                        "Watch a run and grab the keyboard to steer, or turn agents loose in their own space."
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Wire into `landing.rs`**

Add import:

```rust
use coworking::Coworking;
```

Update `Landing` body order: `Banner`, `Hero`, `Pillars`, `Coworking`, `Footer`.

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check** — `make website` → section reveals on scroll; slider handle drifts.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/coworking.rs website/public/style.css
git commit -m "feat(website): add co-working section"
```

---

## Task 7: Pinned scene — Layout (browser → tiled panes)

The first scrubbed pinned scene. A tall track holds a sticky stage; as you scroll, a single browser pane resolves into a tiled browser+terminal layout. Uses a named CSS `scroll-timeline` so the child animations scrub against the track.

**Files:**
- Create (overwrite placeholder): `website/src/landing/scenes.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Write `website/src/landing/scenes.rs` with `LayoutScene`**

```rust
use dioxus::prelude::*;

#[component]
pub fn LayoutScene() -> Element {
    rsx! {
        section { class: "relative min-h-[280vh] [scroll-timeline-name:--layout] [scroll-timeline-axis:block]",
            div { class: "sticky top-0 h-screen flex flex-col items-center justify-center px-6 overflow-hidden",
                div { class: "max-w-2xl text-center mb-10",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Layout" }
                    h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                        "Browser simplicity, tmux power."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "At first glance it's the browser you expect. Underneath sits a malleable, tmux-inspired UI — split, stack, and tile any layout you imagine."
                    }
                }
                div { class: "relative w-full max-w-4xl aspect-video rounded-xl border border-border bg-surface/80 backdrop-blur overflow-hidden shadow-2xl",
                    div { class: "flex h-8 items-center gap-1.5 border-b border-border px-3",
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                    }
                    div { class: "flex h-[calc(100%-2rem)] gap-1 p-1",
                        div { class: "flex-1 rounded-md bg-aurora-cyan/10 border border-aurora-cyan/20" }
                        div {
                            class: "flex flex-col gap-1 overflow-hidden [animation:scene-split_linear_both] [animation-timeline:--layout] [animation-range:entry_30%_cover_60%] supports-[animation-timeline:scroll()]:basis-1/2 basis-1/2 motion-reduce:basis-1/2",
                            div { class: "flex-1 rounded-md bg-accent/10 border border-accent/20" }
                            div { class: "flex-1 rounded-md bg-aurora-violet/10 border border-aurora-violet/20 font-mono text-[10px] text-text-muted p-2",
                                "$ vmux split" }
                        }
                    }
                }
                p { class: "mt-6 text-sm text-text-muted",
                    "Flip the same panes into a live 3D scene, still interactive."
                }
            }
        }
    }
}
```

> The scrub here is intentionally simple/robust: the right column carries a named-timeline animation so it visibly resolves as the track scrolls; the layout's default (no-JS / reduced-motion / unsupported) state is the finished split (`basis-1/2`). Tune `animation-range` values in the browser.

- [ ] **Step 2: Wire into `landing.rs`**

Add import:

```rust
use scenes::LayoutScene;
```

Update `Landing` order: `Banner`, `Hero`, `Pillars`, `Coworking`, `LayoutScene`, `Footer`.

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check** — `make website` → scrolling through the tall section keeps the mock pinned while the panes resolve; with motion reduced the finished split shows statically.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/scenes.rs website/public/style.css
git commit -m "feat(website): add pinned layout scene"
```

---

## Task 8: Pinned scene — Input (talk → type → click)

Second scrubbed scene, added to `scenes.rs`. The interaction priority stack builds as you scroll.

**Files:**
- Modify: `website/src/landing/scenes.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Append `InputScene` to `website/src/landing/scenes.rs`**

```rust
struct Tier {
    rank: &'static str,
    title: &'static str,
    body: &'static str,
    range: &'static str,
}

const TIERS: &[Tier] = &[
    Tier {
        rank: "01",
        title: "Talk or type",
        body: "Direct the whole workspace in natural language. Type for precision, talk for hands-free speed.",
        range: "entry 10% cover 35%",
    },
    Tier {
        rank: "02",
        title: "Keyboard shortcuts",
        body: "Chrome-style for browsing, tmux-style <leader> commands for layout. High velocity, near-zero learning curve.",
        range: "entry 30% cover 55%",
    },
    Tier {
        rank: "03",
        title: "Mouse",
        body: "Plain, intuitive point-and-click that keeps everything grounded in predictable browser behavior.",
        range: "entry 50% cover 75%",
    },
];

#[component]
pub fn InputScene() -> Element {
    rsx! {
        section { class: "relative min-h-[280vh] [scroll-timeline-name:--input] [scroll-timeline-axis:block]",
            div { class: "sticky top-0 h-screen flex flex-col items-center justify-center px-6",
                div { class: "max-w-2xl text-center mb-10",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Input" }
                    h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                        "Talk, type, click."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "Interaction ordered from abstract delegation down to mechanical control."
                    }
                }
                div { class: "w-full max-w-xl flex flex-col gap-4",
                    for t in TIERS.iter() {
                        div {
                            class: "flex items-start gap-4 rounded-xl border border-white/10 bg-white/5 backdrop-blur p-5 [animation:fade-up_linear_both] [animation-timeline:--input] motion-reduce:animate-none supports-[animation-timeline:scroll()]:opacity-100",
                            style: "animation-range: {t.range}",
                            span { class: "text-accent font-mono text-sm pt-0.5", "{t.rank}" }
                            div {
                                h3 { class: "font-semibold mb-1", "{t.title}" }
                                p { class: "text-sm text-text-muted leading-relaxed", "{t.body}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Wire into `landing.rs`**

Update the import:

```rust
use scenes::{InputScene, LayoutScene};
```

Update `Landing` order: `Banner`, `Hero`, `Pillars`, `Coworking`, `LayoutScene`, `InputScene`, `Footer`.

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check** — `make website` → tiers reveal in sequence while pinned; readable statically with motion reduced.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/scenes.rs website/public/style.css
git commit -m "feat(website): add pinned input scene"
```

---

## Task 9: Platform section

"More OS than app" — device layers drifting at different parallax depths.

**Files:**
- Create (overwrite placeholder): `website/src/landing/platform.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Write `website/src/landing/platform.rs`**

```rust
use dioxus::prelude::*;

#[component]
pub fn Platform() -> Element {
    rsx! {
        section { class: "relative overflow-hidden max-w-5xl mx-auto px-6 py-24 sm:py-32 text-center",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-80 w-80 -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-violet/15 blur-[120px]" }
            }
            p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Platform" }
            h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4 max-w-2xl mx-auto",
                "More OS than app."
            }
            p { class: "text-text-muted leading-relaxed max-w-2xl mx-auto mb-14",
                "An OS-like layer for everything you do — the same workspace and agents, reshaped to the device in front of you."
            }
            div { class: "flex items-end justify-center gap-4 sm:gap-8",
                div { class: "h-40 w-56 rounded-xl border border-white/10 bg-white/5 backdrop-blur animate-float motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "Desktop" } }
                div { class: "h-52 w-32 rounded-2xl border border-white/10 bg-white/5 backdrop-blur animate-float [animation-delay:-5s] motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "Phone" } }
                div { class: "h-36 w-44 rounded-xl border border-white/10 bg-white/5 backdrop-blur animate-float [animation-delay:-9s] motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "AR / VR" } }
            }
            p { class: "mt-12 text-sm text-text-muted",
                "Today it runs on macOS (lead) and Linux — with a portable core ready to follow."
            }
        }
    }
}
```

- [ ] **Step 2: Wire into `landing.rs`** — add `use platform::Platform;`; insert `Platform {}` after `InputScene {}`.

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check** — devices drift at different phases; section readable static.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/platform.rs website/public/style.css
git commit -m "feat(website): add platform section"
```

---

## Task 10: CTA section (`#install`)

Large final install block; the banner `Install` link anchors here.

**Files:**
- Create (overwrite placeholder): `website/src/landing/cta.rs`
- Modify: `website/src/landing.rs`

- [ ] **Step 1: Write `website/src/landing/cta.rs`**

```rust
use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};
use crate::landing::INSTALL_CMD;

#[component]
pub fn Cta() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section {
            id: "install",
            class: "relative overflow-hidden scroll-mt-20 px-6 py-28 sm:py-36 text-center",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-[26rem] w-[26rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/25 blur-[120px]" }
            }
            h2 { class: "text-4xl sm:text-6xl font-bold tracking-tight mb-6",
                "Install Vmux."
            }
            div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg/80 backdrop-blur border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-6",
                code { class: "font-mono text-accent", "{INSTALL_CMD}" }
                button {
                    class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        copy(INSTALL_CMD.to_string());
                        toast_api.success("Copied!".to_string(), ToastOptions::new());
                    },
                    "Copy"
                }
            }
            div { class: "flex justify-center",
                button {
                    class: "inline-flex items-center px-7 py-3.5 rounded-lg text-base font-semibold border border-transparent bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        if is_mac {
                            download(());
                        } else {
                            toast_api
                                .info(
                                    "Not supported".to_string(),
                                    ToastOptions::new()
                                        .description("Windows/Linux not supported yet — see GitHub Releases"),
                                );
                        }
                    },
                    "Download .dmg"
                }
            }
            p { class: "mt-5 text-sm text-text-muted", "Requires macOS 13.0 (Ventura) or later." }
        }
    }
}
```

- [ ] **Step 2: Wire into `landing.rs`** — add `use cta::Cta;`; insert `Cta {}` after `Platform {}` and before `Footer {}`.

Final `Landing`:

```rust
#[component]
pub fn Landing() -> Element {
    rsx! {
        div { id: "top",
            Banner {}
            Hero {}
            Pillars {}
            Coworking {}
            LayoutScene {}
            InputScene {}
            Platform {}
            Cta {}
            Footer {}
        }
    }
}
```

- [ ] **Step 3: Build CSS + compile**

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web`
Expected: no errors.

- [ ] **Step 4: Manual check** — banner `Install` smooth-jumps to the CTA (`scroll-mt-20` keeps it clear of the sticky banner); curl copy + `.dmg` work.

- [ ] **Step 5: Format, lint, commit**

```bash
cd website && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add website/src/landing.rs website/src/landing/cta.rs website/public/style.css
git commit -m "feat(website): add final install CTA section"
```

---

## Task 11: Reduced-motion + cross-browser polish pass

Audit every animated element for a sane fallback and tune ranges.

**Files:**
- Modify: any `website/src/landing/*.rs` needing fixes

- [ ] **Step 1: Audit** — grep for animation classes and confirm each has both a static default and a `motion-reduce:animate-none` (or `[animation-timeline:none]`) guard:

Run: `cd website && grep -rn "animation-timeline\|animate-\|animation:" src/landing`
Expected: every match sits on an element whose un-animated state is already the finished layout.

- [ ] **Step 2: Manual cross-browser check**
  - Chrome: full scrub + parallax + reveals.
  - Safari 26+: same (named timelines + `view()` supported).
  - Firefox (no flag): page fully readable, static — no broken/blank sections.
  - DevTools → emulate `prefers-reduced-motion: reduce`: no motion, everything legible.
  - Mobile width (≤640px): sections stack, no horizontal scroll, pinned scenes still legible.

- [ ] **Step 3: Fix any issues found**, rebuild CSS, recompile.

Run: `cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify && dx build --platform web && cargo fmt && cargo clippy --all-targets -- -D warnings`

- [ ] **Step 4: Commit**

```bash
git add website/src/landing website/public/style.css
git commit -m "polish(website): reduced-motion guards and cross-browser tuning"
```

---

## Task 12: Final verification + cleanup

- [ ] **Step 1: Production build (SSG) succeeds**

Run: `make build-website-release`
Expected: completes; `website/target/dx/vmux_website/release/web/public/index.html` exists and contains the landing markup (verifies SSR/SSG renders the new sections).

- [ ] **Step 2: Full golden-path walk** (`make website`): top→bottom scroll — banner pinned, hero blooms, pillars/co-working reveal, both pinned scenes scrub, platform drifts, CTA install works, footer links work. Confirm in Chrome **and** Safari. (User to runtime-test; do not claim success without it.)

- [ ] **Step 3: Remove the empty placeholder leftovers** — confirm no `placeholder()` stubs remain in `website/src/landing/*.rs` (every section file should now export its real component). Grep:

Run: `cd website && grep -rn "fn placeholder" src/landing`
Expected: no output.

- [ ] **Step 4: Delete this plan file** (project rule: remove the plan once implemented).

```bash
git rm docs/plans/2026-06-21-landing-redesign.md
git commit -m "chore: remove implemented landing redesign plan"
```

- [ ] **Step 5: Open PR** — use the open-new-pr skill / `gh pr create` to `main`. Do not use the `-w` web form.

---

## Self-review notes

- **Spec coverage:** Banner ✓(T3), Hero ✓(T4), Pillars ✓(T5), Co-working ✓(T6), Layout pinned ✓(T7), Input pinned ✓(T8), Platform ✓(T9), CTA `#install` ✓(T10), Footer ✓(T2), aurora tokens/keyframes ✓(T1), Tailwind-first/`supports-`/`motion-reduce` ✓(T1,T4–T11), reduced-motion ✓(T11), SSG render ✓(T12), worktree/PR ✓(T12).
- **No fabricated tests:** intentional — no website test harness; verification is compile + CSS build + manual, per spec.
- **Type/name consistency:** component names (`Landing`, `Banner`, `Hero`, `Pillars`, `Coworking`, `LayoutScene`, `InputScene`, `Platform`, `Cta`, `Footer`) and consts (`ICON`, `GITHUB_URL`, `INSTALL_CMD`) are used consistently across tasks; imports updated in lockstep in `landing.rs`.
