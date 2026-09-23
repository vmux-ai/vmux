use crate::CommandBar;
use crate::build_command_bar_open_payload;
use crate::host::payload::CommandBarPicks;
use std::time::{Duration, Instant};
use vmux_api::command_bar::{CommandBarOpenEvent, CommandBarPick, CommandBarPicker};
pub(crate) use vmux_core::launcher::PendingLaunch;
use vmux_core::launcher::{
    HostsLauncher, InlineTransitionRequested, RendersLauncherPanel, RestoreKeyboardToStack,
    StackInPaneChosen,
};

use crate::command_bar::panel::CommandBarPanelActive;
use crate::command_bar::state::{CommandBarStateQuery, command_bar_state};
use crate::command_bar::work_snapshot::{update_recent_files_snapshot, update_work_dirs_snapshot};
use crate::event::{
    CommandBarPanelCloseEvent, CommandBarReadyEvent, CommandBarRenderedEvent, CommandBarRequest,
    CommandBarSizeEvent, OpenId, SearchEngine, SearchEngineSetting,
};
use crate::open::OpenCommand;
use crate::open_target::OpenTarget;
use crate::snapshot::{
    ClaimedUrl, CommandBarUiState, ContributedCommand, ContributedPage, WriteCommandBarSnapshots,
};
use crate::{
    AppCommand, BrowserBarCommand, BrowserCommand, CommandInvocation, LayoutCommand, PaneCommand,
    ReadAppCommands, SpaceCommand, StackCommand,
};
use bevy::{ecs::message::MessageReader, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::event::space::SpaceRequest;
use vmux_core::host::page::HostsPage;
use vmux_core::page::{SettingsPageSpawnRequest, SpacesPageSpawnRequest};
use vmux_core::terminal::{TerminalSpawnRequest, TerminalSpawnTarget};
use vmux_core::{
    PageMetadata, PageOpenRequest, PageOpenTarget, PendingPrompt, PendingPromptAttachments,
};
use vmux_history::now_millis;
use vmux_ui::i18n::{Locale, TranslationValue};

use crate::ResolvedLocale;
use vmux_core::KeyboardOwner;
use vmux_flex::prelude::*;

pub(crate) use vmux_core::focus_pane_entity;

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingLaunch>()
            .add_message::<vmux_core::ContributedCommandChosen>()
            .add_message::<InlineTransitionRequested>()
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<SettingsPageSpawnRequest>()
            .add_message::<SpacesPageSpawnRequest>()
            .add_plugins(UiEventPlugin::<(
                CommandBarRequest,
                CommandBarReadyEvent,
                CommandBarRenderedEvent,
                CommandBarSizeEvent,
            )>::default())
            .add_observer(on_command_bar_request)
            .add_observer(on_command_bar_ready)
            .add_observer(on_command_bar_rendered)
            .add_observer(on_command_bar_size)
            .add_systems(
                Update,
                prewarm_command_bar_modal.before(CefSystems::CreateAndResize),
            )
            .add_systems(
                Update,
                handle_open_command_bar
                    .in_set(ReadAppCommands)
                    .after(prewarm_command_bar_modal)
                    .after(vmux_core::workspace::TabCommandSet)
                    .after(vmux_core::workspace::StackCommandSet),
            )
            .add_systems(
                Update,
                retry_pending_command_bar_open.after(handle_open_command_bar),
            )
            .add_systems(
                Update,
                (
                    update_work_dirs_snapshot,
                    update_recent_files_snapshot,
                    mirror_project_roots,
                )
                    .in_set(WriteCommandBarSnapshots),
            )
            .add_systems(
                Update,
                deferred_dismiss_modal
                    .after(ReadAppCommands)
                    .before(vmux_core::workspace::ComputeFocusSet),
            )
            .add_systems(
                PostUpdate,
                reveal_command_bar.chain().after(LayoutSystems::Layout),
            );
    }
}

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
    frames: u8,
    open_id: OpenId,
    payload: Option<CommandBarOpenEvent>,
    started_at: Option<Instant>,
}

impl PendingCommandBarReveal {
    pub fn is_active(&self) -> bool {
        self.open_id.is_open()
    }
}

const COMMAND_BAR_REVEAL_FRAMES: u8 = 2;
const COMMAND_BAR_REVEAL_FALLBACK_FRAMES: u8 = 10;
const COMMAND_BAR_NATIVE_REVEAL_TIMEOUT: Duration = Duration::from_secs(2);
const COMMAND_BAR_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(100);

pub fn is_command_bar_open(modal_q: &CommandBarStateQuery) -> bool {
    command_bar_state(modal_q).owns_input()
}

pub fn is_command_bar_visible(modal_q: &CommandBarStateQuery) -> bool {
    command_bar_state(modal_q).is_shown()
}

