use crate::GalleryUiState;
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_api::BuiltinIcon;
use vmux_ui::components::input::Input;
use vmux_ui::favicon::Favicon;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{
    AgentIcon, AgentIconVariant, AgentIconView, BuiltinIconView, GitIconView, GitSymbol,
    GitSymbolView, LineIcon, LineIconView,
};

#[component]
pub(crate) fn IconLibrary(state: GalleryUiState) -> Element {
    let mut filter = use_signal(String::new);
    let mut size = use_signal(|| 24_u16);
    let query = filter().to_ascii_lowercase();
    let mut lines = Vec::new();
    for &name in LineIcon::VARIANT_NAMES {
        if format!("LineIcon::{name}")
            .to_ascii_lowercase()
            .contains(&query)
        {
            lines.push((name, LineIcon::parse(name).expect("declared line icon")));
        }
    }
    let mut builtins = Vec::new();
    for &name in BuiltinIcon::VARIANT_NAMES {
        if format!("BuiltinIcon::{name}")
            .to_ascii_lowercase()
            .contains(&query)
        {
            builtins.push((
                name,
                ron::from_str::<BuiltinIcon>(name).expect("declared builtin icon"),
            ));
        }
    }
    let mut git = Vec::new();
    let git_logo = "giticonview".contains(&query);
    for &name in GitSymbol::VARIANT_NAMES {
        if format!("GitSymbol::{name}")
            .to_ascii_lowercase()
            .contains(&query)
        {
            git.push((
                name,
                ron::from_str::<GitSymbol>(name).expect("declared Git symbol"),
            ));
        }
    }
    let mut files = Vec::new();
    for name in [
        "src/",
        "image.png",
        "file.rs",
        "file.ts",
        "file.tsx",
        "file.js",
        "file.py",
        "file.go",
        "file.rb",
        "file.sh",
        "file.lua",
        "file.swift",
        "file.c",
        "file.cpp",
        "file.php",
        "file.kt",
        "file.dart",
        "file.ex",
        "file.hs",
        "file.scala",
        "file.zig",
        "file.nim",
        "file.ml",
        "file.clj",
        "file.elm",
        "file.erl",
        "file.cr",
        "file.jl",
        "file.r",
        "file.pl",
        "file.fs",
        "file.v",
        "file.sol",
        "file.md",
        "file.html",
        "file.css",
        "file.scss",
        "file.vue",
        "file.svelte",
        "file.astro",
        "file.graphql",
        "file.json",
        "file.yaml",
        "file.toml",
        "Dockerfile",
        "file.tf",
        "file.gradle",
        "CMakeLists.txt",
        "file.nix",
        "file.prisma",
        "file.ipynb",
        "file.vim",
        "file.wasm",
        "file.ps1",
        "file.groovy",
        "file.sqlite",
        "file.cfg",
        "file.txt",
        "file.ron",
        "file.bin",
    ] {
        if format!("TypeIcon {name}")
            .to_ascii_lowercase()
            .contains(&query)
        {
            files.push(name);
        }
    }
    let mut assets = Vec::new();
    for name in [
        "agent",
        "agents",
        "docker",
        "editor",
        "error",
        "extensions",
        "files",
        "git",
        "history",
        "knowledge",
        "layout",
        "lsp",
        "projects",
        "services",
        "sessions",
        "settings",
        "shortcuts",
        "simulator",
        "spaces",
        "start",
        "team",
        "terminal",
        "tools",
        "ui",
        "vault",
        "vm",
    ] {
        if format!("Favicon {name}")
            .to_ascii_lowercase()
            .contains(&query)
        {
            assets.push(name);
        }
    }
    let mut agents = Vec::new();
    for &name in AgentIcon::VARIANT_NAMES {
        if format!("AgentIcon::{name}")
            .to_ascii_lowercase()
            .contains(&query)
        {
            agents.push((name, AgentIcon::parse(name).expect("declared agent icon")));
        }
    }
    let total = lines.len()
        + builtins.len()
        + git.len()
        + usize::from(git_logo)
        + files.len()
        + assets.len()
        + agents.len() * 2;
    let icon_class = "size-[var(--gallery-icon-size)] shrink-0";
    rsx! {
        div { class: "space-y-8", style: "--gallery-icon-size:{size()}px;",
            div { class: "flex flex-wrap items-center gap-3",
                Input { value: filter(), placeholder: translate("gallery-search-icons"), aria_label: translate("gallery-search-icons"), class: "min-w-40 flex-1 text-sm",
                    oninput: move |event: FormEvent| filter.set(event.value()),
                }
                label { class: "flex items-center gap-2 text-xs text-muted-foreground",
                    {translate("gallery-icon-size")}
                    select { class: "h-8 rounded-md bg-foreground/5 px-2 text-xs text-foreground", value: "{size()}",
                        onchange: move |event| { if let Ok(value) = event.value().parse::<u16>() { size.set(value.clamp(16, 48)); } },
                        for value in [16, 20, 24, 32, 48] { option { value: "{value}", "{value}px" } }
                    }
                }
                span { class: "font-mono text-xs tabular-nums text-muted-foreground", "{total}" }
            }
            div { class: if state.disabled { "space-y-8 opacity-40" } else { "space-y-8" },
                if !lines.is_empty() {
                    IconFamily { title: "LineIcon", count: lines.len(),
                        for (name, icon) in lines { IconTile { name: name.to_string(), LineIconView { icon, class: icon_class } } }
                    }
                }
                if !builtins.is_empty() {
                    IconFamily { title: "BuiltinIcon", count: builtins.len(),
                        for (name, icon) in builtins { IconTile { name: name.to_string(), BuiltinIconView { icon, class: icon_class } } }
                    }
                }
                if !git.is_empty() || git_logo {
                    IconFamily { title: "GitSymbol", count: git.len() + usize::from(git_logo),
                        for (name, symbol) in git { IconTile { name: name.to_string(), GitSymbolView { symbol, class: icon_class } } }
                        if git_logo { IconTile { name: "GitIconView", GitIconView { class: icon_class } } }
                    }
                }
                if !files.is_empty() {
                    IconFamily { title: "TypeIcon", count: files.len(),
                        for name in files { IconTile { name: name.to_string(), TypeIcon { path: name, is_dir: name.ends_with('/'), class: icon_class } } }
                    }
                }
                if !assets.is_empty() {
                    IconFamily { title: "Favicon", count: assets.len(),
                        for name in assets { IconTile { name: name.to_string(), Favicon { favicon_url: format!("assets/favicons/{name}.svg"), url: "", class: icon_class } } }
                    }
                }
                if !agents.is_empty() {
                    IconFamily { title: "AgentIcon", count: agents.len(),
                        for (name, icon) in agents.iter().copied() {
                            IconTile {
                                name: name.to_string(),
                                AgentIconView { icon, variant: AgentIconVariant::Color, class: icon_class }
                            }
                        }
                    }
                    IconFamily { title: "AgentIcon (Mono)", count: agents.len(),
                        for (name, icon) in agents {
                            IconTile {
                                name: name.to_string(),
                                AgentIconView { icon, variant: AgentIconVariant::Mono, class: icon_class }
                            }
                        }
                    }
                }
            }
            if total == 0 { p { class: "py-12 text-center text-sm text-muted-foreground", {translate("gallery-no-icons")} } }
        }
    }
}

