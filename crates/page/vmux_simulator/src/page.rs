#![allow(non_snake_case)]

use crate::url::SimulatorRoute;
use dioxus::prelude::*;
use vmux_ui::hooks::use_theme;

/// The simulator view.
///
/// The device image is not drawn here: it is a Bevy surface composited at this pane's rect, the
/// same way a browser pane works. This half owns the URL and shows the runtime underneath, which
/// is visible only until the first frame covers it.
#[component]
pub fn Page() -> Element {
    use_theme();
    let runtime = match CurrentRoute::read() {
        Some(SimulatorRoute::Pinned(version)) => format!("iOS {version}"),
        Some(SimulatorRoute::Unpinned) | None => String::new(),
    };

    rsx! {
        div { class: "flex h-screen w-screen items-center justify-center bg-transparent",
            div { class: "text-sm text-muted-foreground", "{runtime}" }
        }
    }
}

/// The page's own URL.
struct CurrentRoute;

impl CurrentRoute {
    #[cfg(target_arch = "wasm32")]
    fn read() -> Option<SimulatorRoute> {
        let pathname = web_sys::window()?.location().pathname().ok()?;
        SimulatorRoute::parse(&pathname)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read() -> Option<SimulatorRoute> {
        None
    }
}
