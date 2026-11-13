use bevy::ecs::system::Commands;
use bevy::input::{InputSystems, mouse::MouseButtonInput};
use bevy::prelude::*;
use bevy::window::Window;
use bevy::window::{CursorMoved, WindowFocused};
#[cfg(target_os = "macos")]
use bevy::winit::WINIT_WINDOWS;
use bevy_cef_core::prelude::NativeMouseButtons;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSView;
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use vmux_api::input::KeyModifiers;

pub struct PointerPlugin;

impl Plugin for PointerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerInbox>()
            .add_message::<PointerInput>()
            .add_message::<PointerUpdated>()
            .configure_sets(
                PreUpdate,
                (PointerSet::Receive, PointerSet::Apply, PointerSet::Route).chain(),
            )
            .add_systems(
                PreUpdate,
                (initialize, ApplyDeferred, receive, translate)
                    .chain()
                    .after(InputSystems)
                    .in_set(PointerSet::Receive),
            )
            .add_systems(PreUpdate, apply.in_set(PointerSet::Apply))
            .add_systems(PostUpdate, publish.in_set(PointerSet::Publish));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerSet {
    Receive,
    Apply,
    Route,
    Publish,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PointerState {
    pub cancelled: bool,
    pub position_px: Option<Vec2>,
    pub buttons: NativeMouseButtons,
    pub sequence: u64,
    pub motion_sequence: u64,
    pub modifiers: Option<KeyModifiers>,
    pub press_sequence: u64,
    pub press_position_px: Option<Vec2>,
    pub press_modifiers: Option<KeyModifiers>,
}

impl PointerState {
    pub fn update(&mut self, input: PointerInput) -> PointerUpdated {
        let before = self.buttons;
        self.cancelled = input.cancelled;
        let previous_position = self.position_px;
        if input.cancelled {
            self.position_px = None;
            self.motion_sequence = 0;
            self.modifiers = None;
            self.press_sequence = 0;
            self.press_position_px = None;
            self.press_modifiers = None;
        }
        if !input.cancelled
            && let Some(position) = input.position_px
        {
            self.position_px = Some(position);
        }
        self.buttons = if input.cancelled {
            NativeMouseButtons::default()
        } else {
            input.buttons
        };
        self.sequence = self.sequence.wrapping_add(1).max(1);
        if !input.cancelled && input.modifiers.is_some() {
            self.modifiers = input.modifiers;
        }
        if (self.buttons.left && !before.left)
            || (self.buttons.right && !before.right)
            || (self.buttons.middle && !before.middle)
        {
            self.press_sequence = self.sequence;
            self.press_position_px = self.position_px;
            self.press_modifiers = self.modifiers;
        }
        if input.motion && !input.cancelled {
            self.motion_sequence = self.sequence;
        }
        PointerUpdated {
            native_token: input.native_token,
            window: input.window,
            position_px: if input.cancelled {
                previous_position
            } else {
                self.position_px
            },
            buttons: self.buttons,
            previous_buttons: before,
            sequence: self.sequence,
            motion: input.motion,
            cancelled: input.cancelled,
            capture_owner: None,
            modifiers: self.modifiers,
            press_sequence: self.press_sequence,
            press_position_px: self.press_position_px,
            press_modifiers: self.press_modifiers,
        }
    }
}

#[derive(Component, Default)]
pub struct WindowGesture {
    pub live_resize: bool,
}

#[derive(Component, Clone, Debug, Default)]
pub struct PointerCapture {
    consumed: Vec<u64>,
    suppressed: Vec<u64>,
    routed: Vec<u64>,
    owner: Option<Entity>,
    buttons: NativeMouseButtons,
}

impl PointerCapture {
    pub fn suppress(&mut self, sequence: u64) {
        self.suppressed.push(sequence);
    }

    pub fn suppresses(&self, sequence: u64) -> bool {
        self.suppressed.contains(&sequence)
    }

    pub fn route(&mut self, sequence: u64) {
        self.routed.push(sequence);
    }

    pub fn routed(&self, sequence: u64) -> bool {
        self.routed.contains(&sequence)
    }

    pub fn consume(&mut self, token: u64) {
        if token != 0 {
            self.consumed.push(token);
        }
    }

    pub fn consumes(&self, token: u64) -> bool {
        token != 0 && self.consumed.contains(&token)
    }

    pub fn owner(&self) -> Option<Entity> {
        self.owner
    }

    pub fn claim(&mut self, owner: Entity, button: MouseButton) {
        if self.owner != Some(owner) {
            self.clear();
        }
        self.owner = Some(owner);
        self.set_button(button, true);
    }

    pub fn release(&mut self, button: MouseButton) {
        self.set_button(button, false);
        if !self.buttons.left && !self.buttons.right && !self.buttons.middle {
            self.owner = None;
        }
    }

    pub fn clear(&mut self) {
        self.owner = None;
        self.buttons = NativeMouseButtons::default();
    }

    fn set_button(&mut self, button: MouseButton, pressed: bool) {
        match button {
            MouseButton::Left => self.buttons.left = pressed,
            MouseButton::Right => self.buttons.right = pressed,
            MouseButton::Middle => self.buttons.middle = pressed,
            _ => {}
        }
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct PointerInput {
    pub native_token: u64,
    pub window: Entity,
    pub position_px: Option<Vec2>,
    pub buttons: NativeMouseButtons,
    pub motion: bool,
    pub cancelled: bool,
    pub modifiers: Option<KeyModifiers>,
}

impl PointerInput {
    pub fn sample(
        window: Entity,
        position_px: Vec2,
        buttons: NativeMouseButtons,
        motion: bool,
    ) -> Self {
        Self {
            native_token: 0,
            window,
            position_px: Some(position_px),
            buttons,
            motion,
            cancelled: false,
            modifiers: None,
        }
    }

    pub fn buttons(window: Entity, buttons: NativeMouseButtons) -> Self {
        Self {
            native_token: 0,
            window,
            position_px: None,
            buttons,
            motion: false,
            cancelled: false,
            modifiers: None,
        }
    }

    fn cancel(window: Entity) -> Self {
        Self {
            cancelled: true,
            ..Self::buttons(window, NativeMouseButtons::default())
        }
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct PointerUpdated {
    pub native_token: u64,
    pub capture_owner: Option<Entity>,
    pub window: Entity,
    pub position_px: Option<Vec2>,
    pub buttons: NativeMouseButtons,
    pub previous_buttons: NativeMouseButtons,
    pub sequence: u64,
    pub motion: bool,
    pub cancelled: bool,
    pub modifiers: Option<KeyModifiers>,
    pub press_sequence: u64,
    pub press_position_px: Option<Vec2>,
    pub press_modifiers: Option<KeyModifiers>,
}

#[derive(Resource, Clone, Default)]
pub struct PointerInbox(Arc<Mutex<PointerMailbox>>);

#[derive(Default)]
struct PointerMailbox {
    pending: VecDeque<PointerInput>,
    reset: bool,
    native_enabled: bool,
    origins: Vec<PointerOrigin>,
}

#[derive(Clone, Copy)]
struct PointerOrigin {
    window: Entity,
    native_window: isize,
    held: bool,
    position: Option<Vec2>,
}

#[cfg(target_os = "macos")]
impl PointerOrigin {
    fn from_pointer(window: Entity, native_window: isize, pointer: &PointerState) -> Self {
        Self {
            window,
            native_window,
            held: pointer.buttons.left || pointer.buttons.right || pointer.buttons.middle,
            position: pointer.position_px,
        }
    }
}

impl PointerInbox {
    pub fn set_native_enabled(&self, enabled: bool) {
        let mut mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        mailbox.native_enabled = enabled;
        if !enabled {
            mailbox.origins.clear();
            mailbox.pending.clear();
            mailbox.reset = true;
        }
    }

    pub fn cancel(&self) {
        let mut mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        mailbox.pending.clear();
        mailbox.reset = true;
    }

    const CAPACITY: usize = 256;

    pub fn send(&self, input: PointerInput) {
        let mut mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if mailbox.pending.len() == Self::CAPACITY {
            if let Some(index) = mailbox.pending.iter().position(|sample| sample.motion) {
                mailbox.pending.remove(index);
            } else {
                mailbox.pending.clear();
                mailbox.reset = true;
            }
        }
        mailbox.pending.push_back(input);
    }

    pub fn source(&self, native_window: isize) -> Option<Entity> {
        let mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        mailbox
            .origins
            .iter()
            .find(|origin| origin.native_window == native_window)
            .map(|origin| origin.window)
    }

    pub fn position(&self, window: Entity) -> Option<Vec2> {
        let mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        for input in mailbox.pending.iter().rev() {
            if input.window == window && input.position_px.is_some() {
                return input.position_px;
            }
        }
        mailbox
            .origins
            .iter()
            .find(|origin| origin.window == window)
            .and_then(|origin| origin.position)
    }

    pub fn release_outside(&self, buttons: NativeMouseButtons) {
        let mut mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let origins = mailbox.origins.clone();
        for origin in origins {
            let pending_held = mailbox
                .pending
                .iter()
                .rev()
                .find(|sample| sample.window == origin.window)
                .is_some_and(|sample| {
                    sample.buttons.left || sample.buttons.right || sample.buttons.middle
                });
            if !origin.held && !pending_held {
                continue;
            }
            if mailbox.pending.len() == Self::CAPACITY {
                mailbox.pending.clear();
                mailbox.reset = true;
            }
            mailbox
                .pending
                .push_back(PointerInput::buttons(origin.window, buttons));
        }
    }

    fn uses_native(&self, window: Entity) -> bool {
        let mailbox = self.0.lock().unwrap_or_else(|error| error.into_inner());
        mailbox.native_enabled && mailbox.origins.iter().any(|origin| origin.window == window)
    }
}

fn initialize(
    windows: Query<Entity, (With<Window>, Without<PointerState>)>,
    mut commands: Commands,
) {
    for entity in &windows {
        commands.entity(entity).insert((
            PointerState::default(),
            PointerCapture::default(),
            WindowGesture::default(),
        ));
    }
}

fn receive(
    inbox: Res<PointerInbox>,
    mut windows: Query<(Entity, &mut PointerCapture), With<PointerState>>,
    mut writer: MessageWriter<PointerInput>,
) {
    for (_, mut capture) in &mut windows {
        capture.consumed.clear();
        capture.suppressed.clear();
        capture.routed.clear();
    }
    let mut mailbox = inbox.0.lock().unwrap_or_else(|error| error.into_inner());
    if std::mem::take(&mut mailbox.reset) {
        for (window, _) in &windows {
            writer.write(PointerInput::cancel(window));
        }
    }
    for input in mailbox.pending.drain(..) {
        writer.write(input);
    }
}

fn translate(
    mut cursors: MessageReader<CursorMoved>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut focus: MessageReader<WindowFocused>,
    windows: Query<(Entity, &Window, &PointerState)>,
    inbox: Res<PointerInbox>,
    mut writer: MessageWriter<PointerInput>,
) {
    let mut samples = Vec::<PointerInput>::new();
    for event in cursors.read() {
        if inbox.uses_native(event.window) {
            continue;
        }
        let Ok((_, window, pointer)) = windows.get(event.window) else {
            continue;
        };
        samples.push(PointerInput::sample(
            event.window,
            event.position * window.resolution.scale_factor(),
            pointer.buttons,
            true,
        ));
    }
    for event in buttons.read() {
        if inbox.uses_native(event.window) {
            continue;
        }
        let Ok((_, window, pointer)) = windows.get(event.window) else {
            continue;
        };
        let mut input = match samples
            .iter()
            .rev()
            .find(|input| input.window == event.window)
        {
            Some(input) => *input,
            None => PointerInput {
                native_token: 0,
                window: event.window,
                position_px: window.physical_cursor_position().or(pointer.position_px),
                buttons: pointer.buttons,
                motion: false,
                cancelled: false,
                modifiers: None,
            },
        };
        let pressed = event.state.is_pressed();
        match event.button {
            MouseButton::Left => input.buttons.left = pressed,
            MouseButton::Right => input.buttons.right = pressed,
            MouseButton::Middle => input.buttons.middle = pressed,
            _ => continue,
        }
        input.motion = false;
        samples.push(input);
    }
    for (entity, window, pointer) in &windows {
        if inbox.uses_native(entity) || samples.iter().any(|sample| sample.window == entity) {
            continue;
        }
        if let Some(position) = window.physical_cursor_position()
            && pointer.position_px != Some(position)
        {
            samples.push(PointerInput::sample(
                entity,
                position,
                pointer.buttons,
                true,
            ));
        }
    }
    for input in samples {
        writer.write(input);
    }
    for event in focus.read() {
        if !event.focused {
            writer.write(PointerInput::cancel(event.window));
        }
    }
}

fn apply(
    mut inputs: MessageReader<PointerInput>,
    mut windows: Query<(&mut PointerState, &mut PointerCapture), With<Window>>,
    entities: Query<()>,
    mut updated: MessageWriter<PointerUpdated>,
) {
    for input in inputs.read() {
        if input
            .position_px
            .is_some_and(|position| !position.is_finite())
        {
            continue;
        }
        let Ok((mut pointer, mut capture)) = windows.get_mut(input.window) else {
            continue;
        };
        let capture_owner = capture.owner();
        if input.cancelled {
            capture.clear();
        }
        let mut event = pointer.update(*input);
        event.capture_owner = capture_owner;
        updated.write(event);
    }
    for (_, mut capture) in &mut windows {
        if capture
            .owner()
            .is_some_and(|owner| !entities.contains(owner))
        {
            capture.clear();
        }
    }
}

fn publish(inbox: Res<PointerInbox>, windows: Query<(Entity, &Window, &PointerState)>) {
    #[allow(unused_mut)]
    let mut origins = Vec::new();
    #[cfg(target_os = "macos")]
    WINIT_WINDOWS.with_borrow(|native| {
        for (entity, _, pointer) in &windows {
            let Some(window) = native.get_window(entity) else {
                continue;
            };
            let Ok(handle) = window.window_handle() else {
                continue;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                continue;
            };
            let view: &NSView = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
            let Some(window) = view.window() else {
                continue;
            };
            origins.push(PointerOrigin::from_pointer(
                entity,
                (&*window as *const objc2_app_kit::NSWindow) as isize,
                pointer,
            ));
        }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = windows;
    inbox
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .origins = origins;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PointerFixture {
        app: App,
        first: Entity,
        second: Entity,
    }

    impl PointerFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, PointerPlugin))
                .add_message::<CursorMoved>()
                .add_message::<WindowFocused>();
            let first = app.world_mut().spawn(Window::default()).id();
            let second = app.world_mut().spawn(Window::default()).id();
            app.update();
            Self { app, first, second }
        }

        fn send(&mut self, input: PointerInput) {
            self.app.world_mut().write_message(input);
        }
    }

    #[test]
    fn pointer_ingress_retains_window_identity_and_button_motion_order() {
        let mut fixture = PointerFixture::new();
        for input in [
            PointerInput::sample(
                fixture.first,
                Vec2::new(10.0, 20.0),
                NativeMouseButtons::default(),
                true,
            ),
            PointerInput::buttons(
                fixture.first,
                NativeMouseButtons {
                    left: true,
                    ..default()
                },
            ),
            PointerInput::sample(
                fixture.second,
                Vec2::new(30.0, 40.0),
                NativeMouseButtons::default(),
                true,
            ),
            PointerInput::buttons(fixture.first, NativeMouseButtons::default()),
        ] {
            fixture.send(input);
        }
        fixture.app.update();
        let first = fixture
            .app
            .world()
            .get::<PointerState>(fixture.first)
            .unwrap();
        assert_eq!(first.position_px, Some(Vec2::new(10.0, 20.0)));
        assert_eq!(first.motion_sequence, 1);
        assert_eq!(first.sequence, 3);
        assert!(!first.buttons.left);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerState>(fixture.second)
                .unwrap()
                .position_px,
            Some(Vec2::new(30.0, 40.0))
        );
        let updates = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<PointerUpdated>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(updates.len(), 4);
        assert!(!updates[0].buttons.left);
        assert!(updates[1].buttons.left);
        assert!(updates[3].previous_buttons.left);
        assert!(!updates[3].buttons.left);
    }

    #[test]
    fn pointer_ingress_rejects_removed_origins_and_nonfinite_samples() {
        let mut fixture = PointerFixture::new();
        let stale = fixture.first;
        fixture.app.world_mut().despawn(stale);
        let replacement = fixture.app.world_mut().spawn(Window::default()).id();
        fixture.send(PointerInput::sample(
            stale,
            Vec2::ONE,
            NativeMouseButtons {
                left: true,
                ..default()
            },
            true,
        ));
        fixture.send(PointerInput::sample(
            fixture.second,
            Vec2::splat(f32::NAN),
            NativeMouseButtons::default(),
            true,
        ));
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerState>(replacement)
                .unwrap()
                .sequence,
            0
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerState>(fixture.second)
                .unwrap()
                .sequence,
            0
        );
    }

    #[test]
    fn pointer_focus_loss_preserves_release_position_and_clears_capture() {
        let mut fixture = PointerFixture::new();
        let owner = fixture.app.world_mut().spawn_empty().id();
        fixture.send(PointerInput::sample(
            fixture.first,
            Vec2::new(30.0, 40.0),
            NativeMouseButtons {
                left: true,
                ..default()
            },
            true,
        ));
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .get_mut::<PointerCapture>(fixture.first)
            .unwrap()
            .claim(owner, MouseButton::Left);
        fixture.send(PointerInput::cancel(fixture.first));
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerCapture>(fixture.first)
                .unwrap()
                .owner(),
            None
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerState>(fixture.first)
                .unwrap()
                .position_px,
            None
        );
        let events = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<PointerUpdated>>()
            .drain()
            .collect::<Vec<_>>();
        let release = events.last().unwrap();
        assert_eq!(release.position_px, Some(Vec2::new(30.0, 40.0)));
        assert_eq!(release.capture_owner, Some(owner));
        assert!(release.cancelled && release.previous_buttons.left && !release.buttons.left);
    }

    #[test]
    fn pointer_owner_despawn_releases_capture_without_an_input_event() {
        let mut fixture = PointerFixture::new();
        let owner = fixture.app.world_mut().spawn_empty().id();
        fixture
            .app
            .world_mut()
            .get_mut::<PointerCapture>(fixture.second)
            .unwrap()
            .claim(owner, MouseButton::Left);
        fixture.app.world_mut().despawn(owner);
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerCapture>(fixture.second)
                .unwrap()
                .owner(),
            None
        );
    }

    #[test]
    fn pointer_mailbox_overflow_cancels_ownership_and_keeps_the_final_release() {
        let mut fixture = PointerFixture::new();
        fixture.send(PointerInput::sample(
            fixture.first,
            Vec2::ONE,
            NativeMouseButtons {
                left: true,
                ..default()
            },
            true,
        ));
        fixture.app.update();
        let inbox = fixture.app.world().resource::<PointerInbox>().clone();
        for _ in 0..PointerInbox::CAPACITY {
            inbox.send(PointerInput::buttons(
                fixture.first,
                NativeMouseButtons {
                    left: true,
                    ..default()
                },
            ));
        }
        inbox.send(PointerInput::buttons(
            fixture.first,
            NativeMouseButtons::default(),
        ));
        fixture.app.update();
        assert!(
            !fixture
                .app
                .world()
                .get::<PointerState>(fixture.first)
                .unwrap()
                .buttons
                .left
        );
        let events = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<PointerUpdated>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(
            events
                .iter()
                .any(|event| event.window == fixture.first && event.cancelled)
        );
        assert!(!events.last().unwrap().buttons.left);
    }
}
