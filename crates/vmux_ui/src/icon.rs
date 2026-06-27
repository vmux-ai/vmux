#[cfg(target_arch = "wasm32")]
pub use wasm::{PageIconView, builtin_icon};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::components::icon::Icon;
    use crate::favicon::Favicon;
    use crate::file_icon::type_icon;
    use dioxus::prelude::*;
    use vmux_core::icon::{BuiltinIcon, PageIcon};

    pub fn builtin_icon(icon: BuiltinIcon, class: &str) -> Element {
        match icon {
            BuiltinIcon::Terminal => rsx! { Icon { class: "{class}",
                path { d: "m4 17 6-6-6-6" }
                path { d: "M12 19h8" }
            } },
            BuiltinIcon::Files => rsx! { Icon { class: "{class}",
                path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
            } },
            BuiltinIcon::Server => rsx! { Icon { class: "{class}",
                path { d: "M4 4h16a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" }
                path { d: "M4 14h16a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-2a2 2 0 0 1 2-2Z" }
                path { d: "M6 7h.01" }
                path { d: "M6 17h.01" }
            } },
            BuiltinIcon::Settings => rsx! { Icon { class: "{class}",
                circle { cx: "12", cy: "12", r: "3" }
                path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" }
            } },
            BuiltinIcon::Clock => rsx! { Icon { class: "{class}",
                circle { cx: "12", cy: "12", r: "10" }
                path { d: "M12 6v6l4 2" }
            } },
            BuiltinIcon::Layers => rsx! { Icon { class: "{class}",
                path { d: "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z" }
                path { d: "m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65" }
                path { d: "m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65" }
            } },
            BuiltinIcon::Users => rsx! { Icon { class: "{class}",
                path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                circle { cx: "9", cy: "7", r: "4" }
                path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
            } },
            BuiltinIcon::Sparkles => rsx! { Icon { class: "{class}",
                path { d: "m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" }
            } },
            BuiltinIcon::Activity => rsx! { Icon { class: "{class}",
                path { d: "M22 12h-4l-3 9L9 3l-3 9H2" }
            } },
            BuiltinIcon::Puzzle => rsx! { Icon { class: "{class}",
                path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
            } },
        }
    }

    #[component]
    pub fn PageIconView(
        icon: PageIcon,
        url: String,
        img_class: String,
        icon_class: String,
    ) -> Element {
        if url.starts_with("file:") {
            return type_icon(&url, url.ends_with('/'), &icon_class);
        }
        if let PageIcon::Builtin(builtin) = &icon {
            return builtin_icon(*builtin, &icon_class);
        }
        let favicon_url = icon.favicon_url().to_string();
        rsx! {
            Favicon {
                favicon_url,
                url,
                class: img_class,
                globe_class: icon_class,
            }
        }
    }
}
