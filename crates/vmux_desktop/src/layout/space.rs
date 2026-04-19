use crate::{
    command::{AppCommand, ReadAppCommands, SpaceCommand},
};
use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_history::LastActivatedAt;

pub(crate) struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_space_commands.in_set(ReadAppCommands))
            .add_systems(PostUpdate, sync_space_visibility);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Space {
    pub name: String,
}

pub(crate) fn space_bundle() -> impl Bundle {
    (
        Space::default(),
        Transform::default(),
        GlobalTransform::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
    )
}

fn handle_space_commands(
    mut reader: MessageReader<AppCommand>,
) {
    for cmd in reader.read() {
        let AppCommand::Space(space_cmd) = *cmd else {
            continue;
        };
        match space_cmd {
            SpaceCommand::New => {}
            SpaceCommand::Close => {}
            SpaceCommand::Next => {}
            SpaceCommand::Previous => {}
            SpaceCommand::Rename => {}
        }
    }
}

fn sync_space_visibility(
    mut spaces: Query<(Entity, &LastActivatedAt, &mut Node), With<Space>>,
) {
    let active = spaces.iter().max_by_key(|(_, ts, _)| ts.0).map(|(e, _, _)| e);
    for (entity, _, mut node) in &mut spaces {
        let target = if Some(entity) == active { Display::Flex } else { Display::None };
        if node.display != target {
            node.display = target;
        }
    }
}
