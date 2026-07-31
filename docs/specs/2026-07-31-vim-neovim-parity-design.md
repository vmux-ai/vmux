# Vim Mode Neovim Parity

## Summary

The editor's vim keymap is a 416-line single file covering roughly fifty bindings. It handles three operators, twelve motions, no text objects, one register, no search, and three ex commands. This document defines the target — practical Neovim parity for a single-buffer modal editor — and the vocabulary changes required to reach it.

Parity here means everything in `:help index.txt` for Normal, Visual, Insert, Operator-pending, Replace, and Command-line modes. It excludes Vimscript and Lua, plugin and remote APIs, `:terminal`, diff mode, and spell checking. Window and buffer management map onto vmux panes and stacks rather than an in-editor window system.

## Decisions

The state machine stays hand-written in `vmux_editor`. No Rust crate provides vim bindings as a library: Zed's `vim` crate is welded to gpui, helix-core is Kakoune selection-first, and lifting either drags in an incompatible dependency tree. This was settled during the editable-editor work and is not reopened here.

Counts, registers, and linewise semantics become operands on a structured operator command rather than properties reconstructed by the keymap. Today `3j` emits `Move(Down)` three times and `2dd` discards the count entirely; both are symptoms of the same missing structure.

Dot-repeat and macros share one mechanism. Both replay input, and insert-mode text bypasses the keymap through the IME path, so replay must live above the keymap where both key events and text input are visible.

Search patterns are translated from vim syntax to `fancy-regex`, already in the tree via syntect. The translation covers `\<`, `\>`, `\v`, `\V`, and character-class aliases. Patterns that do not translate are reported rather than silently reinterpreted.

`:sp`, `:vs`, `Ctrl-w`, `:bn`, and `:ls` operate on vmux panes and stacks. The editor does not grow its own window abstraction.

The frontend stays dumb. The host owns command-line buffer state, search match spans, pending-key display, and cursor shape, and pushes each as derived values. The page renders them.

## Vocabulary

`EditCommand` currently spells out each operator-target pair as its own variant: `DeleteRange`, `YankRange`, `DeleteLine`, `DeleteSelection`, `DeleteToLineEnd`. Adding indent, case, and format operators across motions, text objects, and selections would multiply that set. Collapse it into the operator-times-target form vim itself uses.

```
Operator   Delete Change Yank Indent Outdent Format Upper Lower ToggleCase Fold Rot13
Target     Motion(Motion, count) | TextObject(TextObject) | Line(count) | Selection
```

An operator command carries an operator, a target, a count, and an optional register. `Move` and `Select` carry a count. Registers become a file keyed by name, holding text plus a kind of charwise, linewise, or blockwise, with the unnamed, numbered, small-delete, blackhole, and system registers behaving per vim. Deletes must write to registers; today they do not, so `dd` followed by `p` pastes the last yank.

New commands beyond the operator form: `ReplaceChar`, `JoinLines`, `Increment`, `Search`, `SearchNext`, `Substitute`, `SetMark`, `GotoMark`, `Jump`, `RecordMacro`, `PlayMacro`, `Repeat`.

`Motion` gains WORD variants, find-char with its repeat state, match-pair, screen positions, display-line motions, scroll-by-page, mark targets, and search targets. `EditMode` gains `VisualBlock`, `Replace`, and `CommandLine`. `VisualLine` must acquire real linewise behavior — it is currently a label over charwise selection.

## Modules

`keymap/vim.rs` splits along mode boundaries into `state.rs`, `normal.rs`, `visual.rs`, `insert.rs`, `operator.rs`, `ex.rs`, and `search.rs`. The edit core gains `text_object.rs`, `register.rs`, `search.rs`, and `mark.rs`. Filename-based modules only, per repository convention.

## Phases

Each phase is independently shippable and leaves the keymap in a working state.

**1 — Structured operators.** Introduce the operator form, register file, count operands, and linewise kind. Port existing bindings onto it. Unlocks `>`, `<`, `=`, `gu`, `gU`, `g~`, `J`, `~`, `r`, `Y`, correct `2dd`, `3p`, `d3w`, `2d3w`, deletes populating registers, and linewise paste.

**2 — Text objects.** `TextObject` plus `i` and `a` handling in operator-pending and visual mode: word, WORD, sentence, paragraph, the four bracket pairs, the three quote pairs, and tag.

**3 — Motions.** WORD motions, `f F t T ; ,`, `%`, `H M L`, `zz zt zb`, `Ctrl-d Ctrl-u Ctrl-f Ctrl-b`, `+ - _ |`, and the `g`-prefixed display-line motions. Fold bindings must yield the `z` prefix back to screen positioning.

**4 — Modes.** Visual Block with `I` and `A` insert, Replace mode, Insert-Normal via `Ctrl-o`, and insert-mode chords `Ctrl-w Ctrl-u Ctrl-h Ctrl-t Ctrl-d Ctrl-r Ctrl-a Ctrl-v`. Command-line becomes a real mode with a rendered prompt.

**5 — Search.** `/ ? n N * # g* g#`, incremental highlight, `hlsearch`, `:noh`, and `:s` with ranges and flags. Requires the pattern translator and match spans pushed to the page.

**6 — Ex.** A range-and-command parser covering `%`, `.`, `$`, line numbers, marks, and `'<,'>`. Commands: quit and write variants, `:e`, `:sp`, `:vs`, `:b`, `:ls`, `:set`, `:g/`, `:v/`, `:norm`, `:m`, `:t`, `:d`, `:sort`, `:r`, `:!`. Adds command history and completion.

**7 — Replay.** An input recorder above the keymap capturing key events and IME text. Yields `.`, `q`, `@x`, and `@@` together.

**8 — Marks and jumps.** `m`, backtick, `'`, `:marks`, the jumplist behind `Ctrl-o` and `Ctrl-i`, and the changelist behind `g;` and `g,`. LSP jumps push records.

**9 — Mappings.** A key-sequence trie with `timeoutlen`, `<leader>`, and the `map` family in settings. Enables `jk` escapes and user bindings.

**10 — Undo tree.** Replace the flat snapshot stack with a tree: `g-`, `g+`, `:undolist`, `:earlier`, `:later`, persistent undo, and mode-transition boundaries.

Phases 1 through 3 remove the sharpest daily friction. Phase 5 is the largest single jump in perceived completeness. Phases 9 and 10 are the least urgent.

## Correctness

Several shipped behaviors diverge from vim independently of missing features and should be fixed in the phase that touches them. `cc` destroys indentation instead of preserving it. `o` and `O` insert a bare newline with no autoindent. `Escape` in Normal mode does not clear a stale command-line buffer. `Cmd` clipboard chords fire in Normal mode. `Ctrl-n` and `Ctrl-p` act as arrow keys in every mode. `5gg` ignores its count. Paste at end-of-line can cross a line boundary.

## Validation

Keymap logic is pure and headless, so each phase carries table-driven tests over key sequences asserting emitted commands, and edit-core tests asserting buffer and register state. Behavior only: no source scanning, no assertions on implementation shape. A vim-compatibility corpus of sequence-to-buffer-state cases, checked against real vim where behavior is ambiguous, guards against regression as phases land.
