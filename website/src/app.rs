use dioxus::prelude::*;
use dioxus_primitives::toast::ToastProvider;

use crate::{docs, framework, landing, markdown, use_cases};

const PRODUCT_TITLE: &str = "Vmux — One prompt. Anything, done.";
const PRODUCT_DESCRIPTION: &str =
    "The browser that gets sh*t done. Work with your team and any ACP agent with full IDE support.";
const USE_CASES_TITLE: &str = "Vmux Use Cases — Start with the browser. Finish the task.";
const USE_CASES_DESCRIPTION: &str = "Research, build, review, and keep work moving with your team, any ACP agent, and a full IDE in one browser.";
const FRAMEWORK_TITLE: &str = "Vmux Framework — One IDE. Every platform.";
const FRAMEWORK_DESCRIPTION: &str = "Build cross-platform applications from typed feature plugins, contracts, manifests, and platform adapters.";
const DOCS_TITLE: &str = "Vmux Architecture";
const DOCS_DESCRIPTION: &str =
    "The stable architecture, ownership, state flow, and trust boundaries behind Vmux.";
const SITE_URL: &str = "https://vmux.ai/";
const USE_CASES_URL: &str = "https://vmux.ai/use-cases";
const FRAMEWORK_URL: &str = "https://vmux.ai/framework";
const DOCS_URL: &str = "https://vmux.ai/docs";
const OG_IMAGE: &str = "https://vmux.ai/og.png";

#[component]
pub fn App() -> Element {
    rsx! {
        document::Meta { charset: "UTF-8" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Stylesheet { href: "/style.css" }
        ToastProvider {
            Router::<Route> {}
        }
    }
}

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/_home")]
    HomeStatic {},
    #[route("/use-cases")]
    UseCasesPage {},
    #[route("/framework")]
    FrameworkPage {},
    #[layout(DocsLayout)]
        #[route("/docs")]
        DocsIndex {},
        #[route("/docs/:slug")]
        DocPage { slug: String },
}

#[server(endpoint = "static_routes", output = server_fn::codec::Json)]
async fn static_routes() -> Result<Vec<String>, ServerFnError> {
    let mut routes = vec![
        "/".to_string(),
        "/_home".to_string(),
        "/use-cases".to_string(),
        "/framework".to_string(),
        "/docs".to_string(),
    ];
    routes.extend(docs::DOCS.iter().map(|doc| format!("/docs/{}", doc.slug)));
    Ok(routes)
}

#[component]
fn Metadata(title: String, description: String, keywords: String, url: String) -> Element {
    rsx! {
        document::Title { "{title}" }
        document::Link { rel: "canonical", href: "{url}" }
        document::Meta { name: "description", content: "{description}" }
        document::Meta { name: "keywords", content: "{keywords}" }
        document::Meta { property: "og:type", content: "website" }
        document::Meta { property: "og:site_name", content: "Vmux" }
        document::Meta { property: "og:url", content: "{url}" }
        document::Meta { property: "og:title", content: "{title}" }
        document::Meta { property: "og:description", content: "{description}" }
        document::Meta { property: "og:image", content: OG_IMAGE }
        document::Meta { property: "og:image:width", content: "1200" }
        document::Meta { property: "og:image:height", content: "630" }
        document::Meta { property: "og:image:alt", content: "{title}" }
        document::Meta { name: "twitter:card", content: "summary_large_image" }
        document::Meta { name: "twitter:title", content: "{title}" }
        document::Meta { name: "twitter:description", content: "{description}" }
        document::Meta { name: "twitter:image", content: OG_IMAGE }
        document::Meta { name: "twitter:image:alt", content: "{title}" }
    }
}

#[component]
fn Home() -> Element {
    rsx! {
        Metadata {
            title: PRODUCT_TITLE.to_string(),
            description: PRODUCT_DESCRIPTION.to_string(),
            keywords: "vmux, browser, ACP agents, agent harness, IDE, coding agents".to_string(),
            url: SITE_URL.to_string(),
        }
        landing::Landing {}
    }
}

