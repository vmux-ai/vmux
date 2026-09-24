#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::platform::sleep_ms;

use crate::event::{
    LayoutOverlayEvent, RemoteCopyEvent, RemotePhase, RemoteRequest, RemoteRevokeRequest,
    RemoteStateEvent,
};

#[component]
pub(crate) fn RemoteControl(remote: RemoteStateEvent) -> Element {
    let mut open = use_signal(|| false);
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "remote".to_string(),
            active: open(),
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "remote".to_string(),
            active: false,
        });
    });
    rsx! {
        div { class: "relative ml-1 shrink-0",
            div {
                class: if remote.enabled {
                    "group flex h-7 items-center overflow-hidden rounded-full border border-success/25 bg-success/10 text-success transition-colors hover:bg-success/20"
                } else {
                    "group flex h-7 items-center overflow-hidden rounded-full border border-foreground/10 bg-foreground/[0.04] text-muted-foreground transition-colors hover:bg-foreground/[0.09]"
                },
                button {
                    r#type: "button",
                    class: "flex h-full items-center gap-1.5 pl-2 pr-1.5 text-[10px] font-semibold",
                    aria_label: "Live",
                    onclick: move |_| open.set(!open()),
                    span { class: if remote.enabled { "size-1.5 rounded-full bg-success" } else { "size-1.5 rounded-full bg-muted-foreground/50" } }
                    "Live"
                }
                button {
                    r#type: "button",
                    class: "relative mx-1 h-4 w-7 shrink-0 rounded-full bg-foreground/15",
                    aria_label: "Toggle Live",
                    aria_pressed: remote.enabled,
                    onclick: move |_| {
                        let _ = send(&RemoteRequest {
                            enabled: !remote.enabled,
                        });
                    },
                    span {
                        class: if remote.enabled {
                            "absolute left-3.5 top-0.5 size-3 rounded-full bg-white shadow-sm transition-all"
                        } else {
                            "absolute left-0.5 top-0.5 size-3 rounded-full bg-foreground/70 shadow-sm transition-all"
                        }
                    }
                }
            }
            if open() {
                button {
                    r#type: "button",
                    class: "pointer-events-auto fixed inset-0 z-[998] m-0 h-screen w-screen cursor-default border-0 bg-transparent p-0 outline-none",
                    aria_label: translate("common-close"),
                    onpointerdown: move |event| {
                        event.prevent_default();
                        open.set(false);
                    },
                    oncontextmenu: move |event| {
                        event.prevent_default();
                        open.set(false);
                    },
                }
                div { class: "glass absolute right-0 top-9 z-[999] w-72 overflow-hidden rounded-xl border border-border/80 bg-background/95 shadow-2xl backdrop-blur-xl",
                    RemotePanel { remote: remote.clone() }
                }
            }
        }
    }
}

