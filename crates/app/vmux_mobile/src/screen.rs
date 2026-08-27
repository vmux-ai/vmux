use vmux_wire::protocol::layout::{LayoutNode, LayoutSnapshot, Stack, Tab};

use crate::nav::Route;

#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Chat { sid: Option<String>, title: String },
    Team,
    Launcher,
    Mirror(Stack),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Name {
    Chat,
    Team,
    Launcher,
    Mirror,
}

impl Route for Shown {
    type Name = Name;

    fn name(&self) -> Name {
        match self {
            Self::Chat { sid: Some(_), .. } => Name::Chat,
            Self::Chat { sid: None, .. } | Self::Launcher => Name::Launcher,
            Self::Team => Name::Team,
            Self::Mirror(_) => Name::Mirror,
        }
    }

    fn title(&self) -> String {
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

    fn is(&self, other: &Self) -> bool {
        match (self.sid(), other.sid()) {
            (Some(mine), Some(theirs)) => mine == theirs,
            _ => false,
        }
    }
}

impl Shown {
    pub fn of(stack: Stack) -> Self {
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

    pub fn addressed(url: &str) -> Self {
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

    pub fn has_own_input(&self) -> bool {
        matches!(self, Self::Chat { .. } | Self::Launcher)
    }

    fn sid(&self) -> Option<&str> {
        match self {
            Self::Chat { sid, .. } => sid.as_deref(),
            _ => None,
        }
    }
}

pub struct Mac;

impl Mac {
    pub fn tabs(snapshot: &LayoutSnapshot) -> Vec<(String, Shown)> {
        let mut entries = Vec::new();
        for tab in &snapshot.tabs {
            entries.extend(Self::in_tab(tab));
        }
        entries
    }

    fn in_tab(tab: &Tab) -> Vec<(String, Shown)> {
        let mut stacks = Vec::new();
        Self::collect(&tab.root, &mut stacks);
        let mut entries = Vec::with_capacity(stacks.len());
        for stack in stacks {
            let id = stack.id.clone().unwrap_or_else(|| stack.url.clone());
            entries.push((id, Shown::of(stack)));
        }
        entries
    }

    fn collect(node: &LayoutNode, out: &mut Vec<Stack>) {
        match node {
            LayoutNode::Split { children, .. } => {
                for child in children {
                    Self::collect(child, out);
                }
            }
            LayoutNode::Pane { stacks, .. } => {
                for stack in stacks {
                    out.push(stack.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Mac {
        fn stack(url: &str) -> Stack {
            Stack {
                id: Some(format!("stack:{url}")),
                url: url.to_string(),
                kind: "browser".to_string(),
                ..Default::default()
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
        let entries = Mac::in_tab(&Mac::split_tab(
            "tab:1",
            &["vmux://team"],
            &["vmux://terminal/1", "vmux://start"],
        ));
        let screens: Vec<Shown> = entries.iter().map(|(_, screen)| screen.clone()).collect();
        assert_eq!(
            screens,
            vec![
                Shown::Team,
                Shown::Mirror(Mac::stack("vmux://terminal/1")),
                Shown::Launcher,
            ]
        );
    }

    #[test]
    fn an_agent_url_names_a_conversation_only_when_it_carries_a_session() {
        assert_eq!(
            Shown::of(Mac::stack("vmux://agent/vibe/abc123")),
            Shown::Chat {
                sid: Some("abc123".to_string()),
                title: String::new(),
            }
        );
        assert_eq!(
            Shown::of(Mac::stack("vmux://agent/vibe")),
            Shown::Chat {
                sid: None,
                title: String::new(),
            }
        );
    }

    #[test]
    fn an_unrenderable_stack_is_mirrored_rather_than_dropped() {
        let editor = Mac::stack("file:///tmp/notes.md");
        assert_eq!(Shown::of(editor.clone()), Shown::Mirror(editor));
    }

    #[test]
    fn a_tab_with_no_stacks_contributes_no_entries() {
        assert!(Mac::in_tab(&Mac::tab("tab:1", &[])).is_empty());
    }

    #[test]
    fn an_entry_is_labelled_by_the_stack_it_holds() {
        let mut tab = Mac::tab("tab:1", &["vmux://terminal/7"]);
        tab.name = "Tab 2".to_string();
        assert_eq!(Mac::in_tab(&tab)[0].1.title(), "vmux://terminal/7");
    }

    #[test]
    fn only_a_shared_session_makes_two_screens_the_same() {
        let attached = Shown::of(Mac::stack("vmux://agent/vibe/abc123"));
        let same = Shown::of(Mac::stack("vmux://agent/other/abc123"));
        let elsewhere = Shown::of(Mac::stack("vmux://agent/vibe/zzz999"));

        assert!(attached.is(&same));
        assert!(!attached.is(&elsewhere));
        assert!(!Shown::Launcher.is(&Shown::Launcher));
    }
}
