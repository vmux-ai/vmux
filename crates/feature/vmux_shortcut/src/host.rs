use crate::{
    ShortcutBinding, ShortcutCollectionViewRequest, ShortcutEntry, ShortcutGroup,
    ShortcutProbeClearRequest, ShortcutProbePress, ShortcutProbePressRequest, ShortcutProbeStatus,
    ShortcutStroke, ShortcutUiState, ShortcutUrl,
};
use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::collections::{BTreeMap, HashMap};
use vmux_command::{
    CommandDefinition, KeyCombo, KeyContext, Keymap, Modifiers, ResolvedKey, ResolvedLocale,
    Shortcut,
};
use vmux_ecs::page::PageReady;
use vmux_ecs::wake::Wake;
use vmux_ecs::workspace::ComputeFocusSet;
use vmux_ecs::{
    CollectionView, PageOpenSet, PageOpenTask, UiState, UiStatePlugin, UiStateWrite, UnixMillis,
};
use vmux_feature::manifest::FeaturePlugin;
use vmux_input::{NativeKeyCapture, NativeKeyInput, NativeKeyInputSet};
use vmux_layout::hosted_page::HostedUiPlugin;
use vmux_layout::space::{Space, SpaceHierarchy};
use vmux_layout::stack::{CloseStackRequest, FocusedStack};
use vmux_layout::window::WindowHierarchy;
use vmux_ui::i18n::Locale;

#[vmux_page::page]
pub struct ShortcutPlugin;

impl Plugin for ShortcutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::ShortcutPage::plugin());
        app.add_plugins((
            crate::claim::ClaimPlugin,
            crate::input::InputPlugin,
            HostedUiPlugin::<Shortcuts>::new(Self::manifest()),
            UiEventPlugin::<(
                ShortcutProbePressRequest,
                ShortcutProbeClearRequest,
                ShortcutCollectionViewRequest,
            )>::default(),
            UiStatePlugin::<ShortcutUiState>::default(),
        ))
        .add_message::<NativeKeyInput>()
        .add_message::<CloseStackRequest>()
        .add_observer(sync_ready)
        .add_observer(probe_press_request)
        .add_observer(probe_clear_request)
        .add_observer(collection_view)
        .add_observer(probe_press)
        .add_systems(Update, normalize_alias.in_set(PageOpenSet::ResolveTarget))
        .add_systems(
            Update,
            sync_capture
                .in_set(ShortcutCaptureSet)
                .after(ComputeFocusSet),
        )
        .add_systems(Update, capture_native_keys.after(NativeKeyInputSet))
        .add_systems(Update, (expire_probe, publish_state).chain());
    }
}

#[derive(Component, Default)]
#[require(UiState<ShortcutUiState>)]
#[require(
    ShortcutCapture,
    ProbeSequence,
    ProbeMatch,
    ProbePendingAt,
    CloseArm,
    CollectionView
)]
struct Shortcuts;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct ProbeSequence(Vec<ShortcutStroke>);

impl ProbeSequence {
    fn same_press(left: &ShortcutStroke, right: &ShortcutStroke) -> bool {
        left.code == right.code
            && left.ctrl == right.ctrl
            && left.shift == right.shift
            && left.alt == right.alt
            && left.super_key == right.super_key
    }

    fn after_prefix(stroke: &ShortcutStroke, prefix: &ShortcutStroke) -> ShortcutStroke {
        let mut next = stroke.clone();
        next.ctrl &= !prefix.ctrl;
        next.alt &= !prefix.alt;
        next.super_key &= !prefix.super_key;
        next
    }

    fn starts_with(strokes: &[ShortcutStroke], sequence: &[ShortcutStroke]) -> bool {
        sequence.len() <= strokes.len()
            && strokes
                .iter()
                .zip(sequence)
                .all(|(expected, actual)| Self::same_press(expected, actual))
    }

    fn matches(strokes: &[ShortcutStroke], sequence: &[ShortcutStroke]) -> bool {
        strokes.len() == sequence.len() && Self::starts_with(strokes, sequence)
    }

    fn is_plain_escape(stroke: &ShortcutStroke) -> bool {
        stroke.code == "Escape" && !stroke.ctrl && !stroke.shift && !stroke.alt && !stroke.super_key
    }

