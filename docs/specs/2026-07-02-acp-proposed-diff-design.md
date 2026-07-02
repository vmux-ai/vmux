# ACP Proposed-Edit Diff (Cursor-style) — Design

**Date:** 2026-07-02
**Status:** Draft — implements task 9b. Fresh-context task (new editor primitive; do not rush).

## Goal

When an ACP agent proposes a file edit, show it **the way Cursor does**: the file opens beside the
agent as an **inline diff** — removed lines in red, added lines in green — with an **Accept /
Reject** bar. Accept applies the edit; Reject discards it. No silent apply, no chat-only note.

## Why it needs new code (verified)

- The editor's only diff view is `vmux_git::ui::DiffView` — **git working-tree-vs-HEAD**, keyed by
  `path` + nonce. It cannot render an arbitrary `old_text → new_text`. So a proposed (pre-apply)
  diff needs a new renderer.
- `AcpProposedDiff { sid, call_id, path, old_text, new_text }` is already emitted by the daemon
  (`acp/projector.rs` on `ToolCallContent::Diff` → `acp/driver.rs`) but is **orphaned** — no GUI
  consumer.
- The ACP stream reaches the GUI as `Page*` Bevy messages via `consume_page_agent_stream`
  (`client/page/plugin.rs`); a translation layer converts `ServiceMessage`→`Page*`. A new proposed
  message rides that path.

## Design

### 1. Editor primitive — `ProposedEdit`
- New `vmux_editor` component `ProposedEdit { path, old_text, new_text, call_id }` + a render mode
  in `editor/page.rs` (or a dedicated `vmux://diff` host) that:
  - Computes a line diff with the **`similar`** crate (`TextDiff::from_lines`) → hunks.
  - Renders unified/inline hunks with per-line class: added = `bg-emerald-500/10`, removed =
    `bg-red-500/10 line-through-ish`, context = normal — reusing the **shared syntect highlighter**
    (per [[project_editor_architecture]], one highlighter across editor/preview/diff).
  - Sticky **Accept / Reject** bar (Accept-all for multi-hunk).
- Emits `ProposedDiffResolved { call_id, accept: bool }`.

### 2. Wiring (reuse the permission round-trip)
- Pair the diff with its edit's permission: `AcpProposedDiff.call_id` == the `request_permission`
  call_id for the write. So **Accept = Allow, Reject = Deny** on the existing approval flow
  (`AgentApprovalReply` → `ClientMessage::AgentApprove`) — no new daemon path.
- GUI consume: `AcpProposedDiff` → new `PageAgentProposedDiff` message → in
  `consume_page_agent_stream`, resolve `sid`→`AcpSession` (has `anchor`) → open the file beside the
  agent in `ProposedEdit` mode (reuse the `FileTouched → OpenBeside` resolution:
  anchor→pane→`OpenBesideRequest`, with a `proposed: {old,new,call_id}` payload).
- On resolve: trigger `AgentApprovalReply { session, call_id, decision }`; close the diff overlay;
  the agent then applies via `write_text_file` (already handled, cwd-sandboxed) → the file's git
  diff confirms it.

### 3. Fallback
- If the agent applies via `write_text_file` **without** a `Diff` preview (no `AcpProposedDiff`),
  there's nothing to gate visually — the edit applies and shows in the git diff (today's behavior).
  This primitive only activates when the agent sends `ToolCallContent::Diff`.

## Files
- `crates/vmux_editor/`: `ProposedEdit` component + diff render (new module, reuse highlighter) +
  `similar` dep.
- `crates/vmux_service/src/protocol.rs`: (already has `AcpProposedDiff`) — confirm `call_id` is the
  permission call_id.
- `crates/vmux_agent/src/client/page/plugin.rs` + the `Page*` translator: `PageAgentProposedDiff` +
  consume → open-beside-in-diff-mode + Accept/Reject → `AgentApprovalReply`.
- `crates/vmux_layout`: `OpenBesideRequest` gains an optional proposed-diff payload (or a sibling
  request) so the editor opens in diff mode.

## Test
- Ask an agent to edit a file → file opens beside it as a red/green diff + Accept/Reject.
- Accept → edit applies (git diff confirms); Reject → file unchanged.
- Multi-hunk edit → all hunks shown; Accept-all applies.
- Agent that writes without a Diff preview → applies silently (fallback), no overlay.
