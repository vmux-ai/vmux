extern crate self as vmux_menu;

use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::task::Waker;

use bevy_app::{App, Plugin, PreUpdate, Startup, Update};
use bevy_ecs::prelude::*;
use crossbeam_channel::{Receiver, Sender};

#[cfg(feature = "native")]
pub use native::NativeMenuPlugin;
pub use vmux_macro::menu;

#[doc(hidden)]
pub mod macro_support {
    pub use bevy_ecs::prelude::{ChildOf, Commands, Component, Entity};
}

#[cfg(feature = "native")]
mod native;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<EventSender>() {
            let (sender, receiver) = crossbeam_channel::unbounded();
            let wake = Arc::new(Mutex::new(None));
            app.insert_resource(EventSender { sender, wake })
                .insert_resource(EventInbox(receiver));
        }
        app.add_message::<Selection>()
            .configure_sets(
                Update,
                (MenuSet::Forward, MenuSet::Dispatch, MenuSet::Cleanup).chain(),
            )
            .add_systems(PreUpdate, sync_wake)
            .add_systems(
                Update,
                (
                    forward_events.in_set(MenuSet::Forward),
                    cleanup_transient.in_set(MenuSet::Cleanup),
                ),
            );
    }
}

pub trait MenuAction: Component + Copy + Send + Sync + 'static {
    fn id(self) -> &'static str;

    fn spawn(commands: &mut Commands, about: Option<About>) -> Entity;
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selected<A: MenuAction>(pub A);

pub struct MenuActionPlugin<A: MenuAction> {
    auto_spawn: bool,
    about: Option<About>,
    marker: PhantomData<fn() -> A>,
}

impl<A: MenuAction> Default for MenuActionPlugin<A> {
    fn default() -> Self {
        Self {
            auto_spawn: true,
            about: None,
            marker: PhantomData,
        }
    }
}

impl<A: MenuAction> MenuActionPlugin<A> {
    pub fn with_about(mut self, about: About) -> Self {
        self.about = Some(about);
        self
    }

    pub fn manual_spawn(mut self) -> Self {
        self.auto_spawn = false;
        self
    }
}

impl<A: MenuAction> Plugin for MenuActionPlugin<A> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuPlugin>() {
            app.add_plugins(MenuPlugin);
        }
        app.add_message::<Selected<A>>().add_systems(
            Update,
            forward_actions::<A>
                .in_set(MenuSet::Forward)
                .after(forward_events),
        );
        if self.auto_spawn {
            if let Some(about) = &self.about {
                app.world_mut()
                    .spawn((ActionAbout::<A>::default(), about.clone()));
            }
            app.add_systems(Startup, spawn_actions::<A>);
        }
    }
}

#[derive(Component)]
struct ActionAbout<A: MenuAction>(PhantomData<fn() -> A>);

impl<A: MenuAction> Default for ActionAbout<A> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MenuSet {
    Forward,
    Dispatch,
    Cleanup,
}

#[derive(Resource, Clone)]
pub struct MenuWake(pub Waker);

#[derive(Resource, Clone)]
pub struct EventSender {
    sender: Sender<String>,
    wake: Arc<Mutex<Option<Waker>>>,
}

impl EventSender {
    pub fn send(&self, id: impl Into<String>) {
        let _ = self.sender.send(id.into());
        let waker = self.wake.lock().ok().and_then(|slot| slot.clone());
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn bind_waker(&self, waker: &Waker) {
        let Ok(mut slot) = self.wake.lock() else {
            return;
        };
        if slot
            .as_ref()
            .is_some_and(|current| current.will_wake(waker))
        {
            return;
        }
        *slot = Some(waker.clone());
    }
}

#[derive(Resource)]
struct EventInbox(Receiver<String>);

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuBar;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Submenu {
    title: String,
    enabled: bool,
}

impl Submenu {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            enabled: true,
        }
    }

    pub fn standard_edit(title: impl Into<String>) -> impl Bundle {
        (
            Self::new(title),
            children![
                Predefined::Undo,
                Predefined::Redo,
                Separator,
                Predefined::Cut,
                Predefined::Copy,
                Predefined::Paste,
                Predefined::SelectAll,
            ],
        )
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    id: Option<String>,
    label: String,
    enabled: bool,
    accelerator: Option<String>,
}

impl Entry {
    pub fn new(label: impl Into<String>, enabled: bool) -> Self {
        Self {
            id: None,
            label: label.into(),
            enabled,
            accelerator: None,
        }
    }

