use bevy::{
    ecs::{message::Messages, relationship::Relationship, system::SystemParam},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_api::VmuxRoute;
use vmux_command::{
    CommandDefinition, CommandInvocation, CommandMcp, CommandRequest, CommandTypePlugin,
    InputSchema, ReadCommandRequests,
};
use vmux_core::{
    HostSpawnRoute, PageMetadata, PageOpenRequest, PageOpenTarget,
    host::{UiStateWrite, page::NativelyHosted},
    page::{HostHistory, HostHistoryDelta, HostHistoryStep, PageReady},
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::event::{
    SideSheetProjectOpenRequest, SideSheetResizeEvent, SideSheetSectionRequest,
    SideSheetStackActivateRequest, SideSheetStackCloseRequest, SideSheetStackCreateRequest,
};
use vmux_layout::{
    Header, LayoutCef,
    event::{
        HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
        ReloadEffect,
    },
    pane::{Pane, PaneHoverIntent, PaneSplit, SideSheetCardCollapsed},
    side_sheet::{
        SideSheet, SideSheetPaneExpanded, SideSheetPosition, SideSheetSectionsExpanded,
        SideSheetWidth,
    },
    stack::{ActiveTabParam, CloseStackRequest, Stack, focused_stack},
    state::LayoutUiState,
};

use vmux_terminal::{RestartPty, Terminal};

pub(crate) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            CommandTypePlugin::<NavigationRequest>::default(),
            CommandTypePlugin::<OpenRequest>::default(),
            CommandTypePlugin::<ZoomRequest>::default(),
            CommandTypePlugin::<ShowDevToolsRequest>::default(),
        ))
        .add_observer(on_header_back)
        .add_observer(on_header_forward)
        .add_observer(on_header_reload)
        .add_observer(on_header_address_focus)
        .add_observer(on_side_sheet_stack_activate)
        .add_observer(on_side_sheet_stack_close)
        .add_observer(on_side_sheet_stack_create)
        .add_observer(on_side_sheet_project_open)
        .add_observer(on_side_sheet_section)
        .add_observer(on_side_sheet_resize)
        .add_observer(on_reload_notify_header)
        .add_observer(on_hard_reload_notify_header)
        .add_systems(
            Update,
            (
                handle_navigation_requests,
                handle_open_requests,
                handle_zoom_requests,
                show_dev_tools,
            )
                .chain()
                .in_set(ReadCommandRequests),
        );
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationRequest {
    Back,
    Forward,
    Reload,
    HardReload,
    Stop,
}

impl CommandRequest for NavigationRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("browser_prev_page", "Back", "Browser > Navigation")
                .accelerator("super+[")
                .mcp(CommandMcp::new("Back", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_next_page", "Forward", "Browser > Navigation")
                .accelerator("super+]")
                .mcp(CommandMcp::new("Forward", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_reload", "Reload", "Browser > Navigation")
                .accelerator("super+r")
                .direct("Super+r")
                .mcp(CommandMcp::new("Reload", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_hard_reload", "Hard Reload", "Browser > Navigation")
                .accelerator("super+shift+r")
                .direct("Super+Shift+R")
                .mcp(CommandMcp::new("Hard Reload", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_stop", "Stop Loading", "Browser > Navigation")
                .accelerator("super+.")
                .hidden()
                .mcp(CommandMcp::new("Stop Loading", InputSchema::object()).allow_agent()),
        ]
    }
}

impl TryFrom<&CommandInvocation> for NavigationRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        let request = match invocation.id.as_str() {
            "browser_prev_page" => Self::Back,
            "browser_next_page" => Self::Forward,
            "browser_reload" => Self::Reload,
            "browser_hard_reload" => Self::HardReload,
            "browser_stop" => Self::Stop,
            _ => return Err(()),
        };
        Ok(request)
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub url: Option<String>,
}

impl OpenRequest {
    fn resolved_url(&self, startup_url: Option<&str>) -> String {
        for candidate in [self.url.as_deref(), startup_url] {
            if let Some(url) = candidate.filter(|url| !url.is_empty()) {
                return url.to_string();
            }
        }
        String::new()
    }
}

impl CommandRequest for OpenRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("open_in_place", "Open Here", "Browser > Open").mcp(
                CommandMcp::new(
                    "Navigate the currently focused stack to the given URL. Equivalent to the user typing a URL in the address bar. Use when the user asks to 'go to', 'navigate to', or 'open' a URL without specifying placement; the current page is replaced. If url is omitted, opens the configured startup URL.",
                    InputSchema::object().optional(
                        "url",
                        InputSchema::string().description(
                            "Absolute URL to open. If omitted, opens the startup URL.",
                        ),
                    ),
                ),
            ),
        ]
    }
}

impl TryFrom<&CommandInvocation> for OpenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "open_in_place")
            .then(|| Self {
                url: invocation.argument("url"),
            })
            .ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomRequest {
    In,
    Out,
    Reset,
}

