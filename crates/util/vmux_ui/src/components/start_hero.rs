use dioxus::prelude::*;

use crate::i18n::translate;
use crate::util::cn;

pub const START_BACKDROP_CLASS: &str =
    "bg-[radial-gradient(140%_100%_at_50%_-12%,rgba(129,140,248,0.05),transparent_55%)]";

#[component]
pub fn StartBackdrop() -> Element {
    rsx! {
        div { class: "pointer-events-none absolute inset-0 -z-10 hidden overflow-hidden md:block",
            div { class: "absolute left-1/2 top-[16%] h-[36rem] w-[36rem] -translate-x-1/2 rounded-full blur-[150px] dark:bg-indigo-500/15" }
            div { class: "absolute left-[12%] top-1/3 h-80 w-80 rounded-full blur-[130px] dark:bg-primary/10" }
            div { class: "absolute right-[12%] top-1/4 h-80 w-80 rounded-full blur-[130px] dark:bg-primary/12" }
            div { class: "absolute inset-x-0 bottom-0 h-1/3 bg-gradient-to-t from-transparent to-transparent dark:from-black/40" }
        }
    }
}

#[component]
pub fn StartHero(
    #[props(default = true)] revealed: bool,
    #[props(default)] mark: Option<Element>,
    children: Element,
) -> Element {
    let reveal = if revealed {
        "opacity-100"
    } else {
        "opacity-0 blur-sm translate-y-4"
    };
    let class = cn([
        "relative z-10 mx-auto flex w-full max-w-md flex-col items-center gap-6 transition-all duration-700 ease-out motion-reduce:transition-none md:max-w-3xl md:gap-8",
        reveal,
    ]);

    rsx! {
        div { class,
            div { class: "flex flex-col items-center gap-2",
                if let Some(mark) = mark {
                    {mark}
                }
                h1 { class: "bg-gradient-to-b from-foreground to-foreground/55 bg-clip-text text-4xl font-semibold leading-none tracking-tight text-transparent sm:text-5xl md:text-6xl",
                    "vmux"
                }
                p { class: "text-sm text-muted-foreground md:text-base", {translate("start-tagline")} }
            }
            {children}
        }
    }
}
