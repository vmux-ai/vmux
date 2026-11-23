pub use crate::{PaneSize, PaneSplitGaps};
use bevy::ecs::system::Commands;
use bevy::window::Window;
use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
    window::{CursorIcon, SystemCursorIcon},
};
use moonshine_save::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_ecs::store::StoreAppExt;
use vmux_flex::prelude::*;
#[cfg(test)]
use vmux_history::LastActivatedAt;

use crate::settings::LayoutSettings;

use super::{Pane, PaneSplit, PaneSplitDirection, ResizeRequest};
#[cfg(test)]
use crate::stack::Stack;
use crate::{
    host::command::LayoutRequestSet,
    stack::{ActiveTabParam, LayoutFocus},
};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PointerResizeSet;

#[derive(Component, Default)]
struct PointerCursor(u64);

#[derive(Component, Default)]
pub struct ResizeGripMasks(pub Vec<ComputedNode>);

pub(super) struct ResizePlugin;

impl Plugin for ResizePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ResizeRequest>()
            .register_required_components::<PaneSize, Save>()
            .register_stored::<PaneSize>()
            .add_systems(Update, from_commands.in_set(LayoutRequestSet::Handle))
            .add_systems(
                Update,
                (initialize_pointer, ApplyDeferred, from_pointer)
                    .chain()
                    .in_set(PointerResizeSet)
                    .after(crate::window::WindowFocusSet)
                    .after(crate::host::space::CurrentSpaceSet)
                    .before(vmux_input::PointerSet::Route),
            )
            .add_systems(Last, publish_masks.in_set(vmux_input::PointerSet::Publish))
            .add_systems(PostUpdate, sync_split_gaps)
            .add_observer(resize);
    }
}

#[derive(Component)]
pub struct PaneDrag {
    previous: Entity,
    next: Entity,
    direction: PaneSplitDirection,
    start_position: f32,
    previous_grow: f32,
    next_grow: f32,
}

impl PaneDrag {
    const MIN_PANE_PX: f32 = 60.0;
    const RESIZE_STEP: f32 = 0.05;

    fn resized_pair(
        pane_grow: f32,
        sibling_grow: f32,
        delta: f32,
        parent_length: f32,
    ) -> (f32, f32) {
        let total = pane_grow + sibling_grow;
        let mut pane_grow = pane_grow + delta;
        let mut sibling_grow = sibling_grow - delta;
        let minimum = Self::MIN_PANE_PX / parent_length.max(1.0) * total;
        pane_grow = pane_grow.max(minimum);
        sibling_grow = sibling_grow.max(minimum);
        let adjusted_total = pane_grow + sibling_grow;
        if adjusted_total > 0.0 {
            pane_grow = pane_grow / adjusted_total * total;
            sibling_grow = sibling_grow / adjusted_total * total;
        }
        (pane_grow, sibling_grow)
    }

    fn matches(&self, split: &PaneSplit, siblings: impl Iterator<Item = Entity>) -> bool {
        if self.direction != split.direction {
            return false;
        }
        let mut previous = None;
        for next in siblings {
            if previous == Some(self.previous) && next == self.next {
                return true;
            }
            previous = Some(next);
        }
        false
    }
}

fn from_commands(
    mut reader: MessageReader<ResizeRequest>,
    active_tab: ActiveTabParam,
    all_children: Query<&Children>,
    focus: LayoutFocus,
    parents: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    mut sizes: ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    for request in reader.read() {
        let (_, Some(active), _) = focus.resolve(active_tab.get()) else {
            continue;
        };

        match request.direction {
            None => {
                equalize_siblings(active, &parents, &splits, &all_children, &mut sizes);
            }
            Some(direction) => {
                resize_active_pane(
                    active,
                    direction,
                    &parents,
                    &splits,
                    &all_children,
                    &mut sizes,
                );
            }
        }
    }
}

fn equalize_siblings(
    active: Entity,
    parents: &Query<&ChildOf>,
    splits: &Query<&PaneSplit>,
    all_children: &Query<&Children>,
    sizes: &mut ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    let Ok(parent) = parents.get(active).map(Relationship::get) else {
        return;
    };
    if !splits.contains(parent) {
        return;
    }
    let Ok(children) = all_children.get(parent) else {
        return;
    };
    let children: Vec<Entity> = children.iter().collect();
    {
        let mut nodes = sizes.p0();
        for child in &children {
            if let Ok(mut node) = nodes.get_mut(*child) {
                node.flex_grow = 1.0;
            }
        }
    }
    let mut pane_sizes = sizes.p1();
    for child in children {
        if let Ok(mut size) = pane_sizes.get_mut(child) {
            size.flex_grow = 1.0;
        }
    }
}

