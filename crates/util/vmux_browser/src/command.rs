use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_api::VmuxRoute;
use vmux_api::command_bar::{
    InvokeRequest, OpenRequest as CommandBarPageOpenRequest, SearchEngine,
};
#[cfg(test)]
use vmux_command::CommandDefinition;
#[cfg(test)]
use vmux_command::CommandManifest;
use vmux_command::command_bar::CommandBarDismiss;
use vmux_command::snapshot::{
    ClaimedUrls, CommandBarProjection, ContributedCommand, ContributedPages,
};
use vmux_command::{CommandInvocation, CommandRegistry, ReadCommandRequests, ResolvedLocale};
#[cfg(test)]
use vmux_core::host::manifest::FeaturePlugin;
use vmux_core::launcher::{HostsLauncher, InlineTransitionRequested};
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
    Header, LayoutCef, ReloadRevision,
    event::{
        HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
    },
    pane::{Pane, PaneHoverCooldown, PaneSplit, SideSheetCardCollapsed},
    side_sheet::{SideSheet, SideSheetPaneExpanded, SideSheetPosition, SideSheetSectionsExpanded},
    stack::{CloseStackRequest, FocusedStack, Stack},
    state::LayoutUiState,
};

use vmux_core::terminal::{TerminalSpawnRequest, TerminalSpawnTarget};
use vmux_setting::SearchEngineSetting;
use vmux_terminal::{RestartPty, Terminal};
use vmux_ui::i18n::{Locale, TranslationValue};

pub(crate) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.add_message::<NavigationRequest>()
            .add_message::<OpenRequest>()
            .add_message::<ZoomRequest>()
            .add_message::<ShowDevToolsRequest>()
            .add_plugins(UiEventPlugin::<(CommandBarPageOpenRequest, InvokeRequest)>::default())
            .add_systems(Startup, bind_commands.in_set(vmux_command::BindCommands))
            .add_observer(header_back)
            .add_observer(header_forward)
            .add_observer(header_reload)
            .add_observer(header_address_focus)
            .add_observer(side_sheet_stack_activate)
            .add_observer(side_sheet_stack_close)
            .add_observer(side_sheet_stack_create)
            .add_observer(side_sheet_project_open)
            .add_observer(side_sheet_section)
            .add_observer(side_sheet_resize)
            .add_observer(open_from_bar)
            .add_observer(invoke_from_bar)
            .add_observer(reload_notify_header)
            .add_observer(hard_reload_notify_header)
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

struct PageOpenCommand;

impl PageOpenCommand {
    fn for_target(
        caller: Entity,
        target: Option<vmux_api::open_target::OpenTarget>,
        url: String,
    ) -> CommandInvocation {
        use vmux_api::open_target::{OpenTarget, PaneDirection};

        let (id, arguments) = match target {
            Some(OpenTarget::InPlace) | None => {
                ("open_in_place", serde_json::json!({ "url": url }))
            }
            Some(OpenTarget::InNewStack) => {
                ("open_in_new_stack", serde_json::json!({ "url": url }))
            }
            Some(OpenTarget::InPane {
                direction,
                target,
                mode,
            }) => (
                match direction {
                    PaneDirection::Top => "open_in_pane_top",
                    PaneDirection::Right => "open_in_pane_right",
                    PaneDirection::Bottom => "open_in_pane_bottom",
                    PaneDirection::Left => "open_in_pane_left",
                },
                serde_json::json!({ "url": url, "target": target, "mode": mode }),
            ),
            Some(OpenTarget::InNewTab) => ("open_in_new_tab", serde_json::json!({ "url": url })),
            Some(OpenTarget::InNewSpace) => {
                ("open_in_new_space", serde_json::json!({ "url": url }))
            }
        };
        CommandInvocation::new(caller, id).with_arguments(arguments)
    }
}

struct Home;

impl Home {
    fn resolve(value: &str) -> std::path::PathBuf {
        let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
        if let Some(rest) = value.strip_prefix('~') {
            return match home {
                Some(home) => home.join(rest.trim_start_matches('/')),
                None => std::path::PathBuf::from(value),
            };
        }
        if value.starts_with('/') {
            return std::path::PathBuf::from(value);
        }
        match home {
            Some(home) => home.join(value),
            None => std::path::PathBuf::from(value),
        }
    }