#[component]
fn HomeStatic() -> Element {
    rsx! {
        Metadata {
            title: PRODUCT_TITLE.to_string(),
            description: PRODUCT_DESCRIPTION.to_string(),
            keywords: "vmux, browser, ACP agents, agent harness, IDE, coding agents".to_string(),
            url: SITE_URL.to_string(),
        }
        landing::Landing {}
    }
}

#[component]
fn UseCasesPage() -> Element {
    rsx! {
        Metadata {
            title: USE_CASES_TITLE.to_string(),
            description: USE_CASES_DESCRIPTION.to_string(),
            keywords: "vmux use cases, browser, ACP agents, agent harness, IDE, team workspace".to_string(),
            url: USE_CASES_URL.to_string(),
        }
        use_cases::UseCases {}
    }
}

#[component]
fn FrameworkPage() -> Element {
    rsx! {
        Metadata {
            title: FRAMEWORK_TITLE.to_string(),
            description: FRAMEWORK_DESCRIPTION.to_string(),
            keywords: "vmux framework, cross-platform app framework, ACP agents, plugin architecture, Rust".to_string(),
            url: FRAMEWORK_URL.to_string(),
        }
        framework::Framework {}
    }
}

#[component]
fn DocsLayout() -> Element {
    let route = use_route::<Route>();
    let active_slug = match route {
        Route::DocPage { slug } => slug,
        Route::DocsIndex {} => "architecture".to_string(),
        _ => String::new(),
    };
    let active_heading = use_signal(String::new);

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spy::setup(active_heading);
    });

    rsx! {
        Metadata {
            title: DOCS_TITLE.to_string(),
            description: DOCS_DESCRIPTION.to_string(),
            keywords: "vmux architecture, Bevy ECS, feature plugins, typed contracts, ACP".to_string(),
            url: DOCS_URL.to_string(),
        }
        div { class: "h-screen flex flex-col overflow-hidden",
            header { class: "shrink-0 flex items-center gap-3 px-6 py-3 border-b border-border",
                Link {
                    class: "font-bold tracking-tight text-text hover:text-accent no-underline",
                    to: Route::Home {},
                    "Vmux"
                }
                span { class: "text-text-muted text-sm", "/ Docs" }
                Link {
                    class: "ml-auto text-sm text-text-muted no-underline hover:text-text",
                    to: Route::FrameworkPage {},
                    "Framework"
                }
            }
            div { class: "flex-1 min-h-0 flex max-w-6xl mx-auto w-full",
                nav { class: "w-64 shrink-0 border-r border-border overflow-y-auto py-6 px-3 hidden md:block",
                    {sidebar(active_slug.clone(), active_heading)}
                }
                main {
                    id: "doc-main",
                    class: "flex-1 min-w-0 overflow-y-auto px-6 py-8 sm:px-10",
                    article { class: "mx-auto max-w-3xl",
                        Outlet::<Route> {}
                    }
                }
            }
        }
    }
}

fn sidebar(active_slug: String, active_heading: Signal<String>) -> Element {
    rsx! {
        for (group , idxs) in docs::groups() {
            div { class: "mb-4",
                div { class: "px-3 mb-1 text-xs uppercase tracking-wide text-text-muted", "{group}" }
                for i in idxs {
                    Link {
                        class: "block px-3 py-1.5 rounded-md text-sm text-text no-underline hover:bg-surface",
                        active_class: "bg-surface text-accent",
                        to: Route::DocPage { slug: docs::DOCS[i].slug.to_string() },
                        "{docs::DOCS[i].title}"
                    }
                    if docs::DOCS[i].slug == active_slug {
                        {toc(docs::DOCS[i].content, active_heading)}
                    }
                }
            }
        }
    }
}

fn toc(content: &str, active_heading: Signal<String>) -> Element {
    let headings = markdown::headings(content);
    if headings.is_empty() {
        return rsx! {};
    }
    let current = active_heading();
    let effective = if headings.iter().any(|heading| heading.id == current) {
        current
    } else {
        headings[0].id.clone()
    };
    rsx! {
        div { class: "mt-1 mb-2 ml-3 border-l border-border",
            for heading in headings.iter() {
                {toc_item(heading, &effective)}
            }
        }
    }
}

