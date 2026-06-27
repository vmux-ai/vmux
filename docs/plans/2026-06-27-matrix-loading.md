# Matrix Terminal Agent Loading Screen — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the agent loading skeleton with a Matrix-style digital-rain canvas recolored per agent brand, behind a soft-glass boot console showing the agent name.

**Architecture:** Add a raw-RGB accent to `AgentAccent`. New wasm-only `MatrixRain` Dioxus component draws falling katakana/digit columns (agent name woven in) on a `<canvas>` via `web-sys` + `requestAnimationFrame`, cleaned up with `use_drop`. The `page.rs` loading branch renders `MatrixRain` behind a centered console. Rain is bounded to the boot window (cleared on alt-screen / 10s timeout → unmount → loop stops).

**Tech Stack:** Rust, Dioxus 0.7.4 (wasm), `web-sys` 0.3.98 / `js-sys` 0.3.98, Tailwind v4.

**Work in worktree:** `.worktrees/matrix-loading` (branch `feat/matrix-loading`). All paths below are repo-relative; edit the worktree copies.

---

## File Structure

- `crates/vmux_ui/src/agent_accent.rs` — add `rain_rgb` field + values + tests. (modify)
- `crates/vmux_terminal/Cargo.toml` — add 3 web-sys features. (modify)
- `crates/vmux_terminal/src/matrix_rain.rs` — **new** wasm-only rain component.
- `crates/vmux_terminal/src/lib.rs` — declare `matrix_rain` module (wasm-gated). (modify)
- `crates/vmux_terminal/src/page.rs` — rewrite loading branch + import. (modify)
- `crates/vmux_terminal/src/plugin.rs` — add one source-scrape test. (modify, test only)

---

## Task 1: Add `rain_rgb` accent (TDD)

**Files:**
- Modify/Test: `crates/vmux_ui/src/agent_accent.rs`

- [ ] **Step 1: Update the three tests to assert `rain_rgb`** (they will fail to compile — field missing)

In the `#[cfg(test)] mod tests`, add an assert to each existing test:

```rust
    #[test]
    fn claude_uses_rose_orange() {
        let a = agent_accent("claude");
        assert_eq!(a.grad, "from-orange-400 to-rose-500");
        assert_eq!(a.accent_text, "text-rose-400");
        assert_eq!(a.accent_bg, "bg-rose-400");
        assert_eq!(a.rain_rgb, "251 113 133");
    }

    #[test]
    fn codex_uses_emerald_teal() {
        let a = agent_accent("codex");
        assert_eq!(a.grad, "from-emerald-500 to-teal-600");
        assert_eq!(a.accent_text, "text-emerald-400");
        assert_eq!(a.rain_rgb, "52 211 153");
    }

    #[test]
    fn unknown_falls_back_to_vibe_amber() {
        let a = agent_accent("nope");
        assert_eq!(a.grad, "from-orange-500 to-amber-600");
        assert_eq!(a.grad, agent_accent("vibe").grad);
        assert_eq!(a.rain_rgb, "251 146 60");
    }
```

- [ ] **Step 2: Run tests to verify they fail (compile error: no field `rain_rgb`)**

Run: `cargo test -p vmux_ui --lib agent_accent`
Expected: FAIL — `error[E0609]: no field 'rain_rgb' on type 'AgentAccent'`

- [ ] **Step 3: Add the field to the struct and all three arms**

Add to the struct (after `cta_shadow`):

```rust
pub struct AgentAccent {
    pub glow_top: &'static str,
    pub glow_bottom: &'static str,
    pub grad: &'static str,
    pub accent_text: &'static str,
    pub accent_bg: &'static str,
    pub cta_shadow: &'static str,
    pub rain_rgb: &'static str,
}
```

Add `rain_rgb` to each arm — claude: `rain_rgb: "251 113 133",` · codex: `rain_rgb: "52 211 153",` · `_`: `rain_rgb: "251 146 60",` (place after each `cta_shadow` line).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_ui --lib agent_accent`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
cd .worktrees/matrix-loading
git add crates/vmux_ui/src/agent_accent.rs
git commit -m "feat(ui): add rain_rgb to AgentAccent"
```

---

## Task 2: Add web-sys features for canvas

**Files:**
- Modify: `crates/vmux_terminal/Cargo.toml:48-53`

- [ ] **Step 1: Extend the web-sys feature list**

Replace the `web-sys` dependency block (the `[target.'cfg(target_arch = "wasm32")'.dependencies]` one) with:

```toml
web-sys = { version = "0.3", features = [
    "Window", "Document", "Element", "HtmlElement", "Node",
    "DomRect", "CssStyleDeclaration",
    "ResizeObserver", "ResizeObserverEntry",
    "MouseEvent", "WheelEvent",
    "HtmlCanvasElement", "CanvasRenderingContext2d", "MediaQueryList",
] }
```

- [ ] **Step 2: Verify it resolves**

Run: `cargo metadata --no-deps -q >/dev/null && echo OK`
Expected: `OK` (no manifest error)

- [ ] **Step 3: Commit**

```bash
cd .worktrees/matrix-loading
git add crates/vmux_terminal/Cargo.toml
git commit -m "build(terminal): enable web-sys canvas features"
```

---

## Task 3: Create the `MatrixRain` component

**Files:**
- Create: `crates/vmux_terminal/src/matrix_rain.rs`
- Modify: `crates/vmux_terminal/src/lib.rs:10-11`

- [ ] **Step 1: Create `matrix_rain.rs` with the full component**

```rust
//! Matrix-style digital rain canvas rendered behind the agent loading console.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const FONT_PX: f64 = 16.0;
const GLYPHS: &str = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789";

/// Full-bleed Matrix rain. `accent_rgb` is a `"r g b"` triple (from
/// `AgentAccent::rain_rgb`); `words` are uppercased agent tokens woven into a few
/// columns so the agent name stays legible in the rain.
#[component]
pub fn MatrixRain(accent_rgb: String, words: Vec<String>) -> Element {
    let canvas_id =
        use_hook(|| format!("matrix-rain-{}", (js_sys::Math::random() * 1.0e9) as u64));
    let running: Rc<RefCell<bool>> = use_hook(|| Rc::new(RefCell::new(true)));
    let raf: Rc<RefCell<Option<Closure<dyn FnMut()>>>> =
        use_hook(|| Rc::new(RefCell::new(None)));

    use_effect({
        let canvas_id = canvas_id.clone();
        let accent_rgb = accent_rgb.clone();
        let words = words.clone();
        let running = running.clone();
        let raf = raf.clone();
        move || {
            start_rain(
                canvas_id.clone(),
                accent_rgb.clone(),
                words.clone(),
                running.clone(),
                raf.clone(),
            );
        }
    });

    use_drop({
        let running = running.clone();
        let raf = raf.clone();
        move || {
            *running.borrow_mut() = false;
            *raf.borrow_mut() = None;
        }
    });

    rsx! {
        canvas { id: "{canvas_id}", class: "absolute inset-0 h-full w-full" }
    }
}

fn brighten(accent_rgb: &str) -> String {
    let parts: Vec<u16> = accent_rgb
        .split_whitespace()
        .filter_map(|p| p.parse::<u16>().ok())
        .collect();
    if parts.len() != 3 {
        return "rgb(220 230 255)".to_string();
    }
    let mix = |c: u16| -> u16 { c + (255 - c) * 7 / 10 };
    format!("rgb({} {} {})", mix(parts[0]), mix(parts[1]), mix(parts[2]))
}

fn pick_glyph(glyphs: &[char], words: &[Vec<char>], col: usize, head_row: f64) -> char {
    if !words.is_empty() && col % 7 == 3 {
        let word = &words[col % words.len()];
        if !word.is_empty() {
            let idx = (head_row.max(0.0) as usize) % word.len();
            return word[idx];
        }
    }
    let r = (js_sys::Math::random() * glyphs.len() as f64) as usize;
    glyphs[r.min(glyphs.len() - 1)]
}

fn start_rain(
    canvas_id: String,
    accent_rgb: String,
    words: Vec<String>,
    running: Rc<RefCell<bool>>,
    raf: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(el) = document.get_element_by_id(&canvas_id) else {
        return;
    };
    let Ok(canvas) = el.dyn_into::<web_sys::HtmlCanvasElement>() else {
        return;
    };
    let Ok(Some(ctx_obj)) = canvas.get_context("2d") else {
        return;
    };
    let Ok(ctx) = ctx_obj.dyn_into::<web_sys::CanvasRenderingContext2d>() else {
        return;
    };

    let dpr = window.device_pixel_ratio().max(1.0);

    let reduced = window
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .map(|m| m.matches())
        .unwrap_or(false);

    if reduced {
        let w = canvas.client_width().max(1) as f64;
        let h = canvas.client_height().max(1) as f64;
        canvas.set_width((w * dpr) as u32);
        canvas.set_height((h * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);
        ctx.set_fill_style_str("rgb(30 30 46)");
        ctx.fill_rect(0.0, 0.0, w, h);
        return;
    }

    let glyphs: Vec<char> = GLYPHS.chars().collect();
    let word_chars: Vec<Vec<char>> = words
        .iter()
        .filter(|w| !w.is_empty())
        .map(|w| w.chars().collect())
        .collect();
    let head_color = brighten(&accent_rgb);
    let trail_color = format!("rgb({accent_rgb} / 0.85)");

    let mut cols = (canvas.client_width().max(1) as f64 / FONT_PX).floor().max(1.0) as usize;
    let mut drops: Vec<f64> = (0..cols).map(|_| -(js_sys::Math::random() * 40.0)).collect();

    let win = window.clone();
    let raf_inner = raf.clone();
    let running_inner = running.clone();
    let closure = Closure::wrap(Box::new(move || {
        let w = canvas.client_width().max(1) as f64;
        let h = canvas.client_height().max(1) as f64;
        let want_w = (w * dpr) as u32;
        let want_h = (h * dpr) as u32;
        if canvas.width() != want_w || canvas.height() != want_h {
            canvas.set_width(want_w);
            canvas.set_height(want_h);
            let _ = ctx.reset_transform();
            let _ = ctx.scale(dpr, dpr);
            let new_cols = (w / FONT_PX).floor().max(1.0) as usize;
            if new_cols != cols {
                drops.resize_with(new_cols, || -(js_sys::Math::random() * 40.0));
                cols = new_cols;
            }
        }

        ctx.set_font(&format!("{FONT_PX}px monospace"));
        ctx.set_text_baseline("top");

        ctx.set_fill_style_str("rgba(30, 30, 46, 0.08)");
        ctx.fill_rect(0.0, 0.0, w, h);

        for i in 0..cols {
            let x = i as f64 * FONT_PX;
            let head_row = drops[i];
            let y = head_row * FONT_PX;
            if y >= 0.0 {
                let ch = pick_glyph(&glyphs, &word_chars, i, head_row).to_string();
                ctx.set_fill_style_str(&trail_color);
                let _ = ctx.fill_text(&ch, x, y);
                ctx.set_fill_style_str(&head_color);
                let _ = ctx.fill_text(&ch, x, y);
            }
            if y > h && js_sys::Math::random() > 0.975 {
                drops[i] = 0.0;
            } else {
                drops[i] += 1.0;
            }
        }

        if *running_inner.borrow()
            && let Some(cb) = raf_inner.borrow().as_ref()
        {
            let _ = win.request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }) as Box<dyn FnMut()>);

    *raf.borrow_mut() = Some(closure);
    if let Some(cb) = raf.borrow().as_ref() {
        let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
    }
}
```

