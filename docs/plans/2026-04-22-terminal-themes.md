# Terminal Theme System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Tailwind-class-based terminal color theme system with 13 bundled themes, terminal profiles, TermColor enum, and FLAG_INVERSE fix.

**Architecture:** CSS variables define ANSI colors, Tailwind preset maps them to utility classes (`text-ansi-0`..`text-ansi-15`, `bg-term-bg`, etc.). Native side sends `TermColor` enum (Default/Indexed/Rgb) instead of raw RGB. Theme colors delivered via `TermThemeEvent` as inline CSS var overrides on the container.

**Tech Stack:** Rust/Bevy (native), Dioxus WASM (webview), Tailwind CSS v4, RON serialization, alacritty_terminal

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/vmux_terminal/src/event.rs` | Modify | Add `TermColor` enum, `TermThemeEvent`, update `TermSpan` fg/bg types |
| `crates/vmux_desktop/src/themes.rs` | Create | `TerminalColorScheme`, 13 bundled themes, `get_builtin_themes()`, `resolve_theme()` |
| `crates/vmux_desktop/src/settings.rs` | Modify | `TerminalProfile`, updated `TerminalSettings` with profiles + migration |
| `crates/vmux_desktop/src/terminal.rs` | Modify | `color_to_term_color()`, `named_to_ansi_index()`, `sync_terminal_theme` system |
| `crates/vmux_desktop/src/lib.rs` | Modify | Add `mod themes` |
| `crates/vmux_desktop/src/settings.ron` | Modify | Add `terminal` section with profile |
| `crates/vmux_terminal/src/app.rs` | Modify | `span_classes()`, `span_inline_style()`, `TermThemeEvent` listener, CSS var application |
| `crates/vmux_terminal/assets/index.css` | Modify | ANSI color CSS variable defaults |
| `crates/vmux_ui/tailwind.preset.js` | Modify | Add terminal color tokens |

---

### Task 1: TermColor enum and TermThemeEvent in event.rs

**Files:**
- Modify: `crates/vmux_terminal/src/event.rs`

- [ ] **Step 1: Add TermColor enum and TermThemeEvent**

Replace `TermSpan.fg` and `TermSpan.bg` types from `Option<[u8; 3]>` to `TermColor`. Add `TermThemeEvent` struct and event name constant.

In `crates/vmux_terminal/src/event.rs`, add after the existing constants (line 7):

```rust
pub const TERM_THEME_EVENT: &str = "term_theme";
```

Add after the `TERMINAL_WEBVIEW_URL` constant (after line 9):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermThemeEvent {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
    pub ansi: [[u8; 3]; 16],
}
```

- [ ] **Step 2: Update TermSpan to use TermColor**

Change the `TermSpan` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermSpan {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: u16,
}
```

- [ ] **Step 3: Verify event.rs compiles**

Run: `cargo check -p vmux_terminal`

Expected: Compilation errors in `app.rs` (span_style references `Option<[u8;3]>`) and `terminal.rs` (build_viewport). These are fixed in later tasks.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: add TermColor enum and TermThemeEvent to terminal events"
```

---

### Task 2: Tailwind preset and CSS variables

**Files:**
- Modify: `crates/vmux_ui/tailwind.preset.js`
- Modify: `crates/vmux_terminal/assets/index.css`

- [ ] **Step 1: Add terminal color tokens to tailwind.preset.js**

In `crates/vmux_ui/tailwind.preset.js`, inside `theme.extend.colors` (after the `glass` entry, around line 68), add:

```js
        "term-fg": "var(--term-fg)",
        "term-bg": "var(--term-bg)",
        "term-cursor": "var(--term-cursor)",
        "ansi-0": "var(--ansi-0)",
        "ansi-1": "var(--ansi-1)",
        "ansi-2": "var(--ansi-2)",
        "ansi-3": "var(--ansi-3)",
        "ansi-4": "var(--ansi-4)",
        "ansi-5": "var(--ansi-5)",
        "ansi-6": "var(--ansi-6)",
        "ansi-7": "var(--ansi-7)",
        "ansi-8": "var(--ansi-8)",
        "ansi-9": "var(--ansi-9)",
        "ansi-10": "var(--ansi-10)",
        "ansi-11": "var(--ansi-11)",
        "ansi-12": "var(--ansi-12)",
        "ansi-13": "var(--ansi-13)",
        "ansi-14": "var(--ansi-14)",
        "ansi-15": "var(--ansi-15)",
```