fn toc_item(heading: &markdown::Heading, effective: &str) -> Element {
    let indent = if heading.level >= 3 { "pl-6" } else { "pl-3" };
    let color = if heading.id == effective {
        "text-accent"
    } else {
        "text-text-muted hover:text-text"
    };
    let class = format!("block py-0.5 text-xs no-underline {indent} {color}");
    rsx! {
        a { class: "{class}", href: "#{heading.id}", "{heading.text}" }
    }
}

#[cfg(target_arch = "wasm32")]
mod spy {
    use dioxus::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::prelude::Closure;

    pub fn setup(mut active: Signal<String>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(main) = document.get_element_by_id("doc-main") else {
            return;
        };
        let scope = main.clone();
        let mut update = move || {
            let Ok(list) = scope.query_selector_all("h2[id], h3[id]") else {
                return;
            };
            let rect = scope.get_bounding_client_rect();
            let line = rect.top() + rect.height() * 0.3;
            let mut current = String::new();
            for index in 0..list.length() {
                if let Some(element) = list
                    .item(index)
                    .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
                {
                    if element.get_bounding_client_rect().top() <= line {
                        current = element.id();
                    }
                }
            }
            let at_bottom = f64::from(scope.scroll_top()) + f64::from(scope.client_height())
                >= f64::from(scope.scroll_height()) - 8.0;
            if at_bottom && list.length() > 0 {
                if let Some(element) = list
                    .item(list.length() - 1)
                    .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
                {
                    current = element.id();
                }
            }
            if current.is_empty() || *active.peek() == current {
                return;
            }
            active.set(current.clone());
            if let Ok(history) = window.history() {
                let _ = history.replace_state_with_url(
                    &JsValue::NULL,
                    "",
                    Some(&format!("#{current}")),
                );
            }
        };
        update();
        let callback = Closure::<dyn FnMut()>::new(update);
        let _ = main.add_event_listener_with_callback("scroll", callback.as_ref().unchecked_ref());
        callback.forget();
    }
}

#[component]
fn DocsIndex() -> Element {
    use_effect(|| {
        #[cfg(target_arch = "wasm32")]
        scroll_doc_top();
    });
    doc_body("architecture")
}

#[component]
fn DocPage(slug: String) -> Element {
    let reactive_slug = slug.clone();
    use_effect(use_reactive!(|reactive_slug| {
        let _ = &reactive_slug;
        #[cfg(target_arch = "wasm32")]
        scroll_doc_top();
    }));
    doc_body(&slug)
}

#[cfg(target_arch = "wasm32")]
fn scroll_doc_top() {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("doc-main"))
    {
        element.set_scroll_top(0);
    }
}

fn doc_body(slug: &str) -> Element {
    match docs::find(slug) {
        Some(doc) => {
            let (previous, next) = docs::neighbors(doc.slug);
            rsx! {
                markdown::Markdown { content: doc.content.to_string() }
                nav { class: "mt-16 grid grid-cols-2 gap-4 border-t border-border pt-8",
                    if let Some(previous) = previous {
                        Link {
                            class: "group flex flex-col gap-1 rounded-lg border border-border px-4 py-3 no-underline transition-colors hover:border-accent",
                            to: Route::DocPage { slug: previous.slug.to_string() },
                            span { class: "text-xs text-text-muted", "← Previous" }
                            span { class: "text-sm font-medium text-text group-hover:text-accent", "{previous.title}" }
                        }
                    } else {
                        span {}
                    }
                    if let Some(next) = next {
                        Link {
                            class: "group col-start-2 flex flex-col items-end gap-1 rounded-lg border border-border px-4 py-3 text-right no-underline transition-colors hover:border-accent",
                            to: Route::DocPage { slug: next.slug.to_string() },
                            span { class: "text-xs text-text-muted", "Next →" }
                            span { class: "text-sm font-medium text-text group-hover:text-accent", "{next.title}" }
                        }
                    }
                }
            }
        }
        None => rsx! {
            div { class: "py-12 text-center text-text-muted",
                h1 { class: "mb-2 text-2xl font-bold text-text", "Not found" }
                p { class: "mb-4", "No doc named \"{slug}\"." }
                Link { class: "text-accent underline", to: Route::DocsIndex {}, "Back to docs" }
            }
        },
    }
}
