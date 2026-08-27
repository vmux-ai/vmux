use dioxus::prelude::*;

use crate::components::icon::Icon;
use crate::file_icon::TypeIcon;

pub const SIDEBAR_TREE_ROW: &str = "flex h-8 w-full min-w-0 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-left text-muted-foreground group-hover/project:text-foreground hover:bg-glass-hover hover:text-foreground";

#[component]
pub fn SidebarTreeRow(
    path: String,
    label: String,
    is_dir: bool,
    #[props(default)] expanded: bool,
    #[props(default)] depth: u32,
    #[props(default)] emphasis: bool,
    #[props(default)] title: Option<String>,
    #[props(default = rsx! {})] trailing: Element,
    on_activate: EventHandler<()>,
) -> Element {
    let indent = 0.375 + f64::from(depth) * 0.75;
    let hint = title.unwrap_or_else(|| path.clone());
    rsx! {
        button {
            r#type: "button",
            title: "{hint}",
            class: SIDEBAR_TREE_ROW,
            style: "padding-left:{indent}rem;",
            onclick: move |_| on_activate.call(()),
            if is_dir {
                Icon { class: "h-3 w-3 shrink-0",
                    path { d: if expanded { "m6 9 6 6 6-6" } else { "m9 18 6-6-6-6" } }
                }
                Icon { class: "h-3.5 w-3.5 shrink-0",
                    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                }
            } else {
                span { class: "w-3 shrink-0" }
                TypeIcon { path: path.clone(), is_dir: false, class: "h-3.5 w-3.5 shrink-0" }
            }
            span {
                class: if emphasis { "min-w-0 flex-1 truncate text-ui font-medium" } else { "min-w-0 flex-1 truncate text-ui" },
                "{label}"
            }
            {trailing}
        }
    }
}
