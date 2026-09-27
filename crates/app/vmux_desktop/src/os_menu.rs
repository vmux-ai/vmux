use bevy::prelude::*;
use bevy::window::WindowCloseRequested;
use crossbeam_channel::Receiver;
#[cfg(target_os = "macos")]
use muda::ContextMenu;
use muda::{Menu, MenuEvent, MenuItem, MenuItemKind};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::{runtime::Sel, sel};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSMenuItem};
#[cfg(target_os = "macos")]
use objc2_foundation::MainThreadMarker;
#[cfg(target_os = "macos")]
use vmux_browser::HostFocusIntent;
#[cfg(target_os = "macos")]
use vmux_command::ReadCommandRequests;
use vmux_command::{CommandDefinition, CommandInvocation, WriteCommandRequests};
use vmux_layout::stack::CloseRequest;
use vmux_ui::i18n::{DEFAULT_LOCALE, Locale};

pub struct OsMenuPlugin;

impl Plugin for OsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .spawn((Name::new("OS menu runtime"), OsMenuState::default()));
        app.add_plugins(crate::bookmark::BookmarkMenuPlugin)
            .add_message::<OsMenuSelection>()
            .add_message::<crate::window_manager::CloseVmuxWindow>()
            .add_message::<CloseRequest>()
            .add_message::<vmux_browser::OpenRequest>()
            .add_observer(remember_tab_close)
            .configure_sets(
                Update,
                (
                    OsMenuSet::Forward,
                    OsMenuSet::Dispatch,
                    OsMenuSet::Cleanup,
                )
                    .chain()
                    .in_set(WriteCommandRequests),
            )
            .add_systems(
                Startup,
                setup
                    .after(vmux_setting::SettingsLoadSet)
                    .after(vmux_command::RegisterCommandDefinitions),
            )
            .add_systems(
                Update,
                (
                    sync_menu_locale,
                    remember_stack_close_commands.after(vmux_command::DispatchCommandInvocations),
                    remember_native_page_open_requests
                        .after(vmux_command::DispatchCommandInvocations),
                    hide_window_on_close_request
                        .after(remember_stack_close_commands)
                        .after(remember_native_page_open_requests),
                    sync_close_menu_item.after(hide_window_on_close_request),
                ),
            )
            .add_systems(
                Update,
                (
                    forward_menu_events.in_set(OsMenuSet::Forward),
                    (dispatch_command_menu_selection, hide_windows_from_menu)
                        .in_set(OsMenuSet::Dispatch),
                    cleanup_transient_menu_entries.in_set(OsMenuSet::Cleanup),
                ),
            );
        #[cfg(target_os = "macos")]
        app.add_systems(Update, sync_edit_menu_items.after(ReadCommandRequests))
            .add_systems(PostUpdate, present_context_menus);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum OsMenuSet {
    Forward,
    Dispatch,
    Cleanup,
}

#[derive(Component)]
struct OsMenuState {
    last_menu_command_at: Option<std::time::Instant>,
    last_stack_close_at: Option<std::time::Instant>,
    last_tab_close_at: Option<std::time::Instant>,
    last_native_page_open_at: Option<std::time::Instant>,
    close_item_enabled: bool,
}

impl Default for OsMenuState {
    fn default() -> Self {
        Self {
            last_menu_command_at: None,
            last_stack_close_at: None,
            last_tab_close_at: None,
            last_native_page_open_at: None,
            close_item_enabled: true,
        }
    }
}

#[derive(Component)]
pub(crate) struct OsMenuEntry {
    id: Option<String>,
    #[cfg(target_os = "macos")]
    label: String,
    #[cfg(target_os = "macos")]
    enabled: bool,
}

impl OsMenuEntry {
    #[cfg(target_os = "macos")]
    pub(crate) fn new(label: String, enabled: bool) -> Self {
        Self {
            id: None,
            label,
            enabled,
        }
    }

    pub(crate) fn identified(id: String) -> Self {
        Self {
            id: Some(id),
            #[cfg(target_os = "macos")]
            label: String::new(),
            #[cfg(target_os = "macos")]
            enabled: true,
        }
    }

    fn matches(&self, event_id: &str) -> bool {
        self.id.as_deref() == Some(event_id)
    }
}

#[cfg(target_os = "macos")]
#[derive(Component)]
pub(crate) struct OsContextMenu {
    view: usize,
}

