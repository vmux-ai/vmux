use crate::{
    GalleryAgentRequest, GalleryAttachRequest, GalleryBranchRequest, GalleryCheckedRequest,
    GalleryClickRequest, GalleryCursorRequest, GalleryEffortRequest, GalleryMenuRequest,
    GalleryModelRequest, GalleryPasteRequest, GalleryPermissionRequest, GalleryProjectRequest,
    GalleryRemoveAttachmentRequest, GalleryStatusRequest, GalleryStopRequest, GallerySubmitRequest,
    GalleryTextRequest, GalleryUiState, Story,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::composer::{ComposerFrame, PromptComposer, PromptComposerMode};
use vmux_ui::components::composer_bar::{
    AgentMenuData, BranchMenuData, ComposerBar, ComposerChip, ComposerChipVisual, ComposerMenuKind,
    ComposerMenus, EffortMenuData, ModelMenuData, PermissionMenuData, ProjectMenuData,
};
use vmux_ui::components::input::Input;
use vmux_ui::components::notice::Notice;
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn ComposerPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    if state.story == Story::PromptBox {
        return rsx! {
            div { class: "mx-auto w-full max-w-xl space-y-6 pb-72",
                for glass in [true, false] {
                    div { class: "relative",
                        p { class: "mb-2 font-mono text-xs text-muted-foreground", "glass: {glass}" }
                        PromptBox { glass,
                            Input { value: state.text.clone(), disabled: state.disabled, class: "min-w-0 flex-1", placeholder: translate("gallery-sample"),
                                oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
                            }
                            Button { disabled: state.disabled, variant: ButtonVariant::Ghost,
                                onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: if glass { "prompt".into() } else { "prompt-flat".into() } }); },
                                "+"
                            }
                            if state.menu == if glass { "prompt" } else { "prompt-flat" } {
                                PromptPopup { heading: "PromptPopup", on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                                    Button { class: "w-full", onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "PromptPopup".into() }); }, {translate("gallery-sample")} }
                                }
                            }
                        }
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "mx-auto w-full max-w-4xl space-y-6 py-16",
            div { class: "flex flex-wrap gap-2",
                Button { variant: ButtonVariant::Secondary,
                    onclick: move |_| { let _ = send(&GalleryTextRequest { revision, text: "Review this UI component".into() }); },
                    {translate("gallery-sample")}
                }
                Button { variant: ButtonVariant::Ghost,
                    onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                    "notice / completion: {state.checked}"
                }
            }
            if state.story == Story::ComposerBar {
                ComposerStrip { state }
            } else {
                ComposerFrame {
                    notice: if state.checked { Some(rsx! { Notice { message: translate("gallery-sample"), attached: true } }) } else { None },
                    PromptComposer {
                        value: state.text.clone(),
                        placeholder: translate("gallery-sample"),
                        accent_color: "var(--primary)",
                        accent_gradient: "from-primary to-primary/70",
                        disabled: state.disabled,
                        attachments: state.examples.attachments.clone(),
                        completion: if state.checked { " and describe the expected behavior".into() } else { String::new() },
                        mode: if state.running { PromptComposerMode::Stop } else { PromptComposerMode::Send },
                        action_title: if state.running { translate("common-stop") } else { translate("command-send") },
                        action_enabled: state.running || !state.text.trim().is_empty(),
                        on_input: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                        on_keydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter && !event.modifiers().shift() {
                                event.prevent_default();
                                let _ = send(&GallerySubmitRequest { revision });
                            }
                        },
                        on_paste: move |_| { let _ = send(&GalleryPasteRequest { revision }); },
                        on_attach: move |_| { let _ = send(&GalleryAttachRequest { revision }); },
                        on_remove_attachment: move |index| { let _ = send(&GalleryRemoveAttachmentRequest { revision, index }); },
                        on_action: move |_| {
                            if state.running { let _ = send(&GalleryStopRequest { revision }); }
                            else { let _ = send(&GallerySubmitRequest { revision }); }
                        },
                        footer: Some(rsx! { ComposerStrip { state: state.clone() } }),
                    }
                }
            }
        }
    }
}

