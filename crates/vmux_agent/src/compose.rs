//! Agent loading page: a pure-Dioxus (non-terminal) webview shown over a freshly
//! spawned agent terminal while it boots. Because it's a normal native-focus
//! page (not the keyboard-suppressed terminal window), its textarea works — so
//! the user can type a first prompt during the boot. On submit the prompt is
//! buffered onto the booting terminal (delivered when its TUI is ready) and the
//! loading page is removed, revealing the live agent.

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
mod host {
    use bevy::prelude::*;
    use bevy_cef::prelude::{
        BinEventEmitterPlugin, BinReceive, CefKeyboardTarget, WebviewExtendStandardMaterial,
    };
    use vmux_core::LastActivatedAt;
    use vmux_core::PageMetadata;
    use vmux_core::agent::AgentKind;
    use vmux_core::event::AgentComposeSubmitEvent;

    pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
        host: "compose",
        title: "Compose",
        keywords: &["agent", "prompt", "ai", "chat"],
        icon: Some(vmux_core::BuiltinIcon::Sparkles),
        command_bar: false,
    };

    /// On the loading page: the booting terminal it sits over, so submit can
    /// deliver the prompt to it.
    #[derive(Component, Clone, Copy)]
    pub struct ComposeContext {
        pub terminal: Entity,
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

    /// Spawn the loading page as the active content of `stack`, sitting over the
    /// already-spawned, still-booting `terminal` (also a child of `stack`).
    pub fn attach_compose_over_terminal(
        kind: AgentKind,
        terminal: Entity,
        stack: Entity,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) {
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
                ComposeContext { terminal },
                LastActivatedAt::now(),
                ChildOf(stack),
            ))
            .id();
        commands.entity(browser).insert(CefKeyboardTarget);
    }

    fn on_agent_compose_submit(
        trigger: On<BinReceive<AgentComposeSubmitEvent>>,
        ctx_q: Query<&ComposeContext>,
        mut commands: Commands,
    ) {
        let page = trigger.event_target();
        let Ok(ctx) = ctx_q.get(page) else {
            warn!("agent compose submit: no ComposeContext on {page:?}");
            return;
        };
        let text = trigger.event().payload.text.trim().to_string();
        let submit = trigger.event().payload.submit;
        if submit && !text.is_empty() {
            commands
                .entity(ctx.terminal)
                .insert(vmux_terminal::BufferedAgentPrompt { text, submit: true });
        }
        // Remove the loading page → the booting terminal becomes the active
        // content. Its TUI delivery happens via BufferedAgentPrompt on ready.
        commands.entity(page).try_despawn();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use host::{AgentComposePlugin, ComposeContext, PAGE_MANIFEST, attach_compose_over_terminal};