impl CommandRequest for ZoomRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("browser_zoom_in", "Zoom In", "Browser > View")
                .accelerator("super+=")
                .mcp(CommandMcp::new("Zoom In", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_zoom_out", "Zoom Out", "Browser > View")
                .accelerator("super+-")
                .mcp(CommandMcp::new("Zoom Out", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_zoom_reset", "Actual Size", "Browser > View")
                .accelerator("super+0")
                .mcp(CommandMcp::new("Actual Size", InputSchema::object()).allow_agent()),
        ]
    }
}

impl TryFrom<&CommandInvocation> for ZoomRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        let request = match invocation.id.as_str() {
            "browser_zoom_in" => Self::In,
            "browser_zoom_out" => Self::Out,
            "browser_zoom_reset" => Self::Reset,
            _ => return Err(()),
        };
        Ok(request)
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowDevToolsRequest;

impl CommandRequest for ShowDevToolsRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("browser_dev_tools", "Developer Tools", "Browser > View")
                .accelerator("super+alt+i")
                .mcp(CommandMcp::new("Developer Tools", InputSchema::object()).allow_agent()),
        ]
    }
}

impl TryFrom<&CommandInvocation> for ShowDevToolsRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "browser_dev_tools")
            .then_some(Self)
            .ok_or(())
    }
}

fn handle_navigation_requests(
    mut navigation_requests: MessageReader<NavigationRequest>,
    active_stack: ActiveStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    host_histories: Query<(), With<HostHistory>>,
    mut host_history_steps: MessageWriter<HostHistoryStep>,
    mut commands: Commands,
) {
    for request in navigation_requests.read() {
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
        let (is_terminal, _) = kind_q.get(webview).unwrap_or((false, false));
        match request {
            NavigationRequest::Back => {
                if is_terminal {
                    continue;
                }
                if host_histories.contains(webview) {
                    host_history_steps.write(HostHistoryStep {
                        webview,
                        delta: HostHistoryDelta::Back,
                    });
                } else {
                    commands.trigger(RequestGoBack { webview });
                }
            }
            NavigationRequest::Forward => {
                if is_terminal {
                    continue;
                }
                if host_histories.contains(webview) {
                    host_history_steps.write(HostHistoryStep {
                        webview,
                        delta: HostHistoryDelta::Forward,
                    });
                } else {
                    commands.trigger(RequestGoForward { webview });
                }
            }
            NavigationRequest::Reload => {
                if is_terminal {
                    commands.trigger(RestartPty { entity: webview });
                } else {
                    commands.trigger(RequestReload { webview });
                }
            }
            NavigationRequest::HardReload => {
                if is_terminal {
                    commands.trigger(RestartPty { entity: webview });
                } else {
                    commands.trigger(RequestReloadIgnoreCache { webview });
                }
            }
            NavigationRequest::Stop => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_open_requests(
    mut open_requests: MessageReader<OpenRequest>,
    active_stack: ActiveStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
    native_pages: Query<&NativelyHosted>,
    host_spawn_routes: Query<&HostSpawnRoute>,
    mut meta_q: Query<&mut PageMetadata, With<Browser>>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for request in open_requests.read() {
        let Some(active) = active_stack.get() else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, child_of)| child_of.get() == active)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        let (is_terminal, _) = kind_q.get(webview).unwrap_or((false, false));
        let resolved = request.resolved_url(
            effective_startup_url
                .as_ref()
                .map(|startup| startup.0.as_str()),
        );
        if resolved.is_empty() {
            continue;
        }
        let resolved =
            VmuxRoute::canonical(&resolved).unwrap_or_else(|| resolved.trim().to_string());
        let current_url = meta_q
            .get(webview)
            .map(|metadata| metadata.url.clone())
            .unwrap_or_default();
        let current_is_hosted = native_pages
            .iter()
            .any(|page| page.answers_for(&current_url))
            || host_spawn_routes
                .iter()
                .any(|route| route.answers_for(&current_url));
        let resolved_is_hosted = native_pages.iter().any(|page| page.answers_for(&resolved))
            || host_spawn_routes
                .iter()
                .any(|route| route.answers_for(&resolved));
        if is_terminal || current_is_hosted || resolved_is_hosted {
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(active),
                url: resolved,
                request_id: None,
            });
            continue;
        }
        if let Ok(mut metadata) = meta_q.get_mut(webview) {
            metadata.url = resolved.clone();
            metadata.title = resolved.clone();
            metadata.icon = vmux_core::PageIcon::None;
        }
        commands
            .entity(webview)
            .insert(WebviewSource::new(&resolved));
        commands.trigger(RequestNavigate {
            webview,
            url: resolved,
        });
    }
}

