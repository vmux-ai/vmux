use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use vmux_ecs::launcher::{RestoreKeyboardToStack, StackInPaneChosen};
use vmux_ecs::page::{HostsPage, PageReady};
use vmux_ecs::{Active, KeyboardOwner, PagePresented};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use crate::PendingWebviewReveal;
use crate::cef::Browser;
use crate::host::deadline::{DeadlineOwner, DeadlineScheduler};
use crate::hosted_page::{SurfaceHandoff, SurfacePresentationReady};
use crate::stack::Stack;
use crate::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};

pub struct PresentationPlugin;

#[derive(Component, Clone, Copy)]
pub struct StackRect(pub ComputedNode);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StackMotionSet;

#[derive(Component, Clone, Copy)]
struct RectMotion {
    from: ComputedNode,
    target: ComputedNode,
    started: std::time::Duration,
    window: Option<Entity>,
}

impl RectMotion {
    const DURATION: f32 = vmux_ui::motion::Motion::LAYOUT_MS as f32 / 1000.0;

    fn matches(&self, target: &ComputedNode) -> bool {
        self.target.center.distance(target.center) < 0.01
            && self.target.size.distance(target.size) < 0.01
    }

    fn value(&self, now: std::time::Duration) -> (ComputedNode, bool) {
        let linear =
            (now.saturating_sub(self.started).as_secs_f32() / Self::DURATION).clamp(0.0, 1.0);
        let progress = linear * linear * (3.0 - 2.0 * linear);
        let mut node = self.target;
        node.center = self.from.center.lerp(self.target.center, progress);
        node.size = self.from.size.lerp(self.target.size, progress);
        (node, linear >= 1.0)
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PresentationSettleSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PresentationFlushSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativePresentationSet {
    Place,
    Settle,
    Sync,
    Finalize,
}

#[derive(Message, Clone, Copy)]
pub struct NativeSurfaceReused {
    pub surface: Entity,
}

#[derive(Message, Clone, Copy)]
pub struct NativeSelectionCommitted;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeadlineScheduler>()
            .add_message::<bevy::window::WindowResized>()
            .add_systems(
                PostUpdate,
                (animate_frames, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(StackMotionSet)
                    .after(LayoutSystems::Layout),
            )
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_message::<FocusChanged>()
            .add_message::<NativeSurfaceReused>()
            .add_message::<NativeSelectionCommitted>()
            .configure_sets(
                PostUpdate,
                (
                    NativePresentationSet::Place,
                    NativePresentationSet::Settle,
                    NativePresentationSet::Sync,
                    NativePresentationSet::Finalize,
                )
                    .chain(),
            )
            .add_observer(wake)
            .add_systems(
                First,
                (expire, bevy::ecs::schedule::ApplyDeferred, retire).chain(),
            )
            .add_systems(
                Update,
                (focus_chosen_stack_in_pane, restore_keyboard_to_stack)
                    .before(crate::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                retain_previous_focus.after(crate::stack::ComputeFocusSet),
            )
            .add_systems(Update, schedule_deadline_wake)
            .add_systems(
                PostUpdate,
                settle
                    .in_set(PresentationSettleSet)
                    .after(crate::page_slot::PageSlotSet)
                    .after(crate::hosted_page::HandoffCommitSet)
                    .run_if(changed),
            )
            .add_systems(
                PostUpdate,
                bevy::ecs::schedule::ApplyDeferred
                    .in_set(PresentationFlushSet)
                    .after(PresentationSettleSet),
            )
            .add_systems(
                PostUpdate,
                visibility
                    .after(PresentationFlushSet)
                    .before(LayoutSystems::Layout)
                    .run_if(projection_changed),
            )
            .add_systems(
                PostUpdate,
                (
                    settle_native,
                    bevy::ecs::schedule::ApplyDeferred,
                    visibility,
                    bevy::ecs::schedule::ApplyDeferred,
                )
                    .chain()
                    .in_set(NativePresentationSet::Settle)
                    .run_if(on_message::<NativeSurfaceReused>),
            );
    }
}

fn animate_frames(
    time: Res<Time>,
    scheduler: Res<DeadlineScheduler>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    stacks: Query<
        (
            Entity,
            &ComputedNode,
            &ChildOf,
            Option<&StackRect>,
            Option<&RectMotion>,
        ),
        With<Stack>,
    >,
    panes: Query<&ComputedNode, With<crate::pane::Pane>>,
    hierarchy: crate::window::WindowHierarchy,
    tabs: crate::tab::TabHierarchy,
    toggling: Query<(), With<super::toggle::LayoutTransition>>,
    zooming: Query<(), With<super::zoom::PaneZoomTransition>>,
    mut finished_toggles: RemovedComponents<super::toggle::LayoutTransition>,
    mut finished_zooms: RemovedComponents<super::zoom::PaneZoomTransition>,
    mut resized: MessageReader<bevy::window::WindowResized>,
    mut commands: Commands,
) {
    let changed_windows = resized
        .read()
        .map(|event| event.window)
        .collect::<HashSet<_>>();
    let finished_windows = finished_toggles.read().collect::<HashSet<_>>();
    let finished_tabs = finished_zooms.read().collect::<HashSet<_>>();
    for (stack, computed, parent, previous, running) in &stacks {
        let target = *panes.get(parent.parent()).unwrap_or(computed);
        let owner = DeadlineOwner::StackAnimation(stack);
        if target.is_empty() {
            if previous.is_some_and(|previous| previous.0.is_empty()) && running.is_none() {
                continue;
            }
            commands
                .entity(stack)
                .insert(StackRect(target))
                .remove::<RectMotion>();
            scheduler.cancel(owner);
            continue;
        }
        let window = hierarchy.get(stack);
        let direct = previous.is_none_or(|previous| previous.0.is_empty())
            || window.is_some_and(|window| {
                toggling.contains(window)
                    || finished_windows.contains(&window)
                    || changed_windows.contains(&window)
            })
            || tabs
                .entity(stack)
                .is_some_and(|tab| zooming.contains(tab) || finished_tabs.contains(&tab));
        if direct {
            commands
                .entity(stack)
                .insert(StackRect(target))
                .remove::<RectMotion>();
            scheduler.cancel(owner);
            continue;
        }
        let previous = previous.unwrap().0;
        let changed = previous.center.distance(target.center) >= 0.01
            || previous.size.distance(target.size) >= 0.01;
        let motion = match running {
            Some(running) if running.window == window && running.matches(&target) => *running,
            Some(running) => RectMotion {
                from: running.value(time.elapsed()).0,
                target,
                started: time.elapsed(),
                window,
            },
            None if changed => RectMotion {
                from: previous,
                target,
                started: time.elapsed(),
                window,
            },
            None => {
                scheduler.cancel(owner);
                continue;
            }
        };
        let (node, complete) = motion.value(time.elapsed());
        commands.entity(stack).insert(StackRect(node));
        if complete {
            commands.entity(stack).remove::<RectMotion>();
            scheduler.cancel(owner);
        } else {
            commands.entity(stack).insert(motion);
            scheduler.schedule(
                owner,
                std::time::Instant::now() + std::time::Duration::from_millis(16),
                proxy.as_deref(),
            );
        }
    }
}

fn wake(_: On<Add, Transition>, proxy: Option<Res<EventLoopProxyWrapper>>) {
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

#[derive(Message, Clone, Copy)]
pub(crate) struct FocusChanged {
    pub previous: crate::active_pane::ActiveStack,
    pub current: crate::active_pane::ActiveStack,
}

#[derive(Component, Clone, Debug)]
pub struct Transition {
    outgoing: Entity,
    retire: Vec<Entity>,
    reset_tab: Option<Entity>,
    timeout: TimeoutDisposition,
    deadline: std::time::Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimeoutDisposition {
    Rollback,
    Commit,
}

impl PartialEq for Transition {
    fn eq(&self, other: &Self) -> bool {
        self.outgoing == other.outgoing
            && self.retire == other.retire
            && self.reset_tab == other.reset_tab
            && self.timeout == other.timeout
    }
}

impl Eq for Transition {}

impl Transition {
    pub fn retain(outgoing: Entity) -> Self {
        Self {
            outgoing,
            retire: Vec::new(),
            reset_tab: None,
            timeout: TimeoutDisposition::Rollback,
            deadline: Self::deadline(),
        }
    }

    pub fn retiring(outgoing: Entity, retire: Entity, pending: Option<&Self>) -> Self {
        let mut transition = Self {
            outgoing,
            retire: vec![retire],
            reset_tab: None,
            timeout: TimeoutDisposition::Commit,
            deadline: Self::deadline(),
        };
        if let Some(pending) = pending {
            transition.absorb(pending);
        }
        transition
    }

    pub fn reset_tab(mut self, tab: Entity) -> Self {
        self.reset_tab = Some(tab);
        self
    }

    pub fn outgoing(&self) -> Entity {
        self.outgoing
    }

    pub fn retired(&self) -> &[Entity] {
        &self.retire
    }

    fn absorb(&mut self, superseded: &Self) {
        let mut retained = self
            .retire
            .iter()
            .copied()
            .collect::<bevy::ecs::entity::EntityHashSet>();
        for entity in &superseded.retire {
            if retained.insert(*entity) {
                self.retire.push(*entity);
            }
        }
        if self.reset_tab.is_none() {
            self.reset_tab = superseded.reset_tab;
        }
        if superseded.timeout == TimeoutDisposition::Commit {
            self.timeout = TimeoutDisposition::Commit;
        }
    }

    pub(crate) fn retire(&mut self, entity: Entity) {
        if !self.retire.contains(&entity) {
            self.retire.push(entity);
        }
        self.timeout = TimeoutDisposition::Commit;
    }

    pub(crate) fn cancelable(&self) -> bool {
        self.retire.is_empty()
            && self.reset_tab.is_none()
            && self.timeout == TimeoutDisposition::Rollback
    }

    fn deadline() -> std::time::Instant {
        std::time::Instant::now() + std::time::Duration::from_secs(10)
    }

    fn timed_out(&self) -> bool {
        std::time::Instant::now() >= self.deadline
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct TransitionCommit<'w, 's> {
    tabs: Query<'w, 's, &'static Tab>,
    retiring: Query<'w, 's, (), With<Retiring>>,
    transitions: Query<'w, 's, &'static Transition>,
    commands: Commands<'w, 's>,
}

impl TransitionCommit<'_, '_> {
    fn apply(&mut self, transition: &Transition, retired: &mut HashSet<Entity>) {
        let mut pending = vec![transition.clone()];
        while let Some(transition) = pending.pop() {
            if let Some(tab) = transition.reset_tab
                && let Ok(tab_state) = self.tabs.get(tab)
            {
                self.commands
                    .entity(tab)
                    .insert(Tab {
                        name: tab_state.name.clone(),
                        startup_dir: None,
                    })
                    .remove::<(
                        TabWorkspace,
                        TabWorktree,
                        TabDirDecided,
                        TabWorktreeUnavailable,
                        crate::worktree::TabWorktreeReady,
                        crate::page_context::PendingWorkspaceSelection,
                    )>();
            }
            let mut obligations = transition.retire;
            if self.retiring.contains(transition.outgoing)
                && !obligations.contains(&transition.outgoing)
            {
                obligations.push(transition.outgoing);
            }
            for entity in obligations {
                if !retired.insert(entity) {
                    continue;
                }
                if let Ok(nested) = self.transitions.get(entity) {
                    pending.push(nested.clone());
                }
            }
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    Current,
    Incoming,
    Retained,
    #[default]
    Hidden,
}

impl Layer {
    pub fn visible(self) -> bool {
        self != Self::Hidden
    }

    pub fn interactive(self) -> bool {
        self == Self::Current
    }
}

#[derive(Component)]
pub struct Retiring;

#[derive(Component)]
struct Retired;

#[derive(Component)]
struct TransitionRollback;

#[derive(bevy::ecs::system::SystemParam)]
pub struct Projection<'w, 's> {
    transitions: Query<'w, 's, (Entity, &'static Transition, Has<Active>)>,
}

impl Projection<'_, '_> {
    pub fn outgoing(&self, incoming: Entity) -> Option<Entity> {
        self.transitions
            .get(incoming)
            .ok()
            .map(|(_, transition, _)| transition.outgoing())
    }

    pub fn visible(&self, selected: Entity, candidate: Entity) -> bool {
        if selected == candidate {
            return true;
        }
        let mut current = selected;
        let mut visited = Vec::new();
        for _ in 0..32 {
            if visited.contains(&current) {
                return false;
            }
            visited.push(current);
            let Some(outgoing) = self.outgoing(current) else {
                return false;
            };
            if outgoing == candidate {
                return true;
            }
            current = outgoing;
        }
        false
    }

    pub fn retained(&self, selected: Entity) -> Option<Entity> {
        let mut current = selected;
        let mut retained = None;
        let mut visited = Vec::new();
        for _ in 0..32 {
            if visited.contains(&current) {
                break;
            }
            visited.push(current);
            let Some(outgoing) = self.outgoing(current) else {
                break;
            };
            retained = Some(outgoing);
            current = outgoing;
        }
        retained
    }

    pub fn retaining(&self, candidate: Entity) -> Option<(Entity, Transition)> {
        for (incoming, transition, active) in &self.transitions {
            if !active || incoming == candidate || !self.visible(incoming, candidate) {
                continue;
            }
            return Some((incoming, transition.clone()));
        }
        None
    }

    fn cancelable_path(&self, selected: Entity, retained: Entity) -> Option<Vec<Entity>> {
        let mut current = selected;
        let mut owners = Vec::new();
        for _ in 0..32 {
            if current == retained {
                return Some(owners);
            }
            let (_, transition, _) = self.transitions.get(current).ok()?;
            if !transition.cancelable() || owners.contains(&current) {
                return None;
            }
            owners.push(current);
            current = transition.outgoing();
        }
        None
    }
}

fn retain_previous_focus(
    mut changes: MessageReader<FocusChanged>,
    projection: Projection,
    transitions: Query<(), With<Transition>>,
    entities: Query<(), Or<(With<Tab>, With<Stack>)>>,
    parents: Query<&ChildOf>,
    rollbacks: Query<(), With<TransitionRollback>>,
    mut commands: Commands,
) {
    for change in changes.read() {
        let pair = if change.previous.tab != change.current.tab {
            change.previous.tab.zip(change.current.tab)
        } else if change.previous.stack != change.current.stack {
            change.previous.stack.zip(change.current.stack)
        } else {
            None
        };
        let Some((outgoing, incoming)) = pair else {
            continue;
        };
        let same_container = match (parents.get(outgoing), parents.get(incoming)) {
            (Ok(outgoing_parent), Ok(incoming_parent)) => {
                outgoing_parent.parent() == incoming_parent.parent()
            }
            _ => false,
        };
        if !same_container {
            continue;
        }
        if rollbacks.contains(incoming) {
            commands.entity(incoming).try_remove::<TransitionRollback>();
            continue;
        }
        if let Some(cancelable) = projection.cancelable_path(outgoing, incoming) {
            for owner in cancelable {
                commands.entity(owner).try_remove::<Transition>();
            }
            continue;
        }
        if transitions.contains(incoming)
            || !entities.contains(outgoing)
            || !entities.contains(incoming)
        {
            continue;
        }
        let visible = projection.retained(outgoing).unwrap_or(outgoing);
        if visible != incoming {
            commands
                .entity(incoming)
                .try_insert(Transition::retain(visible));
        }
    }
}

fn expire(
    pending: Query<(Entity, &Transition)>,
    mut activated: Query<&mut LastActivatedAt>,
    mut commit: TransitionCommit,
) {
    let mut retired = HashSet::new();
    for (incoming, transition) in &pending {
        if !transition.timed_out() {
            continue;
        }
        commit.commands.entity(incoming).try_remove::<Transition>();
        if transition.timeout == TimeoutDisposition::Commit {
            commit.apply(transition, &mut retired);
            continue;
        }
        if let Ok(mut outgoing) = commit.commands.get_entity(transition.outgoing()) {
            outgoing
                .try_insert(TransitionRollback)
                .try_remove::<Retiring>();
        }
        if let Ok(mut timestamp) = activated.get_mut(transition.outgoing()) {
            *timestamp = LastActivatedAt::now();
        } else {
            commit
                .commands
                .entity(transition.outgoing())
                .try_insert(LastActivatedAt::now());
        }
        for entity in transition.retired() {
            commit.commands.entity(*entity).try_remove::<Retiring>();
        }
    }
    for entity in retired.iter().copied() {
        commit.commands.entity(entity).try_insert(Retired);
    }
}

fn schedule_deadline_wake(
    scheduler: Res<DeadlineScheduler>,
    changed: Query<(Entity, &Transition), Changed<Transition>>,
    transitions: Query<(Entity, &Transition)>,
    mut removed: RemovedComponents<Transition>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    for entity in removed.read() {
        scheduler.cancel(DeadlineOwner::Presentation(entity));
    }
    if proxy.as_ref().is_some_and(|proxy| proxy.is_added()) {
        for (entity, transition) in &transitions {
            scheduler.schedule(
                DeadlineOwner::Presentation(entity),
                transition.deadline,
                proxy.as_deref(),
            );
        }
        return;
    }
    for (entity, transition) in &changed {
        scheduler.schedule(
            DeadlineOwner::Presentation(entity),
            transition.deadline,
            proxy.as_deref(),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PresentationChanges<'w, 's> {
    transitions: Query<'w, 's, (), Changed<Transition>>,
    active: Query<'w, 's, (), (Or<(With<Tab>, With<Stack>)>, Changed<Active>)>,
    children: Query<
        'w,
        's,
        (),
        (
            Changed<Children>,
            Or<(
                With<Tab>,
                With<Stack>,
                With<crate::pane::Pane>,
                With<crate::pane::PaneSplit>,
            )>,
        ),
    >,
    slots: Query<
        'w,
        's,
        (),
        Or<(
            Changed<crate::page_slot::Current>,
            Changed<crate::page_slot::Transition>,
        )>,
    >,
    surfaces: Query<
        'w,
        's,
        (),
        Or<(
            Changed<Browser>,
            Changed<HostsPage>,
            Changed<PageReady>,
            Changed<PagePresented>,
            Changed<SurfacePresentationReady>,
            Changed<PendingWebviewReveal>,
            Changed<SurfaceHandoff>,
            Changed<crate::hosted_page::SurfaceBlocked>,
            Changed<vmux_ecs::PageTransition>,
        )>,
    >,
    removed_active: RemovedComponents<'w, 's, Active>,
    removed_slot_transitions: RemovedComponents<'w, 's, crate::page_slot::Transition>,
    removed_ready: RemovedComponents<'w, 's, PageReady>,
    removed_presented: RemovedComponents<'w, 's, PagePresented>,
    removed_presentation_ready: RemovedComponents<'w, 's, SurfacePresentationReady>,
    removed_pending_reveal: RemovedComponents<'w, 's, PendingWebviewReveal>,
    removed_handoff: RemovedComponents<'w, 's, SurfaceHandoff>,
    removed_blocked: RemovedComponents<'w, 's, crate::hosted_page::SurfaceBlocked>,
    removed_page_transition: RemovedComponents<'w, 's, vmux_ecs::PageTransition>,
}

impl PresentationChanges<'_, '_> {
    fn take(&mut self) -> bool {
        let active_removed = self.removed_active.read().count() > 0;
        let slot_transition_removed = self.removed_slot_transitions.read().count() > 0;
        let ready_removed = self.removed_ready.read().count() > 0;
        let presented_removed = self.removed_presented.read().count() > 0;
        let presentation_ready_removed = self.removed_presentation_ready.read().count() > 0;
        let pending_reveal_removed = self.removed_pending_reveal.read().count() > 0;
        let handoff_removed = self.removed_handoff.read().count() > 0;
        let blocked_removed = self.removed_blocked.read().count() > 0;
        let page_transition_removed = self.removed_page_transition.read().count() > 0;
        let removed = active_removed
            || slot_transition_removed
            || ready_removed
            || presented_removed
            || presentation_ready_removed
            || pending_reveal_removed
            || handoff_removed
            || blocked_removed
            || page_transition_removed;
        !self.transitions.is_empty()
            || !self.active.is_empty()
            || !self.children.is_empty()
            || !self.slots.is_empty()
            || !self.surfaces.is_empty()
            || removed
    }
}

fn changed(mut changes: PresentationChanges) -> bool {
    changes.take()
}

fn settle(
    pending: Query<(Entity, &Transition, Has<Active>)>,
    readiness: PresentationReadiness,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commit: TransitionCommit,
) {
    if pending.is_empty() {
        return;
    }
    let expired = pending
        .iter()
        .filter_map(|(incoming, transition, _)| transition.timed_out().then_some(incoming))
        .collect::<HashSet<_>>();
    let active_owners = pending
        .iter()
        .filter(|(incoming, _, active)| *active && !expired.contains(incoming))
        .map(|(incoming, transition, _)| (transition.outgoing(), incoming))
        .collect::<HashMap<_, _>>();
    let superseded = pending
        .iter()
        .filter(|(incoming, transition, active)| {
            !*active
                && !expired.contains(incoming)
                && active_owners.contains_key(&transition.outgoing())
        })
        .map(|(incoming, _, _)| incoming)
        .collect::<HashSet<_>>();
    let mut effective = pending
        .iter()
        .filter(|(incoming, _, _)| !expired.contains(incoming))
        .map(|(incoming, transition, _)| (incoming, transition.clone()))
        .collect::<HashMap<_, _>>();
    for incoming in &superseded {
        let Some(transition) = effective.remove(incoming) else {
            continue;
        };
        let Some(owner) = active_owners.get(&transition.outgoing()) else {
            continue;
        };
        if let Some(owner_transition) = effective.get_mut(owner) {
            owner_transition.absorb(&transition);
        }
    }
    for (incoming, transition, active) in &pending {
        if active
            && let Some(merged) = effective.get(&incoming)
            && merged != transition
        {
            commit.commands.entity(incoming).insert(merged.clone());
        }
    }
    let ready = pending
        .iter()
        .filter_map(|(incoming, _, _)| {
            if expired.contains(&incoming) || superseded.contains(&incoming) {
                return None;
            }
            readiness.descendant(incoming).then_some(incoming)
        })
        .collect::<HashSet<_>>();
    let retained = pending
        .iter()
        .filter(|(incoming, _, _)| {
            !expired.contains(incoming)
                && !superseded.contains(incoming)
                && !ready.contains(incoming)
        })
        .map(|(incoming, transition, _)| (incoming, transition.outgoing()))
        .collect::<Vec<_>>();
    let mut retired = HashSet::new();
    for (incoming, original, _) in &pending {
        if expired.contains(&incoming) {
            continue;
        }
        if superseded.contains(&incoming) {
            commit.commands.entity(incoming).try_remove::<Transition>();
            continue;
        }
        if !ready.contains(&incoming) {
            continue;
        }
        let transition = effective.get(&incoming).unwrap_or(original);
        commit.commands.entity(incoming).try_remove::<Transition>();
        commit.apply(transition, &mut retired);
    }
    let retained_outgoing = retained
        .iter()
        .filter_map(|(owner, outgoing)| (!retired.contains(owner)).then_some(*outgoing))
        .collect::<HashSet<_>>();
    for entity in retired.iter().copied() {
        if retained_outgoing.contains(&entity) {
            continue;
        }
        if let Ok(mut entity) = commit.commands.get_entity(entity) {
            entity.try_insert(Retired);
        }
    }
    if (!ready.is_empty() || !superseded.is_empty())
        && let Some(proxy) = proxy
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn retire(retired: Query<Entity, With<Retired>>, mut commands: Commands) {
    for entity in &retired {
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.try_despawn();
        }
    }
}

fn settle_native(
    mut reused: MessageReader<NativeSurfaceReused>,
    pending: Query<(Entity, &Transition, Has<Active>)>,
    retiring: Query<(), With<Retiring>>,
    readiness: PresentationReadiness,
    mut committed: MessageWriter<NativeSelectionCommitted>,
    mut commands: Commands,
) {
    let mut surfaces = HashSet::new();
    for event in reused.read() {
        surfaces.insert(event.surface);
    }
    let mut changed = false;
    for (incoming, transition, active) in &pending {
        if !active
            || !transition.cancelable()
            || transition.timed_out()
            || retiring.contains(transition.outgoing())
            || pending.contains(transition.outgoing())
            || !readiness.descendant(incoming)
            || readiness.reused_branch(incoming, &surfaces) != Some(true)
        {
            continue;
        }
        commands.entity(incoming).try_remove::<Transition>();
        changed = true;
    }
    if changed {
        committed.write(NativeSelectionCommitted);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PresentationReadiness<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    stacks: Query<'w, 's, (), With<Stack>>,
    active_stacks: Query<'w, 's, (), (With<Stack>, With<Active>)>,
    surfaces: Query<'w, 's, (), With<Browser>>,
    slots: Query<
        'w,
        's,
        (
            &'static crate::page_slot::Current,
            Option<&'static crate::page_slot::Transition>,
        ),
        With<Stack>,
    >,
    hosted_ready: Query<
        'w,
        's,
        (),
        (
            With<Browser>,
            With<HostsPage>,
            With<PageReady>,
            With<PagePresented>,
            With<SurfacePresentationReady>,
            Without<PendingWebviewReveal>,
            Without<SurfaceHandoff>,
            Without<crate::hosted_page::SurfaceBlocked>,
            Without<vmux_ecs::PageTransition>,
        ),
    >,
    direct_ready: Query<
        'w,
        's,
        (),
        (
            With<Browser>,
            Without<HostsPage>,
            With<PageReady>,
            With<PagePresented>,
            With<SurfacePresentationReady>,
            Without<PendingWebviewReveal>,
            Without<SurfaceHandoff>,
            Without<crate::hosted_page::SurfaceBlocked>,
            Without<vmux_ecs::PageTransition>,
        ),
    >,
}

impl PresentationReadiness<'_, '_> {
    fn reused_branch(&self, entity: Entity, reused: &HashSet<Entity>) -> Option<bool> {
        if self.stacks.contains(entity) {
            if !self.active_stacks.contains(entity) {
                return None;
            }
            let Ok((current, _)) = self.slots.get(entity) else {
                return Some(false);
            };
            let Some(surface) = current.surface() else {
                return Some(false);
            };
            return Some(self.hosted_ready.contains(surface) && reused.contains(&surface));
        }
        if self.surfaces.contains(entity) {
            return Some(self.hosted_ready.contains(entity) && reused.contains(&entity));
        }
        let children = self.children.get(entity).ok()?;
        let mut found = false;
        for child in children.iter() {
            let Some(ready) = self.reused_branch(child, reused) else {
                continue;
            };
            found = true;
            if !ready {
                return Some(false);
            }
        }
        found.then_some(true)
    }

    fn descendant(&self, entity: Entity) -> bool {
        if self.stacks.contains(entity) {
            return self.stack(entity);
        }
        self.branch(entity).unwrap_or(false)
    }

    fn branch(&self, entity: Entity) -> Option<bool> {
        if self.stacks.contains(entity) {
            if !self.active_stacks.contains(entity) {
                return None;
            }
            return Some(self.stack(entity));
        }
        if self.surfaces.contains(entity) {
            return Some(self.hosted_ready.contains(entity) || self.direct_ready.contains(entity));
        }
        let descendants = self.children.get(entity).ok()?;
        let mut found = false;
        for child in descendants.iter() {
            let Some(ready) = self.branch(child) else {
                continue;
            };
            found = true;
            if !ready {
                return Some(false);
            }
        }
        found.then_some(true)
    }

    fn stack(&self, stack: Entity) -> bool {
        let hosted_fallback = match self.slots.get(stack) {
            Ok((current, transition)) => {
                if transition.is_some() {
                    return false;
                }
                if let Some(surface) = current.surface() {
                    if !self
                        .parents
                        .get(surface)
                        .is_ok_and(|parent| parent.parent() == stack)
                    {
                        return false;
                    }
                    return self.hosted_ready.contains(surface)
                        || self.direct_ready.contains(surface);
                }
                false
            }
            Err(_) => true,
        };
        let Ok(descendants) = self.children.get(stack) else {
            return false;
        };
        let mut found = false;
        for surface in descendants
            .iter()
            .filter(|surface| self.surfaces.contains(*surface))
        {
            found = true;
            let ready = self.direct_ready.contains(surface)
                || hosted_fallback && self.hosted_ready.contains(surface);
            if !ready {
                return false;
            }
        }
        found
    }
}

fn visibility(
    transitions: Query<(Entity, &Transition, Has<Active>), Or<(With<Tab>, With<Stack>)>>,
    mut projection: ParamSet<(
        Query<(Entity, &mut Layer, Has<Active>), Or<(With<Tab>, With<Stack>)>>,
        Query<(&Layer, &mut Node, &mut Visibility), With<Tab>>,
    )>,
) {
    let links = transitions
        .iter()
        .map(|(tab, transition, _)| (tab, transition.outgoing()))
        .collect::<HashMap<_, _>>();
    let mut retained = HashSet::new();
    for (tab, _, active) in &transitions {
        if !active {
            continue;
        }
        let mut current = tab;
        for _ in 0..32 {
            let Some(outgoing) = links.get(&current).copied() else {
                break;
            };
            if !retained.insert(outgoing) {
                break;
            }
            current = outgoing;
        }
    }
    {
        let mut items = projection.p0();
        for (entity, mut layer, active) in &mut items {
            let next = if active {
                if links.contains_key(&entity) {
                    Layer::Incoming
                } else {
                    Layer::Current
                }
            } else if retained.contains(&entity) {
                Layer::Retained
            } else {
                Layer::Hidden
            };
            if *layer != next {
                *layer = next;
            }
        }
    }
    let mut tabs = projection.p1();
    for (layer, mut node, mut visibility) in &mut tabs {
        let display = if layer.visible() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let next_visibility = if layer.visible() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != next_visibility {
            *visibility = next_visibility;
        }
    }
}

fn projection_changed(
    active: Query<(), (Or<(With<Tab>, With<Stack>)>, Changed<Active>)>,
    added: Query<(), (Or<(With<Tab>, With<Stack>)>, Added<Layer>)>,
    transitions: Query<(), Changed<Transition>>,
    mut removed_active: RemovedComponents<Active>,
    mut removed_transitions: RemovedComponents<Transition>,
) -> bool {
    let active_removed = removed_active.read().count() > 0;
    let transition_removed = removed_transitions.read().count() > 0;
    !active.is_empty()
        || !added.is_empty()
        || !transitions.is_empty()
        || active_removed
        || transition_removed
}

fn restore_keyboard_to_stack(
    mut requests: MessageReader<RestoreKeyboardToStack>,
    all_children: Query<&Children>,
    content_pages: Query<
        Entity,
        (
            With<Browser>,
            Without<crate::Header>,
            Without<crate::side_sheet::SideSheet>,
        ),
    >,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(children) = all_children.get(request.stack) else {
            continue;
        };
        for child in children.iter() {
            if content_pages.contains(child) {
                commands.entity(child).try_insert(KeyboardOwner);
            }
        }
    }
}

fn focus_chosen_stack_in_pane(
    mut chosen: MessageReader<StackInPaneChosen>,
    stacks: Query<(), (With<Stack>, Without<Retiring>)>,
    mut commands: Commands,
) {
    for event in chosen.read() {
        if !stacks.contains(event.stack) {
            continue;
        }
        commands.trigger(vmux_ecs::ActivateRequest {
            entity: event.stack,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    #[derive(Component)]
    struct NativeReuseCandidate;

    fn present_native_fixture(
        candidates: Query<Entity, With<NativeReuseCandidate>>,
        mut reused: MessageWriter<NativeSurfaceReused>,
        mut commands: Commands,
    ) {
        for surface in &candidates {
            commands
                .entity(surface)
                .insert((PagePresented, SurfacePresentationReady))
                .remove::<NativeReuseCandidate>();
            reused.write(NativeSurfaceReused { surface });
        }
    }

    struct NativeSelectionFixture {
        app: App,
        incoming: Entity,
        outgoing: Entity,
        surfaces: Vec<Entity>,
    }

    impl NativeSelectionFixture {
        fn new(tab: bool, proofs: &[bool]) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PresentationPlugin))
                .add_systems(
                    PostUpdate,
                    present_native_fixture
                        .in_set(NativePresentationSet::Place)
                        .after(LayoutSystems::Layout),
                );
            let (incoming, outgoing) = if tab {
                let outgoing = app.world_mut().spawn(Tab::default()).id();
                let incoming = app
                    .world_mut()
                    .spawn((Tab::default(), Active, Transition::retain(outgoing)))
                    .id();
                (incoming, outgoing)
            } else {
                let outgoing = app.world_mut().spawn(Stack::default()).id();
                let incoming = app
                    .world_mut()
                    .spawn((Stack::default(), Active, Transition::retain(outgoing)))
                    .id();
                (incoming, outgoing)
            };
            let mut surfaces = Vec::new();
            for proof in proofs {
                let stack = if tab {
                    let pane = app
                        .world_mut()
                        .spawn((crate::pane::Pane, ChildOf(incoming)))
                        .id();
                    app.world_mut()
                        .spawn((Stack::default(), Active, ChildOf(pane)))
                        .id()
                } else {
                    incoming
                };
                let surface = app
                    .world_mut()
                    .spawn((Browser, HostsPage, PageReady, ChildOf(stack)))
                    .id();
                if *proof {
                    app.world_mut()
                        .entity_mut(surface)
                        .insert(NativeReuseCandidate);
                } else {
                    app.world_mut()
                        .entity_mut(surface)
                        .insert((PagePresented, SurfacePresentationReady));
                }
                app.world_mut()
                    .entity_mut(stack)
                    .insert(crate::page_slot::Current::new(
                        Some(surface),
                        vmux_ecs::PageMetadata::default(),
                    ));
                surfaces.push(surface);
            }
            Self {
                app,
                incoming,
                outgoing,
                surfaces,
            }
        }
    }

    #[test]
    fn cached_native_selection_commits_after_placement_in_the_same_frame() {
        for tab in [false, true] {
            let mut fixture = NativeSelectionFixture::new(tab, &[true]);

            fixture.app.update();

            assert!(
                fixture
                    .app
                    .world()
                    .get::<Transition>(fixture.incoming)
                    .is_none()
            );
            assert_eq!(
                fixture.app.world().get::<Layer>(fixture.incoming),
                Some(&Layer::Current)
            );
            assert_eq!(
                fixture.app.world().get::<Layer>(fixture.outgoing),
                Some(&Layer::Hidden)
            );
            assert_eq!(
                fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<NativeSelectionCommitted>>()
                    .drain()
                    .count(),
                1
            );
        }
    }

    #[test]
    fn native_tab_selection_waits_for_every_active_stack_to_be_reused() {
        let mut fixture = NativeSelectionFixture::new(true, &[true, false]);

        fixture.app.update();

        assert!(
            fixture
                .app
                .world()
                .get::<Transition>(fixture.incoming)
                .is_some()
        );
        assert_eq!(
            fixture.app.world().get::<Layer>(fixture.outgoing),
            Some(&Layer::Retained)
        );
    }

    #[test]
    fn native_reuse_cannot_release_a_blocked_surface_or_a_retiring_transition() {
        for retiring in [false, true] {
            let mut fixture = NativeSelectionFixture::new(false, &[true]);
            if retiring {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.incoming)
                    .insert(Transition::retiring(
                        fixture.outgoing,
                        fixture.outgoing,
                        None,
                    ));
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.outgoing)
                    .insert(Retiring);
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.surfaces[0])
                    .insert(crate::hosted_page::SurfaceBlocked);
            }

            fixture.app.update();

            assert!(
                fixture
                    .app
                    .world()
                    .get::<Transition>(fixture.incoming)
                    .is_some()
            );
            assert_eq!(
                fixture.app.world().get::<Layer>(fixture.incoming),
                Some(&Layer::Incoming)
            );
            assert_eq!(
                fixture.app.world().get::<Layer>(fixture.outgoing),
                Some(&Layer::Retained)
            );
        }
    }

    #[test]
    fn hidden_panes_clear_motion_and_restore_their_rectangle_immediately() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let node = ComputedNode {
            size: Vec2::new(100.0, 80.0),
            center: Vec2::new(50.0, 40.0),
            ..Default::default()
        };
        let pane = app.world_mut().spawn((crate::pane::Pane, node)).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), node, StackRect(node), ChildOf(pane)))
            .id();
        app.update();
        app.world_mut()
            .entity_mut(pane)
            .insert(ComputedNode::default());
        app.world_mut().entity_mut(stack).insert(RectMotion {
            from: node,
            target: node,
            started: std::time::Duration::ZERO,
            window: None,
        });
        app.update();
        assert!(app.world().get::<StackRect>(stack).unwrap().0.is_empty());
        assert!(app.world().get::<RectMotion>(stack).is_none());
        app.world_mut().entity_mut(pane).insert(node);
        app.update();
        let presented = app.world().get::<StackRect>(stack).unwrap().0;
        assert_eq!(presented.center, node.center);
        assert_eq!(presented.size, node.size);
        assert!(app.world().get::<RectMotion>(stack).is_none());
    }

    #[test]
    fn finishing_a_layout_transition_commits_its_final_rectangle_without_another_animation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let window = app
            .world_mut()
            .spawn(super::super::toggle::LayoutTransition::finished())
            .id();
        let root = app
            .world_mut()
            .spawn(bevy_cef::prelude::HostWindow(window))
            .id();
        let before = ComputedNode {
            size: Vec2::new(100.0, 80.0),
            center: Vec2::new(50.0, 40.0),
            ..Default::default()
        };
        let pane = app
            .world_mut()
            .spawn((crate::pane::Pane, before, ChildOf(root)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), before, StackRect(before), ChildOf(pane)))
            .id();
        app.update();
        let after = ComputedNode {
            center: Vec2::new(250.0, 90.0),
            size: Vec2::new(200.0, 100.0),
            ..Default::default()
        };
        app.world_mut()
            .entity_mut(window)
            .remove::<super::super::toggle::LayoutTransition>();
        app.world_mut().entity_mut(pane).insert(after);
        app.update();
        let presented = app.world().get::<StackRect>(stack).unwrap().0;
        assert_eq!(presented.center, after.center);
        assert_eq!(presented.size, after.size);
        assert!(app.world().get::<RectMotion>(stack).is_none());
    }

    #[test]
    fn stack_rectangles_move_and_resize_from_the_presented_position() {
        let from = ComputedNode {
            center: Vec2::new(50.0, 40.0),
            size: Vec2::new(100.0, 80.0),
            ..Default::default()
        };
        let target = ComputedNode {
            center: Vec2::new(250.0, 90.0),
            size: Vec2::new(200.0, 100.0),
            ..Default::default()
        };
        let started = std::time::Duration::from_secs(5);
        let motion = RectMotion {
            from,
            target,
            started,
            window: None,
        };
        let (first, complete) = motion.value(started);
        assert_eq!(first.min(), Vec2::ZERO);
        assert!(!complete);
        let (middle, complete) = motion.value(started + std::time::Duration::from_millis(60));
        assert!(middle.center.distance(Vec2::new(150.0, 65.0)) < 0.001);
        assert!(middle.size.distance(Vec2::new(150.0, 90.0)) < 0.001);
        assert!(!complete);
        assert!(
            motion
                .value(started + std::time::Duration::from_millis(120))
                .1
        );
    }

    #[test]
    fn projection_assigns_one_incoming_and_one_retained_layer() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, visibility);
        let outgoing = app.world_mut().spawn(Tab::bundle()).id();
        let incoming = app
            .world_mut()
            .spawn((Tab::bundle(), Active, Transition::retain(outgoing)))
            .id();

        app.update();

        assert_eq!(app.world().get::<Layer>(incoming), Some(&Layer::Incoming));
        assert_eq!(app.world().get::<Layer>(outgoing), Some(&Layer::Retained));
    }

    #[test]
    fn newly_added_inactive_tab_is_hidden_without_an_active_change() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, visibility.run_if(projection_changed));
        app.world_mut()
            .spawn((Tab::default(), Active, Node::default(), Visibility::Visible));
        app.update();
        let inactive = app
            .world_mut()
            .spawn((Tab::default(), Node::default(), Visibility::Visible))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<Node>(inactive).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Visibility>(inactive),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn replacing_a_transition_keeps_all_commit_obligations() {
        let outgoing = Entity::from_bits(1);
        let first_retire = Entity::from_bits(2);
        let second_retire = Entity::from_bits(3);
        let tab = Entity::from_bits(4);
        let pending = Transition::retiring(outgoing, first_retire, None).reset_tab(tab);

        let next = Transition::retiring(outgoing, second_retire, Some(&pending));

        assert_eq!(next.retired(), [second_retire, first_retire]);
        assert_eq!(next.reset_tab, Some(tab));
    }

