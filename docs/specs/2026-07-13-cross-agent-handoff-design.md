# Cross-Agent Session Handoff — Design

Date: 2026-07-13
Status: Approved

## Summary

Allow `/resume` to continue a session from one agent with the currently active agent. For
example, a user in Claude can select a Codex session, see its prior conversation, and continue
the work in a fresh Claude session.

This is a handoff, not a native resume. The source transcript is read from the source agent's
local session store, normalized to user and assistant text, rendered in the target pane, and sent
to the target agent as private context with the user's next prompt. The target receives its normal
tool configuration and inspects the source working directory itself.

## Goals

- Show sessions from every currently supported source strategy in `/resume`.
- Preserve native resume when the selected session belongs to the active agent kind.
- Hand a foreign agent's session to the currently active ACP agent without confirmation.
- Display the imported user and assistant conversation in the target chat.
- Keep private handoff context out of visible user messages.
- Delay target session creation until the user submits the first real prompt.
- Support any session shown by `/resume`, including sessions created outside vmux.
- Preserve handoff presentation when the new target session is resumed later.

## Non-goals

- Loading one agent's native session id through another agent's `session/load` implementation.
- Replaying source tool calls, tool results, thinking, plans, diffs, or metadata into the target.
- Copying source MCP/tool definitions; the target receives its normal tool configuration.
- Making unsupported agent stores discoverable.
- Generating an LLM summary during handoff.
- Modifying the source session.

## Resume semantics

`/resume` returns all sessions discovered by the registered CLI session strategies instead of
filtering the list to the active `AgentKind`.

Selection behavior depends on the source kind:

- Same kind as the active agent: use the existing native resume flow.
- Different kind from the active agent: start the cross-agent handoff flow.

Agent labels remain visible in each row, so pressing Enter on a foreign session is an explicit
agent handoff. No confirmation dialog is shown.

## Transcript readers

Each supported session strategy exposes a transcript-loading operation keyed by session id. The
initial implementations cover Codex, Claude, and Vibe using their local session files.

Readers normalize records into the existing vmux `Message` representation, retaining only:

- user text;
- assistant text.

Readers ignore system metadata, hidden reasoning, tool calls, tool results, plans, diffs, and
provider-specific control records. Malformed records are skipped. Loading succeeds when at least
one usable user or assistant message remains.

The operation runs before changing the current pane. An unreadable, unsupported, missing, or empty
transcript leaves the current pane unchanged and surfaces an inline error.

## Handoff flow

1. The user selects a foreign session from `/resume`.
2. Native code loads and normalizes the source transcript.
3. The current pane keeps its configured target ACP agent and adopts the source session's cwd.
4. The pane displays the imported messages followed by a `Continued from <source agent>` marker.
5. The target stack receives a `PendingHandoff` containing source identity, source session id,
   imported display messages, and the bounded private context payload.
6. No target ACP session is created at selection time.
7. The user's next submitted prompt creates the target session and sends the bounded handoff
   context together with that prompt.
8. The visible transcript appends only the real user prompt after the handoff marker. The private
   context is never rendered as a user bubble.
9. After successful dispatch, subsequent prompts use the normal target-agent flow.

The private context clearly identifies imported user and assistant turns and instructs the target
to continue from the supplied conversation, inspect the current cwd and repository state, and use
its normally injected tools when needed.

## Context limits

The complete normalized source transcript remains visible in the chat. The private payload sent to
the target uses a conservative fixed character budget because ACP does not provide a portable
context-window size for every configured agent.

When the transcript exceeds the budget:

- preserve the newest complete turns;
- preserve chronological order;
- never split a message;
- prepend an omission marker stating that older source turns were not sent;
- show the same omission state near the handoff marker so the user knows the target received less
  context than the UI displays.

## Prompt transport

Prompt transport distinguishes display text from optional private context. The ACP projector
records the display text as the visible user turn while the driver composes the actual ACP prompt
from private context plus display text.

The private context is attached only to the first submitted prompt. A failed dispatch retains the
pending handoff and imported transcript so the user can retry without reselecting the source
session. Successful dispatch consumes the pending context exactly once.

## Persistence and later resume

After the target agent assigns its ACP session id, vmux persists handoff presentation metadata in
its profile data, keyed by target agent id and target session id. The record contains the source
identity, imported display messages, the first visible target prompt, and whether context was
truncated.

When that target session is resumed later, vmux loads the record before transcript replay. The
stored imported messages and first visible prompt replace the private first-prompt payload in the
rendered transcript. Later replayed assistant and user turns continue normally.

Selection without a submitted prompt creates neither a target session id nor a persisted handoff
record. Closing such a pane therefore leaves no empty target session or stale sidecar.

## Error handling

- Source transcript cannot be loaded: keep the current pane and show an inline error.
- Some source records are malformed: skip them and continue when usable messages remain.
- No usable messages remain: reject the handoff as an empty or unsupported transcript.
- Target first prompt fails: retain imported history and pending context for retry.
- Handoff metadata cannot be persisted: continue the live session and surface a warning; native
  target resume still works, but imported-history presentation may be unavailable later.
- Persisted metadata is malformed or missing: ignore it and render the target agent's native replay.

## Testing

- Codex, Claude, and Vibe parser fixtures extract user and assistant text.
- Parser fixtures exclude tools, tool results, thinking, plans, diffs, and metadata.
- Parser fixtures skip malformed records and reject transcripts with no usable messages.
- Context-budget tests preserve newest complete turns, chronological order, and the omission marker.
- Resume-list tests verify all supported source kinds are returned and sorted together.
- Same-agent selection tests verify the existing native resume path remains unchanged.
- Foreign-agent selection tests verify the target agent is retained, source cwd is adopted,
  imported history is rendered, and no target session is created.
- First-prompt tests verify private context is sent exactly once while only display text enters the
  visible transcript.
- Failure tests verify the pending handoff remains retryable.
- Persistence round-trip tests verify later target resume reconstructs the imported history and
  suppresses the private prompt payload.
- Run targeted crate tests during implementation.