#[cfg(target_os = "macos")]
impl OsContextMenu {
    pub(crate) fn new(view: *mut std::ffi::c_void) -> Self {
        Self {
            view: view as usize,
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Component)]
pub(crate) struct OsMenuSeparator;

#[derive(Component)]
struct TransientOsMenuEntry;

#[derive(Component)]
struct HideWindowsMenuEntry;

#[derive(Message, Clone, Copy)]
pub(crate) struct OsMenuSelection(Entity);

impl OsMenuSelection {
    pub(crate) fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub(crate) fn target(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Clone)]
struct OsMenuInbox(Receiver<String>);

const WINDOW_CLOSE_SUPPRESSION_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
const NATIVE_PAGE_OPEN_CLOSE_SUPPRESSION_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1500);

struct OsMenuResource {
    menu: Menu,
    #[cfg(target_os = "macos")]
    context_menu: Option<Menu>,
    locale: Locale,
    close_window: Option<MenuItem>,
    #[cfg(target_os = "macos")]
    edit_items: Vec<Retained<NSMenuItem>>,
}

fn setup(world: &mut World) {
    let definitions = {
        let mut query = world.query::<&CommandDefinition>();
        query.iter(world).cloned().collect::<Vec<_>>()
    };
    let mut menu = Menu::new();
    append_application_menu(&menu).unwrap();
    CommandDefinition::append_native_menus(&definitions, &mut menu).unwrap();
    append_standard_edit_menu(&menu);
    let locale = world
        .get_resource::<vmux_setting::AppSettings>()
        .map(|settings| Locale::requested(Some(&settings.appearance.locale)))
        .unwrap_or_else(Locale::preferred);
    localize_root_menu(&menu, &Locale::from(DEFAULT_LOCALE), &locale, &definitions);
    let close_window = find_menu_item(menu.items(), "app_quit");

    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();
    #[cfg(target_os = "macos")]
    let edit_items = collect_edit_menu_items();

    let proxy = world
        .get_resource::<bevy::winit::EventLoopProxyWrapper>()
        .map(|w| (**w).clone());

    let (menu_events, inbox) = crossbeam_channel::unbounded();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_events.send(event.id.0.clone());
        if let Some(proxy) = &proxy {
            let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }));

    let mut runtime = world.query_filtered::<Entity, With<OsMenuState>>();
    let runtime = runtime.single(world).unwrap();
    world.entity_mut(runtime).insert(OsMenuInbox(inbox));
    world.spawn((
        Name::new("Close Vmux menu item"),
        OsMenuEntry::identified("app_quit".to_string()),
        HideWindowsMenuEntry,
    ));
    let command_entries = {
        let mut query = world.query::<(Entity, &CommandDefinition)>();
        query
            .iter(world)
            .map(|(entity, definition)| (entity, definition.id.clone()))
            .collect::<Vec<_>>()
    };
    for (entity, id) in command_entries {
        world.entity_mut(entity).insert(OsMenuEntry::identified(id));
    }
    world.insert_non_send(OsMenuResource {
        menu,
        #[cfg(target_os = "macos")]
        context_menu: None,
        locale,
        close_window,
        #[cfg(target_os = "macos")]
        edit_items,
    });
}

#[cfg(target_os = "macos")]
fn present_context_menus(
    _non_send: bevy::ecs::system::NonSendMarker,
    mut commands: Commands,
    menu: Option<NonSendMut<OsMenuResource>>,
    context_menus: Query<(&OsContextMenu, &Children), Added<OsContextMenu>>,
    mut entries: Query<&mut OsMenuEntry>,
    separators: Query<(), (With<OsMenuSeparator>, Without<OsMenuEntry>)>,
) {
    let Some(mut menu_resource) = menu else {
        return;
    };
    for (context, children) in &context_menus {
        let menu = Menu::new();
        for child in children.iter() {
            if let Ok(mut entry) = entries.get_mut(child) {
                let id = format!("os_context_menu_{}", child.to_bits());
                let item = MenuItem::with_id(id.clone(), &entry.label, entry.enabled, None);
                let _ = menu.append(&item);
                entry.id = Some(id);
                commands.entity(child).insert(TransientOsMenuEntry);
                continue;
            }
            if separators.contains(child) {
                let _ = menu.append(&muda::PredefinedMenuItem::separator());
            }
        }
        menu_resource.context_menu = Some(menu);
        let Some(menu) = menu_resource.context_menu.as_ref() else {
            continue;
        };
        unsafe {
            menu.show_context_menu_for_nsview(context.view as _, None);
        }
    }
}

