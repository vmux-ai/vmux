use bevy::prelude::*;
use vmux_ecs::host::persistence::PersistenceAppExt;

pub(crate) use agent::{AgentReadKnowledge, AgentSearchKnowledge, AgentWriteKnowledge};

mod agent;
mod index;

#[vmux_native::page]
pub struct KnowledgePlugin;

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Self::MANIFEST.plugin())
            .add_plugins((
                crate::KnowledgeToolPlugin,
                agent::KnowledgeAgentPlugin,
                index::KnowledgeIndexPlugin,
            ))
            .register_persisted::<ExpandedKnowledgeDirs>();
    }
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::knowledge"]
#[require(moonshine_save::prelude::Save)]
pub struct ExpandedKnowledgeDirs(Vec<String>);
