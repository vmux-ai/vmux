use dioxus::prelude::*;

#[component]
pub fn Coworking() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-10 items-center",
                div { class: "animate-fade-up supports-[animation-timeline:view()]:[animation-timeline:view()] supports-[animation-timeline:view()]:[animation-range:entry_0px_cover_40%] motion-reduce:animate-none",
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
                div { class: "rounded-2xl border border-white/10 bg-white/5 backdrop-blur p-8",
                    div { class: "flex items-center justify-between text-xs text-text-muted mb-3",
                        span { "Hands-on pairing" }
                        span { "Full autonomy" }
                    }
                    div { class: "relative h-2 rounded-full bg-border overflow-hidden",
                        div { class: "absolute inset-y-0 left-0 w-1/2 rounded-full bg-gradient-to-r from-accent to-aurora-violet" }
                        div { class: "absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 h-5 w-5 rounded-full bg-accent shadow-lg shadow-accent/40 animate-float motion-reduce:animate-none" }
                    }
                    p { class: "mt-4 text-sm text-text-muted",
                        "Watch a run and grab the keyboard to steer, or turn agents loose in their own space."
                    }
                }
            }
        }
    }
}
