use bevy::prelude::*;

mod agent;
mod index;

pub struct KnowledgePlugin;

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::KnowledgeToolPlugin,
            agent::KnowledgeAgentPlugin,
            index::KnowledgeIndexPlugin,
        ))
        .add_systems(Startup, register_knowledge_page)
        .register_type::<ExpandedKnowledgeDirs>();
    }
}

fn register_knowledge_page(mut commands: Commands) {
    commands.spawn(PAGE_MANIFEST);
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    url: vmux_core::knowledge::KNOWLEDGE_PAGE_URL,
    asset_host: "knowledge",
    owns_subtree: true,
    title: "Knowledge",
    title_message_id: Some("layout-knowledge"),
    replaces_command: None,
    keywords: &["knowledge", "notes", "markdown"],
    icon: Some(vmux_core::BuiltinIcon::Brain),
    command_bar: true,
};

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::knowledge"]
#[require(moonshine_save::prelude::Save)]
pub struct ExpandedKnowledgeDirs(Vec<String>);
