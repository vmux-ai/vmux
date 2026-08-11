# Making the Dioxus pages portable

A page should not know what is hosting it. Today six of them do, and they are marked `cfg(web)`
to say so. This is the sweep that removes the marks.

Scope is ~330 `web_sys`/`wasm_bindgen`/`js_sys` references across 25 files. Not all of it is the
same problem, and the tiers below matter more than the count: most of it is not "port this to
iOS", it is "this should not have been written against the DOM in the first place".

## The template already exists

Three places solved this before and are the reference:

- `vmux_ui::components::prompt_composer::focus_prompt_end` — a `cfg(web)` body and an inert
  `cfg(not(web))` twin. Callers never branch.
- `vmux_ui::transport::{cef, native}` — one module per target, chosen at compile time, behind a
  trait the page uses without knowing which it got.
- `vmux_layout::start::focus::StartFocus` — the same shape, done in this branch.

`vmux_chat` is the proof it works: `page/scroll.rs` holds 7 DOM references and the chat page
still renders on the phone.

## Tiers

### Tier 1 — delete, do not port

A portable Dioxus API already exists. These are not seams waiting to be written.

| pattern | replacement | sites |
|---|---|---|
| `document.set_title(x)` | `document::Title { x }` | editor ×4, terminal, agent, lsp_page |
| `el.scroll_into_view_with_*` | `MountedData::scroll_to` (`mounted` is on workspace-wide; `onmounted` already used at `vmux_editor/src/page.rs:3560`) | palette ×2, editor, chat |
| `window.set_timeout` debounce | `spawn` + portable sleep | palette (`HostSearchTimer`) |

The sleep wants to be `vmux_ui::platform::sleep_ms`, moved from `vmux_chat::platform` so
`vmux_layout` can reach it without depending on `vmux_chat`. It is already the right shape:
`gloo_timers` on web, `tokio` off it.

### Tier 2 — genuine host compensation, needs a seam

CEF takes keyboard focus a frame or more after mount, so the caret has to be re-asserted. That is
a fact about the host. There are currently **three** independent copies of the retry loop —
`start/focus.rs`, `palette.rs:1602`, and `prompt_composer.rs:239`. They should be one thing in
`vmux_ui`, inert off the browser.

### Tier 3 — not a porting problem

`palette.rs:1570-1970` (~400 lines) drives its input imperatively: synthetic `keydown` dispatch,
a `_ctrlBound` latch on the element, canvas 2D text measurement to scroll the caret, manual
`input` event dispatch. `vmux_editor/src/page.rs` (93 references) is the same shape.

Porting this is the wrong goal. The state lives in the DOM instead of in Dioxus, and moving it
into signals is what makes it portable — as a consequence, not as the aim. This tier should be
scoped on its own once Tiers 1 and 2 are done, because the diff will be behavioural and needs
the user at a keyboard to accept it.

## Order

Bottom-up, so each step lands green and nothing waits on a later one.

1. `vmux_ui`: `platform::sleep_ms`, and one focus-retry helper replacing the three copies.
2. Tier 1 substitutions everywhere. Mechanical, no behaviour change.
3. `command_bar::palette` Tiers 1–2; then flip `palette` and `start::page` to `cfg(ui)`.
   **The launcher renders on the phone at this point** — verified by experiment: flipping
   `start::page` alone today yields exactly one error, and it is `palette`.
4. `vmux_layout`: `vault_page`, `extensions_page`, `tools_page`, `debug_page`, `error_page`
   (2, 8, 1, 1, 2 references — small).
5. `vmux_terminal`: `page.rs` (18), `matrix_rain.rs` (11, canvas — may stay `web`, a terminal
   grid and a canvas rain effect are not obviously phone features).
6. Tier 3, scoped separately: `palette` input machinery, then `vmux_editor`.

Step 3 is the one with a user-visible payoff. Steps 4–6 are consistency.

## Verification

Every step: `cargo check` on all three targets (native `--workspace --all-targets`; `-p
vmux_server --target wasm32-unknown-unknown --features web`; `-p vmux_mobile --target
aarch64-apple-ios --features mobile --bin vmux_mobile`), plus clippy at zero and the full test
run.

The load-bearing check is that a page's module gate can move from `cfg(web)` to `cfg(ui)` and the
iOS build stays green. That is the only thing that actually proves portability; compiling the
crate does not, because a `cfg(web)` module is simply absent.

Steps 1, 2 and 4 are behaviour-preserving on the desktop and the existing tests are the oracle.
Steps 3, 5 and 6 change how input and scrolling are driven and **need the user to run the app** —
typing in the launcher, the caret landing without a click, `Ctrl+n`/`Ctrl+p` through the results,
and clicking the hero background without losing focus.

## Watch for

- A source-scanning test broke on the `StartFocus` rename in this branch
  (`page_mount_does_not_start_focus_retry`, `include_str!` + `contains`). `vmux_editor` has
  another: `tests/page_source.rs:80` asserts `page.rs` contains `document.set_title(&title)`,
  which Tier 1 deletes by design. Expect to remove it rather than keep it passing.
- `not(target_arch = "wasm32")` is correct in `vmux_chat`, `vmux_profile`, `vmux_remote` and
  `vmux_ui` — those blocks hold genuinely native dependencies the phone needs (`tokio`, `quinn`,
  `sys-locale`). They are commented as deliberate. Do not "fix" them to `host`.