    fn expanded_file_url(value: &str) -> String {
        let Some(path) = value.strip_prefix("file://") else {
            return value.to_string();
        };
        if !path.starts_with('~') {
            return value.to_string();
        }
        format!("file://{}", Self::resolve(path).display())
    }
}

fn normalize_url(value: &str, search_engine: SearchEngine) -> String {
    let value = value.trim();
    let query = vmux_core::input::NavigationText::new(value);
    if query.is_data_uri() || (value.contains("://") && query.looks_like_url()) {
        value.to_string()
    } else if query.looks_like_url() {
        format!("https://{value}")
    } else {
        search_engine.query_url(value)
    }
}

fn open_from_bar(
    trigger: On<UiInput<CommandBarPageOpenRequest>>,
    search_engine: Option<Single<&SearchEngineSetting>>,
    child_of: Query<&ChildOf>,
    launcher_hosts: Query<(), With<HostsLauncher>>,
    claimed_urls: ClaimedUrls,
    command_bar: Single<&CommandBarProjection>,
    locale: Option<Res<ResolvedLocale>>,
    mut terminal_spawn_requests: MessageWriter<TerminalSpawnRequest>,
    mut chosen_writer: MessageWriter<vmux_core::ContributedCommandChosen>,
    mut inline_transition: MessageWriter<InlineTransitionRequested>,
    mut command_invocations: MessageWriter<CommandInvocation>,
    users: Query<Entity, With<vmux_core::team::User>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let focus = &command_bar.workspace;
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    let caller = users.single().unwrap_or(Entity::PLACEHOLDER);
    let mut custom_keyboard_restore = false;
    let inline_stack = launcher_hosts
        .contains(webview)
        .then(|| child_of.get(webview).ok().map(|parent| parent.0))
        .flatten();
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    let value = Home::expanded_file_url(&request.value);
    let expanded = Home::resolve(&value);
    if expanded.exists() {
        let directory = if expanded.is_dir() {
            &expanded
        } else {
            expanded.parent().unwrap_or(&expanded)
        };
        if let Some(pane) = focus.pane {
            terminal_spawn_requests.write(TerminalSpawnRequest {
                cwd: Some(directory.to_path_buf()),
                target: TerminalSpawnTarget::NewStackInPane(pane),
                metadata: Some(PageMetadata {
                    url: vmux_terminal::TerminalPlugin::URL.to_string(),
                    title: locale.translate_with(
                        "command-terminal-path",
                        &[(
                            "path",
                            TranslationValue::String(&directory.display().to_string()),
                        )],
                    ),
                    ..default()
                }),
            });
            custom_keyboard_restore = true;
        }
    } else {
        let url = normalize_url(
            &value,
            search_engine.map(|setting| setting.0).unwrap_or_default(),
        );
        let inline_transitioned = if matches!(
            request.open,
            None | Some(vmux_api::open_target::OpenTarget::InPlace)
        ) && VmuxRoute::parse(&url)
            .is_some_and(|route| route.supports_inline_transition())
            && let Some(stack) = inline_stack
        {
            inline_transition.write(InlineTransitionRequested { stack, webview });
            if let Some(proxy) = proxy.as_deref() {
                let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
            }
            true
        } else {
            false
        };
        if !inline_transitioned && claimed_urls.contains(&url) {
            if let Some(pane) = focus.pane {
                chosen_writer.write(vmux_core::ContributedCommandChosen {
                    id: url,
                    stack: None,
                    pane: Some(pane),
                });
                custom_keyboard_restore = true;
            }
        } else {
            command_invocations.write(PageOpenCommand::for_target(caller, request.open, url));
        }
    }
    commands.trigger(CommandBarDismiss::new(webview, !custom_keyboard_restore));
}

