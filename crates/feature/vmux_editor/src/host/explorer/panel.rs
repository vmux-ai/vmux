use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{
    ExplorerPanelEvent, ExplorerPanelSetVisible, ExplorerPanelViewSet, ExplorerPanelWidth,
    ExplorerReveal,
};
use vmux_core::host::FileUiStateWrite;
use vmux_core::host::persistence::PersistenceAppExt;
use vmux_core::page::PageReady;
use vmux_setting::{
    AppSettings, EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH,
    SettingsSaveRequest,
};

use super::{ExplorerPanelDefaults, ExplorerPanelSent, RevealCurrent, StackExplorerRevision};
use crate::host::editor::FileView;

pub(super) struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_explorer_panel_defaults)
            .register_persisted::<StackExplorerVisibility>()
            .add_systems(Update, (load_explorer_panel_defaults, emit_explorer_panel))
            .add_observer(toggle_explorer)
            .add_observer(reveal_in_explorer)
            .add_observer(open_find_in_files)
            .add_observer(on_explorer_panel_set_visible)
            .add_observer(on_explorer_panel_view_set)
            .add_observer(on_explorer_panel_width);
    }
}

fn spawn_explorer_panel_defaults(mut commands: Commands) {
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

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StackExplorerView {
    search: bool,
    search_focus_revision: u64,
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

fn load_explorer_panel_defaults(
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
        commands.entity(entity).remove::<ExplorerPanelSent>();
    }
}

fn emit_explorer_panel(
    views: Query<(Entity, Option<&ChildOf>), PanelUnsentReady>,
    visibility: Query<&StackExplorerVisibility>,
    revisions: Query<&StackExplorerRevision>,
    panel_views: Query<&StackExplorerView>,
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
        let view = panel_views.get(scope).copied().unwrap_or_default();
        commands.trigger(FileUiStateWrite::from_event(
            entity,
            &ExplorerPanelEvent {
                visible,
                width: panel.width,
                search: view.search,
                search_focus_revision: view.search_focus_revision,
                client_id: revision.client_id,
                request_id: revision.request_id,
            },
        ));
        commands.entity(entity).insert(ExplorerPanelSent);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ExplorerPanelMutation<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    visibility: Query<'w, 's, &'static mut StackExplorerVisibility>,
    revisions: Query<'w, 's, &'static mut StackExplorerRevision>,
    panel_views: Query<'w, 's, &'static mut StackExplorerView>,
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

    fn next_request_id(&self, scope: Entity) -> u64 {
        self.revisions
            .get(scope)
            .map(|revision| revision.request_id)
            .unwrap_or_default()
            .wrapping_add(1)
            .max(1)
    }

    fn set_panel(
        &mut self,
        scope: Entity,
        visibility: StackExplorerVisibility,
        revision: StackExplorerRevision,
    ) {
        if let Ok(mut state) = self.visibility.get_mut(scope) {
            *state = visibility;
        } else {
            self.commands.entity(scope).insert(visibility);
        }
        if let Ok(mut state) = self.revisions.get_mut(scope) {
            *state = revision;
        } else {
            self.commands.entity(scope).insert(revision);
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
            });
        }
    }

    fn mark_unsent(&mut self) {
        for (entity, _) in &self.editors {
            self.commands.entity(entity).remove::<ExplorerPanelSent>();
        }
    }

    fn mark_scope_sent(&mut self, source: Entity, scope: Entity) {
        for (view, parent) in &self.editors {
            let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
            if view_scope != scope {
                continue;
            }
            if view == source {
                self.commands.entity(view).insert(ExplorerPanelSent);
            } else {
                self.commands.entity(view).remove::<ExplorerPanelSent>();
            }
        }
    }
}

fn toggle_explorer(
    trigger: On<ExplorerToggleRequest>,
    panel: Single<&ExplorerPanelDefaults>,
    mut mutation: ExplorerPanelMutation,
) {
    let entity = trigger.event_target();
    let scope = mutation.scope(entity);
    let visible = mutation.visible(scope, panel.default_visible);
    let request_id = mutation.next_request_id(scope);
    mutation.set_panel(
        scope,
        StackExplorerVisibility { visible: !visible },
        StackExplorerRevision {
            client_id: 0,
            request_id,
        },
    );
    mutation.mark_unsent();
}

fn reveal_in_explorer(trigger: On<ExplorerRevealRequest>, mut mutation: ExplorerPanelMutation) {
    let entity = trigger.event_target();
    let scope = mutation.scope(entity);
    let request_id = mutation.next_request_id(scope);
    mutation.set_panel(
        scope,
        StackExplorerVisibility { visible: true },
        StackExplorerRevision {
            client_id: 0,
            request_id,
        },
    );
    mutation.set_view(scope, false);
    mutation.mark_unsent();
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
    let request_id = mutation.next_request_id(scope);
    mutation.set_panel(
        scope,
        StackExplorerVisibility { visible: true },
        StackExplorerRevision {
            client_id: 0,
            request_id,
        },
    );
    mutation.set_view(scope, true);
    mutation.mark_unsent();
}

fn on_explorer_panel_set_visible(
    trigger: On<UiInput<ExplorerPanelSetVisible>>,
    mut mutation: ExplorerPanelMutation,
) {
    let entity = trigger.event().webview;
    let scope = mutation.scope(entity);
    let next_visibility = StackExplorerVisibility {
        visible: trigger.event().payload.visible,
    };
    let next_revision = StackExplorerRevision {
        client_id: trigger.event().payload.client_id,
        request_id: trigger.event().payload.request_id,
    };
    mutation.set_panel(scope, next_visibility, next_revision);
    mutation.mark_scope_sent(entity, scope);
    if next_visibility.visible {
        mutation.commands.trigger(RevealCurrent {
            entity,
            reveal: ExplorerReveal::Followed,
        });
    }
}

fn on_explorer_panel_view_set(
    trigger: On<UiInput<ExplorerPanelViewSet>>,
    mut mutation: ExplorerPanelMutation,
) {
    let entity = trigger.event().webview;
    let scope = mutation.scope(entity);
    mutation.set_view(scope, trigger.event().payload.search);
    mutation.mark_unsent();
}

fn on_explorer_panel_width(
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
    mutation.mark_unsent();
}