#[component]
fn ComposerStrip(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let opened = match state.menu.as_str() {
        "agent" => Some(ComposerMenuKind::Agent),
        "model" => Some(ComposerMenuKind::Model),
        "effort" => Some(ComposerMenuKind::Effort),
        "permission" => Some(ComposerMenuKind::Permission),
        "project" => Some(ComposerMenuKind::Project),
        "branch" => Some(ComposerMenuKind::Branch),
        _ => None,
    };
    rsx! {
        div { class: "relative @container",
            ComposerBar {
                opened,
                agent: Some(ComposerChip { label: state.agent.clone(), title: "AgentMenu".into(), visual: ComposerChipVisual::Agent { icon: Default::default(), seed: state.agent.clone() }, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "agent".into() }); })) }),
                model: Some(ComposerChip { label: state.model.clone(), title: "ModelMenu".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "model".into() }); })) }),
                effort: Some(ComposerChip { label: state.effort.clone(), title: "EffortMenu".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "effort".into() }); })) }),
                permission: Some(ComposerChip { label: state.permission.clone(), title: "PermissionMenu".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "permission".into() }); })) }),
                project: Some(ComposerChip { label: state.project.clone(), title: "ProjectPicker".into(), visual: ComposerChipVisual::Identity { seed: state.project.clone() }, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "project".into() }); })) }),
                branch: Some(ComposerChip { label: state.branch.clone(), title: "BranchPicker".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "branch".into() }); })) }),
                is_git_repo: true, workspace_known: true, uncommitted: 3, ahead: 2,
                status: state.status.clone(), active_subagents: 2, active_tasks: 1, queued_count: 3,
            }
            ComposerMenus {
                opened, cursor: state.cursor,
                placement: if state.story == Story::PromptComposer { PromptPopupPlacement::Upward } else { PromptPopupPlacement::Downward },
                on_hover: move |cursor| { let _ = send(&GalleryCursorRequest { revision, cursor }); },
                on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                agent: Some(AgentMenuData { options: state.examples.agents.clone(), selected_url: state.agent.clone(), on_select: EventHandler::new(move |value| { let _ = send(&GalleryAgentRequest { revision, value }); }) }),
                model: Some(ModelMenuData { models: state.examples.models.clone(), current_model_id: state.model.clone(), on_select: EventHandler::new(move |entry: vmux_api::session::ModelOptionEntry| { let _ = send(&GalleryModelRequest { revision, value: entry.id }); }) }),
                effort: Some(EffortMenuData { levels: vec!["low".into(), "medium".into(), "high".into()], selected: state.effort.clone(), on_select: EventHandler::new(move |value| { let _ = send(&GalleryEffortRequest { revision, value }); }) }),
                permission: Some(PermissionMenuData { modes: state.examples.modes.clone(), current_mode_id: state.permission.clone(), on_select: EventHandler::new(move |entry: vmux_api::protocol::AcpModeOption| { let _ = send(&GalleryPermissionRequest { revision, value: entry.id }); }) }),
                project: Some(ProjectMenuData { projects: state.examples.projects.clone(), loaded: true, on_pick: EventHandler::new(move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryProjectRequest { revision, value: pick.project }); }), on_choose_another: EventHandler::new(move |_| { let _ = send(&GalleryClickRequest { revision, label: "ProjectPicker.choose_another".into() }); }) }),
                branch: Some(BranchMenuData { project: state.project.clone(), branches: state.examples.branches.clone(), loaded: true, on_pick: EventHandler::new(move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryBranchRequest { revision, value: pick.branch }); }) }),
            }
            div { class: "mt-4 flex flex-wrap gap-1",
                for status in ["idle", "streaming", "awaiting", "installing", "errored", "in_use"] {
                    Button { variant: ButtonVariant::Ghost,
                        onclick: move |_| { let _ = send(&GalleryStatusRequest { revision, status: status.into() }); },
                        "{status}"
                    }
                }
            }
        }
    }
}
