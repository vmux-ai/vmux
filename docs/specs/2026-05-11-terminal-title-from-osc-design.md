# Terminal tab title from OSC

**Date:** 2026-05-11
**Status:** Draft

## Summary

Make terminal tabs in vmux behave like normal CEF pages: when the running program changes the terminal title via OSC 0/2 (e.g. zsh's `precmd`/`preexec`, bash's `PROMPT_COMMAND`, vim's window-title support), the dioxus terminal app updates `document.title`, and the change flows back through CEF into `PageMetadata.title` so the tab bar reflects it.

Scope is title only. Favicon, background color, foreground-process polling, and per-command icon mapping are explicitly out of scope for this change.

## Motivation

Today, `PageMetadata.title` for a terminal entity is set exactly once — at PTY (re)spawn — to the literal string `"Terminal (xxxxxxxx)"` (`crates/vmux_desktop/src/terminal.rs:1879`). Nothing updates it afterwards.

`alacritty_terminal` already parses OSC title sequences and emits `TermEvent::Title(String)`. The current `ServiceEventProxy::send_event` in `crates/vmux_service/src/process.rs:30` only matches `TermEvent::PtyWrite` and silently drops `Title`. Most users' shells already emit useful OSC titles; we are throwing that signal away.

Treating the terminal webview like any other page (its dioxus app sets `document.title`, CEF surfaces it back via `WebviewChromeStateReceiver`, layout chrome writes `PageMetadata.title`) is the smallest change that gets reactive titles and keeps the architecture uniform with browser tabs.

## Architecture

### Data flow

```
alacritty Term::Title(s)            (OSC 0/2 from running program)
        |
        v
ServiceEventProxy::send_event       (crates/vmux_service/src/process.rs:30)
  matches Title(s); forwards to the per-process patch broadcaster
        |
        v
ServiceMessage::ProcessTitle { process_id, title }   (new variant)
  broadcast over the existing patch_tx channel
        |
        v
vmux_desktop terminal.rs            (existing service-message consumer)
  routes to the matching terminal entity, emits TermTitleEvent via
  BinHostEmitEvent to that entity's webview
        |
        v
vmux_terminal dioxus app            (crates/vmux_terminal/src/app.rs)
  use_bin_event_listener::<TermTitleEvent>(...) sets
  web_sys::window().document().set_title(title)
        |
        v
CEF                                 (browser-side title change)
  WebviewChromeStateReceiver fires WebviewChromeStateEvent { title: Some(...), ... }
        |
        v
apply_chrome_state_from_cef         (crates/vmux_layout/src/chrome.rs:41)
  writes meta.title (gate currently suppresses for vmux:// — must be relaxed
  for title; URL stays gated to preserve recent VMX-109 fix)
        |
        v
TabsHostEvent (existing emitter in browser.rs) -> tab bar UI updates
```

### Components touched

**`crates/vmux_terminal/src/event.rs`** — add the wire format used between native and the dioxus terminal webview:

```rust
pub const TERM_TITLE_EVENT: &str = "term_title";

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermTitleEvent {
    pub title: String,
}
```

**`crates/vmux_service/src/protocol.rs`** — add the service ↔ desktop variant:

```rust
ServiceMessage::ProcessTitle {
    process_id: ProcessId,
    title: String,
}
```

**`crates/vmux_service/src/process.rs`** — extend `ServiceEventProxy` so it carries the process id and a sender for `ServiceMessage`s (it currently only carries `pty_writer`). On `TermEvent::Title(s)`, broadcast `ProcessTitle { process_id, title: s }` on the existing `patch_tx`.

The proxy is constructed once per `Process` (see `Process::new` / equivalent), so it already knows its `ProcessId` at construction time — pass it in.

**`crates/vmux_desktop/src/terminal.rs`** — extend the existing `ServiceMessage` matcher (the loop near `ProcessCreated` at line 747) to handle `ProcessTitle`:

1. Look up the entity for `process_id` via the existing `ServiceProcessHandle` query.
2. If the webview is ready (`browsers.has_browser(entity) && browsers.host_emit_ready(&entity)`), trigger `BinHostEmitEvent::from_rkyv(entity, TERM_TITLE_EVENT, &TermTitleEvent { title })`.

No new system, no new resource — fits inside the existing service-loop pattern used for `ProcessCreated` and viewport patches.

**`crates/vmux_terminal/src/app.rs`** — add a third listener alongside the existing `TERM_VIEWPORT_EVENT` and `TERM_THEME_EVENT` listeners:

```rust
let _title_listener =
    use_bin_event_listener::<TermTitleEvent, _>(TERM_TITLE_EVENT, move |evt| {
        if let Some(window) = web_sys::window()
            && let Some(doc) = window.document()
        {
            doc.set_title(&evt.title);
        }
    });
```

Static title fallback (the `<title>` written into the bundled HTML) becomes irrelevant once the listener fires — the listener's first delivery overwrites it.

**`crates/vmux_layout/src/chrome.rs`** — `apply_chrome_state_from_cef` (line 41) currently does:

```rust
let owned_by_native_view = meta.url.starts_with("vmux://");
if let Some(url) = ev.url && !owned_by_native_view { meta.url = url; meta.favicon_url.clear(); }
if let Some(title) = ev.title && !owned_by_native_view { meta.title = title; }
if let Some(favicon) = ev.favicon_url { meta.favicon_url = favicon; }
```

Change to: keep the URL gate (preserves the VMX-109 fix where CEF chrome state was overwriting our `vmux://` URLs), but allow title to flow through unconditionally:

```rust
let url_owned_by_native_view = meta.url.starts_with("vmux://");
if let Some(url) = ev.url && !url_owned_by_native_view { meta.url = url; meta.favicon_url.clear(); }
if let Some(title) = ev.title { meta.title = title; }
if let Some(favicon) = ev.favicon_url { meta.favicon_url = favicon; }
```

Favicon was already ungated and stays that way.

### Initial title and "Terminal (xxxx)" fallback

Today, `meta.title` is set on (re)spawn to `"Terminal (xxxxxxxx)"` at `crates/vmux_desktop/src/terminal.rs:1879`. That assignment stays as the bootstrap value: it's what shows in the tab bar before the first OSC title arrives (e.g. before the shell prompts once). The first `ProcessTitle` event from the shell overwrites it.

Restart-PTY behavior is unchanged — the bootstrap string is reapplied, then the new shell's first OSC title takes over.

## Edge cases & non-goals

- **Shells that don't emit OSC titles.** Title remains the `"Terminal (xxxx)"` bootstrap. Acceptable for v1; a foreground-process poller is the planned follow-up but explicitly not part of this change.
- **OSC titles with shell escape codes / control characters.** Pass through verbatim; alacritty has already parsed and stripped the escape envelope, the payload is plain UTF-8.
- **Empty OSC title (`\e]0;\a`).** Pass empty string through; document title becomes empty. Tab bar shows whatever it shows for an empty title today (we don't introduce a minimum-string fallback in v1 — keeps behavior predictable).
- **Multiple rapid OSC updates.** Each becomes one `BinHostEmit`. The dioxus listener calls `document.set_title` on each; CEF coalesces if it likes. No debouncing in this change.
- **Restart PTY mid-session.** Existing `RestartPty` path resets the bootstrap title via the existing `meta.title = format!(...)` line; the next OSC title from the new shell overwrites it.
- **`ProcessTitle` arriving before the webview is ready.** Drop on the floor — the dioxus app will receive the next title once a frame paints. No queuing in v1. Most shells emit a title within the first prompt cycle so the latency is invisible. (If this turns out to be visibly broken, follow-up: cache last-known title per entity in the desktop side and replay on `UiReady`.)

Out of scope (each is its own follow-up):
- Foreground-process detection via `tcgetpgrp` + libproc.
- Per-command favicon and background-color mapping in `settings.ron`.
- Terminal-side icon scheme (`vmux://icons/...`) for embedded asset delivery.

## Testing

- **Unit test, `vmux_terminal::event`** — round-trip a `TermTitleEvent` through rkyv to confirm wire compatibility.
- **Unit test, `vmux_service::process`** — feed a synthetic `TermEvent::Title("hello")` into a `ServiceEventProxy` and assert a `ServiceMessage::ProcessTitle { title: "hello", .. }` lands on the broadcast receiver. (No real PTY needed; construct the proxy directly.)
- **Integration check, manual** — launch vmux, open a terminal pane in `zsh` or `bash`, run a command (`ls`, `vim`, `ssh somewhere`), confirm the tab title in the tab bar updates as the OSC sequences fire. Then run `printf '\e]0;custom title\a'` and confirm the tab updates to `custom title`.
- **Regression check, vmux:// URL preservation** — confirm that loading `vmux://terminal/<pid>` and waiting for the first OSC title does NOT overwrite the URL (only the title). This guards the recent VMX-109 fix.
- **Regression check, browser tabs** — open a normal `https://` page, confirm its title still updates as before.

## Migration / rollout

- No settings change.
- No protocol breaking change beyond an additive `ServiceMessage` variant; service and desktop are versioned together.
- No persisted state migration.
- Existing `"Terminal (xxxx)"` bootstrap behavior is unchanged for the moment between PTY spawn and the first OSC title.