#[component]
fn RemotePanel(remote: RemoteStateEvent) -> Element {
    let mut show_pairing = use_signal(|| false);
    let mut pairing_generation = use_signal(|| 0_u64);
    let mut pairing_started_paired = use_signal(|| false);
    let mut copied = use_signal(|| false);
    let mut dismissed_pairing_link = use_signal(String::new);
    let active = remote.phase == RemotePhase::Enabled;
    let transitioning = remote.phase == RemotePhase::Starting;
    let pairing_visible = show_pairing()
        || (active
            && !remote.paired
            && !remote.pairing_deep_link.is_empty()
            && dismissed_pairing_link() != remote.pairing_deep_link);
    let status = match remote.phase {
        RemotePhase::Disabled | RemotePhase::Enabled => None,
        RemotePhase::Starting if remote.enabled => Some("Starting…"),
        RemotePhase::Starting => Some("Stopping…"),
        RemotePhase::Error => Some("Needs attention"),
    };
    let qr = if active
        && pairing_visible
        && (!remote.paired || pairing_started_paired())
        && !remote.pairing_deep_link.is_empty()
    {
        PairingCode::svg(&remote.pairing_deep_link)
    } else {
        None
    };
    rsx! {
        div { class: "p-3",
            div { class: "flex items-center gap-2",
                div {
                    class: if remote.enabled {
                        "flex size-7 shrink-0 items-center justify-center rounded-md bg-success/15 text-success"
                    } else {
                        "flex size-7 shrink-0 items-center justify-center rounded-md bg-foreground/5 text-muted-foreground"
                    },
                    Icon { class: "size-4",
                        path { d: "M12 2a10 10 0 1 0 10 10" }
                        path { d: "M12 12 22 2" }
                        path { d: "M15 2h7v7" }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "text-ui font-semibold", "Live" }
                    if let Some(status) = status {
                        div {
                            class: if remote.phase == RemotePhase::Error {
                                "mt-0.5 truncate text-[10px] text-destructive"
                            } else {
                                "mt-0.5 text-[10px] text-muted-foreground"
                            },
                            "{status}"
                        }
                    }
                }
                button {
                    r#type: "button",
                    class: if remote.enabled {
                        "relative h-5 w-9 shrink-0 rounded-full bg-success transition-colors"
                    } else {
                        "relative h-5 w-9 shrink-0 rounded-full bg-foreground/15 transition-colors"
                    },
                    aria_label: "Toggle Live",
                    aria_pressed: remote.enabled,
                    onclick: move |_| {
                        dismissed_pairing_link.set(String::new());
                        if remote.enabled {
                            pairing_generation.set(pairing_generation().wrapping_add(1));
                            show_pairing.set(false);
                        }
                        let _ = send(&RemoteRequest {
                            enabled: !remote.enabled,
                        });
                    },
                    span {
                        class: if remote.enabled {
                            "absolute left-[18px] top-0.5 size-4 rounded-full bg-white shadow-sm transition-all"
                        } else {
                            "absolute left-0.5 top-0.5 size-4 rounded-full bg-white shadow-sm transition-all"
                        }
                    }
                }
            }
            if remote.phase == RemotePhase::Error {
                div { class: "mt-2 rounded-md border border-destructive/20 bg-destructive/5 p-2",
                    div { class: "break-words text-[10px] leading-4 text-destructive", "{remote.error}" }
                    button {
                        r#type: "button",
                        class: "mt-1.5 text-[10px] font-semibold text-foreground hover:opacity-70",
                        onclick: move |_| {
                            let _ = send(&RemoteRequest {
                                enabled: remote.enabled,
                            });
                        },
                        "Retry"
                    }
                }
            } else if transitioning {
                div { class: "mt-3 flex flex-col gap-2",
                    Skeleton { class: "h-1.5 w-full rounded-full bg-success/25" }
                    div { class: "flex items-center gap-2",
                        Skeleton { class: "h-2 w-24" }
                        Skeleton { class: "ml-auto h-2 w-10" }
                    }
                }
            } else if active {
                if let Some(svg) = qr {
                    div { class: "mt-2 flex items-center justify-between gap-2",
                        div { class: "text-[10px] font-semibold text-foreground", "Connect a device" }
                        button {
                            r#type: "button",
                            class: "rounded px-1.5 py-1 text-[9px] font-semibold text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onclick: move |_| {
                                pairing_generation.set(pairing_generation().wrapping_add(1));
                                dismissed_pairing_link.set(remote.pairing_deep_link.clone());
                                show_pairing.set(false);
                            },
                            "Close"
                        }
                    }
                    div { class: "mt-2 flex flex-col items-center rounded-lg bg-white p-2.5 text-zinc-950",
                        div {
                            class: "w-full rounded-sm [&>svg]:block [&>svg]:aspect-square [&>svg]:h-auto [&>svg]:w-full",
                            dangerous_inner_html: "{svg}",
                        }
                        div { class: "mt-1.5 text-center text-[10px] font-semibold", "Scan with your phone" }
                        div { class: "mt-0.5 text-center text-[9px] text-zinc-500", "Opens Vmux and pairs automatically" }
                    }
                    div { class: "mt-2 flex items-center gap-1.5 rounded-md bg-foreground/5 py-1 pl-2 pr-1",
                        div {
                            class: "min-w-0 flex-1 truncate font-mono text-[9px] text-muted-foreground",
                            title: "{remote.pairing_url}",
                            "{remote.pairing_url}"
                        }
                        button {
                            r#type: "button",
                            class: "shrink-0 rounded px-1.5 py-1 text-[9px] font-semibold text-foreground hover:bg-foreground/10",
                            onclick: move |_| {
                                let _ = send(&RemoteCopyEvent);
                                copied.set(true);
                            },
                            if copied() { "Copied" } else { "Copy" }
                        }
                    }
                    div { class: "mt-1.5 text-[9px] leading-4 text-muted-foreground",
                        "Pairing details hide automatically after 2 minutes."
                    }
                } else {
                    div { class: "mt-2 flex items-center gap-2",
                        div { class: if remote.paired { "flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-success" } else { "flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-muted-foreground" },
                            span { class: if remote.paired { "size-1.5 rounded-full bg-success" } else { "size-1.5 rounded-full bg-foreground/25" } }
                            if remote.paired { "Phone paired" } else { "No phone paired" }
                        }
                        button {
                            r#type: "button",
                            class: "text-[10px] font-semibold text-foreground hover:opacity-70",
                            onclick: move |_| {
                                copied.set(false);
                                pairing_started_paired.set(remote.paired);
                                let generation = pairing_generation().wrapping_add(1);
                                pairing_generation.set(generation);
                                show_pairing.set(true);
                                spawn(async move {
                                    sleep_ms(120_000).await;
                                    if pairing_generation() == generation {
                                        show_pairing.set(false);
                                    }
                                });
                            },
                            if remote.paired { "Show QR" } else { "Connect device" }
                        }
                    }
                }
                if !remote.devices.is_empty() {
                    div { class: "mt-2 space-y-1",
                        for device in remote.devices.iter() {
                            div { class: "flex items-center gap-2 rounded-md bg-foreground/5 px-2 py-1.5",
                                div { class: "min-w-0 flex-1 truncate font-mono text-[9px] text-muted-foreground", "{device.id}" }
                                button {
                                    r#type: "button",
                                    class: "shrink-0 rounded px-1.5 py-1 text-[9px] font-semibold text-destructive hover:bg-destructive/10",
                                    onclick: {
                                        let client_id = device.id.clone();
                                        move |_| {
                                            let _ = send(&RemoteRevokeRequest {
                                                client_id: client_id.clone(),
                                            });
                                        }
                                    },
                                    {translate("common-remove")}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct PairingCode;

impl PairingCode {
    fn svg(value: &str) -> Option<String> {
        use qrcode::QrCode;
        use qrcode::render::svg;

        let code = QrCode::new(value).ok()?;
        Some(
            code.render::<svg::Color>()
                .min_dimensions(148, 148)
                .dark_color(svg::Color("#09090b"))
                .light_color(svg::Color("#ffffff"))
                .build(),
        )
    }
}
