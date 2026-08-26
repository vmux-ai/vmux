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
    component: vmux_layout::page::Page,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
<title>vmux</title>
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
    component: vmux_start::page::StartPage,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
<title>Start</title>
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
    NativePage::pane(vmux_history::PAGE_URL, vmux_history::page::Page);

#[cfg(target_os = "macos")]
pub static TEAM_PAGE: NativePage =
    NativePage::pane(vmux_core::event::team::TEAM_PAGE_URL, vmux_team::page::Page);

#[cfg(target_os = "macos")]
pub static AGENTS_PAGE: NativePage = NativePage::pane("vmux://agents/", vmux_agent::page::Page);

#[cfg(target_os = "macos")]
pub static CHAT_PAGE: NativePage =
    NativePage::pane("vmux://agent/", vmux_chat::page::Page).owning_subtree();

#[cfg(target_os = "macos")]
pub static LSP_PAGE: NativePage = NativePage::pane("vmux://lsp/", vmux_editor::lsp_page::Page);

#[cfg(target_os = "macos")]
pub static FILES_PAGE: NativePage = NativePage::pane("file://", vmux_editor::page::Page)
    .owning_subtree()
    .served_from("vmux://files/");

#[cfg(target_os = "macos")]
pub static TERMINAL_PAGE: NativePage = NativePage::pane(
    vmux_terminal::event::TERMINAL_PAGE_URL,
    vmux_terminal::page::Page,
);

#[cfg(target_os = "macos")]
pub static SETTINGS_PAGE: NativePage = NativePage::pane(
    vmux_setting::event::SETTINGS_PAGE_URL,
    vmux_setting::page::Page,
);

#[cfg(target_os = "macos")]
pub static SERVICES_PAGE: NativePage = NativePage::pane(
    vmux_layout::event::SERVICES_PAGE_URL,
    vmux_service::page::Page,
);

#[cfg(target_os = "macos")]
pub static SPACES_PAGE: NativePage =
    NativePage::pane(vmux_wire::space::SPACES_PAGE_URL, vmux_space::page::Page);

#[cfg(target_os = "macos")]
pub static TOOLS_PAGE: NativePage =
    NativePage::pane("vmux://tools/", vmux_layout::tools_page::Page);

#[cfg(target_os = "macos")]
pub static VAULT_PAGE: NativePage =
    NativePage::pane("vmux://vault/", vmux_layout::vault_page::Page).owning_subtree();

#[cfg(target_os = "macos")]
pub static EXTENSIONS_PAGE: NativePage = NativePage::pane(
    vmux_core::event::EXTENSIONS_PAGE_URL,
    vmux_layout::extensions_page::Page,
);

#[cfg(target_os = "macos")]
pub static ERROR_PAGE: NativePage = NativePage::pane(
    vmux_wire::error::ERROR_PAGE_URL,
    vmux_layout::error_page::Page,
);

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
            &TERMINAL_PAGE,
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
    fn the_editor_still_answers_for_file_urls() {
        assert_eq!(FILES_PAGE.url, "file://");
        assert!(FILES_PAGE.answers_for("file:///Users/me/a.rs"));
        assert_eq!(FILES_PAGE.document_url(), "vmux://files/");
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
