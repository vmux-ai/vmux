# Agent Run Placement Design

## Goal

Keep `vmux.run` pane placement deterministic when agents request poor split directions, while allowing users to restore agent-requested placement.

## Settings

Replace the current agent-controlled default with vmux auto placement. Add
`agent.allow_run_placement_override`, defaulting to `false`, as an explicit
opt-out:

- `false` (default): vmux owns run-terminal placement.
- `true`: vmux honors the tool's `mode`, `beside`, and `direction` arguments.

Expose the setting in `vmux://settings/` under Agent.

## Validation

The MCP layer records whether any placement argument (`direction`, `beside`, or `mode`) was explicitly supplied before applying protocol defaults. This lets the app distinguish a bare run from an agent placement override.

With `agent.allow_run_placement_override = false`, a run is accepted only when placement arguments are omitted. Explicit `direction`, `beside`, or `mode` returns an error instructing the agent to omit placement arguments and retry. This avoids silently ignoring intent and gives the agent a recoverable correction.

Runs targeting an existing `terminal` remain unchanged when no placement override is supplied. Explicit placement arguments are still rejected because they conflict with the configured policy.

With `agent.allow_run_placement_override = true`, current behavior remains: `mode`, `beside`, and `direction` are honored, and omitted direction falls back to `right` when a split needs one.

## Placement

Auto placement continues using the existing persistent terminal-region and spiral-placement logic. The first bare run creates or selects the terminal bucket; later bare runs reuse its shell. No split direction from the agent can bypass this while auto placement is configured.

## Testing

- Settings default and serialization tests for `agent.allow_run_placement_override`.
- MCP dispatch tests proving bare runs and explicit placement are distinguishable.
- Agent command handling tests proving the default rejects explicit placement and the opt-out honors it.
- Existing-terminal test proving terminal reuse remains allowed.
