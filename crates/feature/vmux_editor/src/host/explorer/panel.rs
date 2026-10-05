use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::FileUiStateWrite;
use vmux_ecs::event::{
    ExplorerFilesToggle, ExplorerOpenEditorsToggle, ExplorerOutlineToggle, ExplorerPanelEvent,
    ExplorerPanelSetVisible, ExplorerPanelViewSet, ExplorerPanelViewportWidth, ExplorerPanelWidth,
    ExplorerReveal,
};
use vmux_ecs::page::PageReady;
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_setting::{
    AppSettings, EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH,
    SettingsSaveRequest,
};

use super::{ExplorerPanelDefaults, ExplorerPanelSent, RevealCurrent};
use crate::host::editor::FileView;

const EXPLORER_SQUEEZE_TOLERANCE_PX: u32 = 160;
const EDITOR_MIN_WIDTH_PX: u32 = 320;
const NOTE_MAX_CONTENT_WIDTH_PX: u32 = 768;

pub(super) struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_defaults)
            .register_persisted::<StackExplorerVisibility>()
            .add_systems(Update, (load_defaults, mark_document, reflow, emit).chain())
            .add_observer(toggle)
            .add_observer(reveal_in)
            .add_observer(open_find_in_files)
            .add_observer(set_visible)
            .add_observer(view_set)
            .add_observer(open_editors)
            .add_observer(files)
            .add_observer(outline)
            .add_observer(viewport_width)
            .add_observer(width);
    }
}

fn spawn_defaults(mut commands: Commands) {
    commands.spawn((
        Name::new("Explorer panel defaults"),
        ExplorerPanelDefaults {
            default_visible: false,
            width: EXPLORER_DEFAULT_WIDTH,
            loaded: false,
        },
    ));
}

