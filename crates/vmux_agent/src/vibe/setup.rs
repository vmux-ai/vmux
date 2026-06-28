pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

#[cfg(not(target_arch = "wasm32"))]
use crate::vibe::setup::event::{
    AGENT_SETUP_PREREQ_EVENT, AGENT_SETUP_RESULT_EVENT, AgentInstallRunRequest,
    AgentSetupPrereqRequest, AgentSetupPrereqStatus, AgentSetupResult,
};
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
    setup_webview: Entity,
    agent: AgentKind,
    process_id: vmux_service::protocol::ProcessId,
    armed: bool,
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
        app.add_plugins(BinEventEmitterPlugin::<(
            AgentInstallRunRequest,
            AgentSetupPrereqRequest,
        )>::for_hosts(&["agent"]))
            .add_observer(on_agent_install_run)
            .add_observer(on_agent_setup_prereq_request)
            .add_systems(Update, auto_redirect_agent_setup_when_installed)
            .add_systems(Update, detect_agent_install_outcome);
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

/// Homebrew is needed first only on macOS, only for cask agents, and only when
/// `brew` is not already resolvable.
#[cfg(not(target_arch = "wasm32"))]
fn prereq_needs_homebrew(segment: &str, brew_present: bool) -> bool {
    cfg!(target_os = "macos") && vmux_core::agent_setup::requires_homebrew(segment) && !brew_present
}

#[cfg(not(target_arch = "wasm32"))]
fn on_agent_setup_prereq_request(
    trigger: On<BinReceive<AgentSetupPrereqRequest>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let segment = &trigger.event().payload.agent;
    let brew_present = crate::exec::find_executable("brew").is_some();
    let needs_homebrew = prereq_needs_homebrew(segment, brew_present);
    if browsers.has_browser(webview) && browsers.host_emit_ready(&webview) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            AGENT_SETUP_PREREQ_EVENT,
            &AgentSetupPrereqStatus { needs_homebrew },
        ));
    }
}

/// Decide an install pane's outcome from a completed command.
///
/// `None` while not yet `armed` (ignores the shell's spurious pre-command
/// completion). Once armed: `Some(true)` when the agent binary is present
/// (success), `Some(false)` when still absent (failure → Retry).
#[cfg(not(target_arch = "wasm32"))]
fn install_outcome(armed: bool, installed: bool) -> Option<bool> {
    if !armed {
        return None;
    }
    Some(installed)
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_agent_install_outcome(
    mut events: MessageReader<vmux_terminal::CommandLifecycleEvent>,
    mut install_panes: Query<&mut AgentInstallPane>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    use vmux_service::protocol::CommandLifecycleKind;
    for ev in events.read() {
        for mut pane in &mut install_panes {
            if pane.process_id != ev.process_id {
                continue;
            }
            match ev.kind {
                CommandLifecycleKind::Started => pane.armed = true,
                CommandLifecycleKind::Ended { .. } => {
                    let installed = crate::exec::find_executable(pane.agent.executable()).is_some();
                    match install_outcome(pane.armed, installed) {
                        Some(false) => {
                            if browsers.has_browser(pane.setup_webview)
                                && browsers.host_emit_ready(&pane.setup_webview)
                            {
                                commands.trigger(BinHostEmitEvent::from_rkyv(
                                    pane.setup_webview,
                                    AGENT_SETUP_RESULT_EVENT,
                                    &AgentSetupResult { ok: false },
                                ));
                            }
                            pane.armed = false;
                        }
                        Some(true) | None => {}
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_agent_install_run(
    trigger: On<BinReceive<AgentInstallRunRequest>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    ctx: vmux_layout::pane::PlacementCtx,
    mut install_panes: Query<&mut AgentInstallPane>,
    mut commands: Commands,
    mut spawn: MessageWriter<vmux_terminal::TerminalStackSpawnRequest>,
    mut run: MessageWriter<vmux_terminal::RunShellRequest>,
    mut reinput: MessageWriter<vmux_terminal::TerminalReinputRequest>,
) {
    let webview = trigger.event().webview;
    let segment = &trigger.event().payload.agent;
    let Some(kind) = AgentKind::from_url_segment(segment) else {
        warn!("agent install run: unknown agent segment '{segment}'");
        return;
    };
    let brew_present = crate::exec::find_executable("brew").is_some();
    let Some(command) = vmux_core::agent_setup::install_command_chained(segment, brew_present)
    else {
        warn!("agent install run: unknown agent segment '{segment}'");
        return;
    };
    let input = vmux_terminal::shell_input::shell_command_input(&command);

    for mut pane in &mut install_panes {
        if pane.setup_webview == webview {
            reinput.write(vmux_terminal::TerminalReinputRequest {
                process_id: pane.process_id,
                data: input.clone(),
            });
            pane.armed = false;
            return;
        }
    }

    let (Some(pane), Some(setup_stack)) = (focus.pane, focus.stack) else {
        run_install_in_new_tab(&mut run, &command);
        return;
    };
    if !ctx.leaf_panes.contains(pane) {
        run_install_in_new_tab(&mut run, &command);
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
    let process_id = vmux_service::protocol::ProcessId::new();
    commands.entity(install_pane).insert(AgentInstallPane {
        setup_stack,
        setup_webview: webview,
        agent: kind,
        process_id,
        armed: false,
    });
    spawn.write(vmux_terminal::TerminalStackSpawnRequest {
        pane: install_pane,
        cwd: None,
        pending_input: Some(input),
        process_id: Some(process_id),
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn prereq_needs_homebrew_logic() {
        if cfg!(target_os = "macos") {
            assert!(prereq_needs_homebrew("claude", false));
            assert!(prereq_needs_homebrew("codex", false));
            assert!(!prereq_needs_homebrew("claude", true));
        } else {
            assert!(!prereq_needs_homebrew("claude", false));
        }
        assert!(!prereq_needs_homebrew("vibe", false));
        assert!(!prereq_needs_homebrew("nope", false));
    }

    #[test]
    fn install_outcome_gates_on_armed_and_presence() {
        assert_eq!(install_outcome(false, true), None);
        assert_eq!(install_outcome(false, false), None);
        assert_eq!(install_outcome(true, true), Some(true));
        assert_eq!(install_outcome(true, false), Some(false));
    }
}