    fn accepts(&self, phase: ProbeMatch, shortcut: &ShortcutBinding) -> bool {
        if self.0.is_empty() {
            return true;
        }
        if phase == ProbeMatch::Missed {
            return false;
        }
        if !phase.is_contextual() && !shortcut.resolves {
            return false;
        }
        if phase.is_pending() {
            Self::starts_with(&shortcut.strokes, &self.0)
        } else {
            Self::matches(&shortcut.strokes, &self.0)
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ProbeMatch {
    #[default]
    Idle,
    Pending,
    PendingContextual,
    Direct,
    Contextual,
    Missed,
}

impl ProbeMatch {
    fn is_pending(self) -> bool {
        matches!(self, Self::Pending | Self::PendingContextual)
    }

    fn is_contextual(self) -> bool {
        matches!(self, Self::PendingContextual | Self::Contextual)
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProbePendingAt(Option<i64>);

impl ProbePendingAt {
    fn expired(self, now: i64, timeout_ms: u64) -> bool {
        self.0.is_some_and(|started| {
            now >= started.saturating_add(timeout_ms.min(i64::MAX as u64) as i64)
        })
    }

    fn exceeded(self, pressed_at_ms: i64, timeout_ms: u64) -> bool {
        self.0.is_some_and(|started| {
            pressed_at_ms > started.saturating_add(timeout_ms.min(i64::MAX as u64) as i64)
        })
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CloseArm(Option<i64>);

impl CloseArm {
    const WINDOW_MS: i64 = 1_000;

    fn arm_or_close(&mut self, input: &NativeKeyInput) -> bool {
        let closes = input.key == Some(KeyCode::KeyW)
            && input.modifiers.super_key
            && !input.modifiers.ctrl
            && !input.modifiers.shift
            && !input.modifiers.alt;
        if !closes {
            self.0 = None;
            return false;
        }
        let Some(armed_at) = self.0.replace(input.pressed_at_ms) else {
            return false;
        };
        if input.pressed_at_ms.saturating_sub(armed_at) > Self::WINDOW_MS {
            return false;
        }
        self.0 = None;
        true
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ShortcutCapture;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShortcutCaptureSet;

#[derive(SystemParam)]
struct ShortcutCaptureFocus<'w, 's> {
    views: Query<'w, 's, (Entity, &'static ChildOf), With<Shortcuts>>,
    hierarchy: WindowHierarchy<'w, 's>,
    windows: Query<'w, 's, &'static Window>,
    focus: FocusedStack<'w, 's>,
}

impl ShortcutCaptureFocus<'_, '_> {
    fn holds(&self, webview: Entity) -> bool {
        let Ok((_, parent)) = self.views.get(webview) else {
            return false;
        };
        if self.focus.stack != Some(parent.parent()) {
            return false;
        }
        let Some(window) = self.hierarchy.get(webview) else {
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

#[derive(SystemParam)]
struct CanonicalBindings<'w, 's> {
    keymaps: Query<'w, 's, Ref<'static, Keymap>>,
    definitions: Query<'w, 's, Ref<'static, CommandDefinition>>,
    locale: Option<Res<'w, ResolvedLocale>>,
}

impl CanonicalBindings<'_, '_> {
    fn chord_timeout_ms(&self) -> u64 {
        self.keymaps
            .single()
            .map(|keymap| keymap.chord_timeout_ms)
            .unwrap_or_default()
    }

    fn changed(&self) -> bool {
        if self
            .keymaps
            .single()
            .is_ok_and(|keymap| keymap.is_added() || keymap.is_changed())
        {
            return true;
        }
        if self
            .definitions
            .iter()
            .any(|definition| definition.is_added() || definition.is_changed())
        {
            return true;
        }
        self.locale
            .as_ref()
            .is_some_and(|locale| locale.is_added() || locale.is_changed())
    }

    fn groups(&self, context: &KeyContext) -> Vec<ShortcutGroup> {
        let Ok(keymap) = self.keymaps.single() else {
            return Vec::new();
        };
        let locale = self
            .locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(Locale::preferred);
        let mut labels = HashMap::new();
        for definition in &self.definitions {
            labels.insert(
                definition.id.clone(),
                definition.localized_name(locale.as_str()),
            );
        }

        let mut grouped: BTreeMap<String, BTreeMap<(String, String), Vec<ShortcutBinding>>> =
            BTreeMap::new();
        let view = keymap.in_context(context);
        for binding in keymap.bindings() {
            let Some(label) = labels.get(binding.command.as_str()) else {
                continue;
            };
            let (group, name) = match label.split_once(" > ") {
                Some((group, name)) => (group.to_string(), name.to_string()),
                None => (locale.translate("shortcuts-general"), label.clone()),
            };
            let shortcuts = grouped
                .entry(group)
                .or_default()
                .entry((name, binding.command.clone()))
                .or_default();
            let mut shortcut = Self::binding_from_shortcut(&binding.shortcut);
            shortcut.resolves = view.resolves(&binding.shortcut, &binding.command);
            if let Some(when) = binding.when.as_ref() {
                shortcut.contexts.push(when.to_string());
            }
            if let Some(existing) = shortcuts
                .iter_mut()
                .find(|existing| existing.strokes == shortcut.strokes)
            {
                existing.resolves |= shortcut.resolves;
                for when in shortcut.contexts {
                    if !existing.contexts.contains(&when) {
                        existing.contexts.push(when);
                    }
                }
            } else {
                shortcuts.push(shortcut);
            }
        }

        let mut groups = Vec::new();
        for (name, entries_map) in grouped {
            let mut entries = Vec::new();
            for ((entry_name, id), shortcuts) in entries_map {
                entries.push(ShortcutEntry {
                    id,
                    name: entry_name,
                    shortcuts,
                });
            }
            groups.push(ShortcutGroup { name, entries });
        }
        groups
    }

    fn resolve(
        &self,
        context: &KeyContext,
        sequence: &[ShortcutStroke],
    ) -> Option<(Vec<ShortcutStroke>, ProbeMatch)> {
        let groups = self.groups(context);
        for group in &groups {
            for entry in &group.entries {
                for shortcut in &entry.shortcuts {
                    if shortcut.resolves
                        && shortcut.strokes.len() > sequence.len()
                        && ProbeSequence::starts_with(&shortcut.strokes, sequence)
                    {
                        return Some((
                            shortcut.strokes[..sequence.len()].to_vec(),
                            ProbeMatch::Pending,
                        ));
                    }
                }
            }
        }
        for group in &groups {
            for entry in &group.entries {
                for shortcut in &entry.shortcuts {
                    if shortcut.resolves && ProbeSequence::matches(&shortcut.strokes, sequence) {
                        return Some((shortcut.strokes.clone(), ProbeMatch::Direct));
                    }
                }
            }
        }
        for group in &groups {
            for entry in &group.entries {
                for shortcut in &entry.shortcuts {
                    if shortcut.strokes.len() > sequence.len()
                        && ProbeSequence::starts_with(&shortcut.strokes, sequence)
                    {
                        return Some((
                            shortcut.strokes[..sequence.len()].to_vec(),
                            ProbeMatch::PendingContextual,
                        ));
                    }
                }
            }
        }
        for group in &groups {
            for entry in &group.entries {
                for shortcut in &entry.shortcuts {
                    if ProbeSequence::matches(&shortcut.strokes, sequence) {
                        return Some((shortcut.strokes.clone(), ProbeMatch::Contextual));
                    }
                }
            }
        }
        None
    }

    fn status(
        groups: &[ShortcutGroup],
        sequence: &ProbeSequence,
        phase: ProbeMatch,
    ) -> ShortcutProbeStatus {
        if sequence.0.is_empty() {
            return ShortcutProbeStatus::Idle;
        }
        if phase == ProbeMatch::Missed {
            return ShortcutProbeStatus::Miss;
        }
        if phase.is_pending() {
            return ShortcutProbeStatus::Pending;
        }
        if phase.is_contextual() {
            let mut labels = Vec::new();
            for group in groups {
                for entry in &group.entries {
                    for shortcut in &entry.shortcuts {
                        if !ProbeSequence::matches(&shortcut.strokes, &sequence.0) {
                            continue;
                        }
                        let contexts = shortcut.contexts.join(" / ");
                        let label = if contexts.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("{} · {}", entry.name, contexts)
                        };
                        if !labels.contains(&label) {
                            labels.push(label);
                        }
                    }
                }
            }
            return ShortcutProbeStatus::Contextual(labels);
        }
        let mut names = Vec::new();
        for group in groups {
            for entry in &group.entries {
                for shortcut in &entry.shortcuts {
                    if shortcut.resolves
                        && ProbeSequence::matches(&shortcut.strokes, &sequence.0)
                        && !names.contains(&entry.name)
                    {
                        names.push(entry.name.clone());
                    }
                }
            }
        }
        ShortcutProbeStatus::Match(names)
    }

    fn project(
        &self,
        context: &KeyContext,
        sequence: &ProbeSequence,
        phase: ProbeMatch,
        collection_view: CollectionView,
    ) -> ShortcutUiState {
        let all_groups = self.groups(context);
        let active = !sequence.0.is_empty();
        let mut shortcut_count = 0u32;
        let mut groups = Vec::new();
        for group in &all_groups {
            let mut entries = Vec::new();
            for entry in &group.entries {
                shortcut_count = shortcut_count.saturating_add(entry.shortcuts.len() as u32);
                if !entry
                    .shortcuts
                    .iter()
                    .any(|shortcut| sequence.accepts(phase, shortcut))
                {
                    continue;
                }
                let mut entry = entry.clone();
                for shortcut in &mut entry.shortcuts {
                    shortcut.emphasized = active && sequence.accepts(phase, shortcut);
                }
                entries.push(entry);
            }
            if !entries.is_empty() {
                groups.push(ShortcutGroup {
                    name: group.name.clone(),
                    entries,
                });
            }
        }
        let probe_status = Self::status(&all_groups, sequence, phase);
        ShortcutUiState {
            groups,
            probe_sequence: sequence.0.clone(),
            probe_status,
            shortcut_count,
            collection_view,
        }
    }

    fn binding_from_shortcut(shortcut: &Shortcut) -> ShortcutBinding {
        let strokes = match shortcut {
            Shortcut::Direct(combo) => vec![Self::stroke_from_key_combo(combo)],
            Shortcut::Chord(prefix, second) => vec![
                Self::stroke_from_key_combo(prefix),
                Self::stroke_from_key_combo(second),
            ],
        };
        ShortcutBinding {
            label: shortcut.display(),
            strokes,
            resolves: false,
            contexts: Vec::new(),
            emphasized: false,
        }
    }

    fn stroke_from_key_combo(combo: &KeyCombo) -> ShortcutStroke {
        ShortcutStroke {
            code: combo.code(),
            label: combo.key_label(),
            ctrl: combo.modifiers.ctrl,
            shift: combo.modifiers.shift,
            alt: combo.modifiers.alt,
            super_key: combo.modifiers.super_key,
        }
    }

    fn stroke_from_native_input(input: &NativeKeyInput) -> ShortcutStroke {
        let modifiers = Modifiers {
            ctrl: input.modifiers.ctrl,
            shift: input.modifiers.shift,
            alt: input.modifiers.alt,
            super_key: input.modifiers.super_key,
        };
        let resolved = input
            .key
            .map(|key| KeyCombo { key, modifiers })
            .or_else(|| {
                ResolvedKey::parse(&input.text).map(|key| KeyCombo {
                    key: key.key,
                    modifiers,
                })
            });
        if let Some(combo) = resolved {
            return Self::stroke_from_key_combo(&combo);
        }
        ShortcutStroke {
            code: format!("NativeKeyCode{}", input.native_code),
            label: if input.text.is_empty() {
                format!("0x{:02X}", input.native_code)
            } else {
                input.text.to_uppercase()
            },
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            super_key: modifiers.super_key,
        }
    }
}

fn sync_ready(
    trigger: On<UiInput<PageReady>>,
    mut views: Query<(&mut Shortcuts, &mut CollectionView)>,
    hierarchy: SpaceHierarchy,
    spaces: Query<&CollectionView, (With<Space>, Without<Shortcuts>)>,
) {
    let webview = trigger.event().webview;
    let Ok((mut marker, mut view)) = views.get_mut(webview) else {
        return;
    };
    *view = hierarchy
        .get(webview)
        .and_then(|space| spaces.get(space).ok())
        .copied()
        .unwrap_or_default();
    marker.set_changed();
}

fn collection_view(
    trigger: On<UiInput<ShortcutCollectionViewRequest>>,
    hierarchy: SpaceHierarchy,
    mut spaces: Query<&mut CollectionView, (With<Space>, Without<Shortcuts>)>,
    mut views: Query<&mut CollectionView, With<Shortcuts>>,
) {
    let webview = trigger.event().webview;
    let Ok(mut current_view) = views.get_mut(webview) else {
        return;
    };
    let view = trigger.event().payload.view;
    *current_view = view;
    let Some(space) = hierarchy.get(webview) else {
        return;
    };
    if let Ok(mut current) = spaces.get_mut(space) {
        *current = view;
    }
}

fn probe_press_request(
    trigger: On<UiInput<ShortcutProbePressRequest>>,
    views: Query<(), With<Shortcuts>>,
    mut commands: Commands,
) {
    let webview = trigger.event_target();
    if !views.contains(webview) {
        return;
    }
    commands.trigger(ShortcutProbePress::new(
        webview,
        trigger.event().payload.stroke.clone(),
        UnixMillis::now().0,
    ));
}

fn probe_clear_request(
    trigger: On<UiInput<ShortcutProbeClearRequest>>,
    mut views: Query<(&mut ProbeSequence, &mut ProbeMatch, &mut ProbePendingAt), With<Shortcuts>>,
) {
    let Ok((mut sequence, mut phase, mut pending_at)) = views.get_mut(trigger.event_target())
    else {
        return;
    };
    sequence.0.clear();
    *phase = ProbeMatch::Idle;
    pending_at.0 = None;
}

fn probe_press(
    trigger: On<ShortcutProbePress>,
    mut views: Query<
        (
            &mut ProbeSequence,
            &mut ProbeMatch,
            &mut ProbePendingAt,
            Option<&KeyContext>,
        ),
        With<Shortcuts>,
    >,
    bindings: CanonicalBindings,
) {
    let Ok((mut sequence, mut phase, mut pending_at, context)) =
        views.get_mut(trigger.event_target())
    else {
        return;
    };
    let stroke = trigger.event().stroke();
    let pressed_at_ms = trigger.event().pressed_at_ms();
    if ProbeSequence::is_plain_escape(stroke) {
        sequence.0.clear();
        *phase = ProbeMatch::Idle;
        pending_at.0 = None;
        return;
    }
    let timeout_ms = bindings.chord_timeout_ms();
    if phase.is_pending() && pending_at.exceeded(pressed_at_ms, timeout_ms) {
        sequence.0.clear();
        *phase = ProbeMatch::Idle;
        pending_at.0 = None;
    }
    let context = context.unwrap_or(KeyContext::NONE);
    let was_pending = phase.is_pending();
    let candidate = if was_pending {
        let second = ProbeSequence::after_prefix(stroke, &sequence.0[0]);
        vec![sequence.0[0].clone(), second]
    } else {
        vec![stroke.clone()]
    };
    if let Some((resolved, next_phase)) = bindings.resolve(context, &candidate) {
        sequence.0 = resolved;
        *phase = next_phase;
        pending_at.0 = next_phase.is_pending().then_some(pressed_at_ms);
        return;
    }
    if was_pending
        && let Some((resolved, next_phase)) =
            bindings.resolve(context, std::slice::from_ref(stroke))
    {
        sequence.0 = resolved;
        *phase = next_phase;
        pending_at.0 = next_phase.is_pending().then_some(pressed_at_ms);
        return;
    }
    sequence.0 = vec![stroke.clone()];
    *phase = ProbeMatch::Missed;
    pending_at.0 = None;
}

fn sync_capture(
    focus: ShortcutCaptureFocus,
    mut captures: Query<(Entity, Has<NativeKeyCapture>, &mut CloseArm), With<ShortcutCapture>>,
    mut commands: Commands,
) {
    let next = focus.active();
    for (entity, active, mut close_arm) in &mut captures {
        let should_capture = next == Some(entity);
        if active == should_capture {
            continue;
        }
        if should_capture {
            commands.entity(entity).insert(NativeKeyCapture);
        } else {
            close_arm.0 = None;
            commands.entity(entity).remove::<NativeKeyCapture>();
        }
    }
}

type NativeCaptures<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut CloseArm,
        &'static mut ProbeSequence,
        &'static mut ProbeMatch,
        &'static mut ProbePendingAt,
        &'static ChildOf,
    ),
    (With<ShortcutCapture>, With<NativeKeyCapture>),
>;

fn capture_native_keys(
    mut inputs: MessageReader<NativeKeyInput>,
    mut captures: NativeCaptures,
    mut close_requests: MessageWriter<CloseStackRequest>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some((target, mut close_arm, mut sequence, mut phase, mut pending_at, parent)) =
        captures.iter_mut().next()
    else {
        inputs.clear();
        return;
    };
    for input in inputs.read() {
        if !input.captured || input.repeat {
            continue;
        }
        if input.releases_capture() {
            close_arm.0 = None;
            commands.entity(target).remove::<NativeKeyCapture>();
            continue;
        }
        if close_arm.arm_or_close(input) {
            sequence.0.clear();
            *phase = ProbeMatch::Idle;
            pending_at.0 = None;
            {
                let _wake = Wake::beside(proxy.as_deref());
                close_requests.write(CloseStackRequest::by_user(parent.parent()));
            }
            continue;
        }
        commands.trigger(ShortcutProbePress::new(
            target,
            CanonicalBindings::stroke_from_native_input(input),
            input.pressed_at_ms,
        ));
    }
}

fn expire_probe(
    mut views: Query<(&mut ProbeSequence, &mut ProbeMatch, &mut ProbePendingAt), With<Shortcuts>>,
    bindings: CanonicalBindings,
) {
    let now = UnixMillis::now().0;
    let timeout_ms = bindings.chord_timeout_ms();
    for (mut sequence, mut phase, mut pending_at) in &mut views {
        if !phase.is_pending() || !pending_at.expired(now, timeout_ms) {
            continue;
        }
        sequence.0.clear();
        *phase = ProbeMatch::Idle;
        pending_at.0 = None;
    }
}

type ProjectedViews<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, Shortcuts>,
        Ref<'static, ProbeSequence>,
        Ref<'static, ProbeMatch>,
        Ref<'static, CollectionView>,
        Option<Ref<'static, KeyContext>>,
    ),
>;

fn publish_state(views: ProjectedViews, bindings: CanonicalBindings, mut commands: Commands) {
    let canonical_changed = bindings.changed();
    for (entity, marker, sequence, phase, collection_view, context) in &views {
        let context_changed = context.as_ref().is_some_and(Ref::is_changed);
        if !canonical_changed
            && !marker.is_changed()
            && !sequence.is_changed()
            && !phase.is_changed()
            && !collection_view.is_changed()
            && !context_changed
        {
            continue;
        }
        let context = context.as_deref().unwrap_or(KeyContext::NONE);
        let state = bindings.project(context, &sequence, *phase, *collection_view);
        commands.trigger(UiStateWrite::<ShortcutUiState>::from_event(entity, &state));
    }
}

fn normalize_alias(mut tasks: Query<&mut PageOpenTask, Changed<PageOpenTask>>) {
    for mut task in &mut tasks {
        if let Some(canonical) = ShortcutUrl::canonical(&task) {
            task.url = canonical.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::{BinHostEmitEvent, BinIpcEventRawBuffer, Browsers};
    use vmux_api::input::KeyModifiers;
    use vmux_api::{BinEvent, UiEventPermissions, UiState as UiStateContract, UiStateFrame};
    use vmux_command::{Binding, Source, When};
    use vmux_ecs::{PageMetadata, PageNavigation, PageOpenId};
    use vmux_feature::manifest::FeatureManifest;
    use vmux_layout::active_pane::ActiveStack;
    use vmux_layout::hosted_page::HostedPagePlugin;

    #[derive(Resource, Default)]
    struct PublishedStates(Vec<(Entity, ShortcutUiState)>);

    impl PublishedStates {
        fn record(trigger: On<BinHostEmitEvent>, mut published: ResMut<Self>) {
            if trigger.id() != ShortcutUiState::id() {
                return;
            }
            let Ok(frame) = rkyv::from_bytes::<
                UiStateFrame<ShortcutUiState, ShortcutUiState>,
                rkyv::rancor::Error,
            >(trigger.payload()) else {
                return;
            };
            let previous = published
                .0
                .iter()
                .rev()
                .find(|(page, _)| *page == trigger.webview())
                .map(|(_, state)| state.clone());
            let state = match frame {
                UiStateFrame::Snapshot(state) => state,
                UiStateFrame::Updates(updates) => ShortcutUiState::from_updates(previous, updates),
            };
            published.0.push((trigger.webview(), state));
            trigger.mark_delivered();
        }
    }

    struct LayoutCommandFixture;

    impl LayoutCommandFixture {
        fn definitions(panes: bool, tabs: bool) -> Vec<CommandDefinition> {
            CommandDefinition::from_manifest(&FeatureManifest::parse(include_str!(
                "../../vmux_layout/src/feature.ron"
            )))
            .filter(|definition| {
                panes && matches!(definition.id.as_str(), "close_pane" | "rotate_forward")
                    || tabs && definition.id == "open_in_new_tab"
            })
            .collect()
        }

        fn stroke(code: &str, ctrl: bool) -> ShortcutStroke {
            ShortcutStroke {
                code: code.to_string(),
                label: code.to_string(),
                ctrl,
                ..Default::default()
            }
        }

        fn probe_app(keymap: Keymap, definitions: Vec<CommandDefinition>) -> (App, Entity) {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<vmux_command::CommandInvocation>()
                .init_resource::<BinIpcEventRawBuffer>()
                .init_resource::<PublishedStates>()
                .insert_resource(ResolvedLocale(Locale::from("en-US")))
                .add_plugins(ShortcutPlugin)
                .add_observer(PublishedStates::record);
            app.insert_non_send(Browsers::default());
            app.world_mut().spawn((
                Keymap::default(),
                UiEventPermissions {
                    url: ShortcutPlugin::url(),
                    owns_subtree: false,
                    permissions: &[ShortcutUiState::PERMISSION],
                },
            ));
            for definition in definitions {
                app.world_mut().spawn(definition);
            }
            let page = app
                .world_mut()
                .spawn((
                    Shortcuts,
                    PageMetadata {
                        url: ShortcutPlugin::url().to_string(),
                        ..Default::default()
                    },
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                ))
                .id();
            app.world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(page);
            app.update();
            let mut keymaps = app.world_mut().query::<&mut Keymap>();
            *keymaps.single_mut(app.world_mut()).unwrap() = keymap;
            app.update();
            (app, page)
        }

        fn chord_app() -> (App, Entity) {
            let mut keymap = Keymap::default();
            keymap.chord_timeout_ms = 1_000;
            let prefix = KeyCombo {
                key: KeyCode::KeyG,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            };
            let second = KeyCombo {
                key: KeyCode::KeyX,
                modifiers: Modifiers::default(),
            };
            keymap.register(["stack_close"]);
            keymap.extend(
                Source::Settings,
                [Binding {
                    shortcut: Shortcut::Chord(prefix, second),
                    command: "stack_close".into(),
                    when: None,
                }],
            );
            let definitions = vec![CommandDefinition::new(
                "stack_close",
                "Close Stack",
                "Stack",
            )];
            Self::probe_app(keymap, definitions)
        }

        fn latest_state(app: &App, page: Entity) -> ShortcutUiState {
            app.world()
                .resource::<PublishedStates>()
                .0
                .iter()
                .rev()
                .find(|(entity, _)| *entity == page)
                .map(|(_, state)| state.clone())
                .expect("published ShortcutUiState")
        }
    }

    #[test]
    fn probe_waits_for_and_resolves_a_chord() {
        let (mut app, page) = LayoutCommandFixture::chord_app();
        let now = UnixMillis::now().0;

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyG", true),
            now,
        ));
        app.update();
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Pending
        );
        assert_eq!(
            LayoutCommandFixture::latest_state(&app, page).probe_status,
            ShortcutProbeStatus::Pending
        );

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyX", true),
            now + 500,
        ));
        app.update();
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Direct
        );
        assert_eq!(
            LayoutCommandFixture::latest_state(&app, page).probe_status,
            ShortcutProbeStatus::Match(vec!["Stack > Close Stack".to_string()])
        );
    }

    #[test]
    fn probe_restarts_from_a_failed_chord_second_key() {
        let mut keymap = Keymap::default();
        keymap.chord_timeout_ms = 1_000;
        keymap.register(["stack_close", "new"]);
        keymap.extend(
            Source::Settings,
            [
                Binding {
                    shortcut: Shortcut::Chord(
                        KeyCombo {
                            key: KeyCode::KeyG,
                            modifiers: Modifiers {
                                ctrl: true,
                                ..Modifiers::default()
                            },
                        },
                        KeyCombo {
                            key: KeyCode::KeyX,
                            modifiers: Modifiers::default(),
                        },
                    ),
                    command: "stack_close".into(),
                    when: None,
                },
                Binding {
                    shortcut: Shortcut::Direct(KeyCombo {
                        key: KeyCode::KeyN,
                        modifiers: Modifiers::default(),
                    }),
                    command: "new".into(),
                    when: None,
                },
            ],
        );
        let definitions = vec![
            CommandDefinition::new("stack_close", "Close Stack", "Stack"),
            CommandDefinition::new("new", "New", "Stack"),
        ];
        let (mut app, page) = LayoutCommandFixture::probe_app(keymap, definitions);
        let now = UnixMillis::now().0;

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyG", true),
            now,
        ));
        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyN", false),
            now + 500,
        ));
        app.update();

        assert_eq!(
            app.world().get::<ProbeSequence>(page).unwrap().0,
            vec![ShortcutStroke {
                code: "KeyN".to_string(),
                label: "N".to_string(),
                ..Default::default()
            }]
        );
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Direct
        );
    }

    #[test]
    fn chord_prefix_wins_over_a_direct_binding() {
        let mut keymap = Keymap::default();
        keymap.chord_timeout_ms = 1_000;
        let ctrl_b = KeyCombo {
            key: KeyCode::KeyB,
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        };
        keymap.register(["stack_close", "leader"]);
        keymap.extend(
            Source::Settings,
            [
                Binding {
                    shortcut: Shortcut::Chord(
                        ctrl_b,
                        KeyCombo {
                            key: KeyCode::KeyX,
                            modifiers: Modifiers::default(),
                        },
                    ),
                    command: "stack_close".into(),
                    when: None,
                },
                Binding {
                    shortcut: Shortcut::Direct(ctrl_b),
                    command: "leader".into(),
                    when: None,
                },
            ],
        );
        let definitions = vec![
            CommandDefinition::new("stack_close", "Close Stack", "Stack"),
            CommandDefinition::new("leader", "Leader", "Stack"),
        ];
        let (mut app, page) = LayoutCommandFixture::probe_app(keymap, definitions);
        let now = UnixMillis::now().0;

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyB", true),
            now,
        ));
        app.update();
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Pending
        );

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyX", false),
            now + 500,
        ));
        app.update();
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Direct
        );
        assert_eq!(
            LayoutCommandFixture::latest_state(&app, page).probe_status,
            ShortcutProbeStatus::Match(vec!["Stack > Close Stack".to_string()])
        );
    }

    #[test]
    fn capture_escape_clears_the_current_shortcut() {
        let (mut app, page) = LayoutCommandFixture::chord_app();
        let now = UnixMillis::now().0;

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyG", true),
            now,
        ));
        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("Escape", false),
            now + 100,
        ));
        app.update();

        assert!(app.world().get::<ProbeSequence>(page).unwrap().0.is_empty());
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Idle
        );
    }

    #[test]
    fn pending_probe_expires_in_host_state() {
        let (mut app, page) = LayoutCommandFixture::chord_app();
        let now = UnixMillis::now().0;

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyG", true),
            now,
        ));
        app.update();
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Pending
        );

        app.world_mut().get_mut::<ProbePendingAt>(page).unwrap().0 = Some(now - 2_000);
        app.update();

        assert!(app.world().get::<ProbeSequence>(page).unwrap().0.is_empty());
        assert_eq!(
            *app.world().get::<ProbeMatch>(page).unwrap(),
            ProbeMatch::Idle
        );
        assert_eq!(
            LayoutCommandFixture::latest_state(&app, page).probe_status,
            ShortcutProbeStatus::Idle
        );
    }

    #[test]
    fn projected_groups_include_only_matching_shortcuts() {
        let (mut app, page) = LayoutCommandFixture::chord_app();
        let now = UnixMillis::now().0;

        app.world_mut().trigger(ShortcutProbePress::new(
            page,
            LayoutCommandFixture::stroke("KeyG", true),
            now,
        ));
        app.update();
        let state = LayoutCommandFixture::latest_state(&app, page);

        assert_eq!(state.groups.len(), 1);
        assert_eq!(state.groups[0].entries.len(), 1);
        assert!(state.groups[0].entries[0].shortcuts[0].emphasized);
        assert_eq!(state.probe_status, ShortcutProbeStatus::Pending);
    }

    #[test]
    fn canonical_keymap_updates_reproject_without_retained_catalog() {
        let definitions = LayoutCommandFixture::definitions(true, false);
        let keymap = Keymap::defaults_with(&definitions);
        let (mut app, page) = LayoutCommandFixture::probe_app(keymap, definitions);
        let initial_count = LayoutCommandFixture::latest_state(&app, page).shortcut_count;

        for definition in LayoutCommandFixture::definitions(false, true) {
            app.world_mut().spawn(definition);
        }
        app.update();

        let state = LayoutCommandFixture::latest_state(&app, page);
        assert!(state.shortcut_count > initial_count);
        assert!(
            state
                .groups
                .iter()
                .flat_map(|group| &group.entries)
                .any(|entry| entry.id == "open_in_new_tab")
        );
    }

    #[test]
    fn projection_lists_hidden_and_visible_shortcuts() {
        let definitions = LayoutCommandFixture::definitions(true, true);
        let keymap = Keymap::defaults_with(&definitions);
        let (app, page) = LayoutCommandFixture::probe_app(keymap, definitions);
        let state = LayoutCommandFixture::latest_state(&app, page);
        let names = state
            .groups
            .iter()
            .flat_map(|group| group.entries.iter())
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            names
                .iter()
                .any(|name| name.contains("Rotate Panes Forward"))
        );
        assert!(names.iter().any(|name| name.contains("Open in New Tab")));
    }

    #[test]
    fn projection_exposes_chords_as_individual_strokes() {
        let definitions = LayoutCommandFixture::definitions(true, false);
        let keymap = Keymap::defaults_with(&definitions);
        let (app, page) = LayoutCommandFixture::probe_app(keymap, definitions);
        let state = LayoutCommandFixture::latest_state(&app, page);
        let shortcut = state
            .groups
            .iter()
            .flat_map(|group| &group.entries)
            .filter(|entry| entry.id == "close_pane")
            .flat_map(|entry| &entry.shortcuts)
            .find(|shortcut| shortcut.strokes.len() == 2)
            .expect("close pane chord");

        assert_eq!(shortcut.strokes[0].code, "KeyB");
        assert!(shortcut.strokes[0].ctrl);
        assert_eq!(shortcut.strokes[1].code, "KeyX");
        assert_eq!(shortcut.strokes[1].label, "X");
        assert!(!shortcut.strokes[1].ctrl);
    }

    #[test]
    fn projection_marks_the_binding_selected_by_runtime_context() {
        let mut keymap = Keymap::default();
        let combo = KeyCombo {
            key: KeyCode::Escape,
            modifiers: Modifiers::default(),
        };
        keymap.extend(
            Source::Settings,
            [
                Binding {
                    shortcut: Shortcut::Direct(combo),
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
        let definitions = vec![
            CommandDefinition::new("close_pane", "Close Pane", "Layout > Pane"),
            CommandDefinition::new("stack_close", "Close Stack", "Layout > Stack"),
        ];
        keymap.register(definitions.iter().map(|definition| definition.id.as_str()));

        let (app, page) = LayoutCommandFixture::probe_app(keymap, definitions);
        let state = LayoutCommandFixture::latest_state(&app, page);
        let resolving = state
            .groups
            .iter()
            .flat_map(|group| &group.entries)
            .filter(|entry| entry.shortcuts.iter().any(|shortcut| shortcut.resolves))
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(resolving, ["close_pane"]);
    }

    #[test]
    fn page_open_spawns_shortcuts_view() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BinIpcEventRawBuffer>()
            .add_plugins(HostedPagePlugin)
            .add_plugins(ShortcutPlugin);
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: ShortcutPlugin::url().to_string(),
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
            .init_resource::<BinIpcEventRawBuffer>()
            .add_plugins(HostedPagePlugin)
            .add_plugins(ShortcutPlugin);
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "vmux://cheatsheet/".to_string(),
            request_id: None,
        });

        app.update();

        let metadata = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(metadata.url, ShortcutPlugin::url());
    }

    #[test]
    fn focused_live_shortcuts_view_automatically_owns_capture() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BinIpcEventRawBuffer>()
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
        app.world_mut().spawn(
            ActiveStack {
                stack: Some(stack),
                ..default()
            }
            .local_bundle(),
        );

        app.update();
        assert!(app.world().entity(page).contains::<NativeKeyCapture>());
        app.world_mut()
            .resource_mut::<Messages<NativeKeyInput>>()
            .write(NativeKeyInput {
                key: Some(KeyCode::KeyH),
                native_code: 0x04,
                text: "h".to_string(),
                modifiers: KeyModifiers::default(),
                repeat: false,
                captured: true,
                claim: None,
                pressed_at_ms: 1,
            });
        app.update();
        assert_eq!(
            app.world().get::<ProbeSequence>(page).unwrap().0,
            vec![ShortcutStroke {
                code: "KeyH".to_string(),
                label: "H".to_string(),
                ..default()
            }]
        );
        for pressed_at_ms in [2, 3] {
            app.world_mut()
                .resource_mut::<Messages<NativeKeyInput>>()
                .write(NativeKeyInput {
                    key: Some(KeyCode::KeyW),
                    native_code: 0x0D,
                    text: "w".to_string(),
                    modifiers: KeyModifiers {
                        super_key: true,
                        ..Default::default()
                    },
                    repeat: false,
                    captured: true,
                    claim: None,
                    pressed_at_ms,
                });
        }
        app.update();
        let closes = app
            .world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].stack, stack);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert!(!app.world().entity(page).contains::<NativeKeyCapture>());
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert!(app.world().entity(page).contains::<NativeKeyCapture>());
        app.world_mut().despawn(page);
        app.update();
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<NativeKeyCapture>>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
