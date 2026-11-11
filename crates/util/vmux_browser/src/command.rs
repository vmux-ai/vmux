use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_api::VmuxRoute;
use vmux_api::command_bar::{
    InvokeRequest, OpenRequest as CommandBarPageOpenRequest, SearchEngine,
};
use vmux_command::CommandBarDismiss;
#[cfg(test)]
use vmux_command::CommandDefinition;
#[cfg(test)]
use vmux_command::CommandManifest;
use vmux_command::{
    ClaimedUrls, CommandBarWorkspaceSnapshot, ContributedCommand, ContributedPages,
};
use vmux_command::{CommandInvocation, ReadCommandRequests, ResolvedLocale};
use vmux_ecs::launcher::{HostsLauncher, InlineTransitionRequested};
use vmux_ecs::manifest::FeatureManifest;
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;
#[cfg(test)]
use vmux_ecs::page::HostHistoryObserve;
use vmux_ecs::{
    Active, PageMetadata, PageOpenRequest, PageOpenTarget, UiStateWrite,
    page::{HostHistoryDelta, HostHistoryStep, PageReady},
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::event::{
    SideSheetProjectOpenRequest, SideSheetProjectPageOpenRequest, SideSheetResizeEvent,
    SideSheetSectionRequest, SideSheetStackActivateRequest, SideSheetStackCloseRequest,
    SideSheetStackCreateRequest,
};
#[cfg(test)]
use vmux_layout::space::CurrentSpace;
use vmux_layout::{
    LayoutCef, ReloadRevision,
    event::{
        HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
        HeaderStopRequest,
    },
    pane::{Pane, PaneHoverCooldown, PaneSplit, SideSheetCardCollapsed},
    presentation::Retiring,
    side_sheet::{SideSheet, SideSheetPaneExpanded, SideSheetPosition, SideSheetSectionsExpanded},
    space::{Space, SpaceHierarchy},
    stack::{CloseStackRequest, FocusedStack, LayoutFocus, Stack},
    state::LayoutUiState,
    tab::Tab,
    window::{WindowHierarchy, WindowScope},
};

use vmux_ecs::terminal::{TerminalSpawnRequest, TerminalSpawnTarget};
use vmux_setting::SearchEngineSetting;
use vmux_terminal::{RestartPty, Terminal};
use vmux_ui::i18n::{Locale, TranslationValue};

use vmux_api::open_target::{OpenTarget, PaneDirection};

pub(crate) struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>() {
            app.add_plugins(vmux_ecs::PrimitivesPlugin);
        }
        app.add_message::<HostHistoryStep>()
            .add_message::<NavigationRequest>()
            .add_message::<OpenRequest>()
            .add_message::<ZoomRequest>()
            .add_message::<ShowDevToolsRequest>()
            .add_plugins(UiEventPlugin::<(CommandBarPageOpenRequest, InvokeRequest)>::default())
            .add_observer(header_back)
            .add_observer(header_forward)
            .add_observer(header_reload)
            .add_observer(header_stop)
            .add_observer(header_address_focus)
            .add_observer(side_sheet_stack_activate)
            .add_observer(side_sheet_stack_close)
            .add_observer(side_sheet_stack_create)
            .add_observer(side_sheet_project_open)
            .add_observer(side_sheet_project_page_open)
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
    fn for_target(caller: Entity, target: Option<OpenTarget>, url: String) -> CommandInvocation {
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

#[derive(SystemParam)]
struct SearchEngines<'w, 's> {
    manifests: Query<'w, 's, &'static FeatureManifest>,
}

impl SearchEngines<'_, '_> {
    fn resolve(&self, id: &str) -> Option<SearchEngine> {
        for manifest in &self.manifests {
            for engine in &manifest.search_engines {
                if engine.id != id {
                    continue;
                }
                return Some(SearchEngine {
                    id: engine.id.clone(),
                    name: engine.name.clone(),
                    hosts: engine.hosts.clone(),
                    query_url: engine.query_url.clone(),
                });
            }
        }
        None
    }
}

fn normalize_url(value: &str, search_engine: Option<&SearchEngine>) -> String {
    let value = value.trim();
    let query = vmux_path::NavigationText::new(value);
    if query.is_data_uri() || (value.contains("://") && query.looks_like_url()) {
        value.to_string()
    } else if query.looks_like_url() {
        format!("https://{value}")
    } else if let Some(search_engine) = search_engine {
        search_engine.query_url(value)
    } else {
        value.to_string()
    }
}

fn open_from_bar(
    trigger: On<UiInput<CommandBarPageOpenRequest>>,
    search_engine: Option<Single<&SearchEngineSetting>>,
    search_engines: SearchEngines,
    child_of: Query<&ChildOf>,
    launcher_hosts: Query<(), With<HostsLauncher>>,
    claimed_urls: ClaimedUrls,
    workspace: Single<&CommandBarWorkspaceSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    mut terminal_spawn_requests: MessageWriter<TerminalSpawnRequest>,
    mut chosen_writer: MessageWriter<vmux_ecs::ContributedCommandChosen>,
    mut inline_transition: MessageWriter<InlineTransitionRequested>,
    mut command_invocations: MessageWriter<CommandInvocation>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let focus = &*workspace;
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
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
        let search_engine = search_engine
            .map(|setting| setting.0.as_str())
            .unwrap_or("google");
        let search_engine = search_engines.resolve(search_engine);
        let url = normalize_url(&value, search_engine.as_ref());
        let inline_transitioned = if matches!(request.open, None | Some(OpenTarget::InPlace))
            && VmuxRoute::parse(&url).is_some_and(|route| route.supports_inline_transition())
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
                chosen_writer.write(vmux_ecs::ContributedCommandChosen {
                    id: url,
                    stack: None,
                    pane: Some(pane),
                });
                custom_keyboard_restore = true;
            }
        } else {
            command_invocations.write(PageOpenCommand::for_target(webview, request.open, url));
        }
    }
    commands.trigger(CommandBarDismiss::new(webview, !custom_keyboard_restore));
}

