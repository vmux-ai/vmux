#[allow(dead_code)]
mod shortcut {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct Modifiers {
        pub ctrl: bool,
        pub shift: bool,
        pub alt: bool,
        pub super_key: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct KeyCombo {
        pub key: u32,
        pub modifiers: Modifiers,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum Shortcut {
        Direct(KeyCombo),
        Chord(KeyCombo, KeyCombo),
    }
}

use vmux_macro::{DefaultShortcuts, OsSubMenu};

#[derive(OsSubMenu, DefaultShortcuts, Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum Sample {
    #[menu(id = "sample_a", label = "Sample A")]
    A { url: Option<String> },
}

#[test]
fn default_shortcuts_includes_named_field_variant() {
    let pairs = Sample::default_shortcuts();
    assert!(pairs.is_empty());
}