- [ ] **Step 2: Add CSS variable defaults to index.css**

In `crates/vmux_terminal/assets/index.css`, add inside the existing `@layer base` block, before the `html.dark` rule:

```css
  :root {
    --term-fg: rgb(205, 214, 244);
    --term-bg: rgb(30, 30, 46);
    --term-cursor: rgb(245, 224, 220);
    --ansi-0: rgb(69, 71, 90);
    --ansi-1: rgb(243, 139, 168);
    --ansi-2: rgb(166, 227, 161);
    --ansi-3: rgb(249, 226, 175);
    --ansi-4: rgb(137, 180, 250);
    --ansi-5: rgb(245, 194, 231);
    --ansi-6: rgb(148, 226, 213);
    --ansi-7: rgb(186, 194, 222);
    --ansi-8: rgb(88, 91, 112);
    --ansi-9: rgb(243, 139, 168);
    --ansi-10: rgb(166, 227, 161);
    --ansi-11: rgb(249, 226, 175);
    --ansi-12: rgb(137, 180, 250);
    --ansi-13: rgb(245, 194, 231);
    --ansi-14: rgb(148, 226, 213);
    --ansi-15: rgb(166, 173, 200);
  }
```

- [ ] **Step 3: Update terminal container to use TW theme classes**

In `crates/vmux_terminal/assets/index.css`, change the `html.dark body` rule:

```css
  html.dark body {
    color-scheme: dark;
    background-color: transparent;
    color: var(--term-fg);
    font-family: "JetBrains Mono", "SF Mono", "Menlo", "Monaco", "Courier New", monospace;
  }
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: add ANSI color CSS variables and Tailwind preset tokens"
```

---

### Task 3: Webview rendering with Tailwind classes (app.rs)

**Files:**
- Modify: `crates/vmux_terminal/src/app.rs`

- [ ] **Step 1: Add TermThemeEvent listener and theme_style signal**

At the top of the `App()` function (after the viewport signal), add a signal and listener for theme events:

```rust
    let mut theme = use_signal(|| None::<TermThemeEvent>);

    let _theme_listener = use_event_listener::<TermThemeEvent, _>(
        TERM_THEME_EVENT,
        move |data| {
            theme.set(Some(data));
        },
    );
```

- [ ] **Step 2: Build theme CSS variable inline style**

After the `font_style` line, add a function to build CSS variable overrides from the theme signal:

```rust
    let theme_style = {
        let t = theme();
        match t {
            Some(t) => {
                let [fr, fg, fb] = t.foreground;
                let [br, bg, bb] = t.background;
                let [cr, cg, cb] = t.cursor;
                let mut s = format!(
                    "--term-fg:rgb({fr},{fg},{fb});--term-bg:rgb({br},{bg},{bb});--term-cursor:rgb({cr},{cg},{cb});"
                );
                for (i, [r, g, b]) in t.ansi.iter().enumerate() {
                    s.push_str(&format!("--ansi-{i}:rgb({r},{g},{b});"));
                }
                s
            }
            None => String::new(),
        }
    };
```

- [ ] **Step 3: Apply theme_style to container div**

Update the outer container div to include theme CSS vars and use `bg-term-bg text-term-fg`:

```rust
        div {
            class: "relative h-full w-full overflow-hidden bg-term-bg text-term-fg font-mono text-sm leading-tight",
            style: "{font_style}{theme_style}",
```

- [ ] **Step 4: Replace span_style with span_classes and span_inline_style**

Replace the entire `span_style` function with:

```rust
fn span_classes(span: &TermSpan) -> String {
    let mut classes = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    match fg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("text-term-bg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("text-ansi-{i}")),
        TermColor::Rgb(..) => {}
    }

    match bg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("bg-term-fg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("bg-ansi-{i}")),
        TermColor::Rgb(..) => {}
    }

    if span.flags & FLAG_BOLD != 0 { classes.push("font-bold".into()); }
    if span.flags & FLAG_ITALIC != 0 { classes.push("italic".into()); }
    if span.flags & FLAG_UNDERLINE != 0 { classes.push("underline".into()); }
    if span.flags & FLAG_STRIKETHROUGH != 0 { classes.push("line-through".into()); }
    if span.flags & FLAG_DIM != 0 { classes.push("opacity-50".into()); }

    classes.join(" ")
}

fn span_inline_style(span: &TermSpan) -> String {
    let mut parts = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    if let TermColor::Rgb(r, g, b) = fg {
        parts.push(format!("color:rgb({r},{g},{b})"));
    }
    if let TermColor::Rgb(r, g, b) = bg {
        parts.push(format!("background:rgb({r},{g},{b})"));
    }

    parts.join(";")
}
```

- [ ] **Step 5: Update span rendering in RSX**

Replace the span element inside the line loop:

```rust
                        for (span_idx , span) in line.spans.iter().enumerate() {
                            span {
                                key: "{span_idx}",
                                class: "{span_classes(span)}",
                                style: "{span_inline_style(span)}",
                                "{span.text}"
                            }
                        }
```

- [ ] **Step 6: Update cursor to use term-cursor**

Change the cursor span's style:

```rust
                        if row_idx == vp.cursor.row as usize && vp.cursor.visible {
                            span {
                                class: "absolute bg-term-cursor",
                                style: "left: calc(0.25rem + {vp.cursor.col}ch); color: var(--term-bg); animation: blink 1s step-end infinite;",
                                "{cursor_char(&vp, row_idx)}"
                            }
                        }
```

- [ ] **Step 7: Verify app.rs compiles**

Run: `cargo check -p vmux_terminal`

Expected: PASS (app.rs now uses TermColor types from event.rs).

- [ ] **Step 8: Commit**

```bash
git commit -m "feat: replace inline styles with Tailwind classes for terminal spans"
```

---

### Task 4: Native color conversion (terminal.rs)

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add NamedColor import and color_to_term_color function**

In `crates/vmux_desktop/src/terminal.rs`, update the `vte::ansi` import (line 12) to include `NamedColor`:

```rust
    vte::ansi::{Color, NamedColor, Processor},
```

Replace `color_to_rgb` (around line 370) with:

```rust
fn color_to_term_color(color: &Color) -> TermColor {
    match color {
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::DimForeground
            | NamedColor::BrightForeground => TermColor::Default,
            NamedColor::Background | NamedColor::DimBackground => TermColor::Default,
            NamedColor::Cursor => TermColor::Default,
            other => TermColor::Indexed(named_to_ansi_index(other)),
        },
        Color::Indexed(idx) if *idx < 16 => TermColor::Indexed(*idx),
        Color::Indexed(idx) => {
            let [r, g, b] = ansi_256_to_rgb(*idx);
            TermColor::Rgb(r, g, b)
        }
        Color::Spec(rgb) => TermColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn named_to_ansi_index(named: &NamedColor) -> u8 {
    match named {
        NamedColor::Black | NamedColor::DimBlack => 0,
        NamedColor::Red | NamedColor::DimRed => 1,
        NamedColor::Green | NamedColor::DimGreen => 2,
        NamedColor::Yellow | NamedColor::DimYellow => 3,
        NamedColor::Blue | NamedColor::DimBlue => 4,
        NamedColor::Magenta | NamedColor::DimMagenta => 5,
        NamedColor::Cyan | NamedColor::DimCyan => 6,
        NamedColor::White | NamedColor::DimWhite => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => 7, // fallback to white
    }
}
```

- [ ] **Step 2: Add TermColor import**

Add to the existing `use vmux_terminal::event::*;` import (already present via line 30). If not present, add:

```rust
use vmux_terminal::event::TermColor;
```

- [ ] **Step 3: Update build_viewport to use TermColor**

In `build_viewport` (around line 295), change the type tracking variables:

```rust
        let mut cur_fg: TermColor = TermColor::Default;
        let mut cur_bg: TermColor = TermColor::Default;
```

Change the cell color extraction (around line 317):

```rust
            let fg = color_to_term_color(&cell.fg);
            let bg = color_to_term_color(&cell.bg);
```

