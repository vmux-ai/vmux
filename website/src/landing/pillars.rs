use dioxus::prelude::*;

struct Pillar {
    title: &'static str,
    body: &'static str,
}

const PILLARS: &[Pillar] = &[
    Pillar {
        title: "Co-working",
        body: "People and agents work in one shared space — from hands-on pairing to full autonomy. Watch a run and grab the keyboard, or turn agents loose.",
    },
    Pillar {
        title: "Known by heart",
        body: "It looks and acts like a standard web browser. No learning curve — everyone already knows how to use it.",
    },
    Pillar {
        title: "IDE power",
        body: "Beneath the surface: advanced tools, keyboard-driven workflows, and deep environment control for when you want it.",
    },
];

#[component]
pub fn Pillars() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            p { class: "text-center text-sm uppercase tracking-[0.2em] text-accent mb-3",
                "Two worlds, one workspace"
            }
            h2 { class: "text-center text-3xl sm:text-4xl font-bold tracking-tight mb-14 max-w-2xl mx-auto",
                "Vmux bridges chat-first tools and developer IDEs."
            }
            div { class: "grid grid-cols-1 md:grid-cols-3 gap-5",
                for (i , p) in PILLARS.iter().enumerate() {
                    div {
                        class: "rounded-2xl border border-white/10 bg-white/5 backdrop-blur p-7 animate-fade-up supports-[animation-timeline:view()]:[animation-timeline:view()] supports-[animation-timeline:view()]:[animation-range:entry_0%_cover_35%] motion-reduce:animate-none",
                        style: "animation-delay: {i * 120}ms",
                        h3 { class: "text-lg font-semibold text-accent mb-2", "{p.title}" }
                        p { class: "text-sm text-text-muted leading-relaxed", "{p.body}" }
                    }
                }
            }
        }
    }
}
