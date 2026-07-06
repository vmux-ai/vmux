# ACP ↔ CLI Agent Parity — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give ACP agents the same identity/UX treatment as CLI agents, starting with the foundational identity model so ACP agents appear in the team roster and tab strip with their registry avatar.

**Architecture:** ACP sessions currently omit the `team::Profile` + `team::Agent` identity that CLI/Page agents carry, and have no `AgentKind`. This plan attaches a registry-sourced `Profile` + a kind-agnostic `Agent` to ACP sessions and sets `PageMetadata.icon` from the registry icon. The team-roster and tab-strip DOM already render `PageMetadata`-derived favicons via `favicon_src_for_url`, so those surfaces light up with no page/wasm change once the native identity is attached.

**Tech Stack:** Rust, Bevy ECS, `vmux_core` (wasm-compiled shared types), `vmux_agent`, `vmux_team`.

**Scope of THIS plan:** Design-spec Phase 1 (§1 identity + the DOM avatar surfaces A1 roster & A4 tab icon). Phases 2–6 (native focus-ring badge, chat-header avatar, notifications, command-bar/loading polish, session resume) are summarized in "Remaining phases" and get their own plans/PRs.

**Spec:** `docs/specs/2026-07-03-acp-cli-parity-design.md`

---

## File structure

- `crates/vmux_core/src/team.rs` — identity model. Add registry-sourced avatar constructors + a wasm-safe color hash; make `Agent.kind` optional (kind-agnostic session marker).
- `crates/vmux_team/src/plugin.rs` — roster builder. Tolerate an absent `AgentKind` when deriving the favicon-fallback url.
- `crates/vmux_agent/src/plugin.rs` — `attach_acp_agent_to_stack` attaches `Profile`+`Agent`+`PageMetadata.icon`; the page-open handler threads the registry `AcpCatalog` icon into it. Update the three CLI/Page `Agent{}` construction sites for the optional kind.

No new crates. No DOM/wasm changes in this plan.

---

## Task 1: Registry avatar constructors + color hash (`vmux_core::team`)

**Files:**
- Modify: `crates/vmux_core/src/team.rs`
- Test: `crates/vmux_core/src/team.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/vmux_core/src/team.rs`:

```rust
    #[test]
    fn registry_avatar_derives_initials_and_stable_color() {
        let a = AvatarSpec::for_registry("Mistral Vibe", "mistral-vibe");
        assert_eq!(a.initials, "MV");
        // Deterministic: same seed -> same color.
        assert_eq!(a.color, AvatarSpec::for_registry("X", "mistral-vibe").color);
        // Valid 7-char hex.
        assert!(a.color.starts_with('#') && a.color.len() == 7);
    }

    #[test]
    fn registry_color_differs_by_seed() {
        assert_ne!(
            AvatarSpec::for_registry("A", "claude-acp").color,
            AvatarSpec::for_registry("A", "mistral-vibe").color
        );
    }

    #[test]
    fn registry_profile_uses_name() {
        assert_eq!(Profile::registry("Claude Agent", "claude-acp").name, "Claude Agent");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_core team:: 2>&1 | tail -20`
Expected: FAIL — `no function or associated item named 'for_registry'` / `'registry'`.

- [ ] **Step 3: Implement the constructors + hash**

In `crates/vmux_core/src/team.rs`, add to `impl AvatarSpec` (after `for_agent`):

```rust
    /// Avatar for a registry-driven ACP agent: initials from the display name, a stable
    /// brand color hashed from the registry id (so each agent reads distinctly).
    pub fn for_registry(name: &str, seed: &str) -> Self {
        Self {
            initials: initials_of(name),
            color: hash_color(seed),
        }
    }
```

Add to `impl Profile` (after `agent`):

```rust
    pub fn registry(name: &str, seed: &str) -> Self {
        Self {
            name: name.to_string(),
            avatar: AvatarSpec::for_registry(name, seed),
        }
    }
```

Add a free function near `initials_of` (wasm-safe — no rng, deterministic FNV-1a into a fixed palette):