- [ ] **Step 4: Verify terminal.rs compiles**

Run: `cargo check -p vmux_desktop`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: use TermColor enum for terminal cell colors"
```

---

### Task 5: Bundled themes (themes.rs)

**Files:**
- Create: `crates/vmux_desktop/src/themes.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Create themes.rs with TerminalColorScheme and all 13 themes**

Create `crates/vmux_desktop/src/themes.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalColorScheme {
    pub name: String,
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
    pub ansi: [[u8; 3]; 16],
}

pub fn get_builtin_themes() -> Vec<TerminalColorScheme> {
    vec![
        TerminalColorScheme {
            name: "catppuccin-mocha".into(),
            foreground: [205, 214, 244],
            background: [30, 30, 46],
            cursor: [245, 224, 220],
            ansi: [
                [69, 71, 90], [243, 139, 168], [166, 227, 161], [249, 226, 175],
                [137, 180, 250], [245, 194, 231], [148, 226, 213], [186, 194, 222],
                [88, 91, 112], [243, 139, 168], [166, 227, 161], [249, 226, 175],
                [137, 180, 250], [245, 194, 231], [148, 226, 213], [166, 173, 200],
            ],
        },
        TerminalColorScheme {
            name: "catppuccin-latte".into(),
            foreground: [76, 79, 105],
            background: [239, 241, 245],
            cursor: [220, 138, 120],
            ansi: [
                [172, 176, 190], [210, 15, 57], [64, 160, 43], [223, 142, 29],
                [30, 102, 245], [234, 118, 203], [4, 165, 229], [76, 79, 105],
                [140, 143, 161], [210, 15, 57], [64, 160, 43], [223, 142, 29],
                [30, 102, 245], [234, 118, 203], [4, 165, 229], [92, 95, 119],
            ],
        },
        TerminalColorScheme {
            name: "catppuccin-frappe".into(),
            foreground: [198, 208, 245],
            background: [48, 52, 70],
            cursor: [242, 213, 207],
            ansi: [
                [81, 87, 109], [231, 130, 132], [166, 209, 137], [229, 200, 144],
                [140, 170, 238], [244, 184, 228], [129, 200, 190], [181, 191, 226],
                [98, 104, 128], [231, 130, 132], [166, 209, 137], [229, 200, 144],
                [140, 170, 238], [244, 184, 228], [129, 200, 190], [165, 173, 206],
            ],
        },
        TerminalColorScheme {
            name: "catppuccin-macchiato".into(),
            foreground: [202, 211, 245],
            background: [36, 39, 58],
            cursor: [244, 219, 214],
            ansi: [
                [73, 77, 100], [237, 135, 150], [166, 218, 149], [238, 212, 159],
                [138, 173, 244], [245, 189, 230], [139, 213, 202], [184, 192, 224],
                [91, 96, 120], [237, 135, 150], [166, 218, 149], [238, 212, 159],
                [138, 173, 244], [245, 189, 230], [139, 213, 202], [165, 173, 203],
            ],
        },
        TerminalColorScheme {
            name: "dracula".into(),
            foreground: [248, 248, 242],
            background: [40, 42, 54],
            cursor: [248, 248, 242],
            ansi: [
                [33, 34, 44], [255, 85, 85], [80, 250, 123], [241, 250, 140],
                [98, 114, 164], [255, 121, 198], [139, 233, 253], [248, 248, 242],
                [98, 114, 164], [255, 110, 110], [105, 255, 148], [255, 255, 165],
                [123, 139, 189], [255, 146, 223], [164, 255, 255], [255, 255, 255],
            ],
        },
        TerminalColorScheme {
            name: "tokyo-night".into(),
            foreground: [192, 202, 245],
            background: [26, 27, 38],
            cursor: [192, 202, 245],
            ansi: [
                [65, 72, 104], [247, 118, 142], [158, 206, 106], [224, 175, 104],
                [122, 162, 247], [187, 154, 247], [125, 207, 255], [169, 177, 214],
                [65, 72, 104], [247, 118, 142], [158, 206, 106], [224, 175, 104],
                [122, 162, 247], [187, 154, 247], [125, 207, 255], [192, 202, 245],
            ],
        },
        TerminalColorScheme {
            name: "nord".into(),
            foreground: [216, 222, 233],
            background: [46, 52, 64],
            cursor: [216, 222, 233],
            ansi: [
                [59, 66, 82], [191, 97, 106], [163, 190, 140], [235, 203, 139],
                [129, 161, 193], [180, 142, 173], [136, 192, 208], [229, 233, 240],
                [76, 86, 106], [191, 97, 106], [163, 190, 140], [235, 203, 139],
                [129, 161, 193], [180, 142, 173], [143, 188, 187], [236, 239, 244],
            ],
        },
        TerminalColorScheme {
            name: "solarized-dark".into(),
            foreground: [131, 148, 150],
            background: [0, 43, 54],
            cursor: [131, 148, 150],
            ansi: [
                [7, 54, 66], [220, 50, 47], [133, 153, 0], [181, 137, 0],
                [38, 139, 210], [211, 54, 130], [42, 161, 152], [238, 232, 213],
                [0, 43, 54], [203, 75, 22], [88, 110, 117], [101, 123, 131],
                [131, 148, 150], [108, 113, 196], [147, 161, 161], [253, 246, 227],
            ],
        },
        TerminalColorScheme {
            name: "solarized-light".into(),
            foreground: [101, 123, 131],
            background: [253, 246, 227],
            cursor: [101, 123, 131],
            ansi: [
                [238, 232, 213], [220, 50, 47], [133, 153, 0], [181, 137, 0],
                [38, 139, 210], [211, 54, 130], [42, 161, 152], [7, 54, 66],
                [253, 246, 227], [203, 75, 22], [88, 110, 117], [101, 123, 131],
                [131, 148, 150], [108, 113, 196], [147, 161, 161], [0, 43, 54],
            ],
        },
        TerminalColorScheme {
            name: "gruvbox-dark".into(),
            foreground: [235, 219, 178],
            background: [40, 40, 40],
            cursor: [235, 219, 178],
            ansi: [
                [40, 40, 40], [204, 36, 29], [152, 151, 26], [215, 153, 33],
                [69, 133, 136], [177, 98, 134], [104, 157, 106], [168, 153, 132],
                [146, 131, 116], [251, 73, 52], [184, 187, 38], [250, 189, 47],
                [131, 165, 152], [211, 134, 155], [142, 192, 124], [235, 219, 178],
            ],
        },
        TerminalColorScheme {
            name: "one-dark".into(),
            foreground: [171, 178, 191],
            background: [40, 44, 52],
            cursor: [171, 178, 191],
            ansi: [
                [40, 44, 52], [224, 108, 117], [152, 195, 121], [229, 192, 123],
                [97, 175, 239], [198, 120, 221], [86, 182, 194], [171, 178, 191],
                [92, 99, 112], [224, 108, 117], [152, 195, 121], [229, 192, 123],
                [97, 175, 239], [198, 120, 221], [86, 182, 194], [255, 255, 255],
            ],
        },
        TerminalColorScheme {
            name: "rose-pine".into(),
            foreground: [224, 222, 244],
            background: [25, 23, 36],
            cursor: [224, 222, 244],
            ansi: [
                [38, 35, 53], [235, 111, 146], [49, 116, 143], [246, 193, 119],
                [156, 207, 216], [196, 167, 231], [234, 154, 151], [224, 222, 244],
                [110, 106, 134], [235, 111, 146], [49, 116, 143], [246, 193, 119],
                [156, 207, 216], [196, 167, 231], [234, 154, 151], [224, 222, 244],
            ],
        },
        TerminalColorScheme {
            name: "kanagawa".into(),
            foreground: [220, 215, 186],
            background: [31, 31, 40],
            cursor: [195, 176, 135],
            ansi: [
                [22, 22, 29], [195, 64, 67], [118, 148, 106], [192, 163, 77],
                [126, 156, 216], [149, 127, 184], [106, 149, 137], [200, 196, 172],
                [84, 84, 109], [231, 115, 118], [135, 169, 117], [224, 195, 117],
                [127, 180, 202], [148, 130, 196], [127, 180, 169], [220, 215, 186],
            ],
        },
    ]
}

pub fn resolve_theme(
    name: &str,
    custom_themes: &[TerminalColorScheme],
) -> TerminalColorScheme {
    // Check custom themes first
    if let Some(t) = custom_themes.iter().find(|t| t.name == name) {
        return t.clone();
    }
    // Check builtins
    if let Some(t) = get_builtin_themes().into_iter().find(|t| t.name == name) {
        return t;
    }
    // Fallback
    bevy::log::warn!("Unknown terminal theme '{}', falling back to catppuccin-mocha", name);
    get_builtin_themes().into_iter().next().unwrap()
}
```

