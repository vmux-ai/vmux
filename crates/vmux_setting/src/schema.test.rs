use super::*;

#[test]
fn select_field_with_options_round_trips_json() {
    let spec = FieldSpec {
        label: Some("Mode".into()),
        widget: Some(WidgetKind::Select),
        options: vec![
            SelectOption {
                value: "device".into(),
                label: "Device".into(),
            },
            SelectOption {
                value: "light".into(),
                label: "Light".into(),
            },
        ],
        ..Default::default()
    };
    let json = serde_json::to_string(&spec).unwrap();
    let back: FieldSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(back.widget, Some(WidgetKind::Select));
    assert_eq!(back.options.len(), 2);
    assert_eq!(back.options[0].value, "device");
    assert_eq!(back.options[1].label, "Light");
}

#[test]
fn field_lookup_matches_array_indexes_and_dynamic_map_keys() {
    let schema = SettingsSchema {
        fields: vec![
            (
                "agent.acp[].command".into(),
                FieldSpec {
                    label: Some("Command".into()),
                    ..Default::default()
                },
            ),
            (
                "spaces.*.startup_url".into(),
                FieldSpec {
                    label: Some("Startup URL".into()),
                    ..Default::default()
                },
            ),
        ],
        ..Default::default()
    };
    assert_eq!(
        schema
            .field("agent.acp[2].command")
            .unwrap()
            .label
            .as_deref(),
        Some("Command")
    );
    assert_eq!(
        schema
            .field("spaces.personal.startup_url")
            .unwrap()
            .label
            .as_deref(),
        Some("Startup URL")
    );
}
