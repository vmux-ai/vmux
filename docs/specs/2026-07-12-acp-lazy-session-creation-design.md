# ACP Lazy Session Creation Design

## Goal

Do not create or persist an ACP session until the user submits the first prompt. Remove existing empty ACP sessions created by vmux from the local machine through a reviewed one-time cleanup.

## Current Behavior

The ACP driver initializes the agent, calls `session/new`, emits `AcpSessionCreated`, and then waits for input. Opening a fresh ACP pane therefore creates provider state and rewrites the pane URL even if the user never sends a prompt.

When `session/load` fails, the driver also creates a replacement session immediately. That replacement can become another empty session.

## Chosen Approach

Keep ACP initialization eager, but make fresh session creation lazy inside the ACP driver.

Alternatives rejected:

- Delaying only vmux URL persistence would hide empty sessions from `/resume` while leaving provider files on disk.
- Deleting unused sessions when a pane closes would require provider-specific deletion behavior and would miss crashes or forced exits.

## Session Lifecycle

### Fresh pane

1. Start the ACP process and complete protocol initialization.
2. Keep the active session ID as `None`.
3. Emit `Idle` without emitting `AcpSessionCreated`.
4. Leave the pane URL in its fresh form, `vmux://agent/<agent-id>`.
5. On the first `AcpInput::User`, call `session/new` with the pane cwd and configured MCP servers.
6. Store the returned session ID, emit `AcpSessionCreated`, and send the original user prompt to that session.
7. Reuse the same session ID for later prompts and cancellation.

Only user prompts create sessions. Approval and cancellation inputs received before a session exists do not create one.

### Resumed pane

1. If a resume ID exists and the agent supports `session/load`, load it during startup.
2. On success, retain the ID, emit `AcpSessionCreated`, replay provider history, and enter `Idle`.
3. On failure, discard the stale ID and continue as a fresh unassigned pane.
4. Do not create a replacement until the next user prompt.

### Agents without load support

A supplied resume ID is treated as unavailable. The pane remains unassigned until the next user prompt creates a fresh session.

## Errors

If `session/new` fails for the first prompt:

- no session ID is stored or persisted;
- the pane reports an errored run status;
- the failed prompt remains visible in the local projected conversation;
- a later user prompt retries session creation.

Prompt errors after successful creation keep the existing session ID and follow the current prompt error path.

## Persistence

`AcpSessionCreated` remains the only event that assigns `AcpSession.resume` and rewrites page metadata. No GUI-side placeholder ID is introduced.

## Tests

Driver-focused tests cover:

- fresh initialization does not call `session/new` or emit a session ID;
- the first user prompt creates exactly one session before prompting;
- later prompts reuse the created session;
- successful resume loads and reuses the requested session;
- failed resume stays unassigned until a user prompt;
- failed first creation emits an error and leaves the session unassigned;
- cancellation or approval before the first prompt does not create a session.

Existing GUI tests continue to verify that `AcpSessionCreated` persists the returned ID and rewrites both stack and browser metadata.

## One-Time Local Cleanup

Cleanup is an operator action for the current machine, not shipped migration code.

The dry-run scans known ACP provider stores and classifies sessions with provider-specific parsers. A session is eligible only when both conditions are proven:

1. Evidence ties it to a vmux ACP launch, such as a vmux-persisted resume ID or vmux ACP configuration recorded in provider data.
2. It contains zero genuine user prompts.

Bootstrap records, hooks, metadata commands, tool results, and injected context do not count as genuine prompts. Ambiguous records and unknown provider formats are skipped.

The dry-run prints provider, session ID, file path, and classification evidence. Files are deleted only after explicit user approval of that exact list. Related sidecar files are included only when their ownership by an eligible session is unambiguous.

## Scope

This change does not alter CLI sessions, `/resume` filtering, ACP provider storage formats, or session retention after a genuine user prompt.

## Resume Selector Loading State

Opening `/resume` starts an asynchronous native session scan. The selector tracks that request separately from the returned session vector. Before the first request is sent and while its response is pending, the menu shows `Loading sessions…`. After the response arrives, the menu shows results, `No matching sessions`, or `No resumable sessions found` as appropriate. No timer or delayed empty-state heuristic is used.
