//! The screen the phone opens on: what is running, what can be started, and — before any
//! of that — how to reach a Mac at all.

use crate::qr_scanner;
use dioxus::prelude::*;
use vmux_ui::components::prompt_composer::{PromptComposer, PromptComposerAction};
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::file_icon::FilePath;
use vmux_ui::i18n::translate;
use vmux_ui::launcher::results::CommandBarResultItem;
use vmux_ui::launcher::row::ResultRow;
use vmux_wire::PageIcon;
use vmux_wire::room::{RemoteAgent, RemoteSession};

#[derive(Props, Clone, PartialEq)]
pub(crate) struct MobileStartPageProps {
    pub(crate) paired: bool,
    pub(crate) reachable: bool,
    pub(crate) sessions: Vec<RemoteSession>,
    pub(crate) agents: Vec<RemoteAgent>,
    pub(crate) draft: String,
    pub(crate) error: String,
    pub(crate) creating: bool,
    pub(crate) pair_value: String,
    pub(crate) pair_error: String,
    pub(crate) pairing: bool,
    pub(crate) on_draft: EventHandler<String>,
    pub(crate) on_submit: EventHandler<()>,
    pub(crate) on_open: EventHandler<RemoteSession>,
    pub(crate) on_start_agent: EventHandler<String>,
    pub(crate) on_pair_value: EventHandler<String>,
    pub(crate) on_pair: EventHandler<()>,
    pub(crate) on_scan: EventHandler<()>,
    pub(crate) on_disconnect: EventHandler<()>,
    pub(crate) on_open_team: EventHandler<()>,
}

