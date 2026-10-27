use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    window::{WindowFocused, WindowResized},
};
use bevy_cef::prelude::{HostWindow, UiEventPlugin, UiInput};
use vmux_ecs::overlay::WindowOverlay;
use vmux_ecs::page::HostsPage;
use vmux_ecs::{KeyboardOwner, PageMetadata, UiStateWrite};
use vmux_flex::prelude::*;

use crate::Browser;
use crate::event::{
    ProjectDropdownAddRequest, ProjectDropdownCloseRequest, ProjectDropdownOpenRequest,
    ProjectDropdownOpenSelectedRequest, ProjectDropdownPickRequest, ProjectDropdownSizeRequest,
    ProjectDropdownState, SideSheetProjectPageOpenRequest,
};
use crate::overlay::LayoutOverlayDismissRevision;
use crate::state::UiLayout;

use super::plugin::ProjectDropdownPage;

pub(super) struct ProjectDropdownPlugin;

impl Plugin for ProjectDropdownPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ProjectDropdownOpenRequest,
            ProjectDropdownSizeRequest,
            ProjectDropdownPickRequest,
            ProjectDropdownAddRequest,
            ProjectDropdownOpenSelectedRequest,
            ProjectDropdownCloseRequest,
        )>::default())
            .add_observer(open)
            .add_observer(size)
            .add_observer(pick)
            .add_observer(add)
            .add_observer(open_selected)
            .add_observer(close)
            .add_systems(Update, (close_on_resize, close_on_focus_loss));
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ProjectDropdowns<'w, 's> {
    surfaces: Query<'w, 's, (Entity, &'static ProjectDropdownState, &'static ChildOf)>,
    revisions: Query<'w, 's, &'static mut LayoutOverlayDismissRevision>,
    commands: Commands<'w, 's>,
}

impl ProjectDropdowns<'_, '_> {
    fn find(&self, caller: Entity, id: &str) -> Option<(Entity, Entity, ProjectDropdownState)> {
        self.surfaces.iter().find_map(|(entity, state, owner)| {
            (state.id == id && (caller == entity || caller == owner.get())).then_some((
                entity,
                owner.get(),
                state.clone(),
            ))
        })
    }

    fn close(&mut self, entity: Entity, owner: Entity, publish: bool) {
        self.commands.entity(entity).try_despawn();
        if !publish {
            return;
        }
        let Ok(mut revision) = self.revisions.get_mut(owner) else {
            return;
        };
        let effect = revision.next_effect();
        self.commands
            .trigger(UiStateWrite::<UiLayout>::from_event(owner, &effect));
    }

    fn close_matching(&mut self, caller: Entity, id: &str, publish: bool) {
        let Some((entity, owner, _)) = self.find(caller, id) else {
            return;
        };
        self.close(entity, owner, publish);
    }

    fn close_all(&mut self) {
        let open = self
            .surfaces
            .iter()
            .map(|(entity, _, owner)| (entity, owner.get()))
            .collect::<Vec<_>>();
        for (entity, owner) in open {
            self.close(entity, owner, true);
        }
    }
}

fn open(
    trigger: On<UiInput<ProjectDropdownOpenRequest>>,
    existing: Query<(Entity, &ChildOf), With<ProjectDropdownState>>,
    mut commands: Commands,
) {
    let owner = trigger.event().webview;
    let state = &trigger.event().payload.state;
    if state.id.is_empty() || !state.anchor.is_valid() {
        return;
    }
    for (entity, parent) in &existing {
        if parent.get() == owner {
            commands.entity(entity).try_despawn();
        }
    }
    commands.spawn((
        Name::new("Project dropdown"),
        Browser,
        HostsPage,
        WindowOverlay,
        KeyboardOwner,
        state.clone(),
        PageMetadata {
            title: "Project".to_string(),
            url: ProjectDropdownPage::URL.to_string(),
            ..default()
        },
        Node {
            display: Display::Flex,
            position_type: PositionType::Absolute,
            ..default()
        },
        Transform::default(),
        Visibility::Visible,
        ChildOf(owner),
    ));
}

fn size(
    trigger: On<UiInput<ProjectDropdownSizeRequest>>,
    states: Query<(&ProjectDropdownState, &ChildOf)>,
    hosts: Query<&HostWindow>,
    windows: Query<&Window>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, owner)) = states.get(webview) else {
        return;
    };
    let request = &trigger.event().payload;
    if state.id != request.id {
        return;
    }
    let Ok(host) = hosts.get(owner.get()) else {
        return;
    };
    let Ok(window) = windows.get(host.0) else {
        return;
    };
    let Some(frame) = vmux_page::ModalFrame::dropdown(
        state.anchor,
        request.width,
        request.height,
        window.width(),
        window.height(),
    ) else {
        return;
    };
    commands.entity(webview).insert(frame);
}

