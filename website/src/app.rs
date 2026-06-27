use dioxus::prelude::*;
use dioxus_primitives::toast::ToastProvider;

use crate::api::{ApiCrate, ApiIndex, ApiItem};
use crate::{docs, landing, markdown};

const SEO_TITLE: &str = "Vmux — One prompt. Anything, done.";
const SEO_DESCRIPTION: &str = "The browser + IDE that get sh*t done — booking a flight, building a website, opening a PR, all handled by your agents while you watch.";
const SITE_URL: &str = "https://vmux.ai/";
const OG_IMAGE: &str = "https://vmux.ai/og.png";
const OG_IMAGE_ALT: &str = "Vmux — One prompt. Anything, done.";

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/_home")]
    HomeStatic {},
    #[layout(DocsLayout)]
        #[route("/docs")]
        DocsIndex {},
        #[route("/docs/api")]
        ApiIndex {},
        #[route("/docs/api/:crate_name")]
        ApiCrate { crate_name: String },
        #[route("/docs/api/:crate_name/:..path")]
        ApiItem { crate_name: String, path: Vec<String> },
        #[route("/docs/:slug")]
        DocPage { slug: String },
}

#[server(endpoint = "static_routes", output = server_fn::codec::Json)]
async fn static_routes() -> Result<Vec<String>, ServerFnError> {
    let mut routes = vec!["/".to_string(), "/_home".to_string(), "/docs".to_string()];
    routes.extend(docs::DOCS.iter().map(|d| format!("/docs/{}", d.slug)));
    routes.push("/docs/api".to_string());
    if let Some(idx) = crate::api::data::index().await {
        routes.extend(idx.crates.iter().map(|c| format!("/docs/api/{}", c.name)));
    }
    Ok(routes)
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Meta { charset: "UTF-8" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta { name: "description", content: SEO_DESCRIPTION }
        document::Meta {
            name: "keywords",
            content: "vmux, agentic browser, AI coding agents, agent workspace, browser IDE, coding agents, MCP, tmux, vibe coding",
        }
        document::Meta { property: "og:type", content: "website" }
        document::Meta { property: "og:site_name", content: "Vmux" }
        document::Meta { property: "og:title", content: SEO_TITLE }
        document::Meta { property: "og:description", content: SEO_DESCRIPTION }
        document::Meta { property: "og:image", content: OG_IMAGE }
        document::Meta { property: "og:image:width", content: "1200" }
        document::Meta { property: "og:image:height", content: "630" }
        document::Meta { property: "og:image:alt", content: OG_IMAGE_ALT }
        document::Meta { name: "twitter:card", content: "summary_large_image" }
        document::Meta { name: "twitter:title", content: SEO_TITLE }
        document::Meta { name: "twitter:description", content: SEO_DESCRIPTION }
        document::Meta { name: "twitter:image", content: OG_IMAGE }
        document::Meta { name: "twitter:image:alt", content: OG_IMAGE_ALT }
        document::Title { "{SEO_TITLE}" }
        document::Stylesheet { href: "/style.css" }
        ToastProvider {
            Router::<Route> {}
        }
    }
}

#[component]
fn Home() -> Element {
    rsx! {
        document::Link { rel: "canonical", href: SITE_URL }
        document::Meta { property: "og:url", content: SITE_URL }
        landing::Landing {}
    }
}

#[component]
fn HomeStatic() -> Element {
    rsx! {
        document::Link { rel: "canonical", href: SITE_URL }
        document::Meta { property: "og:url", content: SITE_URL }
        landing::Landing {}
    }
}

#[component]
fn DocsLayout() -> Element {
    let route = use_route::<Route>();
    let active_slug = match route {
        Route::DocPage { slug } => slug,
        Route::DocsIndex {} => "experience".to_string(),
        _ => String::new(),
    };
    let active_heading = use_signal(String::new);

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spy::setup(active_heading);
    });

    rsx! {
        div { class: "h-screen flex flex-col overflow-hidden",
            header { class: "shrink-0 flex items-center gap-3 px-6 py-3 border-b border-border",
                Link {
                    class: "font-bold tracking-tight text-text hover:text-accent no-underline",
                    to: Route::Home {},
                    "Vmux"
                }
                span { class: "text-text-muted text-sm", "/ Docs" }
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
                    if docs::DOCS[i].slug == active_slug && docs::DOCS[i].slug != "architecture" {
                        {toc(docs::DOCS[i].content, active_heading)}
                    }
                }
            }
        }
        div { class: "mb-4",
            div { class: "px-3 mb-1 text-xs uppercase tracking-wide text-text-muted", "Reference" }
            Link {
                class: "block px-3 py-1.5 rounded-md text-sm text-text no-underline hover:bg-surface",
                active_class: "bg-surface text-accent",
                to: Route::ApiIndex {},
                "API Reference"
            }
        }
    }
}

