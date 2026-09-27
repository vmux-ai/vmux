use bevy::prelude::*;

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
            .register_type::<ExpandedKnowledgeDirs>();
    }
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::knowledge"]
#[require(moonshine_save::prelude::Save)]
pub struct ExpandedKnowledgeDirs(Vec<String>);
