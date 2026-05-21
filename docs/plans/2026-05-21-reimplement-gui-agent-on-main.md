# Reimplement vmx-gui-agent feature work on top of current main

## Context

- Feature branch tip preserved as tag `vmx-gui-agent-pre-rebase` (was commit `e2cfa01c`).
- Target branch: `vmx-gui-agent-rebased` (created from `origin/main`, ready for commits).
- `vmx-gui-agent` (old branch) is untouched on disk; safe to abandon once rebased branch lands.

## Why reimplement, not rebase

Main (after `0f8932c1`, `16316053`, `cab74006`, `591b1b59`) underwent architectural changes that
make a straight 44-commit rebase impractical:

- Domain crates extracted from `vmux_desktop`: `vmux_setting`, `vmux_space`, `vmux_command`,
  `vmux_server` (was `vmux_page`), `vmux_history` carved out.
- `vmux_agent` module reorg: `app::AppAgentStrategy` → `client::page::strategy::AgentPageStrategy`,
  `cli_trait::CliAgentStrategy` → `client::cli::strategy::CliAgentStrategy`. `App*`/`register_app`
  naming replaced by `Page*`/`register_page`. `AgentVariant::App` → `AgentVariant::Page`.
- `vmux_agent` now depends on `vmux_layout`, `vmux_terminal`, `vmux_command`, `vmux_setting`,
  `vmux_space`, `vmux_service` (broad integration crate). Feature branch had it focused on
  `vmux_core` + `vmux_mcp` only.
- `vmux_layout` lost `command_bar/*` (moved to `vmux_command`), `cef.rs` renamed to `chrome.rs`,
  `space.rs` renamed to `tab.rs`, `page.rs` renamed to `app.rs`. Etc.
- 209 files differ, ~15.5K insertions / 9.6K deletions between branches.

Single-commit squash would revert main's refactor. Per-commit rebase explodes into cascading
conflicts. Reimplementation lets us preserve main's structure while landing the GUI agent work.

## Feature work to port (44 commits, grouped logically)

### Group A — Provider infrastructure in `vmux_agent`

Goal: real LLM provider strategies (Mistral, Anthropic, OpenAI) hitting SSE endpoints.

- A1. Cargo.toml: add `reqwest` features `stream` + `json`, add `futures-util`, `tokio` rt feature,
  add dev-deps `mockito = "1"`, `serial_test = "3"`.
- A2. Add `env_var() -> &'static str` to `AgentPageStrategy` trait.
- A3. New `crate::http` module: `drive_sse(strategy, request)` pumps SSE frames into
  `crossbeam::Sender<StreamEvent>`. Spawns a tokio runtime inside the IoTaskPool future
  (latent bug fix).
- A4. New `crate::providers::openai_shared`: chat-completions SSE parser + `messages_to_chat_completions`
  + `tools_to_function_specs` helpers.
- A5. New `crate::providers::{mistral, openai, anthropic}`: each implements `AgentPageStrategy`
  with provider-specific endpoint + SSE parser.
- A6. New `crate::providers::builtin`: `BUILTIN_PROVIDERS` array + `resolve_default_app_provider`
  (priority: mistral → anthropic → openai based on env vars) + `instantiate_builtin`.
- A7. SSE fixture files under `tests/fixtures/{anthropic,mistral,openai}/{text,tools}.sse`.
- A8. New `tests/streaming_smoke.rs`: end-to-end SSE streaming with `mockito` server.
- A9. Rewrite `tests/echo_smoke.rs` to use mock server + strategy registry.

Verify: `cargo test -p vmux_agent` passes. New tests cover SSE parsing for all three providers.

### Group B — MCP tool dispatch in `vmux_agent`

Goal: agent can call MCP tools in-process and re-stream after tool result.

- B1. New `crate::tools`: `mcp_tool_defs()` bridges `vmux_mcp::tools` to `ToolDef`.
- B2. New `crate::tool_dispatch`: in-process MCP tool dispatch via static channel bridge.
- B3. Update `systems/drain_stream`: handle `ToolUseStart`/`ToolUseArgsDelta`/`ToolUseEnd` →
  transition to `AwaitingApproval`, emit `Error` events.
- B4. New `systems/continue_after_tool`: when last message is `ToolResult`, re-stream.

Verify: `cargo test -p vmux_agent` — drain_stream + continue_after_tool tests pass.

### Group C — Run-state telemetry + toast events

Goal: surface agent run-state transitions to UI + toast notifications on errors.

- C1. New `crate::run_state_kind`: `AgentRunStateKind` discriminant enum + `LastRunStateKind`
  Bevy component.
- C2. New `crate::toast`: `AgentToast` event + `ToastLevel` enum. Uses rkyv for transport.
- C3. New `systems/surface_errors`: detects Idle→Errored transition, appends inline message,
  fires `AgentToast`.
- C4. Update `systems/process_input` to drive real SSE via strategy + IoTaskPool (not just
  echo). Errors when no strategy registered for the resolved provider/model.

Verify: tests pass; `surface_errors` smoke tests check toast emission shape.

### Group D — URL routing additions in `vmux_agent`

Goal: bare `vmux://agent/` URL resolves to default-app provider.

