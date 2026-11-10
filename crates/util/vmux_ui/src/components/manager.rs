use crate::cn::cn;
use crate::components::badge::Badge;
use crate::components::dropdown::{Dropdown, DropdownOption, DropdownSelector};
use crate::components::page::{
    PageContent, PageHeader, PageShell, PageSidebarToggle, PageToolbarRow,
};
use crate::components::skeleton::Skeleton;
use crate::components::tooltip::Tooltip;
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use crate::navigation::NavigationGesture;
use crate::platform::Platform;
use dioxus::core::{Task, current_scope_id};
use dioxus::prelude::*;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum ManagerTone {
    #[default]
    Neutral,
    Primary,
    Green,
    Amber,
}

impl ManagerTone {
    pub fn for_runtime(runtime: &str) -> Self {
        match runtime {
            "native" => Self::Green,
            "node" => Self::Primary,
            "python" => Self::Amber,
            _ => Self::Neutral,
        }
    }
}

impl ManagerTone {
    fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "bg-foreground/[0.06] text-muted-foreground ring-foreground/10",
            Self::Primary => "bg-primary/10 text-primary ring-primary/20",
            Self::Green => "bg-success/10 text-success ring-success/20",
            Self::Amber => "bg-amber-400/10 text-amber-700 dark:text-amber-300 ring-amber-400/20",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum ManagerButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
    Ghost,
}

impl ManagerButtonVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Primary => {
                "vmux-primary-action text-primary ring-transparent hover:bg-foreground/[0.04]"
            }
            Self::Secondary => {
                "bg-foreground/[0.05] text-foreground/80 ring-foreground/10 hover:bg-foreground/[0.09]"
            }
            Self::Danger => {
                "bg-foreground/[0.05] text-foreground/70 ring-foreground/10 hover:bg-ansi-1/15 hover:text-ansi-1"
            }
            Self::Ghost => {
                "text-muted-foreground ring-transparent hover:bg-foreground/[0.08] hover:text-foreground"
            }
        }
    }
}

#[component]
pub fn ManagerPage(children: Element) -> Element {
    rsx! {
        PageShell { {children} }
    }
}

#[component]
pub fn WorkspaceLayout(
    #[props(default = rsx! {})] toolbar: Element,
    #[props(default = rsx! {})] sidebar: Element,
    #[props(default)] search: Option<Element>,
    #[props(default = true)] show_sidebar: bool,
    children: Element,
) -> Element {
    let sidebar_width = vmux_api::editor::TreeWidth::DEFAULT;
    let mut sidebar_open = use_signal(|| !cfg!(any(target_os = "ios", target_os = "android")));
    let duration = crate::motion::Motion::LAYOUT_MS;
    let sidebar_style = if sidebar_open() {
        format!(
            "width:min(var(--vmux-sidebar-width),50vw);opacity:1;transition-duration:{duration}ms;"
        )
    } else {
        format!("width:0px;opacity:0;transition-duration:{duration}ms;")
    };
    rsx! {
        div { class: "flex min-h-0 flex-1 overflow-hidden", style: "--vmux-sidebar-width:{sidebar_width}px;",
                if show_sidebar {
                    aside {
                        class: "flex min-h-0 shrink-0 flex-col overflow-hidden transition-[width,opacity] ease-out motion-reduce:transition-none",
                        style: sidebar_style,
                        aria_hidden: !sidebar_open(),
                        inert: (!sidebar_open()).then_some(""),
                        if let Some(search) = &search {
                            div { class: "w-[min(var(--vmux-sidebar-width),50vw)] shrink-0", {search.clone()} }
                        }
                        div { class: "min-h-0 w-[min(var(--vmux-sidebar-width),50vw)] flex-1 overflow-y-auto px-1 pb-2", {sidebar} }
                    }
                }
                div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                    PageToolbarRow {
                        if show_sidebar {
                            PageSidebarToggle {
                                open: sidebar_open(),
                                label: translate("schema-side-sheet"),
                                ontoggle: move |_| sidebar_open.set(!sidebar_open()),
                            }
                        }
                        if !show_sidebar {
                            if let Some(search) = search {
                                div { class: "w-[min(var(--vmux-sidebar-width),50vw)] min-w-0 shrink-0", {search} }
                            }
                        }
                        div { class: "min-w-0 flex-1", {toolbar} }
                    }
                    {children}
                }
        }
    }
}

