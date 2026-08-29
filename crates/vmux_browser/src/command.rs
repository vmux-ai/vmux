use bevy::{
    ecs::{message::Messages, relationship::Relationship, system::SystemParam},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use std::path::Path;
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, BrowserNavigationCommand, BrowserViewCommand,
    LayoutCommand, ReadAppCommands, StackCommand, open::OpenCommand,
};
use vmux_core::{
    HostSpawnRegistry, PageMetadata, PageOpenRequest, PageOpenTarget,
    page::{HostHistoryDelta, HostHistoryNavigation, PageReady},
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::event::{SideSheetCommandEvent, SideSheetResizeEvent};
use vmux_layout::{
    Header, LayoutCef,
    event::{HeaderCommandEvent, RELOAD_EVENT, ReloadEvent},
    pane::{Pane, PaneHoverIntent, PaneSplit, SideSheetCardCollapsed},
    side_sheet::{
        SideSheet, SideSheetPaneExpanded, SideSheetPosition, SideSheetSectionsExpanded,
        SideSheetWidth,
    },
    stack::{ActiveTabParam, Stack, focused_stack},
};

use vmux_terminal::{RestartPty, Terminal};

use crate::{knowledge_path_url, normalize_vmux_url, project_path_url};
pub(crate) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_header_command_emit)
            .add_observer(on_side_sheet_command_emit)
            .add_observer(on_side_sheet_resize)
            .add_observer(on_reload_notify_header)
            .add_observer(on_hard_reload_notify_header)
            .add_systems(Update, handle_browser_commands.in_set(ReadAppCommands));
    }
}

fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_stack: ActiveStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut meta_q: Query<&mut PageMetadata, With<Browser>>,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
    host_spawn: Res<HostSpawnRegistry>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut font_size_writer: MessageWriter<vmux_terminal::TerminalFontSizeCommand>,
    mut host_history: HostHistoryNavigation,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(browser_cmd) = cmd else {
            continue;
        };
        let Some(active) = active_stack.get() else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, co)| co.get() == active)
            .map(|(e, _)| e)
        else {
            continue;
        };
        let (is_terminal, is_file) = kind_q.get(webview).unwrap_or((false, false));
        let is_text_grid = is_terminal || is_file;
        match browser_cmd {
            BrowserCommand::Navigation(nav) => match nav {
                BrowserNavigationCommand::PrevPage => {
                    if is_terminal || host_history.stepped(webview, HostHistoryDelta::Back) {
                        continue;
                    }
                    commands.trigger(RequestGoBack { webview });
                }
                BrowserNavigationCommand::NextPage => {
                    if is_terminal || host_history.stepped(webview, HostHistoryDelta::Forward) {
                        continue;
                    }
                    commands.trigger(RequestGoForward { webview });
                }
                BrowserNavigationCommand::Reload => {
                    if is_terminal {
                        commands.trigger(RestartPty { entity: webview });
                    } else {
                        commands.trigger(RequestReload { webview });
                    }
                }
                BrowserNavigationCommand::HardReload => {
                    if is_terminal {
                        commands.trigger(RestartPty { entity: webview });
                    } else {
                        commands.trigger(RequestReloadIgnoreCache { webview });
                    }
                }
                BrowserNavigationCommand::Stop => {}
            },
            #[allow(clippy::single_match)]
            BrowserCommand::Open(open_cmd) => match open_cmd {
                OpenCommand::InPlace { .. } => {
                    let resolved = vmux_command::open::OpenUrl::of(
                        open_cmd,
                        effective_startup_url.as_ref().map(|s| s.0.as_str()),
                    );
                    if resolved.is_empty() {
                        continue;
                    }
                    let resolved = normalize_vmux_url(resolved.as_str());
                    let current_url = meta_q
                        .get(webview)
                        .map(|m| m.url.clone())
                        .unwrap_or_default();
                    if is_terminal
                        || host_spawn.needs_host_spawn(&current_url)
                        || host_spawn.needs_host_spawn(&resolved)
                    {
                        page_open_requests.write(PageOpenRequest {
                            target: PageOpenTarget::Stack(active),
                            url: resolved,
                            request_id: None,
                        });
                        continue;
                    }
                    if let Ok(mut meta) = meta_q.get_mut(webview) {
                        meta.url = resolved.clone();
                        meta.title = resolved.clone();
                        meta.icon = vmux_core::PageIcon::None;
                    }
                    commands
                        .entity(webview)
                        .insert(WebviewSource::new(&resolved));
                    commands.trigger(RequestNavigate {
                        webview,
                        url: resolved,
                    });
                }
                _ => {}
            },
            BrowserCommand::View(view) => match view {
                BrowserViewCommand::ZoomIn => {
                    if is_text_grid {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Increase);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 += 0.5;
                    }
                }
                BrowserViewCommand::ZoomOut => {
                    if is_text_grid {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Decrease);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 -= 0.5;
                    }
                }
                BrowserViewCommand::ZoomReset => {
                    if is_text_grid {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Reset);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 = 0.0;
                    }
                }
                BrowserViewCommand::DevTools => {
                    commands.trigger(RequestShowDevTool { webview });
                }
                BrowserViewCommand::ViewSource => {}
                BrowserViewCommand::Print => {}
            },
            BrowserCommand::Bar(_) => {}
        }
    }
}

