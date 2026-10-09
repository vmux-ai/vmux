use crate::CommandBar;
use crate::{CommandBarOpenProjection, CommandBarProjector};
use std::time::{Duration, Instant};
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarPickRow, CommandBarPicker, CommandBarReadyEvent,
    CommandBarRenderedEvent, CommandBarSizeEvent, DismissRequest, OpenId,
};
use vmux_api::open_target::OpenTarget;
use vmux_ecs::launcher::{LauncherDismissRequest, RendersLauncherPanel, RestoreKeyboardToStack};

use crate::host::command_bar::CommandBarDismiss;
use crate::host::command_bar::panel::CommandBarPanelActive;
use crate::host::snapshot::{
    CommandBarContextSnapshot, CommandBarPagesSnapshot, CommandBarProjectRoots,
    CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot,
};
use crate::{CommandInvocation, ReadCommandRequests};
use bevy::{
    ecs::{message::MessageReader, system::SystemParam},
    prelude::*,
};
use bevy_cef::prelude::*;
use vmux_ecs::PageMetadata;
use vmux_ecs::UnixMillis;
use vmux_ecs::page::HostsPage;
use vmux_ui::i18n::Locale;

use crate::ResolvedLocale;
use vmux_ecs::KeyboardOwner;
use vmux_flex::prelude::*;

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandBarToggleRequest>()
            .add_plugins(super::work_snapshot::Plugin)
            .add_message::<CommandBarEditPageRequest>()
            .add_message::<CommandBarPathRequest>()
            .add_message::<CommandBarCommandsRequest>()
            .add_message::<CommandBarOpenRequest>()
            .configure_sets(
                Update,
                (WriteCommandBarRequests, ApplyCommandBarRequests)
                    .chain()
                    .in_set(ReadCommandRequests),
            )
            .add_message::<LauncherDismissRequest>()
            .add_message::<vmux_ecs::ContributedCommandChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_plugins(UiEventPlugin::<(
                DismissRequest,
                CommandBarReadyEvent,
                CommandBarRenderedEvent,
                CommandBarSizeEvent,
            )>::default())
            .add_observer(dismiss)
            .add_observer(close)
            .add_observer(close_panel)
            .add_observer(ready)
            .add_observer(rendered)
            .add_observer(resize)
            .add_systems(Update, prewarm.before(CefSystems::CreateAndResize))
            .add_systems(
                Update,
                open.in_set(ApplyCommandBarRequests)
                    .after(prewarm)
                    .after(vmux_ecs::workspace::TabCommandSet)
                    .after(vmux_ecs::workspace::StackCommandSet),
            )
            .add_systems(Update, retry.after(open))
            .add_systems(
                Update,
                dismiss_requested
                    .after(ReadCommandRequests)
                    .before(vmux_ecs::workspace::ComputeFocusSet),
            )
            .add_systems(PostUpdate, reveal.chain().after(LayoutSystems::Layout))
            .add_systems(Update, wake.after(ReadCommandRequests));
    }
}

fn wake(
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    pending: Query<&PendingCommandBarReveal>,
) {
    if !pending.iter().any(PendingCommandBarReveal::is_active) {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CommandBarToggleRequest;

impl TryFrom<&CommandInvocation> for CommandBarToggleRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "command_bar_open")
            .then_some(Self)
            .ok_or(())
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CommandBarEditPageRequest;

impl TryFrom<&CommandInvocation> for CommandBarEditPageRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "command_bar_edit_page")
            .then_some(Self)
            .ok_or(())
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CommandBarPathRequest;

impl TryFrom<&CommandInvocation> for CommandBarPathRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "command_bar_open_path")
            .then_some(Self)
            .ok_or(())
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CommandBarCommandsRequest;

impl TryFrom<&CommandInvocation> for CommandBarCommandsRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "command_bar_open_commands")
            .then_some(Self)
            .ok_or(())
    }
}

#[derive(Message, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandBarOpenRequest {
    query: Option<String>,
    picker: Option<CommandBarPicker>,
    picks: Vec<CommandBarPickRow>,
    picker_label: String,
    picker_placeholder: String,
    picker_typed: bool,
    picker_numbered: bool,
    replace_active_stack: bool,
}

impl CommandBarOpenRequest {
    pub fn query(query: impl Into<String>) -> Self {
        Self {
            query: Some(query.into()),
            ..default()
        }
    }