fn prepare_command_bar_surface(
    modal_node: &mut Node,
    modal_vis: &mut Visibility,
    native_overlay: bool,
) {
    modal_node.display = Display::Flex;
    *modal_vis = if native_overlay {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

fn close_command_bar_surface(
    modal_node: &mut Node,
    modal_vis: &mut Visibility,
    native_overlay: bool,
) {
    if native_overlay {
        prepare_command_bar_surface(modal_node, modal_vis, true);
    } else {
        modal_node.display = Display::None;
        *modal_vis = Visibility::Hidden;
    }
}

fn prewarm_command_bar_modal(
    mut commands: Commands,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<KeyboardOwner>,
            Has<PendingCommandBarReveal>,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
    >,
) {
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
    prepare_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
    commands.entity(modal_e).insert(PendingCommandBarReveal {
        frames: 0,
        open_id: OpenId::NONE,
        payload: None,
        started_at: None,
    });
}

fn next_command_bar_reveal_frames(
    frames: u8,
    open_id: OpenId,
    rendered_open_id: Option<OpenId>,
) -> Option<u8> {
    if !open_id.is_open() {
        return Some(frames);
    }
    if rendered_open_id != Some(open_id) {
        if frames >= COMMAND_BAR_REVEAL_FALLBACK_FRAMES {
            return None;
        }
        return Some(frames + 1);
    }
    if frames >= COMMAND_BAR_REVEAL_FRAMES {
        None
    } else {
        Some(frames + 1)
    }
}

fn next_command_bar_reveal_frames_for_backend(
    native_windowed: bool,
    native_overlay: bool,
    frames: u8,
    open_id: OpenId,
    rendered_open_id: Option<OpenId>,
    has_native_size: bool,
) -> Option<u8> {
    if (native_windowed || native_overlay)
        && open_id.is_open()
        && (rendered_open_id != Some(open_id) || (native_windowed && !has_native_size))
    {
        return Some(frames.saturating_add(1));
    }
    next_command_bar_reveal_frames(frames, open_id, rendered_open_id)
}

fn native_command_bar_reveal_timed_out(
    native_windowed: bool,
    native_overlay: bool,
    elapsed: Duration,
    open_id: OpenId,
    rendered_open_id: Option<OpenId>,
    has_native_size: bool,
) -> bool {
    (native_windowed || native_overlay)
        && open_id.is_open()
        && elapsed >= COMMAND_BAR_NATIVE_REVEAL_TIMEOUT
        && (rendered_open_id != Some(open_id) || (native_windowed && !has_native_size))
}

fn should_retry_command_bar_open_payload(
    open_id: OpenId,
    payload: Option<&CommandBarOpenEvent>,
    rendered_open_id: Option<OpenId>,
) -> bool {
    open_id.is_open() && payload.is_some() && rendered_open_id != Some(open_id)
}

fn on_command_bar_ready(
    trigger: On<BinReceive<CommandBarReadyEvent>>,
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

fn on_command_bar_rendered(
    trigger: On<BinReceive<CommandBarRenderedEvent>>,
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

fn on_command_bar_size(
    trigger: On<BinReceive<CommandBarSizeEvent>>,
    browsers: NonSend<Browsers>,
    state: Query<(
        &Visibility,
        Option<&PendingCommandBarReveal>,
        Option<&CommandBarNativeSize>,
        Has<WebviewWindowed>,
    )>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((visibility, pending_reveal, current_size, native_windowed)) = state.get(webview) else {
        return;
    };
    if !command_bar_size_should_apply(*visibility, pending_reveal) {
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

fn command_bar_size_should_apply(
    visibility: Visibility,
    pending_reveal: Option<&PendingCommandBarReveal>,
) -> bool {
    visibility != Visibility::Hidden
        || pending_reveal
            .is_some_and(|pending| pending.open_id.is_open() && pending.payload.is_some())
}

#[derive(Default)]
struct CommandBarOpenRequest {
    should_toggle: bool,
    should_dismiss: bool,
    should_dismiss_nav: bool,
    replace_active_stack: bool,
    url_override: Option<String>,
    picker: Option<CommandBarPicker>,
}

fn command_bar_open_request(
    commands: impl IntoIterator<Item = AppCommand>,
) -> CommandBarOpenRequest {
    let mut request = CommandBarOpenRequest::default();
    for cmd in commands {
        match cmd {
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)) => {
                request.should_toggle = true;
                request.url_override = Some(String::new());
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar)) => {
                request.should_toggle = true;
                request.replace_active_stack = true;
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPathBar)) => {
                request.should_toggle = true;
                request.url_override = Some("/".to_string());
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommands)) => {
                request.should_toggle = true;
                request.url_override = Some(">".to_string());
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenExBar)) => {
                request.should_toggle = true;
                request.url_override = Some(":".to_string());
            }
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)) => {
                request.should_toggle = true;
                request.picker = Some(CommandBarPicker::Space);
                request.url_override = Some(String::new());
            }
            AppCommand::Browser(BrowserCommand::Bar(bar)) => {
                let Some(picker) = bar.picker() else {
                    continue;
                };
                request.should_toggle = true;
                request.picker = Some(picker);
                request.url_override = Some(String::new());
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close)) => {
                request.should_dismiss = true;
            }
            AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Next | StackCommand::Previous,
            ))
            | AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft
                | PaneCommand::SelectRight
                | PaneCommand::SelectUp
                | PaneCommand::SelectDown,
            )) => {
                request.should_dismiss_nav = true;
            }
            _ => {}
        }
    }
    request
}

fn command_bar_toggle_should_open(is_open: bool, picker: Option<CommandBarPicker>) -> bool {
    !is_open || picker.is_some()
}

