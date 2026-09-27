use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::event::{
    AgentInstallRunRequest, AgentSetupPrereqRequest, AgentSetupPrereqStatus, AgentSetupResult,
    AgentSetupUiState,
};
use vmux_core::agent::AgentKind;

pub struct AgentSetupPlugin;

impl Plugin for AgentSetupPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiEventPlugin::<(AgentInstallRunRequest, AgentSetupPrereqRequest)>::default(),
            vmux_core::host::UiStatePlugin::<AgentSetupUiState>::default(),
        ))
        .add_observer(on_agent_install_run)
        .add_observer(on_agent_setup_prereq_request)
        .add_systems(Update, auto_redirect_agent_setup_when_installed)
        .add_systems(
            Update,
            (detect_agent_install_outcome, publish_agent_install_outcome).chain(),
        );
    }
}

#[derive(Component)]
struct AgentInstallPane {
    setup_stack: Entity,
    setup_webview: Entity,
    agent: AgentKind,
    process_id: vmux_api::protocol::ProcessId,
    armed: bool,
}

#[derive(Component)]
struct AgentInstallCompleted {
    result: AgentSetupResult,
    close_pane: bool,
}

#[derive(Component)]
pub(crate) struct AgentSetupNavigated;

#[derive(Component)]
#[require(AgentSetupUiStateUpdates)]
pub(crate) struct AgentSetupView;

type AgentSetupUiStateUpdates = vmux_core::host::UiState<AgentSetupUiState>;

fn run_install_in_new_tab(run: &mut MessageWriter<vmux_terminal::RunShellRequest>, command: &str) {
    run.write(vmux_terminal::RunShellRequest {
        command: command.to_string(),
        cwd: String::new(),
        mode: vmux_terminal::ShellMode::NewTab,
    });
}

fn prereq_needs_homebrew(segment: &str, brew_present: bool) -> bool {
    cfg!(target_os = "macos") && vmux_core::agent_setup::requires_homebrew(segment) && !brew_present
}

fn on_agent_setup_prereq_request(
    trigger: On<UiInput<AgentSetupPrereqRequest>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let segment = &trigger.event().payload.agent;
    let brew_present = crate::exec::find_executable("brew").is_some();
    let needs_homebrew = prereq_needs_homebrew(segment, brew_present);
    commands.trigger(
        vmux_core::host::UiStateWrite::<AgentSetupUiState>::from_event(
            webview,
            &AgentSetupPrereqStatus { needs_homebrew },
        ),
    );
}

fn install_outcome(armed: bool, installed: bool) -> Option<bool> {
    if !armed {
        return None;
    }
    Some(installed)
}

fn close_install_pane_after_success(url: &str) -> bool {
    let Some(route) = vmux_api::VmuxRoute::parse(url) else {
        return false;
    };
    let Some(install) = vmux_api::VmuxRoute::parse("vmux://tools/acp") else {
        return false;
    };
    route.same_page(&install)
}

fn detect_agent_install_outcome(
    mut events: MessageReader<vmux_terminal::CommandLifecycleEvent>,
    mut install_panes: Query<(Entity, &mut AgentInstallPane)>,
    setup_stacks: Query<&vmux_core::PageMetadata, With<vmux_layout::stack::Stack>>,
    mut commands: Commands,
) {
    use vmux_api::protocol::CommandLifecycleKind;
    for ev in events.read() {
        for (install_pane, mut pane) in &mut install_panes {
            if pane.process_id != ev.process_id {
                continue;
            }
            match ev.kind {
                CommandLifecycleKind::Started => pane.armed = true,
                CommandLifecycleKind::Ended { .. } => {
                    let installed = crate::exec::find_executable(pane.agent.executable()).is_some();
                    if let Some(ok) = install_outcome(pane.armed, installed) {
                        let close_pane = ok
                            && setup_stacks
                                .get(pane.setup_stack)
                                .is_ok_and(|meta| close_install_pane_after_success(&meta.url));
                        commands.entity(install_pane).insert(AgentInstallCompleted {
                            result: AgentSetupResult {
                                agent: pane.agent.as_url_segment().to_string(),
                                ok,
                            },
                            close_pane,
                        });
                        pane.armed = false;
                    }
                }
            }
        }
    }
}

fn publish_agent_install_outcome(
    completed: Query<
        (Entity, &AgentInstallPane, &AgentInstallCompleted),
        Added<AgentInstallCompleted>,
    >,
    mut commands: Commands,
) {
    for (entity, pane, completed) in &completed {
        commands.trigger(
            vmux_core::host::UiStateWrite::<AgentSetupUiState>::from_event(
                pane.setup_webview,
                &completed.result,
            ),
        );
        if completed.close_pane {
            commands
                .entity(entity)
                .insert(vmux_layout::pane::ForcePaneClose);
        }
    }
}

fn on_agent_install_run(
    trigger: On<UiInput<AgentInstallRunRequest>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    ctx: vmux_layout::pane::PlacementCtx,
    mut install_panes: Query<(Entity, &mut AgentInstallPane)>,
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
    let brew_present = !cfg!(target_os = "macos") || crate::exec::find_executable("brew").is_some();
    let Some(command) = vmux_core::agent_setup::install_command_chained(segment, brew_present)
    else {
        warn!("agent install run: unknown agent segment '{segment}'");
        return;
    };
    let input = vmux_terminal::shell_input::shell_command_input(&command);

    for (entity, mut pane) in &mut install_panes {
        if pane.setup_webview == webview && pane.agent == kind {
            reinput.write(vmux_terminal::TerminalReinputRequest {
                process_id: pane.process_id,
                data: input.clone(),
            });
            pane.armed = false;
            commands.entity(entity).remove::<AgentInstallCompleted>();
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
    let process_id = vmux_api::protocol::ProcessId::new();
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
        shell: None,
        agent_run: false,
        pending_input: Some(input),
        process_id: Some(process_id),
        activate: true,
    });
}

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
            url: format!("{}cli", kind.cli_url_prefix()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::host::UiStateWrite;

    #[derive(Resource, Default)]
    struct Published(Vec<(Entity, AgentSetupResult)>);

    fn record_published(
        trigger: On<UiStateWrite<AgentSetupUiState>>,
        mut published: ResMut<Published>,
    ) {
        let Some(result) = &trigger.event().patch().result else {
            return;
        };
        published
            .0
            .push((trigger.event().webview(), result.clone()));
    }

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

    #[test]
    fn successful_manager_install_closes_terminal_pane() {
        assert!(close_install_pane_after_success("vmux://tools/acp"));
        assert!(close_install_pane_after_success("vmux://tools/acp/"));
        assert!(!close_install_pane_after_success(
            "vmux://sessions/codex/setup"
        ));
    }

    #[test]
    fn completed_install_entity_projects_result_to_its_setup_page() {
        let mut app = App::new();
        app.init_resource::<Published>()
            .add_observer(record_published)
            .add_systems(Update, publish_agent_install_outcome);
        let setup_webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            AgentInstallPane {
                setup_stack: Entity::PLACEHOLDER,
                setup_webview,
                agent: AgentKind::Codex,
                process_id: vmux_api::protocol::ProcessId::new(),
                armed: false,
            },
            AgentInstallCompleted {
                result: AgentSetupResult {
                    agent: "codex".into(),
                    ok: true,
                },
                close_pane: false,
            },
        ));

        app.update();

        let published = app.world().resource::<Published>();
        assert_eq!(published.0.len(), 1);
        assert_eq!(published.0[0].0, setup_webview);
        assert_eq!(published.0[0].1.agent, "codex");
        assert!(published.0[0].1.ok);
    }
}