```rust
/// A stable brand color for a seed string (e.g. an ACP registry id), picked from a fixed
/// palette by an FNV-1a hash. Deterministic and wasm-safe.
pub fn hash_color(seed: &str) -> String {
    const PALETTE: [&str; 8] = [
        "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899",
    ];
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in seed.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    PALETTE[(hash % PALETTE.len() as u64) as usize].to_string()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_core team:: 2>&1 | tail -20`
Expected: PASS (all `team::` tests green).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/team.rs
git commit -m "feat(team): registry-sourced avatar (initials + hashed brand color)"
```

---

## Task 2: Make `team::Agent.kind` optional (kind-agnostic session marker)

CLI/Page agents keep a concrete `AgentKind` (drives the roster favicon-fallback url); ACP has none. Make the field `Option<AgentKind>` and update every construction/read site.

**Files:**
- Modify: `crates/vmux_core/src/team.rs:24` (struct)
- Modify: `crates/vmux_team/src/plugin.rs:173` (read), `:496` (test)
- Modify: `crates/vmux_agent/src/plugin.rs:331`, `:2573`, `:2887` (constructions)

- [ ] **Step 1: Change the struct field**

`crates/vmux_core/src/team.rs`, the `Agent` struct:

```rust
#[derive(Component, Clone, Debug)]
pub struct Agent {
    pub sid: String,
    pub kind: Option<AgentKind>,
}
```

- [ ] **Step 2: Fix the roster read**

`crates/vmux_team/src/plugin.rs:173`, replace:

```rust
                let url = agent.kind.cli_url_prefix();
```
with:
```rust
                let url = agent.kind.map(|k| k.cli_url_prefix()).unwrap_or_default();
```

- [ ] **Step 3: Fix the three production construction sites**

`crates/vmux_agent/src/plugin.rs:331-334` (in `attach_page_agent_to_stack`), replace:
```rust
        vmux_core::team::Agent {
            sid: sid.to_string(),
            kind,
        },
```
with:
```rust
        vmux_core::team::Agent {
            sid: sid.to_string(),
            kind: Some(kind),
        },
```

`crates/vmux_agent/src/plugin.rs:2573-2576` (CLI spawn), replace:
```rust
                    vmux_core::team::Agent {
                        sid: req.session_id.clone().unwrap_or_default(),
                        kind: req.kind,
                    },
```
with:
```rust
                    vmux_core::team::Agent {
                        sid: req.session_id.clone().unwrap_or_default(),
                        kind: Some(req.kind),
                    },
