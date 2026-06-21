# Shell integration: systematic command lifecycle via OSC 133

Date: 2026-06-21
Status: draft (pending review)

## Problem

`vmux run` (agent/MCP command execution) learns when an injected command finishes,
and its exit code, by **rewriting the command**. `run_command_line`
(`vmux_agent/src/plugin.rs:582`) wraps every command per shell:

- nu: `try { <cmd>; print "\n__VMUX_DONE_<token>_($env.LAST_EXIT_CODE)__" } catch { |e| print "\n__VMUX_DONE_<token>_($e.exit_code? | default 1)__" }`
- fish: `<cmd>; printf '\n__VMUX_DONE_<token>_%s__\n' $status`
- bash/zsh/other: `<cmd>; printf '\n__VMUX_DONE_<token>_%s__\n' "$?"`

`vmux_mcp/src/protocol.rs` then polls the terminal output and scrapes
`__VMUX_DONE_<token>_<exit>__` to detect completion + exit code.

Problems:

- **The user sees the wrapper, not their command.** The instrumented line is what is
  typed into the shell, shown in the viewport, and stored in shell history. Re-running
  from history re-runs the wrapper.
- **Fragile.** try/catch + per-shell exit-code extraction; nu needs special-casing; any
  shell whose `$?` / `$status` / `LAST_EXIT_CODE` semantics differ breaks it.
- **Per-command cost, no general signal.** Every run pays the wrapping, and the rest of
  vmux has no notion of "a command started / ended" (for UI status, output capture, etc.).

## Goals

- The user types, sees, re-runs, and gets shell history of **exactly their command** — no
  wrapper, no sentinel.
- Command **start / end + exit code** are detected systematically for **every** command in
  a terminal, not just `vmux run`.
- Native support for the shells vmux targets: **bash, zsh, fish, nu**.
- No permanent edits to the user's dotfiles.

## Non-goals

- Capturing / segmenting per-command output ranges for the UI (the lifecycle events make
  this possible later; not built here).
- Shells beyond the big four — those get a fallback (below), not native integration.
- Changing the interactive terminal renderer.

## Decisions (from brainstorming)

- **Markers:** standard **OSC 133** semantic-prompt sequences (FinalTerm; same family as
  iTerm2 / VSCode / WezTerm), so they are invisible escape codes and interoperable.
- **Scope:** every command (full shell integration), installed once per session.
- **Injection:** at PTY spawn, via each shell's native init mechanism — no dotfile edits.
- **Parsing:** in `vmux_service`, by scanning the PTY byte stream for OSC 133 (alacritty
  already ignores it, so nothing leaks into the grid).
- **Fallback:** unknown / unsupported shell → keep a minimal `__VMUX_DONE_` sentinel (or
  bash wrapper) for `run`, accepting command-mangling only there.

## Architecture

### OSC 133 markers

The shell emits, around each prompt / command:

- `OSC 133 ; A ST` — prompt start
- `OSC 133 ; B ST` — prompt end / command start
- `OSC 133 ; C ST` — pre-execution (command is running)
- `OSC 133 ; D ; <exit> ST` — command end, with exit code

