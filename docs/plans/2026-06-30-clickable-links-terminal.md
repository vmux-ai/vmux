# Clickable Links in Terminal & Agent Panes — Implementation Plan

> **For agentic workers:** Implement inline (vmux CEF builds are too heavy for subagent-driven execution). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** cmd+hover highlights URLs/file-paths in terminal output; cmd+click opens them in a new browser stack beside the pane. Covers terminal panes and agent (vibe/codex) CLI panes (both load `vmux://terminal/`).

**Architecture:** Dumb frontend. The Bevy host detects links while it materializes each `TermViewportPatch`, attaches column-ranged `LinkRange`s to every `TermLine`, and pushes them to the page. The Dioxus page renders an underline overlay on cmd-hover and, on cmd+click, emits a `TermLinkOpenRequest { url }` intent. The host turns that into `AppCommand::Browser(Open(InNewStack { url }))`, mirroring the existing web-shortcut dispatch in `on_term_key`.

**Tech Stack:** Rust, Bevy ECS, bevy_cef (rkyv IPC), Dioxus (WASM), `vmux_command::event::{looks_like_url, looks_like_path}`, `unicode-width`.

---

## File Structure

- `crates/vmux_core/src/event.rs` — data types on the wire: `LinkRange`, `TermLine.links`, `TermLinkOpenRequest`, `TERM_LINK_OPEN_EVENT`.
- `crates/vmux_terminal/Cargo.toml` — move `unicode-width` to shared deps (needed natively now).
- `crates/vmux_terminal/src/link.rs` — NEW: link detection + column mapping + unit tests.
- `crates/vmux_terminal/src/lib.rs` — declare `mod link;`.
- `crates/vmux_terminal/src/plugin.rs` — annotate links at both patch sites; register event; `on_term_link_open` observer; system test.
- `crates/vmux_terminal/src/page.rs` — hover-link state, underline overlay, cmd+click intent emission.

---

## Task 1: Wire types in `vmux_core`

**Files:**
- Modify: `crates/vmux_core/src/event.rs` (`TermLine` ~916, event-id consts ~7, near `TermMouseEvent` ~1081)

- [ ] **Step 1: Add `LinkRange` and a `links` field to `TermLine`**

Replace the `TermLine` struct (around line 916) with:

```rust
pub struct TermLine {
    pub spans: Vec<TermSpan>,
    /// Clickable URL/path ranges in this row, in column coordinates.
    /// Computed by the host; the service always leaves this empty.
    #[serde(default)]
    pub links: Vec<LinkRange>,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct LinkRange {
    /// First column of the link (0-based, inclusive).
    pub start_col: u16,
    /// Last column of the link (0-based, inclusive).
    pub end_col: u16,
    /// Ready-to-open target: `http(s)://…`, `data:…`, or `file://…`.
    pub url: String,
}
```

(`TermLine` keeps its existing derive block — `Default` already derives, and `Vec::default()` is empty.)

- [ ] **Step 2: Add the open-intent event id**

Next to the other `TERM_*_EVENT` consts (top of file, ~line 14) add:

```rust
pub const TERM_LINK_OPEN_EVENT: &str = "term_link_open";
```

- [ ] **Step 3: Add the `TermLinkOpenRequest` event struct**

After `TermMouseEvent` (~line 1092) add:

```rust
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermLinkOpenRequest {
    /// Ready-to-open target resolved by the host (http(s)://, data:, file://).
    pub url: String,
}
```

- [ ] **Step 4: Build**

Run: `cargo build -p vmux_core`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(core): LinkRange on TermLine + TermLinkOpenRequest event"
```

---

## Task 2: Make `unicode-width` available natively

**Files:**
- Modify: `crates/vmux_terminal/Cargo.toml`

- [ ] **Step 1: Move `unicode-width` to shared deps**

In `[dependencies]` (after line 18) add:

```toml
unicode-width = "0.2"
```

Remove the `unicode-width = "0.2"` line from the `[target.'cfg(target_arch = "wasm32")'.dependencies]` block.

- [ ] **Step 2: Build both targets**

