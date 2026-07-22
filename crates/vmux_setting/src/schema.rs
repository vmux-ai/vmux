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
        self.fields
            .iter()
            .find(|(pattern, _)| pattern == path)
            .or_else(|| {
                let normalized = normalize_array_indexes(path);
                self.fields
                    .iter()
                    .find(|(pattern, _)| field_path_matches(pattern, &normalized))
            })
            .map(|(_, spec)| spec)
    }
}

fn normalize_array_indexes(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '[' {
            normalized.push(ch);
            continue;
        }
        let mut digits = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            digits.push(chars.next().unwrap());
        }
        if !digits.is_empty() && chars.next_if_eq(&']').is_some() {
            normalized.push_str("[]");
        } else {
            normalized.push('[');
            normalized.push_str(&digits);
        }
    }
    normalized
}

fn field_path_matches(pattern: &str, path: &str) -> bool {
    let pattern = normalize_array_indexes(pattern);
    let pattern_segments = pattern.split('.').collect::<Vec<_>>();
    let path_segments = path.split('.').collect::<Vec<_>>();
    pattern_segments.len() == path_segments.len()
        && pattern_segments
            .iter()
            .zip(path_segments)
            .all(|(pattern, segment)| *pattern == "*" || *pattern == segment)
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
}
