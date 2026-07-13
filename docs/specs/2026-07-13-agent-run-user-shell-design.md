# Agent Runs in the User's Terminal Shell — Design

**Date:** 2026-07-13
**Status:** Approved
**Ships in:** PR #245

## Problem

Agent `run` commands open a visible native terminal, but that terminal is currently pinned to
`/bin/sh`. This differs from normal vmux terminals and exposes an implementation shell instead of
the user's configured terminal environment. It is especially confusing in a co-working app where
the user watches and can continue interacting with the same terminal.

## Chosen approach

Follow the Cursor model: agent commands run directly in a visible vmux terminal using the same
shell selection as a terminal opened by the user.

This is general behavior, not Nushell-specific behavior. Shell resolution remains:

1. The shell configured by the active terminal theme.
2. The user's `SHELL` environment variable when terminal settings are unavailable.
3. The existing platform fallback.

No separate agent-shell setting and no fixed command-runner shell are introduced.

## Alternatives considered

### Fixed POSIX runner

Keep `/bin/sh` for predictable command syntax. Rejected because the visible terminal would not be
the user's terminal and would continue exposing implementation details.

### Separate configurable agent shell

Add an agent-specific shell setting. Rejected because it duplicates terminal configuration and
creates two competing definitions of the user's terminal environment.

## Command flow

1. Resolve the destination terminal: an explicitly requested terminal, a reusable agent-run
   terminal, or a newly created terminal.
2. For an existing terminal, use its actual `TerminalLaunch.command` as the shell identity.
3. For a new terminal, resolve the shell through the normal vmux terminal settings path.
4. Build the completion-marker wrapper for that exact shell. Nushell, Fish, and POSIX-style shells
   retain their existing marker syntax. The command itself is not translated between shells.
5. Spawn a new terminal with the same resolved shell used to build its initial input. Keep the
   initial input pending until the terminal reports that its prompt is ready.
6. For a ready existing terminal, write the command directly to its PTY.
7. Preserve existing cwd selection, login-shell environment capture, pager suppression, terminal
   reuse, placement, focus, output display, and exit-status reporting.

Using the destination terminal's launch shell prevents marker syntax from becoming stale if the
user changes terminal settings while an older agent-run terminal remains open.

## Failure behavior

- Missing explicit terminal, process startup failure, and stale placement errors keep their current
  responses.
- Shell startup failures remain visible in the native terminal.
- Unknown shell names keep the existing POSIX marker fallback.
- The initial command must never be submitted before a newly spawned shell reaches prompt-ready
  state.

## Testing

- Replace tests asserting `/bin/sh` pinning with tests asserting normal shell resolution.
- Verify a newly spawned agent-run terminal uses the same shell for `TerminalLaunch` and completion
  marker formatting.
- Verify explicit and reused terminals format commands from their actual launch shell.
- Cover Nushell, Fish, POSIX, and unknown-shell marker selection.
- Preserve regression coverage that the first command waits for prompt readiness and executes once.
- Preserve cwd, environment, terminal reuse, placement, output, and non-zero exit-code tests.

## Out of scope

- Translating model-generated commands between shell languages.
- Adding an agent-specific shell preference.
- Running commands in a hidden or detached shell process.
- Changing normal terminal shell configuration.
