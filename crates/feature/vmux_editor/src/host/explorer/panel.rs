use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{ExplorerPanelEvent, ExplorerPanelSetVisible, ExplorerPanelWidth};

use super::{ExplorerPanelDefaults, ExplorerPanelSent, RevealCurrent, StackExplorerRevision};
use crate::host::editor::FileView;

pub(super) struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_explorer_panel_defaults)
            .register_type::<StackExplorerVisibility>()
            .add_systems(Update, (load_explorer_panel_defaults, emit_explorer_panel))
            .add_observer(on_explorer_panel_set_visible)
            .add_observer(on_explorer_panel_width);
    }
}

fn spawn_explorer_panel_defaults(mut commands: Commands) {
    commands.spawn((
        Name::new("Explorer panel defaults"),
        ExplorerPanelDefaults {
            default_visible: false,
            width: vmux_setting::EXPLORER_DEFAULT_WIDTH,
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

type PanelUnsentReady = (
    With<FileView>,
    Without<ExplorerPanelSent>,
    With<vmux_core::page::PageReady>,
);

fn load_explorer_panel_defaults(
    settings: Option<Res<vmux_setting::AppSettings>>,
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
        commands.entity(entity).remove::<ExplorerPanelSent>();
    }
}

fn emit_explorer_panel(
    views: Query<(Entity, Option<&ChildOf>), PanelUnsentReady>,
    visibility: Query<&StackExplorerVisibility>,
    revisions: Query<&StackExplorerRevision>,
    panel: Single<&ExplorerPanelDefaults>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (entity, child_of) in &views {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let scope = child_of.map(ChildOf::parent).unwrap_or(entity);
        let visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        let revision = revisions.get(scope).copied().unwrap_or_default();
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &ExplorerPanelEvent {
                visible,
                width: panel.width,
                client_id: revision.client_id,
                request_id: revision.request_id,
            },
        ));
        commands.entity(entity).insert(ExplorerPanelSent);
    }
}

fn persist_explorer_width(
    width: u32,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>>,
) {
    let Some(mut settings) = settings else {
        return;
    };
    settings.editor.explorer.width = Some(width);
    if let Some(mut saves) = saves {
        saves.write(vmux_setting::SettingsSaveRequest);
    }
}

fn mark_explorer_panel_unsent(views: &Query<Entity, With<FileView>>, commands: &mut Commands) {
    for entity in views {
        commands.entity(entity).remove::<ExplorerPanelSent>();
    }
}

fn apply_stack_explorer_panel(
    scope: Entity,
    visibility: StackExplorerVisibility,
    revision: StackExplorerRevision,
    visibilities: &mut Query<&mut StackExplorerVisibility>,
    revisions: &mut Query<&mut StackExplorerRevision>,
    commands: &mut Commands,
) {
    if let Ok(mut state) = visibilities.get_mut(scope) {
        *state = visibility;
    } else {
        commands.entity(scope).insert(visibility);
    }
    if let Ok(mut state) = revisions.get_mut(scope) {
        *state = revision;
    } else {
        commands.entity(scope).insert(revision);
    }
}

#[allow(clippy::too_many_arguments)]
fn on_explorer_panel_set_visible(
    trigger: On<UiInput<ExplorerPanelSetVisible>>,
    child_of: Query<&ChildOf>,
    mut visibility: Query<&mut StackExplorerVisibility>,
    mut revisions: Query<&mut StackExplorerRevision>,
    editors: Query<(Entity, Option<&ChildOf>), With<FileView>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let scope = child_of.get(entity).map(ChildOf::parent).unwrap_or(entity);
    let next_visibility = StackExplorerVisibility {
        visible: trigger.event().payload.visible,
    };
    let next_revision = StackExplorerRevision {
        client_id: trigger.event().payload.client_id,
        request_id: trigger.event().payload.request_id,
    };
    apply_stack_explorer_panel(
        scope,
        next_visibility,
        next_revision,
        &mut visibility,
        &mut revisions,
        &mut commands,
    );
    for (view, parent) in &editors {
        let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
        if view_scope != scope {
            continue;
        }
        if view == entity {
            commands.entity(view).insert(ExplorerPanelSent);
        } else {
            commands.entity(view).remove::<ExplorerPanelSent>();
        }
    }
    if next_visibility.visible {
        commands.trigger(RevealCurrent {
            entity,
            reveal: vmux_core::event::ExplorerReveal::Followed,
        });
    }
}

fn on_explorer_panel_width(
    trigger: On<UiInput<ExplorerPanelWidth>>,
    mut panel: Single<&mut ExplorerPanelDefaults>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>>,
    views: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    panel.width = trigger.event().payload.px.clamp(
        vmux_setting::EXPLORER_MIN_WIDTH,
        vmux_setting::EXPLORER_MAX_WIDTH,
    );
    persist_explorer_width(panel.width, settings, saves);
    mark_explorer_panel_unsent(&views, &mut commands);
}