    #[test]
    fn presented_successor_retires_the_outgoing_item() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let incoming = app
            .world_mut()
            .spawn((
                Stack::default(),
                Active,
                Transition::retiring(outgoing, outgoing, None),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            PageReady {},
            PagePresented,
            SurfacePresentationReady,
            ChildOf(incoming),
        ));

        app.update();

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_none());

        app.update();

        assert!(app.world().get_entity(outgoing).is_err());
        assert!(app.world().get::<Transition>(incoming).is_none());
    }

    #[test]
    fn unpresented_successor_retains_the_outgoing_item() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn(Stack::default()).id();
        let incoming = app
            .world_mut()
            .spawn((Stack::default(), Active, Transition::retain(outgoing)))
            .id();
        app.world_mut()
            .spawn((Browser, PageReady {}, ChildOf(incoming)));

        app.update();

        assert_eq!(
            app.world().get::<Transition>(incoming),
            Some(&Transition::retain(outgoing)),
        );
    }

    #[test]
    fn expired_transition_restores_the_outgoing_branch() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app
            .world_mut()
            .spawn((Stack::default(), Retiring, LastActivatedAt(1)))
            .id();
        let mut transition = Transition::retain(outgoing);
        transition.deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);
        let incoming = app
            .world_mut()
            .spawn((Stack::default(), Active, transition))
            .id();

        app.update();

        assert!(app.world().get::<Transition>(incoming).is_none());
        assert!(app.world().get::<Retiring>(outgoing).is_none());
        assert!(app.world().get::<LastActivatedAt>(outgoing).unwrap().0 > 1);
    }

    #[test]
    fn expired_committed_transition_applies_tab_reset() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Worktree".into(),
                    startup_dir: Some("/tmp/worktree".into()),
                },
                TabWorkspace {
                    project_dir: "/tmp/worktree".into(),
                },
            ))
            .id();
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let mut transition = Transition::retiring(outgoing, outgoing, None).reset_tab(tab);
        transition.deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);
        let incoming = app
            .world_mut()
            .spawn((Stack::default(), Active, transition))
            .id();

        app.update();

        assert!(app.world().get::<Transition>(incoming).is_none());
        assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir, None);
        assert!(app.world().get::<TabWorkspace>(tab).is_none());
        assert!(app.world().get_entity(outgoing).is_err());
    }

    #[test]
    fn delegated_close_commits_when_the_transition_expires() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let mut transition = Transition::retain(outgoing);
        transition.retire(outgoing);
        transition.deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);
        let incoming = app
            .world_mut()
            .spawn((Stack::default(), Active, transition))
            .id();

        app.update();

        assert!(app.world().get::<Transition>(incoming).is_none());
        assert!(app.world().get_entity(outgoing).is_err());
    }

    #[test]
    fn hosted_successor_retires_only_after_the_page_slot_commits() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let incoming = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                Active,
                Transition::retiring(outgoing, outgoing, None),
            ))
            .id();
        let surface = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                PageReady {},
                PagePresented,
                ChildOf(incoming),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_some());

        app.world_mut()
            .entity_mut(incoming)
            .insert(crate::page_slot::Current::new(
                Some(surface),
                vmux_ecs::PageMetadata::default(),
            ));
        for _ in 0..3 {
            app.update();
        }

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_some());

        app.world_mut()
            .entity_mut(surface)
            .insert(SurfacePresentationReady);
        app.update();
        app.update();
        app.update();

        assert!(app.world().get_entity(outgoing).is_err());
        assert!(app.world().get::<Transition>(incoming).is_none());
    }

    #[test]
    fn retained_stack_surface_cannot_make_the_incoming_stack_ready() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn((Stack::bundle(), Retiring)).id();
        let surface = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                PageReady {},
                PagePresented,
                SurfacePresentationReady,
                ChildOf(outgoing),
            ))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                Active,
                Transition::retiring(outgoing, outgoing, None),
            ))
            .id();
        app.world_mut()
            .entity_mut(incoming)
            .insert(crate::page_slot::Current::new(
                Some(surface),
                vmux_ecs::PageMetadata::default(),
            ));

        for _ in 0..3 {
            app.update();
        }

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_some());
    }

    #[test]
    fn projection_keeps_the_entire_outgoing_chain_visible() {
        let mut app = App::new();
        let first = app.world_mut().spawn_empty().id();
        let middle = app.world_mut().spawn(Transition::retain(first)).id();
        let incoming = app.world_mut().spawn(Transition::retain(middle)).id();

        let visible = app
            .world_mut()
            .run_system_once(move |projection: Projection| projection.visible(incoming, first))
            .unwrap();

        assert!(visible);
    }

    #[test]
    fn retained_owner_prefers_the_active_transition() {
        let mut app = App::new();
        let retained = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(Transition::retain(retained));
        let active = app
            .world_mut()
            .spawn((Active, Transition::retain(retained)))
            .id();

        let owner = app
            .world_mut()
            .run_system_once(move |projection: Projection| projection.retaining(retained))
            .unwrap()
            .unwrap();

        assert_eq!(owner.0, active);
    }

    #[test]
    fn inactive_transition_does_not_own_a_retained_stack() {
        let mut app = App::new();
        let retained = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(Transition::retain(retained));

        let owner = app
            .world_mut()
            .run_system_once(move |projection: Projection| projection.retaining(retained))
            .unwrap();

        assert!(owner.is_none());
    }

    #[test]
    fn returning_to_a_retained_stack_cancels_the_transition() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let pane = app.world_mut().spawn(crate::pane::Pane).id();
        let first = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane)))
            .id();
        let second = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane)))
            .id();

        app.world_mut().write_message(FocusChanged {
            previous: crate::active_pane::ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: Some(first),
            },
            current: crate::active_pane::ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: Some(second),
            },
        });
        app.update();

        assert_eq!(
            app.world().get::<Transition>(second),
            Some(&Transition::retain(first))
        );

        app.world_mut().write_message(FocusChanged {
            previous: crate::active_pane::ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: Some(second),
            },
            current: crate::active_pane::ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: Some(first),
            },
        });
        app.update();

        assert!(app.world().get::<Transition>(second).is_none());
    }

    #[test]
    fn focus_between_panes_does_not_retain_the_previous_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let first_pane = app.world_mut().spawn(crate::pane::Pane).id();
        let second_pane = app.world_mut().spawn(crate::pane::Pane).id();
        let first = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(first_pane)))
            .id();
        let second = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(second_pane)))
            .id();

        app.world_mut().write_message(FocusChanged {
            previous: crate::active_pane::ActiveStack {
                tab: None,
                pane: Some(first_pane),
                stack: Some(first),
            },
            current: crate::active_pane::ActiveStack {
                tab: None,
                pane: Some(second_pane),
                stack: Some(second),
            },
        });
        app.update();

        assert!(app.world().get::<Transition>(second).is_none());
    }

    #[test]
    fn focus_between_spaces_does_not_retain_the_previous_tab() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let first_space = app.world_mut().spawn(crate::space::Space).id();
        let second_space = app.world_mut().spawn(crate::space::Space).id();
        let first = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(first_space)))
            .id();
        let second = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(second_space)))
            .id();

        app.world_mut().write_message(FocusChanged {
            previous: crate::active_pane::ActiveStack {
                tab: Some(first),
                pane: None,
                stack: None,
            },
            current: crate::active_pane::ActiveStack {
                tab: Some(second),
                pane: None,
                stack: None,
            },
        });
        app.update();

        assert!(app.world().get::<Transition>(second).is_none());
    }

    #[test]
    fn committing_a_nested_transition_retires_the_entire_chain() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let first = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let second = app
            .world_mut()
            .spawn((
                Stack::default(),
                Retiring,
                Transition::retiring(first, first, None),
            ))
            .id();
        let third = app
            .world_mut()
            .spawn((
                Stack::default(),
                Active,
                Transition::retiring(second, second, None),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            PageReady {},
            PagePresented,
            SurfacePresentationReady,
            ChildOf(third),
        ));

        app.update();
        app.update();

        assert!(app.world().get_entity(first).is_err());
        assert!(app.world().get_entity(second).is_err());
        assert!(app.world().get_entity(third).is_ok());
    }

    #[test]
    fn opening_stack_does_not_settle_from_its_ready_outgoing_surface() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            crate::page_slot::PageSlotPlugin,
            PresentationPlugin,
        ));
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let incoming = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                Active,
                Transition::retiring(outgoing, outgoing, None),
                vmux_ecs::PageNavigation::new(vmux_ecs::PageOpenId::new()),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            PageReady {},
            PagePresented,
            SurfacePresentationReady,
            ChildOf(incoming),
        ));
        let navigation = app
            .world()
            .get::<vmux_ecs::PageNavigation>(incoming)
            .unwrap()
            .id();
        app.world_mut().spawn((
            Browser,
            HostsPage,
            vmux_ecs::PageMetadata::default(),
            SurfaceHandoff::new(Vec::new(), navigation, 0, std::time::Duration::ZERO),
            ChildOf(incoming),
        ));

        app.update();

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_some());
        assert!(
            app.world()
                .get::<crate::page_slot::Transition>(incoming)
                .is_some()
        );
    }

    #[test]
    fn split_tab_waits_for_every_active_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let incoming = app
            .world_mut()
            .spawn((
                Tab::bundle(),
                Active,
                Transition::retiring(outgoing, outgoing, None),
            ))
            .id();
        let first_pane = app.world_mut().spawn(ChildOf(incoming)).id();
        let first_stack = app
            .world_mut()
            .spawn((Stack::default(), Active, ChildOf(first_pane)))
            .id();
        app.world_mut().spawn((
            Browser,
            PageReady {},
            PagePresented,
            SurfacePresentationReady,
            ChildOf(first_stack),
        ));
        let second_pane = app.world_mut().spawn(ChildOf(incoming)).id();
        let second_stack = app
            .world_mut()
            .spawn((Stack::default(), Active, ChildOf(second_pane)))
            .id();
        let second_surface = app
            .world_mut()
            .spawn((Browser, PageReady {}, ChildOf(second_stack)))
            .id();

        app.update();

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_some());

        app.world_mut()
            .entity_mut(second_surface)
            .insert((PagePresented, SurfacePresentationReady));
        app.update();

        assert!(app.world().get_entity(outgoing).is_ok());
        assert!(app.world().get::<Transition>(incoming).is_none());

        app.update();

        assert!(app.world().get_entity(outgoing).is_err());
        assert!(app.world().get::<Transition>(incoming).is_none());
    }

    #[test]
    fn inactive_superseded_transition_cannot_retire_a_shared_outgoing_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn((Stack::default(), Retiring)).id();
        let superseded = app
            .world_mut()
            .spawn((
                Stack::default(),
                Transition::retiring(outgoing, outgoing, None),
            ))
            .id();
        let current = app
            .world_mut()
            .spawn((Stack::default(), Active, Transition::retain(outgoing)))
            .id();

        app.update();

        assert!(app.world().get::<Transition>(superseded).is_none());
        assert!(app.world().get_entity(outgoing).is_ok());

        app.world_mut().spawn((
            Browser,
            PageReady {},
            PagePresented,
            SurfacePresentationReady,
            ChildOf(current),
        ));
        for _ in 0..3 {
            app.update();
        }

        assert!(app.world().get_entity(outgoing).is_err());
    }

    #[test]
    fn superseded_transition_transfers_retirement_and_tab_reset() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PresentationPlugin));
        let outgoing = app.world_mut().spawn(Stack::default()).id();
        let retired = app.world_mut().spawn(Stack::default()).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Worktree".into(),
                    startup_dir: Some("/tmp/worktree".into()),
                },
                TabWorkspace {
                    project_dir: "/tmp/worktree".into(),
                },
            ))
            .id();
        let superseded = app
            .world_mut()
            .spawn(Transition::retiring(outgoing, retired, None).reset_tab(tab))
            .id();
        let current = app
            .world_mut()
            .spawn((Stack::default(), Active, Transition::retain(outgoing)))
            .id();

        app.update();

        assert!(app.world().get::<Transition>(superseded).is_none());
        let transferred = app.world().get::<Transition>(current).unwrap();
        assert_eq!(transferred.retired(), [retired]);
        assert_eq!(transferred.reset_tab, Some(tab));

        app.world_mut().spawn((
            Browser,
            PageReady {},
            PagePresented,
            SurfacePresentationReady,
            ChildOf(current),
        ));
        for _ in 0..3 {
            app.update();
        }

        assert!(app.world().get_entity(retired).is_err());
        assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir, None);
        assert!(app.world().get::<TabWorkspace>(tab).is_none());
    }
}
