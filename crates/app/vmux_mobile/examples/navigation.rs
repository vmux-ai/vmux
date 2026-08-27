//! What the phone makes of a Mac tab: splits flatten in reading order, the url picks
//! the page, and a stack it has no page for is mirrored rather than dropped.

use vmux_mobile::nav::{NavTab, Screen};
use vmux_wire::protocol::layout::{LayoutNode, SplitDirection, Stack, Tab};

fn main() {
    for entry in NavTab::all_in(&split_tab()) {
        println!("{:<34} {}", entry.name, Drawn::by(&entry.screen));
    }
}

struct Drawn;

impl Drawn {
    fn by(screen: &Screen) -> &'static str {
        match screen {
            Screen::Chat { .. } => "vmux_chat",
            Screen::Team => "vmux_team",
            Screen::Launcher => "vmux_start",
            Screen::Mirror(_) => "mirrored, read-only",
        }
    }
}

fn split_tab() -> Tab {
    Tab {
        id: Some("tab:1".into()),
        name: "Tab 1".into(),
        is_active: true,
        root: LayoutNode::Split {
            id: None,
            direction: SplitDirection::Row,
            flex_weights: vec![0.5, 0.5],
            children: vec![
                pane(&["vmux://agent/vibe/abc123", "vmux://team"]),
                pane(&["vmux://terminal/7", "file:///tmp/notes.md"]),
            ],
        },
    }
}

fn pane(urls: &[&str]) -> LayoutNode {
    let mut stacks = Vec::new();
    for url in urls {
        stacks.push(Stack {
            id: Some(format!("stack:{url}")),
            url: (*url).to_string(),
            ..Default::default()
        });
    }
    LayoutNode::Pane {
        id: None,
        is_zoomed: false,
        stacks,
    }
}
