use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::profile::{ActiveProfile, ProfileDirectories};
use vmux_ecs::{Active, ProfileId, Ready};

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, configure)
            .add_systems(Update, bind.in_set(vmux_account::Set::Publish));
    }
}

fn configure(
    profiles: Query<(&ActiveProfile, &ProfileDirectories)>,
    roots: Query<(), With<vmux_account::Root>>,
    mut commands: Commands,
) {
    if !roots.is_empty() {
        return;
    }
    let Ok((profile, directories)) = profiles.single() else {
        return;
    };
    commands.spawn((
        vmux_account::Root::at(directories.0.shared_data(), profile.0.id()),
        Active,
    ));
}

fn bind(
    roots: ReadyRoots,
    accounts: Query<&ChildOf, With<vmux_account::Account>>,
    profiles: Query<(Entity, &ProfileId, &Name, &vmux_account::Owner), With<vmux_account::Profile>>,
    mut active: Query<(&ActiveProfile, &mut Name), Without<vmux_account::Profile>>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else { return };
    for (current, mut label) in &mut active {
        for (entity, id, name, owner) in &profiles {
            if id.0 != current.0.id() {
                continue;
            }
            let Ok(parent) = accounts.get(owner.0) else {
                continue;
            };
            if parent.parent() != root {
                continue;
            }
            if *label != *name {
                *label = name.clone();
            }
            commands.entity(entity).insert(Active);
        }
    }
}

type ReadyRoots<'w, 's> =
    Query<'w, 's, Entity, (With<vmux_account::Root>, With<Active>, With<Ready>)>;
