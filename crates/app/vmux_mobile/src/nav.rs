use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_wire::protocol::layout::{LayoutNode, LayoutSnapshot, Stack, Tab};

use crate::transition::NativeStack;

#[derive(Clone, Debug, PartialEq)]
pub struct NavTab {
    pub id: String,
    pub name: String,
    pub screen: Screen,
    pub local: bool,
}

impl NavTab {
    pub fn all_in(tab: &Tab) -> Vec<Self> {
        let mut stacks = Vec::new();
        collect(&tab.root, &mut stacks);
        let mut entries = Vec::with_capacity(stacks.len());
        for stack in stacks {
            let id = stack.id.clone().unwrap_or_else(|| stack.url.clone());
            let screen = Screen::of(stack);
            entries.push(Self {
                id,
                name: screen.title(),
                screen,
                local: false,
            });
        }
        entries
    }

    pub(crate) fn root(&self) -> Screen {
        self.screen.clone()
    }

    fn blank(ordinal: u64) -> Self {
        let screen = Screen::Launcher;
        Self {
            id: format!("local:{ordinal}"),
            name: screen.title(),
            screen,
            local: true,
        }
    }

    fn sid(&self) -> Option<&str> {
        match &self.screen {
            Screen::Chat { sid, .. } => sid.as_deref(),
            _ => None,
        }
    }
}

