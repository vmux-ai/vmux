use crate::{
    CAPTURE_STATE_EVENT, EVENT, PAGE_URL, ShortcutBinding, ShortcutCaptureEvent,
    ShortcutCaptureStateEvent, ShortcutCaptureToken, ShortcutEntry, ShortcutGroup, ShortcutStroke,
    ShortcutUrl, ShortcutsEvent, set_capture_target,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, HostWindow,
};
use std::collections::{BTreeMap, HashMap};
use vmux_command::shortcut::{KeyCombo, KeyContext, Keymap, Shortcut};
use vmux_command::{AppCommand, ResolvedLocale, localized_command_name};
use vmux_core::page::PageReady;
use vmux_core::{PageOpenSet, PageOpenTask, workspace::ComputeFocusSet};
use vmux_layout::native_open::{HostedPage, HostedPagePlugin};
use vmux_layout::stack::FocusedStack;
use vmux_layout::window::host_window_of;
use vmux_ui::i18n::Locale;

pub struct ShortcutPlugin;

impl Plugin for ShortcutPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.init_resource::<ShortcutCaptureTarget>()
            .add_plugins((
                HostedPagePlugin::<Shortcuts>::default(),
                BinEventEmitterPlugin::<(ShortcutCaptureEvent,)>::for_hosts(&["shortcuts"]),
            ))
            .add_observer(send_shortcuts)
            .add_observer(update_shortcut_capture)
            .add_systems(
                Update,
                normalize_shortcut_alias.in_set(PageOpenSet::ResolveTarget),
            )
            .add_systems(Update, sync_shortcut_capture.after(ComputeFocusSet));
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "shortcuts",
    title: "Keyboard Shortcuts",
    title_message_id: Some("shortcuts-title"),
    replaces_command: None,
    keywords: &["keyboard", "shortcut", "keymap", "cheatsheet"],
    icon: Some(vmux_core::BuiltinIcon::Keyboard),
    command_bar: true,
};

#[derive(Component, Default)]
struct Shortcuts;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutCaptureTarget {
    token: Option<ShortcutCaptureToken>,
    generation: u64,
}

impl ShortcutCaptureTarget {
    pub fn target(&self) -> Option<Entity> {
        self.token.map(|token| token.target)
    }

    pub fn is_active(&self) -> bool {
        self.token.is_some()
    }

    pub fn token(&self) -> Option<ShortcutCaptureToken> {
        self.token
    }

    pub fn release(&mut self, token: ShortcutCaptureToken, commands: &mut Commands) {
        if self.token == Some(token) {
            self.replace(None, commands);
        }
    }

    fn replace(&mut self, next: Option<Entity>, commands: &mut Commands) {
        if self.target() == next {
            if let Some(webview) = next {
                ShortcutCaptureStateEvent::emit(commands, webview, true);
            }
            return;
        }
        if let Some(webview) = self.target() {
            ShortcutCaptureStateEvent::emit(commands, webview, false);
        }
        self.generation = self.generation.wrapping_add(1);
        self.token = next.map(|target| ShortcutCaptureToken {
            target,
            generation: self.generation,
        });
        set_capture_target(self.token);
        if let Some(webview) = next {
            ShortcutCaptureStateEvent::emit(commands, webview, true);
        }
    }
}

impl ShortcutCaptureStateEvent {
    fn emit(commands: &mut Commands, webview: Entity, active: bool) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CAPTURE_STATE_EVENT,
            &Self { active },
        ));
    }
}

#[derive(SystemParam)]
struct ShortcutCaptureFocus<'w, 's> {
    views: Query<'w, 's, (Entity, &'static ChildOf), With<Shortcuts>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    host_windows: Query<'w, 's, &'static HostWindow>,
    windows: Query<'w, 's, &'static Window>,
    focus: Option<Res<'w, FocusedStack>>,
}

impl ShortcutCaptureFocus<'_, '_> {
    fn holds(&self, webview: Entity) -> bool {
        let Ok((_, parent)) = self.views.get(webview) else {
            return false;
        };
        if self.focus.as_deref().and_then(|focus| focus.stack) != Some(parent.parent()) {
            return false;
        }
        let Some(window) = host_window_of(webview, &self.child_of, &self.host_windows) else {
            return false;
        };
        self.windows
            .get(window)
            .is_ok_and(|window| window.visible && window.focused)
    }