- D1. Add `AgentUrl::AppDefault` variant. Parse logic: bare scheme → `AppDefault`, full
  `vmux://agent/app/{provider}/{model}` → `App { provider, model, sid }`.
- D2. `AgentVariant::from_url_segment` accepts empty segment → resolves to `App`.
- D3. Reject trailing garbage in URLs (test: `trailing_garbage_rejected`).

Note: on main, the file is `url.rs`; feature branch renamed it to `kind.rs`. **Keep main's
name `url.rs`.** All `AgentUrl::*` and `AgentKind` types live there.

Verify: URL parser tests in `url::tests` cover all variants + round-trips.

### Group E — `vmux_desktop` integration (command bar + deep links)

Goal: command bar routes bare `vmux://agent/` via resolver; toasts surface to JS.

- E1. Built-in providers registered on startup; settings becomes overrides only (provider list
  from settings.ron only adds/overrides built-ins).
- E2. Command bar "New chat" default entry uses `resolve_default_app_provider`.
- E3. Command bar new-tab mode also routes bare `vmux://agent/` via default resolver.
- E4. `AgentToast` events emitted to JS via `BinJsEmitEventPlugin`.

Note: these changes touch `vmux_desktop` (or possibly `vmux_command` now that command bar moved).
Map feature branch's `crates/vmux_desktop/src/agent.rs` and `command_bar.rs` changes onto main's
equivalent locations (`vmux_command/src/app.rs`, `vmux_desktop/src/command_bar.rs`).

Verify: `cargo test -p vmux_desktop -p vmux_command` passes; agent deep-link tests + command
bar new-tab tests cover bare URL routing.

### Group F — Polish + cleanup

- F1. `rustfmt` warn! call onto one line in `vmux_desktop` (cosmetic).
- F2. Delete the plan doc (`docs/plans/2026-05-21-reimplement-gui-agent-on-main.md`) per
  AGENTS.md once all groups land.
- F3. Run full pre-push checks on changed crates (fmt + clippy + test).
- F4. Swap branches:
  ```
  git branch -m vmx-gui-agent vmx-gui-agent-old
  git branch -m vmx-gui-agent-rebased vmx-gui-agent
  git push --force-with-lease origin vmx-gui-agent
  git branch -D vmx-gui-agent-old  # after PR is merged
  git tag -d vmx-gui-agent-pre-rebase  # after PR is merged
  ```

## Sequencing

Each group should be 1-3 commits, building incrementally on top of `origin/main`. Suggested
session granularity:

1. Session 1: Group A (provider infra) — ~3-4 hours.
2. Session 2: Group B (tool dispatch) — ~2 hours.
3. Session 3: Group C (run-state + toast) — ~1-2 hours.
4. Session 4: Group D (URL routing) — ~1 hour.
5. Session 5: Group E (vmux_desktop integration) — ~2-3 hours. Most fragile due to refactored
   command bar location.
6. Session 6: Group F (polish, push, swap) — ~30 min.

Total: ~10-13 hours of careful work.

## Verification protocol

After each subtask (A1, A2, etc.):
1. `cargo fmt -p <crate>` — ensure formatting.
2. `env -u CEF_PATH cargo clippy -p <crate> --all-targets -- -D warnings`.
3. `env -u CEF_PATH cargo test -p <crate>`.

After each Group, verify with the changed-crate loop from AGENTS.md:
```
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

## Reference: feature branch artifacts

To inspect what feature branch had for a specific file:
```
git show vmx-gui-agent-pre-rebase:crates/vmux_agent/src/<path>
```

Logical-name mappings (feature → main):
- `crates/vmux_agent/src/app.rs` → `crates/vmux_agent/src/client/page/strategy.rs`
- `crates/vmux_agent/src/app_plugin.rs` → port into `crates/vmux_agent/src/client/page/agent.rs`
- `crates/vmux_agent/src/cli_trait.rs` → `crates/vmux_agent/src/client/cli/strategy.rs`
- `crates/vmux_agent/src/{claude,codex,vibe}.rs` → `crates/vmux_agent/src/client/cli/{claude,codex,vibe}.rs`
- `crates/vmux_agent/src/kind.rs` → `crates/vmux_agent/src/url.rs`
- `crates/vmux_agent/src/{providers/,http.rs,tools.rs,tool_dispatch.rs,toast.rs,run_state_kind.rs}` → new
- `crates/vmux_agent/src/systems/{continue_after_tool,surface_errors}.rs` → new

Type renames (feature → main):
- `AppAgentStrategy` → `AgentPageStrategy`
- `AgentVariant::App` → `AgentVariant::Page`
- `register_app` → `register_page`
- `get_app_by_provider_model` → `get_page_by_provider_model`
- `app_strategies` → `page_strategies`
- `Arc<dyn AppAgentStrategy>` (feature) → keep `Box<dyn AgentPageStrategy>` (main); revisit if
  shared ownership is genuinely needed by ported code.
- `app_providers` (settings field) → `page_providers` (whatever main uses).

## Rollback

If reimplementation goes off-rails:
```
git checkout vmx-gui-agent          # original feature branch unchanged
git branch -D vmx-gui-agent-rebased # discard rebased branch
git tag -d vmx-gui-agent-pre-rebase # if no longer needed as marker
```