fn sync_menu_locale(
    settings: Option<Res<vmux_setting::AppSettings>>,
    menu: Option<NonSendMut<OsMenuResource>>,
    definitions: Query<&CommandDefinition>,
) {
    let (Some(settings), Some(mut menu)) = (settings, menu) else {
        return;
    };
    let locale = Locale::requested(Some(&settings.appearance.locale));
    if menu.locale == locale {
        return;
    }
    let definitions = definitions.iter().cloned().collect::<Vec<_>>();
    localize_root_menu(&menu.menu, &menu.locale, &locale, &definitions);
    menu.locale = locale;
}

fn localize_root_menu(
    menu: &Menu,
    previous_locale: &Locale,
    locale: &Locale,
    definitions: &[CommandDefinition],
) {
    localize_menu_items(menu.items(), previous_locale, locale, definitions);
}

fn localize_menu_items(
    items: Vec<MenuItemKind>,
    previous_locale: &Locale,
    locale: &Locale,
    definitions: &[CommandDefinition],
) {
    for item in items {
        let id = item.id().0.clone();
        if let Some(menu_item) = item.as_menuitem() {
            if id == "app_quit" {
                menu_item.set_text(locale.translate("menu-close-vmux"));
            } else if let Some(definition) =
                definitions.iter().find(|definition| definition.id == id)
            {
                let current = menu_item.text();
                let suffix = current
                    .split_once('\t')
                    .map(|(_, suffix)| format!("\t{suffix}"))
                    .unwrap_or_default();
                let localized = definition.localized_name(locale.as_str());
                let leaf = localized.rsplit(" > ").next().unwrap_or(&localized);
                menu_item.set_text(format!("{leaf}{suffix}"));
            }
        }
        if let Some(submenu) = item.as_submenu() {
            if let Some(title) = localized_submenu_title(&submenu.text(), previous_locale, locale) {
                submenu.set_text(title);
            }
            localize_menu_items(submenu.items(), previous_locale, locale, definitions);
        }
    }
}

fn localized_submenu_title(
    title: &str,
    previous_locale: &Locale,
    locale: &Locale,
) -> Option<String> {
    submenu_message_id(title, previous_locale).map(|message_id| locale.translate(message_id))
}

fn submenu_message_id(title: &str, locale: &Locale) -> Option<&'static str> {
    let english = Locale::from(DEFAULT_LOCALE);
    [
        "menu-scene",
        "menu-layout",
        "menu-terminal",
        "menu-browser",
        "menu-service",
        "menu-bookmark",
        "menu-edit",
        "command-group-interactive-mode",
        "command-group-window",
        "command-group-tab",
        "command-group-pane",
        "command-group-stack",
        "command-group-space",
        "command-group-navigation",
        "command-group-open",
        "command-group-view",
        "command-group-bar",
    ]
    .into_iter()
    .find(|message_id| {
        title == locale.translate(message_id) || title == english.translate(message_id)
    })
}

fn append_standard_edit_menu(menu: &Menu) {
    use muda::{PredefinedMenuItem, Submenu};

    let undo = PredefinedMenuItem::undo(None);
    let redo = PredefinedMenuItem::redo(None);
    let sep = PredefinedMenuItem::separator();
    let cut = PredefinedMenuItem::cut(None);
    let copy = PredefinedMenuItem::copy(None);
    let paste = PredefinedMenuItem::paste(None);
    let select_all = PredefinedMenuItem::select_all(None);
    let Ok(edit) = Submenu::with_items(
        "Edit",
        true,
        &[&undo, &redo, &sep, &cut, &copy, &paste, &select_all],
    ) else {
        return;
    };
    let _ = menu.append(&edit);
}

