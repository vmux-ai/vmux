use bevy::{
    ecs::{message::Messages, relationship::Relationship, system::SystemParam},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_api::VmuxRoute;
use vmux_command::{
    CommandDefinition, CommandInvocation, CommandMcp, InputSchema, ReadCommandRequests,
};
use vmux_core::{
    HostSpawnRegistry, PageMetadata, PageOpenRequest, PageOpenTarget,
    page::{HostHistoryDelta, HostHistoryNavigation, PageReady},
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::event::{SideSheetRequest, SideSheetResizeEvent};
use vmux_layout::{
    Header, LayoutCef,
    event::{HeaderRequest, ReloadEvent},
    pane::{Pane, PaneHoverIntent, PaneSplit, SideSheetCardCollapsed},
    side_sheet::{
        SideSheet, SideSheetPaneExpanded, SideSheetPosition, SideSheetSectionsExpanded,
        SideSheetWidth,
    },
    stack::{ActiveTabParam, CloseStackRequest, Stack, focused_stack},
};

use vmux_terminal::{RestartPty, Terminal};

pub(crate) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        BrowserRequest::register(app);
        app.add_message::<NavigationRequest>()
            .add_message::<OpenRequest>()
            .add_message::<ViewRequest>()
            .add_observer(on_header_request)
            .add_observer(on_side_sheet_request)
            .add_observer(on_side_sheet_resize)
            .add_observer(on_reload_notify_header)
            .add_observer(on_hard_reload_notify_header)
            .add_systems(Update, handle_browser_commands.in_set(ReadCommandRequests));
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

impl NavigationRequest {
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

    fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        let request = match invocation.id.as_str() {
            "browser_prev_page" => Self::Back,
            "browser_next_page" => Self::Forward,
            "browser_reload" => Self::Reload,
            "browser_hard_reload" => Self::HardReload,
            "browser_stop" => Self::Stop,
            _ => return None,
        };
        Some(request)
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

impl OpenRequest {
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

    fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        (invocation.id == "open_in_place").then(|| Self {
            url: invocation.argument("url"),
        })
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRequest {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    DevTools,
    ViewSource,
    Print,
}

impl ViewRequest {
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
            CommandDefinition::new("browser_dev_tools", "Developer Tools", "Browser > View")
                .accelerator("super+alt+i")
                .mcp(CommandMcp::new("Developer Tools", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_view_source", "View Source", "Browser > View")
                .accelerator("super+alt+u")
                .hidden()
                .mcp(CommandMcp::new("View Source", InputSchema::object()).allow_agent()),
            CommandDefinition::new("browser_print", "Print", "Browser > View")
                .hidden()
                .mcp(CommandMcp::new("Print", InputSchema::object()).allow_agent()),
        ]
    }

    fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        let request = match invocation.id.as_str() {
            "browser_zoom_in" => Self::ZoomIn,
            "browser_zoom_out" => Self::ZoomOut,
            "browser_zoom_reset" => Self::ZoomReset,
            "browser_dev_tools" => Self::DevTools,
            "browser_view_source" => Self::ViewSource,
            "browser_print" => Self::Print,
            _ => return None,
        };
        Some(request)
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
enum BrowserRequest {
    Navigate(NavigationRequest),
    Open(OpenRequest),
    View(ViewRequest),
}

impl BrowserRequest {
    fn register(app: &mut App) {
        CommandDefinition::register(app, Self::definitions, Self::from_invocation);
    }

    fn definitions() -> Vec<CommandDefinition> {
        let mut definitions = NavigationRequest::definitions();
        definitions.extend(OpenRequest::definitions());
        definitions.extend(ViewRequest::definitions());
        definitions
    }

    fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        if let Some(request) = NavigationRequest::from_invocation(invocation) {
            return Some(Self::Navigate(request));
        }
        if let Some(request) = OpenRequest::from_invocation(invocation) {
            return Some(Self::Open(request));
        }
        ViewRequest::from_invocation(invocation).map(Self::View)
    }
}

fn handle_browser_commands(
    mut command_requests: MessageReader<BrowserRequest>,
    mut navigation_requests: MessageReader<NavigationRequest>,
    mut open_requests: MessageReader<OpenRequest>,
    mut view_requests: MessageReader<ViewRequest>,
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
    let mut requests = command_requests.read().cloned().collect::<Vec<_>>();
    requests.extend(
        navigation_requests
            .read()
            .copied()
            .map(BrowserRequest::Navigate),
    );
    requests.extend(open_requests.read().cloned().map(BrowserRequest::Open));
    requests.extend(view_requests.read().copied().map(BrowserRequest::View));

    for request in requests {
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
        match request {
            BrowserRequest::Navigate(request) => match request {
                NavigationRequest::Back => {
                    if is_terminal || host_history.stepped(webview, HostHistoryDelta::Back) {
                        continue;
                    }
                    commands.trigger(RequestGoBack { webview });
                }
                NavigationRequest::Forward => {
                    if is_terminal || host_history.stepped(webview, HostHistoryDelta::Forward) {
                        continue;
                    }
                    commands.trigger(RequestGoForward { webview });
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
            },
            BrowserRequest::Open(request) => {
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
            BrowserRequest::View(request) => match request {
                ViewRequest::ZoomIn => {
                    if is_text_grid {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Increase);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 += 0.5;
                    }
                }
                ViewRequest::ZoomOut => {
                    if is_text_grid {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Decrease);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 -= 0.5;
                    }
                }
                ViewRequest::ZoomReset => {
                    if is_text_grid {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Reset);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 = 0.0;
                    }
                }
                ViewRequest::DevTools => {
                    commands.trigger(RequestShowDevTool { webview });
                }
                ViewRequest::ViewSource => {}
                ViewRequest::Print => {}
            },
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

fn on_header_request(
    trigger: On<BinReceive<HeaderRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    let id = match trigger.event().payload {
        HeaderRequest::PreviousPage => "browser_prev_page",
        HeaderRequest::NextPage => "browser_next_page",
        HeaderRequest::Reload => "browser_reload",
        HeaderRequest::FocusAddressBar => "browser_open_page_in_command_bar",
    };
    command_invocations.write(CommandInvocation::new(trigger.event().webview, id));
}

fn on_reload_notify_header(
    _trigger: On<RequestReload>,
    layouts: Query<(Entity, &HostWindow), (With<LayoutCef>, With<PageReady>)>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef_e) = focused_window.0.and_then(|window| {
        layouts
            .iter()
            .find_map(|(entity, host)| (host.0 == window).then_some(entity))
    }) else {
        return;
    };
    if browsers.can_emit_to(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_event(cef_e, &ReloadEvent));
    }
}

fn on_hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    layouts: Query<(Entity, &HostWindow), (With<LayoutCef>, With<PageReady>)>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef_e) = focused_window.0.and_then(|window| {
        layouts
            .iter()
            .find_map(|(entity, host)| (host.0 == window).then_some(entity))
    }) else {
        return;
    };
    if browsers.can_emit_to(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_event(cef_e, &ReloadEvent));
    }
}

fn on_side_sheet_resize(
    trigger: On<BinReceive<SideSheetResizeEvent>>,
    mut width: ResMut<SideSheetWidth>,
    mut sheets: Query<(&SideSheetPosition, &mut vmux_flex::prelude::Node), With<SideSheet>>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<Messages<vmux_setting::SettingsSaveRequest>>>,
) {
    let resize = trigger.event().payload;
    let next = resize.clamped();
    if width.0 != next {
        width.apply(next, &mut sheets);
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

fn on_side_sheet_request(
    trigger: On<BinReceive<SideSheetRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut activation: StackActivation,
    sections_of: vmux_layout::side_sheet::SideSheetSections,
    mut hover_intent: ResMut<PaneHoverIntent>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut stack_requests: MessageWriter<vmux_layout::stack::StackRequest>,
    mut close_stack_requests: MessageWriter<CloseStackRequest>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let pane_id = match evt {
        SideSheetRequest::ActivateStack { pane_id, .. }
        | SideSheetRequest::CloseStack { pane_id, .. }
        | SideSheetRequest::NewStack { pane_id }
        | SideSheetRequest::OpenProjectPath { pane_id, .. }
        | SideSheetRequest::CollapseCard { pane_id }
        | SideSheetRequest::ExpandCard { pane_id }
        | SideSheetRequest::CollapseSection { pane_id, .. }
        | SideSheetRequest::ExpandSection { pane_id, .. } => *pane_id,
    };
    let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id) else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    match evt {
        SideSheetRequest::ActivateStack { stack_id, .. } => {
            let target_stack = children
                .iter()
                .find(|&entity| stack_q.contains(entity) && entity.to_bits() == *stack_id);
            let Some(target_stack) = target_stack else {
                return;
            };
            activation.activate(target_pane, target_stack, &mut commands);

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
        }
        SideSheetRequest::CloseStack { stack_id, .. } => {
            let target_stack = children
                .iter()
                .find(|&entity| stack_q.contains(entity) && entity.to_bits() == *stack_id);
            let Some(target_stack) = target_stack else {
                return;
            };
            close_stack_requests.write(CloseStackRequest::by_user(target_stack));
            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
        }
        SideSheetRequest::NewStack { .. } => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            stack_requests.write(vmux_layout::stack::StackRequest::Open { url: None });
        }
        SideSheetRequest::OpenProjectPath { path, .. } => {
            let Ok(url) = url::Url::from_file_path(path) else {
                return;
            };
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::ActiveStackInPane(target_pane),
                url: url.to_string(),
                request_id: None,
            });
        }
        SideSheetRequest::CollapseCard { .. } => {
            commands
                .entity(target_pane)
                .insert(SideSheetCardCollapsed)
                .remove::<SideSheetPaneExpanded>();
        }
        SideSheetRequest::ExpandCard { .. } => {
            commands
                .entity(target_pane)
                .remove::<SideSheetCardCollapsed>()
                .remove::<SideSheetPaneExpanded>();
        }
        SideSheetRequest::CollapseSection { path, .. }
        | SideSheetRequest::ExpandSection { path, .. } => {
            let expanded = matches!(evt, SideSheetRequest::ExpandSection { .. });
            if path == "pane" {
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
            if !state.set(path, expanded) {
                return;
            }
            if state.is_empty() {
                commands.entity(space).remove::<SideSheetSectionsExpanded>();
            } else {
                commands.entity(space).insert(state);
            }
        }
    }
}

