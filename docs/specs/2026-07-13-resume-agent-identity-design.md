# Resume Selector Agent Identity — Design

Date: 2026-07-13
Status: Approved

## Summary

Show the active ACP agent's human-readable name on every `/resume` result. Resolve the name
from the running ACP implementation instead of maintaining a hardcoded Claude/Codex/Gemini
mapping. Do not claim whether a session originated in ACP or CLI because shared stores make that
unreliable.

## Goals

- Render a compact, right-aligned agent label on each `/resume` row.
- Support every configured ACP agent, including registry and custom agents.
- Prefer the identity reported by the running ACP agent.
- Keep usable fallbacks when an agent omits ACP implementation metadata.
- Reuse the resolved identity in the existing chat profile/header.

## Non-goals

- Detecting whether a shared session was originally created through ACP or CLI.
- Adding session discovery for ACP agents that do not have an existing vmux session lister.
- Renaming configured agents, changing ACP commands, or replacing Gemini CLI with another agent.
- Displaying agent versions in the resume selector.

## Identity precedence

Resolve one display name per active ACP pane:

1. Non-empty ACP `InitializeResponse.agent_info.title`.
2. Non-empty ACP `InitializeResponse.agent_info.name`.
3. Matching ACP registry display name.
4. Configured `AcpAgentConfig.name`.
5. ACP agent id.

The ACP `title` field is intended for human-readable UI. The required `name` field is its
programmatic fallback. Empty or whitespace-only values do not override lower-priority sources.

## Data flow

1. After ACP initialization, the service resolves `agent_info.title` or `agent_info.name` and
   emits a typed service message containing the vmux routing sid and display name.
2. The service bridge converts that message into a typed Bevy message.
3. The ACP client matches the routing sid and updates the pane's `Profile.name`.
4. Chat snapshot change detection includes profile changes so the existing header receives the
   same live identity.
5. A `/resume` list request reads the current pane's `Profile.name`. Native code copies it into
   each `ResumableSessionEntry.agent_name`; if the profile is absent or empty, it falls back to
   the current `AgentKind` display name, then the ACP id.
6. The page renders `agent_name` beside the session title. The existing time/project subtitle is
   unchanged.

Before live ACP metadata arrives, pane creation resolves the registry/config/id fallback. A
resume request during startup therefore still shows a valid name; later requests use the live
ACP-reported name.

## UI

Each result keeps its two-line layout:

- First line: truncated session title on the left, non-shrinking muted agent label on the right.
- Second line: existing relative-time and project subtitle.

Examples: `Claude`, `Codex`, `Antigravity`, or any custom ACP-reported title. No `ACP` or `CLI`
suffix is added.

## Error handling

- Missing `agent_info`: retain registry/config/id fallback.
- Empty title/name: ignore it.
- Identity event for an unknown routing sid: ignore it.
- Empty serialized row label: render no badge rather than an empty visual pill.

## Testing

- Name-resolution unit tests cover title, name, registry, config, id, and whitespace fallbacks.
- ACP driver test verifies initialization emits the resolved live identity when present.
- Service protocol round-trip test covers the new identity message.
- Bridge/ECS test verifies the matching ACP pane's `Profile.name` changes and unrelated panes do
  not.
- Resume-list test verifies `ResumableSessionEntry.agent_name` comes from the active profile.
- Page test verifies the row label renders without changing selection, filtering, or scrolling.
- Run targeted native tests and the vmux_agent WASM compile check.