- [ ] **Step 2: Add mod themes to lib.rs**

In `crates/vmux_desktop/src/lib.rs`, add after `mod terminal;` (line 11):

```rust
mod themes;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p vmux_desktop`

Expected: PASS (themes.rs is self-contained, only used later).

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: add 13 bundled terminal color themes"
```

---

### Task 6: Terminal profiles in settings

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`
- Modify: `crates/vmux_desktop/src/settings.ron`

- [ ] **Step 1: Add TerminalProfile and update TerminalSettings**

In `crates/vmux_desktop/src/settings.rs`, replace the existing `TerminalSettings` struct (around line 140) with:

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct TerminalSettings {
    // Legacy fields for backward compatibility
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    // New fields
    #[serde(default = "default_profile_name")]
    pub default_profile: String,
    #[serde(default)]
    pub profiles: Vec<TerminalProfile>,
    #[serde(default)]
    pub custom_themes: Vec<crate::themes::TerminalColorScheme>,
}

fn default_profile_name() -> String {
    "default".to_string()
}

#[derive(Clone, Debug, Deserialize)]
pub struct TerminalProfile {
    pub name: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_terminal_font_family")]
    pub font_family: String,
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_theme() -> String {
    "catppuccin-mocha".to_string()
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}
```

- [ ] **Step 2: Add helper method for profile resolution**

Add an `impl` block for `TerminalSettings`:

```rust
impl TerminalSettings {
    /// Get the effective profile, migrating legacy fields if needed.
    pub fn resolve_profile(&self, name: &str) -> TerminalProfile {
        // Check explicit profiles
        if let Some(p) = self.profiles.iter().find(|p| p.name == name) {
            return p.clone();
        }
        // Fallback: build from legacy fields or defaults
        TerminalProfile {
            name: name.to_string(),
            theme: default_theme(),
            font_family: self.font_family.clone()
                .unwrap_or_else(default_terminal_font_family),
            shell: self.shell.clone()
                .unwrap_or_else(default_shell),
        }
    }
}
```

- [ ] **Step 3: Update settings.ron with terminal section**

In `crates/vmux_desktop/src/settings.ron`, add before the closing `)`:

```ron
    terminal: (
        default_profile: "default",
        profiles: [
            (
                name: "default",
                theme: "catppuccin-mocha",
                font_family: "JetBrainsMono Nerd Font",
                shell: "/opt/homebrew/bin/nu",
            ),
        ],
    ),