#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_editor::plugin"]
pub struct StackExplorerVisibility {
    pub visible: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct StackExplorerView {
    search: bool,
    search_focus_revision: u64,
    open_editors: bool,
    files: bool,
    outline: bool,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ExplorerPanelViewport {
    width: u32,
    visible: bool,
    user_chose: bool,
}

#[derive(Component)]
struct ExplorerPanelReflow;

#[derive(Component)]
struct ExplorerPanelAutoShow;

impl Default for StackExplorerView {
    fn default() -> Self {
        Self {
            search: false,
            search_focus_revision: 0,
            open_editors: true,
            files: true,
            outline: true,
        }
    }
}

#[derive(EntityEvent)]
pub(crate) struct ExplorerToggleRequest {
    #[event_target]
    entity: Entity,
}

impl From<Entity> for ExplorerToggleRequest {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}

#[derive(EntityEvent)]
pub(crate) struct ExplorerRevealRequest {
    #[event_target]
    entity: Entity,
}

impl From<Entity> for ExplorerRevealRequest {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}

#[derive(EntityEvent)]
pub(crate) struct ExplorerFindInFilesRequest {
    #[event_target]
    entity: Entity,
}

impl From<Entity> for ExplorerFindInFilesRequest {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}

type PanelUnsentReady = (With<FileView>, Without<ExplorerPanelSent>, With<PageReady>);

fn load_defaults(
    settings: Option<Res<AppSettings>>,
    mut panel: Single<&mut ExplorerPanelDefaults>,
    views: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    if panel.loaded {
        return;
    }
    let Some(settings) = settings else {
        return;
    };
    panel.default_visible = settings.editor.explorer.visible();
    panel.width = settings.editor.explorer.width();
    panel.loaded = true;
    for entity in &views {
        commands
            .entity(entity)
            .insert(ExplorerPanelReflow)
            .remove::<ExplorerPanelSent>();
    }
}

fn emit(
    views: Query<(Entity, Option<&ChildOf>, Option<&ExplorerPanelViewport>), PanelUnsentReady>,
    visibility: Query<&StackExplorerVisibility>,
    panel_views: Query<&StackExplorerView>,
    panel: Single<&ExplorerPanelDefaults>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (entity, child_of, viewport) in &views {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let scope = child_of.map(ChildOf::parent).unwrap_or(entity);
        let preferred_visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        let visible = viewport
            .map(|viewport| viewport.visible)
            .unwrap_or(preferred_visible);
        let view = panel_views.get(scope).copied().unwrap_or_default();
        commands.trigger(FileUiStateWrite::from_event(
            entity,
            &ExplorerPanelEvent {
                visible,
                width: panel.width,
                search: view.search,
                open_editors: view.open_editors,
                files: view.files,
                outline: view.outline,
                search_focus_revision: view.search_focus_revision,
            },
        ));
        commands.entity(entity).insert(ExplorerPanelSent);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ExplorerPanelMutation<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    visibility: Query<'w, 's, &'static mut StackExplorerVisibility>,
    panel_views: Query<'w, 's, &'static mut StackExplorerView>,
    viewports: Query<'w, 's, &'static mut ExplorerPanelViewport>,
    editors: Query<'w, 's, (Entity, Option<&'static ChildOf>), With<FileView>>,
    commands: Commands<'w, 's>,
}

impl ExplorerPanelMutation<'_, '_> {
    fn scope(&self, entity: Entity) -> Entity {
        self.child_of
            .get(entity)
            .map(ChildOf::parent)
            .unwrap_or(entity)
    }

    fn visible(&self, scope: Entity, fallback: bool) -> bool {
        self.visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(fallback)
    }

    fn effective(&self, entity: Entity, scope: Entity, fallback: bool) -> bool {
        self.viewports
            .get(entity)
            .map(|viewport| viewport.visible)
            .unwrap_or_else(|_| self.visible(scope, fallback))
    }

    fn set_panel(&mut self, scope: Entity, visibility: StackExplorerVisibility) {
        if let Ok(mut state) = self.visibility.get_mut(scope) {
            *state = visibility;
        } else {
            self.commands.entity(scope).insert(visibility);
        }
    }

    fn set_view(&mut self, scope: Entity, search: bool) {
        if let Ok(mut view) = self.panel_views.get_mut(scope) {
            view.search = search;
            if search {
                view.search_focus_revision = view.search_focus_revision.wrapping_add(1).max(1);
            }
        } else {
            self.commands.entity(scope).insert(StackExplorerView {
                search,
                search_focus_revision: u64::from(search),
                ..default()
            });
        }
    }

    fn set_effective(&mut self, scope: Entity, visible: bool, user_chose: bool) {
        for (entity, parent) in &self.editors {
            let view_scope = parent.map(ChildOf::parent).unwrap_or(entity);
            if view_scope != scope {
                continue;
            }
            if let Ok(mut viewport) = self.viewports.get_mut(entity) {
                viewport.visible = visible;
                viewport.user_chose = user_chose;
            }
            self.commands
                .entity(entity)
                .remove::<(ExplorerPanelAutoShow, ExplorerPanelSent)>();
        }
    }

    fn toggle_open_editors(&mut self, scope: Entity) {
        if let Ok(mut view) = self.panel_views.get_mut(scope) {
            view.open_editors = !view.open_editors;
        } else {
            self.commands.entity(scope).insert(StackExplorerView {
                open_editors: false,
                ..default()
            });
        }
    }

    fn toggle_files(&mut self, scope: Entity) {
        if let Ok(mut view) = self.panel_views.get_mut(scope) {
            view.files = !view.files;
        } else {
            self.commands.entity(scope).insert(StackExplorerView {
                files: false,
                ..default()
            });
        }
    }

    fn toggle_outline(&mut self, scope: Entity) {
        if let Ok(mut view) = self.panel_views.get_mut(scope) {
            view.outline = !view.outline;
        } else {
            self.commands.entity(scope).insert(StackExplorerView {
                outline: false,
                ..default()
            });
        }
    }

    fn mark_unsent(&mut self) {
        for (entity, _) in &self.editors {
            self.commands.entity(entity).remove::<ExplorerPanelSent>();
        }
    }

    fn mark_scope_unsent(&mut self, scope: Entity) {
        for (entity, parent) in &self.editors {
            let view_scope = parent.map(ChildOf::parent).unwrap_or(entity);
            if view_scope == scope {
                self.commands.entity(entity).remove::<ExplorerPanelSent>();
            }
        }
    }
}

fn toggle(
    trigger: On<ExplorerToggleRequest>,
    panel: Single<&ExplorerPanelDefaults>,
    mut mutation: ExplorerPanelMutation,
) {
    let entity = trigger.event_target();
    let scope = mutation.scope(entity);
    let visible = mutation.effective(entity, scope, panel.default_visible);
    mutation.set_panel(scope, StackExplorerVisibility { visible: !visible });
    mutation.set_effective(scope, !visible, true);
}

fn reveal_in(trigger: On<ExplorerRevealRequest>, mut mutation: ExplorerPanelMutation) {
    let entity = trigger.event_target();
    let scope = mutation.scope(entity);
    mutation.set_panel(scope, StackExplorerVisibility { visible: true });
    mutation.set_effective(scope, true, true);
    mutation.set_view(scope, false);
    mutation.commands.trigger(RevealCurrent {
        entity,
        reveal: ExplorerReveal::Requested,
    });
}

fn open_find_in_files(
    trigger: On<ExplorerFindInFilesRequest>,
    mut mutation: ExplorerPanelMutation,
) {
    let entity = trigger.event_target();
    let scope = mutation.scope(entity);
    mutation.set_panel(scope, StackExplorerVisibility { visible: true });
    mutation.set_effective(scope, true, true);
    mutation.set_view(scope, true);
}

fn set_visible(trigger: On<UiInput<ExplorerPanelSetVisible>>, mut mutation: ExplorerPanelMutation) {
    let entity = trigger.event().webview;
    let scope = mutation.scope(entity);
    let next_visibility = StackExplorerVisibility {
        visible: trigger.event().payload.visible,
    };
    mutation.set_panel(scope, next_visibility);
    mutation.set_effective(scope, next_visibility.visible, true);
    if next_visibility.visible {
        mutation.commands.trigger(RevealCurrent {
            entity,
            reveal: ExplorerReveal::Followed,
        });
    }
}

fn view_set(trigger: On<UiInput<ExplorerPanelViewSet>>, mut mutation: ExplorerPanelMutation) {
    let entity = trigger.event().webview;
    let scope = mutation.scope(entity);
    mutation.set_view(scope, trigger.event().payload.search);
    mutation.mark_scope_unsent(scope);
}

fn open_editors(
    trigger: On<UiInput<ExplorerOpenEditorsToggle>>,
    mut mutation: ExplorerPanelMutation,
) {
    let scope = mutation.scope(trigger.event().webview);
    mutation.toggle_open_editors(scope);
    mutation.mark_scope_unsent(scope);
}

fn files(trigger: On<UiInput<ExplorerFilesToggle>>, mut mutation: ExplorerPanelMutation) {
    let scope = mutation.scope(trigger.event().webview);
    mutation.toggle_files(scope);
    mutation.mark_scope_unsent(scope);
}

fn outline(trigger: On<UiInput<ExplorerOutlineToggle>>, mut mutation: ExplorerPanelMutation) {
    let scope = mutation.scope(trigger.event().webview);
    mutation.toggle_outline(scope);
    mutation.mark_scope_unsent(scope);
}

fn viewport_width(
    trigger: On<UiInput<ExplorerPanelViewportWidth>>,
    mut viewports: Query<&mut ExplorerPanelViewport>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let width = trigger.event().payload.px;
    if let Ok(mut viewport) = viewports.get_mut(entity) {
        if viewport.width == width {
            return;
        }
        viewport.width = width;
    } else {
        commands
            .entity(entity)
            .insert(ExplorerPanelViewport { width, ..default() });
    }
    commands
        .entity(entity)
        .insert(ExplorerPanelReflow)
        .remove::<ExplorerPanelSent>();
}

fn mark_document(views: Query<Entity, Changed<FileView>>, mut commands: Commands) {
    for entity in &views {
        commands
            .entity(entity)
            .insert((ExplorerPanelAutoShow, ExplorerPanelReflow))
            .remove::<ExplorerPanelSent>();
    }
}

type ReflowViews<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static ChildOf>,
        &'static mut ExplorerPanelViewport,
        Option<&'static ExplorerPanelAutoShow>,
    ),
    With<ExplorerPanelReflow>,
>;

fn reflow(
    mut views: ReflowViews,
    mut visibility: Query<&mut StackExplorerVisibility>,
    panel: Single<&ExplorerPanelDefaults>,
    mut commands: Commands,
) {
    for (entity, child_of, mut viewport, auto_show) in &mut views {
        if viewport.width == 0 {
            continue;
        }
        let scope = child_of.map(ChildOf::parent).unwrap_or(entity);
        let mut preferred = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        let mut needed = NOTE_MAX_CONTENT_WIDTH_PX.saturating_add(panel.width);
        if viewport.visible {
            needed = needed.saturating_sub(EXPLORER_SQUEEZE_TOLERANCE_PX);
        }
        let fits = viewport.width >= needed;
        let leaves_editor_usable =
            viewport.width >= panel.width.saturating_add(EDITOR_MIN_WIDTH_PX);
        if auto_show.is_some() && fits {
            preferred = true;
            viewport.visible = true;
            if let Ok(mut state) = visibility.get_mut(scope) {
                state.visible = true;
            } else {
                commands
                    .entity(scope)
                    .insert(StackExplorerVisibility { visible: true });
            }
        } else {
            viewport.visible = preferred && (viewport.user_chose || fits && leaves_editor_usable);
        }
        commands
            .entity(entity)
            .remove::<(ExplorerPanelAutoShow, ExplorerPanelReflow)>()
            .remove::<ExplorerPanelSent>();
    }
}

fn width(
    trigger: On<UiInput<ExplorerPanelWidth>>,
    mut panel: Single<&mut ExplorerPanelDefaults>,
    settings: Option<ResMut<AppSettings>>,
    saves: Option<ResMut<Messages<SettingsSaveRequest>>>,
    mut mutation: ExplorerPanelMutation,
) {
    panel.width = trigger
        .event()
        .payload
        .px
        .clamp(EXPLORER_MIN_WIDTH, EXPLORER_MAX_WIDTH);
    if let Some(mut settings) = settings {
        settings.editor.explorer.width = Some(panel.width);
        if let Some(mut saves) = saves {
            saves.write(SettingsSaveRequest);
        }
    }
    for (entity, _) in &mutation.editors {
        mutation.commands.entity(entity).insert(ExplorerPanelReflow);
    }
    mutation.mark_unsent();
}