- [ ] **Step 2: Declare the module (wasm-gated) in `lib.rs`**

After `#[cfg(target_arch = "wasm32")] pub mod page;` (lines 10–11), add:

```rust
#[cfg(target_arch = "wasm32")]
pub mod matrix_rain;
```

- [ ] **Step 3: Typecheck wasm**

Run: `cargo check -p vmux_terminal --target wasm32-unknown-unknown`
Expected: compiles (warnings ok). If `let ... && let` chain errors on edition, split into nested `if let`.

- [ ] **Step 4: Commit**

```bash
cd .worktrees/matrix-loading
git add crates/vmux_terminal/src/matrix_rain.rs crates/vmux_terminal/src/lib.rs
git commit -m "feat(terminal): add MatrixRain digital-rain component"
```

---

## Task 4: Wire rain into the loading branch

**Files:**
- Modify: `crates/vmux_terminal/src/page.rs` (import near line 12; loading branch ~300–361)

- [ ] **Step 1: Add the import**

After `use crate::event::*;` (line 3) add:

```rust
use crate::matrix_rain::MatrixRain;
```

- [ ] **Step 2: Replace the loading branch**

Replace the whole `{ let state = loading.read().clone(); state.map(|(label, segment)| { ... }) }` block (the one rendering `accent.glow_top` / `"starting…"`) with:

```rust
            {
                let state = loading.read().clone();
                state.map(|(label, segment)| {
                    let accent = agent_accent(&segment);
                    let favicon_url = format!("vmux://agent/{segment}/cli/");
                    let words = vec![label.to_uppercase()];
                    rsx! {
                        div {
                            class: "pointer-events-none absolute inset-0 z-40 overflow-hidden bg-term-bg",
                            MatrixRain { accent_rgb: accent.rain_rgb.to_string(), words }
                            div {
                                class: "relative z-10 flex h-full w-full items-center justify-center",
                                div {
                                    class: "flex items-center gap-3 rounded-2xl bg-black/40 px-5 py-4 ring-1 ring-inset ring-white/10 backdrop-blur-md",
                                    div {
                                        class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-white/[0.06] ring-1 ring-inset ring-white/10",
                                        Favicon {
                                            favicon_url: "".to_string(),
                                            url: favicon_url.clone(),
                                            class: "h-5 w-5 shrink-0 rounded object-contain".to_string(),
                                            globe_class: "h-5 w-5 text-muted-foreground".to_string(),
                                        }
                                    }
                                    div {
                                        div { class: "text-sm font-semibold {accent.accent_text}", "{label}" }
                                        div {
                                            class: "flex items-center gap-1.5 text-xs text-muted-foreground",
                                            span { class: "font-mono", "> booting" }
                                            span { class: "inline-block h-3.5 w-2 animate-pulse {accent.accent_bg}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            }
```

