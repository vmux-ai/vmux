use vmux_macro::{CommandBar, DefaultShortcuts, OsSubMenu};

pub use vmux_command::shortcut;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneDirection {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum Sample {
    #[menu(
        expand = "direction",
        id_template = "sample_in_pane_{dir}",
        label_template = "In Pane {Dir}"
    )]
    #[shortcut(
        expand = "direction",
        top = "Super+Shift+K",
        right = "Super+Shift+L",
        bottom = "Super+Shift+J",
        left = "Super+Shift+H"
    )]
    InPane {
        direction: PaneDirection,
        url: Option<String>,
    },
}

#[test]
fn expand_generates_four_menu_ids() {
    for dir in ["top", "right", "bottom", "left"] {
        let id = format!("sample_in_pane_{dir}");
        assert!(
            Sample::from_menu_id(&id).is_some(),
            "expected {id} to resolve via from_menu_id",
        );
    }
}

#[test]
fn expand_generates_four_shortcuts() {
    let shortcuts = Sample::default_shortcuts();
    let ids: Vec<_> = shortcuts.iter().map(|(_, id)| id.clone()).collect();
    assert!(ids.contains(&"sample_in_pane_top".to_string()));
    assert!(ids.contains(&"sample_in_pane_right".to_string()));
    assert!(ids.contains(&"sample_in_pane_bottom".to_string()));
    assert!(ids.contains(&"sample_in_pane_left".to_string()));
}

#[test]
fn expand_generates_command_bar_entries() {
    let entries = Sample::command_bar_entries();
    let ids: Vec<_> = entries.iter().map(|(id, _, _)| id.to_string()).collect();
    assert!(ids.contains(&"sample_in_pane_top".to_string()));
    assert!(ids.contains(&"sample_in_pane_right".to_string()));
    assert!(ids.contains(&"sample_in_pane_bottom".to_string()));
    assert!(ids.contains(&"sample_in_pane_left".to_string()));
}
