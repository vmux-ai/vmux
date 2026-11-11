#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::input::Input;
use vmux_ui::components::page::{PageContent, PageHeader, PageShell, PageToolbarRow};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;

use crate::event::{
    ContainerLogsRequest, ContainerStartRequest, ContainerStopRequest, ContainerUiState,
};

#[vmux_page::page(component = Page, states = ["PageAddressUiState", "ThemeUiState", "ContainerUiState"])]
pub(crate) struct ContainerPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ContainerUiState>().state;
    let mut initialized = use_signal(|| false);
    let mut data = use_signal(String::new);
    let mut shared = use_signal(String::new);
    let mut cpus = use_signal(|| "2".to_string());
    let mut memory = use_signal(|| "2048".to_string());
    use_effect(move || {
        let snapshot = state();
        if !initialized() && !snapshot.data_directory.is_empty() {
            data.set(snapshot.data_directory);
            shared.set(snapshot.shared_directory);
            cpus.set(snapshot.cpus.to_string());
            memory.set(snapshot.memory_mib.to_string());
            initialized.set(true);
        }
    });
    let snapshot = state();
    if snapshot.data_directory.is_empty() {
        return rsx! {
            PageShell {
                PageContent {
                    div { class: "mx-auto max-w-2xl space-y-4",
                        div { class: "h-8 w-48 animate-pulse rounded-md bg-muted/50" }
                        div { class: "h-32 animate-pulse rounded-md bg-muted/30" }
                        div { class: "h-32 animate-pulse rounded-md bg-muted/30" }
                    }
                }
            }
        };
    }
    let locked = snapshot.busy || snapshot.running;
    let can_start = snapshot.supported
        && !locked
        && !data().trim().is_empty()
        && !shared().trim().is_empty()
        && cpus().parse::<u8>().is_ok_and(|value| value > 0)
        && memory().parse::<u32>().is_ok_and(|value| value >= 512);
    let phase = if snapshot.busy && !snapshot.running {
        "created"
    } else {
        snapshot.phase.as_str()
    };
    let phase_label = translate(&format!("container-phase-{phase}"));
    rsx! {
        PageShell {
            PageToolbarRow {
                Button {
                    size: ButtonSize::Sm,
                    disabled: !can_start,
                    onclick: move |_| {
                        let _ = send(&ContainerStartRequest {
                            data_directory: data(), shared_directory: shared(),
                            cpus: cpus().parse().unwrap_or(0), memory_mib: memory().parse().unwrap_or(0),
                        });
                    },
                    {translate("container-start")}
                }
                Button {
                    size: ButtonSize::Sm, variant: ButtonVariant::Secondary,
                    disabled: !snapshot.running || snapshot.busy,
                    onclick: move |_| { let _ = send(&ContainerStopRequest); },
                    {translate("container-stop")}
                }
                span { class: "ml-auto text-xs text-muted-foreground", "{phase_label}" }
            }
            PageHeader {
                h1 { class: "text-base font-semibold", {translate("container-title")} }
                p { class: "mt-1 text-xs text-muted-foreground", {translate("container-description")} }
            }
            PageContent {
                div { class: "mx-auto max-w-2xl space-y-5",
                    if !snapshot.supported {
                        p { class: "text-sm text-muted-foreground", {translate("container-unsupported")} }
                    }
                    label { class: "block space-y-1.5",
                        span { class: "text-xs font-medium", {translate("container-data-directory")} }
                        Input { value: data(), disabled: locked, oninput: move |event: FormEvent| data.set(event.value()) }
                    }
                    label { class: "block space-y-1.5",
                        span { class: "text-xs font-medium", {translate("container-shared-directory")} }
                        Input { value: shared(), disabled: locked, placeholder: translate("container-select-project"), oninput: move |event: FormEvent| shared.set(event.value()) }
                    }
                    div { class: "grid grid-cols-2 gap-4",
                        label { class: "block space-y-1.5",
                            span { class: "text-xs font-medium", {translate("container-cpus")} }
                            Input { r#type: "number", min: "1", value: cpus(), disabled: locked, oninput: move |event: FormEvent| cpus.set(event.value()) }
                        }
                        label { class: "block space-y-1.5",
                            span { class: "text-xs font-medium", {translate("container-memory")} }
                            Input { r#type: "number", min: "512", step: "256", value: memory(), disabled: locked, oninput: move |event: FormEvent| memory.set(event.value()) }
                        }
                    }
                    if !snapshot.socket.is_empty() {
                        div { class: "space-y-1",
                            span { class: "text-xs font-medium", {translate("container-docker-socket")} }
                            code { class: "block break-all rounded-md bg-muted/40 px-3 py-2 text-xs text-muted-foreground", "unix://{snapshot.socket}" }
                        }
                    }
                    if !snapshot.error.is_empty() {
                        pre { class: "whitespace-pre-wrap rounded-md bg-destructive/10 p-3 text-xs text-destructive", "{snapshot.error}" }
                    }
                    section { class: "space-y-2",
                        div { class: "flex items-center justify-between",
                            h2 { class: "text-xs font-medium", {translate("container-logs")} }
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| { let _ = send(&ContainerLogsRequest); }, {translate("container-refresh-logs")} }
                        }
                        if snapshot.logs.is_empty() {
                            p { class: "rounded-md bg-muted/30 p-4 text-xs text-muted-foreground", {translate("container-logs-empty")} }
                        } else {
                            pre { class: "max-h-96 overflow-auto rounded-md bg-muted/40 p-3 font-mono text-xs text-muted-foreground", "{snapshot.logs}" }
                        }
                    }
                }
            }
        }
    }
}
