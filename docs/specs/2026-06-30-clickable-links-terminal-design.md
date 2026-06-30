# Clickable Links in Terminal & Agent Panes

**Date:** 2026-06-30
**Status:** Approved design

## Goal

Make URLs and file paths in terminal output behave like web links. Plain hover
highlights the link under the cursor; a plain click opens it. Applies to both
terminal panes and agent (vibe/codex) panes — they share
`vmux_terminal/src/page.rs`.

## Behavior (UX)

- **Hover** a `http`/`https` URL or file path → it underlines (in the theme accent
  `var(--primary)`) and the cursor becomes a pointer. No modifier needed.
- **Click** → opens it in a **new browser stack beside** the current pane
  (`OpenCommand::InNewStack`):
  - URLs (`http(s)://…`) → web page.
  - File paths → `file://<abs>` → the vmux `file://` editor.
- Hovering/clicking elsewhere is unaffected: only the link's own columns are
  interactive; selection and mouse-reporting work everywhere else.

> Note: an earlier draft gated this behind **cmd** (cmd-hover / cmd+click). That
> was dropped during implementation — plain hover/click is simpler and the
> shipped behavior. The cmd path proved unreliable because this webview's
> `mousemove` events carry an inconsistent modifier flag.

## Architecture

Dumb frontend: detection/logic/geometry live in the backend; the page only
renders pushed link ranges and emits a click intent.

### Detection (host-side)

The host already materializes `TermViewportPatch` from the service at two sites in
`crates/vmux_terminal/src/plugin.rs`:
- `ServiceMessage::ViewportPatch` (diff) — `plugin.rs:1306`
- `ServiceMessage::Snapshot` (full) — `plugin.rs:1369`

Both carry `changed_lines: Vec<(u16, TermLine)>`. A new module
`crates/vmux_terminal/src/link.rs` provides:

```rust
pub fn annotate_links(line: &mut TermLine, cwd: Option<&Path>);
```

It reconstructs the line text from `line.spans`, tokenizes on whitespace, trims
wrapping punctuation, classifies each token, resolves it to a final target URL,
and records the column range:
- URL → only tokens with an explicit scheme (`://`) or a `data:` URI; target is
  the token as-is. Bare domains are **not** autolinked in v1 (avoids reading
  filenames like `foo.txt` as `https://foo.txt`).
- Path → tested with `vmux_command::event::looks_like_path` (reused — the host
  already depends on `vmux_command`); **absolute** paths and `~/…` resolve to a
  `file://<abs>` target.

Column mapping handles wide (CJK/emoji) cells via each span's `col` plus unicode
display width, so highlight ranges line up with rendered cells.

`annotate_links` is applied to every changed line before the patch is built, at
both materialization sites.

### Wire format (data only, in `vmux_core`)

Add a link-range type and attach ranges to each line:

```rust
pub struct LinkRange {
    pub start_col: u16,
    pub end_col: u16, // inclusive
    pub url: String,  // ready-to-open: http(s):// or file://
}

pub struct TermLine {
    pub spans: Vec<TermSpan>,
    pub links: Vec<LinkRange>, // NEW; empty when no links
}
```

The service (`vmux_service`) always leaves `links` empty; the host fills it. No
`vmux_service → vmux_command` dependency is introduced.

### Page (render + intent only)

`crates/vmux_terminal/src/page.rs`: for each row, render one absolutely-positioned
overlay div per `LinkRange`, sized to the link's columns via `--cw`:

- **z-index:2** is required — `render_span` renders the text spans at `z-[1]`, so an
  auto-z overlay would paint behind the glyphs and receive no pointer events
  (renders but is dead to hover/click).
- Hover highlight + pointer cursor come from **native CSS `:hover`** on the overlay
  (`cursor:pointer` inline; underline via an injected stylesheet rule
  `.vmux-link:hover{border-bottom:2px solid var(--primary)}`). CSS `:hover` is used
  instead of a JS-tracked hover signal because the signal approach kept
  flickering/clearing on this webview; CSS hover stays put while the pointer is
  still. Tailwind `hover:`/`group-hover` variants did not reliably generate for
  this page, hence the injected rule.
- `onclick` emits `TermLinkOpenRequest { url }`; `onmousedown` only
  `stop_propagation`/`prevent_default` so the open fires on click completion and
  the terminal underneath doesn't start a selection.

The page performs **no** URL/path detection — it only consumes pushed ranges.

### Open routing (host)

New event (rkyv + serde + `const TERM_LINK_OPEN_EVENT: &str`), registered with the
bin emitter `for_hosts(&["terminal"])` — same as `TermMouseEvent`. (Live terminals,
including agent CLI panes, all load `vmux://terminal/`, so the `"terminal"` host
covers both; `vmux://agent/…` is only the setup/Page-agent placeholder.)

```rust
pub struct TermLinkOpenRequest { pub url: String }
```

Host observer `on_term_link_open(trigger: On<BinReceive<TermLinkOpenRequest>>, …)`:
- Resolve the terminal entity from `trigger.event().webview` (same resolution as
  `on_term_mouse`).
- Write `AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
  url: Some(url) }))`, a matching `CommandIssued`, and wake the loop with
  `WinitUserEvent::WakeUp` — same dispatch shape as the web shortcuts in
  `on_term_key`.

## Non-goals (v1)

- **OSC 8 explicit hyperlinks** — agents print raw URLs; defer. (alacritty exposes
  `cell.hyperlink()` for a later pass.)
- **Relative-path resolution** — not in v1. The host passes `cwd = None`, so only
  absolute paths (and `~/…`) are linkified; relative paths like `crates/foo.rs`
  are left as plain text until terminal-cwd tracking is plumbed in a follow-up.
- **Bare-domain autolinking** — not in v1 (URLs require an explicit scheme).
- No new crates.

## Testing

Native tests (per "verify observable behavior" + "finish then test" — one manual
pass at the very end):

- **Unit (`link.rs`):** `annotate_links` over crafted `TermLine`s →
  - single scheme URL, single absolute path, multiple links on one line;
  - wrapping punctuation trimmed; absolute path gets `file://` + absolute;
  - relative path skipped when `cwd` is `None`;
  - wide-char column alignment;
  - no false positives on bare words / prose / bare filenames (`foo.txt`).
- **System (host):** send `TermLinkOpenRequest { url }` → assert
  `AppCommand::Browser(Open(InNewStack { url }))` is written (and the resulting
  `OpenInNewStackRequest`). Register the event + observer in the plugin's
  `build()` so the test exercises the production wiring.
- **Manual (end):** cmd-hover highlight + cmd+click for a URL and a file path, in
  both a terminal pane and an agent pane.

## Files touched

- `crates/vmux_core/src/event.rs` — `LinkRange`, `TermLine.links`.
- `crates/vmux_terminal/src/link.rs` — NEW: detection + tests.
- `crates/vmux_terminal/src/plugin.rs` — annotate at both patch sites; new event +
  `on_term_link_open` observer; registration in `build()`.
- `crates/vmux_terminal/src/page.rs` — store ranges, cmd-hover highlight, cmd+click
  intent.
- Event def (rkyv/serde + EVENT id) — alongside existing terminal events.