Run: `cargo build -p vmux_terminal && cargo check -p vmux_terminal --target wasm32-unknown-unknown`
Expected: both compile (page.rs still finds `unicode_width`).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_terminal/Cargo.toml
git commit -m "chore(terminal): unicode-width for native link detection"
```

---

## Task 3: Link detection module (`link.rs`)

**Files:**
- Create: `crates/vmux_terminal/src/link.rs`
- Modify: `crates/vmux_terminal/src/lib.rs` (add `mod link;`)

- [ ] **Step 1: Write `link.rs` with detection + column mapping + tests**

```rust
//! Detects clickable URLs and file paths in terminal lines and annotates
//! [`TermLine`]s with column-ranged [`LinkRange`]s for the page to render.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use unicode_width::UnicodeWidthChar;
use vmux_command::event::{is_data_uri, looks_like_path, looks_like_url};
use vmux_core::event::{LinkRange, TermLine};

/// Characters trimmed from the end of a detected token (trailing punctuation
/// that is almost never part of the link).
const TRAILING_TRIM: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '>'];

/// Annotate `line` with the links found in its visible text.
///
/// `cwd` is the terminal's working directory, used to resolve relative file
/// paths. When `None`, relative paths are skipped (URLs and absolute paths
/// are still detected).
pub fn annotate_links(line: &mut TermLine, cwd: Option<&Path>) {
    line.links.clear();

    // Reconstruct the row text and a char-index -> (start_col, width) map from
    // the spans, honoring wide characters via their starting column.
    let mut text = String::new();
    let mut cols: Vec<(u16, u16)> = Vec::new(); // (start_col, width) per char
    for span in &line.spans {
        let mut col = span.col;
        for ch in span.text.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1) as u16;
            text.push(ch);
            cols.push((col, w));
            col = col.saturating_add(w);
        }
    }
    if text.is_empty() {
        return;
    }

    for (char_start, char_end, url) in detect_links_in_text(&text, cwd) {
        let Some(&(start_col, _)) = cols.get(char_start) else {
            continue;
        };
        let Some(&(last_col, last_w)) = cols.get(char_end - 1) else {
            continue;
        };
        line.links.push(LinkRange {
            start_col,
            end_col: last_col + last_w - 1,
            url,
        });
    }
}

/// Find link tokens in `text`. Returns `(char_start, char_end_exclusive, url)`
/// in char-index coordinates.
pub fn detect_links_in_text(text: &str, cwd: Option<&Path>) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        // Trim trailing punctuation from the token.
        let mut end = i;
        while end > start && TRAILING_TRIM.contains(&chars[end - 1]) {
            end -= 1;
        }
        if end <= start {
            continue;
        }
        let token: String = chars[start..end].iter().collect();
        if let Some(url) = resolve_target(&token, cwd) {
            out.push((start, end, url));
        }
    }
    out
}

/// Resolve a token to a ready-to-open URL, or `None` if it is not a link.
fn resolve_target(token: &str, cwd: Option<&Path>) -> Option<String> {
    if looks_like_url(token) {
        if is_data_uri(token) || token.contains("://") {
            return Some(token.to_string());
        }
        // Bare domain like `vmux.ai/docs`.
        return Some(format!("https://{token}"));
    }
    if looks_like_path(token) {
        return resolve_path(token, cwd);
    }
    None
}