    pub fn identified(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            label: String::new(),
            enabled: true,
            accelerator: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_accelerator(mut self, accelerator: impl Into<String>) -> Self {
        self.accelerator = Some(accelerator.into());
        self
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn matches(&self, event_id: &str) -> bool {
        self.id.as_deref() == Some(event_id)
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn accelerator(&self) -> Option<&str> {
        self.accelerator.as_deref()
    }

    pub fn mark_presented(&mut self, id: impl Into<String>) {
        self.id = Some(id.into());
    }
}

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Separator;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Predefined {
    About,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Maximize,
    Fullscreen,
    Hide,
    HideOthers,
    ShowAll,
    CloseWindow,
    Quit,
    Services,
    BringAllToFront,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct About {
    pub name: Option<String>,
    pub version: Option<String>,
    pub short_version: Option<String>,
    pub copyright: Option<String>,
    pub license: Option<String>,
    pub website: Option<String>,
    pub comments: Option<String>,
    pub credits: Option<String>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextMenu {
    view: usize,
}

impl ContextMenu {
    pub fn new(view: *mut c_void) -> Self {
        Self {
            view: view as usize,
        }
    }

    pub fn view(&self) -> *mut c_void {
        self.view as _
    }
}

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transient;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection(Entity);

impl Selection {
    pub fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub fn target(&self) -> Entity {
        self.0
    }
}

fn sync_wake(wake: Option<Res<MenuWake>>, sender: Res<EventSender>) {
    let Some(wake) = wake else {
        return;
    };
    sender.bind_waker(&wake.0);
}

fn forward_events(
    inbox: Res<EventInbox>,
    entries: Query<(Entity, &Entry)>,
    mut selections: MessageWriter<Selection>,
) {
    for event_id in inbox.0.try_iter() {
        let selected = entries
            .iter()
            .find_map(|(entity, entry)| entry.matches(&event_id).then_some(entity));
        if let Some(entity) = selected {
            selections.write(Selection::new(entity));
        }
    }
}

fn spawn_actions<A: MenuAction>(
    mut commands: Commands,
    abouts: Query<(Entity, &About), With<ActionAbout<A>>>,
) {
    let mut about = None;
    for (entity, metadata) in &abouts {
        about = Some(metadata.clone());
        commands.entity(entity).despawn();
    }
    A::spawn(&mut commands, about);
}

fn forward_actions<A: MenuAction>(
    mut selections: MessageReader<Selection>,
    actions: Query<&A>,
    mut selected: MessageWriter<Selected<A>>,
) {
    for selection in selections.read() {
        if let Ok(&action) = actions.get(selection.target()) {
            selected.write(Selected(action));
        }
    }
}

fn cleanup_transient(
    mut selections: MessageReader<Selection>,
    transient: Query<(), With<Transient>>,
    mut commands: Commands,
) {
    for selection in selections.read() {
        if transient.contains(selection.target()) {
            commands.entity(selection.target()).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct WakeCounter(AtomicUsize);

    impl WakeCounter {
        fn new() -> Arc<Self> {
            Arc::new(Self(AtomicUsize::new(0)))
        }

        fn count(&self) -> usize {
            self.0.load(Ordering::Acquire)
        }
    }

    impl std::task::Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[derive(Resource, Default)]
    struct RecordedSelections(Vec<Entity>);

    fn record_selections(
        mut selections: MessageReader<Selection>,
        mut recorded: ResMut<RecordedSelections>,
    ) {
        for selection in selections.read() {
            recorded.0.push(selection.target());
        }
    }

    #[vmux_menu::menu(standard_edit = "Edit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SampleMenu {
        #[submenu("App")]
        #[predefined(about)]
        #[separator]
        #[item("Quit App", id = "sample_quit", accelerator = "super+q")]
        Quit,
        #[submenu("File > Recent")]
        #[item("Open...", accelerator = "super+o")]
        Open,
    }

    #[derive(Resource, Default)]
    struct RecordedActions(Vec<SampleMenu>);

    fn record_actions(
        mut actions: MessageReader<Selected<SampleMenu>>,
        mut recorded: ResMut<RecordedActions>,
    ) {
        for Selected(action) in actions.read() {
            recorded.0.push(*action);
        }
    }

    #[test]
    fn event_sender_wakes_and_forwards_selection_in_order() {
        let mut app = App::new();
        let wakes = WakeCounter::new();
        app.add_plugins(MenuPlugin)
            .insert_resource(MenuWake(Waker::from(Arc::clone(&wakes))))
            .init_resource::<RecordedSelections>()
            .add_systems(Update, record_selections.in_set(MenuSet::Dispatch));

        let bar = app.world_mut().spawn(MenuBar).id();
        let file = app
            .world_mut()
            .spawn((Submenu::new("File"), ChildOf(bar)))
            .id();
        let open = app
            .world_mut()
            .spawn((
                Entry::new("Open", true)
                    .with_id("file_open")
                    .with_accelerator("super+o"),
                ChildOf(file),
            ))
            .id();
        let transient = app
            .world_mut()
            .spawn((
                Entry::new("Ephemeral", true).with_id("ctx_ephemeral"),
                Transient,
            ))
            .id();

        app.update();
        let sender = app.world().resource::<EventSender>().clone();
        sender.send("file_open");
        sender.send("unknown_id");
        sender.send("ctx_ephemeral");
        assert_eq!(wakes.count(), 3);

        app.update();
        assert_eq!(
            app.world().resource::<RecordedSelections>().0,
            vec![open, transient]
        );
        assert!(app.world().get_entity(open).is_ok());
        assert!(app.world().get_entity(transient).is_err());
    }

    #[test]
    fn declarative_menu_macro_spawns_tree_and_dispatches_typed_actions() {
        let mut app = App::new();
        app.add_plugins(MenuActionPlugin::<SampleMenu>::default().with_about(About {
            name: Some("Sample".into()),
            version: Some("1.0.0".into()),
            ..Default::default()
        }))
        .init_resource::<RecordedActions>()
        .add_systems(Update, record_actions.in_set(MenuSet::Dispatch));

        app.update();

        let world = app.world_mut();
        let about = world.query::<&About>().single(world).unwrap();
        assert_eq!(about.name.as_deref(), Some("Sample"));
        let predefined_count = world.query::<&Predefined>().iter(world).count();
        assert_eq!(predefined_count, 7);

        let sender = app.world().resource::<EventSender>().clone();
        sender.send(SampleMenu::Open.id());
        sender.send(SampleMenu::Quit.id());

        app.update();
        assert_eq!(
            app.world().resource::<RecordedActions>().0,
            vec![SampleMenu::Open, SampleMenu::Quit]
        );
    }
}