#[component]
pub(crate) fn MobileStartPage(props: MobileStartPageProps) -> Element {
    let can_submit = !props.creating && !props.draft.trim().is_empty();
    let submit_from_key = props.on_submit;
    let submit_from_action = props.on_submit;
    let on_open = props.on_open;
    let on_start_agent = props.on_start_agent;

    rsx! {
        div {
            class: "relative isolate flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
            header { class: "flex shrink-0 items-center gap-2 px-4 pb-3 pt-[calc(0.75rem+env(safe-area-inset-top))] sm:px-6",
                span { class: "text-sm font-semibold tracking-tight text-foreground", "Vmux" }
                span { class: if props.paired { "ml-auto flex items-center gap-1.5 rounded-full border border-success/20 bg-success/[0.08] px-2.5 py-1 text-[10px] font-medium text-success" } else { "ml-auto flex items-center gap-1.5 rounded-full border border-border bg-muted px-2.5 py-1 text-[10px] font-medium text-muted-foreground" },
                    span { class: if props.paired { "h-1.5 w-1.5 rounded-full bg-success" } else { "h-1.5 w-1.5 rounded-full bg-muted-foreground" } }
                    {if props.reachable { translate("mobile-status-connected") } else if props.paired { translate("mobile-status-reaching") } else { translate("mobile-status-disconnected") }}
                }
                if props.paired {
                    button {
                        class: "ml-2 rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                        r#type: "button",
                        onclick: move |_| props.on_open_team.call(()),
                        {translate("mobile-start-team")}
                    }
                    button {
                        class: "rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                        r#type: "button",
                        onclick: move |_| props.on_disconnect.call(()),
                        {translate("mobile-pair-disconnect")}
                    }
                }
            }
            main { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-[calc(2rem+env(safe-area-inset-bottom))] pt-14 sm:px-6 md:pt-20",
                StartHero {
                    mark: rsx! {
                        div { class: "flex h-11 w-11 items-center justify-center rounded-2xl border border-border bg-gradient-to-br from-violet-500/80 to-cyan-400/80 text-sm font-bold text-white shadow-lg shadow-violet-950/40", "V" }
                    },
                    if props.paired {
                        div { class: "w-full",
                            PromptComposer {
                                value: props.draft.clone(),
                                placeholder: translate("mobile-start-search-placeholder"),
                                accent_color: "#a78bfa".to_string(),
                                accent_gradient: "from-violet-500 to-violet-700".to_string(),
                                autofocus: true,
                                show_attach: false,
                                disabled: props.creating,
                                action: PromptComposerAction::Send,
                                action_title: if props.creating { translate("mobile-start-starting") } else { translate("mobile-start-new-chat") },
                                action_enabled: can_submit,
                                on_input: move |value| props.on_draft.call(value),
                                on_keydown: move |event: KeyboardEvent| {
                                    if event.key() == Key::Enter && !event.modifiers().shift() {
                                        event.prevent_default();
                                        submit_from_key.call(());
                                    }
                                },
                                on_paste: move |_| {},
                                on_attach: move |_| {},
                                on_remove_attachment: move |_| {},
                                on_action: move |_| submit_from_action.call(()),
                            }
                            if !props.error.is_empty() {
                                div { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
                            }
                        }
                        section { class: "mt-6 w-full",
                            div { class: "mb-3 flex items-center gap-2 px-1",
                                h2 { class: "text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground", {translate("mobile-start-stacks")} }
                                span { class: "rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground", "{props.sessions.len()}" }
                            }
                            div { class: "overflow-hidden rounded-2xl border border-border bg-card",
                                if props.sessions.is_empty() {
                                    div { class: "px-4 py-8 text-center text-sm text-muted-foreground", {translate("mobile-start-no-stacks")} }
                                }
                                for (index, session) in props.sessions.iter().cloned().enumerate() {
                                    ResultRow {
                                        key: "{session.sid}",
                                        index,
                                        item: session_result_item(&session),
                                        selected: false,
                                        on_activate: {
                                            let next = session.clone();
                                            move |_| on_open.call(next.clone())
                                        },
                                        on_hover: move |_| {},
                                    }
                                }
                            }
                        }
                        if !props.agents.is_empty() {
                            section { class: "mt-6 w-full",
                                div { class: "mb-3 flex items-center gap-2 px-1",
                                    h2 { class: "text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground", "Start a chat" }
                                }
                                div { class: "overflow-hidden rounded-2xl border border-border bg-card",
                                    for (index, agent) in props.agents.iter().cloned().enumerate() {
                                        ResultRow {
                                            key: "{agent.id}",
                                            index,
                                            item: agent_result_item(&agent),
                                            selected: false,
                                            on_activate: {
                                                let url = agent.url.clone();
                                                move |_| on_start_agent.call(url.clone())
                                            },
                                            on_hover: move |_| {},
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        PairCard {
                            value: props.pair_value.clone(),
                            error: props.pair_error.clone(),
                            pairing: props.pairing,
                            on_value: props.on_pair_value,
                            on_pair: props.on_pair,
                            on_scan: props.on_scan,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub(crate) struct PairCardProps {
    pub(crate) value: String,
    pub(crate) error: String,
    pub(crate) pairing: bool,
    pub(crate) on_value: EventHandler<String>,
    pub(crate) on_pair: EventHandler<()>,
    pub(crate) on_scan: EventHandler<()>,
}

#[component]
pub(crate) fn PairCard(props: PairCardProps) -> Element {
    let mut show_link = use_signal(|| !props.value.trim().is_empty());
    let unavailable = use_hook(|| match qr_scanner::ScannerSupport::detect() {
        qr_scanner::ScannerSupport::Available => None,
        qr_scanner::ScannerSupport::Unavailable(reason) => Some(reason),
    });

    rsx! {
        div { class: "w-full",
            div { class: "mb-5 text-center",
                h2 { class: "text-base font-semibold text-foreground", {translate("mobile-pair-title")} }
                p { class: "mt-1 text-xs leading-5 text-muted-foreground", {translate("mobile-pair-subtitle")} }
            }
            button {
                class: "flex h-14 w-full items-center justify-center gap-2.5 rounded-2xl bg-primary text-sm font-semibold text-primary-foreground shadow-xl shadow-black/20 disabled:pointer-events-none disabled:opacity-40 disabled:shadow-none active:scale-[0.99] active:bg-primary/90",
                r#type: "button",
                disabled: unavailable.is_some(),
                onclick: move |_| props.on_scan.call(()),
                svg {
                    class: "h-5 w-5",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 5a2 2 0 0 1 2-2h2" }
                    path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                    path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                    path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                    rect { width: "5", height: "5", x: "7", y: "7", rx: "1" }
                    path { d: "M17 7v.01" }
                    path { d: "M17 12v5" }
                    path { d: "M12 17h5" }
                }
                {translate("mobile-pair-scan")}
            }
            button {
                class: "mx-auto mt-4 block rounded-lg px-3 py-2 text-xs font-medium text-muted-foreground active:bg-accent active:text-accent-foreground",
                r#type: "button",
                onclick: move |_| show_link.set(!show_link()),
                {if show_link() { translate("mobile-pair-hide-link") } else { translate("mobile-pair-show-link") }}
            }
            if let Some(reason) = unavailable.clone() {
                p { class: "mt-3 text-center text-xs leading-5 text-muted-foreground", "{reason}" }
            }
            if show_link() {
                form {
                    class: "mt-2 flex items-center gap-2 rounded-2xl border border-border bg-muted p-1.5",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-10 min-w-0 flex-1 bg-transparent px-3 font-mono text-base text-foreground outline-none placeholder:text-muted-foreground",
                        r#type: "url",
                        inputmode: "url",
                        autocomplete: "off",
                        autocapitalize: "none",
                        placeholder: translate("mobile-pair-link-placeholder"),
                        value: "{props.value}",
                        oninput: move |event| props.on_value.call(event.value()),
                    }
                    button {
                        class: "h-10 shrink-0 rounded-xl bg-secondary px-4 text-xs font-semibold text-secondary-foreground disabled:opacity-50 active:bg-secondary/80",
                        r#type: "submit",
                        disabled: props.pairing,
                        {if props.pairing { translate("mobile-pair-connecting") } else { translate("mobile-pair-connect") }}
                    }
                }
            }
            if !props.error.is_empty() {
                p { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
            }
        }
    }
}

/// Present an installed agent as a launcher result, matching the desktop's agent rows.
pub(crate) fn agent_result_item(agent: &RemoteAgent) -> CommandBarResultItem {
    CommandBarResultItem::Page {
        url: agent.url.clone(),
        title: agent.name.clone(),
        icon: if agent.icon.is_empty() {
            PageIcon::None
        } else {
            PageIcon::Favicon(agent.icon.clone())
        },
        shortcut: String::new(),
        prompt_target: true,
    }
}

/// Present a relayed session as a launcher result, so the phone and the desktop draw the same row.
pub(crate) fn session_result_item(session: &RemoteSession) -> CommandBarResultItem {
    let mut location = format!(
        "{} \u{b7} {}",
        session.runtime,
        FilePath(&session.cwd).name()
    );
    if let Some(model) = session.model.as_deref() {
        location.push_str(" \u{b7} ");
        location.push_str(model);
    }
    CommandBarResultItem::Stack {
        title: if session.title.is_empty() {
            session.name.clone()
        } else {
            session.title.clone()
        },
        url: format!("vmux://agent/{}", session.sid),
        icon: PageIcon::default(),
        pane_id: 0,
        tab_index: 0,
        location,
    }
}
