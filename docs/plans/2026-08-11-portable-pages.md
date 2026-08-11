# Making the Dioxus pages portable

A page should not know what is hosting it. The ones that do are marked `cfg(web)` to say so. This
is the sweep that removes the marks.

Not all of it is the same problem, and the tiers matter more than the count: most of it is not
"port this to iOS", it is "this should not have been written against the DOM in the first place".

## Verification, and one trap

Every step: `cargo check` on all three targets (native `--workspace --all-targets`; `-p
vmux_server --target wasm32-unknown-unknown --features web`; `-p vmux_mobile --target
aarch64-apple-ios --features mobile --bin vmux_mobile`), plus clippy at zero and the full test run.

The load-bearing check is that a page's module gate can move from `cfg(web)` to `cfg(ui)` and the
iOS build stays green. Compiling the crate proves nothing on its own, because a `cfg(web)` module
is simply absent.

**The trap, found the hard way in step 2:** `cargo check -p vmux_mobile --target aarch64-apple-ios`
does not exercise `vmux_layout`, `vmux_editor` or `vmux_setting` — `vmux_mobile` does not depend on
them. It went green while `vmux_editor` for iOS was broken. **Check each crate against the iOS
target directly.**

The second half is the manifest. A crate whose page becomes `cfg(ui)` must declare `dioxus` and
`vmux_ui` for iOS as well as wasm, spelling the alias out as
`cfg(any(target_arch = "wasm32", target_os = "ios"))` — Cargo resolves dependencies before any
build script runs, so a manifest cannot use `ui` itself. `vmux_agent` had the pattern first.

## Done

### Step 1 — the shared seams (`8ed87d3a`, `bace6c8d`)

`vmux_ui::platform` gained `sleep_ms` and `random_index`, moved out of `vmux_chat` so
`vmux_layout` can reach a timer without depending on chat. Collapsed a third private
`random_index` in `prompt_ghost`.

`vmux_ui::focus::FocusClaim` replaced the CEF focus-retry loop. This said **three** copies; there
were **two**, `start/focus.rs` and `palette.rs`. `prompt_composer`'s `focus_prompt_end` is a
one-shot with 28 call sites — already shared, unchanged. `command_bar/page.rs` retries an event
*send*, not a focus.

The two disagreed on scheduling (rAF vs a 16ms timer) and the launcher's copy placed the caret
with a byte count where `set_selection_range` wants UTF-16 code units. Unified on the timer,
because rAF is least reliable exactly when the claim is most needed — before the host produces
frames. **Not behaviour-preserving; wants eyes on a cold start.**

### Step 2 — `document::Title` (`50d48240`)

Twelve sites, not the seven estimated: it missed `vmux_chat`, `vmux_setting` and four
`vmux_layout` pages. Other `set_title` hits in the tree are native rfd dialogs, out of scope.

Four pages had nothing else holding them to the DOM and moved to `cfg(ui)` — **settings, tools,
debug and the LSP manager now build for the phone**, each verified against the iOS target
directly. `vmux_editor` and `vmux_setting` needed the manifest fix above.

Deleted the `page_source.rs` assertion on `set_title`, as predicted.

## Left

### Tier 1 — a portable API exists

| pattern | replacement | sites |
|---|---|---|
| `window.set_timeout` debounce | `spawn` + `platform::sleep_ms` | palette (`HostSearchTimer`) |

`el.scroll_into_view_with_*` (6 sites: palette ×2, editor ×2, `vmux_layout/page.rs`,
`transport/cef.rs`) **was listed here and does not belong**. `MountedData::scroll_to` needs a
`MountedData` handle, so it needs `onmounted` on the row being scrolled to — but every one of
these looks the target up by generated id (`command-bar-item-{n}`) precisely because it is
whichever row is selected. That is a restructure per call site, or one `vmux_ui` seam in the shape
of `FocusClaim`. Decide which; it is not a substitution.

### Tier 2 — genuine host compensation

Done (`FocusClaim`). Nothing else has turned up.

### Tier 3 — not a porting problem

**This is the blocker, and step 3 below was wrong about that.**

`palette.rs` holds 55 DOM references. **33 of them are in lines 1700-1920** and Tier 1 would
remove none of them:

- `ensure_caret_visible` + `caret_metrics` + `css_px` — canvas 2D text measurement to scroll the
  caret, because programmatic caret moves bypass Chromium's native caret-follow
- `key_for_code` + `dispatch_ctrl_keydown` + `is_vmux_synthetic_keydown` — synthetic `keydown`
  dispatch, with a `_ctrlBound` latch stored on the element
- `raw_selection_start`, `select_all_on_open` — reading and writing the input's selection directly
- `dispatch_input_event` — fires a synthetic `input` event **so Dioxus notices a change the code
  made behind its back**. That one line is the diagnosis: the input's state lives in the DOM, not
  in Dioxus.

`vmux_editor/src/page.rs` (93 references) is the same shape.

Porting this is the wrong goal. Making the command-bar input a controlled Dioxus component —
caret, selection and scroll as signals — is what makes it portable, as a consequence. The diff is
behavioural and needs the user at a keyboard.

## Order

1. ~~`vmux_ui`: `platform::sleep_ms` and one focus helper.~~ **Done.**
2. ~~Tier 1 title substitutions.~~ **Done**, plus four pages flipped to `ui`.
3. ~~`palette` Tiers 1–2, then flip `palette` and `start::page` to `cfg(ui)`.~~ **Premise was
   wrong.** Tier 1–2 leaves 45 of palette's 55 references standing. The launcher reaches the phone
   only after Tier 3. Re-scope as: make the command-bar input controlled, then flip both.
4. `vmux_layout`: `error_page`, `extensions_page`, `vault_page` — one DOM call each for the first
   two, eight for vault. Error and vibe-setup read parameters out of the URL and extensions calls
   `window.confirm()`; both want a seam, not a substitution.
5. `vmux_terminal`: `page.rs` (18), `matrix_rain.rs` (11, canvas — may stay `web`; a terminal grid
   and a canvas rain effect are not obviously phone features).
6. Tier 3: palette's input, then `vmux_editor`.

## Watch for

- `vmux_chat`'s `TabIdentity` sets a favicon as well as a title. `document::Title` and
  `document::Link` both exist, so it is convertible, but converting only the title half would be
  worse than leaving it. It is already a clean `cfg(web)` seam on a type and blocks nothing.
- Source-scanning tests break on exactly this kind of change. Two have already been removed
  (`page_mount_does_not_start_focus_retry`, the `set_title` assertion in `page_source.rs`).
- `not(target_arch = "wasm32")` is correct in `vmux_chat`, `vmux_profile`, `vmux_remote` and
  `vmux_ui` — those blocks hold genuinely native dependencies the phone needs (`tokio`, `quinn`,
  `sys-locale`). They are commented as deliberate. Do not "fix" them to `host`.
