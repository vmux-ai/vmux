#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::project_picker::ProjectSelectPopupContent;
use vmux_ui::hooks::{send, use_theme};

use crate::event::{
    ProjectDropdownAddRequest, ProjectDropdownCloseRequest, ProjectDropdownPickRequest,
    ProjectDropdownOpenSelectedRequest, ProjectDropdownSizeRequest, ProjectDropdownState,
};

const ROOT_ID: &str = "project-dropdown-surface";

#[vmux_page::page(
    page = "project_dropdown",
    component = ProjectDropdownView,
    states = [vmux_ui::theme::UiTheme],
    placement = modal,
    transparent,
    takes = crate::event::ProjectDropdownState,
    body_class = "m-0 block bg-transparent p-0 text-foreground antialiased",
    root_class = "block min-w-0"
)]
pub struct ProjectDropdownPage;

#[component]
fn ProjectDropdownView() -> Element {
    use_theme();
    let state = use_hook(consume_context::<ProjectDropdownState>);
    let id = state.id.clone();
    let measured_id = id.clone();
    use_future(move || {
        let measured_id = measured_id.clone();
        async move {
            let mut eval = document::eval(&format!(
                r#"
requestAnimationFrame(() => {{
  const root = document.getElementById('{ROOT_ID}');
  if (!root) return;
  root.focus({{ preventScroll: true }});
  const rect = root.getBoundingClientRect();
  dioxus.send([Math.ceil(rect.width), Math.ceil(rect.height)]);
}});
"#
            ));
            let Ok([width, height]) = eval.recv::<[f32; 2]>().await else {
                return;
            };
            send(&ProjectDropdownSizeRequest {
                id: measured_id,
                width,
                height,
            });
        }
    });
    let close_id = id.clone();
    let add_id = id.clone();
    let width = state.anchor.width;
    rsx! {
        div {
            id: ROOT_ID,
            tabindex: "0",
            class: "max-h-64 min-w-0 overflow-y-auto rounded-lg bg-popover/20 text-foreground outline-none",
            style: "width:{width}px;",
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    send(&ProjectDropdownCloseRequest {
                        id: close_id.clone(),
                    });
                }
            },
            ProjectSelectPopupContent {
                projects: state.projects.clone(),
                selected_path: state.selected_path.clone(),
                on_pick: move |project: vmux_ecs::event::ProjectRow| {
                    send(&ProjectDropdownPickRequest {
                        id: id.clone(),
                        path: project.path,
                    });
                },
                on_add: move |_| {
                    send(&ProjectDropdownAddRequest {
                        id: add_id.clone(),
                    });
                },
                on_open_selected: move |_| {
                    send(&ProjectDropdownOpenSelectedRequest {
                        id: state.id.clone(),
                    });
                },
            }
        }
    }
}