/// Resolve a file path token to a `file://` URL.
fn resolve_path(token: &str, cwd: Option<&Path>) -> Option<String> {
    let expanded = if let Some(rest) = token.strip_prefix("~/") {
        let home = std::env::var_os("HOME")?;
        Path::new(&home).join(rest)
    } else {
        let p = Path::new(token);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd?.join(p)
        }
    };
    Some(format!("file://{}", expanded.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::event::TermSpan;

    fn line_of(text: &str) -> TermLine {
        TermLine {
            spans: vec![TermSpan {
                text: text.to_string(),
                col: 0,
                grid_cols: text.chars().count() as u16,
                ..Default::default()
            }],
            links: Vec::new(),
        }
    }

    #[test]
    fn detects_https_url() {
        let mut l = line_of("see https://vmux.ai/docs now");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].url, "https://vmux.ai/docs");
        assert_eq!(l.links[0].start_col, 4);
        assert_eq!(l.links[0].end_col, 23);
    }

    #[test]
    fn prefixes_bare_domain() {
        let mut l = line_of("visit vmux.ai/x");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].url, "https://vmux.ai/x");
    }

    #[test]
    fn trims_trailing_punctuation() {
        let mut l = line_of("docs at https://vmux.ai/docs.");
        annotate_links(&mut l, None);
        assert_eq!(l.links[0].url, "https://vmux.ai/docs");
    }

    #[test]
    fn detects_absolute_path() {
        let mut l = line_of("edit /Users/me/main.rs please");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].url, "file:///Users/me/main.rs");
    }

    #[test]
    fn resolves_relative_path_against_cwd() {
        let mut l = line_of("see crates/foo.rs");
        annotate_links(&mut l, Some(Path::new("/work")));
        assert_eq!(l.links[0].url, "file:///work/crates/foo.rs");
    }

    #[test]
    fn skips_relative_path_without_cwd() {
        let mut l = line_of("see crates/foo.rs");
        annotate_links(&mut l, None);
        assert!(l.links.is_empty());
    }

    #[test]
    fn ignores_bare_words() {
        let mut l = line_of("hello world this is prose");
        annotate_links(&mut l, None);
        assert!(l.links.is_empty());
    }

    #[test]
    fn multiple_links_one_line() {
        let mut l = line_of("https://a.com and https://b.com");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 2);
        assert_eq!(l.links[0].url, "https://a.com");
        assert_eq!(l.links[1].url, "https://b.com");
    }

    #[test]
    fn wide_chars_shift_columns() {
        // "あ" is width 2; the URL starts after it (col 0 + 2 = 2).
        let mut l = line_of("あ https://x.io");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        // 'あ'(2) + ' '(1) = col 3.
        assert_eq!(l.links[0].start_col, 3);
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/vmux_terminal/src/lib.rs`, add with the other `mod` declarations:

```rust
mod link;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p vmux_terminal link::`
Expected: all `link::tests::*` pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_terminal/src/link.rs crates/vmux_terminal/src/lib.rs
git commit -m "feat(terminal): URL/path link detection with column mapping"
```

---

## Task 4: Host — annotate patches, register event, dispatch open

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs` (patch sites ~1306 & ~1369; emitter registration ~346; observers ~378; new observer + test)

- [ ] **Step 1: Annotate links at the diff patch site**

In the `ServiceMessage::ViewportPatch { … }` arm, change the destructured `changed_lines` to be mutable and annotate before building the patch. Just before `let patch = TermViewportPatch {` (line ~1326), insert:

```rust
let mut changed_lines = changed_lines;
for (_, line) in changed_lines.iter_mut() {
    crate::link::annotate_links(line, None);
}
```

(`cwd` is `None` for v1 — URLs + absolute paths. Relative-path cwd plumbing is a documented non-goal.)

- [ ] **Step 2: Annotate links at the snapshot patch site**

In the `ServiceMessage::Snapshot { … }` arm, annotate after the `changed_lines` vec is built. Replace the `let patch = TermViewportPatch { changed_lines: lines.into_iter()… }` construction (line ~1384) with:

```rust
let mut changed_lines: Vec<(u16, TermLine)> = lines
    .into_iter()
    .enumerate()
    .map(|(i, l)| (i as u16, l))
    .collect();
for (_, line) in changed_lines.iter_mut() {
    crate::link::annotate_links(line, None);
}
let patch = TermViewportPatch {
    changed_lines,
    cursor,
    cols,
    rows,
    selection: None,
    copy_mode: false,
    full: true,
};
```

(Ensure `TermLine` is imported in `plugin.rs`; it already uses `TermViewportPatch`/`TermSpan` from `vmux_core::event`.)

- [ ] **Step 3: Register the open-intent event with the bin emitter**

In `build()` (line ~346) add `TermLinkOpenRequest` to the emitter tuple:

```rust
.add_plugins(BinEventEmitterPlugin::<(
    TermResizeEvent,
    TermMouseEvent,
    TermKeyEvent,
    TermLinkOpenRequest,
)>::for_hosts(&["terminal"]))
```

- [ ] **Step 4: Register the observer**

After `.add_observer(on_term_key)` (line ~379) add:

```rust
.add_observer(on_term_link_open)
```

- [ ] **Step 5: Write the `on_term_link_open` observer**

Add near `on_term_key` (after it, ~line 2884), mirroring its dispatch shape:

```rust
/// Open a URL/file the user cmd+clicked in the terminal, in a new stack beside
/// the current pane.
fn on_term_link_open(
    trigger: On<BinReceive<TermLinkOpenRequest>>,
    mut app_commands: MessageWriter<AppCommand>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let url = trigger.payload.url.clone();
    if url.is_empty() {
        return;
    }
    let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
        url: Some(url),
    }));
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    issued.write(vmux_command::CommandIssued {
        caller,
        command: cmd.clone(),
    });
    app_commands.write(cmd);
    if let Some(proxy) = proxy.as_ref() {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}