fn handle_open_command_bar(
    mut reader: MessageReader<AppCommand>,
    layout_q: Query<
        (Entity, Has<CommandBarPanelActive>, Option<&HostWindow>),
        With<RendersLauncherPanel>,
    >,
    windows: Query<&Window>,
    all_children: Query<&Children>,
    browser_meta: Query<&PageMetadata, Or<(With<WebviewSource>, With<HostsPage>)>>,
    state: Res<CommandBarUiState>,
    mut restore_keyboard: MessageWriter<RestoreKeyboardToStack>,
    contributed_pages: Query<&ContributedPage>,
    contributed_commands: Query<&ContributedCommand>,
    definitions: Query<&crate::CommandDefinition>,
    locale: Option<Res<ResolvedLocale>>,
    mut commands: Commands,
) {
    let Some((layout_e, is_open, _)) = layout_q
        .iter()
        .find(|(_, _, host)| {
            host.is_some_and(|host| windows.get(host.0).is_ok_and(|window| window.focused))
        })
        .or_else(|| layout_q.iter().next())
    else {
        return;
    };
    let focus = &state.workspace;
    let active_stack_count = focus.stack_count;
    let spaces_snapshot = &state.spaces;
    let space_name = spaces_snapshot.active_space_name.clone();
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    let definitions = definitions.iter().cloned().collect::<Vec<_>>();

    let request = command_bar_open_request(reader.read().cloned());
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let replace_active_stack = request.replace_active_stack;
    let url_override = request.url_override;
    let picker = request.picker;

    let toggle_closes = should_toggle && !command_bar_toggle_should_open(is_open, picker);

    if (should_dismiss || toggle_closes) && is_open {
        close_command_bar_panel(layout_e, &mut commands);
        if let Some(stack) = focus.stack {
            restore_keyboard.write(RestoreKeyboardToStack { stack });
        }
        return;
    }

    if should_dismiss_nav && is_open {
        close_command_bar_panel(layout_e, &mut commands);
        return;
    }

    if !should_toggle || toggle_closes {
        return;
    }

    let current_url = if let Some(override_url) = url_override {
        override_url
    } else {
        focus
            .stack
            .and_then(|tab| {
                let Ok(children) = all_children.get(tab) else {
                    return None;
                };
                children.iter().find_map(|e| browser_meta.get(e).ok())
            })
            .map(|meta| meta.url.clone())
            .unwrap_or_default()
    };

    let bar_tabs = focus.tabs.clone();

    let target = replace_active_stack.then_some(crate::open_target::OpenTarget::InPlace);
    let mut payload = build_command_bar_open_payload(
        OpenId(now_millis() as u64),
        false,
        space_name,
        current_url,
        spaces_snapshot,
        &contributed_pages,
        &contributed_commands,
        &state.pages,
        &state.work,
        &locale,
        active_stack_count,
        bar_tabs,
        target,
        &definitions,
    );
    payload.picker = picker;
    if let Some(picker) = picker {
        payload.picks = CommandBarPicks::for_picker(picker, &locale);
    }
    commands.trigger(BinHostEmitEvent::from_event(layout_e, &payload));
}

fn close_command_bar_panel(layout: Entity, commands: &mut Commands) {
    commands.trigger(BinHostEmitEvent::from_event(
        layout,
        &CommandBarPanelCloseEvent,
    ));
}