fn invoke_from_bar(
    trigger: On<UiInput<InvokeRequest>>,
    contributed_pages: ContributedPages,
    contributed_commands: Query<&ContributedCommand>,
    workspace: Single<&CommandBarWorkspaceSnapshot>,
    users: Query<Entity, With<vmux_ecs::team::User>>,
    mut chosen: MessageWriter<vmux_ecs::ContributedCommandChosen>,
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
        if let Some(pane) = workspace.pane {
            chosen.write(vmux_ecs::ContributedCommandChosen {
                id: request.id.clone(),
                stack: None,
                pane: Some(pane),
            });
            custom_keyboard_restore = true;
        }
    } else if let Some(url) = contributed_pages.page_url(&request.id) {
        invocations.write(PageOpenCommand::for_target(webview, request.open, url));
        custom_keyboard_restore = true;
    } else {
        invocations.write(CommandInvocation::new(caller, &request.id));
    }
    commands.trigger(CommandBarDismiss::new(webview, !custom_keyboard_restore));
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationRequest {
    target: NavigationTarget,
    operation: NavigationOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavigationTarget {
    Origin(Entity),
    Surface(Entity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationOperation {
    Back,
    Forward,
    Reload,
    HardReload,
    Stop,
}

impl NavigationRequest {
    pub fn focused(operation: NavigationOperation) -> Self {
        Self {
            target: NavigationTarget::Origin(Entity::PLACEHOLDER),
            operation,
        }
    }

    pub fn surface(webview: Entity, operation: NavigationOperation) -> Self {
        Self {
            target: NavigationTarget::Surface(webview),
            operation,
        }
    }
}

impl TryFrom<&CommandInvocation> for NavigationRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        let operation = match invocation.id.as_str() {
            "browser_prev_page" => NavigationOperation::Back,
            "browser_next_page" => NavigationOperation::Forward,
            "browser_reload" => NavigationOperation::Reload,
            "browser_hard_reload" => NavigationOperation::HardReload,
            "browser_stop" => NavigationOperation::Stop,
            _ => return Err(()),
        };
        Ok(Self {
            target: NavigationTarget::Origin(invocation.caller),
            operation,
        })
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    origin: Entity,
    pub url: Option<String>,
}

impl OpenRequest {
    pub fn focused(url: Option<String>) -> Self {
        Self {
            origin: Entity::PLACEHOLDER,
            url,
        }
    }

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
                origin: invocation.caller,
                url: invocation.argument("url"),
            })
            .ok_or(())
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoomRequest {
    origin: Entity,
    change: ZoomChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomChange {
    In,
    Out,
    Reset,
}

impl ZoomRequest {
    pub fn focused(change: ZoomChange) -> Self {
        Self {
            origin: Entity::PLACEHOLDER,
            change,
        }
    }
}

impl TryFrom<&CommandInvocation> for ZoomRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        let change = match invocation.id.as_str() {
            "browser_zoom_in" => ZoomChange::In,
            "browser_zoom_out" => ZoomChange::Out,
            "browser_zoom_reset" => ZoomChange::Reset,
            _ => return Err(()),
        };
        Ok(Self {
            origin: invocation.caller,
            change,
        })
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowDevToolsRequest {
    origin: Entity,
}

impl ShowDevToolsRequest {
    pub fn focused() -> Self {
        Self {
            origin: Entity::PLACEHOLDER,
        }
    }
}

impl TryFrom<&CommandInvocation> for ShowDevToolsRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "browser_dev_tools")
            .then_some(Self {
                origin: invocation.caller,
            })
            .ok_or(())
    }
}

