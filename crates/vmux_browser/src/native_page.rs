//! Pages whose components run in this process, painted by a `WKWebView` of their own.
//!
//! One plugin per page, named with where it goes:
//!
//! ```ignore
//! NativePagePlugin::as_layout(&LAYOUT_PAGE),
//! NativePagePlugin::in_pane(&START_PAGE),
//! ```
//!
//! The surface itself belongs to [`vmux_native`], which knows nothing of this app. What lives here
//! is the half that does: which entity gets a view, where that view sits, and who is holding the
//! keyboard — and the three channels a page reaches back through, in
//! [`PageEmbedder`](macos::PageEmbedder).

use bevy::prelude::*;
use vmux_native::NativePage;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

/// Run one page in this process.
///
/// Added once per page. The shared half — the surfaces, and the systems that build, place and
/// render them — is registered by the first one to build and left alone by the rest.
pub struct NativePagePlugin {
    page: &'static NativePage,
    placement: Placement,
    instance: Option<ReadInstance>,
}

/// Reads a page's per-view data off the entity it was opened for.
///
/// A function pointer rather than a generic parameter on the plugin, because every page is stored
/// in one list and only some of them have anything per view to read.
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

    /// Every page is another instance of this, and Bevy rejects a repeated plugin by type.
    fn is_unique(&self) -> bool {
        false
    }
}

impl NativePagePlugin {
    /// A page filling the pane it was opened into.
    pub fn in_pane(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Pane,
            instance: None,
        }
    }

    /// The window's own chrome: full-window, and in front of every pane.
    pub fn as_layout(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Layout,
            instance: None,
        }
    }

    /// A page drawn over the panes rather than among them.
    pub fn as_modal(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Modal,
            instance: None,
        }
    }

    /// Give the page the `C` on the entity it was opened for, before its first render.
    ///
    /// For a page whose two views differ — the error page shows a different failure in each. The
    /// url cannot carry it, because a [`NativePage`] is one const per url; the host builds the
    /// `VirtualDom` itself, so the difference goes in the root scope instead and the page reads it
    /// with `try_consume_context` rather than asking over IPC and rendering twice.
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

/// The layout: the window's chrome, transparent and drawn over every pane.
///
/// macOS only, along with every other page here: a page's components are a `ui` target's, and the
/// only target that is both `ui` and `host` is this one.
///
/// The document below is the wasm bundle's own `index.html` with the wasm removed. It is not
/// decoration: without `index.css` nothing has a Tailwind rule, and without the height and flex
/// rules on `html`, `body` and the root, a flex child has no box to fill — which renders as one
/// icon at its intrinsic size filling the window.
#[cfg(target_os = "macos")]
pub static LAYOUT_PAGE: NativePage = NativePage {
    url: vmux_layout::event::LAYOUT_PAGE_URL,
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
};

/// The launcher: opaque, because it fills its pane rather than floating over one.
#[cfg(target_os = "macos")]
pub static START_PAGE: NativePage = NativePage {
    url: vmux_start::START_PAGE_URL,
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
};

/// Browsing history.
#[cfg(target_os = "macos")]
pub static HISTORY_PAGE: NativePage =
    NativePage::pane(vmux_history::PAGE_URL, vmux_history::page::Page);

/// The user and the agents in the active space.
#[cfg(target_os = "macos")]
pub static TEAM_PAGE: NativePage =
    NativePage::pane(vmux_core::event::team::TEAM_PAGE_URL, vmux_team::page::Page);

/// The agents that can be installed and run.
#[cfg(target_os = "macos")]
pub static AGENTS_PAGE: NativePage = NativePage::pane("vmux://agents/", vmux_agent::page::Page);

/// The app's settings.
#[cfg(target_os = "macos")]
pub static SETTINGS_PAGE: NativePage = NativePage::pane(
    vmux_setting::event::SETTINGS_PAGE_URL,
    vmux_setting::page::Page,
);

/// The background services the daemon is running.
#[cfg(target_os = "macos")]
pub static SERVICES_PAGE: NativePage = NativePage::pane(
    vmux_layout::event::SERVICES_PAGE_URL,
    vmux_service::page::Page,
);

/// The workspaces a window can switch between.
#[cfg(target_os = "macos")]
pub static SPACES_PAGE: NativePage =
    NativePage::pane(vmux_wire::space::SPACES_PAGE_URL, vmux_space::page::Page);

/// The tools an agent can be given.
#[cfg(target_os = "macos")]
pub static TOOLS_PAGE: NativePage =
    NativePage::pane("vmux://tools/", vmux_layout::tools_page::Page);

/// The installed browser extensions.
#[cfg(target_os = "macos")]
pub static EXTENSIONS_PAGE: NativePage = NativePage::pane(
    vmux_core::event::EXTENSIONS_PAGE_URL,
    vmux_layout::extensions_page::Page,
);

/// What a pane shows where a page failed to open, or where no page answers the url.
///
/// The one page never asked for by name: a view carries it because something else could not be
/// opened, so what it reports arrives as a component on that view rather than as part of a url.
#[cfg(target_os = "macos")]
pub static ERROR_PAGE: NativePage = NativePage::pane(
    vmux_wire::error::ERROR_PAGE_URL,
    vmux_layout::error_page::Page,
);

/// Where a native page's view goes.
///
/// Not how it looks: whether a page is see-through is the page's own
/// [`transparent`](NativePage::transparent), because it follows from what the page draws rather
/// than from where it was put.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    /// Fills the window, and stays in front of everything drawn into it.
    ///
    /// Its entity is the layout's rather than a pane's, and it exists before any pane does.
    Layout,
    /// Fills the pane its page was opened into, and is hidden when that pane is not on screen.
    Pane,
    /// Its pane's rectangle, but in front of the panes rather than among them.
    Modal,
}

/// Every page registered by a [`NativePagePlugin`], and where each one goes.
#[derive(Resource, Default)]
struct NativePages(Vec<(&'static NativePage, Placement, Option<ReadInstance>)>);

/// The half of native page hosting that exists once, however many pages there are.
///
/// Added by the shell whether or not any page is, so a build with no renderer says so.
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
