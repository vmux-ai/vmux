use crate::cn::cn;
use crate::components::badge::Badge;
use crate::components::breadcrumb::{
    BreadcrumbOption, BreadcrumbOptionIcon, BreadcrumbSelector, Breadcrumbs,
};
use crate::components::dropdown::{
    Dropdown, DropdownAlign, DropdownOption, DropdownPopup, DropdownSelector,
};
use crate::components::page::{PageContent, PageHeader, PageShell};
use crate::components::skeleton::Skeleton;
use crate::components::tooltip::Tooltip;
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use crate::navigation::NavigationGesture;
use crate::navigation::PageNavigation;
use crate::platform::Platform;
use dioxus::core::Task;
use dioxus::prelude::*;
use vmux_api::BuiltinIcon;

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
    header: Element,
    #[props(default = rsx! {})] sidebar: Element,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col overflow-hidden",
            {header}
            div { class: "flex min-h-0 flex-1 overflow-hidden",
                aside { class: "hidden w-44 shrink-0 flex-col px-2 pb-2 sm:flex",
                    div { class: "min-h-0 flex-1 overflow-y-auto", {sidebar} }
                }
                div { class: "flex min-w-0 flex-1 flex-col overflow-hidden", {children} }
            }
        }
    }
}

#[component]
pub fn WorkspaceHeader(children: Element, #[props(default = rsx! {})] actions: Element) -> Element {
    rsx! {
        header { class: "relative z-40 flex h-14 shrink-0 items-center gap-2 px-2.5",
            div { class: "flex min-w-0 flex-1 items-center gap-2", {children} }
            div { class: "flex shrink-0 items-center gap-1", {actions} }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePage {
    Editor,
    Git,
    History,
    Knowledge,
    Projects,
    Services,
    Sessions,
    Settings,
    Shortcuts,
    Simulator,
    Start,
    Team,
    Terminal,
    Tools,
    Vault,
}

impl WorkspacePage {
    const ALL: [Self; 15] = [
        Self::Start,
        Self::Sessions,
        Self::Projects,
        Self::Git,
        Self::Editor,
        Self::Terminal,
        Self::Knowledge,
        Self::Team,
        Self::Tools,
        Self::Vault,
        Self::History,
        Self::Shortcuts,
        Self::Simulator,
        Self::Services,
        Self::Settings,
    ];

    pub fn option(self) -> BreadcrumbOption {
        let (label, value, icon_url, icon) = match self {
            Self::Editor => (
                translate("editor-editor"),
                "file://",
                "assets/favicons/editor.svg",
                BuiltinIcon::Code,
            ),
            Self::Git => (
                translate("git-title"),
                "git://",
                "assets/favicons/git.svg",
                BuiltinIcon::GitBranch,
            ),
            Self::History => (
                translate("history-title"),
                "vmux://history/",
                "assets/favicons/history.svg",
                BuiltinIcon::Clock,
            ),
            Self::Knowledge => (
                translate("layout-knowledge"),
                "vmux://knowledge/",
                "assets/favicons/knowledge.svg",
                BuiltinIcon::Brain,
            ),
            Self::Projects => (
                translate("layout-projects"),
                "vmux://projects/",
                "assets/favicons/projects.svg",
                BuiltinIcon::Project,
            ),
            Self::Services => (
                translate("services-title"),
                "vmux://services/",
                "assets/favicons/services.svg",
                BuiltinIcon::Activity,
            ),
            Self::Sessions => (
                translate("sessions-title"),
                "vmux://sessions/",
                "assets/favicons/sessions.svg",
                BuiltinIcon::MessagesRound,
            ),
            Self::Settings => (
                translate("settings-title"),
                "vmux://settings/",
                "assets/favicons/settings.svg",
                BuiltinIcon::Settings,
            ),
            Self::Shortcuts => (
                translate("shortcuts-title"),
                "vmux://shortcuts/",
                "assets/favicons/shortcuts.svg",
                BuiltinIcon::Keyboard,
            ),
            Self::Simulator => (
                translate("simulator-title"),
                "vmux://simulator/",
                "assets/favicons/simulator.svg",
                BuiltinIcon::Smartphone,
            ),
            Self::Start => (
                translate("start-title"),
                "vmux://start/",
                "assets/favicons/start.svg",
                BuiltinIcon::Sparkles,
            ),
            Self::Team => (
                translate("team-title"),
                "vmux://team/",
                "assets/favicons/team.svg",
                BuiltinIcon::Users,
            ),
            Self::Terminal => (
                translate("command-terminal"),
                "vmux://terminal/",
                "assets/favicons/terminal.svg",
                BuiltinIcon::Terminal,
            ),
            Self::Tools => (
                translate("tools-title"),
                "vmux://tools/acp",
                "assets/favicons/tools.svg",
                BuiltinIcon::Toolbox,
            ),
            Self::Vault => (
                translate("vault-title"),
                "vmux://vault/",
                "assets/favicons/vault.svg",
                BuiltinIcon::Vault,
            ),
        };
        BreadcrumbOption {
            label,
            value: value.to_string(),
            identity_seed: String::new(),
            icon_url: icon_url.to_string(),
            icon: Some(icon),
        }
    }

    pub fn options() -> Vec<BreadcrumbOption> {
        Self::ALL
            .into_iter()
            .filter(|page| *page != Self::Editor)
            .map(Self::option)
            .collect()
    }

    pub fn options_with(contextual: Vec<BreadcrumbOption>) -> Vec<BreadcrumbOption> {
        let mut options = Self::options();
        for option in contextual {
            if let Some(page) = Self::ALL
                .into_iter()
                .find(|page| page.answers_for(&option.value))
            {
                options.retain(|existing| !page.answers_for(&existing.value));
            }
            options.push(option);
        }
        options.sort_by_cached_key(|option| Self::rank(&option.value));
        options
    }

    fn answers_for(self, url: &str) -> bool {
        match self {
            Self::Editor => url.starts_with("file:"),
            Self::Git => url.starts_with("git:"),
            Self::Terminal => url.starts_with("vmux://terminal"),
            Self::History => url.starts_with("vmux://history"),
            Self::Knowledge => url.starts_with("vmux://knowledge"),
            Self::Projects => url.starts_with("vmux://projects"),
            Self::Services => url.starts_with("vmux://services"),
            Self::Sessions => url.starts_with("vmux://sessions"),
            Self::Settings => url.starts_with("vmux://settings"),
            Self::Shortcuts => url.starts_with("vmux://shortcuts"),
            Self::Simulator => url.starts_with("vmux://simulator"),
            Self::Start => url.starts_with("vmux://start"),
            Self::Team => url.starts_with("vmux://team"),
            Self::Tools => url.starts_with("vmux://tools"),
            Self::Vault => url.starts_with("vmux://vault"),
        }
    }

    fn rank(url: &str) -> usize {
        Self::ALL
            .iter()
            .position(|page| page.answers_for(url))
            .unwrap_or(usize::MAX)
    }
}

#[component]
pub fn WorkspaceBreadcrumbHeader(
    current: BreadcrumbOption,
    options: Vec<BreadcrumbOption>,
    children: Element,
    #[props(default = rsx! {})] actions: Element,
) -> Element {
    let label = current.label.clone();
    let leading = current.clone();
    rsx! {
        WorkspaceHeader { actions,
            Breadcrumbs { class: "min-w-0 flex-1 overflow-hidden".to_string(),
                BreadcrumbSelector {
                    label,
                    value: current.value.clone(),
                    options,
                    leading: rsx! {
                        BreadcrumbOptionIcon {
                            option: leading,
                            class: "size-4 shrink-0".to_string(),
                        }
                    },
                    onselect: PageNavigation::replace,
                }
                {children}
            }
        }
    }
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
            if let Some(icon) = icon {
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
    #[props(default)] autofocus: bool,
    #[props(default)] minimal: bool,
    #[props(default)] onsearch: Option<EventHandler<String>>,
    #[props(default)] ondebounced: Option<EventHandler<String>>,
    #[props(default = 150)] debounce_ms: u32,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default = rsx! {})] prefix: Element,
    #[props(default = rsx! {})] suffix: Element,
) -> Element {
    let mut draft = use_signal(|| value.clone());
    let mut pending = use_signal(|| None::<Task>);
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
    let class = if minimal {
        "relative z-30 flex h-7 min-w-0 flex-1 items-center gap-1.5 px-0.5"
    } else {
        "relative z-30 flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md bg-foreground/[0.025] px-2.5 ring-1 ring-inset ring-foreground/[0.08] focus-within:bg-foreground/[0.05] focus-within:ring-foreground/[0.16]"
    };
    rsx! {
        div { class,
            LineIconView {
                icon: LineIcon::Search,
                class: "size-3.5 shrink-0 text-muted-foreground",
            }
            {prefix}
            input {
                r#type: "search",
                class: "h-full min-w-0 flex-1 bg-transparent text-[11px] text-foreground outline-none placeholder:text-muted-foreground/55",
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
        "flex size-7 shrink-0 items-center justify-center rounded-md bg-foreground/[0.08] text-foreground"
    } else {
        "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
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
            LineIconView { icon: LineIcon::Plus, class: "size-3.5" }
            span { "{label}" }
        }
    }
}

#[component]
pub fn ManagerSearchPopover(
    open: bool,
    active: bool,
    label: String,
    #[props(default)] align_left: bool,
    ontoggle: EventHandler<()>,
    ondismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let align = if align_left {
        DropdownAlign::Start
    } else {
        DropdownAlign::End
    };
    rsx! {
        Dropdown {
            open,
            class: "shrink-0".to_string(),
            keyboard_navigation: true,
            on_dismiss: move |_| ondismiss.call(()),
            ManagerIconButton {
                label,
                active,
                onclick: move |_| ontoggle.call(()),
                LineIconView { icon: LineIcon::Search, class: "size-3.5".to_string() }
            }
            if open {
                DropdownPopup {
                    align,
                    class: "w-[min(28rem,calc(100vw-2rem))]",
                    {children}
                }
            }
        }
    }
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
        ManagerSearchPopover {
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
        div { class: "flex h-7 items-center gap-0.5 rounded-md bg-foreground/[0.025] p-0.5 ring-1 ring-inset ring-foreground/[0.07]",
            button {
                r#type: "button",
                aria_label: list_label,
                aria_pressed: view == ManagerView::List,
                class: if view == ManagerView::List {
                    "flex h-6 items-center gap-1.5 rounded-md bg-foreground/[0.08] px-2 text-[11px] font-medium text-foreground"
                } else {
                    "flex h-6 items-center gap-1.5 rounded-md px-2 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
                },
                onclick: move |_| onchange.call(ManagerView::List),
                LineIconView { icon: LineIcon::List, class: "size-3.5" }
                span { class: "hidden sm:inline", "{list_label}" }
            }
            button {
                r#type: "button",
                aria_label: grid_label,
                aria_pressed: view == ManagerView::Grid,
                class: if view == ManagerView::Grid {
                    "flex h-6 items-center gap-1.5 rounded-md bg-foreground/[0.08] px-2 text-[11px] font-medium text-foreground"
                } else {
                    "flex h-6 items-center gap-1.5 rounded-md px-2 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
                },
                onclick: move |_| onchange.call(ManagerView::Grid),
                LineIconView { icon: grid_icon, class: "size-3.5" }
                span { class: "hidden sm:inline", "{grid_label}" }
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
            ontoggle: move |_| open.set(!open()),
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