fn toc(content: &str, active_heading: Signal<String>) -> Element {
    let hs = markdown::headings(content);
    if hs.is_empty() {
        return rsx! {};
    }
    let current = active_heading();
    let effective = if hs.iter().any(|h| h.id == current) {
        current
    } else {
        hs[0].id.clone()
    };
    rsx! {
        div { class: "mt-1 mb-2 ml-3 border-l border-border",
            for h in hs.iter() {
                {toc_item(h, &effective)}
            }
        }
    }
}

fn toc_item(h: &markdown::Heading, effective: &str) -> Element {
    let indent = if h.level >= 3 { "pl-6" } else { "pl-3" };
    let color = if h.id == effective {
        "text-accent"
    } else {
        "text-text-muted hover:text-text"
    };
    let class = format!("block py-0.5 text-xs no-underline {indent} {color}");
    rsx! {
        a { class: "{class}", href: "#{h.id}", "{h.text}" }
    }
}

#[cfg(target_arch = "wasm32")]
mod spy {
    use dioxus::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::prelude::Closure;

    pub fn setup(mut active: Signal<String>) {
        let Some(win) = web_sys::window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        let Some(main) = doc.get_element_by_id("doc-main") else {
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
            for i in 0..list.length() {
                if let Some(el) = list
                    .item(i)
                    .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
                {
                    if el.get_bounding_client_rect().top() <= line {
                        current = el.id();
                    }
                }
            }
            let at_bottom = f64::from(scope.scroll_top()) + f64::from(scope.client_height())
                >= f64::from(scope.scroll_height()) - 8.0;
            if at_bottom && list.length() > 0 {
                if let Some(el) = list
                    .item(list.length() - 1)
                    .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
                {
                    current = el.id();
                }
            }
            if current.is_empty() || *active.peek() == current {
                return;
            }
            active.set(current.clone());
            if let Ok(history) = win.history() {
                let _ = history.replace_state_with_url(
                    &JsValue::NULL,
                    "",
                    Some(&format!("#{current}")),
                );
            }
        };
        update();
        let cb = Closure::<dyn FnMut()>::new(update);
        let _ = main.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
        cb.forget();
    }
}

#[component]
fn DocsIndex() -> Element {
    use_effect(|| {
        #[cfg(target_arch = "wasm32")]
        scroll_doc_top();
    });
    doc_body("experience")
}

#[component]
fn DocPage(slug: String) -> Element {
    let s = slug.clone();
    use_effect(use_reactive!(|s| {
        let _ = &s;
        #[cfg(target_arch = "wasm32")]
        scroll_doc_top();
    }));
    doc_body(&slug)
}

#[cfg(target_arch = "wasm32")]
fn scroll_doc_top() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("doc-main"))
    {
        el.set_scroll_top(0);
    }
}

fn doc_body(slug: &str) -> Element {
    match docs::find(slug) {
        Some(d) => {
            let (prev, next) = docs::neighbors(d.slug);
            rsx! {
                markdown::Markdown { content: d.content.to_string() }
                nav { class: "mt-16 grid grid-cols-2 gap-4 border-t border-border pt-8",
                    if let Some(p) = prev {
                        Link {
                            class: "group flex flex-col gap-1 rounded-lg border border-border px-4 py-3 no-underline transition-colors hover:border-accent",
                            to: Route::DocPage { slug: p.slug.to_string() },
                            span { class: "text-xs text-text-muted", "← Previous" }
                            span { class: "text-sm font-medium text-text group-hover:text-accent", "{p.title}" }
                        }
                    } else {
                        span {}
                    }
                    if let Some(n) = next {
                        Link {
                            class: "group col-start-2 flex flex-col items-end gap-1 rounded-lg border border-border px-4 py-3 text-right no-underline transition-colors hover:border-accent",
                            to: Route::DocPage { slug: n.slug.to_string() },
                            span { class: "text-xs text-text-muted", "Next →" }
                            span { class: "text-sm font-medium text-text group-hover:text-accent", "{n.title}" }
                        }
                    }
                }
            }
        }
        None => rsx! {
            div { class: "py-12 text-center text-text-muted",
                h1 { class: "text-2xl font-bold text-text mb-2", "Not found" }
                p { class: "mb-4", "No doc named \"{slug}\"." }
                Link { class: "text-accent underline", to: Route::DocsIndex {}, "Back to docs" }
            }
        },
    }
}