fn resize_active_pane(
    active: Entity,
    direction: PaneDirection,
    parents: &Query<&ChildOf>,
    splits: &Query<&PaneSplit>,
    all_children: &Query<&Children>,
    sizes: &mut ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    let axis = match direction {
        PaneDirection::Left | PaneDirection::Right => PaneSplitDirection::Row,
        PaneDirection::Top | PaneDirection::Bottom => PaneSplitDirection::Column,
    };
    let after = matches!(direction, PaneDirection::Right | PaneDirection::Bottom);
    let mut child = active;
    let mut parent = None;
    for _ in 0..10 {
        let Ok(candidate) = parents.get(child).map(Relationship::get) else {
            break;
        };
        if splits
            .get(candidate)
            .is_ok_and(|split| split.direction == axis)
        {
            parent = Some(candidate);
            break;
        }
        child = candidate;
    }
    let Some(parent) = parent else {
        return;
    };
    let Ok(siblings) = all_children.get(parent) else {
        return;
    };
    let siblings: Vec<Entity> = siblings.iter().collect();
    let Some(index) = siblings.iter().position(|entity| *entity == child) else {
        return;
    };
    let sibling = if after {
        let Some(sibling) = siblings.get(index + 1) else {
            return;
        };
        *sibling
    } else {
        let Some(index) = index.checked_sub(1) else {
            return;
        };
        siblings[index]
    };

    let parent_length = {
        let layouts = sizes.p2();
        let size = layouts
            .get(parent)
            .map(|layout| layout.size)
            .unwrap_or(Vec2::ZERO);
        match axis {
            PaneSplitDirection::Row => size.x,
            PaneSplitDirection::Column => size.y,
        }
    };
    let (pane_grow, sibling_grow) = {
        let nodes = sizes.p0();
        (
            nodes.get(child).map_or(1.0, |node| node.flex_grow),
            nodes.get(sibling).map_or(1.0, |node| node.flex_grow),
        )
    };
    let total_grow = pane_grow + sibling_grow;
    let (pane_grow, sibling_grow) = PaneDrag::resized_pair(
        pane_grow,
        sibling_grow,
        PaneDrag::RESIZE_STEP * total_grow,
        parent_length,
    );

    {
        let mut nodes = sizes.p0();
        if let Ok(mut node) = nodes.get_mut(child) {
            node.flex_grow = pane_grow;
        }
        if let Ok(mut node) = nodes.get_mut(sibling) {
            node.flex_grow = sibling_grow;
        }
    }
    let mut pane_sizes = sizes.p1();
    if let Ok(mut size) = pane_sizes.get_mut(child) {
        size.flex_grow = pane_grow;
    }
    if let Ok(mut size) = pane_sizes.get_mut(sibling) {
        size.flex_grow = sibling_grow;
    }
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct ResizeGrip {
    bounds: ComputedNode,
    previous_cursor: Option<CursorIcon>,
}

impl ResizeGrip {
    pub fn minimum_width(scale: f32) -> f32 {
        12.0 * scale
    }

    pub fn bounds(&self) -> ComputedNode {
        self.bounds
    }

    pub fn contains(&self, point: Vec2) -> bool {
        self.bounds.contains(point)
    }

    fn between(direction: PaneSplitDirection, previous: ComputedNode, next: ComputedNode) -> Self {
        let scale = previous.scale().max(next.scale());
        let (mut min, mut max) = match direction {
            PaneSplitDirection::Row => (
                Vec2::new(previous.max().x, previous.min().y.min(next.min().y)),
                Vec2::new(next.min().x, previous.max().y.max(next.max().y)),
            ),
            PaneSplitDirection::Column => (
                Vec2::new(previous.min().x.min(next.min().x), previous.max().y),
                Vec2::new(previous.max().x.max(next.max().x), next.min().y),
            ),
        };
        match direction {
            PaneSplitDirection::Row => {
                let padding = ((Self::minimum_width(scale) - (max.x - min.x)) * 0.5).max(0.0);
                min.x -= padding;
                max.x += padding;
            }
            PaneSplitDirection::Column => {
                let padding = ((Self::minimum_width(scale) - (max.y - min.y)) * 0.5).max(0.0);
                min.y -= padding;
                max.y += padding;
            }
        }
        Self {
            bounds: ComputedNode {
                size: max - min,
                center: (min + max) * 0.5,
                inverse_scale_factor: previous.inverse_scale_factor,
                ..default()
            },
            previous_cursor: None,
        }
    }
}

#[derive(SystemParam)]
struct PointerCursors<'w, 's> {
    windows:
        Query<'w, 's, (Option<&'static ResizeGrip>, Option<&'static CursorIcon>), With<Window>>,
    commands: Commands<'w, 's>,
}

impl PointerCursors<'_, '_> {
    fn publish(&mut self, window: Entity, mut grip: ResizeGrip, icon: CursorIcon) {
        let Ok((current, cursor)) = self.windows.get(window) else {
            return;
        };
        grip.previous_cursor = match current {
            Some(current) => current.previous_cursor.clone(),
            None => cursor.cloned(),
        };
        if current != Some(&grip) {
            self.commands.entity(window).insert(grip);
        }
        if cursor != Some(&icon) {
            self.commands.entity(window).insert(icon);
        }
    }

    fn release(&mut self, window: Entity) {
        let Ok((Some(grip), _)) = self.windows.get(window) else {
            return;
        };
        self.commands.entity(window).remove::<ResizeGrip>();
        if let Some(cursor) = &grip.previous_cursor {
            self.commands.entity(window).insert(cursor.clone());
        } else {
            self.commands.entity(window).remove::<CursorIcon>();
        }
    }
}

fn initialize_pointer(
    windows: Query<Entity, (With<Window>, Without<PointerCursor>)>,
    mut commands: Commands,
) {
    for window in &windows {
        commands
            .entity(window)
            .insert((PointerCursor::default(), ResizeGripMasks::default()));
    }
}

#[derive(SystemParam)]
struct ResizeScene<'w, 's> {
    windows: Query<'w, 's, (Entity, &'static Window, &'static vmux_input::PointerState)>,
    focused: crate::window::FocusedWindow<'w, 's>,
    overlays: super::OverlayInput<'w, 's>,
    windows_for: crate::window::WindowHierarchy<'w, 's>,
    active: ActiveTabParam<'w, 's>,
    panes: crate::Hierarchy<'w, 's>,
    splits: Query<'w, 's, (Entity, &'static PaneSplit, &'static Children), With<Pane>>,
    layouts: Query<'w, 's, &'static ComputedNode>,
}

#[derive(SystemParam)]
struct ResizeControl<'w, 's> {
    feedback: PointerCursors<'w, 's>,
    drags: Query<'w, 's, (Entity, &'static PaneDrag)>,
    captures: Query<'w, 's, &'static mut vmux_input::PointerCapture, With<Window>>,
    cursors: Query<'w, 's, &'static mut PointerCursor, With<Window>>,
    commands: Commands<'w, 's>,
}

#[derive(SystemParam)]
struct ResizeSizes<'w, 's> {
    nodes: Query<'w, 's, &'static mut Node, Without<vmux_ecs::WindowOverlay>>,
    panes: Query<'w, 's, &'static mut PaneSize>,
}

#[derive(Event)]
struct PointerResize(Option<vmux_input::PointerUpdated>);

fn from_pointer(messages: Res<Messages<vmux_input::PointerUpdated>>, mut commands: Commands) {
    let mut cursor = messages.get_cursor();
    for event in cursor.read(&messages) {
        commands.trigger(PointerResize(Some(*event)));
    }
    commands.trigger(PointerResize(None));
}

fn resize(
    input: On<PointerResize>,
    scene: ResizeScene,
    mut control: ResizeControl,
    mut sizes: ResizeSizes,
) {
    let explicit = input.0.is_some();
    let Some(event) = sample(input.0, &scene, &mut control) else {
        return;
    };
    release(&event, explicit, &scene, &mut control);
    let pressed = event.buttons.left && !event.cancelled;
    let just_pressed = explicit && pressed && !event.previous_buttons.left;
    let (Some(window), Some(tab)) = (scene.focused.entity(), scene.active.get()) else {
        return;
    };
    let Ok((_, native, _)) = scene.windows.get(window) else {
        return;
    };
    if !native.focused || scene.overlays.owns(window) {
        for (split, _) in &control.drags {
            control.commands.entity(split).remove::<PaneDrag>();
        }
        return;
    }
    let Some(cursor) = event.position_px else {
        control.feedback.release(window);
        return;
    };
    let hit = if just_pressed {
        event.press_position_px.unwrap_or(cursor)
    } else {
        cursor
    };
    if pressed && dragging(window, tab, cursor, &scene, &mut control, &mut sizes) {
        return;
    }
    if pressed && !just_pressed {
        control.feedback.release(window);
        return;
    }
    if preview(&event, tab, hit, just_pressed, &scene, &mut control, &sizes) {
        return;
    }
    control.feedback.release(window);
}

fn sample(
    input: Option<vmux_input::PointerUpdated>,
    scene: &ResizeScene,
    control: &mut ResizeControl,
) -> Option<vmux_input::PointerUpdated> {
    if let Some(event) = input {
        let Ok(mut cursor) = control.cursors.get_mut(event.window) else {
            return None;
        };
        if event.sequence <= cursor.0 {
            return None;
        }
        cursor.0 = event.sequence;
        if scene.focused.entity() != Some(event.window) {
            return None;
        }
        if control.captures.get(event.window).is_ok_and(|capture| {
            capture
                .owner()
                .is_some_and(|owner| control.drags.get(owner).is_err())
        }) {
            return None;
        }
        return Some(event);
    }
    let Some(window) = scene.focused.entity() else {
        for (split, _) in &control.drags {
            control.commands.entity(split).remove::<PaneDrag>();
            if let Some(window) = scene.windows_for.get(split)
                && let Ok(mut capture) = control.captures.get_mut(window)
                && capture.owner() == Some(split)
            {
                capture.release(MouseButton::Left);
            }
        }
        for (window, _, _) in &scene.windows {
            control.feedback.release(window);
        }
        return None;
    };
    let (_, _, pointer) = scene.windows.get(window).ok()?;
    Some(vmux_input::PointerUpdated {
        native_token: 0,
        capture_owner: None,
        window,
        position_px: pointer.position_px,
        buttons: pointer.buttons,
        previous_buttons: pointer.buttons,
        sequence: pointer.sequence,
        motion: false,
        cancelled: pointer.cancelled,
        modifiers: pointer.modifiers,
        press_sequence: pointer.press_sequence,
        press_position_px: pointer.press_position_px,
        press_modifiers: pointer.press_modifiers,
    })
}

fn release(
    event: &vmux_input::PointerUpdated,
    explicit: bool,
    scene: &ResizeScene,
    control: &mut ResizeControl,
) {
    let window = scene.focused.entity();
    let blocked = window.is_some_and(|window| scene.overlays.owns(window));
    let tab = scene.active.get();
    for (entity, native, _) in &scene.windows {
        if Some(entity) != window || !native.focused || tab.is_none() || blocked {
            control.feedback.release(entity);
        }
    }
    for (split, drag) in &control.drags {
        if explicit
            && scene.windows_for.get(split) == Some(event.window)
            && let Ok(mut capture) = control.captures.get_mut(event.window)
        {
            capture.consume(event.native_token);
            capture.suppress(event.sequence);
        }
        let current = !blocked
            && tab.is_some_and(|tab| scene.panes.contains(tab, split))
            && scene.windows_for.get(split) == window
            && scene.splits.get(split).is_ok_and(|(_, split, children)| {
                drag.matches(
                    split,
                    children.iter().filter(|child| {
                        scene.panes.is_leaf(*child) || scene.splits.contains(*child)
                    }),
                )
            });
        if !event.buttons.left || event.cancelled || !current {
            control.commands.entity(split).remove::<PaneDrag>();
            if let Some(window) = scene.windows_for.get(split)
                && let Ok(mut capture) = control.captures.get_mut(window)
                && capture.owner() == Some(split)
            {
                capture.release(MouseButton::Left);
            }
        }
    }
}

fn dragging(
    window: Entity,
    tab: Entity,
    cursor: Vec2,
    scene: &ResizeScene,
    control: &mut ResizeControl,
    sizes: &mut ResizeSizes,
) -> bool {
    for (entity, drag) in &control.drags {
        if scene.windows_for.get(entity) != Some(window) || !scene.panes.contains(tab, entity) {
            continue;
        }
        let Ok((_, split, children)) = scene.splits.get(entity) else {
            continue;
        };
        if !drag.matches(
            split,
            children
                .iter()
                .filter(|child| scene.panes.is_leaf(*child) || scene.splits.contains(*child)),
        ) {
            continue;
        }
        let (Ok(previous), Ok(next)) = (
            scene.layouts.get(drag.previous),
            scene.layouts.get(drag.next),
        ) else {
            control.commands.entity(entity).remove::<PaneDrag>();
            continue;
        };
        if previous.is_empty() || next.is_empty() {
            control.commands.entity(entity).remove::<PaneDrag>();
            if let Ok(mut capture) = control.captures.get_mut(window)
                && capture.owner() == Some(entity)
            {
                capture.release(MouseButton::Left);
            }
            control.feedback.release(window);
            continue;
        }
        let grip = ResizeGrip::between(split.direction, *previous, *next);
        control
            .feedback
            .publish(window, grip, SystemCursorIcon::Grabbing.into());
        let position = match split.direction {
            PaneSplitDirection::Row => cursor.x,
            PaneSplitDirection::Column => cursor.y,
        };
        let parent_size = scene
            .layouts
            .get(entity)
            .map_or(Vec2::ONE, |layout| layout.size);
        let parent_length = match split.direction {
            PaneSplitDirection::Row => parent_size.x,
            PaneSplitDirection::Column => parent_size.y,
        }
        .max(1.0);
        let delta = (position - drag.start_position) / parent_length
            * (drag.previous_grow + drag.next_grow);
        let (previous_grow, next_grow) =
            PaneDrag::resized_pair(drag.previous_grow, drag.next_grow, delta, parent_length);
        if let Ok(mut node) = sizes.nodes.get_mut(drag.previous) {
            node.flex_grow = previous_grow;
        }
        if let Ok(mut node) = sizes.nodes.get_mut(drag.next) {
            node.flex_grow = next_grow;
        }
        if let Ok(mut size) = sizes.panes.get_mut(drag.previous) {
            size.flex_grow = previous_grow;
        }
        if let Ok(mut size) = sizes.panes.get_mut(drag.next) {
            size.flex_grow = next_grow;
        }
        return true;
    }
    false
}

fn preview(
    event: &vmux_input::PointerUpdated,
    tab: Entity,
    hit: Vec2,
    pressed: bool,
    scene: &ResizeScene,
    control: &mut ResizeControl,
    sizes: &ResizeSizes,
) -> bool {
    let window = event.window;
    for (entity, split, children) in &scene.splits {
        if scene.windows_for.get(entity) != Some(window) || !scene.panes.contains(tab, entity) {
            continue;
        }
        let mut siblings = Vec::new();
        for child in children.iter() {
            if scene.panes.is_leaf(child) || scene.splits.contains(child) {
                siblings.push(child);
            }
        }
        for pair in siblings.windows(2) {
            let (previous_entity, next_entity) = (pair[0], pair[1]);
            let (Ok(previous), Ok(next)) = (
                scene.layouts.get(previous_entity),
                scene.layouts.get(next_entity),
            ) else {
                continue;
            };
            if previous.is_empty() || next.is_empty() {
                continue;
            }
            let grip = ResizeGrip::between(split.direction, *previous, *next);
            if !grip.contains(hit) {
                continue;
            }
            let icon = if pressed {
                SystemCursorIcon::Grabbing
            } else {
                match split.direction {
                    PaneSplitDirection::Row => SystemCursorIcon::ColResize,
                    PaneSplitDirection::Column => SystemCursorIcon::RowResize,
                }
            };
            control.feedback.publish(window, grip, icon.into());
            if pressed {
                if let Ok(mut capture) = control.captures.get_mut(window) {
                    capture.claim(entity, MouseButton::Left);
                    capture.consume(event.native_token);
                    capture.suppress(event.sequence);
                }
                control.commands.entity(entity).insert(PaneDrag {
                    previous: previous_entity,
                    next: next_entity,
                    direction: split.direction,
                    start_position: match split.direction {
                        PaneSplitDirection::Row => hit.x,
                        PaneSplitDirection::Column => hit.y,
                    },
                    previous_grow: sizes
                        .nodes
                        .get(previous_entity)
                        .map_or(1.0, |node| node.flex_grow),
                    next_grow: sizes
                        .nodes
                        .get(next_entity)
                        .map_or(1.0, |node| node.flex_grow),
                });
            }
            return true;
        }
    }
    false
}

fn publish_masks(
    mut windows: Query<(Entity, &Window, &mut ResizeGripMasks)>,
    tabs: Query<
        (
            Entity,
            &ChildOf,
            &crate::presentation::Layer,
            &Node,
            &Visibility,
        ),
        (
            With<crate::tab::Tab>,
            With<vmux_ecs::Active>,
            Without<crate::presentation::Retiring>,
        ),
    >,
    spaces: Query<(), (With<crate::space::Space>, With<vmux_ecs::Active>)>,
    hierarchy: crate::window::WindowHierarchy,
    panes: crate::Hierarchy,
    overlays: super::OverlayInput,
    splits: Query<(Entity, &PaneSplit, &Children), With<Pane>>,
    layouts: Query<&ComputedNode>,
) {
    for (window, native, mut masks) in &mut windows {
        masks.0.clear();
        if !native.visible || !native.focused || overlays.owns(window) {
            continue;
        }
        for (tab, parent, layer, node, visibility) in &tabs {
            if !spaces.contains(parent.parent())
                || !layer.visible()
                || node.display == Display::None
                || *visibility == Visibility::Hidden
                || hierarchy.get(tab) != Some(window)
            {
                continue;
            }
            for (split, direction, children) in &splits {
                if !panes.contains(tab, split) {
                    continue;
                }
                let mut siblings = Vec::new();
                for child in children.iter() {
                    if panes.is_leaf(child) || splits.contains(child) {
                        siblings.push(child);
                    }
                }
                for pair in siblings.windows(2) {
                    let (Ok(previous), Ok(next)) = (layouts.get(pair[0]), layouts.get(pair[1]))
                    else {
                        continue;
                    };
                    if !previous.is_empty() && !next.is_empty() {
                        masks.0.push(
                            ResizeGrip::between(direction.direction, *previous, *next).bounds(),
                        );
                    }
                }
            }
        }
    }
}

fn sync_split_gaps(
    settings: Res<LayoutSettings>,
    mut splits: Query<(&PaneSplit, &mut Node), With<Pane>>,
) {
    if !settings.is_changed() {
        return;
    }
    for (split, mut node) in &mut splits {
        split.apply_gaps(&mut node, crate::event::PANE_GAP_PX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::Tab;
    use bevy::ecs::message::Messages;
    use bevy_cef::prelude::HostWindow;

    struct ResizeFixture {
        app: App,
        left: Entity,
        right: Entity,
        split: Entity,
        window: Option<Entity>,
    }

    impl ResizeFixture {
        fn new(left_grow: f32, right_grow: f32) -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                bevy::input::InputPlugin,
                vmux_input::PointerPlugin,
                ResizePlugin,
            ))
            .add_message::<bevy::window::CursorMoved>()
            .add_message::<bevy::window::WindowFocused>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(LayoutSettings::default());
            let tab = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt::now()))
                .id();
            let split = app
                .world_mut()
                .spawn((
                    Pane,
                    PaneSplit {
                        direction: PaneSplitDirection::Row,
                    },
                    ComputedNode {
                        size: Vec2::new(1000.0, 500.0),
                        ..default()
                    },
                    ChildOf(tab),
                ))
                .id();
            let left = Self::pane(&mut app, split, left_grow, 10);
            let right = Self::pane(&mut app, split, right_grow, 1);
            Self {
                app,
                left,
                right,
                split,
                window: None,
            }
        }

        fn native(direction: PaneSplitDirection) -> Self {
            let mut fixture = Self::new(1.0, 1.0);
            let window = fixture
                .app
                .world_mut()
                .spawn((Window::default(), vmux_ecs::Active))
                .id();
            fixture.window = Some(window);
            let (parent, previous, next) = match direction {
                PaneSplitDirection::Row => (
                    Vec2::new(1000.0, 500.0),
                    Vec2::new(248.0, 250.0),
                    Vec2::new(752.0, 250.0),
                ),
                PaneSplitDirection::Column => (
                    Vec2::new(500.0, 1000.0),
                    Vec2::new(250.0, 248.0),
                    Vec2::new(250.0, 752.0),
                ),
            };
            let size = match direction {
                PaneSplitDirection::Row => Vec2::new(496.0, 500.0),
                PaneSplitDirection::Column => Vec2::new(500.0, 496.0),
            };
            fixture.app.world_mut().entity_mut(fixture.split).insert((
                HostWindow(window),
                Node::default(),
                PaneSplit { direction },
                ComputedNode {
                    size: parent,
                    center: parent / 2.0,
                    ..default()
                },
            ));
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.left)
                .insert(ComputedNode {
                    size,
                    center: previous,
                    ..default()
                });
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.right)
                .insert(ComputedNode {
                    size,
                    center: next,
                    ..default()
                });
            fixture.app.update();
            fixture
        }

        fn pointer(&mut self, left: bool, position: Option<Vec2>) {
            let window = self.window.unwrap();
            let buttons = bevy_cef_core::prelude::NativeMouseButtons { left, ..default() };
            let input = match position {
                Some(position) => vmux_input::PointerInput::sample(window, position, buttons, true),
                None => vmux_input::PointerInput::buttons(window, buttons),
            };
            self.app.world_mut().write_message(input);
            self.app.update();
        }

        fn pane(app: &mut App, parent: Entity, flex_grow: f32, activated_at: i64) -> Entity {
            let pane = app
                .world_mut()
                .spawn((
                    Pane,
                    Node {
                        flex_grow,
                        ..default()
                    },
                    PaneSize { flex_grow },
                    LastActivatedAt(activated_at),
                    ChildOf(parent),
                ))
                .id();
            app.world_mut().spawn((
                Stack::default(),
                LastActivatedAt(activated_at),
                ChildOf(pane),
            ));
            pane
        }

        fn send(&mut self, request: ResizeRequest) {
            self.app
                .world_mut()
                .resource_mut::<Messages<ResizeRequest>>()
                .write(request);
            self.app.update();
        }

        fn grows(&self, pane: Entity) -> (f32, f32) {
            (
                self.app.world().get::<Node>(pane).unwrap().flex_grow,
                self.app.world().get::<PaneSize>(pane).unwrap().flex_grow,
            )
        }
    }

    #[test]
    fn changed_pane_order_or_axis_cancels_capture_without_changing_sizes() {
        for change in 0..5 {
            let mut fixture = ResizeFixture::native(PaneSplitDirection::Row);
            let start = Vec2::new(500.0, 250.0);
            fixture.pointer(false, Some(start));
            fixture.pointer(true, Some(start));
            let previous = fixture.grows(fixture.left);
            let next = fixture.grows(fixture.right);
            match change {
                0 => {
                    let middle = fixture.app.world_mut().spawn(Pane).id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.split)
                        .insert_children(1, &[middle]);
                }
                1 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.split)
                        .insert_children(0, &[fixture.right]);
                }
                2 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.split)
                        .insert(PaneSplit {
                            direction: PaneSplitDirection::Column,
                        });
                }
                3 => {
                    let auxiliary = fixture.app.world_mut().spawn_empty().id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.split)
                        .insert_children(1, &[auxiliary]);
                }
                4 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.split)
                        .remove::<PaneSplit>();
                }
                _ => unreachable!(),
            }
            fixture.pointer(true, Some(start));
            assert_eq!(
                fixture.app.world().get::<PaneDrag>(fixture.split).is_some(),
                change == 3
            );
            assert_eq!(fixture.grows(fixture.left), previous);
            assert_eq!(fixture.grows(fixture.right), next);
            if change != 3 {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<ResizeGrip>(fixture.window.unwrap())
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn overlay_input_blocks_capture_and_cancels_an_existing_drag() {
        for scope in 0..3 {
            let mut fixture = ResizeFixture::native(PaneSplitDirection::Row);
            let start = Vec2::new(500.0, 250.0);
            fixture.pointer(false, Some(start));
            let overlay = fixture
                .app
                .world_mut()
                .spawn((vmux_ecs::WindowOverlay, vmux_ecs::KeyboardOwner))
                .id();
            if scope != 1 {
                let window = if scope == 2 {
                    fixture.app.world_mut().spawn(Window::default()).id()
                } else {
                    fixture.window.unwrap()
                };
                fixture
                    .app
                    .world_mut()
                    .entity_mut(overlay)
                    .insert(HostWindow(window));
            }
            fixture.pointer(true, Some(start));
            if scope == 2 {
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_some());
                fixture
                    .app
                    .world_mut()
                    .entity_mut(overlay)
                    .insert(HostWindow(fixture.window.unwrap()));
                fixture.pointer(true, Some(Vec2::new(600.0, 250.0)));
            } else {
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
                fixture.app.world_mut().entity_mut(overlay).despawn();
                fixture.pointer(true, Some(Vec2::new(600.0, 250.0)));
            }
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ResizeGrip>(fixture.window.unwrap())
                    .is_none()
            );
            assert_eq!(fixture.grows(fixture.left), (1.0, 1.0));
            assert_eq!(fixture.grows(fixture.right), (1.0, 1.0));
        }
    }

    #[test]
    fn empty_endpoint_cancels_drag_without_resizing_or_retaining_capture() {
        for empty_previous in [true, false] {
            let mut fixture = ResizeFixture::native(PaneSplitDirection::Row);
            let start = Vec2::new(500.0, 250.0);
            fixture.pointer(false, Some(start));
            fixture.pointer(true, Some(start));
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_some());
            let endpoint = if empty_previous {
                fixture.left
            } else {
                fixture.right
            };
            fixture
                .app
                .world_mut()
                .entity_mut(endpoint)
                .insert(ComputedNode::default());
            fixture.pointer(true, Some(Vec2::new(600.0, 250.0)));
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<vmux_input::PointerCapture>(fixture.window.unwrap())
                    .unwrap()
                    .owner()
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ResizeGrip>(fixture.window.unwrap())
                    .is_none()
            );
            assert_eq!(fixture.grows(fixture.left), (1.0, 1.0));
            assert_eq!(fixture.grows(fixture.right), (1.0, 1.0));
        }
    }

    #[test]
    fn native_pointer_resizes_each_axis_and_releases_without_a_winit_mouse_event() {
        for direction in [PaneSplitDirection::Row, PaneSplitDirection::Column] {
            let mut fixture = ResizeFixture::native(direction);
            let (start, moved) = match direction {
                PaneSplitDirection::Row => (Vec2::new(500.0, 250.0), Vec2::new(600.0, 250.0)),
                PaneSplitDirection::Column => (Vec2::new(250.0, 500.0), Vec2::new(250.0, 600.0)),
            };
            fixture.pointer(false, Some(start));
            fixture.pointer(true, Some(start));
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_some());
            assert!(
                !fixture
                    .app
                    .world()
                    .resource::<ButtonInput<MouseButton>>()
                    .pressed(MouseButton::Left)
            );
            fixture.pointer(true, Some(moved));
            let previous = fixture.grows(fixture.left);
            let next = fixture.grows(fixture.right);
            assert!((previous.0 - 1.2).abs() < 0.0001);
            assert!((next.0 - 0.8).abs() < 0.0001);
            assert_eq!(previous.0, previous.1);
            assert_eq!(next.0, next.1);
            fixture.pointer(false, None);
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
            assert_eq!(fixture.grows(fixture.left), previous);
            let window = fixture.window.unwrap();
            let sequence = fixture
                .app
                .world()
                .get::<vmux_input::PointerState>(window)
                .unwrap()
                .sequence;
            let capture = fixture
                .app
                .world()
                .get::<vmux_input::PointerCapture>(window)
                .unwrap();
            assert_eq!(capture.owner(), None);
            assert!(capture.suppresses(sequence));
            fixture.pointer(false, Some(moved));
            let sequence = fixture
                .app
                .world()
                .get::<vmux_input::PointerState>(window)
                .unwrap()
                .sequence;
            assert!(
                !fixture
                    .app
                    .world()
                    .get::<vmux_input::PointerCapture>(window)
                    .unwrap()
                    .suppresses(sequence)
            );
        }
    }

    #[test]
    fn pointer_cannot_start_a_resize_in_another_window() {
        let mut fixture = ResizeFixture::native(PaneSplitDirection::Row);
        let window = fixture.app.world_mut().spawn(Window::default()).id();
        let split = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                Node::default(),
                ComputedNode {
                    size: Vec2::new(800.0, 500.0),
                    center: Vec2::new(400.0, 250.0),
                    ..default()
                },
                HostWindow(window),
            ))
            .id();
        let previous = ResizeFixture::pane(&mut fixture.app, split, 1.0, 1);
        let next = ResizeFixture::pane(&mut fixture.app, split, 1.0, 1);
        fixture
            .app
            .world_mut()
            .entity_mut(previous)
            .insert(ComputedNode {
                size: Vec2::new(396.0, 500.0),
                center: Vec2::new(198.0, 250.0),
                ..default()
            });
        fixture
            .app
            .world_mut()
            .entity_mut(next)
            .insert(ComputedNode {
                size: Vec2::new(396.0, 500.0),
                center: Vec2::new(602.0, 250.0),
                ..default()
            });
        fixture.pointer(false, Some(Vec2::new(400.0, 250.0)));
        fixture.pointer(true, Some(Vec2::new(400.0, 250.0)));
        assert!(fixture.app.world().get::<PaneDrag>(split).is_none());
        assert_eq!(fixture.grows(previous), (1.0, 1.0));
        assert_eq!(fixture.grows(next), (1.0, 1.0));
    }

    #[test]
    fn divider_capture_uses_the_press_before_motion_in_the_same_frame() {
        for direction in [PaneSplitDirection::Row, PaneSplitDirection::Column] {
            for (start, end, captured) in [(500.0, 700.0, true), (470.0, 500.0, false)] {
                let mut fixture = ResizeFixture::native(direction);
                let start = match direction {
                    PaneSplitDirection::Row => Vec2::new(start, 250.0),
                    PaneSplitDirection::Column => Vec2::new(250.0, start),
                };
                let end = match direction {
                    PaneSplitDirection::Row => Vec2::new(end, 250.0),
                    PaneSplitDirection::Column => Vec2::new(250.0, end),
                };
                fixture.pointer(false, Some(start));
                fixture
                    .app
                    .world_mut()
                    .write_message(vmux_input::PointerInput::sample(
                        fixture.window.unwrap(),
                        start,
                        bevy_cef_core::prelude::NativeMouseButtons {
                            left: true,
                            ..default()
                        },
                        false,
                    ));
                fixture.pointer(true, Some(end));
                assert_eq!(
                    fixture.app.world().get::<PaneDrag>(fixture.split).is_some(),
                    captured
                );
                if captured {
                    fixture.app.update();
                    assert!((fixture.grows(fixture.left).0 - 1.4).abs() < 0.0001);
                    assert!((fixture.grows(fixture.right).0 - 0.6).abs() < 0.0001);
                }
            }
        }
    }

    #[test]
    fn expanded_grip_previews_capture_and_restores_the_cursor_at_each_scale() {
        for direction in [PaneSplitDirection::Row, PaneSplitDirection::Column] {
            for (scale, coordinate, outside) in [(1.0, 505.0, 520.0), (2.0, 510.0, 530.0)] {
                let mut fixture = ResizeFixture::native(direction);
                let window = fixture.window.unwrap();
                fixture
                    .app
                    .world_mut()
                    .entity_mut(window)
                    .insert(CursorIcon::from(SystemCursorIcon::Pointer));
                for entity in [fixture.left, fixture.right] {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<ComputedNode>(entity)
                        .unwrap()
                        .inverse_scale_factor = 1.0 / scale;
                }
                let (position, moved, missed, icon) = match direction {
                    PaneSplitDirection::Row => (
                        Vec2::new(coordinate, 250.0),
                        Vec2::new(coordinate + 100.0, 250.0),
                        Vec2::new(outside, 250.0),
                        SystemCursorIcon::ColResize,
                    ),
                    PaneSplitDirection::Column => (
                        Vec2::new(250.0, coordinate),
                        Vec2::new(250.0, coordinate + 100.0),
                        Vec2::new(250.0, outside),
                        SystemCursorIcon::RowResize,
                    ),
                };
                fixture.pointer(false, Some(position));
                assert_eq!(
                    fixture.app.world().get::<CursorIcon>(window),
                    Some(&CursorIcon::from(icon))
                );
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
                fixture.pointer(true, Some(position));
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_some());
                assert_eq!(
                    fixture.app.world().get::<CursorIcon>(window),
                    Some(&CursorIcon::from(SystemCursorIcon::Grabbing))
                );
                fixture.pointer(true, Some(moved));
                assert!(fixture.grows(fixture.left).0 > 1.0);
                assert!(fixture.grows(fixture.right).0 < 1.0);
                fixture.pointer(false, None);
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
                assert!(fixture.app.world().get::<ResizeGrip>(window).is_none());
                assert_eq!(
                    fixture.app.world().get::<CursorIcon>(window),
                    Some(&CursorIcon::from(SystemCursorIcon::Pointer))
                );
                fixture.pointer(false, Some(missed));
                fixture.pointer(true, Some(missed));
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
                fixture.pointer(true, Some(position));
                assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
                assert!(fixture.app.world().get::<ResizeGrip>(window).is_none());
            }
        }
    }

    #[test]
    fn split_gap_only_applies_on_split_axis() {
        let row = PaneSplitDirection::Row.gaps(8.0);
        let column = PaneSplitDirection::Column.gaps(8.0);

        assert_eq!(row.column_gap, Val::Px(8.0));
        assert_eq!(row.row_gap, Val::Px(0.0));
        assert_eq!(column.column_gap, Val::Px(0.0));
        assert_eq!(column.row_gap, Val::Px(8.0));
    }

    #[test]
    fn applying_split_gap_clears_cross_axis_gap() {
        let split = PaneSplit {
            direction: PaneSplitDirection::Row,
        };
        let mut node = Node {
            column_gap: Val::Px(16.0),
            row_gap: Val::Px(16.0),
            ..default()
        };

        split.apply_gaps(&mut node, 8.0);

        assert_eq!(node.column_gap, Val::Px(8.0));
        assert_eq!(node.row_gap, Val::Px(0.0));
    }

    #[test]
    fn resize_command_updates_node_and_persisted_size() {
        let mut fixture = ResizeFixture::new(1.0, 1.0);

        fixture.send(ResizeRequest::direction(PaneDirection::Right));

        let left = fixture.grows(fixture.left);
        let right = fixture.grows(fixture.right);
        assert!(left.0 > 1.0);
        assert!(right.0 < 1.0);
        assert_eq!(left.0, left.1);
        assert_eq!(right.0, right.1);
    }

    #[test]
    fn equalize_command_updates_node_and_persisted_size() {
        let mut fixture = ResizeFixture::new(2.0, 1.0);

        fixture.send(ResizeRequest::equalize());

        assert_eq!(fixture.grows(fixture.left), (1.0, 1.0));
        assert_eq!(fixture.grows(fixture.right), (1.0, 1.0));
    }
}
