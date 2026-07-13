# Resume Selector Agent Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each `/resume` result with the active ACP agent's live, human-readable identity without hardcoded provider-name mappings.

**Architecture:** Capture optional ACP `InitializeResponse.agent_info` in `vmux_service`, forward the resolved display name through the service-to-Bevy bridge, and update the ACP pane's `Profile`. The resume collector copies that profile name into each row; registry/config/id fallbacks cover startup and agents that omit metadata.

**Tech Stack:** Rust, Bevy ECS messages/components, ACP schema v1, rkyv IPC, Dioxus/WASM, Tailwind classes.

---

## File structure

- `crates/vmux_service/src/acp/driver.rs`: resolve and publish ACP implementation identity.
- `crates/vmux_service/src/protocol.rs`: carry identity over service IPC.
- `crates/vmux_service/src/agent_events.rs`: expose identity as a Bevy message.
- `crates/vmux_terminal/src/plugin.rs`: bridge service identity into ECS.
- `crates/vmux_agent/src/client/acp.rs`: apply live identity to the matching ACP profile.
- `crates/vmux_agent/src/plugin.rs`: resolve registry/config/id fallback names.
- `crates/vmux_agent/src/chat_page/event.rs`: serialize `agent_name` with resume rows.
- `crates/vmux_agent/src/chat_page.rs`: source labels from the active profile and refresh snapshots.
- `crates/vmux_agent/src/chat_page/page.rs`: render the row label.

### Task 1: Publish ACP-reported identity from the service

**Files:**
- Modify: `crates/vmux_service/src/acp/driver.rs`
- Modify: `crates/vmux_service/src/protocol.rs`
- Modify: `crates/vmux_service/src/agent_events.rs`

- [ ] **Step 1: Write failing name-resolution and protocol tests**

Add to `crates/vmux_service/src/acp/driver.rs` tests:

```rust
use agent_client_protocol::schema::v1::Implementation;

#[test]
fn acp_display_name_prefers_title_then_name() {
    let titled = Implementation::new("antigravity", "1.0").title("Antigravity");
    assert_eq!(acp_display_name(Some(&titled)).as_deref(), Some("Antigravity"));

    let named = Implementation::new("claude-code-acp", "1.0");
    assert_eq!(
        acp_display_name(Some(&named)).as_deref(),
        Some("claude-code-acp")
    );
}

#[test]
fn acp_display_name_ignores_blank_metadata() {
    let blank_title = Implementation::new("codex-acp", "1.0").title("   ");
    assert_eq!(
        acp_display_name(Some(&blank_title)).as_deref(),
        Some("codex-acp")
    );
    let blank = Implementation::new("   ", "1.0");
    assert_eq!(acp_display_name(Some(&blank)), None);
    assert_eq!(acp_display_name(None), None);
}
```

Add to the `services` array in `acp_protocol_messages_roundtrip`:

```rust
ServiceMessage::AcpAgentInfo {
    sid: "s".into(),
    name: "Antigravity".into(),
},
```

- [ ] **Step 2: Run tests and verify failure**

```bash
cargo test -p vmux_service acp_display_name
cargo test -p vmux_service acp_protocol_messages_roundtrip
```

Expected: compile failures because `acp_display_name` and `ServiceMessage::AcpAgentInfo` do not exist.

- [ ] **Step 3: Add identity message types and resolution**

Add to `ServiceMessage`:

```rust
/// Identity reported by an ACP agent during initialization.
AcpAgentInfo {
    sid: String,
    name: String,
},
```

Add to `crates/vmux_service/src/agent_events.rs`:

```rust
/// Human-readable identity reported by a running ACP agent.
#[derive(Message)]
pub struct PageAgentInfo {
    pub sid: String,
    pub name: String,
}
```

Import `Implementation` in `driver.rs`, then add:

```rust
fn acp_display_name(info: Option<&Implementation>) -> Option<String> {
    let info = info?;
    info.title
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            let name = info.name.trim();
            (!name.is_empty()).then_some(name)
        })
        .map(str::to_string)
}
```

Immediately after initialization succeeds:

```rust
if let Some(name) = acp_display_name(init_resp.agent_info.as_ref()) {
    main_shared.emit(ServiceMessage::AcpAgentInfo {
        sid: main_shared.sid.clone(),
        name,
    });
}
```

- [ ] **Step 4: Run tests and verify pass**

```bash
cargo test -p vmux_service acp_display_name
cargo test -p vmux_service acp_protocol_messages_roundtrip
```

Expected: all matching tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/acp/driver.rs crates/vmux_service/src/protocol.rs crates/vmux_service/src/agent_events.rs
git commit -m "feat(service): publish ACP agent identity"
```

### Task 2: Bridge live identity into ACP profiles

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`
- Modify: `crates/vmux_agent/src/client/acp.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`

- [ ] **Step 1: Write failing ECS and bridge tests**

Add to `crates/vmux_agent/src/client/acp.rs` tests:

```rust
#[test]
fn live_acp_identity_updates_only_matching_profile() {
    use vmux_core::team::Profile;
    use vmux_service::agent_events::PageAgentInfo;

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_plugins(AcpAgentPlugin);
    let matching = app.world_mut().spawn((
        AcpSession {
            agent_id: "antigravity".into(),
            sid: "s1".into(),
            cwd: "/tmp".into(),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        },
        Profile::registry("Configured", "antigravity"),
    )).id();
    let unrelated = app.world_mut().spawn((
        AcpSession {
            agent_id: "claude".into(),
            sid: "s2".into(),
            cwd: "/tmp".into(),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        },
        Profile::registry("Claude", "claude"),
    )).id();

    app.world_mut().write_message(PageAgentInfo {
        sid: "s1".into(),
        name: "Antigravity".into(),
    });
    app.update();

    assert_eq!(app.world().get::<Profile>(matching).unwrap().name, "Antigravity");
    assert_eq!(app.world().get::<Profile>(unrelated).unwrap().name, "Claude");
}
```

Add to `crates/vmux_terminal/src/plugin.rs` tests:

```rust
#[test]
fn service_bridge_routes_acp_agent_info() {
    let source = include_str!("plugin.rs");
    assert!(source.contains("ServiceMessage::AcpAgentInfo"));
    assert!(source.contains("PageAgentInfo"));
}
```

- [ ] **Step 2: Run tests and verify failure**

```bash
cargo test -p vmux_agent live_acp_identity_updates_only_matching_profile
cargo test -p vmux_terminal service_bridge_routes_acp_agent_info
```

Expected: failures because the message is not registered, bridged, or applied.

- [ ] **Step 3: Route the service message**

Add to `PollServiceWriters`:

```rust
page_agent_info: MessageWriter<'w, vmux_service::agent_events::PageAgentInfo>,
```

Add to the service-message match:

```rust
ServiceMessage::AcpAgentInfo { sid, name } => {
    writers
        .page_agent_info
        .write(vmux_service::agent_events::PageAgentInfo { sid, name });
}
```

- [ ] **Step 4: Register and apply identity**

Register `PageAgentInfo` beside `PageAgentSessionCreated`, add `apply_acp_agent_info` to the ACP plugin Update systems, and implement:

```rust
fn apply_acp_agent_info(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentInfo>,
    mut sessions: Query<(&AcpSession, &mut vmux_core::team::Profile)>,
) {
    for event in reader.read() {
        let name = event.name.trim();
        if name.is_empty() {
            continue;
        }
        for (session, mut profile) in &mut sessions {
            if session.sid == event.sid && profile.name != name {
                *profile = vmux_core::team::Profile::registry(name, &session.agent_id);
            }
        }
    }
}
```

Add `Changed<Profile>` to the `Or` filter in `push_chat_to_page`:

```rust
Or<(
    Changed<AgentMessages>,
    Changed<AgentRunState>,
    Changed<PromptQueue>,
    Changed<Profile>,
)>,
```

- [ ] **Step 5: Run tests and verify pass**

```bash
cargo test -p vmux_agent live_acp_identity_updates_only_matching_profile
cargo test -p vmux_terminal service_bridge_routes_acp_agent_info
```

Expected: both tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs crates/vmux_agent/src/client/acp.rs crates/vmux_agent/src/chat_page.rs
git commit -m "feat(agent): apply live ACP identity"
```

### Task 3: Add fallbacks and render identity in `/resume`

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`
- Modify: `crates/vmux_agent/src/chat_page/event.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`
- Modify: `crates/vmux_agent/src/chat_page/page.rs`

- [ ] **Step 1: Write failing fallback and selector tests**

Add to `plugin.rs` tests:

```rust
#[test]
fn acp_profile_name_prefers_registry_then_config_then_id() {
    use crate::acp_registry::{Distribution, RegistryAgent};
    use vmux_setting::AcpAgentConfig;

    let mut cfg = AcpAgentConfig {
        id: "claude".into(),
        name: "Configured Claude".into(),
        command: "npx".into(),
        args: vec![],
        env: vec![],
        cwd: None,
    };
    let catalog = crate::client::acp::AcpCatalog {
        agents: vec![RegistryAgent {
            id: "claude-acp".into(),
            name: "Claude".into(),
            version: None,
            description: None,
            icon: None,
            repository: None,
            distribution: Distribution::default(),
        }],
    };

    assert_eq!(acp_profile_name(&cfg, Some(&catalog)), "Claude");
    assert_eq!(acp_profile_name(&cfg, None), "Configured Claude");
    cfg.name = "   ".into();
    assert_eq!(acp_profile_name(&cfg, None), "claude");
}
```

Extend `resumable_sessions_rkyv_roundtrip` with:

```rust
agent_name: "Claude".into(),
```

and:

```rust
assert_eq!(back.sessions[0].agent_name, "Claude");
```

Add to `chat_page.rs` native tests:

```rust
#[test]
fn resume_agent_name_prefers_profile_then_kind_then_id() {
    let profile = Profile::registry("Antigravity", "antigravity");
    assert_eq!(
        resume_agent_name(Some(&profile), Some(AgentKind::Claude), Some("claude")),
        "Antigravity"
    );
    assert_eq!(
        resume_agent_name(None, Some(AgentKind::Claude), Some("claude")),
        "Claude"
    );
    assert_eq!(resume_agent_name(None, None, Some("custom-acp")), "custom-acp");
}

#[test]
fn composer_resume_rows_render_agent_name() {
    let source = include_str!("chat_page/page.rs");
    assert!(source.contains("session.agent_name"));
    assert!(source.contains("shrink-0 text-xs text-muted-foreground"));
}
```

- [ ] **Step 2: Run tests and verify failure**

```bash
cargo test -p vmux_agent acp_profile_name_prefers_registry_then_config_then_id
cargo test -p vmux_agent resumable_sessions_rkyv_roundtrip
cargo test -p vmux_agent resume_agent_name_prefers_profile_then_kind_then_id
cargo test -p vmux_agent composer_resume_rows_render_agent_name
```

Expected: failures because fallback helpers, the serialized field, and UI label do not exist.

- [ ] **Step 3: Resolve registry/config/id fallback names**

Add near `acp_icon_for_id`:

```rust
fn acp_profile_name(
    config: &vmux_setting::AcpAgentConfig,
    catalog: Option<&crate::client::acp::AcpCatalog>,
) -> String {
    let registry_id = crate::acp_install::registry_id_alias(&config.id);
    catalog
        .and_then(|catalog| catalog.agents.iter().find(|agent| agent.id == registry_id))
        .map(|agent| agent.name.trim())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            let name = config.name.trim();
            (!name.is_empty()).then_some(name)
        })
        .unwrap_or(config.id.as_str())
        .to_string()
}
```

