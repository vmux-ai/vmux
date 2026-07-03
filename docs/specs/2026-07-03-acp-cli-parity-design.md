# ACP ↔ CLI Agent Parity — Design

Date: 2026-07-03
Branch: `feat/acp-cli-parity`
Status: proposed

## 1. Problem

ACP agents (native chat hosted via the Agent Client Protocol, added in PR #216) are
second-class citizens compared to CLI/Page agents. They render and stream, but they are
missing the identity, focus-ring, notification, and command-bar treatments that CLI agents
get: no profile avatar, no focus-ring avatar badge, no "finished" notification / done-dot,
absent from the team roster, generic tab icon, unbranded loading state, and no session
continuity across restart.

### Root cause (single, structural)

CLI agents co-locate every identity component on one entity — the child terminal webview:

```
TerminalLaunch + vmux_core::agent::AgentSession{kind} + ProcessId + team::Profile + team::Agent
```
(`crates/vmux_agent/src/plugin.rs:2557-2577`)

ACP splits identity across two entities and omits the shared components entirely:

- Stack entity: `AcpSession`, `AgentMessages`, `AgentApprovalPolicy`, `AgentRunState`, `PageMetadata`
  (`crates/vmux_agent/src/plugin.rs:354-371`)
- Child webview: only the `anchor: ProcessId` (`plugin.rs:374-378`)

There is **no `team::Profile`, no `team::Agent`, no `AgentKind`, no `TerminalLaunch`** anywhere on
an ACP session. And ACP identity is a registry string (`agent_id` / `name` / `icon` URL from
`RegistryAgent`, `crates/vmux_agent/src/acp_registry.rs:24-36`), not the fixed `AgentKind` enum
(`crates/vmux_core/src/agent.rs:19`) that every downstream consumer branches on.

Every gap in §2 is a symptom of this one omission.

## 2. Parity audit (inventory)

Severity: 🔴 absent/broken · 🟡 degraded · ✅ works. "shared" = also affects Page agents; fixing
via the shared chat page fixes both.

### A — Identity / avatar
| # | Gap | Mechanism | Sev |
|---|-----|-----------|-----|
| A1 | Absent from team side-sheet (no avatar/name/dots, not focusable) | `build_team_members` query requires `Profile`+`Agent` (`vmux_team/src/plugin.rs:135-193`) | 🔴 |
| A2 | No avatar logo on native focus-ring badge (own + follow panes); ring *color* survives | `agent_kind()`→`None` for ACP (`plugin.rs:758-766, 844-852`) → badge cleared (`vmux_browser/src/lib.rs:1220,1231`) | 🟡 |
| A3 | Chat header: no avatar; name is URL-scraped raw id; static green dot lies about state | `ChatSnapshot` carries no name/icon; header scrapes `location.pathname` (`chat_page/page.rs:82-87`, `chat_page/event.rs:19-29`) | 🔴 shared |
| A4 | Tab strip: generic Sparkles icon for every chat agent; registry `icon` unused | `PageMetadata.icon` unset at attach (`plugin.rs:354-359`); favicon fallback knows only 4 hardcoded ids (`vmux_ui/src/favicon.rs:16-43`) | 🟡 shared |

### B — Focus ring
| # | Gap | Mechanism | Sev |
|---|-----|-----------|-----|
| B1 | Avatar badge missing (= A2). Base ring + per-agent ring color both work (anchor-hash key). | color via `agent_ring_rgb(key)` (`vmux_browser/src/lib.rs:1025,1032`); only `AgentKind` logo missing | 🟡 |
| B2 | ACP chat pane never emits `ActivatePane` — its own home pane isn't claimed as the agent's active pane | follow panes are claimed via anchor (`claim_browser_pane`/`handle_agent_file_touch`); the chat webview is not | 🟡 |

### C — Notifications
| # | Gap | Mechanism | Sev |
|---|-----|-----------|-----|
| C1 | No automatic "agent finished" at all — Streaming→Idle emits no `AgentAttention` | `consume_page_agent_stream` sets run-state only (`client/page/plugin.rs:185-215`) | 🔴 shared |
| C2 | Done-dot never lights (tab strip amber dot + team list) | `AgentDoneUnseen` only set via bell (Agent-gated) or `Notify` tool (`plugin.rs:600-665`) | 🔴 |
| C3 | Bell→attention skips ACP (moot: ACP has no PTY bell — attention must come from C1) | `agent_bell_to_attention` requires `With<team::Agent>` (`plugin.rs:564-578`) | 🔴 |
| C4 | Even explicit `Notify` MCP tool fails — caller unresolved | `AgentSpaceWriters.agents` requires `team::Agent` (`plugin.rs:511-519, 979-993`) | 🔴 |
| C5 | OS notification name degrades to generic "Agent" | `mark_agent_done` reads `Profile` for name (`plugin.rs:606-640`) | 🟡 |
| C6 | Error toast never fires (error still shows in chat) | `surface_errors` requires `AgentSession` (`systems/surface_errors.rs:9-39`) | 🟡 |

### D–H — other surfaces
| # | Gap | Mechanism | Sev |
|---|-----|-----------|-----|
| D1 | cwd absent from command-bar "open-pane dirs" | `update_work_dirs_snapshot` queries `TerminalLaunch` only (`command_bar/work_snapshot.rs:60-104`); ACP cwd in `AcpSession.cwd` | 🟡 shared |
| D2 | Recent files | file-follow → real `file://` pages via shared path | ✅ |
| E1 | No branded loading/install identity — generic mono dot-bounce vs CLI accent+favicon+matrix splash | `chat_page/page.rs:122-142` vs `vmux_terminal/src/page.rs:362-419` | 🟡 shared |
| E2 | Status dot static green (lies) | `chat_page/page.rs:83` not wired to `AgentRunState` | 🟡 shared |
| F1 | Transcript + backend session lost on restart — sid regenerated, `AgentMessages` default, no ACP `loadSession` | `AcpSession` not `Serialize`/`Reflect`; reopen mints new uuid (`plugin.rs:2345`) | 🔴 separable |
| G1 | Window/OS title has no agent identity | absent for CLI too — no gap | ✅ |
| H1 | Chat run-state (installing/streaming/awaiting/errored) | works for ACP | ✅ |

## 3. Goals / non-goals

**Goals.** Bring ACP agents to full parity with CLI agents across identity/avatar (A),
focus-ring badge (B), notifications (C), command-bar + loading polish (D/E), and session
resume (F). Prefer a single generalized identity model over per-kind branching so Page agents
and future registry agents benefit automatically.

**Non-goals.** Changing the ACP transport/daemon architecture; adding a per-mode padding knob;
window/OS-title identity (G1 — absent for everyone, out of scope). No new workspace crates
(reuse `vmux_core`/`vmux_agent`/`vmux_browser`; external deps are fine).

## 4. Approach

**Chosen: generalize the identity model.** Make `Profile`/avatar source-agnostic — CLI sources
identity from `AgentKind`, ACP from its `RegistryAgent`. Every consumer (team roster, native
badge, tab icon, notifications, ring) reads the generalized identity and stops branching on
`AgentKind`.

Rejected alternatives:
- *Parallel ACP path* (`Or<(With<Agent>, With<AcpSession>)>` + duplicate roster/badge branches):
  two code paths to sync forever, no Page/future benefit, "icon everywhere" bolted on twice.
- *Fake `AgentKind::Acp`*: `AgentKind` drives `executable()`/`TerminalKind`/PTY launch; stuffing
  registry strings in corrupts its meaning.

## 5. Design

### §1 Identity model (`crates/vmux_core/src/team.rs`)

`AvatarSpec` stays wasm-safe (serde only, no `Component`) — it is serialized to the DOM — and
gains an optional icon source:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvatarIcon {
    Kind(AgentKind),   // CLI/Page: resolves to a bundled brand logo
    Remote(String),    // ACP: registry icon URL (https, often SVG)
}

pub struct AvatarSpec {
    pub initials: String,
    pub color: String,
    pub icon: Option<AvatarIcon>,   // NEW
}
```

- `AvatarSpec::for_agent(kind)` → `icon: Some(Kind(kind))` (unchanged initials/color).
- `AvatarSpec::for_registry(name, icon_url, id)` (NEW) → `initials = initials_of(name)`,
  `color = hash_color(id)` (stable FNV→HSL hex, same technique as `agent_ring_rgb`),
  `icon = icon_url.map(Remote)`.

`team::Agent` becomes a pure session marker — its only use of `kind` was the roster icon URL
(`vmux_team/src/plugin.rs:173`), which now comes from `Profile.avatar`:

```rust
pub struct Agent { pub sid: String }   // drop `kind: AgentKind`
```

`Profile` is unchanged in shape (`{ name, avatar }`) and becomes the single identity component
for all agent kinds. `Profile::registry(name, icon_url, id)` (NEW) builds an ACP profile.

`attach_acp_agent_to_stack` (`plugin.rs:344-379`) attaches `Profile` + `Agent{sid}` sourced from
the `RegistryAgent` (looked up in `AcpCatalog` by `agent_id`; fall back to `AcpAgentConfig.name`
and no icon when absent). Update `attach_page_agent_to_stack` (`plugin.rs:319-335`) and the CLI
spawn (`plugin.rs:2568-2577`) to the new `Agent{sid}` shape.

### §2 Avatar rendering (four surfaces read the generalized avatar)

1. **Team roster (DOM).** `TeamMemberRow` gains an `icon` field (the resolved URL, or empty).
   `build_team_members` (`vmux_team/src/plugin.rs:135-193`) now matches ACP (it has `Profile`+
   `Agent`). Frontend `TeamAvatar`: render `<img src=icon>` when present, else initials+color.
2. **Native focus-ring badge (macOS `CALayer`).** `windowed_ring_for`/`sync_windowed_frames`
   (`vmux_browser/src/lib.rs:1006-1231`) resolve the avatar from the agent's `Profile` instead of
   `AgentKind`. `Kind(k)` → existing `agent_logo(k)` PNG. `Remote(url)` → **fetch + cache +
   rasterize** to premultiplied RGBA (SVG via `usvg`/`resvg`+`tiny-skia`; raster via `image`),
   cached under `~/.vmux/agents/icons/<id>.png`, keyed by id, re-fetched when the URL changes.
   This is the highest-effort item ("registry icon everywhere").
3. **Tab strip.** Set `PageMetadata.icon = PageIcon::favicon(registry_icon)` in
   `attach_acp_agent_to_stack` so the tab shows the brand icon instead of the Sparkles default.
4. **Chat header + empty hero (A3).** Add `name` + `icon` (+ `accent_color`, see §5) to
   `ChatSnapshot` (`chat_page/event.rs:19-29`); `snapshot_of` (`chat_page.rs:76-102`) reads them
   from the session's `Profile`. Header/hero render the real avatar + name instead of scraping
   the URL. Fixes Page agents too.

### §3 Focus ring (B)

`ActiveStack.kind: Option<AgentKind>` (`vmux_layout/src/active_panes.rs:17-25`) is replaced by
`agent: Option<Entity>` — the agent session entity, which the badge renderer resolves to a
`Profile` (single source of truth, always fresh; no avatar data duplicated into the resource).
Every `ActivatePane` emitter (`claim_browser_pane` `plugin.rs:771-785`; `handle_agent_file_touch`
`plugin.rs:936-948`) fills it with the resolved agent entity rather than calling `agent_kind()`.
`windowed_ring_for` (`vmux_browser/src/lib.rs:1006`) looks up that entity's `Profile.avatar` to
pick the badge image. Follow-pane badges then render for any agent (A2/B1).

B2: emit an `ActivatePane` for the ACP session's own chat webview so its home pane is registered
under `ProfileId::Agent(key)` and shows the agent ring + badge, matching how a CLI agent's home
terminal is claimed. (Verify the CLI home-pane claim path during implementation and mirror it.)

### §4 Notifications (C)

New transition detector in `consume_page_agent_stream` (`client/page/plugin.rs:185-215`): when a
session's run-state goes **Streaming → Idle**, emit
`AgentAttention { entity, title: "{Profile.name} finished", body }`, gated on `!agent_is_viewed`
(reuse `plugin.rs:590`) and the existing 3s dedup window. This single fix lights the done-dot
(C2), fires the OS notification (via `mark_agent_done`), and works for **both ACP and Page**
(both share this consumer and now both have `Profile`).

- C4: generalize `AgentSpaceWriters.agents` caller resolution to also resolve an ACP caller —
  match the `AcpSession` stack whose child webview carries the `Notify` call's `anchor`
  `ProcessId` (via `ChildOf`). Keep the anchor on the webview only (adding it to the stack would
  double-match the follow resolvers' `(Entity, &ProcessId, &ChildOf)` query).
- C5: `mark_agent_done` now finds `Profile` on ACP → real "{name} finished" text.
- C6: widen `surface_errors` (`systems/surface_errors.rs:16`) to
  `Or<(With<AgentSession>, With<AcpSession>)>` so ACP `Errored` also produces a toast.
- C3: no change — ACP has no PTY bell; C1 covers the "finished" outcome.

### §5 Command bar + loading polish (D/E)

- D1: add a second source to `update_work_dirs_snapshot` (`command_bar/work_snapshot.rs:60-104`) —
  `Query<&AcpSession>` reading `.cwd` — merged with the terminal cwd list. (Page agents are web,
  no cwd — N/A.)
- E1: give the chat page a branded loading/install state. `ChatSnapshot` carries `accent_color`
  (= `Profile.avatar.color`) and `icon`; the page renders the install/streaming/idle-empty states
  with the agent icon + accent, mirroring the CLI splash's intent (`vmux_terminal/src/page.rs:362-419`).
- E2: wire the header status dot (`chat_page/page.rs:83`) to the `status` already in `ChatSnapshot`
  (idle=green, streaming=amber pulse, installing=blue, awaiting=purple, errored=red).

### §6 Session resume (F) — last phase, capability-gated

ACP uses a single-segment URL by design (`vmux://agent/<id>`), so the sid cannot live in the URL
(2-segment collides with the Page form). Persist it via ECS reflection instead:

- Make `AcpSession` derive `Reflect + Serialize + Deserialize` + `#[reflect(Component)]` and
  register it for per-profile ECS persistence so `sid` (and `agent_id`/`cwd`) round-trip.
- Transcript: make `AgentMessages` `Reflect`+persisted for display continuity (offline fallback).
- Backend continuity: on restore, if the restored `AcpSession` has a sid and the agent advertises
  the ACP `loadSession` capability, reconnect via `loadSession(sid)` (daemon replays history);
  otherwise fall back to today's fresh-session behavior. Gate the whole phase behind a capability
  check + setting; degrade gracefully. This phase is protocol-dependent and the most likely to
  slip — it ships behind A–E.

## 6. Phasing

Each phase is independently testable and shippable:

1. **§1 + §2** — identity model + avatar across the four surfaces (unblocks the rest).
2. **§3** — focus-ring badge on own + follow panes.
3. **§4** — notifications (done-dot, OS notify, error toast; also fixes Page).
4. **§5** — command-bar cwd + branded loading + live status dot.
5. **§6** — session resume (gated, last).

## 7. Testing

Bevy system/message integration tests (per project convention — send typed messages, run
schedules, assert ECS state/messages), not ad-hoc helper calls:

- §1: `attach_acp_agent_to_stack` inserts `Profile`+`Agent`; `AvatarSpec::for_registry` derives
  stable color per id and `initials_of(name)`; `for_agent` still yields `Kind`.
- §2: `build_team_members` includes an ACP entity and serializes its icon URL; `TabRow`/
  `PageMetadata.icon` populated from registry icon; `ChatSnapshot` carries name+icon. Native
  rasterization: unit-test SVG→RGBA and the id-keyed cache (path + invalidation on URL change).
- §3: `ActivatePane` for follow + chat panes carries the ACP avatar; renderer selects it.
- §4: driving a `Streaming→Idle` transition emits exactly one `AgentAttention` (deduped, only when
  unviewed) → `AgentDoneUnseen` set + `OsNotify` for both ACP and Page; `surface_errors` emits a
  toast for an errored `AcpSession`.
- §5: work-dir snapshot includes an ACP `cwd`; status-dot mapping covers all `AgentRunState`.
- §6: `AcpSession`/`AgentMessages` round-trip through reflection persistence; restore path
  reuses the persisted sid; fresh-session fallback when `loadSession` is unavailable.

Manual runtime verification deferred to one pass at the end (per project workflow): open an ACP
agent, confirm avatar in tab/team/badge/header, run a turn to completion unfocused → done-dot +
OS notification with the real name, restart → transcript/session restored (§6).

## 8. Risks

- **Native remote-icon rasterization (§2.2)** is the biggest single effort and adds an SVG dep;
  mitigate by caching aggressively and falling back to initials+color badge on fetch/parse
  failure so the badge is never blank.
- **`team::Agent.kind` removal** touches `attach_page_agent_to_stack`, the CLI spawn, and the
  roster icon derivation; a compile-time sweep of `Agent {` / `.kind` on `team::Agent` covers it.
- **`AvatarSpec`/`ChatSnapshot`/`TeamMemberRow` are wire contracts** compiled for wasm; keep new
  fields serde-only and cfg-gate nothing bevy into them (`AvatarSpec` must stay `Component`-free).
  Typecheck pages with `cargo check --target wasm32` before the full build.
- **§6 protocol dependence** — `loadSession` support varies by agent; ship gated with a clean
  fresh-session fallback.
