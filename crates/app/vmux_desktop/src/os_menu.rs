use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::WindowCloseRequested;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::{runtime::Sel, sel};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSMenuItem};
#[cfg(target_os = "macos")]
use objc2_foundation::MainThreadMarker;
use vmux_browser::OpenRequest;
#[cfg(target_os = "macos")]
use vmux_browser::{HostFocusIntent, HostFocusTarget};
#[cfg(target_os = "macos")]
use vmux_command::ReadCommandRequests;
use vmux_command::{
    BindCommands, CommandDefinition, CommandInvocation, DispatchCommandInvocations,
    WriteCommandRequests,
};
#[cfg(target_os = "macos")]
use vmux_ecs::page::BindsEditingChords;
use vmux_ecs::team::User;
use vmux_ecs::wake::Wake;
use vmux_layout::stack::CloseRequest;
use vmux_layout::tab::TabClosed;
use vmux_menu::{
    About, Entry, MenuBar, MenuPlugin, MenuSet, MenuWake, NativeMenuPlugin, Predefined, Selection,
    Separator, Submenu,
};
use vmux_setting::{AppSettings, SettingsLoadSet};
use vmux_ui::i18n::{DEFAULT_LOCALE, Locale, TranslationValue};

pub struct OsMenuPlugin;

impl Plugin for OsMenuPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuPlugin>() {
            app.add_plugins(MenuPlugin);
        }
        if !app.is_plugin_added::<NativeMenuPlugin>() {
            app.add_plugins(NativeMenuPlugin);
        }
        app.add_systems(PreStartup, spawn_state)
            .add_systems(PreUpdate, sync_wake)
            .add_message::<WindowCloseRequested>()
            .add_message::<crate::runtime::HideAllWindowsRequest>()
            .add_message::<crate::window::CloseVmuxWindow>()
            .add_message::<CloseRequest>()
            .add_message::<OpenRequest>()
            .add_observer(remember_tab_close)
            .configure_sets(
                Update,
                (MenuSet::Forward, MenuSet::Dispatch, MenuSet::Cleanup)
                    .chain()
                    .in_set(WriteCommandRequests),
            )
            .add_systems(Startup, setup.after(SettingsLoadSet).after(BindCommands))
            .add_systems(
                Update,
                (
                    sync_locale,
                    remember_stack_close_commands.after(DispatchCommandInvocations),
                    remember_page_open.after(DispatchCommandInvocations),
                    hide_window_on_close_request
                        .after(remember_selection)
                        .after(remember_stack_close_commands)
                        .after(remember_page_open),
                    sync_close_item.after(hide_window_on_close_request),
                ),
            )
            .add_systems(
                Update,
                (
                    remember_selection,
                    dispatch_command_selection,
                    hide_windows_from_selection,
                )
                    .in_set(MenuSet::Dispatch),
            );
        #[cfg(target_os = "macos")]
        app.init_non_send::<EditMenuHandles>()
            .add_systems(Update, sync_edit_items.after(ReadCommandRequests));
    }
}

#[derive(Component)]
struct OsMenuState {
    locale: Locale,
    last_menu_command_at: Option<std::time::Instant>,
    last_stack_close_at: Option<std::time::Instant>,
    last_tab_close_at: Option<std::time::Instant>,
    last_hosted_page_open_at: Option<std::time::Instant>,
    close_item_enabled: bool,
}

impl Default for OsMenuState {
    fn default() -> Self {
        Self {
            locale: Locale::preferred(),
            last_menu_command_at: None,
            last_stack_close_at: None,
            last_tab_close_at: None,
            last_hosted_page_open_at: None,
            close_item_enabled: true,
        }
    }
}

impl OsMenuState {
    const WINDOW_CLOSE_SUPPRESSION: std::time::Duration = std::time::Duration::from_millis(300);
    const HOSTED_PAGE_OPEN_SUPPRESSION: std::time::Duration =
        std::time::Duration::from_millis(1500);
}

#[derive(Component)]
struct HideWindowsMenuEntry;

#[derive(Component)]
struct CommandMenuEntry(Entity);

