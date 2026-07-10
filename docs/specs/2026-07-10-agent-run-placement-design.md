# Agent Run Placement Design

## Goal

Keep `vmux.run` pane placement deterministic when agents request poor split directions, while allowing users to restore agent-requested placement.

## Settings

Add `agent.run_placement` with two values:

- `auto` (default): vmux owns run-terminal placement.
- `requested`: vmux honors the tool's `mode`, `beside`, and `direction` arguments.

Expose the setting in `vmux://settings/` under Agent.

## Validation

The MCP layer preserves whether `direction` was omitted instead of converting omission to `right`.

With `agent.run_placement = "auto"`, a run is accepted only when placement arguments are omitted or use their defaults. Explicit `direction`, `beside`, or a non-`auto` `mode` returns an error instructing the agent to omit placement arguments and retry. This avoids silently ignoring intent and gives the agent a recoverable correction.

Runs targeting an existing `terminal` remain unchanged when no placement override is supplied. Explicit placement arguments are still rejected because they conflict with the configured policy.

With `agent.run_placement = "requested"`, current behavior remains: `mode`, `beside`, and `direction` are honored, and omitted direction falls back to `right` when a split needs one.

## Placement

Auto placement continues using the existing persistent terminal-region and spiral-placement logic. The first bare run creates or selects the terminal bucket; later bare runs reuse its shell. No split direction from the agent can bypass this while auto placement is configured.

## Testing

- Settings default and serialization tests for `agent.run_placement`.
- MCP dispatch test proving omitted direction stays omitted.
- Agent command handling tests proving auto rejects explicit placement and requested mode honors it.
- Existing-terminal test proving terminal reuse remains allowed.
