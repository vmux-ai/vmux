use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsSchema {
    #[serde(default)]
    pub sections: Vec<SectionSpec>,
    #[serde(default)]
    pub fields: Vec<(String, FieldSpec)>,
}

impl SettingsSchema {
    pub fn field(&self, path: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|(p, _)| p == path).map(|(_, s)| s)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub synthetic_keys: Vec<String>,
    #[serde(default)]
    pub root_path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub widget: Option<WidgetKind>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub omit: bool,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub options: Vec<SelectOption>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WidgetKind {
    LeaderKbd,
    BindingsList,
    Select,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[cfg(test)]
mod select_widget_tests {
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
}