fn pick(trigger: On<UiInput<ProjectDropdownPickRequest>>, mut dropdowns: ProjectDropdowns) {
    let request = &trigger.event().payload;
    let Some((entity, owner, state)) = dropdowns.find(trigger.event().webview, request.id.as_str())
    else {
        return;
    };
    dropdowns.commands.trigger(UiInput {
        webview: owner,
        payload: vmux_ecs::event::space::ProjectActivateRequest {
            path: request.path.clone(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: Some(state.pane_id),
        },
    });
    dropdowns.close(entity, owner, true);
}

fn add(trigger: On<UiInput<ProjectDropdownAddRequest>>, mut dropdowns: ProjectDropdowns) {
    let request = &trigger.event().payload;
    let Some((entity, owner, state)) = dropdowns.find(trigger.event().webview, request.id.as_str())
    else {
        return;
    };
    dropdowns.commands.trigger(UiInput {
        webview: owner,
        payload: vmux_ecs::event::ProjectPickerRequest {
            current_dir: state.current_dir,
            pane_id: Some(state.pane_id),
        },
    });
    dropdowns.close(entity, owner, true);
}

fn open_selected(
    trigger: On<UiInput<ProjectDropdownOpenSelectedRequest>>,
    mut dropdowns: ProjectDropdowns,
) {
    let request = &trigger.event().payload;
    let Some((entity, owner, state)) = dropdowns.find(trigger.event().webview, request.id.as_str())
    else {
        return;
    };
    dropdowns.commands.trigger(UiInput {
        webview: owner,
        payload: SideSheetProjectPageOpenRequest {
            pane_id: state.pane_id,
            path: state.selected_path,
        },
    });
    dropdowns.close(entity, owner, true);
}

fn close(trigger: On<UiInput<ProjectDropdownCloseRequest>>, mut dropdowns: ProjectDropdowns) {
    let request = &trigger.event().payload;
    let publish = dropdowns
        .find(trigger.event().webview, request.id.as_str())
        .is_some_and(|(entity, _, _)| entity == trigger.event().webview);
    dropdowns.close_matching(trigger.event().webview, request.id.as_str(), publish);
}

fn close_on_resize(mut resized: MessageReader<WindowResized>, mut dropdowns: ProjectDropdowns) {
    if resized.read().next().is_none() {
        return;
    }
    dropdowns.close_all();
}

fn close_on_focus_loss(mut focused: MessageReader<WindowFocused>, mut dropdowns: ProjectDropdowns) {
    if focused.read().all(|event| event.focused) {
        return;
    }
    dropdowns.close_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ProjectDropdownState {
        ProjectDropdownState {
            id: "project".to_string(),
            pane_id: 7,
            current_dir: "/repo".to_string(),
            selected_path: "/repo".to_string(),
            projects: Vec::new(),
            anchor: vmux_api::dropdown::DropdownAnchor {
                left: 20.0,
                top: 30.0,
                width: 180.0,
                height: 32.0,
            },
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<WindowResized>()
            .add_message::<WindowFocused>()
            .add_plugins(ProjectDropdownPlugin);
        app
    }

    #[test]
    fn open_and_close_own_the_modal_under_the_source_page() {
        let mut app = app();
        let owner = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiInput {
            webview: owner,
            payload: ProjectDropdownOpenRequest { state: state() },
        });
        app.world_mut().flush();

        let mut query = app
            .world_mut()
            .query::<(Entity, &ProjectDropdownState, &ChildOf)>();
        let open = query
            .iter(app.world())
            .map(|(entity, state, parent)| (entity, state.id.clone(), parent.get()))
            .collect::<Vec<_>>();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].1, "project");
        assert_eq!(open[0].2, owner);

        app.world_mut().trigger(UiInput {
            webview: owner,
            payload: ProjectDropdownCloseRequest {
                id: "project".to_string(),
            },
        });
        app.world_mut().flush();

        assert!(query.iter(app.world()).next().is_none());
    }

    #[test]
    fn invalid_anchor_does_not_spawn_a_modal() {
        let mut app = app();
        let owner = app.world_mut().spawn_empty().id();
        let mut state = state();
        state.anchor.width = 0.0;

        app.world_mut().trigger(UiInput {
            webview: owner,
            payload: ProjectDropdownOpenRequest { state },
        });
        app.world_mut().flush();

        assert!(
            app.world_mut()
                .query::<&ProjectDropdownState>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