(`OSC` = `ESC ]`, `ST` = `ESC \` or `BEL`.) vmux only needs `C` (a command started) and
`D;<exit>` (it ended with code). `A` / `B` are emitted for interop and future use.

### Per-shell emission (injected snippets)

Small native snippets, shipped + versioned by vmux. Sketches:

- **bash** — `trap ... DEBUG` (preexec) + `PROMPT_COMMAND` (precmd), or bash-preexec: emit
  `C` before a command and `D;$?` before the next prompt. Adapt the well-known VSCode / iTerm
  bash integration.
- **zsh** — `preexec` / `precmd` hooks emit `C` and `D;$?`.
- **fish** — `fish_preexec` / `fish_postexec` events emit `C` and `D;$status`.
- **nu** — `$env.config.hooks.pre_execution` emits `C`; `pre_prompt` emits
  `D;($env.LAST_EXIT_CODE)`.

### Injection at PTY spawn (no dotfile edits)

`Process::new_with_wake` (`vmux_service/src/process.rs:250`) already controls the shell
`command`, `args`, and `env`. Inject the integration via each shell's native,
non-persistent init:

- **bash** — `--rcfile <vmux-snippet>`; the snippet first sources the user's `~/.bashrc`,
  then installs hooks.
- **zsh** — set `ZDOTDIR` to a temp dir whose `.zshrc` sources the user's, then installs
  hooks.
- **fish** — `--init-command 'source <vmux-snippet>'` (or a `conf.d` entry).
- **nu** — source the snippet via nu's config / env-file flag. Highest risk: nu replaces
  rather than layers config, so the snippet must source the user's config, then append hooks.

Snippets ship inside the vmux app bundle; injection selects one by the shell basename. The
user's own config still loads — vmux only appends hooks.

### Parsing in vmux_service

`Process` feeds PTY bytes to `alacritty_terminal`'s vte (`process.rs:359` / `:1179`).
alacritty ignores OSC 133 (no grid output), and its `EventListener` does not surface it
(only `TermEvent::Title` etc. today).

Add an **OSC 133 scanner** over the raw `data` in the drain loop, alongside
`processor.advance`:

- Scan for `ESC ] 133 ; … (ST | BEL)`; keep a small bounded carry-over buffer so a sequence
  split across `read()` chunks is reassembled.
- On `C`: emit "command started". On `D;<exit>`: emit "command ended, exit = <n>".
- Broadcast a new `ServiceMessage` (e.g. `CommandLifecycle { process_id, kind, exit_code }`)
  on the existing `patch_tx` channel, next to `ViewportPatch` / `ProcessTitle`.

alacritty keeps rendering the stream unchanged; OSC 133 never appears in the viewport.

### Consumption

- **MCP `run`** (`vmux_mcp/src/protocol.rs`): send the user's clean command; wait for the
  next `D;<exit>` lifecycle event for that process instead of scraping `__VMUX_DONE_`.
  Correlate by sequencing (`run` sends one command, waits for the subsequent `C` → `D`).
  Return output + exit.
- **Remove the wrapper**: delete `run_command_line` instrumentation
  (`vmux_agent/src/plugin.rs:582-600`) and the `__VMUX_DONE_` parse machinery in
  `protocol.rs` for supported shells.
- **(Future)** lifecycle events become available to the UI (per-command status, spinners) —
  out of scope here.

### Fallback for unknown shells

If the shell basename is not one of bash / zsh / fish / nu (or snippet injection fails),
fall back to the current `__VMUX_DONE_` sentinel / bash wrapper for `run`. Those commands
stay mangled / visible — only for unrecognized shells. No lifecycle events for typed
commands there.

## Testing

- Per-shell snippet emits the expected OSC 133 bytes: spawn each shell with injection, run a
  command, assert `C` and `D;<code>` (including non-zero exit) appear in the raw PTY stream
  and are absent from the rendered grid.
- OSC 133 scanner: unit tests for whole + chunk-split sequences, `BEL` vs `ST` terminators,
  interleaving with normal output, and ignoring non-133 OSC.
- `run` correlation: a clean command returns correct output + exit via lifecycle events;
  a concurrent typed command does not confuse the wait.
- Injection layers user config (the user's rc still runs) and makes no dotfile edits.
- Fallback path still works for an unknown shell.

## Risks

- **nu** is the highest-risk target (config layering + hook semantics). Validate first; if
  non-viable, nu temporarily uses the fallback.
- **Split OSC sequences** across read chunks — the scanner must buffer; bound it so a
  malformed stream cannot grow it without limit.
- **Snippet drift** vs upstream shells — vendor + version the snippets; cover with the
  emission tests.
- **History / rc interactions** — `--rcfile` / `ZDOTDIR` must source the user's config or the
  user loses their environment; tested explicitly.
