pub mod data;
pub mod model;

use dioxus::prelude::*;

use crate::Route;
use crate::markdown::{Markdown, highlight_code};
use model::{CrateDoc, Item, Module};

#[component]
pub fn ApiIndex() -> Element {
    let idx = use_resource(|| async move { data::index().await });
    let r = idx.read_unchecked();
    let body = match &*r {
        Some(Some(i)) => rsx! {
            div { class: "grid gap-3 sm:grid-cols-2",
                for c in i.crates.iter() {
                    Link {
                        class: "block rounded-lg border border-border px-4 py-3 no-underline transition-colors hover:border-accent",
                        to: Route::ApiCrate { crate_name: c.name.clone() },
                        div { class: "font-mono text-sm font-medium text-accent", "{c.name}" }
                        div { class: "text-sm text-text-muted", "{c.blurb_md}" }
                    }
                }
            }
        },
        _ => rsx! { p { class: "text-text-muted", "Loading…" } },
    };
    rsx! {
        h1 { class: "scroll-mt-6 text-3xl sm:text-4xl font-bold tracking-tight mt-4 mb-6", "API Reference" }
        {body}
    }
}

#[component]
pub fn ApiCrate(crate_name: String) -> Element {
    let name = crate_name.clone();
    let doc = use_resource(move || {
        let name = name.clone();
        async move { data::crate_doc(&name).await }
    });
    let r = doc.read_unchecked();
    match &*r {
        Some(Some(d)) => render_module(d, &d.root, true),
        _ => rsx! { p { class: "text-text-muted", "No crate \"{crate_name}\"." } },
    }
}

#[component]
pub fn ApiItem(crate_name: String, path: Vec<String>) -> Element {
    let name = crate_name.clone();
    let doc = use_resource(move || {
        let name = name.clone();
        async move { data::crate_doc(&name).await }
    });
    let target = path.join("::");
    let r = doc.read_unchecked();
    match &*r {
        Some(Some(d)) => match find_module(&d.root, &crate_name, &target) {
            Some(m) => render_module(d, m, false),
            None => match find_item(&d.root, &target) {
                Some(it) => render_item(it),
                None => rsx! { p { class: "text-text-muted", "No item \"{target}\"." } },
            },
        },
        _ => rsx! { p { class: "text-text-muted", "Loading…" } },
    }
}

fn segs(path: &str) -> Vec<String> {
    path.split("::").skip(1).map(|s| s.to_string()).collect()
}

fn render_module(doc: &CrateDoc, m: &Module, is_root: bool) -> Element {
    let title = if is_root {
        doc.name.clone()
    } else {
        m.path.clone()
    };
    rsx! {
        h1 { class: "scroll-mt-6 text-3xl font-bold tracking-tight mt-4 mb-3 font-mono", "{title}" }
        Markdown { content: m.docs_md.clone() }
        if !m.submodules.is_empty() {
            h2 { class: "scroll-mt-6 text-2xl font-semibold mt-10 mb-3 pb-2 border-b border-border", "Modules" }
            ul { class: "list-disc pl-6 my-4 space-y-1.5",
                for sm in m.submodules.iter() {
                    li {
                        Link {
                            class: "text-accent underline underline-offset-2 font-mono text-sm",
                            to: Route::ApiItem { crate_name: doc.name.clone(), path: segs(&sm.path) },
                            "{sm.path}"
                        }
                    }
                }
            }
        }
        if !m.items.is_empty() {
            h2 { class: "scroll-mt-6 text-2xl font-semibold mt-10 mb-3 pb-2 border-b border-border", "Items" }
            ul { class: "list-disc pl-6 my-4 space-y-1.5",
                for it in m.items.iter() {
                    li {
                        Link {
                            class: "text-accent underline underline-offset-2 font-mono text-sm",
                            to: Route::ApiItem { crate_name: doc.name.clone(), path: segs(&it.path) },
                            "{it.name}"
                        }
                        span { class: "text-text-muted text-sm", " — {first_line(&it.docs_md)}" }
                    }
                }
            }
        }
    }
}

fn render_item(it: &Item) -> Element {
    let html = highlight_code("rust", &it.signature);
    rsx! {
        h1 { class: "scroll-mt-6 text-3xl font-bold tracking-tight mt-4 mb-3 font-mono", "{it.name}" }
        pre { class: "bg-code-bg border border-border rounded-lg p-4 my-5 overflow-x-auto",
            code { class: "font-mono text-sm leading-relaxed", dangerous_inner_html: "{html}" }
        }
        Markdown { content: it.docs_md.clone() }
        if !it.members.is_empty() {
            h2 { class: "scroll-mt-6 text-2xl font-semibold mt-10 mb-3 pb-2 border-b border-border", "Members" }
            for mem in it.members.iter() {
                div { class: "my-4",
                    code { class: "font-mono text-[0.85em] bg-code-bg text-accent rounded-md border border-border px-1.5 py-0.5", "{mem.signature}" }
                    Markdown { content: mem.docs_md.clone() }
                }
            }
        }
    }
}

fn find_item<'a>(m: &'a Module, target: &str) -> Option<&'a Item> {
    if let Some(it) = m
        .items
        .iter()
        .find(|i| i.path.split("::").skip(1).collect::<Vec<_>>().join("::") == target)
    {
        return Some(it);
    }
    for sm in &m.submodules {
        if let Some(it) = find_item(sm, target) {
            return Some(it);
        }
    }
    None
}

fn find_module<'a>(root: &'a Module, crate_name: &str, target: &str) -> Option<&'a Module> {
    let full = if target.is_empty() {
        crate_name.to_string()
    } else {
        format!("{crate_name}::{target}")
    };
    find_module_by_full(root, &full)
}

fn find_module_by_full<'a>(m: &'a Module, full: &str) -> Option<&'a Module> {
    if m.path == full {
        return Some(m);
    }
    for sm in &m.submodules {
        if let Some(found) = find_module_by_full(sm, full) {
            return Some(found);
        }
    }
    None
}

fn first_line(md: &str) -> String {
    md.lines().next().unwrap_or("").to_string()
}

#[component]
pub fn RenderItemProbe(item: Item) -> Element {
    render_item(&item)
}
