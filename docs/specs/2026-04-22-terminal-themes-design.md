# Terminal Theme System Design

## Overview

Add a Tailwind-class-based terminal color theme system with 13 bundled themes, minimal terminal profiles, and proper ANSI 16-color support. Fix FLAG_INVERSE rendering.

## Scope

**In scope:**
- Terminal color scheme data structure and 13 bundled theme presets
- Minimal terminal profile (theme + font + shell)
- CSS variable + Tailwind class system for ANSI colors in the webview
- `TermColor` enum replacing `Option<[u8; 3]>` for cell colors
- `TermThemeEvent` for delivering theme colors to webview
- FLAG_INVERSE rendering fix in `span_classes()`
- Settings hot-reload for live theme switching
- Backward compatibility migration for existing `TerminalSettings`

**Out of scope (follow-up spec):**
- Full profile system (env vars, working directory, cursor style, layout, sessions)
- Theme editor UI
- Custom theme import from other terminal emulators

## Data Structures

### TerminalColorScheme

Defined in `crates/vmux_desktop/src/themes.rs` (new file).

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalColorScheme {
    pub name: String,
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
    /// ANSI colors 0-15:
    /// [black, red, green, yellow, blue, magenta, cyan, white,
    ///  bright_black, bright_red, bright_green, bright_yellow,
    ///  bright_blue, bright_magenta, bright_cyan, bright_white]
    pub ansi: [[u8; 3]; 16],
}
```

### TerminalProfile

Added to `crates/vmux_desktop/src/settings.rs`.

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalProfile {
    pub name: String,
    pub theme: String,           // theme key, e.g. "catppuccin-mocha"
    pub font_family: String,     // e.g. "JetBrains Mono"
    pub shell: String,           // e.g. "/opt/homebrew/bin/nu"
}
```

### TerminalSettings (updated)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalSettings {
    #[serde(default = "default_profile_name")]
    pub default_profile: String,
    #[serde(default)]
    pub profiles: Vec<TerminalProfile>,
    #[serde(default)]
    pub custom_themes: Vec<TerminalColorScheme>,
}
```

Backward compatibility: If `profiles` is empty on deserialization, auto-create a default profile from legacy `shell` and `font_family` fields (kept with `#[serde(default)]` for migration).

### TermColor (shared event type)

Replaces `Option<[u8; 3]>` for `fg`/`bg` in `TermSpan`. Defined in `crates/vmux_terminal/src/event.rs`.

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TermColor {
    Default,
    Indexed(u8),         // ANSI 0-15
    Rgb(u8, u8, u8),     // true color or 256-color (16-255)
}
```

### TermThemeEvent (shared event type)

Defined in `crates/vmux_terminal/src/event.rs`.

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TermThemeEvent {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
    pub ansi: [[u8; 3]; 16],
}
```

## Bundled Themes

All defined as const data in `themes.rs`. `get_builtin_themes() -> Vec<TerminalColorScheme>`.

| Theme | Key | Default |
|-------|-----|---------|
| Catppuccin Mocha | `catppuccin-mocha` | Yes |
| Catppuccin Latte | `catppuccin-latte` | |
| Catppuccin Frappe | `catppuccin-frappe` | |
| Catppuccin Macchiato | `catppuccin-macchiato` | |
| Dracula | `dracula` | |
| Tokyo Night | `tokyo-night` | |
| Nord | `nord` | |
| Solarized Dark | `solarized-dark` | |
| Solarized Light | `solarized-light` | |
| Gruvbox Dark | `gruvbox-dark` | |
| One Dark | `one-dark` | |
| Rose Pine | `rose-pine` | |
| Kanagawa | `kanagawa` | |

Custom themes in `settings.ron` under `custom_themes` override builtins with the same name.

## Color Flow

### Native side (`terminal.rs`)

