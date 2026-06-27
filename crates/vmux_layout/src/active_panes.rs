use crate::pane::{Pane, PaneSplit};
use crate::stack::{ComputeFocusSet, FocusedStack};
use bevy::prelude::*;
use std::collections::HashMap;

/// Identity of a participant whose focus the layout tracks. The local human
/// drives OS keyboard focus; agents each get their own active pane and focus
/// ring. Remote participants are a future variant feeding the same flow.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ProfileId {
    Local,
    Agent(String),
}

/// The tab/pane/stack a profile is currently focused on. Same shape as
/// `FocusedStack`, but per profile.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ActiveStack {
    pub tab: Option<Entity>,
    pub pane: Option<Entity>,
    pub stack: Option<Entity>,
}

/// Per-profile active pane. `ProfileId::Local` mirrors `FocusedStack` every
/// frame (so existing local-only consumers keep working); agent/remote entries
/// are set via `ActivatePane`.
#[derive(Resource, Default)]
pub struct ActivePanes(pub HashMap<ProfileId, ActiveStack>);

impl ActivePanes {
    pub fn get(&self, profile: &ProfileId) -> Option<ActiveStack> {
        self.0.get(profile).copied()
    }

    /// The local human's active pane — the single source of truth for "the
    /// focused pane" (replaces the former global `FocusedStack`).
    pub fn local(&self) -> ActiveStack {
        self.0.get(&ProfileId::Local).copied().unwrap_or_default()
    }
}

/// A profile claims an active pane. Emitted by that profile's own actions: the
/// local human's via `FocusedStack` mirroring, an agent's on navigate/click,
/// and (future) a remote participant's over the network — the same message.
#[derive(Message, Clone)]
pub struct ActivatePane {
    pub profile: ProfileId,
    pub active: ActiveStack,
}

pub struct ActivePanesPlugin;

impl Plugin for ActivePanesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePanes>()
            .add_message::<ActivatePane>()
            .add_systems(
                Update,
                (
                    mirror_local_active_pane,
                    apply_active_panes,
                    prune_active_panes,
                )
                    .chain()
                    .after(ComputeFocusSet),
            );
    }
}

fn mirror_local_active_pane(focus: Res<FocusedStack>, mut active: ResMut<ActivePanes>) {
    active.0.insert(
        ProfileId::Local,
        ActiveStack {
            tab: focus.tab,
            pane: focus.pane,
            stack: focus.stack,
        },
    );
}

fn apply_active_panes(mut reader: MessageReader<ActivatePane>, mut active: ResMut<ActivePanes>) {
    for ev in reader.read() {
        active.0.insert(ev.profile.clone(), ev.active);
    }
}

fn prune_active_panes(
    mut active: ResMut<ActivePanes>,
    panes: Query<(), (With<Pane>, Without<PaneSplit>)>,
) {
    active.0.retain(|profile, st| {
        *profile == ProfileId::Local || st.pane.is_some_and(|p| panes.contains(p))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_sets_per_profile_without_cross_contamination() {
        let mut app = App::new();
        app.init_resource::<ActivePanes>()
            .add_message::<ActivatePane>()
            .add_systems(Update, apply_active_panes);

        let (user_pane, agent_pane) = {
            let world = app.world_mut();
            (world.spawn_empty().id(), world.spawn_empty().id())
        };
        app.world_mut().write_message(ActivatePane {
            profile: ProfileId::Local,
            active: ActiveStack {
                tab: None,
                pane: Some(user_pane),
                stack: None,
            },
        });
        app.world_mut().write_message(ActivatePane {
            profile: ProfileId::Agent("a1".to_string()),
            active: ActiveStack {
                tab: None,
                pane: Some(agent_pane),
                stack: None,
            },
        });
        app.update();

        let active = app.world().resource::<ActivePanes>();
        assert_eq!(active.get(&ProfileId::Local).unwrap().pane, Some(user_pane));
        assert_eq!(
            active
                .get(&ProfileId::Agent("a1".to_string()))
                .unwrap()
                .pane,
            Some(agent_pane)
        );
    }

    #[test]
    fn agent_activation_does_not_touch_local() {
        let mut app = App::new();
        app.init_resource::<ActivePanes>()
            .add_message::<ActivatePane>()
            .add_systems(Update, apply_active_panes);

        let agent_pane = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<ActivePanes>().0.insert(
            ProfileId::Local,
            ActiveStack {
                tab: None,
                pane: None,
                stack: None,
            },
        );
        app.world_mut().write_message(ActivatePane {
            profile: ProfileId::Agent("a1".to_string()),
            active: ActiveStack {
                tab: None,
                pane: Some(agent_pane),
                stack: None,
            },
        });
        app.update();

        let active = app.world().resource::<ActivePanes>();
        assert_eq!(active.get(&ProfileId::Local).unwrap().pane, None);
        assert_eq!(
            active
                .get(&ProfileId::Agent("a1".to_string()))
                .unwrap()
                .pane,
            Some(agent_pane)
        );
    }
}