#[derive(SystemParam)]
struct BrowserRequestTarget<'w, 's> {
    focus: FocusedStack<'w, 's>,
    users: Query<'w, 's, (), With<vmux_ecs::team::User>>,
    focused_space: vmux_layout::space::FocusedSpace<'w, 's>,
    hierarchy: WindowHierarchy<'w, 's>,
    scope: WindowScope<'w, 's>,
    space_hierarchy: SpaceHierarchy<'w, 's>,
    spaces: Query<'w, 's, &'static vmux_ecs::EffectiveStartupUrl, With<Space>>,
    active_tabs:
        Query<'w, 's, (Entity, &'static ChildOf), (With<Tab>, With<Active>, Without<Retiring>)>,
    layout: LayoutFocus<'w, 's>,
    stacks: Query<'w, 's, (), (With<Stack>, Without<Retiring>)>,
    parents: Query<'w, 's, &'static ChildOf>,
}

impl BrowserRequestTarget<'_, '_> {
    fn stack(&self, origin: Entity) -> Option<Entity> {
        if origin != Entity::PLACEHOLDER && !self.users.contains(origin) {
            self.hierarchy.get(origin)?;
            let space = self.scope.current_space(origin)?;
            if self
                .space_hierarchy
                .get(origin)
                .is_some_and(|origin_space| origin_space != space)
            {
                return None;
            }
            let tab = self
                .active_tabs
                .iter()
                .find(|(_, parent)| parent.parent() == space)
                .map(|(tab, _)| tab);
            return self
                .layout
                .resolve(tab)
                .2
                .filter(|stack| self.stacks.contains(*stack));
        }
        self.focus
            .stack
            .filter(|stack| self.stacks.contains(*stack))
    }

