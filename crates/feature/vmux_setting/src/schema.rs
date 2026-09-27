use crate::state::SettingsSelectOption;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsSchema {
    pub sections: Vec<SectionSpec>,
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

#[derive(Clone, Debug, PartialEq)]
pub struct SectionSpec {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub synthetic_keys: Vec<String>,
    pub root_path: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldSpec {
    pub label: Option<String>,
    pub description: Option<String>,
    pub hint: Option<String>,
    pub placeholder: Option<String>,
    pub widget: Option<WidgetKind>,
    pub order: Vec<String>,
    pub omit: bool,
    pub step: Option<f64>,
    pub options: Vec<SettingsSelectOption>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WidgetKind {
    LeaderKbd,
    BindingsList,
    Select,
}

#[cfg(test)]
mod tests {
    use super::*;

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