At both ACP attach call sites, calculate the fallback and pass `&name` instead of `&cfg.name`:

```rust
let name = acp_profile_name(cfg, catalog.as_deref());
```

Use `catalog` directly in the page-open helper where it is already `Option<&AcpCatalog>`.

- [ ] **Step 4: Serialize the active profile name**

Add to `ResumableSessionEntry`:

```rust
/// Human-readable active ACP agent name.
pub agent_name: String,
```

Add in `chat_page.rs`:

```rust
fn resume_agent_name(
    profile: Option<&Profile>,
    kind: Option<AgentKind>,
    acp_id: Option<&str>,
) -> String {
    profile
        .map(|profile| profile.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| kind.map(|kind| kind.display_name().to_string()))
        .or_else(|| acp_id.map(str::to_string))
        .unwrap_or_default()
}
```

Update `on_resume_list_request` to query `Profile`, retain the current ACP id, compute `agent_name` before spawning the IO task, and populate each row:

```rust
agent_name: agent_name.clone(),
```

Keep current-kind filtering unchanged; this does not add new session backends.

- [ ] **Step 5: Render the right-aligned label**

Replace the standalone title span with:

```rust
div { class: "flex min-w-0 items-baseline gap-2",
    span { class: "min-w-0 flex-1 truncate text-sm text-foreground", "{session.title}" }
    if !session.agent_name.is_empty() {
        span { class: "shrink-0 text-xs text-muted-foreground", "{session.agent_name}" }
    }
}
```

Leave subtitle, row ids, selection, click handling, and scroll behavior unchanged.

- [ ] **Step 6: Run tests and verify pass**

```bash
cargo test -p vmux_agent acp_profile_name_prefers_registry_then_config_then_id
cargo test -p vmux_agent resumable_sessions_rkyv_roundtrip
cargo test -p vmux_agent resume_agent_name_prefers_profile_then_kind_then_id
cargo test -p vmux_agent composer_resume_rows_render_agent_name
```

Expected: all matching tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs crates/vmux_agent/src/chat_page/event.rs crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/page.rs
git commit -m "feat(agent): label resume sessions by ACP agent"
```

### Task 4: Verify, remove the plan, and update PR #241

**Files:**
- Delete: `docs/plans/2026-07-13-resume-agent-identity.md`

- [ ] **Step 1: Run targeted native tests**

```bash
cargo test -p vmux_service acp_display_name
cargo test -p vmux_service acp_protocol_messages_roundtrip
cargo test -p vmux_terminal service_bridge_routes_acp_agent_info
cargo test -p vmux_agent live_acp_identity_updates_only_matching_profile
cargo test -p vmux_agent acp_profile_name_prefers_registry_then_config_then_id
cargo test -p vmux_agent resumable_sessions_rkyv_roundtrip
cargo test -p vmux_agent resume_agent_name_prefers_profile_then_kind_then_id
cargo test -p vmux_agent composer_resume_rows_render_agent_name
```

Expected: all commands pass.

- [ ] **Step 2: Compile the chat page for WASM**

```bash
cargo check -p vmux_agent --target wasm32-unknown-unknown
```

Expected: check succeeds.

- [ ] **Step 3: Check formatting and diff cleanliness**

```bash
cargo fmt --all -- --check
git diff --check
git status --short
```

Expected: checks succeed; only the completed implementation plan remains for deletion.

- [ ] **Step 4: Delete the completed plan with `apply_patch`**

Delete `docs/plans/2026-07-13-resume-agent-identity.md`.

- [ ] **Step 5: Commit plan removal**

```bash
git add docs/plans/2026-07-13-resume-agent-identity.md
git commit -m "docs(agent): remove completed identity plan"
```

- [ ] **Step 6: Push the feature branch**

```bash
git push origin feat/acp-cli-resume
```

Expected: PR #241 points to the pushed head commit.