fn collect(node: &LayoutNode, out: &mut Vec<Stack>) {
    match node {
        LayoutNode::Split { children, .. } => {
            for child in children {
                collect(child, out);
            }
        }
        LayoutNode::Pane { stacks, .. } => {
            for stack in stacks {
                out.push(stack.clone());
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Chat { sid: Option<String>, title: String },
    Team,
    Launcher,
    Mirror(Stack),
}

impl Screen {
    fn of(stack: Stack) -> Self {
        let url = stack.url.trim_end_matches('/');
        if let Some(rest) = url.strip_prefix("vmux://agent/") {
            let sid = rest.split('/').nth(1).map(str::to_string);
            return Self::Chat {
                sid,
                title: stack.title,
            };
        }
        if url == "vmux://team" {
            return Self::Team;
        }
        if url == "vmux://start" {
            return Self::Launcher;
        }
        Self::Mirror(stack)
    }

    pub(crate) fn addressed(url: &str) -> Self {
        Self::of(Stack {
            id: None,
            title: String::new(),
            url: url.to_string(),
            kind: String::new(),
            is_loading: false,
            icon: Default::default(),
            is_self: false,
            process_id: None,
        })
    }

    pub(crate) fn has_own_input(&self) -> bool {
        matches!(self, Self::Chat { .. } | Self::Launcher)
    }

    pub(crate) fn title(&self) -> String {
        match self {
            Self::Chat { title, .. } if !title.is_empty() => title.clone(),
            Self::Chat { .. } => vmux_ui::i18n::translate("mobile-nav-untitled-chat"),
            Self::Team => vmux_ui::i18n::translate("mobile-start-team"),
            Self::Launcher => vmux_ui::i18n::translate("mobile-nav-launcher"),
            Self::Mirror(stack) if !stack.title.is_empty() => stack.title.clone(),
            Self::Mirror(stack) if !stack.url.is_empty() => stack.url.clone(),
            Self::Mirror(stack) => stack.kind.clone(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Nav {
    tabs: Signal<Vec<NavTab>>,
    local: Signal<Vec<NavTab>>,
    opened: Signal<u64>,
    selected: Signal<Option<String>>,
    pushed: Signal<HashMap<String, Vec<Screen>>>,
}

pub(crate) fn use_nav() -> Nav {
    Nav {
        tabs: use_signal(Vec::new),
        local: use_signal(Vec::new),
        opened: use_signal(|| 0),
        selected: use_signal(|| None),
        pushed: use_signal(HashMap::new),
    }
}

impl Nav {
    pub(crate) fn tabs(&self) -> Vec<NavTab> {
        let mut all = self.tabs.read().clone();
        all.extend(self.local.read().iter().cloned());
        all
    }

    pub(crate) fn open_blank(&self) {
        let mut opened = self.opened;
        let ordinal = opened();
        opened.set(ordinal.wrapping_add(1));
        let tab = NavTab::blank(ordinal);
        let id = tab.id.clone();
        let mut local = self.local;
        local.write().push(tab);
        self.select(&id);
    }

    pub(crate) fn selected(&self) -> Option<NavTab> {
        let tabs = self.tabs.read();
        let selected = self.selected.read();
        if let Some(id) = selected.as_ref() {
            for tab in tabs.iter() {
                if &tab.id == id {
                    return Some(tab.clone());
                }
            }
        }
        tabs.first().cloned()
    }

    pub(crate) fn current(&self) -> Screen {
        let Some(tab) = self.selected() else {
            return Screen::Launcher;
        };
        match self.pushed.read().get(&tab.id).and_then(|s| s.last()) {
            Some(screen) => screen.clone(),
            None => tab.root(),
        }
    }

    pub(crate) fn depth(&self) -> usize {
        let Some(tab) = self.selected() else {
            return 0;
        };
        self.pushed.read().get(&tab.id).map_or(0, Vec::len)
    }

    pub(crate) fn select(&self, id: &str) {
        let mut selected = self.selected;
        selected.set(Some(id.to_string()));
    }

    pub(crate) fn open(&self, screen: Screen) {
        if self.replaced(&screen) {
            return;
        }
        let title = screen.title();
        let pushing = NativeStack::push();
        self.push(screen);
        pushing.finish(title);
    }

    fn replaced(&self, screen: &Screen) -> bool {
        let Some(tab) = self.selected() else {
            return false;
        };
        if !tab.local || self.depth() > 0 {
            return false;
        }
        let mut local = self.local;
        let mut held = local.write();
        let Some(entry) = held.iter_mut().find(|entry| entry.id == tab.id) else {
            return false;
        };
        entry.name = screen.title();
        entry.screen = screen.clone();
        true
    }

    pub(crate) fn back(&self) {
        let popping = NativeStack::pop();
        self.pop();
        popping.finish();
    }

    fn push(&self, screen: Screen) {
        let Some(tab) = self.selected() else {
            return;
        };
        let mut pushed = self.pushed;
        pushed.write().entry(tab.id).or_default().push(screen);
    }

    pub(crate) fn pop(&self) {
        let Some(tab) = self.selected() else {
            return;
        };
        let mut pushed = self.pushed;
        let mut held = pushed.write();
        let Some(stack) = held.get_mut(&tab.id) else {
            return;
        };
        stack.pop();
    }

    pub(crate) fn apply(&self, snapshot: &LayoutSnapshot) {
        let mut next = Vec::new();
        for tab in &snapshot.tabs {
            next.extend(NavTab::all_in(tab));
        }

        let mut local = self.local;
        local.write().retain(|entry| !Self::mirrored(entry, &next));
        let mut pushed = self.pushed;
        let live: Vec<String> = next
            .iter()
            .map(|tab| tab.id.clone())
            .chain(local.read().iter().map(|tab| tab.id.clone()))
            .collect();
        pushed.write().retain(|id, _| live.contains(id));

        let mut selected = self.selected;
        let still_there = match selected.read().as_ref() {
            Some(id) => live.contains(id),
            None => false,
        };
        if !still_there {
            let landed = Self::landing(&next, snapshot.focused.stack.as_deref())
                .or_else(|| local.read().first().map(|tab| tab.id.clone()));
            selected.set(landed);
        }

        let mut tabs = self.tabs;
        tabs.set(next);
    }

    fn mirrored(entry: &NavTab, reported: &[NavTab]) -> bool {
        let Some(sid) = entry.sid() else {
            return false;
        };
        reported.iter().any(|tab| tab.sid() == Some(sid))
    }

    fn landing(tabs: &[NavTab], focused: Option<&str>) -> Option<String> {
        if let Some(id) = focused {
            for tab in tabs {
                if tab.id == id {
                    return Some(tab.id.clone());
                }
            }
        }
        tabs.first().map(|tab| tab.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Reported;

    impl Reported {
        fn stack(url: &str) -> Stack {
            Stack {
                id: Some(format!("stack:{url}")),
                title: String::new(),
                url: url.to_string(),
                kind: "browser".to_string(),
                is_loading: false,
                icon: Default::default(),
                is_self: false,
                process_id: None,
            }
        }

        fn pane(urls: &[&str]) -> LayoutNode {
            let mut stacks = Vec::new();
            for url in urls {
                stacks.push(Self::stack(url));
            }
            LayoutNode::Pane {
                id: None,
                is_zoomed: false,
                stacks,
            }
        }

        fn tab(id: &str, urls: &[&str]) -> Tab {
            Tab {
                id: Some(id.to_string()),
                name: id.to_string(),
                is_active: false,
                root: Self::pane(urls),
            }
        }

        fn split_tab(id: &str, left: &[&str], right: &[&str]) -> Tab {
            let mut tab = Self::tab(id, &[]);
            tab.root = LayoutNode::Split {
                id: None,
                direction: vmux_wire::protocol::layout::SplitDirection::Row,
                flex_weights: vec![0.5, 0.5],
                children: vec![Self::pane(left), Self::pane(right)],
            };
            tab
        }
    }

    #[test]
    fn a_split_becomes_one_entry_per_stack_in_reading_order() {
        let entries = NavTab::all_in(&Reported::split_tab(
            "tab:1",
            &["vmux://team"],
            &["vmux://terminal/1", "vmux://start"],
        ));
        let screens: Vec<Screen> = entries.iter().map(|entry| entry.screen.clone()).collect();
        assert_eq!(
            screens,
            vec![
                Screen::Team,
                Screen::Mirror(Reported::stack("vmux://terminal/1")),
                Screen::Launcher,
            ]
        );
    }

    #[test]
    fn an_agent_url_names_a_conversation_only_when_it_carries_a_session() {
        let attached = Screen::of(Reported::stack("vmux://agent/vibe/abc123"));
        assert_eq!(
            attached,
            Screen::Chat {
                sid: Some("abc123".to_string()),
                title: String::new(),
            }
        );

        let bare = Screen::of(Reported::stack("vmux://agent/vibe"));
        assert_eq!(
            bare,
            Screen::Chat {
                sid: None,
                title: String::new(),
            }
        );
    }

    #[test]
    fn an_unrenderable_stack_is_mirrored_rather_than_dropped() {
        let editor = Reported::stack("file:///tmp/notes.md");
        assert_eq!(Screen::of(editor.clone()), Screen::Mirror(editor));
    }

    #[test]
    fn landing_prefers_the_mac_focus_and_falls_back_to_the_first_entry() {
        let mut tabs = NavTab::all_in(&Reported::tab("tab:1", &["vmux://start"]));
        tabs.extend(NavTab::all_in(&Reported::tab("tab:2", &["vmux://team"])));
        let (first, second) = (tabs[0].id.clone(), tabs[1].id.clone());

        assert_eq!(Nav::landing(&tabs, Some(&second)), Some(second));
        assert_eq!(Nav::landing(&tabs, None), Some(first.clone()));
        assert_eq!(Nav::landing(&tabs, Some("stack:99")), Some(first));
        assert_eq!(Nav::landing(&[], Some("stack:1")), None);
    }

    #[test]
    fn a_local_tab_gives_way_once_the_mac_reports_its_conversation() {
        let mut begun = NavTab::blank(0);
        begun.screen = Screen::Chat {
            sid: Some("abc123".to_string()),
            title: String::new(),
        };
        let reported = NavTab::all_in(&Reported::tab("tab:1", &["vmux://agent/vibe/abc123"]));

        assert!(Nav::mirrored(&begun, &reported));
        let elsewhere = NavTab::all_in(&Reported::tab("tab:1", &["vmux://agent/vibe/zzz999"]));
        assert!(!Nav::mirrored(&begun, &elsewhere));
    }

    #[test]
    fn a_blank_tab_survives_every_poll() {
        let reported = NavTab::all_in(&Reported::tab("tab:1", &["vmux://agent/vibe/abc123"]));
        assert!(!Nav::mirrored(&NavTab::blank(0), &reported));
        assert!(!Nav::mirrored(&NavTab::blank(0), &[]));
    }

    #[test]
    fn a_tab_with_no_stacks_contributes_no_entries() {
        assert!(NavTab::all_in(&Reported::tab("tab:1", &[])).is_empty());
    }

    #[test]
    fn an_entry_is_labelled_by_the_stack_it_holds() {
        let mut tab = Reported::tab("tab:1", &["vmux://terminal/7"]);
        tab.name = "Tab 2".to_string();
        assert_eq!(NavTab::all_in(&tab)[0].name, "vmux://terminal/7");
    }
}