fn handle_zoom_requests(
    mut requests: MessageReader<ZoomRequest>,
    active_stack: ActiveStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut font_size_writer: MessageWriter<vmux_terminal::TerminalFontSizeCommand>,
) {
    for request in requests.read() {
        let Some(active) = active_stack.get() else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, child_of)| child_of.get() == active)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        let (is_terminal, is_file) = kind_q.get(webview).unwrap_or((false, false));
        let is_text_grid = is_terminal || is_file;
        match request {
            ZoomRequest::In => {
                if is_text_grid {
                    font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Increase);
                } else if let Ok(mut zoom) = zoom_q.get_mut(webview) {
                    zoom.0 += 0.5;
                }
            }
            ZoomRequest::Out => {
                if is_text_grid {
                    font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Decrease);
                } else if let Ok(mut zoom) = zoom_q.get_mut(webview) {
                    zoom.0 -= 0.5;
                }
            }
            ZoomRequest::Reset => {
                if is_text_grid {
                    font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Reset);
                } else if let Ok(mut zoom) = zoom_q.get_mut(webview) {
                    zoom.0 = 0.0;
                }
            }
        }
    }
}

fn show_dev_tools(
    mut requests: MessageReader<ShowDevToolsRequest>,
    active_stack: ActiveStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    for _ in requests.read() {
        let Some(active) = active_stack.get() else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, child_of)| child_of.get() == active)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        commands.trigger(RequestShowDevTool { webview });
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

fn on_header_back(
    trigger: On<UiInput<HeaderBackRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_prev_page",
    ));
}

fn on_header_forward(
    trigger: On<UiInput<HeaderForwardRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_next_page",
    ));
}

fn on_header_reload(
    trigger: On<UiInput<HeaderReloadRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_reload",
    ));
}

fn on_header_address_focus(
    trigger: On<UiInput<HeaderAddressFocusRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_open_page_in_command_bar",
    ));
}

fn on_reload_notify_header(
    _trigger: On<RequestReload>,
    layouts: Query<(Entity, &HostWindow), (With<LayoutCef>, With<PageReady>)>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    mut commands: Commands,
) {
    let Some(cef_e) = focused_window.0.and_then(|window| {
        layouts
            .iter()
            .find_map(|(entity, host)| (host.0 == window).then_some(entity))
    }) else {
        return;
    };
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        cef_e,
        &ReloadEffect,
    ));
}