- [ ] **Step 3: Typecheck wasm**

Run: `cargo check -p vmux_terminal --target wasm32-unknown-unknown`
Expected: compiles. (`agent_accent` and `Favicon` already imported; `MatrixRain` now imported.)

- [ ] **Step 4: Commit**

```bash
cd .worktrees/matrix-loading
git add crates/vmux_terminal/src/page.rs
git commit -m "feat(terminal): matrix-rain agent loading screen"
```

---

## Task 5: Lock the wiring with a source-scrape test

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs` (the `#[cfg(test)] mod tests`, near the existing `terminal_page_*` include_str tests ~3598)

- [ ] **Step 1: Add the test**

```rust
    #[test]
    fn agent_loading_uses_matrix_rain() {
        let page = include_str!("page.rs");
        assert!(page.contains("MatrixRain {"));
        assert!(page.contains("accent.rain_rgb"));

        let rain = include_str!("matrix_rain.rs");
        assert!(rain.contains("request_animation_frame"));
        assert!(rain.contains("use_drop"));
        assert!(rain.contains("prefers-reduced-motion"));
    }
```

> `include_str!("matrix_rain.rs")` resolves relative to `plugin.rs` (same `src/` dir) and embeds the file at compile time on all targets, so this native test compiles even though the module itself is wasm-only.

- [ ] **Step 2: Run the test**

Run: `cargo test -p vmux_terminal --lib agent_loading_uses_matrix_rain`
Expected: PASS. (First build is heavy — pulls bevy/cef.)

- [ ] **Step 3: Commit**

```bash
cd .worktrees/matrix-loading
git add crates/vmux_terminal/src/plugin.rs
git commit -m "test(terminal): assert matrix-rain loading wiring"
```

---

## Task 6: Final verification

- [ ] **Step 1: Format + lint the touched crates**

Run: `cargo fmt -p vmux_ui -p vmux_terminal` then `git checkout -- patches/` (fmt also rewrites vendored patches — discard those, keep only `crates/`).
Run: `cargo clippy -p vmux_terminal --target wasm32-unknown-unknown -- -D warnings`
Expected: clean.

- [ ] **Step 2: Native tests for touched crates**

Run: `cargo test -p vmux_ui --lib agent_accent` and `cargo test -p vmux_terminal --lib agent_loading_uses_matrix_rain`
Expected: PASS.

- [ ] **Step 3: Runtime test (user)**

Build/run vmux, open a terminal pane, launch each agent (Claude, Codex, Vibe). Confirm:
- Digital rain fills the loading screen, recolored to the agent brand (rose/orange, emerald/teal, orange/amber).
- The agent name is woven into the rain and shown in the centered soft-glass console with a blinking accent cursor.
- Rain stops (no idle CPU) once the agent TUI takes over (alt-screen) or after the 10s timeout.

- [ ] **Step 4: Delete this plan file** (per AGENTS.md, once fully implemented) and open the PR.

---

## Self-Review notes

- **Spec coverage:** rain_rgb (T1) · canvas features (T2) · MatrixRain component w/ katakana+digits, woven words, head/trail color, dpr, resize, reduced-motion, rAF + use_drop cleanup (T3) · loading-branch rewrite, skeleton/glow removed, soft-glass console, name woven (T4) · tests (T1 unit, T5 scrape) · CPU bound via unmount (T3 use_drop) · scope = terminal only. All covered.
- **No placeholders:** every code step is complete.
- **Type consistency:** `rain_rgb: &'static str` defined T1, consumed as `accent.rain_rgb.to_string()` T4; `MatrixRain { accent_rgb: String, words: Vec<String> }` defined T3, called with those exact prop names T4; helpers `brighten`/`pick_glyph`/`start_rain` defined and used within T3.
- **Edition note:** uses `if let` chains (`&& let`), already used in this codebase (`page.rs:109`, `plugin.rs`); if the wasm target rejects, split into nested `if let`.
