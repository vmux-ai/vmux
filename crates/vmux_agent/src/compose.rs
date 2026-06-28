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
    use vmux_core::PageMetadata;
    use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
    use vmux_core::event::AgentComposeSubmitEvent;

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

    pub struct AgentComposePlugin;

    impl Plugin for AgentComposePlugin {
        fn build(&self, app: &mut App) {
            app.world_mut().spawn(PAGE_MANIFEST);
            app.add_plugins(
                BinEventEmitterPlugin::<(AgentComposeSubmitEvent,)>::for_hosts(&["compose"]),
            )
            .add_observer(on_agent_compose_submit);
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
        mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    ) {
        let entity = trigger.event_target();
        let Ok((ctx, child_of)) = ctx_q.get(entity) else {
            warn!("agent compose submit: no ComposeContext on {entity:?}");
            return;
        };
        let text = trigger.event().payload.text.trim().to_string();
        let submit = trigger.event().payload.submit;
        let initial_prompt = (submit && !text.is_empty()).then_some(text);
        spawn_agent.write(SpawnAgentInStackRequest {
            kind: ctx.kind,
            cwd: ctx.cwd.clone(),
            session_id: None,
            stack: child_of.get(),
            initial_prompt,
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use host::{AgentComposePlugin, ComposeContext, PAGE_MANIFEST, attach_compose_to_stack};
