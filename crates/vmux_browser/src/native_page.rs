use bevy::prelude::*;
use vmux_native::NativePage;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

pub struct NativePagePlugin {
    page: &'static NativePage,
    placement: Placement,
    instance: Option<ReadInstance>,
}

type ReadInstance = fn(&World, Entity) -> vmux_native::Instance;

impl Plugin for NativePagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<NativePagesPlugin>() {
            app.add_plugins(NativePagesPlugin);
        }
        app.world_mut().resource_mut::<NativePages>().0.push((
            self.page,
            self.placement,
            self.instance,
        ));
    }

    fn is_unique(&self) -> bool {
        false
    }
}

impl NativePagePlugin {
    pub fn in_pane(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Pane,
            instance: None,
        }
    }

    pub fn as_layout(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Layout,
            instance: None,
        }
    }

    pub fn as_modal(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Modal,
            instance: None,
        }
    }

    pub fn takes<C: Component + Clone>(mut self) -> Self {
        self.instance = Some(Self::read::<C>);
        self
    }

    fn read<C: Component + Clone>(world: &World, entity: Entity) -> vmux_native::Instance {
        let Some(value) = world.get::<C>(entity).cloned() else {
            return vmux_native::Instance::default();
        };
        vmux_native::Instance::of(move |scope| scope.provide(value))
    }
}

#[cfg(target_os = "macos")]
pub static LAYOUT_PAGE: NativePage = NativePage {
    url: vmux_layout::event::LAYOUT_PAGE_URL,
    document_url: None,
    title: "vmux",
    reports_title: true,
    favicon: true,
    component: vmux_layout::page::Page,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: transparent; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
    html_attributes: r#"lang="en" class="h-full" style="color-scheme: light dark""#,
    body_class: "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 \
                 text-foreground antialiased",
    transparent: true,
    owns_subtree: false,
};

#[cfg(target_os = "macos")]
pub static START_PAGE: NativePage = NativePage {
    url: vmux_start::START_PAGE_URL,
    document_url: None,
    title: "Start",
    reports_title: true,
    favicon: true,
    component: vmux_start::page::StartPage,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
    html_attributes: r#"lang="en" class="h-full" style="color-scheme: light dark""#,
    body_class: "m-0 flex h-full min-h-0 flex-col overflow-hidden p-0 text-foreground antialiased",
    transparent: false,
    owns_subtree: false,
};

#[cfg(target_os = "macos")]
pub static HISTORY_PAGE: NativePage =
    NativePage::pane(vmux_history::PAGE_URL, vmux_history::page::Page).titled("History");

#[cfg(target_os = "macos")]
pub static SHORTCUTS_PAGE: NativePage =
    NativePage::pane(vmux_shortcut::PAGE_URL, vmux_shortcut::page::Page)
        .titled("Keyboard Shortcuts");

#[cfg(target_os = "macos")]
pub static TEAM_PAGE: NativePage =
    NativePage::pane(vmux_core::event::team::TEAM_PAGE_URL, vmux_team::page::Page).titled("Team");

#[cfg(target_os = "macos")]
pub static AGENTS_PAGE: NativePage =
    NativePage::pane("vmux://agents/", vmux_agent::page::Page).titled("Agents");

#[cfg(target_os = "macos")]
pub static CHAT_PAGE: NativePage = NativePage::pane("vmux://sessions/", vmux_chat::page::Page)
    .titled("Sessions")
    .preserving_host_title()
    .without_favicon()
    .owning_subtree();

#[cfg(target_os = "macos")]
pub static LSP_PAGE: NativePage =
    NativePage::pane("vmux://lsp/", vmux_editor::lsp_page::Page).titled("Language Servers");

#[cfg(target_os = "macos")]
pub static FILES_PAGE: NativePage = NativePage::pane("file://", vmux_editor::page::Page)
    .titled("Files")
    .owning_subtree();

#[cfg(target_os = "macos")]
pub static PROJECTS_PAGE: NativePage =
    NativePage::pane(vmux_wire::space::PROJECTS_PAGE_URL, vmux_editor::page::Page)
        .titled("Projects")
        .owning_subtree();

#[cfg(target_os = "macos")]
pub static KNOWLEDGE_PAGE: NativePage = NativePage::pane(
    vmux_core::knowledge::KNOWLEDGE_PAGE_URL,
    vmux_editor::page::Page,
)
.titled("Knowledge")
.owning_subtree();