fn on_hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    layouts: Query<(Entity, &HostWindow), (With<LayoutCef>, With<PageReady>)>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    mut commands: Commands,
) {
    let Some(cef_e) = focused_window.0.and_then(|window| {
        layouts
            .iter()
            .find_map(|(entity, host)| (host.0 == window).then_some(entity))
    }) else {
        return;
    };
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        cef_e,
        &ReloadEffect,
    ));
}

fn on_side_sheet_resize(
    trigger: On<UiInput<SideSheetResizeEvent>>,
    mut width: ResMut<SideSheetWidth>,
    mut sheets: Query<(&SideSheetPosition, &mut vmux_flex::prelude::Node), With<SideSheet>>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<Messages<vmux_setting::SettingsSaveRequest>>>,
) {
    let resize = trigger.event().payload;
    let next = resize.clamped();
    if width.0 != next {
        apply_side_sheet_width(&mut width, next, &mut sheets);
    }
    if !resize.settled {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    settings.layout.side_sheet.width = next;
    if let Some(mut saves) = saves {
        saves.write(vmux_setting::SettingsSaveRequest);
    }
}

fn apply_side_sheet_width(
    width: &mut SideSheetWidth,
    next: f32,
    sheets: &mut Query<(&SideSheetPosition, &mut vmux_flex::prelude::Node), With<SideSheet>>,
) {
    width.0 = next;
    for (position, mut node) in sheets {
        if *position == SideSheetPosition::Left {
            node.width = vmux_flex::prelude::Val::Px(next);
        }
    }
}

fn on_side_sheet_stack_activate(
    trigger: On<UiInput<SideSheetStackActivateRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut last_activated: Query<&mut LastActivatedAt>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes
        .iter()
        .find(|entity| entity.to_bits() == request.pane_id)
    else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let target_stack = children
        .iter()
        .find(|&entity| stack_q.contains(entity) && entity.to_bits() == request.stack_id);
    let Some(target_stack) = target_stack else {
        return;
    };
    let activated_at = LastActivatedAt::now();
    for entity in [target_pane, target_stack] {
        if let Ok(mut value) = last_activated.get_mut(entity) {
            *value = activated_at;
        } else {
            commands.entity(entity).insert(activated_at);
        }
    }
    hover_intent.target = None;
    hover_intent.last_activation = Some(std::time::Instant::now());
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn on_side_sheet_stack_close(
    trigger: On<UiInput<SideSheetStackCloseRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    mut requests: MessageWriter<CloseStackRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes
        .iter()
        .find(|entity| entity.to_bits() == request.pane_id)
    else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let target_stack = children
        .iter()
        .find(|&entity| stack_q.contains(entity) && entity.to_bits() == request.stack_id);
    let Some(target_stack) = target_stack else {
        return;
    };
    requests.write(CloseStackRequest::by_user(target_stack));
    hover_intent.target = None;
    hover_intent.last_activation = Some(std::time::Instant::now());
}

fn on_side_sheet_stack_create(
    trigger: On<UiInput<SideSheetStackCreateRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut requests: MessageWriter<vmux_layout::stack::OpenRequest>,
    mut commands: Commands,
) {
    let Some(target_pane) = leaf_panes
        .iter()
        .find(|entity| entity.to_bits() == trigger.event().payload.pane_id)
    else {
        return;
    };
    commands.entity(target_pane).insert(LastActivatedAt::now());
    requests.write(vmux_layout::stack::OpenRequest { url: None });
}

fn on_side_sheet_project_open(
    trigger: On<UiInput<SideSheetProjectOpenRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut requests: MessageWriter<PageOpenRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes
        .iter()
        .find(|entity| entity.to_bits() == request.pane_id)
    else {
        return;
    };
    let Ok(url) = url::Url::from_file_path(&request.path) else {
        return;
    };
    requests.write(PageOpenRequest {
        target: PageOpenTarget::ActiveStackInPane(target_pane),
        url: url.to_string(),
        request_id: None,
    });
}

fn on_side_sheet_section(
    trigger: On<UiInput<SideSheetSectionRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    sections_of: vmux_layout::side_sheet::SideSheetSections,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes
        .iter()
        .find(|entity| entity.to_bits() == request.pane_id)
    else {
        return;
    };
    if request.path == "pane" {
        let mut pane = commands.entity(target_pane);
        if request.expanded {
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
    if !state.set(&request.path, request.expanded) {
        return;
    }
    if state.is_empty() {
        commands.entity(space).remove::<SideSheetSectionsExpanded>();
    } else {
        commands.entity(space).insert(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_core::page::HostHistory;
    use vmux_flex::prelude::{Node, Val};
    use vmux_layout::pane::Pane;
    use vmux_layout::space::{Space, SpaceId};
    use vmux_layout::stack::stack_bundle;
    use vmux_layout::tab::Tab;
    use vmux_setting::AppSettings;

    #[derive(Resource, Default)]
    struct CefNavigations(Vec<Entity>);

    impl CefNavigations {
        fn record_back(trigger: On<RequestGoBack>, mut navigations: ResMut<Self>) {
            navigations.0.push(trigger.webview);
        }
    }

    #[test]
    fn browser_mcp_definitions_are_the_dispatchable_command_set() {
        let definitions = NavigationRequest::definitions()
            .into_iter()
            .chain(OpenRequest::definitions())
            .chain(ZoomRequest::definitions())
            .chain(ShowDevToolsRequest::definitions())
            .collect::<Vec<_>>();
        let tools = definitions
            .iter()
            .filter_map(CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            [
                "browser_prev_page",
                "browser_next_page",
                "browser_reload",
                "browser_hard_reload",
                "browser_stop",
                "open_in_place",
                "browser_zoom_in",
                "browser_zoom_out",
                "browser_zoom_reset",
                "browser_dev_tools",
            ],
        );
        for tool in tools {
            let arguments = match tool.name.as_str() {
                "open_in_place" => serde_json::json!({"url": "https://vmux.ai"}),
                _ => serde_json::json!({}),
            };
            let invocation =
                CommandInvocation::new(Entity::PLACEHOLDER, tool.name).with_arguments(arguments);
            assert!(
                NavigationRequest::try_from(&invocation).is_ok()
                    || OpenRequest::try_from(&invocation).is_ok()
                    || ZoomRequest::try_from(&invocation).is_ok()
                    || ShowDevToolsRequest::try_from(&invocation).is_ok()
            );
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
                .add_message::<PageOpenRequest>()
                .add_message::<vmux_terminal::TerminalFontSizeCommand>()
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

        fn pressed_back(&mut self) {
            self.app.world_mut().trigger(UiInput::<HeaderBackRequest> {
                webview: Entity::PLACEHOLDER,
                payload: HeaderBackRequest,
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
    fn command_invocations_dispatch_to_concrete_requests() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((
            CommandTypePlugin::<NavigationRequest>::default(),
            CommandTypePlugin::<OpenRequest>::default(),
            CommandTypePlugin::<ZoomRequest>::default(),
            CommandTypePlugin::<ShowDevToolsRequest>::default(),
        ));
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write_batch([
                CommandInvocation::new(Entity::PLACEHOLDER, "open_in_place")
                    .with_arguments(serde_json::json!({"url": "https://vmux.ai"})),
                CommandInvocation::new(Entity::PLACEHOLDER, "browser_reload"),
            ]);

        app.update();

        let open_requests = app
            .world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let navigation_requests = app
            .world_mut()
            .resource_mut::<Messages<NavigationRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            open_requests,
            [OpenRequest {
                url: Some("https://vmux.ai".to_string()),
            }]
        );
        assert_eq!(navigation_requests, [NavigationRequest::Reload]);
    }

    #[test]
    fn the_back_arrow_walks_host_history_instead_of_asking_chromium() {
        let mut arrow = NavArrow::over_a_natively_hosted_page();

        arrow.pressed_back();

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

        arrow.pressed_back();

        assert_eq!(arrow.cef_navigations(), vec![arrow.view]);
    }

    #[test]
    fn the_side_sheet_close_button_names_the_stack_it_sits_on() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<vmux_layout::stack::OpenRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PaneHoverIntent>()
            .add_observer(on_side_sheet_stack_close);

        let pane = app.world_mut().spawn(Pane).id();
        let middle = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let active = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt(2), ChildOf(pane)))
            .id();
        let mut cursor = app
            .world()
            .resource::<Messages<CloseStackRequest>>()
            .get_cursor();

        app.world_mut()
            .trigger(UiInput::<SideSheetStackCloseRequest> {
                webview: Entity::PLACEHOLDER,
                payload: SideSheetStackCloseRequest {
                    pane_id: pane.to_bits(),
                    stack_id: middle.to_bits(),
                },
            });
        app.world_mut().flush();

        let requests = app.world().resource::<Messages<CloseStackRequest>>();
        let closed: Vec<Entity> = cursor.read(requests).map(|request| request.stack).collect();
        assert_eq!(
            closed,
            vec![middle],
            "the button closes its own stack, not whichever one is active"
        );
        assert_eq!(
            app.world().get::<LastActivatedAt>(active).unwrap().0,
            2,
            "closing an inactive stack must not disturb activation"
        );
    }

    #[test]
    fn side_sheet_resize_is_live_but_saved_only_when_settled() {
        let mut settings = AppSettings::embedded();
        settings.layout.side_sheet.width = 220.0;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .insert_resource(SideSheetWidth(220.0))
            .add_message::<vmux_setting::SettingsSaveRequest>()
            .add_observer(on_side_sheet_resize);
        let sheet = app
            .world_mut()
            .spawn((SideSheet, SideSheetPosition::Left, Node::default()))
            .id();
        let mut saves = app
            .world()
            .resource::<Messages<vmux_setting::SettingsSaveRequest>>()
            .get_cursor();

        app.world_mut().trigger(UiInput::<SideSheetResizeEvent> {
            webview: Entity::PLACEHOLDER,
            payload: SideSheetResizeEvent::live(320.0),
        });
        app.world_mut().flush();

        assert_eq!(app.world().resource::<SideSheetWidth>().0, 320.0);
        assert_eq!(
            app.world().get::<Node>(sheet).unwrap().width,
            Val::Px(320.0)
        );
        assert_eq!(
            app.world()
                .resource::<AppSettings>()
                .layout
                .side_sheet
                .width,
            220.0
        );
        assert_eq!(
            saves
                .read(
                    app.world()
                        .resource::<Messages<vmux_setting::SettingsSaveRequest>>(),
                )
                .count(),
            0,
        );

        app.world_mut().trigger(UiInput::<SideSheetResizeEvent> {
            webview: Entity::PLACEHOLDER,
            payload: SideSheetResizeEvent::settled(320.0),
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .resource::<AppSettings>()
                .layout
                .side_sheet
                .width,
            320.0
        );
        assert_eq!(
            saves
                .read(
                    app.world()
                        .resource::<Messages<vmux_setting::SettingsSaveRequest>>(),
                )
                .count(),
            1,
        );
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
                .add_message::<vmux_layout::stack::OpenRequest>()
                .add_message::<PageOpenRequest>()
                .init_resource::<PaneHoverIntent>()
                .add_observer(on_side_sheet_section);

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
                .trigger(UiInput::<SideSheetSectionRequest> {
                    webview: Entity::PLACEHOLDER,
                    payload: SideSheetSectionRequest {
                        pane_id: self.pane_in_first_tab.to_bits(),
                        path: section.to_string(),
                        expanded: true,
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
        spaces.expand("bookmarks");

        let second_tab = spaces.second_tab;
        assert!(
            spaces.sections_under(second_tab).bookmarks,
            "a card is expanded for the whole space, so switching tab must not fold it"
        );
    }

    #[test]
    fn expanding_a_card_leaves_the_other_spaces_alone() {
        let mut spaces = SideSheetSpaces::start();
        spaces.expand("bookmarks");

        let elsewhere = spaces.tab_in_other_space;
        assert!(!spaces.sections_under(elsewhere).bookmarks);
    }
}