fn invoke_from_bar(
    trigger: On<UiInput<InvokeRequest>>,
    contributed_pages: ContributedPages,
    contributed_commands: Query<&ContributedCommand>,
    command_bar: Single<&CommandBarProjection>,
    users: Query<Entity, With<vmux_core::team::User>>,
    mut chosen: MessageWriter<vmux_core::ContributedCommandChosen>,
    mut invocations: MessageWriter<CommandInvocation>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    let caller = users.single().unwrap_or(Entity::PLACEHOLDER);
    let mut custom_keyboard_restore = false;
    if contributed_commands
        .iter()
        .any(|command| command.id == request.id)
    {
        if let Some(pane) = command_bar.workspace.pane {
            chosen.write(vmux_core::ContributedCommandChosen {
                id: request.id.clone(),
                stack: None,
                pane: Some(pane),
            });
            custom_keyboard_restore = true;
        }
    } else if let Some(url) = contributed_pages.page_url(&request.id) {
        invocations.write(PageOpenCommand::for_target(caller, request.open, url));
        custom_keyboard_restore = true;
    } else {
        invocations.write(CommandInvocation::new(caller, &request.id));
    }
    commands.trigger(CommandBarDismiss::new(webview, !custom_keyboard_restore));
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationRequest {
    Back,
    Forward,
    Reload,
    HardReload,
    Stop,
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

impl TryFrom<&CommandInvocation> for ShowDevToolsRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "browser_dev_tools")
            .then_some(Self)
            .ok_or(())
    }
}

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.message::<NavigationRequest>(&mut commands);
    registry.message::<OpenRequest>(&mut commands);
    registry.message::<ZoomRequest>(&mut commands);
    registry.message::<ShowDevToolsRequest>(&mut commands);
}

fn handle_navigation_requests(
    mut navigation_requests: MessageReader<NavigationRequest>,
    focus: FocusedStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    host_histories: Query<(), With<HostHistory>>,
    mut host_history_steps: MessageWriter<HostHistoryStep>,
    mut commands: Commands,
) {
    for request in navigation_requests.read() {
        let Some(active) = focus.stack else {
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

#[derive(SystemParam)]
struct BrowserOpen<'w, 's> {
    focus: FocusedStack<'w, 's>,
    browsers: Query<
        'w,
        's,
        (Entity, &'static ChildOf),
        (With<Browser>, Without<Header>, Without<SideSheet>),
    >,
    kinds: Query<'w, 's, (Has<Terminal>, Has<vmux_editor::FileView>)>,
    focused_space: vmux_layout::space::FocusedSpace<'w, 's>,
    native_pages: Query<'w, 's, &'static NativelyHosted>,
    host_spawn_routes: Query<'w, 's, &'static HostSpawnRoute>,
    metadata: Query<'w, 's, &'static mut PageMetadata, With<Browser>>,
}

impl BrowserOpen<'_, '_> {
    fn target(&self) -> Option<(Entity, Entity, bool)> {
        let stack = self.focus.stack?;
        let webview = self
            .browsers
            .iter()
            .find_map(|(entity, child_of)| (child_of.get() == stack).then_some(entity))?;
        let is_terminal = self.kinds.get(webview).is_ok_and(|kind| kind.0);
        Some((stack, webview, is_terminal))
    }

    fn hosted(&self, url: &str) -> bool {
        self.native_pages.iter().any(|page| page.answers_for(url))
            || self
                .host_spawn_routes
                .iter()
                .any(|route| route.answers_for(url))
    }
}

fn handle_open_requests(
    mut open_requests: MessageReader<OpenRequest>,
    mut browser: BrowserOpen,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for request in open_requests.read() {
        let Some((active, webview, is_terminal)) = browser.target() else {
            continue;
        };
        let resolved = request.resolved_url(browser.focused_space.startup_url());
        if resolved.is_empty() {
            continue;
        }
        let resolved =
            VmuxRoute::canonical(&resolved).unwrap_or_else(|| resolved.trim().to_string());
        let current_url = browser
            .metadata
            .get(webview)
            .map(|metadata| metadata.url.clone())
            .unwrap_or_default();
        let current_is_hosted = browser.hosted(&current_url);
        let resolved_is_hosted = browser.hosted(&resolved);
        if is_terminal || current_is_hosted || resolved_is_hosted {
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(active),
                url: resolved,
                request_id: None,
            });
            continue;
        }
        if let Ok(mut metadata) = browser.metadata.get_mut(webview) {
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
    focus: FocusedStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut font_size_writer: MessageWriter<vmux_terminal::TerminalFontSizeCommand>,
) {
    for request in requests.read() {
        let Some(active) = focus.stack else {
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
    focus: FocusedStack,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    for _ in requests.read() {
        let Some(active) = focus.stack else {
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

fn header_back(
    trigger: On<UiInput<HeaderBackRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_prev_page",
    ));
}

fn header_forward(
    trigger: On<UiInput<HeaderForwardRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_next_page",
    ));
}

fn header_reload(
    trigger: On<UiInput<HeaderReloadRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_reload",
    ));
}

fn header_address_focus(
    trigger: On<UiInput<HeaderAddressFocusRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_open_page_in_command_bar",
    ));
}

fn reload_notify_header(
    _trigger: On<RequestReload>,
    mut layouts: Query<
        (Entity, &HostWindow, &mut ReloadRevision),
        (With<LayoutCef>, With<PageReady>),
    >,
    focused_window: vmux_layout::window::FocusedWindow,
    mut commands: Commands,
) {
    let Some((cef_e, mut revision)) = focused_window.entity().and_then(|window| {
        layouts
            .iter_mut()
            .find_map(|(entity, host, revision)| (host.0 == window).then_some((entity, revision)))
    }) else {
        return;
    };
    let effect = revision.next_effect();
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &effect));
}

