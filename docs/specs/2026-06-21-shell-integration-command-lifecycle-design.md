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

`Process` feeds PTY bytes to `alacritty_terminal`'s ansi processor
(`process.rs:359` / `:1179`). alacritty's high-level `Handler` does **not** expose OSC 133
(its "semantic" API is word-selection chars, unrelated), and its `EventListener` only
surfaces `TermEvent::Title` etc. — OSC 133 falls through as an unhandled OSC: ignored, no
grid output.

Rather than hand-roll a byte scanner, ride alacritty's own parser library: run a second,
tiny **`vte::Parser`** in the drain loop, fed the same `data`, with a custom `Perform` that
implements only `osc_dispatch`. vte's state machine handles ESC/OSC framing, `ST` vs `BEL`
terminators, and sequences split across `read()` chunks — correct reassembly for free.

- In `osc_dispatch`, match `params[0] == b"133"`; on `C` emit "command started", on
  `D;<exit>` parse `params[2]` and emit "command ended, exit = <n>". Ignore all other OSC.
- Broadcast a new `ServiceMessage` (e.g. `CommandLifecycle { process_id, kind, exit_code }`)
  on the existing `patch_tx` channel, next to `ViewportPatch` / `ProcessTitle`.

alacritty keeps rendering the stream unchanged; OSC 133 never appears in the viewport.
(`vte` is already a transitive dep via `alacritty_terminal`; add it as a direct dep.)

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
- OSC 133 `Perform`: feed a persistent `vte::Parser` whole + chunk-split sequences, `BEL` vs
  `ST` terminators, interleaving with normal output, and non-133 OSC; assert the right
  lifecycle events fire and nothing else does.
- `run` correlation: a clean command returns correct output + exit via lifecycle events;
  a concurrent typed command does not confuse the wait.
- Injection layers user config (the user's rc still runs) and makes no dotfile edits.
- Fallback path still works for an unknown shell.

## Risks

- **nu** is the highest-risk target (config layering + hook semantics). Validate first; if
  non-viable, nu temporarily uses the fallback.
- **Parser state across chunks** — handled by vte's state machine (the `Perform` runs in a
  persistent `Parser` per process); no hand-rolled buffering. The second parser only inspects
  OSC, so per-byte cost is negligible.
- **Snippet drift** vs upstream shells — vendor + version the snippets; cover with the
  emission tests.
- **History / rc interactions** — `--rcfile` / `ZDOTDIR` must source the user's config or the
  user loses their environment; tested explicitly.
