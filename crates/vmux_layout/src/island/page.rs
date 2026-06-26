use dioxus::prelude::*;
use vmux_command::island::{ISLAND_RENDER_EVENT, IslandRenderEvent, IslandState};
use vmux_ui::hooks::{use_bin_event_listener, use_theme};

use crate::command_bar::page::Page as CommandBarPage;

pub const ISLAND_IDLE_CLASS: &str =
    "inline-flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm text-foreground";
const ISLAND_ACTIVITY_CLASS: &str =
    "inline-flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm text-foreground";

/// Island web page. Transparent body (the native `NSGlassEffectView` is the backdrop). Subscribes
/// to `ISLAND_RENDER_EVENT` and morphs between states; the Search state reuses the command bar.
#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(|| IslandState::Idle);
    let _island_listener =
        use_bin_event_listener::<IslandRenderEvent, _>(ISLAND_RENDER_EVENT, move |data| {
            state.set(data.state);
        });

    let content = match state.read().clone() {
        IslandState::Idle => rsx! {
            div { class: ISLAND_IDLE_CLASS, span { "vmux" } }
        },
        IslandState::Search => rsx! { CommandBarPage {} },
        IslandState::Activity(activity) => {
            let label = activity.label.clone();
            rsx! {
                div { class: ISLAND_ACTIVITY_CLASS,
                    span { class: "h-2 w-2 rounded-full bg-sidebar-primary" }
                    span { "{label}" }
                }
            }
        }
        IslandState::Notify(notice) => {
            let label = notice.label.clone();
            rsx! {
                div { class: ISLAND_ACTIVITY_CLASS, span { "{label}" } }
            }
        }
    };

    rsx! {
        div { class: "flex items-center justify-center bg-transparent", {content} }
    }
}
