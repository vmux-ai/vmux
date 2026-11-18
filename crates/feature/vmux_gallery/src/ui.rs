#![allow(non_snake_case)]

use crate::{GalleryUiState, RegisteredStory, Story};
use controls::StoryControls;
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerPage, ManagerSearch, ManagerSearchRow, ManagerSidebarItem, WorkspaceLayout,
};
use vmux_ui::components::page::PageContent;
use vmux_ui::components::page_address::PageBreadcrumbs;
use vmux_ui::hooks::{use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::navigation::PageNavigation;

mod controls;
pub(crate) mod icons;
pub(crate) mod previews;

#[vmux_page::page(
    component = Page,
    states = [vmux_api::address::PageAddressUiState, vmux_ui::theme::ThemeUiState, crate::GalleryUiState],
    subtree
)]
pub(crate) struct GalleryPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let binding = use_ui_state::<GalleryUiState>();
    let state = binding.state;
    let mut filter = use_signal(String::new);
    let current = state();
    let query = filter().to_ascii_lowercase();
    rsx! {
        document::Title { "{current.story.name()}" }
        ManagerPage {
            PageBreadcrumbs { state: current.address.clone() }
            WorkspaceLayout {
                resizable: true,
                search: rsx! { ManagerSearchRow {
                    ManagerSearch {
                        value: filter(), placeholder: translate("gallery-search"),
                        onsearch: move |query| filter.set(query),
                    }
                } },
                sidebar: rsx! {
                    nav { class: "flex flex-col gap-3 pb-3",
                        for &category in Story::CATEGORIES {
                            {
                                let mut stories = Vec::new();
                                for story in Story::ALL {
                                    let label = if story == Story::Icon { translate("gallery-all-icons") } else { story.name().to_string() };
                                    if story.category() == category && (story.component_names().to_ascii_lowercase().contains(&query) || label.to_ascii_lowercase().contains(&query) || translate(category).to_ascii_lowercase().contains(&query)) {
                                        stories.push(story);
                                    }
                                }
                                rsx! {
                                    if !stories.is_empty() {
                                        details { key: "{category}", class: "group", open: true,
                                            summary { class: "flex cursor-pointer list-none items-center gap-1.5 px-2 py-1.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/70 [&::-webkit-details-marker]:hidden",
                                                span { class: "inline-block text-[8px] transition-transform group-open:rotate-90", "›" }
                                                {translate(category)}
                                                span { class: "ml-auto font-mono text-[9px] tabular-nums text-muted-foreground/40", "{stories.len()}" }
                                            }
                                            div { class: "flex flex-col gap-0.5",
                                                for story in stories {
                                                    ManagerSidebarItem {
                                                        key: "{story.slug()}",
                                                        label: if story == Story::Icon { translate("gallery-all-icons") } else { story.name().to_string() },
                                                        count: 0, show_count: false,
                                                        selected: story == current.story,
                                                        onclick: move |event| PageNavigation::click(story.url(), &event),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                PageContent { content_class: "@container/gallery mx-auto w-full max-w-6xl",
                    header { class: "mb-6 space-y-2",
                        p { class: "text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60", {translate(current.story.category())} }
                        h1 { class: "text-xl font-medium tracking-tight", if current.story == Story::Icon { {translate("gallery-all-icons")} } else { "{current.story.name()}" } }
                        p { class: "font-mono text-[11px] leading-5 text-muted-foreground/60", {current.story.component_names()} }
                    }
                    div { class: if current.story == Story::Icon { "grid gap-6" } else { "grid items-start gap-6 @3xl/gallery:grid-cols-[minmax(0,1fr)_16rem]" },
                        main { class: "min-w-0 space-y-4",
                            if current.story != Story::Icon {
                                div { class: "flex items-center justify-between text-xs text-muted-foreground",
                                    span { {translate("gallery-preview")} }
                                    span { class: "text-[10px]", {translate("gallery-description")} }
                                }
                            }
                            section { class: if current.story == Story::Icon { "min-w-0" } else { "min-w-0 rounded-xl bg-foreground/[0.025] p-5" },
                                aria_label: translate("gallery-preview"),
                                div { class: "w-full min-w-0", RegisteredStory { key: "{current.revision}", state: current.clone() } }
                            }
                            details { class: "rounded-lg bg-foreground/[0.015] px-3 py-2 text-xs text-muted-foreground",
                                summary { class: "flex cursor-pointer items-center justify-between gap-3 font-medium",
                                    {translate("gallery-events")}
                                    span { class: "font-mono text-[10px] font-normal tabular-nums", "{current.interaction_count}" }
                                }
                                div { class: "space-y-3 pt-3",
                                    p { class: "text-[11px]", {translate("gallery-events-description")} }
                                    if let Some(error) = (binding.error)() {
                                        pre { role: "alert", class: "whitespace-pre-wrap text-destructive", "{error}" }
                                    }
                                    output { class: "block overflow-x-auto rounded-md bg-foreground/[0.035] p-3 font-mono text-[11px]", aria_live: "polite",
                                        if current.interaction_count == 0 { {translate("gallery-no-events")} }
                                        else { "#{current.interaction_count} {current.last_event}" }
                                    }
                                    pre { class: "overflow-x-auto whitespace-pre-wrap font-mono text-[10px] leading-5",
                                        "disabled: {current.disabled}\nchecked: {current.checked}\ntext: {current.text:?}\nchoice: {current.choice:?}\nmenu: {current.menu:?}\nagent: {current.agent:?}\nmodel: {current.model:?}\neffort: {current.effort:?}\npermission: {current.permission:?}\nproject: {current.project:?}\nbranch: {current.branch:?}\nrunning: {current.running}\nsubmitted: {current.submitted:?}"
                                    }
                                }
                            }
                        }
                        if current.story != Story::Icon {
                            aside { class: "min-w-0 @3xl/gallery:sticky @3xl/gallery:top-0", StoryControls { state: current.clone() } }
                        }
                    }
                }
            }
        }
    }
}