fn build_open_command(target: Option<OpenTarget>, url: String) -> OpenCommand {
    match target {
        Some(OpenTarget::InPlace) | None => OpenCommand::InPlace { url: Some(url) },
        Some(OpenTarget::InNewStack) => OpenCommand::InNewStack { url: Some(url) },
        Some(OpenTarget::InPane {
            direction,
            target,
            mode,
        }) => OpenCommand::InPane {
            direction,
            target,
            mode,
            url: Some(url),
        },
        Some(OpenTarget::InNewTab) => OpenCommand::InNewTab { url: Some(url) },
        Some(OpenTarget::InNewSpace) => OpenCommand::InNewSpace { url: Some(url) },
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
    if crate::event::is_data_uri(value)
        || (value.contains("://") && crate::event::looks_like_url(value))
    {
        value.to_string()
    } else if crate::event::looks_like_url(value) {
        format!("https://{}", value)
    } else {
        search_engine.search_url(value)
    }
}

fn on_command_bar_request(
    trigger: On<BinReceive<CommandBarRequest>>,
    search_engine: Option<Res<SearchEngineSetting>>,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
    >,
    queries: (
        Query<&ChildOf>,
        Query<(), With<HostsLauncher>>,
        Query<&ContributedPage>,
        Query<&ContributedCommand>,
        Query<&ClaimedUrl>,
    ),
    resources: (Res<CommandBarUiState>, Option<Res<ResolvedLocale>>),
    mut writer_params: ParamSet<(
        MessageWriter<AppCommand>,
        MessageWriter<PageOpenRequest>,
        MessageWriter<TerminalSpawnRequest>,
    )>,
    mut chosen_writer: MessageWriter<vmux_core::ContributedCommandChosen>,
    mut inline_transition: MessageWriter<InlineTransitionRequested>,
    mut stack_chosen: MessageWriter<StackInPaneChosen>,
    mut restore_keyboard: MessageWriter<RestoreKeyboardToStack>,
    mut ex_lines: MessageWriter<crate::host::ExLineSubmitted>,
    mut picked: MessageWriter<crate::host::FileStatusPicked>,
    mut issued: MessageWriter<crate::CommandIssued>,
    mut command_invocations: MessageWriter<CommandInvocation>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (child_of_q, launcher_hosts, contributed_pages, contributed_commands, claimed_urls) =
        queries;
    let (command_bar, locale) = resources;
    let focus = &command_bar.workspace;
    let webview = trigger.event().webview;
    let evt = &trigger.event().payload;
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let terminals_snapshot = &command_bar.terminals;
    let terminal_page_url = terminals_snapshot.terminal_page_url.clone();
    let running_terminals = terminals_snapshot.running.clone();
    let mut custom_keyboard_restore = false;
    let inline_transition_stack = launcher_hosts
        .contains(webview)
        .then(|| child_of_q.get(webview).ok().map(|parent| parent.0))
        .flatten();
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    match evt {
        CommandBarRequest::Prompt {
            text,
            target_url,
            attachments: submitted,
        } => {
            let prompt = text.trim();
            let attachments = submitted
                .iter()
                .filter(|attachment| !attachment.path.is_empty())
                .map(|attachment| vmux_api::protocol::AgentAttachment {
                    path: attachment.path.clone(),
                    name: attachment.name.clone(),
                    mime_type: attachment.mime_type.clone(),
                    size: attachment.size,
                })
                .collect::<Vec<_>>();
            if !prompt.is_empty() || !attachments.is_empty() {
                let focused = focus.stack;
                if let Some(stack) = focused
                    && let Some(url) =
                        ContributedPage::prompt_url(&contributed_pages, target_url.as_deref())
                {
                    if inline_transition_stack == Some(stack)
                        && vmux_api::agent::supports_inline_agent_transition(&url)
                    {
                        inline_transition.write(InlineTransitionRequested { stack, webview });
                        if let Some(proxy) = proxy.as_deref() {
                            let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
                        }
                    }
                    commands
                        .entity(stack)
                        .insert(PendingPrompt(prompt.to_string()));
                    if !attachments.is_empty() {
                        commands
                            .entity(stack)
                            .insert(PendingPromptAttachments(attachments));
                    } else {
                        commands.entity(stack).remove::<PendingPromptAttachments>();
                    }
                    writer_params.p1().write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url,
                        request_id: None,
                    });
                    custom_keyboard_restore = true;
                }
            }
        }
        CommandBarRequest::Open { value, open } => {
            let value = &Home::expanded_file_url(value);
            let expanded = Home::resolve(value);
            let is_path = expanded.exists();

            if is_path {
                let dir = if expanded.is_dir() {
                    &expanded
                } else {
                    expanded.parent().unwrap_or(&expanded)
                };
                if let Some(pane_e) = focus.pane {
                    writer_params.p2().write(TerminalSpawnRequest {
                        cwd: Some(dir.to_path_buf()),
                        target: TerminalSpawnTarget::NewStackInPane(pane_e),
                        metadata: Some(PageMetadata {
                            url: terminal_page_url.clone(),
                            title: locale.translate_with(
                                "command-terminal-path",
                                &[("path", TranslationValue::String(&dir.display().to_string()))],
                            ),
                            ..default()
                        }),
                    });
                    custom_keyboard_restore = true;
                }
            } else {
                let url = normalize_url(
                    value,
                    search_engine.map(|setting| setting.0).unwrap_or_default(),
                );
                let inline_transition = if matches!(open, None | Some(OpenTarget::InPlace))
                    && vmux_api::agent::supports_inline_agent_transition(&url)
                    && let Some(stack) = inline_transition_stack
                {
                    inline_transition.write(InlineTransitionRequested { stack, webview });
                    if let Some(proxy) = proxy.as_deref() {
                        let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
                    }
                    true
                } else {
                    false
                };
                if !inline_transition && ClaimedUrl::contains(&claimed_urls, &url) {
                    if let Some(pane_e) = focus.pane {
                        chosen_writer.write(vmux_core::ContributedCommandChosen {
                            id: url.clone(),
                            stack: None,
                            pane: Some(pane_e),
                        });
                        custom_keyboard_restore = true;
                    }
                } else {
                    let target = *open;
                    let cmd =
                        AppCommand::Browser(BrowserCommand::Open(build_open_command(target, url)));
                    issued.write(crate::CommandIssued {
                        caller,
                        command: cmd.clone(),
                    });
                    writer_params.p0().write(cmd);
                }
            }
        }
        CommandBarRequest::Terminal { value } => {
            let known_terminal = running_terminals.get(value).copied();
            if let Some(entity) = known_terminal {
                focus_pane_entity(entity, &mut commands, &child_of_q);
                custom_keyboard_restore = true;
            } else {
                if value.starts_with(&terminal_page_url) {
                    bevy::log::warn!("no terminal pane for {}; spawning new", value);
                }
                let cwd = if value.is_empty() || value.contains("://") {
                    None
                } else {
                    let expanded = if value.starts_with("~/") {
                        std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(&value[2..]))
                            .unwrap_or_else(|_| std::path::PathBuf::from(&value))
                    } else if value.starts_with('/') {
                        std::path::PathBuf::from(&value)
                    } else {
                        std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(value))
                            .unwrap_or_else(|_| std::path::PathBuf::from(&value))
                    };
                    Some(expanded)
                };
                {
                    let active_pane_opt = focus.pane;
                    if let Some(pane_e) = active_pane_opt {
                        writer_params.p2().write(TerminalSpawnRequest {
                            cwd: cwd.clone(),
                            target: TerminalSpawnTarget::NewStackInPane(pane_e),
                            metadata: Some(PageMetadata {
                                url: terminal_page_url.clone(),
                                title: locale.translate("command-terminal"),
                                ..default()
                            }),
                        });
                    } else {
                        let cmd =
                            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                                url: Some("vmux://terminal/".into()),
                            }));
                        issued.write(crate::CommandIssued {
                            caller,
                            command: cmd.clone(),
                        });
                        writer_params.p0().write(cmd);
                    }
                }
            }
        }
        CommandBarRequest::Command { id, open } => {
            let is_contributed = contributed_commands.iter().any(|command| &command.id == id);
            if is_contributed {
                if let Some(pane) = focus.pane {
                    chosen_writer.write(vmux_core::ContributedCommandChosen {
                        id: id.clone(),
                        stack: None,
                        pane: Some(pane),
                    });
                    custom_keyboard_restore = true;
                }
            } else if let Some(url) = ContributedPage::page_url(&contributed_pages, id) {
                let target = *open;
                let cmd =
                    AppCommand::Browser(BrowserCommand::Open(build_open_command(target, url)));
                issued.write(crate::CommandIssued {
                    caller,
                    command: cmd.clone(),
                });
                writer_params.p0().write(cmd);
                custom_keyboard_restore = true;
            } else {
                command_invocations.write(CommandInvocation::new(caller, id));
            }
        }
        CommandBarRequest::Space { id } => {
            custom_keyboard_restore = true;
            if !id.is_empty() {
                commands.trigger(BinReceive {
                    webview,
                    payload: SpaceRequest::Attach {
                        space_id: id.clone(),
                    },
                });
            }
        }
        CommandBarRequest::SwitchTab { pane, index } => {
            stack_chosen.write(StackInPaneChosen {
                pane_bits: *pane,
                index: *index,
            });
        }
        CommandBarRequest::Ex { line } => {
            ex_lines.write(crate::host::ExLineSubmitted {
                stack: focus.stack,
                line: line.clone(),
            });
        }
        CommandBarRequest::Pick { pick } => {
            if let CommandBarPick::Picker(next) = pick {
                if let Some(bar) = BrowserBarCommand::opening(*next) {
                    let cmd = AppCommand::Browser(BrowserCommand::Bar(bar));
                    issued.write(crate::CommandIssued {
                        caller,
                        command: cmd.clone(),
                    });
                    writer_params.p0().write(cmd);
                }
            } else {
                picked.write(crate::host::FileStatusPicked {
                    stack: focus.stack,
                    pick: pick.clone(),
                });
            }
        }
        CommandBarRequest::Dismiss => {}
    }

    if let Ok((modal_e, mut modal_node, mut modal_vis, native_overlay)) = modal_q.single_mut() {
        close_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
        commands
            .entity(modal_e)
            .remove::<KeyboardOwner>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
    }
    if !custom_keyboard_restore && let Some(stack) = focus.stack {
        restore_keyboard.write(RestoreKeyboardToStack { stack });
    }
}

