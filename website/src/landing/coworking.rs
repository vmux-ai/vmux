use dioxus::prelude::*;

#[component]
pub fn Coworking() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-10 items-center",
                div { class: "reveal",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Co-working" }
                    h2 { class: "text-3xl sm:text-4xl font-bold tracking-tight mb-4",
                        "Build alongside your agents."
                    }
                    p { class: "text-text-muted leading-relaxed mb-4",
                        "People and agents work, build, and orchestrate tasks side by side, in real time, in one shared space."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "Find your own balance — and let it shift as you trust agents more."
                    }
                }
                div {
                    class: "rounded-2xl border border-white/10 bg-white/5 backdrop-blur p-8 reveal",
                    style: "transition-delay: 120ms",
                    div { class: "flex items-center gap-4",
                        div { class: "flex flex-col items-center gap-1 shrink-0",
                            div { class: "h-10 w-10 rounded-full bg-gradient-to-br from-accent/30 to-accent/5 border border-accent/40 flex items-center justify-center text-accent",
                                svg {
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    class: "w-5 h-5",
                                    path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                                    circle { cx: "12", cy: "7", r: "4" }
                                }
                            }
                            span { class: "text-[10px] text-text-muted", "You" }
                        }
                        div { class: "flex-1",
                            div { class: "flex items-center justify-between text-xs text-text-muted mb-2",
                                span { "Hands-on pairing" }
                                span { "Full autonomy" }
                            }
                            div { class: "relative h-2 rounded-full bg-border",
                                div { class: "absolute inset-y-0 left-0 w-1/2 rounded-full bg-gradient-to-r from-accent to-aurora-violet animate-slide motion-reduce:animate-none",
                                    div { class: "absolute right-0 top-1/2 -translate-y-1/2 translate-x-1/2 h-5 w-5 rounded-full bg-accent shadow-lg shadow-accent/40" }
                                }
                            }
                        }
                        div { class: "flex flex-col items-center gap-1 shrink-0",
                            div { class: "h-10 w-10 rounded-full bg-gradient-to-br from-aurora-violet/30 to-aurora-cyan/5 border border-aurora-violet/40 flex items-center justify-center text-aurora-violet",
                                svg {
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    class: "w-5 h-5",
                                    path { d: "M12 8V4H8" }
                                    rect { width: "16", height: "12", x: "4", y: "8", rx: "2" }
                                    path { d: "M2 14h2" }
                                    path { d: "M20 14h2" }
                                    path { d: "M15 13v2" }
                                    path { d: "M9 13v2" }
                                }
                            }
                            span { class: "text-[10px] text-text-muted", "Agent" }
                        }
                    }
                    p { class: "mt-4 text-sm text-text-muted",
                        "Watch a run and grab the keyboard to steer, or turn agents loose in their own space."
                    }
                }
            }
        }
    }
}