#[component]
fn IconFamily(title: String, count: usize, children: Element) -> Element {
    rsx! {
        section { class: "space-y-3",
            h2 { class: "flex items-center gap-2 text-sm font-medium",
                "{title}"
                span { class: "font-mono text-[10px] font-normal tabular-nums text-muted-foreground", "{count}" }
            }
            div { class: "grid grid-cols-[repeat(auto-fill,minmax(92px,1fr))] gap-2", {children} }
        }
    }
}

#[component]
fn IconTile(name: String, children: Element) -> Element {
    rsx! {
        div { class: "group flex min-h-24 min-w-0 flex-col items-center justify-center gap-3 rounded-lg bg-foreground/[0.025] px-2 py-4 transition-colors hover:bg-foreground/[0.06]", title: name.clone(),
            div { class: "flex h-12 items-center justify-center text-foreground", {children} }
            code { class: "max-w-full break-all text-center text-[10px] leading-4 text-muted-foreground group-hover:text-foreground", "{name}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_icon_names_decode_for_every_preview_family() {
        for &name in LineIcon::VARIANT_NAMES {
            assert!(LineIcon::parse(name).is_some(), "LineIcon::{name}");
        }
        for &name in BuiltinIcon::VARIANT_NAMES {
            ron::from_str::<BuiltinIcon>(name).expect(name);
        }
        for &name in GitSymbol::VARIANT_NAMES {
            ron::from_str::<GitSymbol>(name).expect(name);
        }
        for &name in AgentIcon::VARIANT_NAMES {
            assert!(AgentIcon::parse(name).is_some(), "AgentIcon::{name}");
        }
    }
}