    fn startup_url(&self, origin: Entity) -> Option<&str> {
        if origin != Entity::PLACEHOLDER && !self.users.contains(origin) {
            self.hierarchy.get(origin)?;
            let space = self.scope.current_space(origin)?;
            return self.spaces.get(space).ok().map(|url| url.0.as_str());
        }
        self.focused_space.startup_url()
    }
}

impl BrowserRequestTarget<'_, '_> {
    fn navigation_stack(
        &self,
        target: NavigationTarget,
        surfaces: &vmux_layout::hosted_page::StackSurface,
    ) -> Option<Entity> {
        match target {
            NavigationTarget::Origin(origin) => self.stack(origin),
            NavigationTarget::Surface(webview) => {
                let stack = self.parents.get(webview).ok()?.parent();
                if !self.stacks.contains(stack) || !surfaces.is_current(stack, webview) {
                    return None;
                }
                if let Some(space) = self.space_hierarchy.get(webview)
                    && self.scope.current_space(webview) != Some(space)
                {
                    return None;
                }
                Some(stack)
            }
        }
    }
}

#[derive(Default)]
struct GestureCooldown {
    stacks: std::collections::HashMap<Entity, std::time::Instant>,
}

impl GestureCooldown {
    fn accepts(&mut self, stack: Entity, now: std::time::Instant) -> bool {
        self.stacks
            .retain(|_, last| now.duration_since(*last) < std::time::Duration::from_millis(650));
        if self
            .stacks
            .get(&stack)
            .is_some_and(|last| now.duration_since(*last) < std::time::Duration::from_millis(650))
        {
            return false;
        }
        self.stacks.insert(stack, now);
        true
    }
}

fn handle_navigation_requests(
    mut navigation_requests: MessageReader<NavigationRequest>,
    target: BrowserRequestTarget,
    surfaces: vmux_layout::hosted_page::StackSurface,
    blocked: Query<
        (),
        Or<(
            With<BinIpcBlocked>,
            With<vmux_layout::hosted_page::SurfaceBlocked>,
            With<Retiring>,
        )>,
    >,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    histories: crate::navigation::NavigationHistory,
    mut host_history_steps: MessageWriter<HostHistoryStep>,
    mut commands: Commands,
    mut gestures: Local<GestureCooldown>,
    browsers: Option<NonSend<Browsers>>,
) {
    for request in navigation_requests.read() {
        if let NavigationTarget::Surface(webview) = request.target
            && blocked.contains(webview)
        {
            continue;
        }
        let Some(active) = target.navigation_stack(request.target, &surfaces) else {
            continue;
        };
        let Some(webview) = surfaces
            .current(active)
            .or_else(|| surfaces.incoming(active))
        else {
            continue;
        };
        let (is_terminal, _) = kind_q.get(webview).unwrap_or((false, false));
        if matches!(request.target, NavigationTarget::Surface(_))
            && matches!(
                request.operation,
                NavigationOperation::Back | NavigationOperation::Forward
            )
            && (is_terminal || !gestures.accepts(active, std::time::Instant::now()))
        {
            continue;
        }
        match request.operation {
            NavigationOperation::Back => {
                if is_terminal {
                    continue;
                }
                if let Some(owner) = histories.owner(active, webview, HostHistoryDelta::Back) {
                    host_history_steps.write(HostHistoryStep {
                        webview: owner,
                        delta: HostHistoryDelta::Back,
                    });
                } else {
                    commands.trigger(RequestGoBack { webview });
                }
            }
            NavigationOperation::Forward => {
                if is_terminal {
                    continue;
                }
                if let Some(owner) = histories.owner(active, webview, HostHistoryDelta::Forward) {
                    host_history_steps.write(HostHistoryStep {
                        webview: owner,
                        delta: HostHistoryDelta::Forward,
                    });
                } else {
                    commands.trigger(RequestGoForward { webview });
                }
            }
            NavigationOperation::Reload => {
                if is_terminal {
                    commands.trigger(RestartPty { entity: webview });
                } else {
                    commands.trigger(RequestReload { webview });
                }
            }
            NavigationOperation::HardReload => {
                if is_terminal {
                    commands.trigger(RestartPty { entity: webview });
                } else {
                    commands.trigger(RequestReloadIgnoreCache { webview });
                }
            }
            NavigationOperation::Stop => {
                let loading = surfaces.incoming(active).unwrap_or(webview);
                if let Some(browsers) = &browsers {
                    browsers.stop_loading_webview(&loading);
                }
                commands.trigger(StopLoadingRequested { webview: loading });
                commands
                    .trigger(vmux_layout::hosted_page::SurfaceCancelRequest { webview: loading });
            }
        }
    }
}

#[derive(EntityEvent)]
pub(crate) struct StopLoadingRequested {
    #[event_target]
    pub webview: Entity,
}

fn handle_open_requests(
    mut open_requests: MessageReader<OpenRequest>,
    target: BrowserRequestTarget,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
) {
    for request in open_requests.read() {
        let Some(active) = target.stack(request.origin) else {
            continue;
        };
        let resolved = request.resolved_url(target.startup_url(request.origin));
        if resolved.is_empty() {
            continue;
        }
        let resolved =
            VmuxRoute::canonical(&resolved).unwrap_or_else(|| resolved.trim().to_string());
        page_open_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(active),
            url: resolved,
            request_id: None,
        });
    }
}

