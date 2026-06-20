mod docs;
mod hooks;
mod markdown;

use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, ToastProvider, use_toast};

use hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};

const ICON: Asset = asset!("/assets/icon.png");
const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Home {},
    #[route("/_home")]
    HomeStatic {},
    #[layout(DocsLayout)]
        #[route("/docs")]
        DocsIndex {},
        #[route("/docs/:slug")]
        DocPage { slug: String },
}

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(server_only! {
            ServeConfig::builder()
                .incremental(
                    dioxus::server::IncrementalRendererConfig::new()
                        .static_dir(
                            std::env::current_exe()
                                .unwrap()
                                .parent()
                                .unwrap()
                                .join("public"),
                        )
                        .clear_cache(false),
                )
                .enable_out_of_order_streaming()
        })
        .launch(App);
}

#[server(endpoint = "static_routes", output = server_fn::codec::Json)]
async fn static_routes() -> Result<Vec<String>, ServerFnError> {
    let mut routes = vec!["/".to_string(), "/_home".to_string(), "/docs".to_string()];
    routes.extend(docs::DOCS.iter().map(|d| format!("/docs/{}", d.slug)));
    Ok(routes)
}

#[component]
fn App() -> Element {
    rsx! {
        document::Meta { charset: "UTF-8" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta {
            name: "description",
            content: "An agent-first workspace with a browser and IDE built in — co-work with agents in one shared space.",
        }
        document::Title { "Vmux — agent-first workspace" }
        document::Stylesheet { href: "/style.css" }
        ToastProvider {
            Router::<Route> {}
        }
    }
}

#[component]
fn Home() -> Element {
    rsx! {
        Hero {}
        Features {}
        Footer {}
    }
}

#[component]
fn HomeStatic() -> Element {
    rsx! {
        Hero {}
        Features {}
        Footer {}
    }
}

#[component]
fn Hero() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section { class: "text-center max-w-3xl mx-auto pt-16 pb-12 px-6 sm:pt-24 sm:pb-16 sm:px-8",
            img {
                src: ICON,
                alt: "Vmux icon",
                class: "w-32 h-32 mb-6 inline-block rounded-3xl",
            }
            h1 { class: "text-4xl sm:text-5xl font-bold mb-2 tracking-tight", "Vmux" }
            p { class: "text-base sm:text-xl text-text-muted mb-10 max-w-xl mx-auto",
                "Vibe Multiplexer — an agent-first workspace with a browser and IDE built in."
            }
            div { class: "flex flex-wrap justify-center gap-3 mb-6",
                button {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold border border-transparent bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        if is_mac {
                            download(());
                        } else {
                            toast_api
                                .info(
                                    "Not supported".to_string(),
                                    ToastOptions::new()
                                        .description("Windows/Linux not supported yet — see GitHub Releases"),
                                );
                        }
                    },
                    "Download .dmg"
                }
                a {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                    href: GITHUB_URL,
                    target: "_blank",
                    "GitHub"
                }
                Link {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                    to: Route::DocsIndex {},
                    "Docs"
                }
            }
            div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-4",
                code { class: "font-mono text-accent", "{INSTALL_CMD}" }
                button {
                    class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        copy(INSTALL_CMD.to_string());
                        toast_api.success("Copied!".to_string(), ToastOptions::new());
                    },
                    "Copy"
                }
            }
            p { class: "text-sm text-text-muted", "Requires macOS 13.0 (Ventura) or later." }
        }
    }
}

#[component]
fn Features() -> Element {
    let features = [
        (
            "Co-work with agents",
            "People and agents build side by side in one shared space — from hands-on pairing to full autonomy, you set the balance.",
        ),
        (
            "Browser simplicity, tmux power",
            "Looks like the browser you already know; split, stack, and tile panes like tmux underneath.",
        ),
        (
            "IDE power underneath",
            "Keyboard-driven workflows and deep environment control — and agents drive the whole workspace over MCP.",
        ),
        (
            "3D workspace",
            "Powered by Bevy. Flip your panes into a live, GPU-rendered 3D scene — same workspace, still interactive.",
        ),
    ];

    rsx! {
        section { class: "max-w-3xl mx-auto py-12 px-8",
            h2 { class: "text-center text-3xl mb-8", "Features" }
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-5",
                for (title , desc) in features {
                    div { class: "bg-surface border border-border rounded-xl p-6",
                        h3 { class: "text-base mb-2 text-accent", "{title}" }
                        p { class: "text-sm text-text-muted leading-relaxed", "{desc}" }
                    }
                }
            }
        }
    }
}

#[component]
fn Footer() -> Element {
    rsx! {
        footer { class: "text-center py-12 px-8 text-text-muted text-sm",
            p {
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: GITHUB_URL,
                    target: "_blank",
                    "GitHub"
                }
                " · "
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: "https://github.com/vmux-ai/vmux/blob/main/LICENSE",
                    target: "_blank",
                    "MIT License"
                }
            }
        }
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
            let top = scope.get_bounding_client_rect().top();
            let mut current = String::new();
            for i in 0..list.length() {
                if let Some(node) = list.item(i) {
                    if let Some(el) = node.dyn_ref::<web_sys::Element>() {
                        if el.get_bounding_client_rect().top() - top <= 96.0 {
                            current = el.id();
                        }
                    }
                }
            }
            let changed = *active.peek() != current;
            if !current.is_empty() && changed {
                active.set(current);
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
    doc_body("experience")
}

#[component]
fn DocPage(slug: String) -> Element {
    doc_body(&slug)
}

fn doc_body(slug: &str) -> Element {
    match docs::find(slug) {
        Some(d) => rsx! {
            markdown::Markdown { content: d.content.to_string() }
        },
        None => rsx! {
            div { class: "py-12 text-center text-text-muted",
                h1 { class: "text-2xl font-bold text-text mb-2", "Not found" }
                p { class: "mb-4", "No doc named \"{slug}\"." }
                Link { class: "text-accent underline", to: Route::DocsIndex {}, "Back to docs" }
            }
        },
    }
}
