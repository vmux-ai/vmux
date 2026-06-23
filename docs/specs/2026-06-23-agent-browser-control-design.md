# Agent-Driven Browser Control via MCP (native, no-eval)

Date: 2026-06-23
Status: Approved design, pending implementation plan
Branch: `feat/agent-browser-control`

## Goal

Expose MCP tools so an in-app assistant (vibe / Le Chat, or vmux's own agent) can drive the user's **live, logged-in** browser pages: open, navigate, read the DOM, find elements, and interact (mouse, typing, scrolling) on its own.

## Locked decisions

1. **Perception + addressing = hybrid.** A compact semantic snapshot with stable per-snapshot `ref` ids for most interaction, plus raw pixel coordinates as an escape hatch (canvas/maps/drag), paired with the existing `vmux_screenshot`.
2. **Consumer = in-app assistant on live sessions.** High trust, high power — the agent acts as the user on real authed pages. No permission gate (explicit user choice).
3. **Act → observe loop.** Every action auto-waits for the page to settle, then returns a fresh snapshot, so the agent always has current state in one MCP round-trip (fits the one-shot MCP constraint and vibe's 60s tool timeout).
4. **No eval, native only.** Never inject or evaluate JavaScript in pages. Read the DOM through CEF's native DOM API (`CefFrame::VisitDOM` → `CefDOMNode`), interact through native input events (`send_mouse_*`, `send_key`). No WASM is injected into pages (same-origin makes externally-hosted WASM useless against third-party pages, and injecting a WASM driver would still need an in-page JS loader subject to CSP).

## Non-goals (v1 YAGNI)

- No macro / record-replay.
- No drag-and-drop (mouse-down/move/up sequence) — note as future.
- Main frame only — no deep iframe traversal (VisitDOM is per-frame) — note as future.
- No `wait_for_selector` custom predicates — generic auto-wait only.
- No screenshot embedded in the snapshot — `vmux_screenshot` stays a separate tool.
- No cross-snapshot stable refs — per-snapshot integer refs (enhancement noted).

## Architecture / data flow

Reads cross CEF's two processes natively; no JS runs in the page.

```text
MCP tool ─► AgentQuery ─► Bevy RequestSnapshot(entity)
  └► browser proc: frame.send_process_message(RENDERER, "VMUX_SNAPSHOT", id)
       └► render proc handler: frame.visit_dom(visitor)        ← native walk, no eval
            visitor (in visit() callback) builds {url,title,nodes[]} with refs+bboxes
            → frame.send_process_message(BROWSER, "VMUX_SNAPSHOT_RESULT", id, payload)
       └► browser proc inbound handler → channel keyed by id → resolves AgentQueryResult
  ◄── snapshot text back to the agent
```

Interactions (click / type / scroll) need **no** render-process round-trip: they resolve `ref → cached bbox` from the last snapshot for the target webview and fire **native** input on the webview host, then auto-wait + re-snapshot.

## MCP tool set

All tools take an optional `target`; default = `FocusedStack` active webview. All action tools return a fresh snapshot.

| Tool | Params | Behavior |
|------|--------|----------|
| `vmux_browser_snapshot` | `target?`, `mode=interactive\|text` | Perception primitive. `interactive` = filtered node list; `text` = readable innerText for articles. |
| `vmux_browser_navigate` | `url`, `new_tab?`, `target?` | Upgrade existing tool: load, auto-wait, return snapshot. `new_tab` covers "open browser". |
| `vmux_browser_click` | `ref` \| `x,y`, `button?`, `target?` | `ref` → cached bbox center → native click; or raw coords (vision escape hatch). |
| `vmux_browser_type` | `ref?`, `text`, `submit?`, `target?` | Focus bbox (native click) → native `send_key` per char. `submit=true` sends Enter. |
| `vmux_browser_press_key` | `key`, `modifiers?`, `target?` | Enter/Tab/Esc/Arrows/Cmd+A… reuses existing keyboard map. |
| `vmux_browser_scroll` | `ref` \| `x,y`, `dx`, `dy`, `target?` | Native `send_mouse_wheel` at point. |
| `vmux_browser_find` | `query`, `target?` | Filtered subset of the snapshot (name/role substring match). Convenience over full snapshot. |
| `vmux_browser_go_back` / `go_forward` / `reload` | `target?` | Upgrade existing to auto-wait + return snapshot. |

## Snapshot shape (token-frugal)

Flat list of *interesting* nodes only — interactive (`a`, `button`, `input`, `select`, `textarea`, `[role]`, `[tabindex]`, click-handler-ish) plus landmarks / headings / labels. Skip `script` / `style` / hidden (empty bbox = hidden).

```jsonc
{
  "url": "https://…",
  "title": "…",
  "nodes": [
    { "ref": 12, "role": "button", "name": "Sign in", "bbox": [x, y, w, h] },
    { "ref": 13, "role": "textbox", "name": "Email", "value": "", "bbox": [x, y, w, h], "state": ["required"] }
  ],
  "truncated": false
}
```

`viewport` and `scroll` are deferred to the interaction follow-on plan (not in the v1 read-path data model).

- `role` = aria `role` attribute, else tag-derived.
- `name` = `aria-label` ‖ `alt` ‖ `title` ‖ `placeholder` ‖ trimmed innerText (capped). Approximate accessible name — no full a11y engine in v1.
- `value` = input/textarea value.
- `state` = optional flags derived from attributes (`disabled`, `required`, `checked`, …).
- `ref` = per-snapshot integer. Browser side caches `ref → bbox` (+ role/name) for the active webview; invalidated on the next snapshot/navigation.
- Cap node count; set `truncated` when exceeded.

## Targeting

Optional `target` = `pane:<bits>` | `stack:<bits>`; default = `FocusedStack` active webview. Reuses `target::active_webview_for_tab` and the existing navigate resolution in `vmux_browser` — same convention as current MCP tools.

## Auto-wait (approximate, no JS)

- Primary signal: CEF loading-state (`OnLoadingStateChange` → false / load-end) for real navigations.
- SPA updates emit no load event and we can't run a MutationObserver without eval, so add a short fixed debounce (~400ms) after every action, then snapshot.
- Total capped ~15s (well under vibe's 60s tool timeout). Agent can re-snapshot if a slow SPA hasn't settled.
- This approximation is the one accepted cost of the no-eval decision.

## Interaction details

- **Click by ref:** look up cached bbox (CSS px, frame coords) → center → device px (`× device_scale_factor`) → `Browsers::send_mouse_click` (OSR path calls `host.set_focus(true)` first; works regardless of native window focus).
- **Click by coords:** agent-supplied viewport CSS px → same path.
- **Type:** click bbox center to focus → `send_key` per char (real key events; React/IME see them). `submit` → Enter.
- **press_key:** named key + modifiers (cmd/ctrl/alt/shift) → `send_key`, reusing `patches/bevy_cef-0.5.2/src/keyboard.rs` mapping.
- **scroll:** `send_mouse_wheel(dx, dy)` at bbox/coords. Off-screen targets: wheel by delta (no `scrollIntoView`, which would need eval).

## Where code lands

- `patches/bevy_cef_core-0.5.2`: render-process visitor + `VMUX_SNAPSHOT` / `VMUX_SNAPSHOT_RESULT` process messages; browser-process inbound routing → channel. **Confirm/patch the `cef` binding to expose `DomVisitor` / `DomNode` / `DomDocument`.**
- `patches/bevy_cef-0.5.2`: Bevy `RequestSnapshot` message + result channel (mirror existing `src/common/ipc/js_emit.rs` plumbing); ensure loading-state is surfaced for auto-wait.
- `vmux_browser`: handler systems, targeting, auto-wait state machine, `ref → bbox` cache resource, native input dispatch, node filter + role/name derivation, find filter.
- `vmux_service`: new `AgentQuery` / `AgentCommand` variants (Snapshot, Click, Type, PressKey, Scroll, Find; Navigate gains snapshot return).
- `vmux_agent`: command → Bevy message bridge (`plugin.rs`).
- `vmux_mcp`: tool defs + dispatch (`tools.rs`) + response text (`protocol.rs`).

## Testing

Follows AGENTS.md: Bevy message + system integration; assert on ECS state / emitted messages, no ad hoc helper bypass.

- **Unit (no CEF):** abstract the DOM walk behind a trait → snapshot serializer, node filter, role/name derivation, `ref → bbox` cache, click coord math (device-scale) all testable against synthetic node fixtures. find filter.
- **Bevy integration:** register messages + systems, send `RequestSnapshot` / `RequestClick` / etc., run schedules, assert emitted `AgentQueryResult` / native-input messages.
- **Auto-wait state machine:** drive loading-state messages, assert resolve-after-load + debounce and the timeout cap.
- **Manual runtime:** user runtime-tests on a real site (golden path + an authed form).

## Feasibility risks (ranked)

1. **`cef` Rust binding may not expose `VisitDOM` / `CefDOMNode`.** First implementation step is a spike to confirm; patch the binding if missing (CEF crates are already patched). Highest risk — gates the whole read path.
2. **`get_element_bounds` coordinate space vs OSR texture + `device_scale_factor`.** Verify clicks land on target; likely a scale multiply. Medium.
3. **SPA quiescence is approximate** (load-state + debounce). Accepted.
4. **Hidden-element detection** via empty-bbox heuristic (no computed style). Minor.

## Security note

Live, high-trust by explicit user choice: these tools read authed DOM and submit forms as the user, with no gate. Natural scoping limit: tools act only on the user's already-open vmux pages (plus new tabs via `navigate new_tab`), not arbitrary background sessions.