    fn active(&self) -> Option<Entity> {
        self.views
            .iter()
            .find_map(|(webview, _)| self.holds(webview).then_some(webview))
    }
}

impl HostedPage for Shortcuts {
    const HOST: &'static str = "shortcuts";
    const URL: &'static str = PAGE_URL;
    const TITLE: &'static str = "Keyboard Shortcuts";
}

fn send_shortcuts(
    trigger: On<BinReceive<PageReady>>,
    views: Query<(), With<Shortcuts>>,
    keymap: Res<Keymap>,
    contexts: Query<&KeyContext>,
    locale: Option<Res<ResolvedLocale>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !views.contains(webview) || !browsers.can_emit_to(&webview) {
        return;
    }
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    let context = contexts.get(webview).unwrap_or(KeyContext::NONE);
    let payload = ShortcutsEvent::of(&keymap, context, &locale);
    commands.trigger(BinHostEmitEvent::from_rkyv(webview, EVENT, &payload));
}

fn update_shortcut_capture(
    trigger: On<BinReceive<ShortcutCaptureEvent>>,
    focus: ShortcutCaptureFocus,
    mut target: ResMut<ShortcutCaptureTarget>,
    mut commands: Commands,
) {
    let webview = trigger.event_target();
    if focus.views.get(webview).is_err() {
        return;
    }
    if !trigger.payload.active {
        if target.target() == Some(webview) {
            target.replace(None, &mut commands);
        } else {
            ShortcutCaptureStateEvent::emit(&mut commands, webview, false);
        }
        return;
    }
    if focus.holds(webview) {
        target.replace(Some(webview), &mut commands);
    } else {
        ShortcutCaptureStateEvent::emit(&mut commands, webview, false);
    }
}

fn sync_shortcut_capture(
    focus: ShortcutCaptureFocus,
    mut target: ResMut<ShortcutCaptureTarget>,
    mut commands: Commands,
) {
    let next = focus.active();
    if target.target() == next {
        return;
    }
    target.replace(next, &mut commands);
}

fn normalize_shortcut_alias(mut tasks: Query<&mut PageOpenTask, Changed<PageOpenTask>>) {
    for mut task in &mut tasks {
        if let Some(canonical) = ShortcutUrl::canonical(&task.url) {
            task.url = canonical.to_string();
        }
    }
}

impl ShortcutsEvent {
    fn of(keymap: &Keymap, context: &KeyContext, locale: &Locale) -> Self {
        let mut labels = HashMap::new();
        for (id, fallback) in AppCommand::shortcut_labels() {
            labels.insert(id, localized_command_name(locale.as_str(), id, fallback));
        }

        let mut grouped: BTreeMap<String, BTreeMap<(String, String), Vec<ShortcutBinding>>> =
            BTreeMap::new();
        let view = keymap.in_context(context);
        for binding in keymap.bindings() {
            if AppCommand::from_shortcut_id(&binding.command).is_none() {
                continue;
            }
            let Some(label) = labels.get(binding.command.as_str()) else {
                continue;
            };
            let (group, name) = label
                .split_once(" > ")
                .map(|(group, name)| (group.to_string(), name.to_string()))
                .unwrap_or_else(|| (locale.translate("shortcuts-general"), label.clone()));
            let shortcuts = grouped
                .entry(group)
                .or_default()
                .entry((name, binding.command.clone()))
                .or_default();
            let mut shortcut = ShortcutBinding::of(&binding.shortcut);
            shortcut.resolves = view.resolves(&binding.shortcut, &binding.command);
            if let Some(context) = binding.when.as_ref() {
                shortcut.contexts.push(context.to_string());
            }
            if let Some(existing) = shortcuts
                .iter_mut()
                .find(|existing| existing.strokes == shortcut.strokes)
            {
                existing.resolves |= shortcut.resolves;
                for context in shortcut.contexts {
                    if !existing.contexts.contains(&context) {
                        existing.contexts.push(context);
                    }
                }
            } else {
                shortcuts.push(shortcut);
            }
        }

        let groups = grouped
            .into_iter()
            .map(|(name, entries)| ShortcutGroup {
                name,
                entries: entries
                    .into_iter()
                    .map(|((name, id), shortcuts)| ShortcutEntry {
                        id,
                        name,
                        shortcuts,
                    })
                    .collect(),
            })
            .collect();
        Self {
            groups,
            chord_timeout_ms: keymap.chord_timeout_ms,
        }
    }
}

impl ShortcutBinding {
    fn of(shortcut: &Shortcut) -> Self {
        let strokes = match shortcut {
            Shortcut::Direct(combo) => vec![ShortcutStroke::from_key_combo(combo)],
            Shortcut::Chord(prefix, second) => {
                vec![
                    ShortcutStroke::from_key_combo(prefix),
                    ShortcutStroke::from_key_combo(second),
                ]
            }
        };
        Self {
            label: shortcut.display(),
            strokes,
            resolves: false,
            contexts: Vec::new(),
        }
    }
}

impl ShortcutStroke {
    fn from_key_combo(combo: &KeyCombo) -> Self {
        Self {
            code: combo.code(),
            label: combo.key_label(),
            ctrl: combo.modifiers.ctrl,
            shift: combo.modifiers.shift,
            alt: combo.modifiers.alt,
            super_key: combo.modifiers.super_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::KeyCode;
    use vmux_command::shortcut::{Binding, Modifiers, Source, When};
    use vmux_core::{PageMetadata, PageOpenId, PageOpenTask};
    use vmux_layout::native_open::NativeOpenPlugin;

    #[test]
    fn event_lists_hidden_and_visible_shortcuts() {
        let event = ShortcutsEvent::of(
            &Keymap::defaults(),
            KeyContext::NONE,
            &Locale::from("en-US"),
        );
        let names = event
            .groups
            .iter()
            .flat_map(|group| group.entries.iter())
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name.contains("Rotate Forward")));
        assert!(names.iter().any(|name| name.contains("Open in New Tab")));
    }