#[component]
pub fn ManagerSearchRow(children: Element) -> Element {
    rsx! { PageToolbarRow { class: "relative w-full px-2".to_string(), {children} } }
}

#[component]
pub fn ManagerSidebarSection(title: String, children: Element) -> Element {
    rsx! {
        section { class: "flex flex-col gap-0.5",
            h2 { class: "px-2 pb-1 text-[10px] font-medium text-muted-foreground/65", "{title}" }
            {children}
        }
    }
}

#[component]
pub fn ManagerSidebarItem(
    label: String,
    count: usize,
    selected: bool,
    #[props(default)] icon: Option<LineIcon>,
    #[props(default)] builtin_icon: Option<vmux_api::BuiltinIcon>,
    #[props(default)] leading: Option<Element>,
    #[props(default)] dot: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if selected {
        "bg-foreground/[0.065] text-foreground"
    } else {
        "text-muted-foreground hover:bg-foreground/[0.035] hover:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            class: "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] transition-colors {class}",
            onclick: move |event| onclick.call(event),
            if let Some(leading) = leading {
                {leading}
            } else if let Some(icon) = builtin_icon {
                crate::icon::BuiltinIconView { icon, class: "size-3.5 shrink-0".to_string() }
            } else if let Some(icon) = icon {
                LineIconView { icon, class: "size-3.5 shrink-0".to_string() }
            } else {
                span { class: "size-2 shrink-0 rounded-full {dot}" }
            }
            span { class: "min-w-0 flex-1 truncate", "{label}" }
            span { class: "text-[10px] tabular-nums text-muted-foreground/65", "{count}" }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerTab {
    pub id: String,
    pub label: String,
    pub href: String,
}

