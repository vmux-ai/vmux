# Per-Profile Active Pane + Multi-Focus Rings

**Date:** 2026-06-27
**Status:** Design — approved direction, pending spec review
**Related:** PR #180 (visible agent browser research), `docs/specs/2026-06-23-agent-browser-control-design.md` (click/type)

## Problem

vmux has a single global active pane (`FocusedStack`, derived each frame from `LastActivatedAt` stamps); the focus ring follows it. Consequences:

- Agent browser tools (`browser_navigate`/`snapshot`/`scroll`) with no explicit `pane` fall back to this global focus — i.e. the **human's** focused pane — so an agent can hijack the user's pane.
- Agent actions (open/spawn) stamp `LastActivatedAt`, moving the **user's** focus ring.
- There is no notion of "this agent's own active pane" distinct from the human's.

This does not scale to where vmux is going: **multiple participants** — several humans and several agents, some **remote in the future** — each needing their own active pane and their own visible focus.

## Goal

- Every **profile** (a participant: a human or an agent; local now, remote-capable later) has its own active pane.
- The layout shows a focus ring **per profile with a visible active pane**, color-coded by that profile's identity (so the user can see "my focus" and "agent X's focus" at once).
- A profile's active pane changes **only** in response to that profile's own actions. No participant's actions move another participant's ring.
- Agent browser tools target the **acting agent's own** active pane (resolved via its anchor), never another profile's.
- Only the **local human** whose machine this is drives OS keyboard/mouse focus.

## Non-goals

- Remote transport/networking. v1 is local (one local human + local agents), but the data model and activation flow are designed so a remote participant is just another `ProfileId` feeding the same message — no consumer reshaping required later. (YAGNI: no sync protocol now.)
- Redefining identity. Reuse existing user/agent identities (team facepile + agent session/anchor).

## Data model

The single global `FocusedStack` is replaced by a per-profile map. Uniform — the local human is one profile among many, not a privileged singleton in the structure.

```rust
/// Stable identity of any participant. Remote-ready: a remote human/agent is
/// just another ProfileId. v1 constructs only Local + Agent.
pub enum ProfileId {
    Local,              // this machine's human (drives OS focus)
    Agent(AgentKey),    // an agent, keyed by its session id
    // future: Remote(Uuid)
}

#[derive(Resource, Default)]
pub struct ActivePanes(pub HashMap<ProfileId, ActiveStack>);

pub struct ActiveStack { pub tab: Option<Entity>, pub pane: Option<Entity>, pub stack: Option<Entity> }
```

**Activation is message-driven** (per AGENTS.md: prefer message + system integration, and so remote sources can feed it):

```rust
#[derive(Message)]
pub struct ActivatePane { pub profile: ProfileId, pub stack: Entity }
```

- Local human input (pane click, keyboard nav, tab select, command bar) emits `ActivatePane { Local, .. }`.
- Agent actions (navigate/click/scroll/open that target a pane) emit `ActivatePane { Agent(sid), .. }`.
- A remote participant (future) emits the same message with its `ProfileId`.

One system (`apply_active_panes`) consumes `ActivatePane` and updates `ActivePanes[profile]` (resolving tab/pane from the stack). Profiles whose target stack/pane despawns are pruned.

## Migration from `FocusedStack`

`FocusedStack` stays as a **derived mirror of `ActivePanes[ProfileId::Local]`** so the many existing local-only consumers (OS keyboard focus, command bar, clipboard target) keep working unchanged.

- Today `compute_focused_stack` derives focus from the global `LastActivatedAt` timeline. Change: `LastActivatedAt::now()` is stamped **only by local-human actions**. Agent open/spawn stops stamping it (they instead set their own `ActivePanes[Agent]`). So `FocusedStack` (computed from `LastActivatedAt`) becomes purely the local human's active pane, and `ActivePanes[Local]` mirrors it.
- Consumer audit (each `FocusedStack` reader): OS keyboard focus (`host_focus`), command bar, clipboard → stay on `FocusedStack` (= local human). Focus-ring rendering → becomes per-profile (reads `ActivePanes`). Agent target resolution → reads `ActivePanes[acting agent]` via anchor.

## Agent targeting (fixes the hijack)

- Browser tools (`browser_navigate`/`snapshot`/`scroll`, and future `click`/`type`) carry the agent **anchor** (the mechanism `open_page`/`run`/`read_layout` already use). Handlers resolve `anchor → agent → ActivePanes[Agent]`, defaulting to the agent's own pane (`resolve_self_pane`) or the browser pane it opened. They never read `FocusedStack`.
- When an agent navigates/opens a browser pane, it sets `ActivePanes[Agent]` to that pane (emits `ActivatePane{Agent,..}`) **without** stamping `LastActivatedAt` — so the human's ring is untouched.

## Focus rings (per profile)

- Render one ring per profile that currently has a **visible** active pane (typically two — local human + the acting agent; scales to N).
- Ring color = the profile's identity color (the facepile/team accent already associated with the human and each agent).
- **Browse mode** (macOS default, windowed CEF): the native windowed focus ring is a CALayer border per webview; extend `set_windowed_focus_ring` to take a color and to allow multiple panes ringed at once (one per profile-active webview).
- **OSR / Player mode**: the mesh ring becomes one instance per active profile-pane, colored per profile.
- Edge: a pane active for more than one profile → draw the local human's ring on top (composite/offset decided at implementation).

## OS focus

Only `ActivePanes[Local]` (= `FocusedStack`) drives OS keyboard first-responder and the clipboard target. Agent input (`send_mouse_click`/`send_key`) must not steal it — restore the local human's `set_focus`/`CefKeyboardTarget` after agent input, or target without `set_focus` where the host allows.

## Persistence

`ActivePanes` is ephemeral runtime state (like `FocusedStack` today) — not saved to `.ron`.

## Testing

- **Unit:** `ActivatePane` → `ActivePanes` per profile; isolation (agent activation never mutates `ActivePanes[Local]`; local activation never mutates an agent's entry); pruning when a target despawns.
- **Integration (Bevy):** agent navigate sets `ActivePanes[Agent]` to its pane while `FocusedStack`/`ActivePanes[Local]` stays put; ring system emits one ring per visible profile-pane with the right color; anchor→agent-pane targeting resolves without reading `FocusedStack`.
- **Identity/color:** ring color resolves from the profile identity for both a human and an agent.

## Open items (verify during implementation)

- Exact profile-identity + per-profile color source in `vmux_team` (facepile accent).
- `AgentSession`/anchor → `ProfileId::Agent(key)` mapping.
- Windowed ring: multi-instance + color plumbing through `set_windowed_focus_ring`.

## Scope / sequencing

This is a foundational focus-model refactor that **subsumes** the "agent targets its own pane" fix for #180's browser tools.

- **#180** (research tools: disable web tools, navigate-returns-snapshot, viewport, scroll, steer) is green + CodeRabbit-approved — merge as the foundation (after the human's runtime test).
- **This refactor** (per-profile active panes + multi-rings + agent-targeting) = its own PR on top.
- **Click/type** (`docs/specs/2026-06-23-agent-browser-control-design.md`) stacks on this refactor, since interaction acts on the agent's own active pane.