#[cfg(target_os = "macos")]
pub static TERMINAL_PAGE: NativePage = NativePage::pane(
    vmux_terminal::event::TERMINAL_PAGE_URL,
    vmux_terminal::page::Page,
)
.titled("Terminal");

#[cfg(target_os = "macos")]
pub static SETTINGS_PAGE: NativePage = NativePage::pane(
    vmux_setting::event::SETTINGS_PAGE_URL,
    vmux_setting::page::Page,
)
.titled("Settings");

#[cfg(target_os = "macos")]
pub static SERVICES_PAGE: NativePage = NativePage::pane(
    vmux_layout::event::SERVICES_PAGE_URL,
    vmux_service::page::Page,
)
.titled("Services");

#[cfg(target_os = "macos")]
pub static SIMULATOR_PAGE: NativePage =
    NativePage::pane(vmux_simulator::url::PAGE_URL, vmux_simulator::page::Page).owning_subtree();

#[cfg(target_os = "macos")]
pub static SPACES_PAGE: NativePage =
    NativePage::pane(vmux_wire::space::SPACES_PAGE_URL, vmux_space::page::Page).titled("Spaces");

#[cfg(target_os = "macos")]
pub static TOOLS_PAGE: NativePage =
    NativePage::pane("vmux://tools/", vmux_layout::tools_page::Page).titled("Tools");

#[cfg(target_os = "macos")]
pub static VAULT_PAGE: NativePage =
    NativePage::pane("vmux://vault/", vmux_layout::vault_page::Page)
        .titled("Vault")
        .owning_subtree();

#[cfg(target_os = "macos")]
pub static EXTENSIONS_PAGE: NativePage = NativePage::pane(
    vmux_core::event::EXTENSIONS_PAGE_URL,
    vmux_layout::extensions_page::Page,
)
.titled("Extensions");

#[cfg(target_os = "macos")]
pub static ERROR_PAGE: NativePage = NativePage::pane(
    vmux_wire::error::ERROR_PAGE_URL,
    vmux_layout::error_page::Page,
)
.titled("Error");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    Layout,
    Pane,
    Modal,
}

#[derive(Resource, Default)]
struct NativePages(Vec<(&'static NativePage, Placement, Option<ReadInstance>)>);

pub struct NativePagesPlugin;

impl Plugin for NativePagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativePages>();
        #[cfg(target_os = "macos")]
        app.add_plugins(macos::NativePagesMacosPlugin);
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(other::NativePagesOtherPlugin);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn every_page_loads_its_document_over_the_vmux_scheme() {
        for page in [
            &LAYOUT_PAGE,
            &START_PAGE,
            &CHAT_PAGE,
            &LSP_PAGE,
            &FILES_PAGE,
            &PROJECTS_PAGE,
            &KNOWLEDGE_PAGE,
            &TERMINAL_PAGE,
            &SIMULATOR_PAGE,
            &VAULT_PAGE,
        ] {
            assert!(
                page.document_url().starts_with("vmux://"),
                "{} loads from {}, which no protocol handler serves",
                page.url,
                page.document_url()
            );
        }
    }

    #[test]
    fn chat_document_uses_the_host_that_serves_its_assets() {
        let host = CHAT_PAGE
            .document_url()
            .strip_prefix("vmux://")
            .and_then(|url| url.split('/').next())
            .unwrap();

        assert_eq!(host, vmux_start::PAGE_MANIFEST.host);
    }

    #[test]
    fn the_editor_still_answers_for_file_urls() {
        assert_eq!(FILES_PAGE.url, "file://");
        assert!(FILES_PAGE.answers_for("file:///Users/me/a.rs"));
        assert_eq!(FILES_PAGE.document_url(), "vmux://start/");
    }
    #[test]
    fn the_vault_claims_the_provider_deep_links_and_nothing_next_door() {
        assert!(VAULT_PAGE.answers_for("vmux://vault/"));
        assert!(VAULT_PAGE.answers_for("vmux://vault/?provider=github"));
        assert!(VAULT_PAGE.answers_for("vmux://vault/?provider=cloud_folder"));
        assert!(!VAULT_PAGE.answers_for("vmux://vaults/"));
        assert!(!VAULT_PAGE.answers_for("vmux://tools/"));
    }
}
