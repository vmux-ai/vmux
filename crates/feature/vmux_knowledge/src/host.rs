use bevy::prelude::*;
use vmux_core::host::persistence::PersistenceAppExt;

mod agent;
mod index;

pub(crate) use agent::{AgentReadKnowledge, AgentSearchKnowledge, AgentWriteKnowledge};

#[vmux_native::page]
pub struct KnowledgePlugin;

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::manifest::FeatureManifestPlugin::<
            crate::Feature,
        >::new(crate::FEATURE_MANIFEST))
            .add_plugins(Self::MANIFEST.plugin())
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
