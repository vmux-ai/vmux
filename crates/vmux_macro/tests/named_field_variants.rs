use vmux_macro::OsSubMenu;

#[derive(OsSubMenu, Debug, Clone, PartialEq, Eq)]
enum Sample {
    #[menu(id = "sample_a", label = "Sample A")]
    A { url: Option<String> },
    #[menu(id = "sample_b", label = "Sample B")]
    B { name: Option<String> },
}

#[test]
fn from_menu_id_returns_variant_with_default_fields() {
    let parsed = Sample::from_menu_id("sample_a").expect("sample_a should resolve");
    assert_eq!(parsed, Sample::A { url: None });
}

#[test]
fn from_menu_id_handles_multiple_named_field_variants() {
    let parsed = Sample::from_menu_id("sample_b").expect("sample_b should resolve");
    assert_eq!(parsed, Sample::B { name: None });
}