`color_to_rgb()` replaced with `color_to_term_color()`:

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
```

`named_to_ansi_index()` maps: Black=0, Red=1, Green=2, Yellow=3, Blue=4, Magenta=5, Cyan=6, White=7, BrightBlack=8, ..., BrightWhite=15.

`build_viewport()` uses `TermColor` instead of `Option<[u8; 3]>` for span fg/bg.

### Theme event delivery

New Bevy system `sync_terminal_theme` with query filter `Or<(Added<Terminal>, Changed<Terminal>)>` plus `Res<AppSettings>.is_changed()`. Runs in `Update` schedule.

When triggered, for each matching terminal entity:
1. Read `terminal.profile` name
2. Look up `TerminalProfile` in `settings.terminal.profiles` by name
3. Resolve theme: check `settings.terminal.custom_themes` first, then `get_builtin_themes()`, fallback to `catppuccin-mocha` with `warn!()`
4. Send `TermThemeEvent` via `HostEmitEvent` to that terminal's `Browser` entity

Theme lookup: check `custom_themes` first, then `get_builtin_themes()`, fallback to `catppuccin-mocha` with a warning.

Registration: `JsEmitEventPlugin::<TermThemeEvent>` added to the terminal plugin so the webview can receive theme events.

### Webview side (`app.rs`)

On receiving `TermThemeEvent`, apply CSS variables to the terminal container element:

```
--term-fg: rgb(r, g, b)
--term-bg: rgb(r, g, b)
--term-cursor: rgb(r, g, b)
--ansi-0 through --ansi-15: rgb(r, g, b)
```

Implementation: Store theme colors as Dioxus `Signal<Option<TermThemeEvent>>`. On `TermThemeEvent` received via `use_event_listener`, update the signal. The terminal container div reads the signal and applies CSS variables as inline style properties (e.g. `style="--term-fg:rgb(205,214,244); --term-bg:rgb(30,30,46); --ansi-0:rgb(69,71,90); ..."`) on the outermost container element. This ensures CSS variables are scoped to the terminal instance.

#### Rendering with Tailwind classes

The current `span_style()` is replaced by two functions:

- `span_classes(span) -> String` — Tailwind utility classes for indexed ANSI colors and text flags
- `span_inline_style(span) -> String` — inline styles only for `TermColor::Rgb` (true color / 256-color above 15)

Both functions account for FLAG_INVERSE by swapping fg/bg before rendering.

**Color rendering strategy:**

| `TermColor` variant | Rendering method | Example output |
|---|---|---|
| `Default` | Inherits from container | _(no class)_ |
| `Default` (inverse) | TW class | `text-term-bg` / `bg-term-fg` |
| `Indexed(i)` | TW class via CSS var | `text-ansi-1` / `bg-ansi-8` |
| `Rgb(r,g,b)` | Inline style (dynamic) | `color:rgb(243,139,168)` |

**Flag rendering (all TW classes):**

| Flag | Tailwind class |
|---|---|
| `FLAG_BOLD` | `font-bold` |
| `FLAG_ITALIC` | `italic` |
| `FLAG_UNDERLINE` | `underline` |
| `FLAG_STRIKETHROUGH` | `line-through` |
| `FLAG_DIM` | `opacity-50` |

```rust
fn span_classes(span: &TermSpan) -> String {
    let mut classes = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    // Foreground
    match fg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("text-term-bg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("text-ansi-{i}")),
        TermColor::Rgb(..) => {} // handled by span_inline_style()
    }

    // Background
    match bg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("bg-term-fg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("bg-ansi-{i}")),
        TermColor::Rgb(..) => {} // handled by span_inline_style()
    }

    // Flags
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

Each `<span>` in the terminal viewport uses both attributes:

```rust
span {
    class: "{span_classes(span)}",
    style: "{span_inline_style(span)}",
    "{span.text}"
}
```

### FLAG_INVERSE fix

Handled inside `span_classes()` and `span_inline_style()` by swapping fg/bg before rendering (see above). The `Default` + `Default` inverse case emits `text-term-bg bg-term-fg` classes.

## CSS & Tailwind Changes

### `index.css`

Add default CSS variable declarations in a `@layer base` block (overridden at runtime by `TermThemeEvent` inline styles on the container):

```css
@layer base {
  :root {
    /* Catppuccin Mocha defaults - overridden by TermThemeEvent inline styles */
    --term-fg: rgb(205, 214, 244);
    --term-bg: rgb(30, 30, 46);
    --term-cursor: rgb(245, 224, 220);
    --ansi-0: rgb(69, 71, 90);    /* Black */
    --ansi-1: rgb(243, 139, 168); /* Red */
    --ansi-2: rgb(166, 227, 161); /* Green */
    --ansi-3: rgb(249, 226, 175); /* Yellow */
    --ansi-4: rgb(137, 180, 250); /* Blue */
    --ansi-5: rgb(245, 194, 231); /* Magenta */
    --ansi-6: rgb(148, 226, 213); /* Cyan */
    --ansi-7: rgb(186, 194, 222); /* White */
    --ansi-8: rgb(88, 91, 112);   /* Bright Black */
    --ansi-9: rgb(243, 139, 168); /* Bright Red */
    --ansi-10: rgb(166, 227, 161);/* Bright Green */
    --ansi-11: rgb(249, 226, 175);/* Bright Yellow */
    --ansi-12: rgb(137, 180, 250);/* Bright Blue */
    --ansi-13: rgb(245, 194, 231);/* Bright Magenta */
    --ansi-14: rgb(148, 226, 213);/* Bright Cyan */
    --ansi-15: rgb(166, 173, 200);/* Bright White */
  }
}
```

Terminal container background uses `bg-term-bg`, default text color uses `text-term-fg`. Cursor styling uses `bg-term-cursor`.

### `tailwind.preset.js`

Add terminal color tokens to the shared preset, following the existing CSS-var pattern:

```js
// Inside theme.extend.colors
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

This enables classes like `text-ansi-1`, `bg-ansi-8`, `text-term-fg`, `bg-term-bg` etc.

## Terminal Component Changes

`Terminal` struct gets a `profile: String` field storing the profile name. Defaults to the `default_profile` from settings.

```rust
pub struct Terminal {
    // existing fields...
    pub profile: String,
}
```

`Terminal::new()` does not take settings as an argument (it has no access to Bevy resources). Instead, the `profile` field defaults to `"default"`. A setup system running on `Added<Terminal>` reads `AppSettings` to resolve the actual default profile name and updates `terminal.profile` if needed. Future per-tab profile override will set this field directly.

## Settings Example

```ron
(
    terminal: (
        default_profile: "default",
        profiles: [
            (
                name: "default",
                theme: "catppuccin-mocha",
                font_family: "JetBrains Mono",
                shell: "/opt/homebrew/bin/nu",
            ),
        ],
        custom_themes: [],
    ),
)
```

## Files Changed

| File | Change |
|------|--------|
| `crates/vmux_desktop/src/themes.rs` | New: theme presets, `TerminalColorScheme`, `get_builtin_themes()` |
| `crates/vmux_desktop/src/terminal.rs` | `color_to_term_color()`, `sync_terminal_theme` system, profile field on Terminal |
| `crates/vmux_desktop/src/settings.rs` | `TerminalProfile`, updated `TerminalSettings`, migration logic |
| `crates/vmux_desktop/src/lib.rs` | Add `mod themes` |
| `crates/vmux_terminal/src/event.rs` | `TermColor` enum, `TermThemeEvent`, update `TermSpan` |
| `crates/vmux_terminal/src/app.rs` | `TermThemeEvent` listener, CSS var application, `span_classes()` + `span_inline_style()` replacing `span_style()`, FLAG_INVERSE |
| `crates/vmux_terminal/assets/index.css` | ANSI color CSS variable defaults, use `bg-term-bg`, `text-term-fg`, `bg-term-cursor` |
| `crates/vmux_ui/tailwind.preset.js` | Add `term-fg`, `term-bg`, `term-cursor`, `ansi-0`..`ansi-15` color tokens |

## Testing

- Visual: verify ANSI 16 colors render distinctly (run a color test script in terminal)
- Visual: switch theme in settings.ron, confirm live update
- Visual: verify FLAG_INVERSE renders correctly (reverse video text)
- Verify backward compat: existing settings.ron without profiles field loads without error
- Verify fallback: unknown theme name in profile falls back to catppuccin-mocha
