//! Agent compose page: a normal (non-terminal) webview shown when opening an
//! agent. The user types a prompt while a Matrix-rain backdrop plays; on Enter
//! the agent CLI is spawned and the prompt is delivered into it. Because this is
//! not a terminal, CEF keyboard input is not suppressed, so the textarea simply
//! works (unlike an in-terminal overlay).

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
mod host {
    use bevy::ecs::relationship::Relationship;
    use bevy::prelude::*;
    use bevy_cef::prelude::{
        BinEventEmitterPlugin, BinReceive, CefKeyboardTarget, WebviewExtendStandardMaterial,
    };
    use vmux_core::LastActivatedAt;
    use vmux_core::PageMetadata;
    use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
    use vmux_core::event::AgentComposeSubmitEvent;
    use vmux_service::protocol::ProcessId;
    use vmux_terminal::{Terminal, TerminalModeMap};

    pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
        host: "compose",
        title: "Compose",
        keywords: &["agent", "prompt", "ai", "chat"],
        icon: Some(vmux_core::BuiltinIcon::Sparkles),
        command_bar: false,
    };

    /// Carries the agent kind + working directory from compose-page attach to the
    /// submit observer, so spawning the CLI doesn't have to re-derive them.
    #[derive(Component, Clone)]
    pub struct ComposeContext {
        pub kind: AgentKind,
        pub cwd: std::path::PathBuf,
    }

    /// On the compose page's stack: the sibling stack where the agent terminal is
    /// booting. The compose stack stays visible (higher `LastActivatedAt`) until
    /// the terminal's TUI is ready, then [`swap_compose_on_ready`] reveals it.
    #[derive(Component, Clone, Copy)]
    pub struct PendingComposeSwap {
        pub terminal_stack: Entity,
    }

    pub struct AgentComposePlugin;

    impl Plugin for AgentComposePlugin {
        fn build(&self, app: &mut App) {
            app.world_mut().spawn(PAGE_MANIFEST);
            app.add_plugins(
                BinEventEmitterPlugin::<(AgentComposeSubmitEvent,)>::for_hosts(&["compose"]),
            )
            .add_observer(on_agent_compose_submit)
            .add_systems(Update, swap_compose_on_ready);
        }
    }

    /// Replace a stack's content with the compose page for `kind`.
    pub fn attach_compose_to_stack(
        kind: AgentKind,
        cwd: std::path::PathBuf,
        stack: Entity,
        children_q: &Query<&Children>,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) {
        crate::plugin::clear_stack_children(stack, children_q, commands);
        let title = format!("Start {}", kind.display_name());
        let url = kind.compose_url();
        commands.entity(stack).insert(PageMetadata {
            url: url.clone(),
            title: title.clone(),
            bg_color: Some("#0b0c0f".to_string()),
            ..default()
        });
        let browser = commands
            .spawn((
                vmux_layout::Browser::new_with_title(meshes, webview_mt, &url, &title),
                ComposeContext { kind, cwd },
                ChildOf(stack),
            ))
            .id();
        commands.entity(browser).insert(CefKeyboardTarget);
    }

    fn on_agent_compose_submit(
        trigger: On<BinReceive<AgentComposeSubmitEvent>>,
        ctx_q: Query<(&ComposeContext, &ChildOf)>,
        child_of_q: Query<&ChildOf>,
        mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
        mut commands: Commands,
    ) {
        let page = trigger.event_target();
        let Ok((ctx, page_parent)) = ctx_q.get(page) else {
            warn!("agent compose submit: no ComposeContext on {page:?}");
            return;
        };
        let compose_stack = page_parent.get();
        let text = trigger.event().payload.text.trim().to_string();
        let run = trigger.event().payload.submit && !text.is_empty();

        let pane = child_of_q.get(compose_stack).ok().map(|co| co.get());
        if !run || pane.is_none() {
            spawn_agent.write(SpawnAgentInStackRequest {
                kind: ctx.kind,
                cwd: ctx.cwd.clone(),
                session_id: None,
                stack: compose_stack,
                initial_prompt: run.then(|| text.clone()),
            });
            return;
        }

        let terminal_stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt(1),
                ChildOf(pane.unwrap()),
            ))
            .id();
        spawn_agent.write(SpawnAgentInStackRequest {
            kind: ctx.kind,
            cwd: ctx.cwd.clone(),
            session_id: None,
            stack: terminal_stack,
            initial_prompt: Some(text),
        });
        commands
            .entity(compose_stack)
            .insert(PendingComposeSwap { terminal_stack });
    }

    /// Reveal the booting agent and remove the compose page once the agent
    /// terminal's TUI is up (alt-screen).
    fn swap_compose_on_ready(
        pending_q: Query<(Entity, &PendingComposeSwap)>,
        children_q: Query<&Children>,
        term_q: Query<&ProcessId, With<Terminal>>,
        mode_map: Option<Res<TerminalModeMap>>,
        mut commands: Commands,
    ) {
        let Some(mode_map) = mode_map else { return };
        for (compose_stack, swap) in &pending_q {
            let ready = children_q
                .get(swap.terminal_stack)
                .map(|kids| {
                    kids.iter().any(|child| {
                        term_q
                            .get(child)
                            .ok()
                            .and_then(|pid| mode_map.modes.get(pid))
                            .map(|m| m.alt_screen)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if ready {
                commands
                    .entity(swap.terminal_stack)
                    .insert(LastActivatedAt::now());
                commands.entity(compose_stack).despawn();
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use host::{AgentComposePlugin, ComposeContext, PAGE_MANIFEST, attach_compose_to_stack};
