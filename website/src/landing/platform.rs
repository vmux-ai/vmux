use dioxus::prelude::*;

#[component]
pub fn Platform() -> Element {
    rsx! {
        section { class: "relative overflow-hidden max-w-5xl mx-auto px-6 py-24 sm:py-32 text-center",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-80 w-80 -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-violet/15 blur-[120px]" }
            }
            p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Platform" }
            h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4 max-w-2xl mx-auto",
                "More OS than app."
            }
            p { class: "text-text-muted leading-relaxed max-w-2xl mx-auto mb-14",
                "An OS-like layer for everything you do — the same workspace and agents, reshaped to the device in front of you."
            }
            div { class: "flex items-end justify-center gap-4 sm:gap-8",
                div { class: "h-40 w-56 rounded-xl border border-white/10 bg-white/5 backdrop-blur animate-float motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "Desktop" } }
                div { class: "h-52 w-32 rounded-2xl border border-white/10 bg-white/5 backdrop-blur animate-float [animation-delay:-5s] motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "Phone" } }
                div { class: "h-36 w-44 rounded-xl border border-white/10 bg-white/5 backdrop-blur animate-float [animation-delay:-9s] motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "AR / VR" } }
            }
            p { class: "mt-12 text-sm text-text-muted",
                "Today it runs on macOS (lead) and Linux — with a portable core ready to follow."
            }
        }
    }
}
