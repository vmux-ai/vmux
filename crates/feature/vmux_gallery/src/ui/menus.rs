use crate::{
    GalleryAgentRequest, GalleryBranchRequest, GalleryClickRequest, GalleryCursorRequest,
    GalleryEffortRequest, GalleryMenuRequest, GalleryModelRequest, GalleryPermissionRequest,
    GalleryProjectRequest, GalleryUiState, Story,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::agent_menu::AgentMenu;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::effort_menu::EffortMenu;
use vmux_ui::components::mcp_menu::McpMenu;
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::permission_menu::PermissionMenu;
use vmux_ui::components::project_picker::{
    BranchPicker, ProjectPicker, ProjectSelectPopup, ProjectSelectTrigger,
};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::hooks::send;

#[component]
pub(super) fn MenuPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    if state.story == Story::PromptMediaOptions {
        return rsx! {
            div { class: "mx-auto max-w-md",
                PromptMediaOptions { items: state.examples.media, selected: state.cursor, loading: false,
                    on_hover: move |cursor| { let _ = send(&GalleryCursorRequest { revision, cursor }); },
                    on_select: move |index| { let _ = send(&GalleryClickRequest { revision, label: format!("PromptMediaOptions:{index}") }); },
                }
            }
        };
    }
    let menu = match state.story {
        Story::AgentMenu => "agent",
        Story::ModelMenu => "model",
        Story::EffortMenu => "effort",
        Story::PermissionMenu => "permission",
        Story::ProjectPicker => "project",
        Story::McpMenu => "mcp",
        _ => "",
    };
    rsx! {
        fieldset { disabled: state.disabled, class: "mx-auto w-full max-w-lg space-y-6",
            div { class: "relative min-h-80",
                Button { variant: ButtonVariant::Secondary,
                    onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: menu.into() }); },
                    "{state.story.name()}"
                }
                if state.menu == menu {
                    MenuPopup { state: state.clone() }
                }
            }
            if state.story == Story::ProjectPicker {
                div { class: "relative min-h-80",
                    ProjectSelectTrigger { label: state.project.clone(), title: "ProjectSelectTrigger", open: state.menu == "project-select",
                        onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "project-select".into() }); },
                    }
                    if state.menu == "project-select" {
                        ProjectSelectPopup {
                            projects: state.examples.projects.clone(), selected_path: state.project.clone(),
                            on_pick: move |row: vmux_api::space::ProjectRow| { let _ = send(&GalleryProjectRequest { revision, value: row.path }); },
                            on_add: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ProjectSelectPopup.add".into() }); },
                        }
                    }
                }
                div { class: "relative min-h-80",
                    Button { onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "branch".into() }); }, "BranchPicker" }
                    if state.menu == "branch" {
                        BranchPicker { project: state.project.clone(), branches: state.examples.branches.clone(), loaded: true,
                            on_hover: move |cursor| { let _ = send(&GalleryCursorRequest { revision, cursor }); },
                            on_pick: move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryBranchRequest { revision, value: pick.branch }); },
                            on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MenuPopup(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let hover = EventHandler::new(move |cursor| {
        let _ = send(&GalleryCursorRequest { revision, cursor });
    });
    let dismiss = EventHandler::new(move |_| {
        let _ = send(&GalleryMenuRequest {
            revision,
            menu: String::new(),
        });
    });
    match state.story {
        Story::AgentMenu => rsx! { AgentMenu {
            options: state.examples.agents, selected_url: state.agent, cursor: state.cursor,
            on_hover: hover, on_dismiss: dismiss,
            on_select: move |value| { let _ = send(&GalleryAgentRequest { revision, value }); },
        } },
        Story::ModelMenu => rsx! { ModelMenu {
            models: state.examples.models, current_model_id: state.model, selected: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |(_, entry): (usize, vmux_api::session::ModelOptionEntry)| { let _ = send(&GalleryModelRequest { revision, value: entry.id }); },
        } },
        Story::EffortMenu => rsx! { EffortMenu {
            levels: vec!["low".into(), "medium".into(), "high".into()], selected: state.effort, cursor: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |value| { let _ = send(&GalleryEffortRequest { revision, value }); },
        } },
        Story::PermissionMenu => rsx! { PermissionMenu {
            modes: state.examples.modes, current_mode_id: state.permission, selected: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |entry: vmux_api::protocol::AcpModeOption| { let _ = send(&GalleryPermissionRequest { revision, value: entry.id }); },
        } },
        Story::ProjectPicker => rsx! { ProjectPicker {
            projects: state.examples.projects, loaded: true, cursor: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_pick: move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryProjectRequest { revision, value: pick.project }); },
            on_choose_another: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ProjectPicker.choose_another".into() }); },
        } },
        Story::McpMenu => rsx! { McpMenu {
            entries: state.examples.mcp.servers.clone(), state: state.examples.mcp, selected: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |index| { let _ = send(&GalleryClickRequest { revision, label: format!("McpMenu:{index}") }); },
        } },
        _ => rsx! {},
    }
}