#[derive(Component)]
struct LocalizedSubmenu(&'static str);

impl LocalizedSubmenu {
    fn message_id(title: &str, locale: &Locale) -> Option<&'static str> {
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
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct EditMenuHandles {
    items: Vec<Retained<NSMenuItem>>,
}

#[cfg(target_os = "macos")]
impl EditMenuHandles {
    fn collect() -> Vec<Retained<NSMenuItem>> {
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
}

fn spawn_state(mut commands: Commands) {
    commands.spawn((Name::new("OS menu runtime"), OsMenuState::default()));
}

fn sync_wake(
    job_wake: Option<Res<vmux_ecs::JobWake>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    menu_wake: Option<Res<MenuWake>>,
    mut commands: Commands,
) {
    let waker = if let Some(job_wake) = job_wake {
        Some(job_wake.0.clone())
    } else {
        Wake::optional_waker(proxy.as_deref().map(|proxy| (**proxy).clone()))
    };
    let Some(waker) = waker else {
        return;
    };
    if menu_wake
        .as_ref()
        .is_some_and(|current| current.0.will_wake(&waker))
    {
        return;
    }
    commands.insert_resource(MenuWake(waker));
}

fn close_product_label(locale: &Locale) -> String {
    locale.translate_with(
        "menu-close-product",
        &[(
            "product",
            TranslationValue::String(vmux_profile::Product::name()),
        )],
    )
}

fn localized_command_leaf(definition: &CommandDefinition, locale: &Locale) -> String {
    let localized = definition.localized_name(locale.as_str());
    localized
        .rsplit(" > ")
        .next()
        .unwrap_or(&localized)
        .to_string()
}

fn setup(
    mut commands: Commands,
    definitions: Query<(Entity, &CommandDefinition)>,
    settings: Option<Res<AppSettings>>,
    mut state: Single<&mut OsMenuState>,
) {
    let locale = settings
        .as_deref()
        .map(|settings| Locale::requested(Some(&settings.appearance.locale)))
        .unwrap_or_else(Locale::preferred);
    let english = Locale::from(DEFAULT_LOCALE);
    state.locale = locale.clone();

    let bar = commands
        .spawn((Name::new("Application menu bar"), MenuBar))
        .id();
    spawn_application_menu(&mut commands, bar, &locale);
    spawn_command_menus(&mut commands, bar, &definitions, &english, &locale);
    spawn_standard_edit_menu(&mut commands, bar, &locale);
}

fn spawn_application_menu(commands: &mut Commands, bar: Entity, locale: &Locale) {
    let app_name = env!("VMUX_BUILD_TITLE").to_string();
    let version = match env!("VMUX_BUILD_PROFILE") {
        "local" | "dev" => format!("v{} ({})", env!("CARGO_PKG_VERSION"), env!("VMUX_GIT_HASH")),
        _ => format!("v{}", env!("CARGO_PKG_VERSION")),
    };
    let submenu = commands.spawn((Submenu::new(app_name), ChildOf(bar))).id();
    commands.spawn((
        Predefined::About,
        About {
            version: Some(version),
            copyright: Some(String::new()),
            ..default()
        },
        ChildOf(submenu),
    ));
    commands.spawn((Separator, ChildOf(submenu)));
    commands.spawn((
        Name::new("Close Vmux menu item"),
        Entry::new(close_product_label(locale), true)
            .with_id("app_quit")
            .with_accelerator("super+q"),
        HideWindowsMenuEntry,
        ChildOf(submenu),
    ));
}

fn spawn_command_menus(
    commands: &mut Commands,
    bar: Entity,
    definitions: &Query<(Entity, &CommandDefinition)>,
    english: &Locale,
    locale: &Locale,
) {
    let mut submenus = HashMap::<(Entity, String), Entity>::new();
    for (command_entity, definition) in definitions {
        if definition.hidden || !definition.native_menu {
            continue;
        }
        let mut parent = bar;
        let mut has_group = false;
        for segment in definition.group.split(" > ") {
            if segment.is_empty() {
                continue;
            }
            has_group = true;
            let key = (parent, segment.to_string());
            parent = if let Some(&existing) = submenus.get(&key) {
                existing
            } else {
                let message_id = LocalizedSubmenu::message_id(segment, english);
                let title = message_id
                    .map(|id| locale.translate(id))
                    .unwrap_or_else(|| segment.to_string());
                let entity = commands.spawn((Submenu::new(title), ChildOf(parent))).id();
                if let Some(id) = message_id {
                    commands.entity(entity).insert(LocalizedSubmenu(id));
                }
                submenus.insert(key, entity);
                entity
            };
        }
        if !has_group {
            continue;
        }
        let mut entry =
            Entry::new(localized_command_leaf(definition, locale), true).with_id(&definition.id);
        if let Some(accelerator) = definition.native_menu_accelerator() {
            entry = entry.with_accelerator(accelerator);
        }
        commands.spawn((entry, CommandMenuEntry(command_entity), ChildOf(parent)));
    }
}

fn spawn_standard_edit_menu(commands: &mut Commands, bar: Entity, locale: &Locale) {
    let edit = commands
        .spawn((
            Submenu::new(locale.translate("menu-edit")),
            LocalizedSubmenu("menu-edit"),
            ChildOf(bar),
        ))
        .id();
    commands.spawn((Predefined::Undo, ChildOf(edit)));
    commands.spawn((Predefined::Redo, ChildOf(edit)));
    commands.spawn((Separator, ChildOf(edit)));
    commands.spawn((Predefined::Cut, ChildOf(edit)));
    commands.spawn((Predefined::Copy, ChildOf(edit)));
    commands.spawn((Predefined::Paste, ChildOf(edit)));
    commands.spawn((Predefined::SelectAll, ChildOf(edit)));
}

fn sync_locale(
    settings: Option<Res<AppSettings>>,
    mut state: Single<&mut OsMenuState>,
    mut submenus: Query<(&mut Submenu, &LocalizedSubmenu)>,
    mut close_entries: Query<&mut Entry, (With<HideWindowsMenuEntry>, Without<CommandMenuEntry>)>,
    mut command_entries: Query<(&mut Entry, &CommandMenuEntry), Without<HideWindowsMenuEntry>>,
    definitions: Query<&CommandDefinition>,
) {
    let Some(settings) = settings else {
        return;
    };
    let locale = Locale::requested(Some(&settings.appearance.locale));
    if state.locale == locale {
        return;
    }
    for (mut submenu, localized) in &mut submenus {
        submenu.set_title(locale.translate(localized.0));
    }
    let close_label = close_product_label(&locale);
    for mut entry in &mut close_entries {
        entry.set_label(close_label.clone());
    }
    for (mut entry, command_entry) in &mut command_entries {
        if let Ok(definition) = definitions.get(command_entry.0) {
            entry.set_label(localized_command_leaf(definition, &locale));
        }
    }
    state.locale = locale;
}

#[cfg(target_os = "macos")]
fn edit_menu_items_enabled(intent: HostFocusIntent, binds_chords: bool) -> bool {
    match intent.kind() {
        HostFocusTarget::WinitHost => false,
        HostFocusTarget::HostedPane => !binds_chords,
        HostFocusTarget::Windowed | HostFocusTarget::LayoutView => true,
    }
}

#[cfg(target_os = "macos")]
fn sync_edit_items(
    mut handles: NonSendMut<EditMenuHandles>,
    intent: Query<Ref<HostFocusIntent>>,
    chord_panes: Query<(), With<BindsEditingChords>>,
) {
    let Ok(intent) = intent.single() else {
        return;
    };
    let collected_now = if handles.items.is_empty() {
        handles.items = EditMenuHandles::collect();
        !handles.items.is_empty()
    } else {
        false
    };
    if !collected_now && !intent.is_changed() {
        return;
    }
    let binds_chords = intent
        .hosted_pane_entity()
        .is_some_and(|pane| chord_panes.contains(pane));
    let enabled = edit_menu_items_enabled(*intent, binds_chords);
    for item in &handles.items {
        item.setEnabled(enabled);
    }
}

fn sync_close_item(
    windows: Query<&Window>,
    mut state: Single<&mut OsMenuState>,
    mut close_entries: Query<&mut Entry, With<HideWindowsMenuEntry>>,
) {
    let any_visible = windows.iter().any(|w| w.visible);
    if state.close_item_enabled == any_visible {
        return;
    }
    state.close_item_enabled = any_visible;
    for mut entry in &mut close_entries {
        entry.set_enabled(any_visible);
    }
}

fn remember_selection(
    mut selections: MessageReader<Selection>,
    mut state: Single<&mut OsMenuState>,
) {
    if selections.read().next().is_some() {
        state.last_menu_command_at = Some(std::time::Instant::now());
    }
}

fn dispatch_command_selection(
    mut selections: MessageReader<Selection>,
    command_entries: Query<&CommandMenuEntry>,
    definitions: Query<&CommandDefinition>,
    users: Query<Entity, With<User>>,
    mut invocations: MessageWriter<CommandInvocation>,
) {
    let caller = users.iter().next().unwrap_or(Entity::PLACEHOLDER);
    for selection in selections.read() {
        let target = command_entries
            .get(selection.target())
            .map(|entry| entry.0)
            .unwrap_or_else(|_| selection.target());
        let Ok(definition) = definitions.get(target) else {
            continue;
        };
        invocations.write(CommandInvocation::new(caller, definition.id.clone()));
    }
}

fn hide_windows_from_selection(
    mut selections: MessageReader<Selection>,
    menu_items: Query<(), With<HideWindowsMenuEntry>>,
    mut hide_windows: MessageWriter<crate::runtime::HideAllWindowsRequest>,
) {
    for selection in selections.read() {
        if menu_items.contains(selection.target()) {
            hide_windows.write(crate::runtime::HideAllWindowsRequest);
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

fn remember_tab_close(_trigger: On<TabClosed>, mut state: Single<&mut OsMenuState>) {
    state.last_tab_close_at = Some(std::time::Instant::now());
}

fn remember_page_open(mut reader: MessageReader<OpenRequest>, mut state: Single<&mut OsMenuState>) {
    for request in reader.read() {
        if request
            .url
            .as_deref()
            .is_some_and(|url| url.starts_with("vmux://"))
        {
            state.last_hosted_page_open_at = Some(std::time::Instant::now());
        }
    }
}

fn hide_window_on_close_request(
    mut closed: MessageReader<WindowCloseRequested>,
    mut windows: Query<&mut Window>,
    mut close_windows: MessageWriter<crate::window::CloseVmuxWindow>,
    state: Single<&OsMenuState>,
) {
    let from_menu_key_equivalent = state
        .last_menu_command_at
        .is_some_and(|t| t.elapsed() < OsMenuState::WINDOW_CLOSE_SUPPRESSION);
    let from_tab_close = state
        .last_tab_close_at
        .is_some_and(|t| t.elapsed() < OsMenuState::WINDOW_CLOSE_SUPPRESSION);
    let from_stack_close = state
        .last_stack_close_at
        .is_some_and(|t| t.elapsed() < OsMenuState::WINDOW_CLOSE_SUPPRESSION);
    let from_hosted_page_open = state
        .last_hosted_page_open_at
        .is_some_and(|t| t.elapsed() < OsMenuState::HOSTED_PAGE_OPEN_SUPPRESSION);
    let window_count = windows.iter().count();
    for event in closed.read() {
        if from_menu_key_equivalent || from_stack_close || from_tab_close || from_hosted_page_open {
            info!(
                target: "vmux_desktop::window_close",
                window = ?event.window,
                from_menu_key_equivalent,
                from_stack_close,
                from_tab_close,
                from_hosted_page_open,
                "suppressed WindowCloseRequested"
            );
            continue;
        }
        if window_count > 1 {
            close_windows.write(crate::window::CloseVmuxWindow(event.window));
            continue;
        }
        if let Ok(mut window) = windows.get_mut(event.window) {
            window.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Waker;

    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::window::Window;
    use vmux_command::CommandPlugin;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_menu::EventSender;
    use vmux_setting::{AgentSettings, AppSettings, BrowserSettings, ShortcutSettings};

    #[cfg(target_os = "macos")]
    #[test]
    fn only_a_pane_that_binds_the_chords_takes_the_menu_away_from_the_platform() {
        let pane = HostFocusIntent::hosted_pane(Entity::PLACEHOLDER);
        let cases = [
            (pane, true, false),
            (pane, false, true),
            (HostFocusIntent::windowed(Entity::PLACEHOLDER), false, true),
            (HostFocusIntent::layout_view(), false, true),
            (HostFocusIntent::winit_host(), false, false),
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
                pane: PaneSettings::default(),
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
        let mut app = App::new();
        let mut settings = test_settings();
        settings.appearance.locale = "en-US".to_string();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .insert_resource(settings);
        app.world_mut().run_schedule(PreStartup);
        for (id, group) in [
            ("layout_cmd", "Layout"),
            ("terminal_cmd", "Terminal"),
            ("browser_cmd", "Browser"),
            ("service_cmd", "Service"),
            ("bookmark_cmd", "Bookmark"),
        ] {
            app.world_mut().spawn(CommandDefinition::new(id, id, group));
        }
        app.world_mut().run_schedule(Startup);

        app.world_mut()
            .resource_mut::<AppSettings>()
            .appearance
            .locale = "ja".to_string();
        app.world_mut().run_schedule(Update);

        let world = app.world_mut();
        let mut titles = world
            .query_filtered::<&Submenu, With<LocalizedSubmenu>>()
            .iter(world)
            .map(|submenu| submenu.title().to_string())
            .collect::<Vec<_>>();
        titles.sort();
        let mut expected = vec![
            "レイアウト".to_string(),
            "ターミナル".to_string(),
            "ブラウザ".to_string(),
            "サービス".to_string(),
            "ブックマーク".to_string(),
            "編集".to_string(),
        ];
        expected.sort();
        assert_eq!(titles, expected);
    }

    #[test]
    fn command_menu_selection_emits_command_invocation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .insert_resource(test_settings());
        app.world_mut().run_schedule(PreStartup);
        let command = app
            .world_mut()
            .spawn(CommandDefinition::new(
                "terminal_next",
                "Next Terminal",
                "Terminal",
            ))
            .id();

        app.world_mut().write_message(Selection::new(command));
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
    fn startup_menu_bar_routes_command_events_and_wakes_job_wake() {
        struct WakeCount(AtomicUsize);

        impl std::task::Wake for WakeCount {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::AcqRel);
            }

            fn wake_by_ref(self: &Arc<Self>) {
                self.0.fetch_add(1, Ordering::AcqRel);
            }
        }

        let mut app = App::new();
        let wakes = Arc::new(WakeCount(AtomicUsize::new(0)));
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .insert_resource(test_settings())
            .insert_resource(vmux_ecs::JobWake(Waker::from(Arc::clone(&wakes))));
        app.world_mut().run_schedule(PreStartup);
        app.world_mut().spawn(CommandDefinition::new(
            "terminal_next",
            "Next Terminal",
            "Terminal",
        ));
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PreUpdate);
        app.world_mut().run_schedule(PreUpdate);

        let sender = app.world().resource::<EventSender>().clone();
        sender.send("terminal_next");
        assert_eq!(wakes.0.load(Ordering::Acquire), 1);

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
        let world = app.world_mut();
        let state = world.query::<&OsMenuState>().single(world).unwrap();
        assert!(state.last_menu_command_at.is_some());
    }

    #[test]
    fn close_menu_selection_emits_hide_windows_request() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .insert_resource(test_settings());
        app.world_mut().run_schedule(PreStartup);
        let close = app.world_mut().spawn(HideWindowsMenuEntry).id();

        app.world_mut().write_message(Selection::new(close));
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
        app.world_mut().run_schedule(PreStartup);

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
        app.world_mut().run_schedule(PreStartup);

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
        app.world_mut().run_schedule(PreStartup);

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut().trigger(vmux_layout::tab::TabClosed);
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn window_close_request_after_hosted_page_open_is_suppressed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());
        app.world_mut().run_schedule(PreStartup);

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<Messages<vmux_browser::OpenRequest>>()
            .write(vmux_browser::OpenRequest::focused(Some(
                "vmux://terminal".to_string(),
            )));
        app.world_mut()
            .resource_mut::<Messages<WindowCloseRequested>>()
            .write(WindowCloseRequested { window });

        app.world_mut().run_schedule(Update);

        assert!(app.world().get::<Window>(window).unwrap().visible);
    }

    #[test]
    fn delayed_window_close_request_after_hosted_page_open_is_suppressed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
            .add_message::<CloseRequest>()
            .add_message::<WindowCloseRequested>()
            .insert_resource(test_settings());
        app.world_mut().run_schedule(PreStartup);

        let window = app.world_mut().spawn(Window::default()).id();
        {
            let world = app.world_mut();
            let mut state = world.query::<&mut OsMenuState>();
            state.single_mut(world).unwrap().last_hosted_page_open_at =
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
        app.world_mut().run_schedule(PreStartup);
        app.world_mut().run_schedule(Startup);

        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut().run_schedule(Update);
        let enabled = {
            let world = app.world_mut();
            let mut entries = world.query_filtered::<&Entry, With<HideWindowsMenuEntry>>();
            entries.single(world).unwrap().enabled()
        };
        assert!(enabled, "a visible window means Close is enabled");

        app.world_mut().get_mut::<Window>(window).unwrap().visible = false;
        app.world_mut().run_schedule(Update);
        let enabled = {
            let world = app.world_mut();
            let mut entries = world.query_filtered::<&Entry, With<HideWindowsMenuEntry>>();
            entries.single(world).unwrap().enabled()
        };
        assert!(!enabled, "all windows hidden means Close is disabled");

        app.world_mut().get_mut::<Window>(window).unwrap().visible = true;
        app.world_mut().run_schedule(Update);
        let enabled = {
            let world = app.world_mut();
            let mut entries = world.query_filtered::<&Entry, With<HideWindowsMenuEntry>>();
            entries.single(world).unwrap().enabled()
        };
        assert!(enabled, "showing a window re-enables Close");
    }
}
