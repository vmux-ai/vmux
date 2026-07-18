//! Local-first Markdown knowledge base: vault storage, library page, preview, and editor handoff.

pub mod event;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::KnowledgePlugin;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "notes",
    title: "Knowledge",
    keywords: &["notes", "knowledge", "markdown", "wiki"],
    icon: Some(vmux_core::BuiltinIcon::BookOpen),
    command_bar: true,
};