#[derive(SystemParam)]
struct StackActivation<'w, 's> {
    last_activated: Query<'w, 's, &'static mut LastActivatedAt>,
}

impl StackActivation<'_, '_> {
    fn activate(&mut self, pane: Entity, stack: Entity, commands: &mut Commands) {
        let at = LastActivatedAt::now();
        self.stamp(pane, at, commands);
        self.stamp(stack, at, commands);
    }

    fn stamp(&mut self, entity: Entity, at: LastActivatedAt, commands: &mut Commands) {
        if let Ok(mut value) = self.last_activated.get_mut(entity) {
            *value = at;
            return;
        }
        commands.entity(entity).insert(at);
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
        let definitions = BrowserRequest::definitions();
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
                "browser_view_source",
                "browser_print",
            ],
        );
        for tool in tools {
            let arguments = match tool.name.as_str() {
                "open_in_place" => serde_json::json!({"url": "https://vmux.ai"}),
                _ => serde_json::json!({}),
            };
            let invocation =
                CommandInvocation::new(Entity::PLACEHOLDER, tool.name).with_arguments(arguments);
            assert!(BrowserRequest::from_invocation(&invocation).is_some());
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

        fn pressed(&mut self, request: HeaderRequest) {
            self.app.world_mut().trigger(BinReceive::<HeaderRequest> {
                webview: Entity::PLACEHOLDER,
                payload: request,
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
    fn command_invocations_keep_their_original_order() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        BrowserRequest::register(&mut app);
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write_batch([
                CommandInvocation::new(Entity::PLACEHOLDER, "open_in_place")
                    .with_arguments(serde_json::json!({"url": "https://vmux.ai"})),
                CommandInvocation::new(Entity::PLACEHOLDER, "browser_reload"),
            ]);

        app.update();

        let requests = app
            .world_mut()
            .resource_mut::<Messages<BrowserRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            requests,
            [
                BrowserRequest::Open(OpenRequest {
                    url: Some("https://vmux.ai".to_string()),
                }),
                BrowserRequest::Navigate(NavigationRequest::Reload),
            ]
        );
    }

    #[test]
    fn the_back_arrow_walks_host_history_instead_of_asking_chromium() {
        let mut arrow = NavArrow::over_a_natively_hosted_page();

        arrow.pressed(HeaderRequest::PreviousPage);

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

        arrow.pressed(HeaderRequest::PreviousPage);

        assert_eq!(arrow.cef_navigations(), vec![arrow.view]);
    }

    #[test]
    fn the_side_sheet_close_button_names_the_stack_it_sits_on() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<vmux_layout::stack::StackRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PaneHoverIntent>()
            .add_observer(on_side_sheet_request);

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

        app.world_mut().trigger(BinReceive::<SideSheetRequest> {
            webview: Entity::PLACEHOLDER,
            payload: SideSheetRequest::CloseStack {
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

        app.world_mut().trigger(BinReceive::<SideSheetResizeEvent> {
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

        app.world_mut().trigger(BinReceive::<SideSheetResizeEvent> {
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
                .add_message::<vmux_layout::stack::StackRequest>()
                .add_message::<PageOpenRequest>()
                .init_resource::<PaneHoverIntent>()
                .add_observer(on_side_sheet_request);

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
                .trigger(BinReceive::<SideSheetRequest> {
                    webview: Entity::PLACEHOLDER,
                    payload: SideSheetRequest::ExpandSection {
                        pane_id: self.pane_in_first_tab.to_bits(),
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
