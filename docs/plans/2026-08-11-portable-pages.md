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

### Step 2b — scroll and debounce (`416a40f8`)

Six identical `scroll_into_view` calls, all agreeing on `Nearest`, collapsed onto the seam that
already existed as `Host::scroll_item_into_view`. It now returns whether the element was found,
because `vmux_layout/page.rs` latches "already scrolled" only on success — collapsing that to an
unconditional call looks like a simplification and silently breaks the retry.

`MountedData::scroll_to` was the plan's answer and is the wrong one: it needs `onmounted` on the
row being revealed, but every caller looks the row up by generated id *because* it is whichever
row is selected.

palette's debounce moved to `spawn` + `platform::sleep_ms`, which is what step 1 was for. The old
`Closure::once_into_js` is why cancelling had to *call* the callback — invoking a once-closure is
how it is freed.

**Tiers 1 and 2 are now complete.** palette is down from 55 DOM references to 42.

### Step 3a — the readline edits, as arithmetic (`979af958`)

First half of Tier 3, and non-behavioural apart from two bug fixes it exposed.

`CtrlKeyCapture::RerouteToDioxus` turned out never to be constructed:
`ctrl_key_capture_for_code` is its only possible producer and never returns it. So palette's
reroute arm, `dispatch_ctrl_keydown`, `key_for_code` and the `_vmuxSyntheticKeydown` check were
all unreachable, as was `ignore_physical_rerouted_ctrl_keydown` — whose test asserted `!false`
six times and had no oracle. Deleted.

The eight edit arms were arithmetic interleaved with element calls, so none of it could be
tested. `CtrlEditAction::apply(value, caret, ghost) -> Edited` is the arithmetic;
`apply_ctrl_edit` is the remainder that needs the element.

**Two bugs the extraction made visible**, both in the case the plan already told the user to
test — a non-ASCII query:

- The caret arrives in UTF-16 code units. `Forward`/`Back` converted it; the four deletions used
  it as a byte offset directly. On `"aé本b"` with the caret after `本`, Ctrl+D deleted `本`.
- Writing back had the mirror problem: `set_selection_range` was handed byte offsets. Hence the
  new `byte_offset_to_utf16`, tested against its inverse across surrogate pairs.

palette: 42 → 33 DOM references, 161 lines shorter.

### Step 3b — the caret seam (`ac4d6155`)

`vmux_ui::caret::TextCaret`, shaped like `FocusClaim`. Byte offsets in and out, so UTF-16 stops
at the boundary instead of leaking into callers. `place()` sets the caret *and* scrolls to it,
because a programmatic move bypasses Chromium's caret-follow and every caller already did both;
the canvas text measurement moves with it.

`caret_scroll_left` and the UTF-16 conversions moved `vmux_start` → `vmux_ui` as predicted below,
tests included. `vmux_start::keyboard` keeps the chord map and the edit arithmetic.

One behavioural difference, deliberate: opening the bar rewinds the field to the start of the
text so a long URL reads as an offer to overtype. Plain Cmd+A never did and still does not — that
is why `select_all` did not absorb both callers.

palette: 33 → 25 DOM references.

## Left

Only Tier 3's last step, plus consistency work of doubtful value (see Order).

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

Porting this is the wrong goal. Making the command-bar input a controlled Dioxus component is
what makes it portable, as a consequence. The diff is behavioural and needs the user at a
keyboard.

**Refinement after finishing Tiers 1–2.** The input's `value` is *already* controlled
(`palette.rs:1344`). What is imperative is everything around it: caret position, selection, and
Ctrl-key editing, which is done by dispatching synthetic events rather than updating the signal.

This matters because there is **no portable Dioxus API for caret or selection control** — unlike
title, scroll and focus, there is nothing to substitute in. So Tier 3 cannot be "use the portable
API". It is two changes at once:

1. ~~The edit arithmetic, lifted out of the DOM calls.~~ **Done in 3a.**
2. ~~A caret/selection seam in `vmux_ui`, in the shape of `FocusClaim`.~~ **Done in 3b.**
3. The value written through the query signal rather than to the element, which is what finally
   removes `dispatch_input_event` and the capture-phase listener.

All 25 DOM references left in palette are in five places, and only two kinds of thing:

| what | dies how |
|---|---|
| `focus_and_install_ctrl_bindings` — capture-phase listener, `_ctrlBound` latch | (3) |
| `apply_ctrl_edit`'s `set_value`, `dispatch_input_event` | (3) |
| `accept_completion`'s element lookup and `set_value` | (3) |
| `handle_plain_meta_a` — takes a `web_sys::KeyboardEvent` | (3), with the listener |
| `install_start_menu_click_outside` | unrelated: popup dismissal, not input |

**Why the listener is capture-phase, which (3) has to answer.** Chromium's own macOS readline
emulation acts on Ctrl+A/E before a bubble-phase Dioxus `onkeydown` would see the key, so the
handler has to run first to preempt it. Moving to `onkeydown` is not a like-for-like swap; it has
to be shown that Dioxus's handler still wins.

(3) is the behavioural half and the reason a user has to sit with it: Ctrl+A/E/F/B/W/K/U in the
command bar, on a long URL, with a non-ASCII query, plus Cmd+L select-all-on-open. It also has an
ordering problem that cannot be designed from reading alone — placing the caret after
`query.set(..)` has to happen after Dioxus re-renders, or the render clobbers it.
`TextCaret::select_all_from_start_next_frame` already carries a version of that constraint and is
the place to grow whatever (3) needs.

## Order

1. ~~`vmux_ui`: `platform::sleep_ms` and one focus helper.~~ **Done.**
2. ~~Tier 1 title substitutions.~~ **Done**, plus four pages flipped to `ui`.
3. ~~`palette` Tiers 1–2, then flip `palette` and `start::page` to `cfg(ui)`.~~ **Premise was
   wrong.** Tier 1–2 leaves 45 of palette's 55 references standing. The launcher reaches the phone
   only after Tier 3. Re-scope as: make the command-bar input controlled, then flip both.
4. Tier 3: palette's input, then `vmux_editor`. **This is the only remaining step with a payoff.**
5. ~~`vmux_layout`: `error_page`, `extensions_page`, `vault_page`.~~ **Probably not worth doing.**
   Each is one or two DOM calls, but ask what the flip buys: the phone never navigates to
   `vmux://error/` and has no extension manager, so making those two `cfg(ui)` would make them
   compile somewhere they will never render. `vault_page` (8 calls) is the only one that plausibly
   belongs on a phone. Do that one if a reason appears; leave the others marked `web`, which is
   what they honestly are.
6. ~~`vmux_terminal`.~~ Same question, more strongly: a terminal grid and a canvas rain effect are
   not phone features.

The original order had 4–6 as "consistency". Having finished Tiers 1–2, that consistency looks
like busywork — `cfg(web)` on a page that only ever renders in CEF is not a defect, it is an
accurate label. The defect was only ever pages that *could* be portable and were not.

## Watch for

- `vmux_chat`'s `TabIdentity` sets a favicon as well as a title. `document::Title` and
  `document::Link` both exist, so it is convertible, but converting only the title half would be
  worse than leaving it. It is already a clean `cfg(web)` seam on a type and blocks nothing.
- Source-scanning tests break on exactly this kind of change. Two have already been removed
  (`page_mount_does_not_start_focus_retry`, the `set_title` assertion in `page_source.rs`).
- `not(target_arch = "wasm32")` is correct in `vmux_chat`, `vmux_profile`, `vmux_remote` and
  `vmux_ui` — those blocks hold genuinely native dependencies the phone needs (`tokio`, `quinn`,
  `sys-locale`). They are commented as deliberate. Do not "fix" them to `host`.