#[derive(SystemParam)]
struct ActiveStack<'w, 's> {
    active_tab: ActiveTabParam<'w, 's>,
    all_children: Query<'w, 's, &'static Children>,
    leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
}

impl ActiveStack<'_, '_> {
    fn get(&self) -> Option<Entity> {
        let (_, _, stack) = focused_stack(
            self.active_tab.get(),
            &self.all_children,
            &self.leaf_panes,
            &self.pane_ts,
            &self.pane_children,
            &self.stack_ts,
        );
        stack
    }
}

fn on_header_command_emit(
    trigger: On<BinReceive<HeaderCommandEvent>>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
) {
    let cmd = match trigger.event().payload.header_command.as_str() {
        "prev_page" => BrowserCommand::Navigation(BrowserNavigationCommand::PrevPage),
        "next_page" => BrowserCommand::Navigation(BrowserNavigationCommand::NextPage),
        "reload" => BrowserCommand::Navigation(BrowserNavigationCommand::Reload),
        "focus_address_bar" => BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar),
        _ => return,
    };
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let cmd = AppCommand::Browser(cmd);
    issued.write(vmux_command::CommandIssued {
        caller,
        command: cmd.clone(),
    });
    messages.write(cmd);
}

fn on_reload_notify_header(
    _trigger: On<RequestReload>,
    cef: Option<Single<Entity, (With<LayoutCef>, With<PageReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef) = cef else { return };
    let cef_e = *cef;
    if browsers.can_emit_to(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}

fn on_hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    cef: Option<Single<Entity, (With<LayoutCef>, With<PageReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef) = cef else { return };
    let cef_e = *cef;
    if browsers.can_emit_to(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}

fn on_side_sheet_resize(
    trigger: On<BinReceive<SideSheetResizeEvent>>,
    mut width: ResMut<SideSheetWidth>,
    mut sheets: Query<(&SideSheetPosition, &mut vmux_flex::prelude::Node), With<SideSheet>>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<Messages<vmux_setting::SettingsSaveRequest>>>,
) {
    let next = trigger.event().payload.clamped();
    if width.0 == next {
        return;
    }
    width.apply(next, &mut sheets);
    let Some(mut settings) = settings else {
        return;
    };
    settings.layout.side_sheet.width = next;
    if let Some(mut saves) = saves {
        saves.write(vmux_setting::SettingsSaveRequest);
    }
}

fn on_side_sheet_command_emit(
    trigger: On<BinReceive<SideSheetCommandEvent>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut last_activated: Query<&mut LastActivatedAt>,
    sections_of: vmux_layout::side_sheet::SideSheetSections,
    mut hover_intent: ResMut<PaneHoverIntent>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    mut open_beside: MessageWriter<vmux_layout::OpenBesideRequest>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let Ok(pane_id) = evt.pane_id.parse::<u64>() else {
        return;
    };
    let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id) else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let stack_entities: Vec<Entity> = children.iter().filter(|&e| stack_q.contains(e)).collect();

    match evt.command.as_str() {
        "activate_stack" => {
            let Some(&target_stack) = stack_entities.get(evt.stack_index as usize) else {
                return;
            };
            let activated_at = LastActivatedAt::now();
            if let Ok(mut value) = last_activated.get_mut(target_pane) {
                *value = activated_at;
            } else {
                commands.entity(target_pane).insert(activated_at);
            }
            if let Ok(mut value) = last_activated.get_mut(target_stack) {
                *value = activated_at;
            } else {
                commands.entity(target_stack).insert(activated_at);
            }

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
        }
        "close_stack" => {
            let Some(&target_stack) = stack_entities.get(evt.stack_index as usize) else {
                return;
            };
            commands.entity(target_pane).insert(LastActivatedAt::now());
            commands.entity(target_stack).insert(LastActivatedAt::now());
            let cmd = AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
        }
        "new_stack" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            let cmd =
                AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: None }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        "collapse_card" => {
            commands
                .entity(target_pane)
                .insert(SideSheetCardCollapsed)
                .remove::<SideSheetPaneExpanded>();
        }
        "expand_card" => {
            commands
                .entity(target_pane)
                .remove::<SideSheetCardCollapsed>()
                .remove::<SideSheetPaneExpanded>();
        }
        "collapse_section" | "expand_section" => {
            let expanded = evt.command == "expand_section";
            if evt.path == "pane" {
                let mut pane = commands.entity(target_pane);
                if expanded {
                    pane.remove::<SideSheetCardCollapsed>()
                        .remove::<SideSheetPaneExpanded>();
                } else {
                    pane.insert(SideSheetCardCollapsed)
                        .remove::<SideSheetPaneExpanded>();
                }
                return;
            }
            let Some(space) = sections_of.space_of(target_pane) else {
                return;
            };
            let mut state = sections_of.under(target_pane);
            if !state.set(&evt.path, expanded) {
                return;
            }
            if state.is_empty() {
                commands.entity(space).remove::<SideSheetSectionsExpanded>();
            } else {
                commands.entity(space).insert(state);
            }
        }
        "open_project_path" => {
            let Some(url) = project_path_url(Path::new(&evt.path)) else {
                return;
            };
            open_beside.write(vmux_layout::OpenBesideRequest {
                pane: target_pane,
                direction: None,
                url,
                request_id: [0; 16],
                focus: true,
            });
        }
        "open_knowledge_path" => {
            let Some(mut url) = knowledge_path_url(
                &vmux_core::knowledge::KnowledgeVault::user().into_root(),
                Path::new(&evt.path),
            ) else {
                return;
            };
            if evt.stack_index > 0 && !Path::new(&evt.path).is_dir() {
                url.push_str(&format!("#L{}", evt.stack_index));
            }
            open_beside.write(vmux_layout::OpenBesideRequest {
                pane: target_pane,
                direction: None,
                url,
                request_id: [0; 16],
                focus: true,
            });
        }
        "open_tools" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                url: Some("vmux://tools/".to_string()),
            }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        "open_vault" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                url: Some("vmux://vault/".to_string()),
            }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_core::page::HostHistory;
    use vmux_layout::pane::Pane;
    use vmux_layout::space::{Space, SpaceId};
    use vmux_layout::stack::stack_bundle;
    use vmux_layout::tab::Tab;

    #[derive(Resource, Default)]
    struct CefNavigations(Vec<Entity>);

    impl CefNavigations {
        fn record_back(trigger: On<RequestGoBack>, mut navigations: ResMut<Self>) {
            navigations.0.push(trigger.webview);
        }
    }

    struct NavArrow {
        app: App,
        view: Entity,
    }

    impl NavArrow {
        fn over(page: impl Bundle) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_core::CorePlugin, CommandPlugin))
                .add_message::<AppCommand>()
                .add_message::<vmux_command::CommandIssued>()
                .add_message::<PageOpenRequest>()
                .add_message::<vmux_terminal::TerminalFontSizeCommand>()
                .init_resource::<HostSpawnRegistry>()
                .init_resource::<CefNavigations>()
                .add_observer(CefNavigations::record_back);

            let tab = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt::now()))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
                .id();
            let stack = app
                .world_mut()
                .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                .id();
            let view = app.world_mut().spawn((Browser, ChildOf(stack), page)).id();

            Self { app, view }
        }

        fn over_a_natively_hosted_page() -> Self {
            let mut history = HostHistory::default();
            history.observe("file:///a.rs", 0);
            history.observe("file:///b.rs", 0);
            Self::over(history)
        }

        fn pressed(&mut self, button: &str) {
            self.app
                .world_mut()
                .trigger(BinReceive::<HeaderCommandEvent> {
                    webview: Entity::PLACEHOLDER,
                    payload: HeaderCommandEvent {
                        header_command: button.to_string(),
                    },
                });
            self.app.update();
            self.app.update();
        }

        fn walked_back(&self) -> bool {
            self.app
                .world()
                .get::<HostHistory>(self.view)
                .is_some_and(HostHistory::can_go_forward)
        }

        fn cef_navigations(&self) -> Vec<Entity> {
            self.app.world().resource::<CefNavigations>().0.clone()
        }
    }

    #[test]
    fn the_back_arrow_walks_host_history_instead_of_asking_chromium() {
        let mut arrow = NavArrow::over_a_natively_hosted_page();

        arrow.pressed("prev_page");

        assert!(
            arrow.walked_back(),
            "the arrow must move the host history cursor off its newest entry"
        );
        assert!(
            arrow.cef_navigations().is_empty(),
            "a natively hosted page has no Chromium browser to walk back"
        );
    }

    #[test]
    fn the_back_arrow_still_asks_chromium_for_a_page_chromium_renders() {
        let mut arrow = NavArrow::over(());

        arrow.pressed("prev_page");

        assert_eq!(arrow.cef_navigations(), vec![arrow.view]);
    }

    #[test]
    fn side_sheet_close_routes_through_the_stack_command() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<AppCommand>()
            .add_message::<vmux_command::CommandIssued>()
            .init_resource::<PaneHoverIntent>()
            .add_observer(on_side_sheet_command_emit);

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app.world_mut().spawn((stack_bundle(), ChildOf(pane))).id();

        app.world_mut()
            .trigger(BinReceive::<SideSheetCommandEvent> {
                webview: Entity::PLACEHOLDER,
                payload: SideSheetCommandEvent {
                    command: "close_stack".to_string(),
                    pane_id: pane.to_bits().to_string(),
                    stack_index: 0,
                    path: String::new(),
                },
            });
        app.world_mut().flush();

        let commands = app.world().resource::<Messages<AppCommand>>();
        let mut cursor = commands.get_cursor();
        let sent: Vec<&AppCommand> = cursor.read(commands).collect();
        assert_eq!(
            sent,
            vec![&AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close
            ))],
        );
        assert!(app.world().get::<LastActivatedAt>(stack).is_some());
    }

    struct SideSheetSpaces {
        app: App,
        pane_in_first_tab: Entity,
        second_tab: Entity,
        tab_in_other_space: Entity,
    }

    impl SideSheetSpaces {
        fn start() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
                .add_message::<AppCommand>()
                .add_message::<vmux_command::CommandIssued>()
                .init_resource::<PaneHoverIntent>()
                .add_observer(on_side_sheet_command_emit);

            let space = app
                .world_mut()
                .spawn((Space, SpaceId("work".to_string())))
                .id();
            let first_tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
            let pane_in_first_tab = app.world_mut().spawn((Pane, ChildOf(first_tab))).id();
            app.world_mut()
                .spawn((stack_bundle(), ChildOf(pane_in_first_tab)));
            let second_tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
            let other_space = app
                .world_mut()
                .spawn((Space, SpaceId("play".to_string())))
                .id();
            let tab_in_other_space = app
                .world_mut()
                .spawn((Tab::default(), ChildOf(other_space)))
                .id();

            Self {
                app,
                pane_in_first_tab,
                second_tab,
                tab_in_other_space,
            }
        }

        fn expand(&mut self, section: &str) {
            self.app
                .world_mut()
                .trigger(BinReceive::<SideSheetCommandEvent> {
                    webview: Entity::PLACEHOLDER,
                    payload: SideSheetCommandEvent {
                        command: "expand_section".to_string(),
                        pane_id: self.pane_in_first_tab.to_bits().to_string(),
                        stack_index: 0,
                        path: section.to_string(),
                    },
                });
            self.app.world_mut().flush();
        }

        fn sections_under(&mut self, entity: Entity) -> SideSheetSectionsExpanded {
            self.app
                .world_mut()
                .run_system_once(
                    move |sections: vmux_layout::side_sheet::SideSheetSections| {
                        sections.under(entity)
                    },
                )
                .expect("the reader runs")
        }
    }

    #[test]
    fn an_expanded_card_stays_expanded_on_another_tab_of_the_same_space() {
        let mut spaces = SideSheetSpaces::start();
        spaces.expand("projects");

        let second_tab = spaces.second_tab;
        assert!(
            spaces.sections_under(second_tab).projects,
            "a card is expanded for the whole space, so switching tab must not fold it"
        );
    }

    #[test]
    fn expanding_a_card_leaves_the_other_spaces_alone() {
        let mut spaces = SideSheetSpaces::start();
        spaces.expand("projects");

        let elsewhere = spaces.tab_in_other_space;
        assert!(!spaces.sections_under(elsewhere).projects);
    }
}