fn hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    mut layouts: Query<
        (Entity, &HostWindow, &mut ReloadRevision),
        (With<LayoutCef>, With<PageReady>),
    >,
    focused_window: vmux_layout::window::FocusedWindow,
    mut commands: Commands,
) {
    let Some((cef_e, mut revision)) = focused_window.entity().and_then(|window| {
        layouts
            .iter_mut()
            .find_map(|(entity, host, revision)| (host.0 == window).then_some((entity, revision)))
    }) else {
        return;
    };
    let effect = revision.next_effect();
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &effect));
}

fn side_sheet_resize(
    trigger: On<UiInput<SideSheetResizeEvent>>,
    mut sheets: Query<(&SideSheetPosition, &mut vmux_flex::prelude::Node), With<SideSheet>>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<MessageWriter<vmux_setting::SettingsSaveRequest>>,
) {
    let resize = trigger.event().payload;
    let next = resize.clamped();
    for (position, mut node) in &mut sheets {
        if *position == SideSheetPosition::Left && node.width != vmux_flex::prelude::Val::Px(next) {
            node.width = vmux_flex::prelude::Val::Px(next);
        }
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

fn side_sheet_stack_activate(
    trigger: On<UiInput<SideSheetStackActivateRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut last_activated: Query<&mut LastActivatedAt>,
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
    commands
        .entity(target_pane)
        .insert(PaneHoverCooldown::start());
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn side_sheet_stack_close(
    trigger: On<UiInput<SideSheetStackCloseRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut requests: MessageWriter<CloseStackRequest>,
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
    requests.write(CloseStackRequest::by_user(target_stack));
    commands
        .entity(target_pane)
        .insert(PaneHoverCooldown::start());
}

fn side_sheet_stack_create(
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

fn side_sheet_project_open(
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

fn side_sheet_section(
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
        let definitions = CommandManifest::for_feature::<crate::Feature>();
        let mut definitions = definitions.into_vec();
        definitions.retain(|definition| definition.id != "browser_open_history");
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
        app.add_plugins((
            MinimalPlugins,
            FeaturePlugin::<crate::Feature>::default(),
            vmux_command::CommandRuntimePlugin,
        ))
        .add_message::<NavigationRequest>()
        .add_message::<OpenRequest>()
        .add_message::<ZoomRequest>()
        .add_message::<ShowDevToolsRequest>()
        .add_systems(Startup, bind_commands.in_set(vmux_command::BindCommands));
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
            .add_observer(side_sheet_stack_close);

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
            .add_message::<vmux_setting::SettingsSaveRequest>()
            .add_observer(side_sheet_resize);
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
                .add_observer(side_sheet_section);

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
