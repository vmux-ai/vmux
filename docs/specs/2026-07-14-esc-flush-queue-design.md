# Esc flush queue

Date: 2026-07-14
Status: Approved

## Problem

Agent prompts submitted while a turn is running remain queued until the turn settles. Pressing Esc should interrupt the active turn and send all queued prompts immediately as one combined turn. Normal completion should preserve FIFO behavior and dispatch one queued prompt per turn.

The current branch has four correctness gaps:

- the page chooses flush versus cancel from a delayed `ChatSnapshot`, so Enter→Esc and snapshot-lag races can emit the wrong event;
- `flush_pending` remains set when Stop or Ctrl+C follows an Esc flush, so the later explicit cancellation can still drain the queue;
- flushing queued prompts from `AgentRunState::Errored` does not restore `Idle`, leaving the queue blocked;
- both normal and Esc-triggered drains call `take_merged`, changing ordinary FIFO queue semantics.

## Approaches

### Chosen: one native Escape intent

Replace the page-side flush-versus-cancel decision with one `ChatEscape` event. Native code reads the authoritative `PromptQueue` and `AgentRunState`, then chooses whether to flush queued prompts or perform a normal cancellation.

This removes the snapshot race and keeps queue transitions in the native state owner.

### Rejected: optimistic page queue state

Update the page's queued state immediately after submit and continue emitting separate `ChatFlush` and `ChatCancel` events. This reduces one race window but retains two sources of truth and remains vulnerable to delayed native state transitions.

### Rejected: cancellation generations

Attach generation identifiers to flush and cancellation requests. This can define ordering precisely but adds protocol and state-machine complexity that is unnecessary when native code can handle Escape atomically.

## Design

### Page event flow

The chat page emits `ChatEscape` for Escape after selector handling. Native code decides the queue action. The page may still clear an idle draft locally when its latest snapshot reports no active turn or queued prompts, but it does not select between flush and cancel.

The Stop button and Ctrl+C continue to emit `ChatCancel`. These are explicit normal cancellations and override any pending flush request.

### Native Escape handling

`on_chat_escape` resolves the stack from the webview and inspects its queue and run state.

When the queue is non-empty:

1. mark the queue for flush and unpause it;
2. change `Errored` to `Idle` so dispatch can resume;
3. cancel the service turn only for `Streaming` or `AwaitingApproval`;
4. let the existing ACP or Page dispatch system send the merged queue after the state becomes idle.

When the queue is empty and the state is `Streaming` or `AwaitingApproval`, clear any stale flush request and perform a normal cancellation.

Other states require no native action.

### Queue transitions

Encapsulate queue state changes in `PromptQueue` methods:

- normal enqueue appends one prompt and unpauses without discarding an active flush request;
- requesting a flush requires a non-empty queue, sets `flush_pending`, and unpauses;
- normal cancellation clears `flush_pending` before sending `AgentCancel`;
- clear empties the queue and resets pause and flush state;
- resume clears pause and flush state;
- dispatch pops one prompt during normal FIFO operation;
- dispatch drains and joins every prompt with a blank line only when `flush_pending` is set, then clears the flag.

An `Interrupted` status pauses the queue when no flush is pending. A flush-triggered interruption leaves it running so the merged dispatch can proceed.

### Error handling

An Esc flush from `Errored` reuses the existing submit-after-error recovery rule by setting the run state to `Idle`. If no service connection or session ID is available, queue state remains intact instead of dropping prompts; normal dispatch or a later state transition can retry.

No new queue-size limit is introduced. Aggregate IPC limits are broader queue-policy work and are not required to fix the state and ordering defects.

## Testing

Follow red-green TDD for each transition.

Add `PromptQueue` unit coverage for:

- normal dispatch pops only the first prompt;
- flush dispatch merges all prompts and clears `flush_pending`;
- normal cancellation, clear, and resume reset flush state;
- enqueue during a pending flush remains part of the flush batch.

Add Bevy observer/system coverage for:

- Escape with queued prompts marks a flush;
- Escape with queued prompts from `Errored` restores `Idle`;
- normal cancel after an Esc flush clears the flush request;
- `Interrupted` pauses a normal queue but not a pending flush;
- ACP and Page dispatch preserve FIFO normally and merge only for flush.

Add page-source coverage ensuring Escape emits the single native `ChatEscape` intent rather than choosing `ChatFlush` or `ChatCancel` from snapshot queue state.

Run `cargo test -p vmux_agent`, `cargo clippy -p vmux_agent --all-targets -- -D warnings`, formatting checks, and a wasm32 check for the page event changes.

## Risks

- Escape during a narrow transition from queued to newly dispatched work may become a normal cancellation once native state shows an empty queue and active turn. This matches the authoritative state at event handling time.
- New submissions arriving after an Esc request but before interruption completion join the pending flush batch. This preserves the meaning of “send all queued prompts now.”
- Event renaming must stay synchronized between wasm page emission and native `BinEventEmitterPlugin` registration.