```

- [ ] **Step 4: Update terminal.rs to use new settings structure**

In `crates/vmux_desktop/src/terminal.rs`, update `Terminal::new()` to read shell from profile. Find the shell extraction (around line 100):

```rust
        let shell = settings
            .terminal
            .as_ref()
            .map(|t| t.shell.clone())
            .unwrap_or_else(default_shell);
```

Replace with:

```rust
        let shell = settings
            .terminal
            .as_ref()
            .map(|t| t.resolve_profile(&t.default_profile).shell)
            .unwrap_or_else(default_shell);
```

Update `sync_terminal_viewport` font_family extraction (around line 270):

```rust
    let font_family = settings
        .terminal
        .as_ref()
        .map(|t| t.font_family.clone());
```

Replace with:

```rust
    let font_family = settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_profile(&t.default_profile).font_family);
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p vmux_desktop`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat: add terminal profiles with theme support in settings"
```

---

### Task 7: Theme event delivery (sync_terminal_theme system)

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add TermThemeEvent import and JsEmitEventPlugin registration**

In `crates/vmux_desktop/src/terminal.rs`, add to the imports:

```rust
use vmux_terminal::event::TERM_THEME_EVENT;
```

In `TerminalPlugin::build()`, add the `JsEmitEventPlugin` for theme events. Find the existing `JsEmitEventPlugin::<TermResizeEvent>` line (around line 76) and add after it:

```rust
            .add_plugins(JsEmitEventPlugin::<vmux_terminal::event::TermThemeEvent>::default())
```

- [ ] **Step 2: Add sync_terminal_theme system**

Add the system to `TerminalPlugin::build()` in the `Update` systems. Find the existing `.add_systems(Update, ...)` (around line 82) and add `sync_terminal_theme`:

```rust
            .add_systems(Update, (poll_pty_output, sync_terminal_viewport, sync_terminal_theme).chain())
```

Add the system implementation after `on_term_resize`:

```rust
fn sync_terminal_theme(
    q: Query<Entity, With<Terminal>>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut sent: Local<bool>,
) {
    // Send theme on first run or when settings change
    if !settings.is_changed() && *sent {
        return;
    }

    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let profile = terminal_settings.resolve_profile(&terminal_settings.default_profile);
    let theme = crate::themes::resolve_theme(&profile.theme, &terminal_settings.custom_themes);

    let event = vmux_terminal::event::TermThemeEvent {
        foreground: theme.foreground,
        background: theme.background,
        cursor: theme.cursor,
        ansi: theme.ansi,
    };

    let body = ron::ser::to_string(&event).unwrap_or_default();

    for entity in &q {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(HostEmitEvent::new(entity, TERM_THEME_EVENT, &body));
            *sent = true;
        }
    }
}
```

- [ ] **Step 3: Also send theme on terminal ready**

Update `on_term_ready` to also mark the theme as needing resend. The simplest approach: the `sync_terminal_theme` system already runs every frame checking `settings.is_changed()`. For newly created terminals, we need to send theme. Update the `sent` local to reset when new terminals appear.

Actually, simpler: change `sync_terminal_theme` to also trigger on `Added<Terminal>`:

```rust
fn sync_terminal_theme(
    q: Query<Entity, With<Terminal>>,
    new_terminals: Query<Entity, Added<Terminal>>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut last_theme_hash: Local<u64>,
) {
    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let profile = terminal_settings.resolve_profile(&terminal_settings.default_profile);
    let theme = crate::themes::resolve_theme(&profile.theme, &terminal_settings.custom_themes);

    // Simple hash to detect theme changes
    let hash = {
        let mut h: u64 = 0;
        for b in &theme.foreground { h = h.wrapping_mul(31).wrapping_add(*b as u64); }
        for b in &theme.background { h = h.wrapping_mul(31).wrapping_add(*b as u64); }
        for row in &theme.ansi { for b in row { h = h.wrapping_mul(31).wrapping_add(*b as u64); } }
        h
    };

    let theme_changed = hash != *last_theme_hash;
    if !theme_changed && new_terminals.is_empty() {
        return;
    }
    *last_theme_hash = hash;

    let event = vmux_terminal::event::TermThemeEvent {
        foreground: theme.foreground,
        background: theme.background,
        cursor: theme.cursor,
        ansi: theme.ansi,
    };
    let body = ron::ser::to_string(&event).unwrap_or_default();

    let targets: Vec<Entity> = if theme_changed {
        q.iter().collect()
    } else {
        new_terminals.iter().collect()
    };

    for entity in targets {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(HostEmitEvent::new(entity, TERM_THEME_EVENT, &body));
        }
    }
}
```

- [ ] **Step 4: Verify full build**

Run: `cargo check -p vmux_desktop`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: add sync_terminal_theme system for live theme delivery"
```

---

### Task 8: Final verification

- [ ] **Step 1: Full workspace check**

Run: `cargo check`

Expected: PASS with no new errors.

- [ ] **Step 2: Verify settings.ron parses**

The embedded `settings.ron` is parsed at compile time via `include_str!`. If `cargo check` passes, the RON is syntactically valid.

- [ ] **Step 3: Commit all remaining changes**

```bash
git commit -m "feat: terminal theme system with 13 bundled themes and Tailwind classes"
```
