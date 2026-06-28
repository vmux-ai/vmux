# Agent compose-page rewrite

Replace the in-terminal DOM composer (which fought keyboard suppression, focus,
and terminal lifecycle — proven non-working in logs: no keystroke ever reached
it) with a dedicated **compose page**: a normal CEF page (not a terminal, so
keyboard is NOT suppressed → typing works, like the setup page / command bar).
Open agent → compose page (rain + typewriter examples + real input) → on Enter,
spawn the CLI agent and deliver the prompt into its PTY.

## Keep (delivery primitives, already correct)
- `vmux_terminal::shell_input::bracketed_paste_input`
- `vmux_terminal::plugin::BufferedAgentPrompt`, `agent_prompt_flush_bytes`,
  `flush_buffered_agent_prompt` (delivers buffer into PTY on `alt_screen` + submit)

## Revert (the in-terminal approach + its fights)
- `vmux_browser/src/lib.rs`: `suppress.0 = terminal_q.contains(browser_e)` (drop the
  agent_loading flip + query + diagnostic log). Terminals must stay suppressed.
- `vmux_terminal/src/plugin.rs`:
  - `handle_terminal_keyboard`: drop the agent-loading gate (restore native PTY keys).
  - `clear_agent_loading`: restore alt-screen clear for agents (drop 180s
    `AGENT_COMPOSE_TIMEOUT`); restore `&ProcessId` + `mode_map` params.
  - Remove `on_agent_prompt_draft` observer + its registration + `AgentPromptDraftEvent`
    from the `BinEventEmitterPlugin` tuple; remove `merge_prompt_draft`.
  - Remove the `vmux-kbd` diagnostics.
  - Restore the two clear-loading tests to alt-screen behavior.
- `vmux_terminal/src/page.rs`: remove the composer (textarea, `PromptGhost`,
  focus helpers, container key/mouse gates, `prompt_draft`/`prompt_committed`
  signals, examples const, `Rc`/`RefCell` imports). Loading overlay returns to
  rain + booting card only. The typewriter/examples UI **moves** to the compose page.

## Add

### vmux_core (`agent.rs`, `event.rs`)
- `SpawnAgentInStackRequest.initial_prompt: Option<String>`.
- `AgentKind::compose_url()` → `vmux://compose/<segment>/`.
- `event.rs`: `AGENT_COMPOSE_SUBMIT_EVENT` const + `AgentComposeSubmitEvent { agent: String, text: String, submit: bool }` (rkyv+serde). `submit=false` = cancel (spawn with no prompt).

### vmux_agent (`compose.rs` + `compose/page.rs` + reuse)
- `compose.rs` (host gate `#[cfg(not(wasm))]`): `PageManifest { host: "compose", ... command_bar:false }`; `AgentComposePlugin` registers `BinEventEmitterPlugin::<(AgentComposeSubmitEvent,)>::for_hosts(&["compose"])` + `on_agent_compose_submit` observer; `attach_compose_to_stack(kind, stack, ...)` (mirror `attach_cli_setup_to_stack`, url = `kind.compose_url()`, `Browser` + `CefKeyboardTarget`).
- `compose/page.rs` (`#[cfg(wasm)]`): read kind segment from `window.location.pathname`; render `MatrixRain` + agent favicon/label + a focused `<textarea>` + the typewriter ghost (moved from terminal page). Enter → emit `AgentComposeSubmitEvent{agent, text, submit:true}`; Esc → `submit:false`.
- `on_agent_compose_submit`: find the stack hosting the compose page (by url/segment); `spawn_agent.write(SpawnAgentInStackRequest{ kind, cwd, session_id:None, stack, initial_prompt: submit.then_some(text) })`.
- `handle_spawn_agent_requests`: after spawning the terminal, if `req.initial_prompt` is Some & non-empty, `commands.entity(terminal).insert(BufferedAgentPrompt{ text, submit:true })`. (Existing flush delivers on alt_screen.)
- Agent open (bare `vmux://agent/<kind>/`, the `None`/`from_url_segment` branch in the open handler): if CLI installed → `attach_compose_to_stack` instead of `spawn_agent.write`. (Not-installed still → setup page. CLI url `…/cli/<sid>` still spawns directly for resume.)

### vmux_server (`lib.rs`)
- Add `render_compose: "compose" => vmux_agent::compose::page::Page` to the render macro.
- Ensure `../vmux_agent/src` tracked for wasm rebuild + tailwind `@source` (it hosts the page).

## Verify
- `cargo check -p vmux_terminal --target wasm32` + `-p vmux_server --target wasm32` (pages compile).
- `cargo check -p vmux_agent -p vmux_browser -p vmux_terminal` (host).
- `cargo test -p vmux_core -p vmux_terminal --lib`; fmt; clippy.
- Runtime (user): open an installed agent → compose page with working input + typewriter → type → Enter → agent spawns and runs the prompt.
