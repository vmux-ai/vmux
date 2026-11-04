use crate::pane::{Pane, PaneSplit};
use crate::stack::ComputeFocusSet;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::ops::Deref;

pub struct ActivePanePlugin;

impl Plugin for ActivePanePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ActivatePane>()
            .add_systems(Startup, spawn_local)
            .add_systems(
                Update,
                (apply_requests, prune_entities)
                    .chain()
                    .after(ComputeFocusSet),
            );
    }
}

#[derive(Component, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ActivePaneScope {
    Local,
    Agent(String),
}

#[derive(Component, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ActiveStack {
    pub tab: Option<Entity>,
    pub pane: Option<Entity>,
    pub stack: Option<Entity>,
}

impl ActiveStack {
    pub fn local_bundle(self) -> impl Bundle {
        (Name::new("Local active pane"), ActivePaneScope::Local, self)
    }
}

const EMPTY_ACTIVE_STACK: ActiveStack = ActiveStack {
    tab: None,
    pane: None,
    stack: None,
};

#[derive(SystemParam)]
pub struct FocusedStack<'w, 's> {
    scopes: Query<'w, 's, (&'static ActivePaneScope, Ref<'static, ActiveStack>)>,
}

impl FocusedStack<'_, '_> {
    pub fn as_ref(&self) -> Option<&ActiveStack> {
        self.scopes.iter().find_map(|(scope, active)| {
            (*scope == ActivePaneScope::Local).then_some(active.into_inner())
        })
    }

    pub fn as_deref(&self) -> Option<&ActiveStack> {
        self.as_ref()
    }

    pub fn is_changed(&self) -> bool {
        self.scopes
            .iter()
            .find_map(|(scope, active)| {
                (*scope == ActivePaneScope::Local).then_some(active.is_changed())
            })
            .unwrap_or(false)
    }
}

impl Deref for FocusedStack<'_, '_> {
    type Target = ActiveStack;

    fn deref(&self) -> &Self::Target {
        self.as_ref().unwrap_or(&EMPTY_ACTIVE_STACK)
    }
}

#[derive(SystemParam)]
pub struct ActivePaneQuery<'w, 's> {
    scopes: Query<'w, 's, (&'static ActivePaneScope, &'static ActiveStack)>,
}

impl ActivePaneQuery<'_, '_> {
    pub fn get(&self, scope: &ActivePaneScope) -> Option<ActiveStack> {
        self.scopes
            .iter()
            .find_map(|(candidate, active)| (candidate == scope).then_some(*active))
    }

    pub fn local(&self) -> ActiveStack {
        self.get(&ActivePaneScope::Local).unwrap_or_default()
    }

    pub fn agent_in_pane(&self, pane: Entity) -> Option<(&str, ActiveStack)> {
        self.scopes.iter().find_map(|(scope, active)| {
            let ActivePaneScope::Agent(id) = scope else {
                return None;
            };
            (active.pane == Some(pane)).then_some((id.as_str(), *active))
        })
    }
}

#[derive(Message, Clone)]
pub struct ActivatePane {
    pub scope: ActivePaneScope,
    pub active: ActiveStack,
}

fn spawn_local(mut commands: Commands) {
    commands.spawn(ActiveStack::default().local_bundle());
}

fn apply_requests(
    mut reader: MessageReader<ActivatePane>,
    mut scopes: Query<(&ActivePaneScope, &mut ActiveStack)>,
    mut commands: Commands,
) {
    let mut pending = Vec::<(ActivePaneScope, ActiveStack)>::new();
    for request in reader.read() {
        let mut applied = false;
        for (scope, mut active) in &mut scopes {
            if *scope != request.scope {
                continue;
            }
            *active = request.active;
            applied = true;
            break;
        }
        if applied {
            continue;
        }
        for (scope, active) in &mut pending {
            if *scope != request.scope {
                continue;
            }
            *active = request.active;
            applied = true;
            break;
        }
        if !applied {
            pending.push((request.scope.clone(), request.active));
        }
    }
    for (scope, active) in pending {
        let name = match &scope {
            ActivePaneScope::Local => "Local active pane".to_string(),
            ActivePaneScope::Agent(id) => format!("Agent active pane: {id}"),
        };
        commands.spawn((Name::new(name), scope, active));
    }
}

fn prune_entities(
    scopes: Query<(Entity, &ActivePaneScope, &ActiveStack)>,
    panes: Query<(), (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
) {
    for (entity, scope, active) in &scopes {
        if *scope != ActivePaneScope::Local && !active.pane.is_some_and(|pane| panes.contains(pane))
        {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_sets_per_profile_without_cross_contamination() {
        let mut app = App::new();
        app.add_plugins(ActivePanePlugin);

        let (user_pane, agent_pane) = {
            let world = app.world_mut();
            (world.spawn(Pane).id(), world.spawn(Pane).id())
        };
        app.world_mut().write_message(ActivatePane {
            scope: ActivePaneScope::Local,
            active: ActiveStack {
                tab: None,
                pane: Some(user_pane),
                stack: None,
            },
        });
        app.world_mut().write_message(ActivatePane {
            scope: ActivePaneScope::Agent("a1".to_string()),
            active: ActiveStack {
                tab: None,
                pane: Some(agent_pane),
                stack: None,
            },
        });
        app.update();

        let mut scopes = app.world_mut().query::<(&ActivePaneScope, &ActiveStack)>();
        let local = scopes
            .iter(app.world())
            .find_map(|(scope, active)| (*scope == ActivePaneScope::Local).then_some(*active))
            .unwrap();
        let agent = scopes
            .iter(app.world())
            .find_map(|(scope, active)| {
                (*scope == ActivePaneScope::Agent("a1".to_string())).then_some(*active)
            })
            .unwrap();
        assert_eq!(local.pane, Some(user_pane));
        assert_eq!(agent.pane, Some(agent_pane));
    }

    #[test]
    fn agent_activation_does_not_touch_local() {
        let mut app = App::new();
        app.add_plugins(ActivePanePlugin);
        app.update();

        let agent_pane = app.world_mut().spawn(Pane).id();
        app.world_mut().write_message(ActivatePane {
            scope: ActivePaneScope::Agent("a1".to_string()),
            active: ActiveStack {
                tab: None,
                pane: Some(agent_pane),
                stack: None,
            },
        });
        app.update();

        let mut scopes = app.world_mut().query::<(&ActivePaneScope, &ActiveStack)>();
        let local = scopes
            .iter(app.world())
            .find_map(|(scope, active)| (*scope == ActivePaneScope::Local).then_some(*active))
            .unwrap();
        let agent = scopes
            .iter(app.world())
            .find_map(|(scope, active)| {
                (*scope == ActivePaneScope::Agent("a1".to_string())).then_some(*active)
            })
            .unwrap();
        assert_eq!(local.pane, None);
        assert_eq!(agent.pane, Some(agent_pane));
    }
}
