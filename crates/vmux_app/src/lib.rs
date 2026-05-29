pub const VMUX_APP_HOSTS: [&str; 7] = [
    "layout",
    "command-bar",
    "terminal",
    "services",
    "history",
    "spaces",
    "settings",
];

pub const VMUX_APP_DIST_DIR: &str = "dist";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmuxAppRegistration {
    pub host: &'static str,
    pub bundle_dir: &'static str,
}

pub fn vmux_app_registrations() -> impl Iterator<Item = VmuxAppRegistration> {
    VMUX_APP_HOSTS.into_iter().map(|host| VmuxAppRegistration {
        host,
        bundle_dir: VMUX_APP_DIST_DIR,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub struct VmuxAppPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl bevy::prelude::Plugin for VmuxAppPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<vmux_server::Server>();
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut server = app.world_mut().resource_mut::<vmux_server::Server>();
        for registration in vmux_app_registrations() {
            server.register(
                manifest_dir.clone(),
                &vmux_server::PageConfig::with_custom_host(registration.host)
                    .with_bundle_dir(registration.bundle_dir),
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
#[allow(non_snake_case)]
pub fn App() -> Element {
    let host = current_host();
    match host.as_str() {
        "layout" => rsx! { vmux_layout::page::Page {} },
        "command-bar" => rsx! { vmux_layout::command_bar::page::Page {} },
        "terminal" => rsx! { vmux_terminal::page::Page {} },
        "services" => rsx! { vmux_service::page::Page {} },
        "history" => rsx! { vmux_history::page::Page {} },
        "spaces" => rsx! { vmux_space::page::Page {} },
        "settings" => rsx! { vmux_setting::page::Page {} },
        _ => rsx! { UnknownPage { host } },
    }
}

#[cfg(target_arch = "wasm32")]
fn current_host() -> String {
    web_sys::window()
        .and_then(|window| window.location().host().ok())
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
#[component]
fn UnknownPage(host: String) -> Element {
    rsx! {
        div { class: "flex h-screen items-center justify-center bg-background text-foreground",
            div { class: "text-sm text-muted-foreground", "Unknown vmux app host: {host}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_cover_existing_vmux_pages() {
        assert_eq!(
            VMUX_APP_HOSTS,
            [
                "layout",
                "command-bar",
                "terminal",
                "services",
                "history",
                "spaces",
                "settings"
            ]
        );
    }

    #[test]
    fn every_host_uses_the_shared_dist() {
        for registration in vmux_app_registrations() {
            assert_eq!(registration.bundle_dir, "dist");
        }
    }
}
