#![allow(non_snake_case)]

use crate::{GalleryDisabledRequest, GalleryResetRequest, GalleryUiState, RegisteredStory, Story};
use controls::StoryControls;
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerPage, ManagerSearch, ManagerSearchRow, ManagerSidebarItem, WorkspaceLayout,
};
use vmux_ui::components::page::PageContent;
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::navigation::PageNavigation;

mod controls;
pub(crate) mod previews;

#[vmux_page::page(
    component = Page,
    states = ["PageAddressUiState", "ThemeUiState", "GalleryUiState"],
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
    let mut toolbar = Vec::new();
    toolbar.push(ToolbarControl::button(
        translate("gallery-reset"),
        move |_| {
            let _ = send(&GalleryResetRequest);
        },
    ));
    let disabled = current.disabled;
    toolbar.push(
        ToolbarControl::button(translate("gallery-disabled"), move |_| {
            let _ = send(&GalleryDisabledRequest {
                disabled: !disabled,
            });
        })
        .active(disabled),
    );
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
                    nav { class: "flex flex-col gap-0.5",
                        for story in Story::ALL {
                            if story.component_names().to_ascii_lowercase().contains(&filter().to_ascii_lowercase()) {
                                ManagerSidebarItem {
                                    key: "{story.slug()}",
                                    label: story.name().to_string(),
                                    count: 0, show_count: false,
                                    selected: story == current.story,
                                    onclick: move |event| PageNavigation::click(story.url(), &event),
                                }
                            }
                        }
                    }
                },
                toolbar: rsx! { PageToolbar { controls: toolbar } },
                PageContent { content_class: "mx-auto w-full max-w-5xl",
                    div { class: "mb-5 flex flex-wrap items-center justify-between gap-3",
                        h1 { class: "font-mono text-base font-semibold", "{current.story.name()}" }
                        span { class: "text-xs text-muted-foreground", {translate("gallery-description")} }
                    }
                    p { class: "mb-3 font-mono text-[11px] text-muted-foreground", {current.story.component_names()} }
                    StoryControls { state: current.clone() }
                    section { class: "min-h-64 rounded-lg bg-foreground/[0.025] p-4",
                        aria_label: translate("gallery-preview"),
                        RegisteredStory { key: "{current.revision}", state: current.clone() }
                    }
                    details { class: "mt-6 text-xs text-muted-foreground", open: true,
                        summary { class: "mb-3 cursor-pointer font-medium", {translate("gallery-events")} }
                        p { class: "mb-3", {translate("gallery-events-description")} }
                        if let Some(error) = (binding.error)() {
                            pre { role: "alert", class: "whitespace-pre-wrap text-destructive", "{error}" }
                        }
                        output { class: "block overflow-x-auto rounded-lg bg-foreground/[0.035] p-3 font-mono", aria_live: "polite",
                            if current.interaction_count == 0 { {translate("gallery-no-events")} }
                            else { "#{current.interaction_count} {current.last_event}" }
                        }
                        pre { class: "mt-3 overflow-x-auto whitespace-pre-wrap font-mono",
                            "disabled: {current.disabled}\nchecked: {current.checked}\ntext: {current.text:?}\nchoice: {current.choice:?}\nmenu: {current.menu:?}\nagent: {current.agent:?}\nmodel: {current.model:?}\neffort: {current.effort:?}\npermission: {current.permission:?}\nproject: {current.project:?}\nbranch: {current.branch:?}\nrunning: {current.running}\nsubmitted: {current.submitted:?}"
                        }
                    }
                }
            }
        }
    }
}
