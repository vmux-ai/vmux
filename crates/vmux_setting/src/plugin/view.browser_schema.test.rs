use super::*;

#[test]
fn schema_exposes_search_engine_select() {
    let schema = build_settings_schema();
    let field = schema
        .field("browser.search_engine")
        .expect("search engine field");
    assert_eq!(field.widget, Some(WidgetKind::Select));
    let values: Vec<_> = field
        .options
        .iter()
        .map(|option| option.value.as_str())
        .collect();
    assert_eq!(
        values,
        vec!["google", "bing", "duckduckgo", "brave", "kagi"]
    );
}