```

- [ ] **Step 4: Fix the two test construction sites**

`crates/vmux_team/src/plugin.rs:496-500`, change `kind: AgentKind::Claude,` to `kind: Some(AgentKind::Claude),`.

`crates/vmux_agent/src/plugin.rs:2887-2890`, change `kind: vmux_core::agent::AgentKind::Claude,` to `kind: Some(vmux_core::agent::AgentKind::Claude),`.

- [ ] **Step 5: Verify the workspace compiles + existing tests pass**

Run: `cargo test -p vmux_core -p vmux_team -p vmux_agent 2>&1 | tail -25`
Expected: PASS. If a compile error names another `team::Agent { ... kind` site, wrap that `kind` in `Some(...)` the same way (the grep for construction sites found only these four; a new one would be a merge from main).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_core/src/team.rs crates/vmux_team/src/plugin.rs crates/vmux_agent/src/plugin.rs
git commit -m "refactor(team): make Agent.kind optional for kind-agnostic (ACP) sessions"
```

---

## Task 3: Attach registry identity + icon in `attach_acp_agent_to_stack`

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs:344-379` (`attach_acp_agent_to_stack`)
- Test: `crates/vmux_agent/src/plugin.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_agent/src/plugin.rs`:

```rust
    #[test]
    fn acp_attach_gives_profile_agent_and_icon() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        let stack = app.world_mut().spawn_empty().id();

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut mt: ResMut<Assets<WebviewExtendStandardMaterial>>| {
                    attach_acp_agent_to_stack(
                        stack,
                        "mistral-vibe",
                        "Mistral Vibe",
                        "sid-1",
                        std::path::Path::new("/tmp"),
                        Some("https://cdn.example/vibe.svg"),
                        &mut commands,
                        &mut meshes,
                        &mut mt,
                    );
                },
            )
            .unwrap();

        let world = app.world();
        let profile = world.get::<vmux_core::team::Profile>(stack).expect("profile");
        assert_eq!(profile.name, "Mistral Vibe");
        let agent = world.get::<vmux_core::team::Agent>(stack).expect("agent");
        assert_eq!(agent.sid, "sid-1");
        assert_eq!(agent.kind, None);
        let meta = world.get::<PageMetadata>(stack).expect("meta");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent acp_attach_gives_profile_agent_and_icon 2>&1 | tail -20`
Expected: FAIL — arity mismatch (`attach_acp_agent_to_stack` takes 8 args, not 9) / missing `Profile`.

- [ ] **Step 3: Add the `icon` param and attach identity**

`crates/vmux_agent/src/plugin.rs`, change the signature of `attach_acp_agent_to_stack` to add `icon: Option<&str>` after `cwd`:

```rust
pub fn attach_acp_agent_to_stack(
    stack: Entity,
    agent_id: &str,
    name: &str,
    sid: &str,
    cwd: &std::path::Path,
    icon: Option<&str>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
```

Set the icon on `PageMetadata` (the `commands.entity(stack).insert(PageMetadata { ... })` block):

```rust
    commands.entity(stack).insert(PageMetadata {
        url: format!("vmux://agent/{agent_id}"),
        title: name.to_string(),
        bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
        icon: vmux_core::PageIcon::favicon(icon.unwrap_or("")),
        ..default()
    });
```

Add `Profile` + `Agent` to the identity insert block (alongside `AcpSession`, `AgentMessages`, …):

```rust
    commands.entity(stack).insert((
        crate::client::acp::AcpSession {
            agent_id: agent_id.to_string(),
            sid: sid.to_string(),
            cwd: cwd.to_path_buf(),
            anchor,
        },
        crate::AgentMessages::default(),
        crate::AgentApprovalPolicy::default(),
        crate::AgentRunState::default(),
        vmux_core::team::Profile::registry(name, agent_id),
        vmux_core::team::Agent {
            sid: sid.to_string(),
            kind: None,
        },
    ));
```

(If `vmux_core::PageIcon` does not resolve, use `vmux_core::icon::PageIcon`.)

- [ ] **Step 4: Update the existing caller**

`crates/vmux_agent/src/plugin.rs` in `handle_agent_page_open_task` (ACP branch, ~line 2346), add the `icon` argument. For now pass `None` (Task 4 wires the real registry icon):

```rust
                attach_acp_agent_to_stack(
                    task.stack,
                    &cfg.id,
                    &cfg.name,
                    &sid,
                    default_cwd,
                    None,
                    commands,
                    meshes,
                    webview_mt,
                );
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_agent acp_attach_gives_profile_agent_and_icon 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(acp): attach registry Profile+Agent+favicon to ACP sessions"
```

---

## Task 4: Thread the registry icon (`AcpCatalog`) into the page-open handler

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (`handle_agent_page_open` :2186, `handle_agent_page_open_task` :2236, ACP branch :2346)
- Test: `crates/vmux_agent/src/plugin.rs` (inline)

- [ ] **Step 1: Write the failing test for the icon lookup helper**

Add to the `tests` module in `crates/vmux_agent/src/plugin.rs`:

```rust
    #[test]
    fn acp_icon_for_id_reads_catalog() {
        use crate::acp_registry::{Distribution, RegistryAgent};
        let catalog = crate::client::acp::AcpCatalog {
            agents: vec![RegistryAgent {
                id: "mistral-vibe".to_string(),
                name: "Mistral Vibe".to_string(),
                version: None,
                description: None,
                icon: Some("https://cdn.example/vibe.svg".to_string()),
                repository: None,
                distribution: Distribution::default(),
            }],
        };
        assert_eq!(
            acp_icon_for_id(Some(&catalog), "mistral-vibe").as_deref(),
            Some("https://cdn.example/vibe.svg")
        );
        assert_eq!(acp_icon_for_id(Some(&catalog), "absent"), None);
        assert_eq!(acp_icon_for_id(None, "mistral-vibe"), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent acp_icon_for_id_reads_catalog 2>&1 | tail -20`
Expected: FAIL — `cannot find function 'acp_icon_for_id'`.

- [ ] **Step 3: Add the lookup helper**

`crates/vmux_agent/src/plugin.rs`, near `attach_acp_agent_to_stack`:

```rust
/// The registry icon URL for an ACP agent id, if the catalog is loaded and lists it.
fn acp_icon_for_id(catalog: Option<&crate::client::acp::AcpCatalog>, id: &str) -> Option<String> {
    catalog?
        .agents
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.icon.clone())
}
```

- [ ] **Step 4: Thread `AcpCatalog` through the handler and pass the icon**

In `handle_agent_page_open` (:2186) add a system param:
```rust
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
```
and pass it to the task fn (add as the final argument of the `handle_agent_page_open_task(...)` call):
```rust
            catalog.as_deref(),
```

In `handle_agent_page_open_task` (:2236) add the matching final parameter:
```rust
    catalog: Option<&crate::client::acp::AcpCatalog>,
```

In the ACP branch, replace the `None,` icon argument from Task 3 Step 4 with the looked-up icon:
```rust
                let icon = acp_icon_for_id(catalog, &cfg.id);
                attach_acp_agent_to_stack(
                    task.stack,
                    &cfg.id,
                    &cfg.name,
                    &sid,
                    default_cwd,
                    icon.as_deref(),
                    commands,
                    meshes,
                    webview_mt,
                );
```

- [ ] **Step 5: Run the test + package build**

Run: `cargo test -p vmux_agent acp_icon_for_id_reads_catalog 2>&1 | tail -20`
Expected: PASS.
Run: `cargo build -p vmux_agent 2>&1 | tail -15`
Expected: builds (handler wiring compiles).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(acp): source ACP tab/roster favicon from the registry catalog"
```

---

## Task 5: End-to-end roster inclusion test (`vmux_team`)

Prove A1: an ACP-shaped entity (Profile + Agent{kind:None} + PageMetadata favicon) in the active space produces a team row whose `icon` is the registry favicon and whose `url` is empty (no `AgentKind` fallback).

**Files:**
- Test: `crates/vmux_team/src/plugin.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_team/src/plugin.rs`:

```rust
    #[test]
    fn acp_agent_appears_in_roster_with_registry_icon() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        let space = app.world_mut().spawn(Space).id();
        app.insert_resource(ActiveSpaceEntity(Some(space)));
        app.world_mut().spawn((Profile::user(), User));
        app.world_mut().spawn((
            Profile::registry("Mistral Vibe", "mistral-vibe"),
            Agent { sid: "sid-1".to_string(), kind: None },
            PageMetadata {
                url: "vmux://agent/mistral-vibe".to_string(),
                icon: vmux_core::PageIcon::favicon("https://cdn.example/vibe.svg"),
                ..default()
            },
            ChildOf(space),
        ));

        let rows = app
            .world_mut()
            .run_system_once(
                |active: Res<ActiveSpaceEntity>,
                 user_q: Query<(Entity, &Profile), With<User>>,
                 agent_q: Query<(
                    Entity,
                    &Profile,
                    &Agent,
                    Option<&AgentRunState>,
                    Option<&SessionId>,
                    Option<&vmux_core::notify::AgentDoneUnseen>,
                 )>,
                 child_of: Query<&ChildOf>,
                 space_marker: Query<(), With<Space>>,
                 meta_q: Query<&PageMetadata>,
                 children_q: Query<&Children>| {
                    build_team_members(
                        &active, &user_q, &agent_q, &child_of, &space_marker, &meta_q, &children_q,
                    )
                },
            )
            .unwrap();

        let agent = rows.iter().find(|r| !r.is_user).expect("acp agent in roster");
        assert_eq!(agent.name, "Mistral Vibe");
        assert_eq!(agent.icon, "https://cdn.example/vibe.svg");
        assert_eq!(agent.url, "");
    }
```

- [ ] **Step 2: Run test to verify it fails (or passes)**

Run: `cargo test -p vmux_team acp_agent_appears_in_roster_with_registry_icon 2>&1 | tail -25`
Expected: PASS after Tasks 1–2 (the production code already supports this once identity is attached). This test is the regression guard for A1. If it FAILS on a missing import, add `use vmux_core::PageMetadata;` / `use vmux_agent::AgentRunState;` to the test module as needed (mirror the existing imports at the top of `plugin.rs`).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_team/src/plugin.rs
git commit -m "test(team): ACP agent appears in roster with registry favicon"
```

---

## Task 6: Phase 1 verification

- [ ] **Step 1: Run the affected package tests**

Run: `cargo test -p vmux_core -p vmux_team -p vmux_agent 2>&1 | tail -30`
Expected: all green.

- [ ] **Step 2: Typecheck the wasm pages (team.rs is wasm-compiled)**

Run: `cargo check -p vmux_core --target wasm32-unknown-unknown 2>&1 | tail -15`
Expected: builds — `AvatarSpec`/`hash_color` stay `Component`-free and wasm-safe.

- [ ] **Step 3: Format + clippy on the touched crates**

Run: `cargo fmt -p vmux_core -p vmux_team -p vmux_agent && git checkout -- patches/ 2>/dev/null; cargo clippy -p vmux_core -p vmux_team -p vmux_agent 2>&1 | tail -20`
Expected: no warnings. (Note: `cargo fmt` may reformat vendored `patches/` — revert those, commit only `crates/` changes.)

- [ ] **Step 4: Commit any fmt changes**

```bash
git add crates/
git commit -m "style: fmt acp parity phase 1" || echo "nothing to format"
```

Manual runtime check (deferred, one pass): open an ACP agent from the launcher → its tab shows the registry icon (not the Sparkles default); open the Team side-sheet → the ACP agent appears with its registry avatar.

---

## Remaining phases (own plans/PRs)

Each maps to a design-spec section and ships independently:

- **Phase 2 — Native focus-ring badge + chat header avatar (§2.2, §2.4, §3).** Add `AvatarSpec.icon: Option<AvatarIcon{Kind|Remote}>`; render the registry icon on the macOS `CALayer` badge (fetch + cache + rasterize SVG via `usvg`/`resvg`, cache `~/.vmux/agents/icons/<id>.png`, fall back to initials+color on failure); replace `ActiveStack.kind` with `agent: Option<Entity>` so `windowed_ring_for` resolves the badge from `Profile`; add `agent_name`/`agent_icon`/`accent_color` to `ChatSnapshot` and render the header/hero avatar (fixes Page too).
- **Phase 3 — Notifications (§4).** In `consume_page_agent_stream`, emit `AgentAttention` on Streaming→Idle (gated `!agent_is_viewed`, deduped) → done-dot + OS notify for ACP and Page; generalize `AgentSpaceWriters` caller resolution + `surface_errors` to recognize `AcpSession`.
- **Phase 4 — Command bar + polish (§5).** `AcpSession.cwd` into `update_work_dirs_snapshot`; branded install/loading state; wire the chat status dot to `AgentRunState`.
- **Phase 5 — Session resume (§6, gated last).** `AcpSession`/`AgentMessages` `Reflect`+persisted; reconnect via ACP `loadSession` when the agent advertises it, else fresh-session fallback.

---

## Self-review notes

- **Spec deviation (§1):** the spec says "drop `team::Agent.kind`"; this plan makes it `Option<AgentKind>` instead — smaller migration, and CLI keeps its favicon-fallback url (`agent.kind.map(cli_url_prefix)`), which is load-bearing for CLI roster avatars. Spec §1 updated to match.
- **Spec deviation (§2 avatar field):** `AvatarSpec.icon` is deferred to Phase 2 — the roster/tab DOM render from `PageMetadata`-derived favicons (`TeamMemberRow.icon` + `favicon_src_for_url`), independent of `AvatarSpec`, so Phase 1 needs no `AvatarIcon` enum and no DOM change.
- **Type consistency:** `Agent { sid, kind: Option<AgentKind> }` used identically in Tasks 2/3/5; `attach_acp_agent_to_stack(icon: Option<&str>)` and `acp_icon_for_id(...) -> Option<String>` consistent across Tasks 3/4.