#[component]
pub fn ManagerTabs(
    active: String,
    tabs: Vec<ManagerTab>,
    #[props(default)] content_class: String,
    onselect: EventHandler<String>,
) -> Element {
    let content_class = if content_class.is_empty() {
        "mx-auto flex w-full items-center justify-center".to_string()
    } else {
        content_class
    };
    rsx! {
        nav { class: "flex shrink-0 overflow-x-auto bg-foreground/[0.018] px-5 py-2.5",
            div { class: "{content_class}",
                div { class: "flex min-w-max items-center gap-0.5 rounded-lg bg-foreground/[0.055] p-1",
                    for tab in tabs {
                        a {
                            href: "{tab.href}",
                            onclick: move |event| {
                                event.prevent_default();
                                onselect.call(tab.href.clone());
                            },
                            aria_current: if tab.id == active { "page" } else { "false" },
                            class: if tab.id == active {
                                "flex h-7 items-center rounded-md bg-foreground/[0.08] px-3 text-xs font-semibold text-foreground"
                            } else {
                                "flex h-7 items-center rounded-lg px-3 text-xs font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
                            },
                            "{tab.label}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ManagerHeader(
    title: String,
    count: usize,
    search_value: String,
    search_placeholder: String,
    onsearch: EventHandler<FormEvent>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default)] content_class: String,
    actions: Element,
) -> Element {
    let content_class = if content_class.is_empty() {
        "mx-auto w-full max-w-5xl".to_string()
    } else {
        content_class
    };
    rsx! {
        PageHeader { content_class,
            div {
                div { class: "flex items-center gap-3",
                    h1 { class: "text-base font-semibold tracking-tight", "{title}" }
                    span { class: "text-xs tabular-nums text-muted-foreground/70", "{count}" }
                    div { class: "flex-1" }
                    {actions}
                }
                input {
                    r#type: "search",
                    class: "mt-3 w-full rounded-lg bg-foreground/[0.05] px-4 py-2.5 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:bg-foreground/[0.075]",
                    placeholder: "{search_placeholder}",
                    value: "{search_value}",
                    oninput: move |event| onsearch.call(event),
                    onkeydown: move |event| {
                        if let Some(handler) = &onkeydown {
                            handler.call(event);
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub fn ManagerToolbar(children: Element, #[props(default)] content_class: String) -> Element {
    let content_class = if content_class.is_empty() {
        "mx-auto flex h-full w-full max-w-5xl items-center gap-2.5".to_string()
    } else {
        content_class
    };
    rsx! {
        header { class: "h-11 shrink-0 bg-foreground/[0.018] px-5",
            div { class: "{content_class}", {children} }
        }
    }
}

#[component]
pub fn ManagerControls(children: Element, #[props(default)] class: String) -> Element {
    let class = if class.is_empty() {
        "flex min-h-7 items-center justify-end gap-2".to_string()
    } else {
        class
    };
    rsx! {
        div { class: "{class}", {children} }
    }
}

#[component]
pub fn ManagerSearch(
    value: String,
    placeholder: String,
    #[props(default)] id: Option<String>,
    #[props(default)] autofocus: bool,
    #[props(default)] onsearch: Option<EventHandler<String>>,
    #[props(default)] ondebounced: Option<EventHandler<String>>,
    #[props(default = 150)] debounce_ms: u32,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default = rsx! {})] prefix: Element,
    #[props(default = rsx! {})] suffix: Element,
) -> Element {
    let mut draft = use_signal(|| value.clone());
    let mut pending = use_signal(|| None::<Task>);
    let generated_id = use_hook(|| format!("vmux-manager-search-input-{}", current_scope_id().0));
    let id = id.unwrap_or(generated_id);
    use_effect(use_reactive!(|value| {
        if *draft.peek() != value {
            draft.set(value);
        }
    }));
    use_drop(move || {
        if let Some(task) = pending.write().take() {
            task.cancel();
        }
    });
    let class = "group/manager-search relative z-30 flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 transition-colors duration-100 hover:bg-foreground/[0.025] focus-within:bg-foreground/[0.045] motion-reduce:transition-none";
    let input_class = "h-full w-full min-w-0 flex-1 bg-transparent text-ui text-foreground outline-none placeholder:text-muted-foreground/55";
    rsx! {
        div { class, "data-vmux-search-active": if draft().is_empty() { "false" } else { "true" },
            label { r#for: id.clone(), class: "flex h-full shrink-0 cursor-text items-center",
                LineIconView {
                    icon: LineIcon::Search,
                    class: "size-3.5 shrink-0 text-muted-foreground/70 transition-colors duration-100 group-focus-within/manager-search:text-foreground",
                }
            }
            {prefix}
            label { r#for: id.clone(), class: "flex h-full min-w-0 flex-1 cursor-text items-center",
                input {
                    id,
                    r#type: "search",
                    class: input_class,
                    aria_label: placeholder.clone(),
                    placeholder,
                    value: draft(),
                    autofocus,
                    autocomplete: "off",
                    spellcheck: "false",
                    oninput: move |event: FormEvent| {
                        let query = event.value();
                        draft.set(query.clone());
                        if let Some(handler) = onsearch {
                            handler.call(query.clone());
                        }
                        if let Some(task) = pending.write().take() {
                            task.cancel();
                        }
                        let Some(handler) = ondebounced else {
                            return;
                        };
                        if debounce_ms == 0 {
                            handler.call(query);
                            return;
                        }
                        pending.set(Some(spawn(async move {
                            Platform::sleep(debounce_ms).await;
                            handler.call(query);
                        })));
                    },
                    onkeydown: move |event| {
                        event.stop_propagation();
                        if let Some(handler) = &onkeydown {
                            handler.call(event);
                        }
                    },
                }
            }
            {suffix}
        }
    }
}

#[component]
pub fn ManagerIconButton(
    label: String,
    #[props(default)] active: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let class = if active {
        "flex size-6 shrink-0 items-center justify-center rounded-md text-primary"
    } else {
        "flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
    };
    rsx! {
        Tooltip { label: label.clone(),
            button {
                r#type: "button",
                class,
                aria_label: label,
                aria_pressed: active,
                disabled,
                onclick: move |event| onclick.call(event),
                {children}
            }
        }
    }
}

#[component]
pub fn ManagerAddButton(
    label: String,
    #[props(default = LineIcon::Plus)] icon: LineIcon,
    #[props(default)] active: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "vmux-primary-action flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-[11px] font-medium transition-opacity disabled:pointer-events-none disabled:opacity-40",
            aria_label: label.clone(),
            aria_pressed: active,
            disabled,
            onclick: move |event| onclick.call(event),
            LineIconView { icon, class: "size-3.5" }
            span { "{label}" }
        }
    }
}

#[component]
pub fn ManagerExpandableSearch(
    open: bool,
    active: bool,
    label: String,
    ontoggle: EventHandler<()>,
    ondismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let root_id = format!("vmux-manager-search-{}", current_scope_id().0);
    let duration = crate::motion::Motion::CONTENT_MS;
    let class = if open {
        "relative z-[100] h-7 w-[min(28rem,45vw)] shrink-0"
    } else {
        "relative z-auto h-7 w-7 shrink-0"
    };
    rsx! {
        Dropdown {
            open,
            class: class.to_string(),
            keyboard_navigation: true,
            on_dismiss: move |_| ondismiss.call(()),
            div {
                id: root_id.clone(),
                class: "h-7 w-full",
                "data-vmux-search-open": if open { "true" } else { "false" },
                if open {
                    ManagerSearchDismiss {
                        root_id: root_id.clone(),
                        ondismiss: move |_| ondismiss.call(()),
                    }
                    div { style: "--vmux-motion-ms:{duration}ms;", class: "relative h-7 w-full animate-[dx-fade-in_var(--vmux-motion-ms)_ease-out_forwards] motion-reduce:animate-none", {children} }
                } else {
                    ManagerIconButton {
                        label,
                        active,
                        onclick: move |_| ontoggle.call(()),
                        LineIconView { icon: LineIcon::Search, class: "size-3.5".to_string() }
                    }
                }
            }
        }
    }
}

#[component]
fn ManagerSearchDismiss(root_id: String, ondismiss: EventHandler<()>) -> Element {
    use_future(move || {
        let script = format!(
            r#"
const root = document.getElementById({root_id:?});
if (!root) {{
  dioxus.send(false);
}} else {{
  const entries = globalThis.__vmuxManagerSearchEntries ??= new Map();
  entries.get(root.id)?.();
  let active = true;
  const cleanup = () => {{
    if (!active) return;
    active = false;
    observer.disconnect();
    document.removeEventListener('pointerdown', pointerdown, true);
    document.removeEventListener('keydown', keydown, true);
    window.removeEventListener('blur', dismiss, true);
    if (entries.get(root.id) === cleanup) entries.delete(root.id);
  }};
  const dismiss = () => {{
    if (!active) return;
    cleanup();
    dioxus.send(true);
  }};
  const pointerdown = event => {{
    if (event.composedPath().includes(root)) return;
    dismiss();
  }};
  const keydown = event => {{
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    dismiss();
  }};
  const observer = new MutationObserver(() => {{
    if (root.isConnected && root.getAttribute('data-vmux-search-open') === 'true') return;
    cleanup();
  }});
  entries.set(root.id, cleanup);
  observer.observe(document, {{
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['data-vmux-search-open'],
  }});
  document.addEventListener('pointerdown', pointerdown, true);
  document.addEventListener('keydown', keydown, true);
  window.addEventListener('blur', dismiss, true);
}}
"#
        );
        async move {
            let mut eval = document::eval(&script);
            if eval.recv::<bool>().await.is_ok_and(|dismiss| dismiss) {
                ondismiss.call(());
            }
        }
    });
    rsx! {}
}

#[component]
pub fn ManagerSearchAction(
    value: String,
    placeholder: String,
    onsearch: EventHandler<String>,
    #[props(default)] label: String,
    #[props(default)] active: bool,
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default = rsx! {})] prefix: Element,
    #[props(default = rsx! {})] suffix: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let label = if label.is_empty() {
        placeholder.clone()
    } else {
        label
    };
    rsx! {
        ManagerExpandableSearch {
            open: open(),
            active: active || !value.is_empty(),
            label,
            ontoggle: move |_| open.set(!open()),
            ondismiss: move |_| open.set(false),
            ManagerSearch {
                value,
                placeholder,
                autofocus: true,
                onsearch,
                onkeydown,
                prefix,
                suffix,
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ManagerView {
    #[default]
    List,
    Grid,
}

#[component]
pub fn ManagerViewToggle(
    view: ManagerView,
    list_label: String,
    grid_label: String,
    #[props(default = LineIcon::Grid2X2)] grid_icon: LineIcon,
    onchange: EventHandler<ManagerView>,
) -> Element {
    rsx! {
        div { class: "flex items-center gap-0.5",
            button {
                r#type: "button",
                aria_label: list_label.clone(),
                title: list_label,
                aria_pressed: view == ManagerView::List,
                class: if view == ManagerView::List {
                    "flex size-6 items-center justify-center rounded text-foreground"
                } else {
                    "flex size-6 items-center justify-center rounded text-muted-foreground/60 transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
                },
                onclick: move |_| onchange.call(ManagerView::List),
                LineIconView { icon: LineIcon::List, class: "size-3.5" }
            }
            button {
                r#type: "button",
                aria_label: grid_label.clone(),
                title: grid_label,
                aria_pressed: view == ManagerView::Grid,
                class: if view == ManagerView::Grid {
                    "flex size-6 items-center justify-center rounded text-foreground"
                } else {
                    "flex size-6 items-center justify-center rounded text-muted-foreground/60 transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
                },
                onclick: move |_| onchange.call(ManagerView::Grid),
                LineIconView { icon: grid_icon, class: "size-3.5" }
            }
        }
    }
}

#[component]
pub fn ManagerList(children: Element, #[props(default)] class: String) -> Element {
    let content_class = if class.is_empty() {
        "mx-auto flex w-full max-w-5xl flex-col gap-3".to_string()
    } else {
        class
    };
    rsx! {
        PageContent { content_class, {children} }
    }
}

#[component]
pub fn ManagerSection(label: String, count: usize, children: Element) -> Element {
    rsx! {
        section { class: "flex flex-col gap-2",
            header { class: "flex items-center gap-2 px-1",
                h2 { class: "text-[11px] font-semibold text-foreground/80", "{label}" }
                span { class: "text-[10px] tabular-nums text-muted-foreground/55", "{count}" }
            }
            {children}
        }
    }
}

#[component]
pub fn ManagerCollection(
    view: ManagerView,
    #[props(default)] grid_size: ManagerGridSize,
    children: Element,
) -> Element {
    let class = match (view, grid_size) {
        (ManagerView::Grid, ManagerGridSize::Compact) => {
            "grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-2"
        }
        (ManagerView::Grid, ManagerGridSize::Comfortable) => {
            "grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-2"
        }
        (ManagerView::List, _) => "flex flex-col gap-2",
    };
    rsx! {
        div { class, {children} }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ManagerGridSize {
    #[default]
    Compact,
    Comfortable,
}

#[component]
pub fn ManagerRow(
    icon: Element,
    title: String,
    subtitle: String,
    meta: Element,
    actions: Element,
    #[props(default = true)] show_icon: bool,
    #[props(default = true)] icon_surface: bool,
    #[props(default)] selected: bool,
    #[props(default)] onselect: Option<EventHandler<NavigationGesture>>,
) -> Element {
    let interactive = onselect.is_some();
    let class = match (interactive, selected) {
        (_, true) => {
            "group flex items-center gap-4 rounded-lg bg-primary/[0.09] px-4 py-3 shadow-[inset_2px_0_0_var(--primary)] transition-colors"
        }
        (true, false) => {
            "group flex cursor-pointer items-center gap-4 rounded-lg bg-foreground/[0.03] px-4 py-3 transition-colors hover:bg-foreground/[0.065]"
        }
        (false, false) => {
            "group flex items-center gap-4 rounded-lg bg-foreground/[0.03] px-4 py-3 transition-colors hover:bg-foreground/[0.065]"
        }
    };
    rsx! {
        div {
            class,
            role: if interactive { "button" } else { "group" },
            tabindex: if interactive { "0" } else { "-1" },
            aria_selected: selected,
            onclick: move |event| {
                if let Some(handler) = &onselect {
                    handler.call(NavigationGesture::from_mouse(&event));
                }
            },
            onkeydown: move |event| {
                let activate = event.key() == Key::Enter
                    || matches!(event.key(), Key::Character(key) if key == " ");
                if !activate {
                    return;
                }
                event.prevent_default();
                if let Some(handler) = &onselect {
                    handler.call(NavigationGesture::default());
                }
            },
            if show_icon {
                div { class: if icon_surface {
                        "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.065]"
                    } else {
                        "flex h-9 w-9 shrink-0 items-center justify-center"
                    },
                    {icon}
                }
            }
            div { class: "flex min-w-0 flex-1 flex-col gap-1",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate font-medium text-foreground/95", "{title}" }
                    {meta}
                }
                if !subtitle.is_empty() {
                    span { class: "truncate text-xs text-muted-foreground/70", "{subtitle}" }
                }
            }
            div { class: "flex shrink-0 items-center gap-2", {actions} }
        }
    }
}

#[component]
pub fn ManagerCard(
    icon: Element,
    title: String,
    subtitle: String,
    meta: Element,
    actions: Element,
    #[props(default = true)] show_icon: bool,
    #[props(default = true)] icon_surface: bool,
    #[props(default)] selected: bool,
    onselect: EventHandler<NavigationGesture>,
) -> Element {
    let keyboard_select = onselect;
    rsx! {
        div {
            class: if selected {
                "group flex min-h-28 min-w-0 cursor-pointer flex-col rounded-xl bg-primary/[0.09] p-3 shadow-[inset_2px_0_0_var(--primary)] transition-colors"
            } else {
                "group flex min-h-28 min-w-0 cursor-pointer flex-col rounded-xl bg-foreground/[0.03] p-3 transition-colors hover:bg-foreground/[0.06]"
            },
            role: "button",
            tabindex: "0",
            aria_selected: selected,
            onclick: move |event| onselect.call(NavigationGesture::from_mouse(&event)),
            onkeydown: move |event| {
                let activate = event.key() == Key::Enter
                    || matches!(event.key(), Key::Character(key) if key == " ");
                if !activate {
                    return;
                }
                event.prevent_default();
                keyboard_select.call(NavigationGesture::default());
            },
            div { class: "flex min-w-0 items-start gap-3",
                if show_icon {
                    div { class: if icon_surface {
                            "flex size-10 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.065]"
                        } else {
                            "flex size-10 shrink-0 items-center justify-center"
                        },
                        {icon}
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate font-medium text-foreground/95", "{title}" }
                    if !subtitle.is_empty() {
                        div { class: "mt-0.5 truncate text-xs text-muted-foreground/70", "{subtitle}" }
                    }
                }
                {actions}
            }
            div { class: "mt-3 flex min-w-0 flex-wrap items-center gap-1.5",
                {meta}
            }
        }
    }
}

#[component]
pub fn ManagerDetails(
    icon: Element,
    title: String,
    subtitle: String,
    children: Element,
    actions: Element,
    #[props(default = true)] show_icon: bool,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        aside { class: "flex min-h-0 w-[21rem] shrink-0 flex-col bg-foreground/[0.018]",
            header { class: "flex shrink-0 items-start gap-3 p-4",
                if show_icon {
                    div { class: "flex size-9 shrink-0 items-center justify-center", {icon} }
                }
                div { class: "min-w-0 flex-1",
                    h2 { class: "truncate text-sm font-semibold text-foreground", "{title}" }
                    if !subtitle.is_empty() {
                        div { class: "mt-0.5 truncate text-xs text-muted-foreground/70", "{subtitle}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                    aria_label: translate("common-close"),
                    onclick: move |event| onclose.call(event),
                    LineIconView { icon: LineIcon::X, class: "size-3.5" }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-4 pb-4", {children} }
            footer { class: "flex shrink-0 flex-wrap items-center justify-end gap-2 px-4 pb-4 pt-2",
                {actions}
            }
        }
    }
}

#[component]
pub fn ManagerThumbnail(src: Option<String>, fallback: String) -> Element {
    rsx! {
        if let Some(src) = src.filter(|src| !src.is_empty()) {
            img {
                class: "h-6 w-6 rounded object-contain",
                src,
                draggable: "false",
            }
        } else {
            span { class: "font-mono text-[10px] text-muted-foreground", "{fallback}" }
        }
    }
}

#[component]
pub fn ManagerBadge(
    #[props(default)] tone: ManagerTone,
    #[props(default = true)] uppercase: bool,
    children: Element,
) -> Element {
    let typography = if uppercase {
        "uppercase tracking-wide"
    } else {
        "tracking-normal"
    };
    rsx! {
        Badge { class: cn(["rounded-full px-2 py-0.5 text-[10px] font-medium ring-1 ring-inset", typography, tone.classes()]),
            {children}
        }
    }
}

#[component]
pub fn ManagerButton(
    #[props(default)] variant: ManagerButtonVariant,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            class: "shrink-0 rounded-lg px-3 py-1.5 text-xs font-medium ring-1 ring-inset transition-colors disabled:pointer-events-none disabled:opacity-50 {variant.classes()}",
            disabled,
            onclick: move |event| onclick.call(event),
            {children}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerSelectItem {
    pub value: String,
    pub label: String,
}

#[component]
pub fn ManagerSelect(
    items: Vec<ManagerSelectItem>,
    value: Option<String>,
    placeholder: String,
    #[props(default)] disabled: bool,
    onselect: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let selected = value.unwrap_or_default();
    let empty = items.is_empty();
    let label = items
        .iter()
        .find(|item| item.value == selected)
        .map(|item| item.label.clone())
        .unwrap_or_else(|| placeholder.clone());
    let disabled = disabled || empty;

    rsx! {
        DropdownSelector {
            label,
            open: open(),
            active: !selected.is_empty(),
            disabled,
            class: "min-w-0".to_string(),
            trigger_class: "w-full min-w-24 justify-between bg-foreground/[0.045] px-2.5 text-foreground hover:bg-foreground/[0.075]".to_string(),
            popup_class: "max-h-80 min-w-40 overflow-y-auto p-1".to_string(),
            on_open_change: move |next| open.set(next),
            on_dismiss: move |_| open.set(false),
            for item in items {
                {
                    let item_selected = item.value == selected;
                    rsx! {
                        DropdownOption {
                            key: "{item.value}",
                            label: item.label,
                            selected: item_selected,
                            onclick: move |_| {
                                open.set(false);
                                onselect.call(item.value.clone());
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ManagerSpinner(detail: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 text-xs text-muted-foreground",
            span { class: "h-3.5 w-3.5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
            if !detail.is_empty() {
                span { class: "max-w-44 truncate", "{detail}" }
            }
        }
    }
}

#[component]
pub fn ManagerEmpty(
    title: String,
    detail: String,
    #[props(default = rsx! {})] actions: Element,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-3 py-16 text-center",
            div { class: "text-sm text-muted-foreground", "{title}" }
            if !detail.is_empty() {
                div { class: "text-xs text-muted-foreground/70", "{detail}" }
            }
            {actions}
        }
    }
}

#[component]
pub fn ManagerSkeleton() -> Element {
    rsx! {
        div { class: "contents", "data-vmux-loading-surface": "",
        for i in 0..3 {
            div { key: "{i}", class: "flex items-center gap-4 rounded-lg bg-foreground/[0.03] px-4 py-3",
                Skeleton { class: "h-9 w-9 shrink-0 rounded-lg bg-foreground/[0.06]" }
                div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                    Skeleton { class: "h-3 w-32 bg-foreground/[0.06]" }
                    Skeleton { class: "h-2.5 w-48 bg-foreground/[0.05]" }
                }
            }
        }
        }
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, ElementId, Mutation, VirtualDom};
    use dioxus::html::{EventData, SerializedMouseData};

    #[component]
    fn SidebarSearch() -> Element {
        rsx! {
            WorkspaceLayout {
                search: rsx! { input { r#type: "search", id: "sidebar-search" } },
                sidebar: rsx! { div { "Navigation" } },
                div { "Content" }
            }
        }
    }

    #[derive(Default)]
    struct SidebarInert {
        blocked: bool,
        toggle: Option<ElementId>,
    }

    impl SidebarInert {
        fn apply(&mut self, edits: Vec<Mutation>) {
            for edit in edits {
                match edit {
                    Mutation::SetAttribute { name, value, .. } if name == "inert" => {
                        self.blocked = !matches!(value, AttributeValue::None);
                    }
                    Mutation::NewEventListener { name, id } if name == "click" => {
                        self.toggle = Some(id);
                    }
                    _ => {}
                }
            }
        }

        fn click(&mut self, dom: &mut VirtualDom) {
            let event = EventData::Mouse(SerializedMouseData::default());
            dom.runtime().handle_event(
                "click",
                Event::new(event.into_any(), true),
                self.toggle.unwrap(),
            );
            self.apply(dom.render_immediate_to_vec().edits);
        }
    }

    #[test]
    fn sidebar_search_is_interactive_while_open_and_inert_only_while_closed() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(SidebarSearch);
        let mut sidebar = SidebarInert::default();
        sidebar.apply(dom.rebuild_to_vec().edits);
        assert!(!sidebar.blocked);
        sidebar.click(&mut dom);
        assert!(sidebar.blocked);
        sidebar.click(&mut dom);
        assert!(!sidebar.blocked);
    }
}