```

Add `TermLinkOpenRequest` and `TERM_LINK_OPEN_EVENT` to the `vmux_core::event::{…}` import (and confirm `BrowserCommand`, `OpenCommand`, `AppCommand` are already imported — they are, via `terminal_command_from_shortcut_id`).

- [ ] **Step 6: Add a system test for the dispatch**

In the `#[cfg(test)] mod tests` of `plugin.rs`, add:

```rust
#[test]
fn term_link_open_emits_browser_open_command() {
    use vmux_core::event::TermLinkOpenRequest;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AppCommand>()
        .add_message::<vmux_command::CommandIssued>()
        .add_observer(on_term_link_open);
    let user = app.world_mut().spawn(vmux_core::team::User).id();

    app.world_mut().trigger(BinReceive::<TermLinkOpenRequest> {
        webview: Entity::PLACEHOLDER,
        payload: TermLinkOpenRequest {
            url: "https://vmux.ai".into(),
        },
    });
    app.update();

    let msgs = app.world().resource::<Messages<AppCommand>>();
    let mut cursor = msgs.get_cursor();
    let found = cursor.read(msgs).any(|c| {
        matches!(
            c,
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: Some(u) }))
                if u == "https://vmux.ai"
        )
    });
    assert!(found, "expected InNewStack open command");
    let _ = user;
}
```