fn handle_zoom_requests(
    mut requests: MessageReader<ZoomRequest>,
    target: BrowserRequestTarget,
    surfaces: vmux_layout::hosted_page::StackSurface,
    kind_q: Query<(Has<Terminal>, Has<vmux_editor::FileView>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut font_size_writer: MessageWriter<vmux_terminal::TerminalFontSizeCommand>,
) {
    for request in requests.read() {
        let Some(active) = target.stack(request.origin) else {
            continue;
        };
        let Some(webview) = surfaces.current(active) else {
            continue;
        };
        let (is_terminal, is_file) = kind_q.get(webview).unwrap_or((false, false));
        let is_text_grid = is_terminal || is_file;
        match request.change {
            ZoomChange::In => {
                if is_text_grid {
                    font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Increase);
                } else if let Ok(mut zoom) = zoom_q.get_mut(webview) {
                    zoom.0 += 0.5;
                }
            }
            ZoomChange::Out => {
                if is_text_grid {
                    font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Decrease);
                } else if let Ok(mut zoom) = zoom_q.get_mut(webview) {
                    zoom.0 -= 0.5;
                }
            }
            ZoomChange::Reset => {
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
    target: BrowserRequestTarget,
    surfaces: vmux_layout::hosted_page::StackSurface,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some(active) = target.stack(request.origin) else {
            continue;
        };
        let Some(webview) = surfaces.current(active) else {
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

fn header_stop(
    trigger: On<UiInput<HeaderStopRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "browser_stop",
    ));
}

fn header_address_focus(
    trigger: On<UiInput<HeaderAddressFocusRequest>>,
    mut command_invocations: MessageWriter<CommandInvocation>,
) {
    command_invocations.write(CommandInvocation::new(
        trigger.event().webview,
        "command_bar_edit_page",
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
    mut sheets: Query<(Entity, &SideSheetPosition, &mut vmux_flex::prelude::Node), With<SideSheet>>,
    scope: WindowScope,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<MessageWriter<vmux_setting::SettingsSaveRequest>>,
) {
    let resize = trigger.event().payload;
    let next = resize.clamped();
    let mut accepted = false;
    for (entity, position, mut node) in &mut sheets {
        if scope.contains(trigger.event().webview, entity) && *position == SideSheetPosition::Left {
            accepted = true;
            if node.width != vmux_flex::prelude::Val::Px(next) {
                node.width = vmux_flex::prelude::Val::Px(next);
            }
        }
    }
    if !accepted || !resize.settled {
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
    stack_q: Query<Entity, (With<Stack>, Without<Retiring>)>,
    scope: WindowScope,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes.iter().find(|entity| {
        entity.to_bits() == request.pane_id
            && scope.contains_current_space(trigger.event().webview, *entity)
    }) else {
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
    commands.trigger(vmux_ecs::ActivateRequest {
        entity: target_stack,
    });
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
    stack_q: Query<Entity, (With<Stack>, Without<Retiring>)>,
    scope: WindowScope,
    mut requests: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes.iter().find(|entity| {
        entity.to_bits() == request.pane_id
            && scope.contains_current_space(trigger.event().webview, *entity)
    }) else {
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
    spaces: SpaceHierarchy,
    startup_urls: Query<&vmux_ecs::EffectiveStartupUrl, With<Space>>,
    scope: WindowScope,
    mut requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let Some(target_pane) = leaf_panes.iter().find(|entity| {
        entity.to_bits() == trigger.event().payload.pane_id
            && scope.contains_current_space(trigger.event().webview, *entity)
    }) else {
        return;
    };
    let Some(url) = spaces
        .get(target_pane)
        .and_then(|space| startup_urls.get(space).ok())
        .map(|url| url.0.clone())
    else {
        return;
    };
    commands.entity(target_pane).insert(LastActivatedAt::now());
    requests.write(PageOpenRequest {
        target: PageOpenTarget::NewStackInPane(target_pane),
        url,
        request_id: None,
    });
}

fn side_sheet_project_open(
    trigger: On<UiInput<SideSheetProjectOpenRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    scope: WindowScope,
    mut requests: MessageWriter<PageOpenRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes.iter().find(|entity| {
        entity.to_bits() == request.pane_id
            && scope.contains_current_space(trigger.event().webview, *entity)
    }) else {
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

fn side_sheet_project_page_open(
    trigger: On<UiInput<SideSheetProjectPageOpenRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    projects: vmux_ecs::profile::Projects,
    scope: WindowScope,
    mut requests: MessageWriter<PageOpenRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes.iter().find(|entity| {
        entity.to_bits() == request.pane_id
            && scope.contains_current_space(trigger.event().webview, *entity)
    }) else {
        return;
    };
    let Ok(root) = projects.path() else {
        return;
    };
    let url = match vmux_project::ProjectRoute::attach(root, std::path::Path::new(&request.path)) {
        Ok(url) => url,
        Err(error) => {
            bevy::log::warn!("project could not be opened: {error}");
            return;
        }
    };
    requests.write(PageOpenRequest {
        target: PageOpenTarget::ActiveStackInPane(target_pane),
        url,
        request_id: None,
    });
}

fn side_sheet_section(
    trigger: On<UiInput<SideSheetSectionRequest>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    sections_of: vmux_layout::side_sheet::SideSheetSections,
    scope: WindowScope,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some(target_pane) = leaf_panes.iter().find(|entity| {
        entity.to_bits() == request.pane_id
            && scope.contains_current_space(trigger.event().webview, *entity)
    }) else {
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
    if state == SideSheetSectionsExpanded::default() {
        commands.entity(space).remove::<SideSheetSectionsExpanded>();
    } else {
        commands.entity(space).insert(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_ecs::page::HostHistory;
    use vmux_flex::prelude::{Node, Val};
    use vmux_layout::pane::Pane;
    use vmux_layout::space::{Space, SpaceId};
    use vmux_layout::stack::Stack;
    use vmux_layout::tab::Tab;
    use vmux_setting::AppSettings;

    #[derive(Resource, Default)]
    struct CefNavigations(Vec<Entity>);

    #[derive(Resource, Default)]
    struct StoppedPages(Vec<Entity>);

    impl StoppedPages {
        fn record(request: On<StopLoadingRequested>, mut pages: ResMut<Self>) {
            pages.0.push(request.event().webview);
        }
    }

    #[derive(Resource, Default)]
    struct ReloadedPages(Vec<(Entity, NavigationOperation)>);

    impl ReloadedPages {
        fn record(request: On<RequestReload>, mut pages: ResMut<Self>) {
            pages.0.push((request.webview, NavigationOperation::Reload));
        }

        fn record_ignore_cache(request: On<RequestReloadIgnoreCache>, mut pages: ResMut<Self>) {
            pages
                .0
                .push((request.webview, NavigationOperation::HardReload));
        }
    }

    #[test]
    fn keyboard_reload_uses_the_selected_page_for_the_user_caller() {
        let mut fixture = NavArrow::over(HostHistory::default());
        fixture
            .app
            .init_resource::<ReloadedPages>()
            .add_observer(ReloadedPages::record)
            .add_observer(ReloadedPages::record_ignore_cache);
        let user = fixture.app.world_mut().spawn(vmux_ecs::team::User).id();
        for id in ["browser_reload", "browser_hard_reload"] {
            fixture
                .app
                .world_mut()
                .write_message(CommandInvocation::new(user, id));
            fixture.app.update();
            fixture.app.update();
        }
        let unrelated = fixture.app.world_mut().spawn_empty().id();
        fixture
            .app
            .world_mut()
            .write_message(CommandInvocation::new(unrelated, "browser_reload"));
        fixture.app.update();
        fixture.app.update();
        assert_eq!(
            fixture.app.world().resource::<ReloadedPages>().0,
            [
                (fixture.view, NavigationOperation::Reload),
                (fixture.view, NavigationOperation::HardReload),
            ]
        );
    }

    #[test]
    fn the_stop_command_reaches_the_selected_page() {
        let mut fixture = NavArrow::over(HostHistory::default());
        fixture
            .app
            .init_resource::<StoppedPages>()
            .add_observer(StoppedPages::record);
        fixture
            .app
            .world_mut()
            .write_message(NavigationRequest::focused(NavigationOperation::Stop));
        fixture.app.update();
        assert_eq!(
            fixture.app.world().resource::<StoppedPages>().0,
            [fixture.view]
        );
    }

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
            app.add_plugins((
                MinimalPlugins,
                vmux_ecs::PrimitivesPlugin,
                vmux_ecs::page::PagePlugin,
                CommandPlugin,
            ))
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
                .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                .id();
            let view = app.world_mut().spawn((Browser, ChildOf(stack), page)).id();
            app.world_mut()
                .entity_mut(stack)
                .insert(vmux_layout::page_slot::Current::new(
                    Some(view),
                    PageMetadata::default(),
                ));
            app.world_mut().spawn(
                vmux_layout::active_pane::ActiveStack {
                    tab: Some(tab),
                    pane: Some(pane),
                    stack: Some(stack),
                }
                .local_bundle(),
            );

            Self { app, view }
        }

        fn over_a_hosted_page() -> Self {
            let mut fixture = Self::over(HostHistory::default());
            for url in ["file:///a.rs", "file:///b.rs"] {
                fixture.app.world_mut().write_message(HostHistoryObserve {
                    webview: fixture.view,
                    url: url.to_string(),
                    top_line: 0,
                });
                fixture.app.update();
            }
            fixture
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

        fn another_hosted_page(&mut self) -> Entity {
            let stack = self.app.world().get::<ChildOf>(self.view).unwrap().parent();
            let pane = self.app.world().get::<ChildOf>(stack).unwrap().parent();
            let tab = self.app.world().get::<ChildOf>(pane).unwrap().parent();
            let other_pane = self
                .app
                .world_mut()
                .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
                .id();
            let other_stack = self
                .app
                .world_mut()
                .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(other_pane)))
                .id();
            let view = self
                .app
                .world_mut()
                .spawn((Browser, HostHistory::default(), ChildOf(other_stack)))
                .id();
            self.app.world_mut().entity_mut(other_stack).insert(
                vmux_layout::page_slot::Current::new(Some(view), PageMetadata::default()),
            );
            for url in ["file:///other-a.rs", "file:///other-b.rs"] {
                self.app.world_mut().write_message(HostHistoryObserve {
                    webview: view,
                    url: url.to_string(),
                    top_line: 0,
                });
                self.app.update();
            }
            view
        }

        fn swiped_back(&mut self, view: Entity) {
            self.app
                .world_mut()
                .write_message(NavigationRequest::surface(view, NavigationOperation::Back));
            self.app.update();
            self.app.update();
        }
    }

    #[test]
    fn a_swipe_moves_the_source_panes_host_history_without_changing_the_focused_pane() {
        let mut arrow = NavArrow::over_a_hosted_page();
        let source = arrow.another_hosted_page();
        arrow.swiped_back(source);
        assert!(!arrow.walked_back());
        assert!(
            arrow
                .app
                .world()
                .get::<HostHistory>(source)
                .unwrap()
                .can_go_forward()
        );
        assert!(arrow.cef_navigations().is_empty());
    }

    #[test]
    fn a_swipe_from_a_replaced_or_blocked_surface_is_ignored() {
        let mut arrow = NavArrow::over_a_hosted_page();
        arrow
            .app
            .world_mut()
            .entity_mut(arrow.view)
            .insert(BinIpcBlocked);
        arrow.swiped_back(arrow.view);
        assert!(!arrow.walked_back());
        arrow
            .app
            .world_mut()
            .entity_mut(arrow.view)
            .remove::<BinIpcBlocked>();
        let stack = arrow
            .app
            .world()
            .get::<ChildOf>(arrow.view)
            .unwrap()
            .parent();
        let current = arrow.app.world_mut().spawn((Browser, ChildOf(stack))).id();
        arrow
            .app
            .world_mut()
            .entity_mut(stack)
            .insert(vmux_layout::page_slot::Current::new(
                Some(current),
                PageMetadata::default(),
            ));
        arrow.swiped_back(arrow.view);
        assert!(!arrow.walked_back());
        assert!(arrow.cef_navigations().is_empty());
    }

    #[test]
    fn momentum_from_a_new_document_cannot_navigate_the_same_stack_twice() {
        let mut arrow = NavArrow::over(());
        let stack = arrow
            .app
            .world()
            .get::<ChildOf>(arrow.view)
            .unwrap()
            .parent();
        arrow.swiped_back(arrow.view);
        let replacement = arrow.app.world_mut().spawn((Browser, ChildOf(stack))).id();
        arrow
            .app
            .world_mut()
            .entity_mut(stack)
            .insert(vmux_layout::page_slot::Current::new(
                Some(replacement),
                PageMetadata::default(),
            ));
        arrow.swiped_back(replacement);
        assert_eq!(arrow.cef_navigations(), [arrow.view]);
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
        .add_message::<ShowDevToolsRequest>();
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
                origin: Entity::PLACEHOLDER,
                url: Some("https://vmux.ai".to_string()),
            }]
        );
        assert_eq!(
            navigation_requests,
            [NavigationRequest::focused(NavigationOperation::Reload)]
        );
    }

    #[test]
    fn sidebar_selection_advances_the_entire_restored_ancestry() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::PrimitivesPlugin))
            .add_observer(side_sheet_stack_activate);
        let restored = LastActivatedAt(LastActivatedAt::now().0 + 5_000);
        let window = app.world_mut().spawn_empty().id();
        let root = app.world_mut().spawn((HostWindow(window), restored)).id();
        let space = app
            .world_mut()
            .spawn((Space, CurrentSpace, Active, restored, ChildOf(root)))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), Active, restored, ChildOf(space)))
            .id();
        let branch = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: vmux_layout::pane::PaneSplitDirection::Row,
                },
                restored,
                ChildOf(tab),
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((Pane, restored, ChildOf(branch)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), restored, ChildOf(leaf)))
            .id();
        let view = app.world_mut().spawn(ChildOf(root)).id();
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: SideSheetStackActivateRequest {
                pane_id: leaf.to_bits(),
                stack_id: stack.to_bits(),
            },
        });
        app.update();
        let selected = app.world().get::<LastActivatedAt>(stack).unwrap().0;
        assert!(selected > restored.0);
        for entity in [root, space, tab, branch, leaf] {
            assert_eq!(
                app.world().get::<LastActivatedAt>(entity).unwrap().0,
                selected
            );
        }
    }

    #[test]
    fn header_origin_targets_its_own_window_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, handle_open_requests);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_space = app
            .world_mut()
            .spawn((
                Space,
                CurrentSpace,
                Active,
                vmux_ecs::EffectiveStartupUrl::default(),
                ChildOf(first_root),
            ))
            .id();
        let second_space = app
            .world_mut()
            .spawn((
                Space,
                CurrentSpace,
                Active,
                vmux_ecs::EffectiveStartupUrl::default(),
                ChildOf(second_root),
            ))
            .id();
        let first_tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Active,
                LastActivatedAt(1),
                ChildOf(first_space),
            ))
            .id();
        let second_tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Active,
                LastActivatedAt(1),
                ChildOf(second_space),
            ))
            .id();
        let first_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(first_tab)))
            .id();
        let second_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(second_tab)))
            .id();
        let first_stack = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                Active,
                LastActivatedAt(1),
                ChildOf(first_pane),
            ))
            .id();
        let second_stack = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                Active,
                LastActivatedAt(1),
                ChildOf(second_pane),
            ))
            .id();
        let second_header = app.world_mut().spawn(ChildOf(second_root)).id();
        app.world_mut().spawn(
            vmux_layout::active_pane::ActiveStack {
                tab: Some(first_tab),
                pane: Some(first_pane),
                stack: Some(first_stack),
            }
            .local_bundle(),
        );
        app.world_mut().write_message(OpenRequest {
            origin: second_header,
            url: Some("vmux://projects/".to_string()),
        });

        app.update();

        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert!(matches!(
            opened[0].target,
            PageOpenTarget::Stack(stack) if stack == second_stack
        ));
    }

    #[test]
    fn the_back_arrow_walks_host_history_instead_of_asking_chromium() {
        let mut arrow = NavArrow::over_a_hosted_page();

        arrow.pressed_back();

        assert!(
            arrow.walked_back(),
            "the arrow must move the host history cursor off its newest entry"
        );
        assert!(
            arrow.cef_navigations().is_empty(),
            "a hosted page has no Chromium browser to walk back"
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
            .spawn((Stack::bundle(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let active = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt(2), ChildOf(pane)))
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
        let mut settings = AppSettings::default();
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
                .spawn((Space, SpaceId("work".to_string()), vmux_ecs::Active))
                .id();
            let first_tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
            let pane_in_first_tab = app.world_mut().spawn((Pane, ChildOf(first_tab))).id();
            app.world_mut()
                .spawn((Stack::bundle(), ChildOf(pane_in_first_tab)));
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

        fn set(&mut self, section: &str, expanded: bool) {
            self.app
                .world_mut()
                .trigger(UiInput::<SideSheetSectionRequest> {
                    webview: Entity::PLACEHOLDER,
                    payload: SideSheetSectionRequest {
                        pane_id: self.pane_in_first_tab.to_bits(),
                        path: section.to_string(),
                        expanded,
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
    fn files_start_closed_and_stay_expanded_across_pages() {
        let mut spaces = SideSheetSpaces::start();
        let first_pane = spaces.pane_in_first_tab;
        assert!(!spaces.sections_under(first_pane).projects);
        spaces.set("projects", true);

        let second_tab = spaces.second_tab;
        assert!(
            spaces.sections_under(second_tab).projects,
            "a card is expanded for the whole space, so switching tab must not fold it"
        );
    }

    #[test]
    fn expanding_a_card_leaves_the_other_spaces_alone() {
        let mut spaces = SideSheetSpaces::start();
        spaces.set("projects", true);

        let elsewhere = spaces.tab_in_other_space;
        assert!(!spaces.sections_under(elsewhere).projects);
    }

    #[test]
    fn bookmarks_start_open_and_keep_the_closed_choice_across_tabs() {
        let mut spaces = SideSheetSpaces::start();
        let first_pane = spaces.pane_in_first_tab;
        assert!(spaces.sections_under(first_pane).bookmarks);
        spaces.set("bookmarks", false);
        let second_tab = spaces.second_tab;
        assert!(!spaces.sections_under(second_tab).bookmarks);
        let elsewhere = spaces.tab_in_other_space;
        assert!(spaces.sections_under(elsewhere).bookmarks);
        spaces.set("bookmarks", true);
        assert!(spaces.sections_under(second_tab).bookmarks);
    }
}
