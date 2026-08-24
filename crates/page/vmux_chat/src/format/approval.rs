#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDetail {
    pub label: String,
    pub value: String,
}

impl ApprovalDetail {
    pub fn rows(args_json: &str) -> Vec<Self> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(args_json) else {
            if args_json.trim().is_empty() {
                return Vec::new();
            }
            return vec![Self {
                label: "Details".to_string(),
                value: args_json.to_string(),
            }];
        };
        let mut details = Vec::new();
        Self::flatten("", &value, &mut details);
        details
    }

    fn flatten(path: &str, value: &serde_json::Value, details: &mut Vec<Self>) {
        if let serde_json::Value::Object(fields) = value {
            for (name, value) in fields {
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}.{name}")
                };
                Self::flatten(&child_path, value, details);
            }
            return;
        }
        let value = match value {
            serde_json::Value::String(value) => value.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
        };
        details.push(Self {
            label: Self::label(path),
            value,
        });
    }

    fn label(path: &str) -> String {
        let path = path.strip_prefix("arguments.").unwrap_or(path);
        let label = if path.is_empty() { "details" } else { path };
        label
            .split('.')
            .map(|part| {
                let words = part.replace('_', " ");
                let mut chars = words.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_details_parse_nested_json() {
        assert_eq!(
            ApprovalDetail::rows(
                r#"{"arguments":{"path":"/tmp/SKILL.md"},"server":"vmux","tool":"read_file"}"#
            ),
            vec![
                ApprovalDetail {
                    label: "Path".into(),
                    value: "/tmp/SKILL.md".into(),
                },
                ApprovalDetail {
                    label: "Server".into(),
                    value: "vmux".into(),
                },
                ApprovalDetail {
                    label: "Tool".into(),
                    value: "read_file".into(),
                },
            ]
        );
        assert!(ApprovalDetail::rows("{}").is_empty());
    }
}