    #[test]
    fn event_exposes_chords_as_individual_strokes() {
        let event = ShortcutsEvent::of(
            &Keymap::defaults(),
            KeyContext::NONE,
            &Locale::from("en-US"),
        );
        let shortcut = event
            .bindings()
            .find(|(entry, shortcut)| entry.id == "stack_close" && shortcut.strokes.len() == 2)
            .map(|(_, shortcut)| shortcut)
            .expect("close stack chord");

        assert_eq!(shortcut.strokes[0].code, "KeyG");
        assert!(shortcut.strokes[0].ctrl);
        assert_eq!(shortcut.strokes[1].code, "KeyX");
        assert_eq!(shortcut.strokes[1].keycaps(), ["X"]);
    }

    #[test]
    fn event_marks_the_binding_selected_by_runtime_context() {
        let mut keymap = Keymap::default();
        let combo = KeyCombo {
            key: KeyCode::Escape,
            modifiers: Modifiers::default(),
        };
        keymap.extend(
            Source::Settings,
            [
                Binding {
                    shortcut: Shortcut::Direct(combo.clone()),
                    command: "close_pane".into(),
                    when: When::parse("!chat.selector"),
                },
                Binding {
                    shortcut: Shortcut::Direct(combo),
                    command: "stack_close".into(),
                    when: None,
                },
            ],
        );

        let event = ShortcutsEvent::of(&keymap, KeyContext::NONE, &Locale::from("en-US"));
        let resolving = event
            .resolutions()
            .map(|(entry, _)| entry.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(resolving, ["close_pane"]);
    }

    #[test]
    fn page_open_spawns_shortcuts_view() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(NativeOpenPlugin)
            .add_plugins(ShortcutPlugin);
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: PAGE_URL.to_string(),
            request_id: None,
        });

        app.update();

        let title = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Shortcuts>>()
            .single(app.world())
            .expect("shortcuts webview spawned")
            .title
            .clone();
        assert_eq!(title, "Keyboard Shortcuts");
    }

    #[test]
    fn old_cheatsheet_urls_open_the_shortcuts_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(NativeOpenPlugin)
            .add_plugins(ShortcutPlugin);
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://cheatsheet/".to_string(),
            request_id: None,
        });

        app.update();

        let metadata = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(metadata.url, PAGE_URL);
    }

    #[test]
    fn focused_live_shortcuts_view_automatically_owns_capture() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(ShortcutPlugin);
        let window = app
            .world_mut()
            .spawn(Window {
                focused: true,
                visible: true,
                ..default()
            })
            .id();
        let root = app.world_mut().spawn(HostWindow(window)).id();
        let stack = app.world_mut().spawn(ChildOf(root)).id();
        let page = app.world_mut().spawn((Shortcuts, ChildOf(stack))).id();
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });

        app.update();
        assert_eq!(
            app.world().resource::<ShortcutCaptureTarget>().target(),
            Some(page)
        );
        assert_eq!(
            crate::capture_target().map(|token| token.target),
            Some(page)
        );

        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert_eq!(
            app.world().resource::<ShortcutCaptureTarget>().target(),
            None
        );
        assert_eq!(crate::capture_target(), None);

        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert_eq!(
            app.world().resource::<ShortcutCaptureTarget>().target(),
            Some(page)
        );
        assert_eq!(
            crate::capture_target().map(|token| token.target),
            Some(page)
        );

        app.world_mut().despawn(page);
        app.update();
        assert_eq!(
            app.world().resource::<ShortcutCaptureTarget>().target(),
            None
        );
        assert_eq!(crate::capture_target(), None);
    }
}
