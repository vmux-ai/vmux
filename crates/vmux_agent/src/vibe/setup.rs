pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

#[cfg(not(target_arch = "wasm32"))]
use crate::vibe::setup::event::AgentInstallRunRequest;
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::agent::AgentKind;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct AgentInstallPane {
    setup_stack: Entity,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub(crate) struct AgentSetupNavigated;

#[cfg(not(target_arch = "wasm32"))]
pub struct AgentSetupPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for AgentSetupPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(AgentInstallRunRequest,)>::for_hosts(&["agent"]))
            .add_observer(on_agent_install_run)
            .add_systems(Update, auto_redirect_agent_setup_when_installed);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_install_in_new_tab(run: &mut MessageWriter<vmux_terminal::RunShellRequest>, command: &str) {
    run.write(vmux_terminal::RunShellRequest {
        command: command.to_string(),
        cwd: String::new(),
        mode: vmux_terminal::ShellMode::NewTab,
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn on_agent_install_run(
    trigger: On<BinReceive<AgentInstallRunRequest>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    ctx: vmux_layout::pane::PlacementCtx,
    mut commands: Commands,
    mut spawn: MessageWriter<vmux_terminal::TerminalStackSpawnRequest>,
    mut run: MessageWriter<vmux_terminal::RunShellRequest>,
) {
    let segment = &trigger.event().payload.agent;
    let Some(command) = vmux_core::agent_setup::install_command(segment) else {
        warn!("agent install run: unknown agent segment '{segment}'");
        return;
    };
    let input = vmux_terminal::shell_input::shell_command_input(command);
    let (Some(pane), Some(setup_stack)) = (focus.pane, focus.stack) else {
        run_install_in_new_tab(&mut run, command);
        return;
    };
    if !ctx.leaf_panes.contains(pane) {
        run_install_in_new_tab(&mut run, command);
        return;
    }
    let existing_tabs: Vec<Entity> = ctx
        .pane_children
        .get(pane)
        .map(|c| c.iter().filter(|&e| ctx.tab_filter.contains(e)).collect())
        .unwrap_or_default();
    let already_split = ctx.split_dir_q.contains(pane);
    let install_pane = vmux_layout::pane::split_or_extend(
        &mut commands,
        pane,
        vmux_layout::pane::PaneSplitDirection::Row,
        &existing_tabs,
        true,
        already_split,
    );
    commands
        .entity(install_pane)
        .insert(AgentInstallPane { setup_stack });
    spawn.write(vmux_terminal::TerminalStackSpawnRequest {
        pane: install_pane,
        cwd: None,
        pending_input: Some(input),
        process_id: Some(vmux_service::protocol::ProcessId::new()),
        activate: true,
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn auto_redirect_agent_setup_when_installed(
    time: Res<Time>,
    mut throttle: Local<f32>,
    setup_stacks: Query<
        (Entity, &vmux_core::PageMetadata),
        (
            With<vmux_layout::stack::Stack>,
            Without<AgentSetupNavigated>,
        ),
    >,
    install_panes: Query<(Entity, &AgentInstallPane)>,
    mut commands: Commands,
) {
    *throttle += time.delta_secs();
    if *throttle < 0.5 {
        return;
    }
    *throttle = 0.0;

    for (setup_stack, meta) in &setup_stacks {
        let Some(kind) = AgentKind::all()
            .into_iter()
            .find(|k| meta.url == k.setup_url())
        else {
            continue;
        };
        if crate::exec::find_executable(kind.executable()).is_none() {
            continue;
        }
        commands.spawn(vmux_core::PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack: setup_stack,
            url: kind.cli_url_prefix(),
            request_id: None,
        });
        commands.entity(setup_stack).insert(AgentSetupNavigated);
        for (install_pane, marker) in &install_panes {
            if marker.setup_stack == setup_stack {
                commands
                    .entity(install_pane)
                    .insert(vmux_layout::pane::ForcePaneClose);
            }
        }
    }
}
