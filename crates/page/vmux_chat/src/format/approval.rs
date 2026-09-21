#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDetail {
    pub label: String,
    pub value: String,
}

impl ApprovalDetail {
    pub fn rows(value: &vmux_api::json::JsonValue) -> Vec<Self> {
        let mut details = Vec::new();
        Self::flatten("", value, &mut details);
        details
    }

    fn flatten(path: &str, value: &vmux_api::json::JsonValue, details: &mut Vec<Self>) {
        if let vmux_api::json::JsonValue::Object(fields) = value {
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
            vmux_api::json::JsonValue::String(value) => value.clone(),
            other => other
                .to_serde()
                .and_then(|value| serde_json::to_string_pretty(&value))
                .unwrap_or_default(),
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
        let args = vmux_api::json::JsonValue::parse(
            r#"{"arguments":{"path":"/tmp/SKILL.md"},"server":"vmux","tool":"read_file"}"#,
        )
        .unwrap();
        assert_eq!(
            ApprovalDetail::rows(&args),
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
        assert!(ApprovalDetail::rows(&vmux_api::json::JsonValue::Object(Vec::new())).is_empty());
    }
}
