fn page_source() -> &'static str {
    include_str!("../src/page.rs")
}

#[test]
fn initial_load_does_not_schedule_the_search_debounce() {
    let source = page_source();
    let initial_effect = source
        .split("use_effect(move || {")
        .nth(1)
        .and_then(|rest| rest.split("    rsx! {").next())
        .expect("initial effect");

    assert!(initial_effect.contains("emit_query(\"\", 1, 0)"));
    assert!(!initial_effect.contains("schedule_query("));
}

#[test]
fn search_debounce_reads_generation_non_reactively() {
    let source = page_source();
    let scheduler = source
        .split("fn schedule_query(")
        .nth(1)
        .and_then(|rest| rest.split("#[component]").next())
        .expect("query scheduler");

    assert!(scheduler.contains("generation.peek()"));
    assert!(!scheduler.contains("generation()"));
}