fn deferred_dismiss_modal(
    mut pending_launch: ResMut<PendingLaunch>,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
    >,
    panel_q: Query<Entity, (With<RendersLauncherPanel>, With<CommandBarPanelActive>)>,
    mut commands: Commands,
) {
    if !pending_launch.dismiss_modal {
        return;
    }
    pending_launch.dismiss_modal = false;
    for layout_e in &panel_q {
        close_command_bar_panel(layout_e, &mut commands);
    }
    if let Ok((modal_e, mut modal_node, mut modal_vis, native_overlay)) = modal_q.single_mut()
        && modal_node.display != Display::None
    {
        close_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
        commands
            .entity(modal_e)
            .remove::<KeyboardOwner>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
    }
}

fn reveal_command_bar(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Visibility,
            &mut PendingCommandBarReveal,
            Option<&CommandBarRenderedOpen>,
            Option<&CommandBarNativeSize>,
            Has<WebviewWindowed>,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
    >,
) {
    for (entity, mut vis, mut pending, rendered, native_size, native_windowed, native_overlay) in
        &mut query
    {
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let elapsed = pending
            .started_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or_default();
        if native_command_bar_reveal_timed_out(
            native_windowed,
            native_overlay,
            elapsed,
            pending.open_id,
            rendered_open_id,
            native_size.is_some(),
        ) {
            commands.entity(entity).remove::<PendingCommandBarReveal>();
            commands.trigger(BinReceive::<CommandBarRequest> {
                webview: entity,
                payload: CommandBarRequest::Dismiss,
            });
            continue;
        }
        match next_command_bar_reveal_frames_for_backend(
            native_windowed,
            native_overlay,
            pending.frames,
            pending.open_id,
            rendered_open_id,
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

fn retry_pending_command_bar_open(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    mut query: Query<
        (
            Entity,
            &mut PendingCommandBarReveal,
            Option<&CommandBarRenderedOpen>,
            Has<CommandBarRecreating>,
        ),
        With<CommandBar>,
    >,
    mut last_emit: Local<std::collections::HashMap<Entity, Instant>>,
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
        if !should_retry_command_bar_open_payload(pending.open_id, Some(payload), rendered_open_id)
        {
            last_emit.remove(&entity);
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        if last_emit
            .get(&entity)
            .is_some_and(|last| now.duration_since(*last) < COMMAND_BAR_OPEN_RETRY_INTERVAL)
        {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_event(entity, payload));
        pending.started_at.get_or_insert(now);
        last_emit.insert(entity, now);
    }
}

fn mirror_project_roots(mut state: ResMut<CommandBarUiState>) {
    if !state.is_changed() || state.work.projects == state.projects.roots {
        return;
    }
    state.work.projects = state.projects.roots.clone();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::CommandBarOpenEvent;
    use crate::event::CommandBarSpace;
    use crate::{CommandPlugin, ReadAppCommands};
    use crate::{command_bar_open_payload, localized_command_name};
    use bevy::ecs::schedule::{NodeId, Schedules, SystemSet};
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::BinEvent;
    use vmux_core::overlay::OverlayState;

    #[test]
    fn build_payload_includes_commands_and_target() {
        let mut world = World::new();
        let payload = world
            .run_system_once(
                |pages: Query<&ContributedPage>, commands: Query<&ContributedCommand>| {
                    let definitions = [crate::CommandDefinition {
                        id: "test_command".to_string(),
                        label: "Test Command".to_string(),
                        group: "Test".to_string(),
                        accelerator: None,
                        hidden: false,
                        native_menu: false,
                        shortcut_label: None,
                        shortcuts: Vec::new(),
                    }];
                    build_command_bar_open_payload(
                        OpenId(7),
                        false,
                        String::new(),
                        String::new(),
                        &Default::default(),
                        &pages,
                        &commands,
                        &Default::default(),
                        &crate::snapshot::CommandBarWorkSnapshot::default(),
                        &Locale::from("en-US"),
                        0,
                        Vec::new(),
                        Some(OpenTarget::InPlace),
                        &definitions,
                    )
                },
            )
            .expect("payload system runs");
        assert_eq!(payload.open_id, OpenId(7));
        assert_eq!(payload.target, Some(OpenTarget::InPlace));
        assert!(!payload.commands.is_empty());
    }

    #[test]
    fn command_names_localize_every_hierarchy_segment() {
        assert_eq!(
            localized_command_name("ja", "browser_prev_page", "fallback".to_string()),
            "ブラウザ > ナビゲーション > 戻る"
        );
        assert_eq!(
            localized_command_name("ja", "close_pane", "fallback".to_string()),
            "レイアウト > ペイン > ペインを閉じる"
        );
    }

    #[test]
    fn command_bar_open_payload_retries_until_rendered_ack() {
        let payload = CommandBarOpenEvent {
            open_id: OpenId(7),
            ..Default::default()
        };
        assert!(should_retry_command_bar_open_payload(
            OpenId(7),
            Some(&payload),
            None
        ));
        assert!(should_retry_command_bar_open_payload(
            OpenId(7),
            Some(&payload),
            Some(OpenId(6))
        ));
        assert!(!should_retry_command_bar_open_payload(
            OpenId(7),
            Some(&payload),
            Some(OpenId(7))
        ));
        assert!(!should_retry_command_bar_open_payload(
            OpenId::NONE,
            Some(&payload),
            None
        ));
        assert!(!should_retry_command_bar_open_payload(
            OpenId(7),
            None,
            None
        ));
    }

    #[derive(Resource, Default)]
    struct CapturedCommandBarOpen(bool);

    fn capture_command_bar_open(
        modal_q: CommandBarStateQuery,
        mut captured: ResMut<CapturedCommandBarOpen>,
    ) {
        captured.0 = is_command_bar_open(&modal_q);
    }

    #[test]
    fn hidden_prewarmed_modal_is_not_command_bar_open() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CapturedCommandBarOpen>()
            .add_systems(Update, capture_command_bar_open);
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
    fn closed_native_overlay_stays_renderable_without_being_open() {
        let mut node = Node::default();
        let mut visibility = Visibility::Hidden;

        close_command_bar_surface(&mut node, &mut visibility, true);

        assert_eq!(node.display, Display::Flex);
        assert_eq!(visibility, Visibility::Visible);
        assert!(!OverlayState::resolve(node.display, visibility, false, false).owns_input());
    }

    #[test]
    fn command_bar_modal_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, prewarm_command_bar_modal);
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
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, prewarm_command_bar_modal);
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
        assert_eq!(next_command_bar_reveal_frames(1, OpenId(7), None), Some(2));
        assert_eq!(
            next_command_bar_reveal_frames(1, OpenId(7), Some(OpenId(6))),
            Some(2)
        );
        assert_eq!(
            next_command_bar_reveal_frames(0, OpenId(7), Some(OpenId(7))),
            Some(1)
        );
        assert_eq!(
            next_command_bar_reveal_frames(2, OpenId(7), Some(OpenId(7))),
            None
        );
    }

    #[test]
    fn command_bar_reveal_falls_back_when_rendered_event_is_missing() {
        assert_eq!(next_command_bar_reveal_frames(0, OpenId(7), None), Some(1));
        assert_eq!(next_command_bar_reveal_frames(10, OpenId(7), None), None);
        assert_eq!(
            next_command_bar_reveal_frames(10, OpenId(7), Some(OpenId(6))),
            None
        );
    }

    #[test]
    fn command_bar_reveal_does_not_require_texture_after_rendered_event() {
        assert_eq!(
            next_command_bar_reveal_frames(2, OpenId(7), Some(OpenId(7))),
            None
        );
        assert_eq!(
            next_command_bar_reveal_frames(2, OpenId(7), Some(OpenId(7))),
            None
        );
    }

    #[test]
    fn native_command_bar_waits_for_size_and_rendered_ack() {
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, false, 10, OpenId(7), None, true),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(
                true,
                false,
                10,
                OpenId(7),
                Some(OpenId(7)),
                false
            ),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(
                true,
                false,
                2,
                OpenId(7),
                Some(OpenId(7)),
                true
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_aborts_stalled_reveal() {
        assert!(!native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT - Duration::from_millis(1),
            OpenId(7),
            None,
            false,
        ));
        assert!(native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            None,
            false,
        ));
        assert!(native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            Some(OpenId(7)),
            false,
        ));
        assert!(!native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            Some(OpenId(7)),
            true,
        ));
        assert!(!native_command_bar_reveal_timed_out(
            false,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            None,
            false,
        ));
    }

    #[test]
    fn native_overlay_waits_for_rendered_ack() {
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(false, true, 10, OpenId(7), None, false),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(
                false,
                true,
                2,
                OpenId(7),
                Some(OpenId(7)),
                false
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_stalled_reveal_stays_hidden() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reveal_command_bar);
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
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reveal_command_bar);
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
    fn native_command_bar_ignores_hidden_prewarm_size() {
        assert!(!command_bar_size_should_apply(Visibility::Hidden, None));
        assert!(command_bar_size_should_apply(Visibility::Visible, None));
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
        };

        assert!(command_bar_size_should_apply(
            Visibility::Hidden,
            Some(&pending)
        ));
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, false, 0, OpenId(7), None, true),
            Some(1)
        );
    }

    #[test]
    fn command_bar_payload_includes_space_name() {
        let payload = command_bar_open_payload(
            OpenId(7),
            false,
            "Work".to_string(),
            "https://example.com".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(payload.space_name, "Work");
        assert_eq!(payload.open_id, OpenId(7));
    }

    #[test]
    fn command_bar_payload_includes_spaces() {
        let spaces = vec![CommandBarSpace {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "Personal".to_string(),
            is_active: true,
            tab_count: 2,
        }];

        let payload = command_bar_open_payload(
            OpenId(8),
            true,
            "Work".to_string(),
            "vmux://spaces/".to_string(),
            spaces.clone(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(payload.spaces, spaces);
        assert!(payload.native_windowed);
    }

    #[test]
    fn space_open_command_opens_space_switch_mode() {
        let request = command_bar_open_request([AppCommand::Layout(LayoutCommand::Space(
            SpaceCommand::Open,
        ))]);

        assert!(request.should_toggle);
        assert_eq!(request.picker, Some(CommandBarPicker::Space));
        assert_eq!(request.url_override, Some(String::new()));
    }

    #[test]
    fn every_status_bar_command_asserts_its_own_picker() {
        for (bar, expected) in [
            (BrowserBarCommand::OpenGotoLine, CommandBarPicker::GotoLine),
            (BrowserBarCommand::OpenIndentation, CommandBarPicker::Indent),
            (
                BrowserBarCommand::OpenLineEnding,
                CommandBarPicker::LineEnding,
            ),
            (BrowserBarCommand::OpenEncoding, CommandBarPicker::Encoding),
            (
                BrowserBarCommand::OpenReopenWithEncoding,
                CommandBarPicker::EncodingReopen,
            ),
            (
                BrowserBarCommand::OpenSaveWithEncoding,
                CommandBarPicker::EncodingSave,
            ),
        ] {
            let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(bar))]);

            assert!(request.should_toggle, "{bar:?}");
            assert_eq!(request.picker, Some(expected), "{bar:?}");
            assert_eq!(
                BrowserBarCommand::opening(expected),
                Some(bar),
                "the round trip is what lets a sub-list re-open the bar"
            );
        }
    }

    #[test]
    fn the_generic_bar_commands_assert_no_picker() {
        for bar in [
            BrowserBarCommand::OpenCommandBar,
            BrowserBarCommand::OpenPathBar,
            BrowserBarCommand::OpenCommands,
            BrowserBarCommand::OpenExBar,
        ] {
            let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(bar))]);

            assert_eq!(request.picker, None, "{bar:?}");
            assert!(request.should_toggle, "{bar:?}");
        }
    }

    #[test]
    fn duplicate_open_is_ignored_while_command_bar_is_visible() {
        assert!(command_bar_toggle_should_open(false, None));
        assert!(!command_bar_toggle_should_open(true, None));
        assert!(command_bar_toggle_should_open(
            true,
            Some(CommandBarPicker::Space)
        ));
        assert!(command_bar_toggle_should_open(
            false,
            Some(CommandBarPicker::Space)
        ));
    }

    #[test]
    fn open_in_new_stack_does_not_dismiss_command_bar() {
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack { url: None },
        ))]);

        assert!(!request.should_dismiss);
    }

    #[test]
    fn open_command_bar_forces_empty_url_override() {
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenCommandBar,
        ))]);

        assert!(request.should_toggle);
        assert_eq!(request.url_override, Some(String::new()));
    }

    #[test]
    fn open_page_in_command_bar_leaves_url_override_unset_so_current_url_is_prefilled() {
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenPageInCommandBar,
        ))]);

        assert!(request.should_toggle);
        assert_eq!(request.url_override, None);
    }

    #[derive(Resource, Default)]
    struct EmittedToPage(Vec<(Entity, &'static str, Vec<u8>)>);

    fn capture_page_emit(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<EmittedToPage>) {
        emitted
            .0
            .push((trigger.webview(), trigger.id(), trigger.payload().to_vec()));
    }

    fn panel_app() -> App {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageOpenRequest>()
            .add_message::<InlineTransitionRequested>()
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .init_resource::<CommandBarUiState>()
            .init_resource::<PendingLaunch>()
            .init_resource::<EmittedToPage>()
            .add_observer(capture_page_emit)
            .add_systems(Update, handle_open_command_bar);
        app
    }

    fn emitted_to_page(app: &App) -> Vec<(Entity, &'static str)> {
        app.world()
            .resource::<EmittedToPage>()
            .0
            .iter()
            .map(|(webview, id, _)| (*webview, *id))
            .collect()
    }

    fn open_payload(app: &App) -> CommandBarOpenEvent {
        let event_id = CommandBarOpenEvent::id();
        let (_, _, bytes) = app
            .world()
            .resource::<EmittedToPage>()
            .0
            .iter()
            .find(|(_, id, _)| id == &event_id)
            .expect("no open payload emitted");
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(bytes)
            .expect("open payload should round-trip")
    }

    fn send(app: &mut App, command: AppCommand) {
        app.world_mut().write_message(command);
        app.update();
    }

    #[test]
    fn opening_the_command_bar_pushes_the_payload_to_the_layout_page() {
        let mut app = panel_app();
        let layout = app.world_mut().spawn(RendersLauncherPanel).id();

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, CommandBarOpenEvent::id())]
        );
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
            .resource_mut::<CommandBarUiState>()
            .workspace
            .stack = Some(stack);

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, CommandBarOpenEvent::id())]
        );
        assert_eq!(open_payload(&app).url, "");
    }

    #[test]
    fn toggling_an_open_command_bar_asks_the_page_to_close_it() {
        let mut app = panel_app();
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, CommandBarPanelCloseEvent::id())]
        );
    }

    #[test]
    fn a_surface_opening_under_the_launcher_closes_the_panel_too() {
        let mut app = panel_app();
        app.add_systems(Update, deferred_dismiss_modal);
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();
        app.world_mut()
            .resource_mut::<PendingLaunch>()
            .dismiss_modal = true;

        app.update();

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, CommandBarPanelCloseEvent::id())],
            "the launcher is drawn by the layout page here, so closing only the overlay window \
             leaves it on screen"
        );
    }

    #[test]
    fn space_switch_reopens_an_already_open_command_bar() {
        let mut app = panel_app();
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();

        send(
            &mut app,
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, CommandBarOpenEvent::id())]
        );
        assert_eq!(open_payload(&app).picker, Some(CommandBarPicker::Space));
    }

    #[test]
    fn open_page_in_command_bar_marks_payload_as_in_place_target() {
        let mut app = panel_app();
        app.world_mut().spawn(RendersLauncherPanel);

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar)),
        );

        assert_eq!(open_payload(&app).target, Some(OpenTarget::InPlace));
    }

    #[test]
    fn dismiss_action_closes_command_bar_modal_in_one_pass() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(InputPlugin)
            .add_message::<TerminalSpawnRequest>()
            .add_message::<InlineTransitionRequested>()
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_message::<vmux_core::terminal::ProcessesMonitorSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>();

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

        app.world_mut().trigger(BinReceive::<CommandBarRequest> {
            webview: modal,
            payload: CommandBarRequest::Dismiss,
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

        app.world_mut()
            .run_system_once(prewarm_command_bar_modal)
            .unwrap();

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
                "prewarm should re-arm reveal at OpenId::NONE (which never fires until handle_open_command_bar bumps it)"
            );
        }
    }

    #[test]
    fn command_bar_open_runs_after_tab_commands() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(InputPlugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();
        let tab_command_set = graph
            .system_sets
            .get_key(vmux_core::workspace::StackCommandSet.intern())
            .unwrap();
        let read_command_systems = graph.systems_in_set(ReadAppCommands.intern()).unwrap();
        let tab_command_systems = graph
            .systems_in_set(vmux_core::workspace::StackCommandSet.intern())
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
    fn build_open_command_none_target_yields_in_place() {
        let cmd = build_open_command(None, "https://example.com".to_string());
        assert_eq!(
            cmd,
            OpenCommand::InPlace {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_place_target_yields_in_place() {
        let cmd = build_open_command(Some(OpenTarget::InPlace), "https://example.com".to_string());
        assert_eq!(
            cmd,
            OpenCommand::InPlace {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_new_stack_target() {
        let cmd = build_open_command(
            Some(OpenTarget::InNewStack),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InNewStack {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_new_tab_target() {
        let cmd = build_open_command(
            Some(OpenTarget::InNewTab),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InNewTab {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_new_space_target() {
        let cmd = build_open_command(
            Some(OpenTarget::InNewSpace),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InNewSpace {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_pane_target() {
        use crate::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
        let cmd = build_open_command(
            Some(OpenTarget::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
            }),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://example.com".to_string()),
            }
        );
    }

    #[test]
    fn a_picked_editor_row_carries_a_path_the_editor_can_open() {
        let home = std::env::var("HOME").expect("a home directory");

        assert_eq!(
            Home::expanded_file_url("file://~/.vmux"),
            format!("file://{home}/.vmux"),
            "the launcher builds this row from what was typed, so the tilde arrives unexpanded"
        );
        assert_eq!(
            Home::expanded_file_url("file:///etc/hosts"),
            "file:///etc/hosts"
        );
        assert_eq!(
            Home::expanded_file_url("https://vmux.ai/~jun"),
            "https://vmux.ai/~jun"
        );
    }

    #[test]
    fn normalize_url_adds_https_for_domain() {
        assert_eq!(
            normalize_url("google.com", SearchEngine::Google),
            "https://google.com"
        );
    }

    #[test]
    fn normalize_url_preserves_explicit_protocol() {
        assert_eq!(
            normalize_url("http://example.com", SearchEngine::Google),
            "http://example.com"
        );
        assert_eq!(
            normalize_url("https://example.com", SearchEngine::Google),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_url_search_query_becomes_google() {
        assert_eq!(
            normalize_url("hello world", SearchEngine::Google),
            "https://www.google.com/search?q=hello+world"
        );
    }

    #[test]
    fn normalize_url_uses_selected_search_engine() {
        assert_eq!(
            normalize_url("hello world", SearchEngine::DuckDuckGo),
            "https://duckduckgo.com/?q=hello+world"
        );
    }

    #[test]
    fn normalize_url_searches_multiline_text_with_embedded_url() {
        let prompt = "Continue DSK-627\nPR: https://github.com/example/repo/pull/1";
        assert_eq!(
            normalize_url(prompt, SearchEngine::Google),
            "https://www.google.com/search?q=Continue+DSK-627%0APR%3A+https%3A%2F%2Fgithub.com%2Fexample%2Frepo%2Fpull%2F1"
        );
    }

    #[test]
    fn normalize_url_preserves_vmux_protocol() {
        assert_eq!(
            normalize_url("vmux://terminal/123", SearchEngine::Google),
            "vmux://terminal/123"
        );
    }

    #[test]
    fn normalize_url_preserves_data_scheme() {
        let data = "data:text/html,<style>body{background:white}</style><h1>x</h1>";
        assert_eq!(normalize_url(data, SearchEngine::Google), data);
    }

    #[test]
    fn pending_reveal_is_active_only_with_real_open_id() {
        assert!(
            !PendingCommandBarReveal {
                frames: 0,
                open_id: OpenId::NONE,
                payload: None,
                started_at: None,
            }
            .is_active()
        );
        assert!(
            PendingCommandBarReveal {
                frames: 0,
                open_id: OpenId(7),
                payload: None,
                started_at: Some(Instant::now()),
            }
            .is_active()
        );
    }
}
