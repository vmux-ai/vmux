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
}

impl Plugin for NativePagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<NativePagesPlugin>() {
            app.add_plugins(NativePagesPlugin);
        }
        app.world_mut()
            .resource_mut::<NativePages>()
            .0
            .push((self.page, self.placement));
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
        }
    }

    /// The window's own chrome: full-window, and in front of every pane.
    pub fn as_layout(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Layout,
        }
    }

    /// A page drawn over the panes rather than among them.
    pub fn as_modal(page: &'static NativePage) -> Self {
        Self {
            page,
            placement: Placement::Modal,
        }
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
    url: vmux_layout::start::START_PAGE_URL,
    component: vmux_layout::start::page::StartPage,
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
struct NativePages(Vec<(&'static NativePage, Placement)>);

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
