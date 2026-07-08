# Hide agent `run` completion markers (D1: tokened OSC escape)

## Problem

The agent `run` tool (CLI blocking + ACP) wraps each command with visible printf
markers:

```
printf '\n__VMUX_START_<token>__\n'; <cmd>; printf '\n__VMUX_DONE_<token>_<exit>__\n'
```

`run_blocking` greps the terminal grid for `__VMUX_DONE_<token>_<exit>__` to learn
the exit code. The markers render as literal text lines in the terminal (seen in
v0.0.22).

They were invisible once: `355a431b` used OSC 133 shell-integration hooks +
`CommandExit` seq baseline. `5bc425f9` (#221) reverted to visible printf markers.

### Why #221 reverted (root cause)

The OSC 133 completion path captured `baseline_seq` by polling `CommandExit`
**after** the command was already dispatched to the PTY — a TOCTOU race. A fast
command emits `133;D` before the client snapshots the baseline, so the client
waits for a `D` that already passed → timeout / "still running". A fresh shell's
`precmd` also fires a spurious `133;D` at its first prompt. OSC 133's `D` carries
an exit code but **no per-command token**, so completion could not be correlated
to a specific run — the seq delta raced.

## Approach: tokened OSC completion escape (D1)

Keep a per-run token (what made the printf markers robust) but deliver it as an
**invisible OSC escape** instead of visible text.

The `run` wrapper appends, after the command:

```
ESC ] 6973 ; <token> ; <exit> BEL
```

`6973` is a vmux-private OSC code (distinct from `133`, so it does not disturb the
existing OSC 133 lifecycle used by the vibe "armed" pane). The escape is consumed
by the VTE parser — never rendered.

A new service-side scanner (`run_marker.rs`, sibling of `osc133.rs`) extracts
`(token, exit)` from the PTY byte stream. Because the token travels inline with the
exact command, completion detection is:

- **race-free** — no baseline; the client matches its own token whenever the slot holds it
- **interleave-proof** — user-typed commands don't carry the token (matters for the shared/unified terminal direction)
- **`-c`-proof** — does not depend on shell-integration hook injection

## Components

1. **`crates/vmux_service/src/run_marker.rs`** (new)
   - `RunMarkerScanner` (VTE `Parser` + `Perform`), `pub const VMUX_RUN_OSC = b"6973"`.
   - `feed(&[u8]) -> Vec<RunMarker { token, exit }>`. Handles sequences split across feeds.

2. **`crates/vmux_service/src/process.rs`**
   - New fields: `run_marker: RunMarkerScanner`, `last_run_completion: Option<(String, i32)>`.
   - `poll()` feeds PTY bytes to the scanner; each marker sets `last_run_completion`.
   - `pub fn run_completion(&self) -> Option<(String, i32)>`.

3. **`crates/vmux_service/src/protocol.rs`**
   - `AgentQuery::RunCompletion { process_id }`.
   - `AgentQueryResult::RunCompletion { token: Option<String>, exit: Option<i32> }`.

4. **`crates/vmux_service/src/server.rs`**
   - Serve `RunCompletion` from `process.run_completion()`.
   - Add arm to `query_result_to_content`.

5. **`crates/vmux_agent/src/plugin.rs`**
   - `command_with_marker` emits the OSC escape per shell (bash/zsh/sh, fish, nu)
     instead of visible printf. Token flow (`done_marker = Some(token)`) unchanged.

6. **`crates/vmux_mcp/src/protocol.rs`**
   - `run_blocking` polls `AgentQuery::RunCompletion` and matches its own token
     instead of grepping the grid. Output via `output_since(baseline, final)`.
   - Remove `extract_done_marker_output`. `output_since` becomes live.

## Kept as-is

- OSC 133 shell-integration hooks (`shell_integration.rs`) + `CommandLifecycle` —
  load-bearing for the vibe "armed" pane (`vibe/setup.rs`). D1 uses a separate OSC
  code, so no double-count.
- Per-run token generation (`run_done_token`, `blocking_run_with_marker`).

## Known limitations / follow-ups

- **Output capture** is `output_since` (baseline/final grid diff), matching the
  OSC-133 era. Slightly noisier than the printf-slice (may include command echo /
  trailing prompt). Exit code is token-exact. Exact output extraction is a possible
  follow-up.
- **Wrapper echo**: the wrapped command source may still echo (pre-existing shell
  behavior). D1 removes the executed marker *output* lines (the reported symptom);
  runtime-verify no visible OSC-source residue remains.

## Testing

- `run_marker.rs`: unit tests — extract token+exit, split-across-feeds, ignore
  other OSC / `133`, malformed exit.
- `command_with_marker`: emits the OSC escape (no `__VMUX_` text) per shell.
- `run_blocking` token match (existing test infra).
- Targeted: `cargo test -p vmux_service -p vmux_mcp -p vmux_agent`; fmt; clippy.
- Runtime: user confirms markers gone and `run` still reports exit codes.
