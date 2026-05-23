use vmux_macro::{CommandBar, OsSubMenu};

#[derive(OsSubMenu, CommandBar, Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum Sample {
    #[menu(id = "sample_a", label = "Sample A")]
    A { url: Option<String> },
}

#[test]
fn command_bar_entries_include_named_field_variant() {
    let entries = Sample::command_bar_entries();
    assert!(entries.iter().any(|(id, _, _)| *id == "sample_a"));
}
