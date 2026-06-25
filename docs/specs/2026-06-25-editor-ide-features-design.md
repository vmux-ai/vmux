# Editor IDE Features — Design

Date: 2026-06-25
Branch: `feat/editable-editor` (stacked on the editable-editor + LSP work, PR #162)
Status: approved design

## Summary

Add IDE features to the editable `file://` editor on top of the existing LSP
integration (diagnostics) and the new edit core: **hover**, **go-to-definition**,
**find-references**, and **completions**, with **live `didChange`** so the
language server sees unsaved edits. Triggers include mouse (hover, Cmd+click),
keyboard (vim `gd`/`gr`/`K`, VSCode `F12`/`Shift+F12`/`Ctrl+Space`), and a
right-click context menu.

## Locked decisions

- v1 ships all four: hover, go-to-definition, find-references, completions.
- **Live `didChange`** (send in-memory buffer text, debounced) — upgrades the
  current save-gated diagnostics and makes hover accurate while typing.
- Go-to-def triggers: keyboard shortcut **and** right-click menu (+ Cmd+click).
- Lands stacked on `feat/editable-editor` (PR #162).

## Constraints discovered

- The LSP client (`lsp/client.rs`) already has id-correlated request/response,
  but `request()` **blocks** the calling thread — unusable for interactive
  requests from a Bevy system (would freeze the UI). Need a non-blocking path.
- `LspManager::change(path)` reads from **disk** — hence diagnostics are
  save-gated today. Live sync needs a text-carrying variant.
- Editor cursor is `(line, char-col)`; LSP is `(line, utf16-col)`. Convert at
  the boundary using rope line text.
- Diagnostics already map through `EditState` (fixed regression on this branch).

## Shared infrastructure

1. **Non-blocking requests** — `ServerClient::send_request(method, params) ->
   (i64, mpsc::Receiver<Value>)`: allocate id, register sender in the pending
   map, send, return the receiver without blocking. The reader thread fills it
   via `dispatch_message`. Keep blocking `request()` for initialize/shutdown.
2. **In-flight tracking + drain** — `LspManager` holds `Vec<InFlight { entity:
   Entity, kind: ReqKind, rx: Receiver<Value> }>`. A system `drain_lsp_requests`
   polls each with `try_recv`; on a result it parses per `kind` and emits the
   outbound event (or performs the go-to-def navigation). `ReqKind ∈ {Hover,
   Definition, References, Completion}` carries any needed context (e.g. the
   completion request's replace range / caret).
3. **Request methods** — `LspManager::{hover,definition,references,completion}(
   entity, path, line, utf16_col)`: resolve the doc's client+uri, `send_request`,
   push an `InFlight`. No-op if the doc has no server.
4. **Position mapping** — add `char_to_utf16_col(line_text, char_col) -> u32`
   (sum `len_utf16()` over the first `char_col` chars); reuse existing
   `utf16_to_char_col` for responses. Line text from the rope.
5. **Live `didChange`** — `LspManager::change_with_text(path, &str)` bumps the
   doc version and sends `didChange` with the supplied text. `run_commands`,
   after a text-changing batch, marks the entity dirty-for-lsp; a debounced
   flush (~150 ms) calls `change_with_text(path, &edit.core.buffer.text())` and
   re-runs lint. Replaces the save-only `LspEditDirty`→disk path.
6. **LSP actions through the keymap** — extend `EditCommand` with non-edit
   actions: `GotoDefinition`, `FindReferences`, `Hover`, `TriggerCompletion`.
   `EditCore::apply` returns these untouched (no buffer change) via a new
   `EditOutcome.lsp_action: Option<LspAction>`; `run_commands` intercepts and
   calls the manager at the caret. Keymaps: vim `gd`/`gr`/`K`; VSCode `F12`/
   `Shift+F12`/`Ctrl+Space`.

## Features

### Hover
- Trigger: page mouse-hover over text (debounced ~300 ms) → `FileHoverRequest{
  line, col }`; or `K`/cursor via keymap. Clear on mouse-leave or any edit.
- Native: `manager.hover` → `textDocument/hover` → `FileHoverEvent{ line, col,
  contents: String }` (markdown joined to a string).
- Page: render an LSP hover card (reuse the diagnostic-card styling), anchored
  near the token; markdown shown as preformatted text in v1.

### Go to definition
- Triggers: Cmd+click (page → `FileDefinitionRequest{line,col}`), `F12`/`gd`,
  right-click menu.
- Native: `textDocument/definition` → first `(uri, range)`. Navigate:
  - same file → set caret to target, autoscroll, emit window+cursor.
  - different file → open it via the existing page-open flow and attach
    `PendingGoto{ line, col }`; when the new `EditState` loads, consume it to
    position the caret.

### Find references
- Triggers: `Shift+F12`/`gr`/right-click → `textDocument/references` (include
  declaration) → `FileReferencesEvent{ items: Vec<RefItem{path, line, col,
  preview}> }`.
- Page: a references panel (soft-glass list), keyboard-navigable; Enter
  navigates like go-to-def (reuses `FileDefinitionRequest`-style open).

### Completions
- Triggers: typing identifier chars (auto) + `Ctrl+Space` → `textDocument/
  completion` at caret → `FileCompletionEvent{ items: Vec<CompletionItem{label,
  insert_text, kind, detail}>, replace_from_col }`.
- Page: caret-anchored popup; filter as you type, ↑/↓ select, Enter/Tab commit
  (replace the typed prefix via an `InsertText`/replace edit), Esc dismiss.
  While open the popup captures ↑/↓/Enter/Tab/Esc before the editor.

## New UI surfaces (page.rs, soft-glass)
- Hover card, right-click context menu (Go to Def / Find Refs / Show Docs),
  references panel, completion popup.

## Wire protocol (vmux_core/event.rs)
- Inbound: `FileHoverRequest{line,col}`, `FileDefinitionRequest{line,col}`,
  `FileReferencesRequest{line,col}`, `FileCompletionRequest{line,col}`.
  (Right-click menu reuses these once a menu item is chosen.)
- Outbound: `FileHoverEvent{line,col,contents}`, `FileReferencesEvent{items}`,
  `FileCompletionEvent{items,replace_from_col}`. Go-to-def acts natively via the
  existing open + `FileCursorEvent` path.
- Reuse the standard serde + rkyv derive set and `FILE_*` constants.

## Testing
- Pure Rust: `char_to_utf16_col` round-trips; keymap emits the new LSP actions
  (`gd`/`F12`→GotoDefinition, etc.); `EditOutcome.lsp_action` plumbing.
- Bevy/manager: `change_with_text` bumps version + sends; `drain_lsp_requests`
  parses a mocked hover/definition/references/completion response into the right
  outbound event (mock LSP bin already exists). Go-to-def same-file caret move
  asserted on `EditCore` state; `PendingGoto` consumed on load.
- Manual (user, end): hover shows docs; Cmd+click/F12/gd jumps (same + cross
  file); Shift+F12/gr lists refs and navigates; completions popup filters +
  commits; live diagnostics update while typing; right-click menu works.

## Milestones (stacked on #162)
1. **M1** shared infra: non-blocking `send_request`, in-flight drain, position
   mapping, `change_with_text` + live debounced sync, `EditCommand` LSP actions.
2. **M2** hover (mouse + `K`) + hover card.
3. **M3** go-to-definition: shortcuts (`F12`/`gd`) + Cmd+click + right-click
   menu + same/cross-file navigation.
4. **M4** find-references: request + references panel + navigation.
5. **M5** completions: request + popup + filter + commit.

Completions (M5) is the heaviest; M1–M3 deliver the core "jump + inspect" loop.
