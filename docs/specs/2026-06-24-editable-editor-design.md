# Editable Editor — Design

Date: 2026-06-24
Branch: `feat/editable-editor` (off `main`, post LSP merge #157)
Status: approved design, pre-plan

## Summary

Make the `file://` editor editable. Today it is a read-only, virtualized,
syntax-highlighted viewer: the native side (Bevy) owns the buffer and syntect
highlighting, and the WASM page (Dioxus → DOM) renders a windowed slice of
highlighted lines. This adds writing: a native-authoritative edit core on a
rope, two interchangeable keymaps (Vim + VSCode-default, chosen in settings),
IME input, selections, clipboard, undo/redo, and save — while preserving the
existing native-authoritative / virtualized / native-syntect / native-LSP
architecture.

## Goals (v1)

- Insert / delete / newline / tab editing of text files.
- Motions: `Left/Right/Up/Down`, word next/prev/end, line start / first-non-blank
  / end, doc start/end, page up/down, goto-line.
- Selections: Vim visual + visual-line; VSCode shift-select.
- Clipboard: yank/cut/paste via the system clipboard (native).
- Undo / redo.
- Save to disk, dirty state, title dirty marker.
- **IME composition** (Japanese/CJK, dead keys, accents).
- **Two keymaps**, selectable via `settings.ron` `editor.keymap` (`vscode` |
  `vim`), default `vscode`.
- Click / drag to place / extend the cursor.
- Cursor auto-scroll (cursor can leave the virtualized window).
- Live diagnostics: LSP `didChange` (debounced) so squiggles refresh while
  editing.

## Non-goals (later)

LSP completions / hover-actions / signature help; format-on-save; multi-cursor;
in-file search/replace; Vim macros, named registers, ex-commands beyond
`:w`/`:q`; autosave; editing files > 5 MB; minimap integration (separate
branch).

## Decisions (locked during brainstorming)

- **Approach: native-authoritative edit core on `ropey`.** Hand-roll the thin
  editing layer + both keymaps; treat Zed/helix/lapce as reference, not deps.
  - *Why not Zed:* its `editor`/`vim` are welded to `gpui` (a whole UI
    framework) — not a library; its `text` buffer is a CRDT for collab; only its
    rope is liftable and `ropey` already is that, cleaner and published.
  - *Why not helix-core:* it is Kakoune selection-first (not Vim), and drags a
    semver-unstable, tree-sitter-laden dep tree against our bevy-rc/CEF build.
  - *Why not page-authoritative (CodeMirror/contenteditable):* a JS dependency,
    duplicate buffer, abandons native syntect + native-authoritative model.
  - **No Rust lib provides Vim bindings** (helix ≠ vim; Zed vim is gpui-bound;
    CM vim is JS). The Vim modal state machine is hand-rolled work we own
    regardless of rope choice — so the editing layer's only value is rope +
    selection + transaction plumbing, a few hundred testable lines on `ropey`.
- **IME in v1** → the page must host a focused hidden `<textarea>` to capture
  `compositionstart/update/end` + `beforeinput`. Raw keydown forwarding cannot
  do IME.
- **Keymap selectable via settings**, absent key → `vscode` default (honors the
  no-auto-seed rule: absent == global fallback).

## Architecture

Authority stays native. `EditCore` (rope + cursor + mode + undo) is the single
source of truth, one per `FileView` entity. The page is a view + input sensor:
it owns a hidden `<textarea>` for IME/keys and renders caret/selection over the
existing virtualized line window. No buffer in the page.

Both keymaps emit the **same `EditCommand` vocabulary**, so `EditCore` is
keymap-agnostic — Vim and VSCode are two translators over one core. This is the
central simplification: the hard logic is written and tested once.

The **mode hint** is the key page↔native coupling. `FileCursorEvent.mode` tells
the textarea controller how to route a key: in Vim Normal/Visual a plain `d` is
a *command*; in Insert/VSCode a plain `d` is *text* (IME/`input` composes it).
Without the hint the page cannot tell text from command.

### The keystroke loop

```
page (textarea)                     native (Bevy, per FileView)
  keydown (cmd / non-text) ─FileKeyEvent──▶ Keymap.handle ─▶ Vec<EditCommand>
  input / compositionend   ─FileTextInput─▶ (InsertText)   ─┘        │
  click / drag             ─FilePointerEvent▶ (place/extend cursor)  ▼
                                                            EditCore.apply
                                                            ├─ mutate rope
                                                            ├─ move cursor / sel
                                                            ├─ push undo, mark dirty
                                                            ├─ HighlightCache.invalidate_from(line)
                                                            └─ LSP didChange (debounced)
  render ◀─FileViewportPatch (highlighted window) ─────────┤
  caret  ◀─FileCursorEvent {mode, mode_label, sels, primary}┤
  dot    ◀─FileDirtyEvent {dirty} ────────────────────────┘
```

## Wire protocol (`crates/vmux_core/src/event.rs`)

Inbound (page → native):

- `FileTextInput { text: String }` — committed text: typing, IME
  `compositionend`, paste-as-text. Composition *preview* shows in the overlay
  textarea itself; only the committed result crosses the wire.
- `FileKeyEvent { key: String, code: String, mods: KeyMods, repeat: bool }` —
  non-text keys and all chords. `KeyMods { ctrl, alt, shift, meta }`.
- `FilePointerEvent { line: u32, col: u32, extend: bool }` — click-to-place /
  drag-extend.

Outbound (native → page):

- `FileViewportPatch` — **unchanged** (highlighted visible window).
- `FileCursorEvent { mode: EditMode, mode_label: String, selections:
  Vec<SelSpan>, primary: CursorPos }` — `SelSpan`/`CursorPos` carry **visual
  columns** (native computes display width via `unicode-width`) so the monospace
  page does `x = col*cw`, `y = (line - first_line)*ch`.
- `FileDirtyEvent { dirty: bool }` — title/header dot.
- `FileExternalChange { path: String }` — file changed on disk while the buffer
  is dirty (don't clobber; show a banner).

Reused as-is: `FileMetaEvent`, `FileScrollEvent`, `FileResizeEvent`,
`FileDiagnosticsEvent`, `FileLspStatusEvent`, dir/image/preview events.

All new types derive the existing `serde` + `rkyv` set, with `FILE_*_EVENT`
string constants, matching the current contract.

## Native edit core (`crates/vmux_editor`, non-wasm)

Module layout (filename-module pattern, no `mod.rs`):

```
src/edit.rs                  // re-exports
src/edit/buffer.rs           // TextBuffer on ropey
src/edit/command.rs          // EditCommand, Motion, EditMode, Selection, CursorPos
src/edit/core.rs             // EditCore: apply(), undo, clipboard, dirty, autoscroll
src/edit/highlight_cache.rs  // incremental syntect (reuses highlight.rs syntax/theme pick)
src/keymap.rs                // Keymap trait, KeymapKind, KeyInput, Mods
src/keymap/vim.rs            // VimKeymap (modal)
src/keymap/vscode.rs         // VscodeKeymap (non-modal)
```

**Buffer.** `ropey::Rope` is the authority. Char offsets internally;
`unicode-segmentation` for grapheme-correct cursor left/right and word motions;
`unicode-width` for the visual columns sent to the page.

**Verb vocabulary** (shared by both keymaps):

```rust
enum Motion {
    Left, Right, Up, Down, WordNext, WordPrev, WordEnd,
    LineStart, FirstNonBlank, LineEnd, DocStart, DocEnd,
    PageUp, PageDown, GotoLine(u32),
}

enum EditCommand {
    Move(Motion), Select(Motion),          // core extends iff Select OR mode == Visual
    InsertText(String), InsertNewline, InsertTab,
    DeleteBack, DeleteForward, DeleteWordBack, DeleteToLineEnd,
    DeleteRange(Motion), YankRange(Motion), // Vim operators: dw = DeleteRange(WordNext)
    DeleteSelection, DeleteLine,
    Yank, Cut, Paste, PasteBefore,          // clipboard + register (tracks linewise)
    SetMode(EditMode),                      // Esc / i / v / V; a,o,O composed in keymap
    Undo, Redo, Save,
}
```

Vim operators (`dw`, `cc`, `yy`, `3dd`) and insert-entry variants (`a`, `o`,
`O`) resolve **in the keymap** to sequences of these — e.g. `cw` →
`[DeleteRange(WordNext), SetMode(Insert)]`, `a` → `[Move(Right),
SetMode(Insert)]`. Counts expand in the keymap. The core stays general.

**`EditCore`** (component per `FileView`) holds `TextBuffer`, `Vec<Selection>`
(single primary in v1; `Vec` to ease future multi-cursor), `EditMode`, an undo
stack, a clipboard register, and `dirty`. `apply(cmd) -> EditOutcome {
text_changed, sel_changed, mode_changed, dirty_changed, scroll_to: Option<u32>
}` drives emits, LSP, and autoscroll.

**Undo is cheap via rope structural sharing.** `ropey::Rope::clone` is ~O(1)
(Arc-shared B-tree nodes), so an undo entry is a `(Rope, Vec<Selection>)`
snapshot, coalesced into groups (a typing run = one undo). Vim `u` / `Ctrl-r`;
VSCode `Cmd-Z` / `Cmd-Shift-Z`.

**`HighlightCache`** solves syntect's stateful-from-top highlighting. It keeps
`Vec<(ParseState, HighlightState)>` — the parser state after each line.
`invalidate_from(line)` truncates that vector; `line_window(rope, a..b)` resumes
from the nearest cached state and highlights only the visible slice. Per
keystroke this is O(edit→window), not O(file). It reuses the syntax + theme
selection already in `highlight.rs` (refactor that selection into a shared
helper).

## Keymaps (`crates/vmux_editor`, non-wasm)

```rust
struct Mods { ctrl: bool, alt: bool, shift: bool, meta: bool }
struct KeyInput { key: String, mods: Mods, repeat: bool }

trait Keymap {
    fn handle(&mut self, k: KeyInput) -> Vec<EditCommand>;
    fn mode(&self) -> EditMode;
    fn mode_label(&self) -> String;   // "NORMAL"/"INSERT"/"VISUAL"; "" for vscode
}
```

- **`VimKeymap`** — internal state: mode, pending count, pending operator,
  `g`-prefix, last-find. v1 set: `h j k l w b e 0 ^ $ gg G`, `i a o O I A`,
  `x dd cc yy dw cw de p P`, `v V`, counts (`3j`, `d2w`), `u`/`Ctrl-r`,
  minimal ex `:w` / `:wq` / `:q`, `Esc`.
- **`VscodeKeymap`** — near-stateless: arrows (Shift → `Select`),
  `Cmd/Ctrl C/V/X/Z/Y/A/S`, `Home`/`End`, `Alt`/`Ctrl`+arrow word-jump,
  `Backspace`/`Delete` (+ word), `Enter`, `Tab`.

Selected by `editor.keymap`: new `EditorSettings { keymap: KeymapKind }` in
`crates/vmux_setting/src/plugin/runtime.rs` (sibling of `TerminalSettings`),
serde `default` = `Vscode`. Stored as a component on the `FileView`; rebuilt if
settings change.

Both keymaps produce the same `EditCommand`s, so they are tested by feeding key
sequences and asserting both the emitted commands and the resulting buffer.

## Plugin wiring (`crates/vmux_editor/src/plugin.rs`)

- `load_file_buffers`: for text files, insert `EditCore` (rope from content) +
  `HighlightCache` + the selected keymap, replacing `FileBuffer` as the text
  authority. Dir / image / `__error__` paths unchanged. `emit_window` pulls the
  highlighted slice from `HighlightCache.line_window(rope, slice)`;
  `total_lines = rope.len_lines()`.
- New observers: `on_file_key` (`FileKeyEvent`), `on_file_text_input`
  (`FileTextInput`), `on_file_pointer` (`FilePointerEvent`). Each runs the
  keymap → `Vec<EditCommand>` → folds `core.apply` → from the aggregated
  `EditOutcome` emits `FileViewportPatch` (text/scroll), `FileCursorEvent`
  (sel/mode), `FileDirtyEvent` (dirty); on text change schedules LSP
  `didChange`.
- **Save** (`EditCommand::Save`): write `rope.to_string()` to `fv.path`, record
  a `SelfWrite { path, hash }` guard, clear dirty, emit `FileDirtyEvent{false}`,
  fire LSP `didSave`.
- **External-edit conflict** (`reload_changed_files`): if the change matches a
  recent `SelfWrite` → ignore (our own write); else if `EditCore.dirty` → do
  **not** reload, emit `FileExternalChange`; else reload as today. (Fixes the
  current unconditional reload, which would discard unsaved edits.)
- **LSP `didChange` debounce**: mark `LspDirty` and flush after ~150 ms idle, so
  diagnostics refresh without a request per keystroke. `LspManager::change()`
  already exists.

## Page input controller (`crates/vmux_editor/src/page.rs`, Text mode)

- Hidden overlay `<textarea id="file-input">` positioned at the caret
  (`spellcheck` / `autocapitalize` / `autocorrect` / `autocomplete` off), kept
  focused — the IME + keystroke sensor.
  - `compositionend` / `input` (when not composing) → take value →
    `FileTextInput`, then clear. (IME underline renders in the overlay during
    composition; only the commit crosses the wire.)
  - `keydown`: skip when `isComposing`; if `mode != Insert` **or** key is
    non-text **or** any ctrl/alt/meta chord → `preventDefault` + `FileKeyEvent`;
    else fall through to `input` (printable / IME). Mode comes from
    `FileCursorEvent`.
- Render over the virtualized window: blinking caret div at
  `(col*cw, (line - first_line)*ch)`; per-visible-line selection rects
  (line-spanning → to EOL); Vim mode badge in the header; dirty dot from
  `FileDirtyEvent`.
- `onmousedown` / drag on text → `FilePointerEvent { line, col, extend:
  shiftKey }`. Wheel → `FileScrollEvent` unchanged.
- **Clipboard is native** (`arboard`): `Cmd/Ctrl C/V/X` route as command chords
  → `Yank/Cut/Paste` → arboard. Unifies Vim registers + system clipboard, and
  avoids browser clipboard-permission prompts.
- New `web-sys` features: `HtmlTextAreaElement`, `InputEvent`,
  `CompositionEvent`, `MouseEvent`.

## Dependencies

New: `ropey` (MIT — the rope), `arboard` (system clipboard; `NonSend` handle).
Already in-tree: `unicode-width`, `unicode-segmentation`. (Internal-crate count
unchanged — no new workspace crates.)

## Testing

- **Pure Rust unit** (no wasm/cef — the bulk of the logic):
  - core: command sequences → assert rope text, cursor, mode, dirty,
    undo/redo, clipboard register.
  - each keymap: `KeyInput` sequences → assert emitted `EditCommand`s, and drive
    the core → assert end text. Vim motions / operators / counts / modes / undo;
    VSCode arrows / shift-select / chords / word-delete.
  - highlight cache: an edit that opens or closes a block comment re-highlights
    the tail; window correctness; `invalidate_from`.
  - visual-column math for CJK / wide characters.
- **Bevy system + message integration** (per AGENTS.md): register events +
  systems, send `FileKeyEvent` / `FileTextInput`, run the schedule, assert
  emitted `FileViewportPatch` / `FileCursorEvent` / `FileDirtyEvent` and
  `EditCore` state. The existing mock LSP binary drives didChange → diagnostics.
- **Manual** (at the end, by the user): Japanese IME; caret/selection visuals;
  latency feel; save + dirty + title dot; external-change banner; keymap switch
  via settings.

## Risks / open questions

- **Keystroke latency**: one local-IPC round-trip per key over the CEF bin
  channel. Expected fine (scroll already uses it). If it ever feels laggy, add
  page-side optimistic echo (page applies inserts/deletes to the visible window
  locally; native reconciles) — deliberately deferred to keep v1 correct and
  simple.
- **IME vs Vim Normal mode**: the textarea must not start composition on
  command keys in Normal mode. Handled by the mode hint: only Insert/VSCode
  routes printable keys to `input`.
- **Undo granularity**: snapshot-group by typing run; revisit if it feels coarse
  vs Vim's per-insert-session expectation.
- **Wide-char caret accuracy**: native sends visual columns; assumes the
  monospace font renders CJK at 2 cells. Good enough for v1.

## Build order

One feature branch on top of `main` (post LSP merge), internally milestoned;
the user runtime-tests at the end (matches the LSP-branch workflow).

1. **M1** — edit core + `EditCommand` + undo + pure-Rust tests.
2. **M2** — both keymaps + `EditorSettings` + pure-Rust tests.
3. **M3** — incremental highlight cache + tests.
4. **M4** — wire-protocol events + plugin observers + emits + integration tests.
5. **M5** — page textarea / IME / keydown routing + caret/selection render +
   pointer.
6. **M6** — save + external-conflict + LSP `didChange` debounce.
