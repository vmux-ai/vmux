use crate::pane::{Pane, PaneSplit};
use crate::stack::{ComputeFocusSet, FocusedStack};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

pub struct ActivePanePlugin;

impl Plugin for ActivePanePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ActivatePane>()
            .add_systems(Startup, spawn_local_active_pane)
            .add_systems(
                Update,
                (
                    mirror_local_active_pane,
                    apply_active_pane_requests,
                    prune_active_pane_entities,
                )
                    .chain()
                    .after(ComputeFocusSet),
            );
    }
}

#[derive(Component, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ProfileId {
    Local,
    Agent(String),
}

#[derive(Component, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ActiveStack {
    pub tab: Option<Entity>,
    pub pane: Option<Entity>,
    pub stack: Option<Entity>,
    pub kind: Option<vmux_core::agent::AgentKind>,
}

#[derive(SystemParam)]
pub struct ActivePaneQuery<'w, 's> {
    profiles: Query<'w, 's, (&'static ProfileId, &'static ActiveStack)>,
}

impl ActivePaneQuery<'_, '_> {
    pub fn get(&self, profile: &ProfileId) -> Option<ActiveStack> {
        self.profiles
            .iter()
            .find_map(|(candidate, active)| (candidate == profile).then_some(*active))
    }

    pub fn local(&self) -> ActiveStack {
        self.get(&ProfileId::Local).unwrap_or_default()
    }

    pub fn agent_in_pane(&self, pane: Entity) -> Option<(&str, ActiveStack)> {
        self.profiles.iter().find_map(|(profile, active)| {
            let ProfileId::Agent(id) = profile else {
                return None;
            };
            (active.pane == Some(pane)).then_some((id.as_str(), *active))
        })
    }
}

#[derive(Message, Clone)]
pub struct ActivatePane {
    pub profile: ProfileId,
    pub active: ActiveStack,
}

fn spawn_local_active_pane(mut commands: Commands) {
    commands.spawn((
        Name::new("Local active pane"),
        ProfileId::Local,
        ActiveStack::default(),
    ));
}

fn mirror_local_active_pane(
    focus: Res<FocusedStack>,
    mut profiles: Query<(&ProfileId, &mut ActiveStack)>,
    mut commands: Commands,
) {
    let next = ActiveStack {
        tab: focus.tab,
        pane: focus.pane,
        stack: focus.stack,
        kind: None,
    };
    for (profile, mut active) in &mut profiles {
        if *profile == ProfileId::Local {
            *active = next;
            return;
        }
    }
    commands.spawn((Name::new("Local active pane"), ProfileId::Local, next));
}

fn apply_active_pane_requests(
    mut reader: MessageReader<ActivatePane>,
    mut profiles: Query<(&ProfileId, &mut ActiveStack)>,
    mut commands: Commands,
) {
    let mut pending = Vec::<(ProfileId, ActiveStack)>::new();
    for request in reader.read() {
        let mut applied = false;
        for (profile, mut active) in &mut profiles {
            if *profile != request.profile {
                continue;
            }
            let kind = request.active.kind.or(active.kind);
            *active = ActiveStack {
                kind,
                ..request.active
            };
            applied = true;
            break;
        }
        if applied {
            continue;
        }
        for (profile, active) in &mut pending {
            if *profile != request.profile {
                continue;
            }
            let kind = request.active.kind.or(active.kind);
            *active = ActiveStack {
                kind,
                ..request.active
            };
            applied = true;
            break;
        }
        if !applied {
            pending.push((request.profile.clone(), request.active));
        }
    }
    for (profile, active) in pending {
        let name = match &profile {
            ProfileId::Local => "Local active pane".to_string(),
            ProfileId::Agent(id) => format!("Agent active pane: {id}"),
        };
        commands.spawn((Name::new(name), profile, active));
    }
}

fn prune_active_pane_entities(
    profiles: Query<(Entity, &ProfileId, &ActiveStack)>,
    panes: Query<(), (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
) {
    for (entity, profile, active) in &profiles {
        if *profile != ProfileId::Local && !active.pane.is_some_and(|pane| panes.contains(pane)) {
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
        app.init_resource::<FocusedStack>()
            .add_plugins(ActivePanePlugin);

        let (user_pane, agent_pane) = {
            let world = app.world_mut();
            (world.spawn(Pane).id(), world.spawn(Pane).id())
        };
        app.world_mut().write_message(ActivatePane {
            profile: ProfileId::Local,
            active: ActiveStack {
                tab: None,
                pane: Some(user_pane),
                stack: None,
                kind: None,
            },
        });
        app.world_mut().write_message(ActivatePane {
            profile: ProfileId::Agent("a1".to_string()),
            active: ActiveStack {
                tab: None,
                pane: Some(agent_pane),
                stack: None,
                kind: None,
            },
        });
        app.update();

        let mut profiles = app.world_mut().query::<(&ProfileId, &ActiveStack)>();
        let local = profiles
            .iter(app.world())
            .find_map(|(profile, active)| (*profile == ProfileId::Local).then_some(*active))
            .unwrap();
        let agent = profiles
            .iter(app.world())
            .find_map(|(profile, active)| {
                (*profile == ProfileId::Agent("a1".to_string())).then_some(*active)
            })
            .unwrap();
        assert_eq!(local.pane, Some(user_pane));
        assert_eq!(agent.pane, Some(agent_pane));
    }

    #[test]
    fn agent_activation_does_not_touch_local() {
        let mut app = App::new();
        app.init_resource::<FocusedStack>()
            .add_plugins(ActivePanePlugin);
        app.update();

        let agent_pane = app.world_mut().spawn(Pane).id();
        app.world_mut().write_message(ActivatePane {
            profile: ProfileId::Agent("a1".to_string()),
            active: ActiveStack {
                tab: None,
                pane: Some(agent_pane),
                stack: None,
                kind: None,
            },
        });
        app.update();

        let mut profiles = app.world_mut().query::<(&ProfileId, &ActiveStack)>();
        let local = profiles
            .iter(app.world())
            .find_map(|(profile, active)| (*profile == ProfileId::Local).then_some(*active))
            .unwrap();
        let agent = profiles
            .iter(app.world())
            .find_map(|(profile, active)| {
                (*profile == ProfileId::Agent("a1".to_string())).then_some(*active)
            })
            .unwrap();
        assert_eq!(local.pane, None);
        assert_eq!(agent.pane, Some(agent_pane));
    }

    #[test]
    fn activation_without_kind_preserves_profile_kind() {
        let mut app = App::new();
        app.init_resource::<FocusedStack>()
            .add_plugins(ActivePanePlugin);
        app.update();

        let profile = ProfileId::Agent("a1".to_string());
        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().spawn((
            profile.clone(),
            ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: None,
                kind: Some(vmux_core::agent::AgentKind::Codex),
            },
        ));
        app.world_mut().write_message(ActivatePane {
            profile: profile.clone(),
            active: ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: None,
                kind: None,
            },
        });

        app.update();

        let mut profiles = app.world_mut().query::<(&ProfileId, &ActiveStack)>();
        let active = profiles
            .iter(app.world())
            .find_map(|(candidate, active)| (candidate == &profile).then_some(*active))
            .unwrap();
        assert_eq!(active.kind, Some(vmux_core::agent::AgentKind::Codex));
    }
}
