//! Acting on what the user asked for.
//!
//! App commands arrive through `ReadAppCommands`; the rest come from the layout page's own
//! chrome as bin events — the header, the side sheet, and the two reload paths — and are handled
//! as observers so they take effect in the frame they were sent.

use bevy::{
    ecs::{message::Messages, relationship::Relationship},
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
    HostSpawnRegistry, PageMetadata, PageOpenRequest, PageOpenTarget, page::PageReady,
};
use vmux_history::LastActivatedAt;
pub use vmux_layout::Browser;
use vmux_layout::event::SideSheetCommandEvent;
use vmux_layout::{
    Header, LayoutCef,
    event::{HeaderCommandEvent, RELOAD_EVENT, ReloadEvent},
    pane::{Pane, PaneHoverIntent, PaneSplit, SideSheetCardCollapsed},
    side_sheet::{SideSheet, SideSheetPaneExpanded, SideSheetSectionsExpanded},
    stack::{ActiveTabParam, Stack, focused_stack},
    tab::Tab,
};

use vmux_terminal::{RestartPty, Terminal};

use crate::{knowledge_path_url, normalize_vmux_url};
pub(crate) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_header_command_emit)
            .add_observer(on_side_sheet_command_emit)
            .add_observer(on_reload_notify_header)
            .add_observer(on_hard_reload_notify_header)
            .add_systems(Update, handle_browser_commands.in_set(ReadAppCommands));
    }
}

pub(crate) fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut meta_q: Query<&mut PageMetadata, With<Browser>>,
    terminal_q: Query<(), With<Terminal>>,
    effective_startup_url: Option<Res<vmux_layout::settings::EffectiveStartupUrl>>,
    host_spawn: Res<HostSpawnRegistry>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut font_size_writer: MessageWriter<vmux_terminal::TerminalFontSizeCommand>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(browser_cmd) = cmd else {
            continue;
        };
        let (_, _, active_stack_opt) = focused_stack(
            active_tab_param.get(),
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_stack_opt else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, co)| co.get() == active)
            .map(|(e, _)| e)
        else {
            continue;
        };
        let is_terminal = terminal_q.contains(webview);
        match browser_cmd {
            BrowserCommand::Navigation(nav) => match nav {
                BrowserNavigationCommand::PrevPage => {
                    if !is_terminal {
                        commands.trigger(RequestGoBack { webview });
                    }
                }
                BrowserNavigationCommand::NextPage => {
                    if !is_terminal {
                        commands.trigger(RequestGoForward { webview });
                    }
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
                    if is_terminal {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Increase);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 += 0.5;
                    }
                }
                BrowserViewCommand::ZoomOut => {
                    if is_terminal {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Decrease);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 -= 0.5;
                    }
                }
                BrowserViewCommand::ZoomReset => {
                    if is_terminal {
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
    if browsers.has_browser(cef_e) && browsers.host_emit_ready(&cef_e) {
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
    if browsers.has_browser(cef_e) && browsers.host_emit_ready(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}
fn on_side_sheet_command_emit(
    trigger: On<BinReceive<SideSheetCommandEvent>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut last_activated: Query<&mut LastActivatedAt>,
    child_of: Query<&ChildOf>,
    tabs: Query<(), With<Tab>>,
    section_states: Query<&SideSheetSectionsExpanded, With<Tab>>,
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
            let mut current = target_pane;
            let tab = loop {
                if tabs.contains(current) {
                    break Some(current);
                }
                let Ok(parent) = child_of.get(current) else {
                    break None;
                };
                current = parent.parent();
            };
            let Some(tab) = tab else {
                return;
            };
            let mut state = section_states.get(tab).copied().unwrap_or_default();
            if !state.set(&evt.path, expanded) {
                return;
            }
            if state.is_empty() {
                commands.entity(tab).remove::<SideSheetSectionsExpanded>();
            } else {
                commands.entity(tab).insert(state);
            }
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
                profile: None,
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
