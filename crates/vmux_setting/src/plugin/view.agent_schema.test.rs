use super::*;

#[test]
fn schema_exposes_run_placement_override_under_agent() {
    let schema = build_settings_schema();
    assert!(schema.sections.iter().any(|section| section.id == "agent"));
    let field = schema
        .field("agent.allow_run_placement_override")
        .expect("run placement override field");
    assert_eq!(field.label.as_deref(), Some("Allow run placement override"));
}