(Adjust the `BinReceive` construction / message-reading idiom to match the versions already used in this file's tests — check an existing observer test in `plugin.rs` for the exact `On`/`BinReceive` trigger shape and `Messages` cursor API, and follow it. The assertion target is the key part.)

- [ ] **Step 7: Run tests**

Run: `cargo test -p vmux_terminal`
Expected: new test + existing tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): annotate link ranges + open on cmd+click intent"
```

---

## Task 5: Page — cmd-hover highlight + cmd+click intent

**Files:**
- Modify: `crates/vmux_terminal/src/page.rs` (Page signals ~30; mouse handlers ~198–230; `TerminalRow` ~446–490)

- [ ] **Step 1: Add hover-link state to `Page`**

After the other `use_signal` declarations (~line 39) add:

```rust
// (row, start_col, end_col) of the link currently highlighted under a cmd-hover.
let mut hover_link = use_signal(|| None::<(u16, u16, u16)>);
```

- [ ] **Step 2: Find the link under a cell**

Add a helper near the other mouse helpers (~line 663). It reads the row's `links` from the rendered rows signal:

```rust
/// Find the link covering `(col, row)`, returning `(start_col, end_col, url)`.
fn link_at(rows: &Signal<Vec<Signal<TermLine>>>, col: u16, row: u16) -> Option<(u16, u16, String)> {
    let row_sig = rows.peek().get(row as usize).copied()?;
    let line = row_sig.peek();
    line.links
        .iter()
        .find(|l| col >= l.start_col && col <= l.end_col)
        .map(|l| (l.start_col, l.end_col, l.url.clone()))
}
```

- [ ] **Step 3: Open on cmd+left-down; otherwise current behavior**

Replace the `onmousedown` handler (lines 198–205) with:

```rust
onmousedown: move |e: Event<MouseData>| {
    e.prevent_default();
    focus_terminal_container();
    let dims = cell_dims();
    if let Some((col, row)) = mouse_to_cell(&e, padding, dims) {
        let mods = modifier_bits(&e);
        let is_left = trigger_button_id(&e) == 0;
        if is_left && mods & MOD_SUPER != 0
            && let Some((_, _, url)) = link_at(&rows, col, row)
        {
            let _ = try_cef_bin_emit_rkyv(&TermLinkOpenRequest { url });
            return;
        }
        emit_mouse(trigger_button_id(&e), col, row, mods, true, false);
    }
},
```

- [ ] **Step 4: Track hovered link on cmd-move**

In the `onmousemove` handler (lines 219–230), after `last_mouse_cell.set(...)` and before `emit_mouse(...)`, add cmd-hover tracking and skip the PTY motion forward while highlighting:

```rust
let mods = modifier_bits(&e);
if mods & MOD_SUPER != 0 {
    let next = link_at(&rows, col, row).map(|(s, end, _)| (row, s, end));
    if *hover_link.peek() != next {
        hover_link.set(next);
    }
    if next.is_some() {
        return; // hovering a link: don't forward motion to the PTY
    }
} else if hover_link.peek().is_some() {
    hover_link.set(None);
}
```

- [ ] **Step 5: Clear hover on mouse leave**

Add an `onmouseleave` handler on the container div (next to `onmouseup`):

```rust
onmouseleave: move |_| {
    if hover_link.peek().is_some() {
        hover_link.set(None);
    }
},
```

- [ ] **Step 6: Pointer cursor while hovering a link**

Where the container `style` is composed (the `style:` attribute at line ~196), append a pointer cursor when a link is hovered. Change the `cell_style` usage to include:

```rust
let cursor_css = if hover_link().is_some() { "cursor:pointer;" } else { "" };
```

and add `{cursor_css}` into the `style:` string for the container div.

- [ ] **Step 7: Render the underline overlay in `TerminalRow`**

Pass `hover_link` into `TerminalRow` (add a `hover_link: Signal<Option<(u16, u16, u16)>>` prop next to `selection` at line ~451, and pass it at the call site ~386). Inside `TerminalRow`, after the selection overlay (~line 482) add:

```rust
{
    let hl = hover_link();
    if let Some((hrow, hstart, hend)) = hl
        && hrow == row_idx
    {
        let w = hend - hstart + 1;
        rsx! {
            div {
                class: "absolute pointer-events-none",
                style: "left:calc(var(--cw, 1ch) * {hstart});width:calc(var(--cw, 1ch) * {w});bottom:0;height:1px;background:currentColor;",
            }
        }
    } else {
        rsx! {}
    }
}
```

- [ ] **Step 8: Typecheck the WASM page**

Run: `cargo check -p vmux_terminal --target wasm32-unknown-unknown`
Expected: compiles.

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_terminal/src/page.rs
git commit -m "feat(terminal): cmd-hover underline + cmd+click to open links"
```

---

## Task 6: Verify

- [ ] **Step 1: fmt + clippy + tests** (do not reformat `patches/`)

Run:
```bash
cargo fmt -p vmux_core -p vmux_terminal
git checkout -- patches/ 2>/dev/null || true
cargo clippy -p vmux_terminal --all-targets
cargo test -p vmux_terminal
```
Expected: clean.

- [ ] **Step 2: Manual pass (single pass, at the end)**

Build and run vmux. In both a plain terminal pane and an agent (vibe) pane:
- Print a URL (`echo https://vmux.ai/docs`) and an absolute path (`ls /Users`/`echo /etc/hosts`).
- Hold cmd → URL/path underlines, cursor becomes pointer.
- cmd+click → opens in a new browser stack beside (URL → web page; path → `file://` editor).
- Without cmd → selection/drag and mouse-reporting apps behave as before.

---

## Self-Review Notes

- **Spec coverage:** behavior (hover+click), detection host-side (Task 4), wire `LinkRange`/`TermLine.links` (Task 1), page render+intent (Task 5), open routing `InNewStack` (Task 4), tests (Tasks 3–4 + manual). All covered.
- **Host correction vs spec:** live terminals (incl. agent CLI) all load `vmux://terminal/`, so the emitter registers `for_hosts(&["terminal"])` only — not the extra hosts the spec listed. (Spec updated.)
- **Type consistency:** `LinkRange { start_col, end_col, url }`, `TermLine.links`, `TermLinkOpenRequest { url }`, `TERM_LINK_OPEN_EVENT` used identically across tasks.
- **v1 non-goal:** `cwd = None` at both patch sites → relative paths not linkified yet (documented).