    pub fn picker_with(
        picker: CommandBarPicker,
        picks: Vec<CommandBarPickRow>,
        label: impl Into<String>,
        placeholder: impl Into<String>,
        typed: bool,
        numbered: bool,
    ) -> Self {
        Self {
            query: Some(String::new()),
            picker: Some(picker),
            picks,
            picker_label: label.into(),
            picker_placeholder: placeholder.into(),
            picker_typed: typed,
            picker_numbered: numbered,
            ..default()
        }
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WriteCommandBarRequests;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ApplyCommandBarRequests;

#[derive(Component)]
struct CommandBarReady;

#[derive(Component)]
struct CommandBarRenderedOpen(OpenId);

#[derive(Component)]
struct CommandBarOpenedOnce;

#[derive(Component)]
struct CommandBarRecreating;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CommandBarNativeSize {
    pub width: f32,
    pub height: f32,
    pub shell_left: f32,
    pub shell_top: f32,
    pub shell_width: f32,
    pub shell_height: f32,
}

#[derive(Component)]
pub struct PendingCommandBarReveal {
    pub(super) frames: u8,
    pub(super) open_id: OpenId,
    pub(super) payload: Option<CommandBarOpenEvent>,
    pub(super) started_at: Option<Instant>,
    pub(super) last_retry: Option<Instant>,
}

pub(super) const COMMAND_BAR_REVEAL_FRAMES: u8 = 2;
pub(super) const COMMAND_BAR_REVEAL_FALLBACK_FRAMES: u8 = 10;
pub(super) const COMMAND_BAR_NATIVE_REVEAL_TIMEOUT: Duration = Duration::from_secs(2);
const COMMAND_BAR_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(100);

type PrewarmRow = (
    Entity,
    &'static mut Node,
    &'static mut Visibility,
    Has<KeyboardOwner>,
    Has<PendingCommandBarReveal>,
    Has<WebviewNativeOverlay>,
);
type ResizeRow = (
    &'static Visibility,
    Option<&'static PendingCommandBarReveal>,
    Option<&'static CommandBarNativeSize>,
    Has<WebviewWindowed>,
);
type RevealRow = (
    Entity,
    &'static mut Visibility,
    &'static mut PendingCommandBarReveal,
    Option<&'static CommandBarRenderedOpen>,
    Option<&'static CommandBarNativeSize>,
    Has<WebviewWindowed>,
    Has<WebviewNativeOverlay>,
);
type RetryRow = (
    Entity,
    &'static mut PendingCommandBarReveal,
    Option<&'static CommandBarRenderedOpen>,
    Has<CommandBarRecreating>,
);
type CloseRow = (
    Entity,
    &'static mut Node,
    &'static mut Visibility,
    Has<WebviewNativeOverlay>,
);
type LauncherLayout = (
    Entity,
    Has<CommandBarPanelActive>,
    Option<&'static HostWindow>,
);
type BrowserPageFilter = Or<(With<WebviewSource>, With<HostsPage>)>;

#[derive(SystemParam)]
struct CommandBarSurface<'w, 's> {
    bars: Query<'w, 's, CloseRow, With<CommandBar>>,
}

impl CommandBarSurface<'_, '_> {
    fn close(&mut self) -> Option<Entity> {
        let Ok((entity, mut node, mut visibility, native_overlay)) = self.bars.single_mut() else {
            return None;
        };
        if native_overlay {
            node.display = Display::Flex;
            *visibility = Visibility::Visible;
        } else {
            node.display = Display::None;
            *visibility = Visibility::Hidden;
        }
        Some(entity)
    }

    fn close_visible(&mut self) -> Option<Entity> {
        let Ok((entity, mut node, mut visibility, native_overlay)) = self.bars.single_mut() else {
            return None;
        };
        if node.display == Display::None {
            return None;
        }
        if native_overlay {
            node.display = Display::Flex;
            *visibility = Visibility::Visible;
        } else {
            node.display = Display::None;
            *visibility = Visibility::Hidden;
        }
        Some(entity)
    }
}

fn prewarm(mut commands: Commands, mut modal_q: Query<PrewarmRow, With<CommandBar>>) {
    let Ok((
        modal_e,
        mut modal_node,
        mut modal_vis,
        has_keyboard_target,
        pending_reveal,
        native_overlay,
    )) = modal_q.single_mut()
    else {
        return;
    };
    if has_keyboard_target || pending_reveal {
        return;
    }
    modal_node.display = Display::Flex;
    *modal_vis = if native_overlay {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    commands.entity(modal_e).insert(PendingCommandBarReveal {
        frames: 0,
        open_id: OpenId::NONE,
        payload: None,
        started_at: None,
        last_retry: None,
    });
}

fn ready(
    trigger: On<UiInput<CommandBarReadyEvent>>,
    mut pending_q: Query<&mut PendingCommandBarReveal>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if let Ok(mut pending) = pending_q.get_mut(webview)
        && pending.open_id.is_open()
        && pending.started_at.is_none()
    {
        pending.started_at = Some(Instant::now());
    }
    commands
        .entity(webview)
        .insert(CommandBarReady)
        .remove::<CommandBarRecreating>();
}

fn rendered(
    trigger: On<UiInput<CommandBarRenderedEvent>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    browsers.set_windowed_focus(&webview, true);
    browsers.execute_js(
        &webview,
        "const input = document.getElementById('command-bar-input'); if (input) { input.focus({ preventScroll: true }); }",
    );
    commands.entity(webview).insert((
        CommandBarRenderedOpen(trigger.event().payload.open_id),
        CommandBarOpenedOnce,
    ));
}

fn resize(
    trigger: On<UiInput<CommandBarSizeEvent>>,
    browsers: NonSend<Browsers>,
    state: Query<ResizeRow>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((visibility, pending_reveal, current_size, native_windowed)) = state.get(webview) else {
        return;
    };
    if *visibility == Visibility::Hidden
        && !pending_reveal.is_some_and(PendingCommandBarReveal::accepts_size)
    {
        return;
    }
    let payload = trigger.event().payload;
    if native_windowed
        && let Some(open_id) = pending_reveal
            .filter(|pending| pending.open_id.is_open())
            .map(|pending| pending.open_id)
    {
        browsers.set_windowed_focus(&webview, true);
        browsers.execute_js(
            &webview,
            "const input = document.getElementById('command-bar-input'); if (input) { input.focus({ preventScroll: true }); }",
        );
        commands
            .entity(webview)
            .insert((CommandBarRenderedOpen(open_id), CommandBarOpenedOnce));
    }
    if current_size.is_some_and(|size| {
        size.width == payload.width.max(1) as f32
            && size.height == payload.height.max(1) as f32
            && size.shell_left == payload.shell_left as f32
            && size.shell_top == payload.shell_top as f32
            && size.shell_width == payload.shell_width.max(1) as f32
            && size.shell_height == payload.shell_height.max(1) as f32
    }) {
        return;
    }
    commands.entity(webview).insert(CommandBarNativeSize {
        width: payload.width.max(1) as f32,
        height: payload.height.max(1) as f32,
        shell_left: payload.shell_left as f32,
        shell_top: payload.shell_top as f32,
        shell_width: payload.shell_width.max(1) as f32,
        shell_height: payload.shell_height.max(1) as f32,
    });
}

#[derive(Default)]
struct CommandBarOpenState {
    should_toggle: bool,
    should_dismiss: bool,
    should_dismiss_nav: bool,
    replace_active_stack: bool,
    url_override: Option<String>,
    picker: Option<CommandBarPicker>,
    picks: Vec<CommandBarPickRow>,
    picker_label: String,
    picker_placeholder: String,
    picker_typed: bool,
    picker_numbered: bool,
}

#[derive(SystemParam)]
struct OpenRequests<'w, 's> {
    invocations: MessageReader<'w, 's, CommandInvocation>,
    toggle: MessageReader<'w, 's, CommandBarToggleRequest>,
    edit_page: MessageReader<'w, 's, CommandBarEditPageRequest>,
    path: MessageReader<'w, 's, CommandBarPathRequest>,
    commands: MessageReader<'w, 's, CommandBarCommandsRequest>,
    open: MessageReader<'w, 's, CommandBarOpenRequest>,
}

impl OpenRequests<'_, '_> {
    fn read(&mut self) -> CommandBarOpenState {
        let mut request = CommandBarOpenState::default();
        if self.toggle.read().next().is_some() {
            request.should_toggle = true;
            request.url_override = Some(String::new());
        }
        if self.edit_page.read().next().is_some() {
            request.should_toggle = true;
            request.replace_active_stack = true;
        }
        if self.path.read().next().is_some() {
            request.should_toggle = true;
            request.url_override = Some("/".to_string());
        }
        if self.commands.read().next().is_some() {
            request.should_toggle = true;
            request.url_override = Some(">".to_string());
        }
        for open in self.open.read() {
            request.should_toggle = true;
            request.replace_active_stack |= open.replace_active_stack;
            if open.query.is_some() {
                request.url_override.clone_from(&open.query);
            }
            if open.picker.is_some() {
                request.picker.clone_from(&open.picker);
                request.picks.clone_from(&open.picks);
                request.picker_label.clone_from(&open.picker_label);
                request
                    .picker_placeholder
                    .clone_from(&open.picker_placeholder);
                request.picker_typed = open.picker_typed;
                request.picker_numbered = open.picker_numbered;
            }
        }
        for invocation in self.invocations.read() {
            match invocation.id.as_str() {
                "stack_close" => {
                    request.should_dismiss = true;
                }
                "stack_next" | "stack_previous" | "select_pane_left" | "select_pane_right"
                | "select_pane_up" | "select_pane_down" => {
                    request.should_dismiss_nav = true;
                }
                _ => {
                    continue;
                }
            }
        }
        request
    }
}

#[derive(SystemParam)]
struct OpenSources<'w, 's> {
    layouts: Query<'w, 's, LauncherLayout, With<RendersLauncherPanel>>,
    windows: Query<'w, 's, &'static Window>,
    children: Query<'w, 's, &'static Children>,
    metadata: Query<'w, 's, &'static PageMetadata, BrowserPageFilter>,
    workspace: Single<'w, 's, &'static CommandBarWorkspaceSnapshot>,
    projects: Single<'w, 's, &'static CommandBarProjectRoots>,
    context: Single<'w, 's, &'static CommandBarContextSnapshot>,
    pages: Single<'w, 's, &'static CommandBarPagesSnapshot>,
    work: Single<'w, 's, &'static CommandBarWorkSnapshot>,
    projector: CommandBarProjector<'w, 's>,
    locale: Option<Res<'w, ResolvedLocale>>,
}

impl OpenSources<'_, '_> {
    fn layout(&self) -> Option<(Entity, bool)> {
        self.layouts
            .iter()
            .find(|(_, _, host)| {
                host.is_some_and(|host| self.windows.get(host.0).is_ok_and(|window| window.focused))
            })
            .or_else(|| self.layouts.iter().next())
            .map(|(entity, open, _)| (entity, open))
    }

    fn current_url(&self, override_url: Option<String>) -> String {
        if let Some(url) = override_url {
            return url;
        }
        self.workspace
            .stack
            .and_then(|stack| {
                let Ok(children) = self.children.get(stack) else {
                    return None;
                };
                children
                    .iter()
                    .find_map(|entity| self.metadata.get(entity).ok())
            })
            .map(|metadata| metadata.url.clone())
            .unwrap_or_default()
    }

    fn locale(&self) -> Locale {
        self.locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(Locale::preferred)
    }
}

fn open(
    mut open_requests: OpenRequests,
    sources: OpenSources,
    mut restore_keyboard: MessageWriter<RestoreKeyboardToStack>,
    mut commands: Commands,
) {
    let request = open_requests.read();
    if !request.should_toggle && !request.should_dismiss && !request.should_dismiss_nav {
        return;
    }

    let Some((layout_e, is_open)) = sources.layout() else {
        return;
    };
    let focus = &*sources.workspace;
    let context = (*sources.context).clone();
    let locale = sources.locale();
    let toggle_closes = request.should_toggle && is_open && request.picker.is_none();
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let replace_active_stack = request.replace_active_stack;
    let url_override = request.url_override;
    let picker = request.picker;
    let picks = request.picks;
    let picker_label = request.picker_label;
    let picker_placeholder = request.picker_placeholder;
    let picker_typed = request.picker_typed;
    let picker_numbered = request.picker_numbered;

    if (should_dismiss || toggle_closes) && is_open {
        commands.trigger(CommandBarPanelClose { layout: layout_e });
        if let Some(stack) = focus.stack {
            restore_keyboard.write(RestoreKeyboardToStack { stack });
        }
        return;
    }

    if should_dismiss_nav && is_open {
        commands.trigger(CommandBarPanelClose { layout: layout_e });
        return;
    }

    if !should_toggle || toggle_closes {
        return;
    }

    let current_url = sources.current_url(url_override);

    let bar_tabs = focus.tabs.clone();

    let target = replace_active_stack.then_some(OpenTarget::InPlace);
    let mut payload = sources.projector.project(CommandBarOpenProjection {
        open_id: OpenId(UnixMillis::now().0 as u64),
        native_windowed: false,
        context,
        url: current_url,
        pages: (*sources.pages).clone(),
        projects: (*sources.projects).clone(),
        work: (*sources.work).clone(),
        locale: locale.clone(),
        tabs: bar_tabs,
        target,
    });
    payload.picker = picker;
    payload.picker_label = picker_label;
    payload.picker_placeholder = picker_placeholder;
    payload.picker_typed = picker_typed;
    payload.picker_numbered = picker_numbered;
    payload.caret_at_end =
        super::driver::PaletteQuery::new(&payload.url).opens_at_end(payload.picker.as_ref());
    payload.picks = picks;
    commands.trigger(vmux_ecs::UiStateWrite::<
        vmux_api::command_bar::CommandBarUiState,
    >::from_event(layout_e, &payload));
}

#[derive(EntityEvent)]
struct CommandBarPanelClose {
    #[event_target]
    layout: Entity,
}

fn close_panel(trigger: On<CommandBarPanelClose>, mut commands: Commands) {
    commands.trigger(vmux_ecs::UiStateWrite::<
        vmux_api::command_bar::CommandBarUiState,
    >::from_event(
        trigger.event().layout, &CommandBarOpenEvent::default()
    ));
}

fn dismiss(trigger: On<UiInput<DismissRequest>>, mut commands: Commands) {
    commands.trigger(CommandBarDismiss::new(trigger.event().webview, true));
}

fn close(
    trigger: On<CommandBarDismiss>,
    workspace: Single<&CommandBarWorkspaceSnapshot>,
    panels: Query<(), With<RendersLauncherPanel>>,
    mut surface: CommandBarSurface,
    mut restore_keyboard: MessageWriter<RestoreKeyboardToStack>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if panels.contains(webview) {
        commands.trigger(CommandBarPanelClose { layout: webview });
    }
    if let Some(modal_e) = surface.close() {
        commands
            .entity(modal_e)
            .remove::<KeyboardOwner>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
    }
    if trigger.event().restore_keyboard
        && let Some(stack) = workspace.stack
    {
        restore_keyboard.write(RestoreKeyboardToStack { stack });
    }
}

fn dismiss_requested(
    mut requests: MessageReader<LauncherDismissRequest>,
    mut surface: CommandBarSurface,
    panel_q: Query<Entity, (With<RendersLauncherPanel>, With<CommandBarPanelActive>)>,
    mut commands: Commands,
) {
    if requests.read().next().is_none() {
        return;
    }
    for layout_e in &panel_q {
        commands.trigger(CommandBarPanelClose { layout: layout_e });
    }
    if let Some(modal_e) = surface.close_visible() {
        commands
            .entity(modal_e)
            .remove::<KeyboardOwner>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
    }
}

fn reveal(mut commands: Commands, mut query: Query<RevealRow, With<CommandBar>>) {
    for (entity, mut vis, mut pending, rendered, native_size, native_windowed, native_overlay) in
        &mut query
    {
        let now = Instant::now();
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        if pending.timed_out(
            now,
            rendered_open_id,
            native_windowed,
            native_overlay,
            native_size.is_some(),
        ) {
            commands.entity(entity).remove::<PendingCommandBarReveal>();
            commands.trigger(UiInput::<DismissRequest> {
                webview: entity,
                payload: DismissRequest,
            });
            continue;
        }
        match pending.next_frame(
            rendered_open_id,
            native_windowed,
            native_overlay,
            native_size.is_some(),
        ) {
            Some(frames) => pending.frames = frames,
            None => {
                *vis = Visibility::Visible;
                commands.entity(entity).remove::<PendingCommandBarReveal>();
            }
        }
    }
}

fn retry(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    mut query: Query<RetryRow, With<CommandBar>>,
) {
    let now = Instant::now();
    for (entity, mut pending, rendered, recreating) in &mut query {
        if recreating {
            continue;
        }
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let Some(payload) = pending.payload.as_ref() else {
            continue;
        };
        if !pending.should_retry(rendered_open_id) {
            pending.last_retry = None;
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        if pending
            .last_retry
            .is_some_and(|last| now.duration_since(last) < COMMAND_BAR_OPEN_RETRY_INTERVAL)
        {
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<
            vmux_api::command_bar::CommandBarUiState,
        >::from_event(entity, payload));
        pending.started_at.get_or_insert(now);
        pending.last_retry = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandDefinition;
    use crate::{CommandPlugin, ReadCommandRequests};
    use bevy::ecs::schedule::{NodeId, Schedules, SystemSet};
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::command_bar::{CommandBarOpenEvent, CommandBarUiState, CommandBarUiStatePatch};
    use vmux_api::open_target::OpenTarget;
    use vmux_ecs::UiStateWrite;
    use vmux_ecs::launcher::HostsLauncher;
    use vmux_ecs::overlay::OverlayState;

    #[test]
    fn command_bar_mcp_definitions_are_the_dispatchable_command_set() {
        let definitions = crate::CommandManifest::for_feature::<crate::Feature>().into_vec();
        let tools = definitions
            .iter()
            .filter_map(CommandDefinition::agent_tool)
            .filter(|tool| {
                let invocation = CommandInvocation::new(Entity::PLACEHOLDER, &tool.name);
                CommandBarToggleRequest::try_from(&invocation).is_ok()
                    || CommandBarEditPageRequest::try_from(&invocation).is_ok()
                    || CommandBarPathRequest::try_from(&invocation).is_ok()
                    || CommandBarCommandsRequest::try_from(&invocation).is_ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            [
                "command_bar_open",
                "command_bar_edit_page",
                "command_bar_open_path",
                "command_bar_open_commands",
            ],
        );
        for tool in tools {
            let invocation = CommandInvocation::new(Entity::PLACEHOLDER, tool.name);
            match invocation.id.as_str() {
                "command_bar_open" => {
                    assert!(CommandBarToggleRequest::try_from(&invocation).is_ok());
                }
                "command_bar_edit_page" => {
                    assert!(CommandBarEditPageRequest::try_from(&invocation).is_ok());
                }
                "command_bar_open_path" => {
                    assert!(CommandBarPathRequest::try_from(&invocation).is_ok());
                }
                "command_bar_open_commands" => {
                    assert!(CommandBarCommandsRequest::try_from(&invocation).is_ok());
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn build_payload_includes_commands_and_target() {
        let mut world = World::new();
        world.spawn(CommandDefinition {
            id: "test_command".to_string(),
            aliases: Vec::new(),
            label: "Test Command".to_string(),
            group: "Test".to_string(),
            accelerator: None,
            hidden: false,
            native_menu: false,
            shortcut_label: None,
            shortcuts: Vec::new(),
            toolbar: None,
            mcp: None,
        });
        let payload = world
            .run_system_once(|projector: CommandBarProjector| {
                projector.project(CommandBarOpenProjection {
                    open_id: OpenId(7),
                    native_windowed: false,
                    context: Default::default(),
                    url: String::new(),
                    pages: Default::default(),
                    projects: Default::default(),
                    work: Default::default(),
                    locale: Locale::from("en-US"),
                    tabs: Vec::new(),
                    target: Some(OpenTarget::InPlace),
                })
            })
            .expect("payload system runs");
        assert_eq!(payload.open_id, OpenId(7));
        assert_eq!(payload.target, Some(OpenTarget::InPlace));
        assert!(!payload.commands.is_empty());
    }

    #[test]
    fn command_names_localize_every_hierarchy_segment() {
        let browser = CommandDefinition::new("browser_prev_page", "Back", "Browser > Navigation");
        let pane = CommandDefinition::new("close_pane", "Close Pane", "Layout > Pane");
        assert_eq!(
            browser.localized_name("ja"),
            "ブラウザ > ナビゲーション > 戻る"
        );
        assert_eq!(
            pane.localized_name("ja"),
            "レイアウト > ペイン > ペインを閉じる"
        );
    }

    #[test]
    fn command_bar_open_payload_retries_until_rendered_ack() {
        let pending = PendingCommandBarReveal::waiting(0, OpenId(7));
        assert!(pending.should_retry(None));
        assert!(pending.should_retry(Some(OpenId(6))));
        assert!(!pending.should_retry(Some(OpenId(7))));

        let closed = PendingCommandBarReveal::waiting(0, OpenId::NONE);
        assert!(!closed.should_retry(None));

        let empty = PendingCommandBarReveal {
            payload: None,
            ..PendingCommandBarReveal::waiting(0, OpenId(7))
        };
        assert!(!empty.should_retry(None));
    }

    #[derive(Resource, Default)]
    struct CapturedCommandBarOpen(bool);

    type CaptureRow = (
        &'static Node,
        &'static Visibility,
        Has<KeyboardOwner>,
        Has<vmux_ecs::overlay::OverlayShownInline>,
    );

    fn capture_bar_open(
        modal_q: Query<CaptureRow, With<CommandBar>>,
        mut captured: ResMut<CapturedCommandBarOpen>,
    ) {
        captured.0 = OverlayState::from_surfaces(modal_q.iter()).owns_input();
    }

    #[test]
    fn hidden_prewarmed_modal_is_not_command_bar_open() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CapturedCommandBarOpen>()
            .add_systems(Update, capture_bar_open);
        app.world_mut().spawn((
            CommandBar,
            Node {
                display: Display::Flex,
                ..default()
            },
            Visibility::Hidden,
        ));

        app.update();

        assert!(!app.world().resource::<CapturedCommandBarOpen>().0);
    }

    #[test]
    fn command_bar_modal_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prewarm);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                Node {
                    display: Display::None,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();

        app.update();

        let node = app.world().get::<Node>(modal).unwrap();
        let visibility = app.world().get::<Visibility>(modal).unwrap();
        let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

        assert_eq!(node.display, Display::Flex);
        assert_eq!(*visibility, Visibility::Hidden);
        assert_eq!(reveal.open_id, OpenId::NONE);
        assert!(app.world().get::<KeyboardOwner>(modal).is_none());
    }

    #[test]
    fn ready_command_bar_modal_still_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prewarm);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                CommandBarReady,
                Node {
                    display: Display::None,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();

        app.update();

        let node = app.world().get::<Node>(modal).unwrap();
        let visibility = app.world().get::<Visibility>(modal).unwrap();
        let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

        assert_eq!(node.display, Display::Flex);
        assert_eq!(*visibility, Visibility::Hidden);
        assert_eq!(reveal.open_id, OpenId::NONE);
    }

    #[test]
    fn command_bar_reveal_waits_for_matching_open_id() {
        assert_eq!(
            PendingCommandBarReveal::waiting(1, OpenId(7)).next_frame(None, false, false, false),
            Some(2)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(1, OpenId(7)).next_frame(
                Some(OpenId(6)),
                false,
                false,
                false,
            ),
            Some(2)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(0, OpenId(7)).next_frame(
                Some(OpenId(7)),
                false,
                false,
                false,
            ),
            Some(1)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(2, OpenId(7)).next_frame(
                Some(OpenId(7)),
                false,
                false,
                false,
            ),
            None
        );
    }

    #[test]
    fn command_bar_reveal_falls_back_when_rendered_event_is_missing() {
        assert_eq!(
            PendingCommandBarReveal::waiting(0, OpenId(7)).next_frame(None, false, false, false),
            Some(1)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(10, OpenId(7)).next_frame(None, false, false, false),
            None
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(10, OpenId(7)).next_frame(
                Some(OpenId(6)),
                false,
                false,
                false,
            ),
            None
        );
    }

    #[test]
    fn command_bar_reveal_does_not_require_texture_after_rendered_event() {
        assert_eq!(
            PendingCommandBarReveal::waiting(2, OpenId(7)).next_frame(
                Some(OpenId(7)),
                false,
                false,
                false,
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_waits_for_size_and_rendered_ack() {
        assert_eq!(
            PendingCommandBarReveal::waiting(10, OpenId(7)).next_frame(None, true, false, true),
            Some(11)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(10, OpenId(7)).next_frame(
                Some(OpenId(7)),
                true,
                false,
                false,
            ),
            Some(11)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(2, OpenId(7)).next_frame(
                Some(OpenId(7)),
                true,
                false,
                true,
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_aborts_stalled_reveal() {
        let now = Instant::now();
        let mut pending = PendingCommandBarReveal::waiting(0, OpenId(7));
        pending.started_at = Some(now - COMMAND_BAR_NATIVE_REVEAL_TIMEOUT);

        assert!(pending.timed_out(now, None, true, false, false));
        assert!(pending.timed_out(now, Some(OpenId(7)), true, false, false));
        assert!(!pending.timed_out(now, Some(OpenId(7)), true, false, true));
        assert!(!pending.timed_out(now, None, false, false, false));

        pending.started_at =
            Some(now - (COMMAND_BAR_NATIVE_REVEAL_TIMEOUT - Duration::from_millis(1)));
        assert!(!pending.timed_out(now, None, true, false, false));
    }

    #[test]
    fn native_overlay_waits_for_rendered_ack() {
        assert_eq!(
            PendingCommandBarReveal::waiting(10, OpenId(7)).next_frame(None, false, true, false),
            Some(11)
        );
        assert_eq!(
            PendingCommandBarReveal::waiting(2, OpenId(7)).next_frame(
                Some(OpenId(7)),
                false,
                true,
                false,
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_stalled_reveal_stays_hidden() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, reveal);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                WebviewWindowed,
                Visibility::Hidden,
                PendingCommandBarReveal {
                    frames: u8::MAX,
                    open_id: OpenId(7),
                    payload: Some(CommandBarOpenEvent {
                        open_id: OpenId(7),
                        ..Default::default()
                    }),
                    started_at: Some(Instant::now() - COMMAND_BAR_NATIVE_REVEAL_TIMEOUT),
                    last_retry: None,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<PendingCommandBarReveal>(modal).is_none());
        assert_eq!(
            app.world().get::<Visibility>(modal),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn native_command_bar_does_not_timeout_from_rapid_updates() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, reveal);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                WebviewWindowed,
                Visibility::Hidden,
                PendingCommandBarReveal {
                    frames: 0,
                    open_id: OpenId(7),
                    payload: Some(CommandBarOpenEvent {
                        open_id: OpenId(7),
                        ..Default::default()
                    }),
                    started_at: Some(Instant::now()),
                    last_retry: None,
                },
            ))
            .id();

        for _ in 0..256 {
            app.update();
        }

        assert!(app.world().get::<PendingCommandBarReveal>(modal).is_some());
        assert_eq!(
            app.world().get::<Visibility>(modal),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn command_bar_reveal_without_payload_rejects_size() {
        let pending = PendingCommandBarReveal {
            payload: None,
            ..PendingCommandBarReveal::waiting(0, OpenId(7))
        };
        assert!(!pending.accepts_size());
    }

    #[test]
    fn native_command_bar_accepts_hidden_open_size() {
        let pending = PendingCommandBarReveal {
            frames: 0,
            open_id: OpenId(7),
            payload: Some(CommandBarOpenEvent {
                open_id: OpenId(7),
                ..Default::default()
            }),
            started_at: Some(Instant::now()),
            last_retry: None,
        };

        assert!(pending.accepts_size());
        assert_eq!(pending.next_frame(None, true, false, true), Some(1));
    }

    #[derive(Resource, Default)]
    struct EmittedToPage(Vec<(Entity, CommandBarUiStatePatch)>);

    fn capture_page_emit(
        trigger: On<UiStateWrite<CommandBarUiState>>,
        mut emitted: ResMut<EmittedToPage>,
    ) {
        emitted
            .0
            .push((trigger.event().webview(), trigger.event().update().clone()));
    }

    fn panel_app() -> App {
        let mut app = App::new();
        app.add_message::<CommandInvocation>()
            .add_message::<CommandBarToggleRequest>()
            .add_message::<CommandBarEditPageRequest>()
            .add_message::<CommandBarPathRequest>()
            .add_message::<CommandBarCommandsRequest>()
            .add_message::<CommandBarOpenRequest>()
            .add_message::<RestoreKeyboardToStack>()
            .add_message::<LauncherDismissRequest>()
            .init_resource::<EmittedToPage>()
            .add_observer(capture_page_emit)
            .add_observer(close_panel)
            .add_systems(Update, open);
        app.world_mut().spawn((
            CommandBarWorkspaceSnapshot::default(),
            CommandBarProjectRoots::default(),
            CommandBarContextSnapshot::default(),
            CommandBarPagesSnapshot::default(),
            CommandBarWorkSnapshot::default(),
        ));
        app
    }

    fn emitted_to_page(app: &App) -> Vec<Entity> {
        app.world()
            .resource::<EmittedToPage>()
            .0
            .iter()
            .map(|(webview, _)| *webview)
            .collect()
    }

    fn open_payload(app: &App) -> CommandBarOpenEvent {
        let (_, patch) = app
            .world()
            .resource::<EmittedToPage>()
            .0
            .iter()
            .find(|(_, patch)| patch.snapshot.is_some())
            .expect("no open payload emitted");
        let snapshot = patch.snapshot.as_ref().unwrap();
        *snapshot.clone()
    }

    fn send(app: &mut App, id: &str) {
        match id {
            "command_bar_open" => {
                app.world_mut().write_message(CommandBarToggleRequest);
            }
            "command_bar_edit_page" => {
                app.world_mut().write_message(CommandBarEditPageRequest);
            }
            "command_bar_open_path" => {
                app.world_mut().write_message(CommandBarPathRequest);
            }
            "command_bar_open_commands" => {
                app.world_mut().write_message(CommandBarCommandsRequest);
            }
            _ => {
                app.world_mut()
                    .write_message(CommandInvocation::new(Entity::PLACEHOLDER, id));
            }
        }
        app.update();
    }

    #[test]
    fn opening_the_command_bar_pushes_the_payload_to_the_layout_page() {
        let mut app = panel_app();
        let layout = app.world_mut().spawn(RendersLauncherPanel).id();

        send(&mut app, "command_bar_open");

        assert_eq!(emitted_to_page(&app), vec![layout]);
    }

    #[test]
    fn the_start_page_gets_the_same_empty_command_bar_as_every_other_page() {
        let mut app = panel_app();
        let layout = app.world_mut().spawn(RendersLauncherPanel).id();
        let stack = app.world_mut().spawn(()).id();
        app.world_mut().spawn((
            HostsLauncher,
            HostsPage,
            PageMetadata {
                url: "vmux://start/".to_string(),
                ..default()
            },
            ChildOf(stack),
        ));
        app.world_mut()
            .run_system_once(move |mut state: Single<&mut CommandBarWorkspaceSnapshot>| {
                state.stack = Some(stack);
            })
            .unwrap();

        send(&mut app, "command_bar_open");

        assert_eq!(emitted_to_page(&app), vec![layout]);
        assert_eq!(open_payload(&app).url, "");
    }

    #[test]
    fn toggling_an_open_command_bar_asks_the_page_to_close_it() {
        let mut app = panel_app();
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();

        send(&mut app, "command_bar_open");

        assert_eq!(emitted_to_page(&app), vec![layout]);
        assert!(!open_payload(&app).open_id.is_open());
    }

    #[test]
    fn a_surface_opening_under_the_launcher_closes_the_panel_too() {
        let mut app = panel_app();
        app.add_systems(Update, dismiss_requested);
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();
        app.world_mut().write_message(LauncherDismissRequest);

        app.update();

        assert_eq!(
            emitted_to_page(&app),
            vec![layout],
            "the launcher is drawn by the layout page here, so closing only the overlay window \
             leaves it on screen"
        );
        assert!(!open_payload(&app).open_id.is_open());
    }

    #[test]
    fn dismissing_a_layout_page_command_bar_closes_the_panel() {
        let mut app = panel_app();
        app.add_observer(close);
        app.world_mut().spawn((
            CommandBar,
            Node {
                display: Display::None,
                ..default()
            },
            Visibility::Hidden,
        ));
        let layout = app.world_mut().spawn(RendersLauncherPanel).id();

        app.world_mut()
            .trigger(CommandBarDismiss::new(layout, true));
        app.world_mut().flush();

        assert_eq!(emitted_to_page(&app), vec![layout]);
        assert!(!open_payload(&app).open_id.is_open());
    }

    #[test]
    fn open_page_in_command_bar_marks_payload_as_in_place_target() {
        let mut app = panel_app();
        app.world_mut().spawn(RendersLauncherPanel);

        send(&mut app, "command_bar_edit_page");

        assert_eq!(open_payload(&app).target, Some(OpenTarget::InPlace));
    }

    #[test]
    fn dismiss_action_closes_command_bar_modal_in_one_pass() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(Plugin)
            .add_message::<RestoreKeyboardToStack>()
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>();
        app.world_mut()
            .spawn(CommandBarWorkspaceSnapshot::default());

        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Visible,
                KeyboardOwner,
                CommandBarRenderedOpen(OpenId(1)),
            ))
            .id();

        app.world_mut().trigger(UiInput::<DismissRequest> {
            webview: modal,
            payload: DismissRequest,
        });
        app.world_mut().flush();

        let vis_after_close = *app.world().get::<Visibility>(modal).unwrap();
        let display_after_close = app.world().get::<Node>(modal).unwrap().display;
        let has_kb_after_close = app.world().get::<KeyboardOwner>(modal).is_some();
        let has_rendered_after_close = app.world().get::<CommandBarRenderedOpen>(modal).is_some();
        let has_pending_after_close = app.world().get::<PendingCommandBarReveal>(modal).is_some();

        assert_eq!(
            vis_after_close,
            Visibility::Hidden,
            "modal should be hidden after dismiss"
        );
        assert_eq!(
            display_after_close,
            Display::None,
            "modal should have display None after dismiss"
        );
        assert!(
            !has_kb_after_close,
            "KeyboardOwner should be removed after dismiss"
        );
        assert!(
            !has_rendered_after_close,
            "CommandBarRenderedOpen should be cleared after dismiss"
        );
        assert!(
            !has_pending_after_close,
            "PendingCommandBarReveal should be cleared after dismiss"
        );

        app.world_mut().run_system_once(prewarm).unwrap();

        let vis_after_prewarm = *app.world().get::<Visibility>(modal).unwrap();
        let display_after_prewarm = app.world().get::<Node>(modal).unwrap().display;
        let has_kb_after_prewarm = app.world().get::<KeyboardOwner>(modal).is_some();
        let pending_open_id_after_prewarm = app
            .world()
            .get::<PendingCommandBarReveal>(modal)
            .map(|p| p.open_id);

        assert_eq!(
            vis_after_prewarm,
            Visibility::Hidden,
            "modal must stay hidden after prewarm"
        );
        assert!(
            !has_kb_after_prewarm,
            "KeyboardOwner must not return after prewarm"
        );
        assert!(
            !OverlayState::resolve(
                display_after_prewarm,
                Visibility::Hidden,
                has_kb_after_prewarm,
                false
            )
            .owns_input(),
            "is_command_bar_open must report false after dismiss + prewarm"
        );
        if let Some(open_id) = pending_open_id_after_prewarm {
            assert_eq!(
                open_id,
                OpenId::NONE,
                "prewarm should re-arm reveal at OpenId::NONE until open bumps it"
            );
        }
    }

    #[test]
    fn command_bar_open_runs_after_tab_commands() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(Plugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();
        let tab_command_set = graph
            .system_sets
            .get_key(vmux_ecs::workspace::StackCommandSet.intern())
            .unwrap();
        let read_command_systems = graph.systems_in_set(ReadCommandRequests.intern()).unwrap();
        let tab_command_systems = graph
            .systems_in_set(vmux_ecs::workspace::StackCommandSet.intern())
            .unwrap();
        let command_bar_open_system = read_command_systems
            .iter()
            .copied()
            .find(|system| !tab_command_systems.contains(system))
            .unwrap();

        assert!(graph.dependency().graph().contains_edge(
            NodeId::Set(tab_command_set),
            NodeId::System(command_bar_open_system)
        ));
    }

    #[test]
    fn pending_reveal_is_active_only_with_real_open_id() {
        assert!(
            !PendingCommandBarReveal {
                frames: 0,
                open_id: OpenId::NONE,
                payload: None,
                started_at: None,
                last_retry: None,
            }
            .is_active()
        );
        assert!(
            PendingCommandBarReveal {
                frames: 0,
                open_id: OpenId(7),
                payload: None,
                started_at: Some(Instant::now()),
                last_retry: None,
            }
            .is_active()
        );
    }
}