fn append_application_menu(menu: &Menu) -> Result<(), muda::Error> {
    use muda::{AboutMetadata, PredefinedMenuItem, Submenu};

    let app_name = match env!("VMUX_BUILD_PROFILE") {
        "release" => "Vmux".to_string(),
        "local" => format!("Vmux ({})", env!("VMUX_GIT_HASH")),
        "dev" => format!("Vmux Dev ({})", env!("VMUX_GIT_HASH")),
        other => format!("Vmux ({other})"),
    };
    let version = match env!("VMUX_BUILD_PROFILE") {
        "local" | "dev" => format!("v{} ({})", env!("CARGO_PKG_VERSION"), env!("VMUX_GIT_HASH")),
        _ => format!("v{}", env!("CARGO_PKG_VERSION")),
    };
    let submenu = Submenu::new(app_name, true);
    let quit = MenuItem::with_id(
        "app_quit",
        "Close Vmux",
        true,
        Some("super+q".parse().unwrap()),
    );
    submenu.append_items(&[
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                version: Some(version),
                copyright: Some(String::new()),
                ..default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    menu.append(&submenu)
}

#[cfg(target_os = "macos")]
fn collect_edit_menu_items() -> Vec<Retained<NSMenuItem>> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    let Some(main_menu) = NSApplication::sharedApplication(mtm).mainMenu() else {
        return Vec::new();
    };
    let selectors: [Sel; 6] = [
        sel!(undo:),
        sel!(redo:),
        sel!(cut:),
        sel!(copy:),
        sel!(paste:),
        sel!(selectAll:),
    ];
    let mut items = Vec::new();
    for top in 0..main_menu.numberOfItems() {
        let Some(submenu) = main_menu.itemAtIndex(top).and_then(|item| item.submenu()) else {
            continue;
        };
        let mut found = false;
        for idx in 0..submenu.numberOfItems() {
            let Some(item) = submenu.itemAtIndex(idx) else {
                continue;
            };
            if item
                .action()
                .is_some_and(|selector| selectors.contains(&selector))
            {
                items.push(item);
                found = true;
            }
        }
        if found {
            submenu.setAutoenablesItems(false);
        }
    }
    items
}

#[cfg(target_os = "macos")]
fn edit_menu_items_enabled(intent: HostFocusIntent, binds_chords: bool) -> bool {
    match intent {
        HostFocusIntent::WinitHost => false,
        HostFocusIntent::NativePane(_) => !binds_chords,
        HostFocusIntent::Windowed(_) | HostFocusIntent::LayoutView => true,
    }
}

#[cfg(target_os = "macos")]
fn sync_edit_menu_items(
    menu: Option<NonSend<OsMenuResource>>,
    intent: Option<Res<HostFocusIntent>>,
    chord_panes: Query<(), With<vmux_core::host::page::BindsEditingChords>>,
) {
    let Some(intent) = intent else {
        return;
    };
    if !intent.is_changed() {
        return;
    }
    let Some(menu) = menu else {
        return;
    };
    let binds_chords = match *intent {
        HostFocusIntent::NativePane(pane) => chord_panes.contains(pane),
        _ => false,
    };
    let enabled = edit_menu_items_enabled(*intent, binds_chords);
    for item in &menu.edit_items {
        item.setEnabled(enabled);
    }
}

fn find_menu_item(items: Vec<MenuItemKind>, id: &str) -> Option<MenuItem> {
    for item in items {
        if item.id().0 == id
            && let Some(menu_item) = item.as_menuitem()
        {
            return Some(menu_item.clone());
        }
        if let Some(submenu) = item.as_submenu()
            && let Some(menu_item) = find_menu_item(submenu.items(), id)
        {
            return Some(menu_item);
        }
    }
    None
}

fn sync_close_menu_item(
    menu: Option<NonSend<OsMenuResource>>,
    windows: Query<&Window>,
    mut state: Single<&mut OsMenuState>,
) {
    let any_visible = windows.iter().any(|w| w.visible);
    if state.close_item_enabled == any_visible {
        return;
    }
    state.close_item_enabled = any_visible;
    if let Some(menu) = menu
        && let Some(item) = &menu.close_window
    {
        item.set_enabled(any_visible);
    }
}

fn forward_menu_events(
    inbox: Option<Single<&OsMenuInbox>>,
    menu_entries: Query<(Entity, &OsMenuEntry)>,
    mut state: Single<&mut OsMenuState>,
    mut selections: MessageWriter<OsMenuSelection>,
) {
    let Some(inbox) = inbox else {
        return;
    };
    let drained = inbox.0.try_iter().collect::<Vec<_>>();
    if drained.is_empty() {
        return;
    }

    state.last_menu_command_at = Some(std::time::Instant::now());
    for event_id in drained {
        let selected = menu_entries
            .iter()
            .find_map(|(entity, entry)| entry.matches(&event_id).then_some(entity));
        if let Some(entity) = selected {
            selections.write(OsMenuSelection::new(entity));
        }
    }
}

fn dispatch_command_menu_selection(
    mut selections: MessageReader<OsMenuSelection>,
    definitions: Query<&CommandDefinition>,
    users: Query<Entity, With<vmux_core::team::User>>,
    mut invocations: MessageWriter<CommandInvocation>,
) {
    let caller = users.iter().next().unwrap_or(Entity::PLACEHOLDER);
    for selection in selections.read() {
        let Ok(definition) = definitions.get(selection.target()) else {
            continue;
        };
        invocations.write(CommandInvocation::new(caller, definition.id.clone()));
    }
}

fn hide_windows_from_menu(
    mut selections: MessageReader<OsMenuSelection>,
    menu_items: Query<(), With<HideWindowsMenuEntry>>,
    mut hide_windows: MessageWriter<crate::runtime::HideAllWindowsRequest>,
) {
    for selection in selections.read() {
        if menu_items.contains(selection.target()) {
            hide_windows.write(crate::runtime::HideAllWindowsRequest);
        }
    }
}

fn cleanup_transient_menu_entries(
    mut selections: MessageReader<OsMenuSelection>,
    transient: Query<(), With<TransientOsMenuEntry>>,
    mut commands: Commands,
) {
    for selection in selections.read() {
        if transient.contains(selection.target()) {
            commands.entity(selection.target()).despawn();
        }
    }
}

fn remember_stack_close_commands(
    mut reader: MessageReader<CloseRequest>,
    mut state: Single<&mut OsMenuState>,
) {
    for _ in reader.read() {
        state.last_stack_close_at = Some(std::time::Instant::now());
    }
}

fn remember_tab_close(
    _trigger: On<vmux_layout::tab::TabClosed>,
    mut state: Single<&mut OsMenuState>,
) {
    state.last_tab_close_at = Some(std::time::Instant::now());
}

fn remember_native_page_open_requests(
    mut reader: MessageReader<vmux_browser::OpenRequest>,
    mut state: Single<&mut OsMenuState>,
) {
    for request in reader.read() {
        if request
            .url
            .as_deref()
            .is_some_and(|url| url.starts_with("vmux://"))
        {
            state.last_native_page_open_at = Some(std::time::Instant::now());
        }
    }
}

fn hide_window_on_close_request(
    mut closed: MessageReader<WindowCloseRequested>,
    mut windows: Query<&mut Window>,
    mut close_windows: MessageWriter<crate::window_manager::CloseVmuxWindow>,
    state: Single<&OsMenuState>,
) {
    let from_menu_key_equivalent = state
        .last_menu_command_at
        .is_some_and(|t| t.elapsed() < WINDOW_CLOSE_SUPPRESSION_WINDOW);
    let from_tab_close = state
        .last_tab_close_at
        .is_some_and(|t| t.elapsed() < WINDOW_CLOSE_SUPPRESSION_WINDOW);
    let from_stack_close = state
        .last_stack_close_at
        .is_some_and(|t| t.elapsed() < WINDOW_CLOSE_SUPPRESSION_WINDOW);
    let from_native_page_open = state
        .last_native_page_open_at
        .is_some_and(|t| t.elapsed() < NATIVE_PAGE_OPEN_CLOSE_SUPPRESSION_WINDOW);
    let window_count = windows.iter().count();
    for event in closed.read() {
        if from_menu_key_equivalent || from_stack_close || from_tab_close || from_native_page_open {
            info!(
                target: "vmux_desktop::window_close",
                window = ?event.window,
                from_menu_key_equivalent,
                from_stack_close,
                from_tab_close,
                from_native_page_open,
                "suppressed WindowCloseRequested"
            );
            continue;
        }
        if window_count > 1 {
            close_windows.write(crate::window_manager::CloseVmuxWindow(event.window));
            continue;
        }
        if let Ok(mut window) = windows.get_mut(event.window) {
            window.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::window::Window;
    use vmux_command::CommandPlugin;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AgentSettings, AppSettings, BrowserSettings, ShortcutSettings};

    #[cfg(target_os = "macos")]
    #[test]
    fn only_a_pane_that_binds_the_chords_takes_the_menu_away_from_the_platform() {
        let pane = HostFocusIntent::NativePane(Entity::PLACEHOLDER);
        let cases = [
            (pane, true, false),
            (pane, false, true),
            (HostFocusIntent::Windowed(Entity::PLACEHOLDER), false, true),
            (HostFocusIntent::LayoutView, false, true),
            (HostFocusIntent::WinitHost, false, false),
        ];

        for (intent, binds_chords, want) in cases {
            assert_eq!(
                edit_menu_items_enabled(intent, binds_chords),
                want,
                "{intent:?} with binds_chords={binds_chords}"
            );
        }
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            update_channel: Default::default(),
            agent: AgentSettings::default(),
            spaces: Default::default(),
            projects: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    #[test]
    fn root_menu_titles_use_requested_locale() {
        let titles = [
            "Layout", "Terminal", "Browser", "Service", "Bookmark", "Edit",
        ]
        .into_iter()
        .filter_map(|title| {
            localized_submenu_title(title, &Locale::from("en-US"), &Locale::from("ja"))
        })
        .collect::<Vec<_>>();
        assert_eq!(
            titles,
            [
                "レイアウト",
                "ターミナル",
                "ブラウザ",
                "サービス",
                "ブックマーク",
                "編集",
            ]
        );
    }

    #[test]
    fn command_menu_selection_emits_command_invocation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .insert_resource(test_settings());
        let command = app
            .world_mut()
            .spawn(CommandDefinition::new(
                "terminal_next",
                "Next Terminal",
                "Terminal",
            ))
            .id();

        app.world_mut()
            .write_message(OsMenuSelection::new(command));
        app.world_mut().run_schedule(Update);

        let invocations = app
            .world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            invocations,
            vec![CommandInvocation::new(Entity::PLACEHOLDER, "terminal_next")]
        );
    }

    #[test]
    fn close_menu_selection_emits_hide_windows_request() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .insert_resource(test_settings());
        let close = app.world_mut().spawn(HideWindowsMenuEntry).id();

        app.world_mut()
            .write_message(OsMenuSelection::new(close));
        app.world_mut().run_schedule(Update);

        let requests = app
            .world_mut()
            .resource_mut::<Messages<crate::runtime::HideAllWindowsRequest>>()
            .drain()
            .count();
        assert_eq!(requests, 1);
    }

    #[test]
    fn unsuppressed_window_close_hides_window() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<vmux_browser::OpenRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(!app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn window_close_request_after_stack_close_command_is_suppressed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn window_close_request_after_tab_close_is_suppressed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut().trigger(vmux_layout::tab::TabClosed);
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn window_close_request_after_native_page_open_is_suppressed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<Messages<vmux_browser::OpenRequest>>()
            .write(vmux_browser::OpenRequest {
                url: Some("vmux://terminal".to_string()),
            });
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn delayed_window_close_request_after_native_page_open_is_suppressed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());

        let window = app.world_mut().spawn(Window::default()).id();
        {
            let world = app.world_mut();
            let mut state = world.query::<&mut OsMenuState>();
            state.single_mut(world).unwrap().last_native_page_open_at =
                Some(std::time::Instant::now() - std::time::Duration::from_millis(1000));
        }
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn close_menu_item_disabled_when_all_windows_hidden() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut().run_schedule(Update);
        let enabled = {
            let world = app.world_mut();
            let mut state = world.query::<&OsMenuState>();
            state.single(world).unwrap().close_item_enabled
        };
        assert!(enabled, "a visible window means Close is enabled");

        app.world_mut().get_mut::<Window>(window).unwrap().visible = false;
        app.world_mut().run_schedule(Update);
        let enabled = {
            let world = app.world_mut();
            let mut state = world.query::<&OsMenuState>();
            state.single(world).unwrap().close_item_enabled
        };
        assert!(!enabled, "all windows hidden means Close is disabled");

        app.world_mut().get_mut::<Window>(window).unwrap().visible = true;
        app.world_mut().run_schedule(Update);
        let enabled = {
            let world = app.world_mut();
            let mut state = world.query::<&OsMenuState>();
            state.single(world).unwrap().close_item_enabled
        };
        assert!(enabled, "showing a window re-enables Close");
    }
}
