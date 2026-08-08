//! The `vmux://agent` chat page: a native Dioxus UI that renders an agent session's
//! conversation + run-state (pushed from ECS) and sends prompt/approval intents back.
//! This is the single agent front-end; it replaced the legacy CLI-install setup page.

#[cfg(any(test, frontend))]
pub(crate) mod composer;
pub mod event;

#[cfg(frontend)]
pub mod page;
#[cfg(frontend)]
mod scroll;

#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub use plugin::*;

// Tool-call arguments flattened into rows a person can read. Shared rather than desktop-only:
// the page renders them, and nothing in the plugin does.
#[cfg(any(test, frontend))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ApprovalDetail {
    label: String,
    value: String,
}

#[cfg(any(test, frontend))]
fn approval_details(args_json: &str) -> Vec<ApprovalDetail> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(args_json) else {
        return if args_json.trim().is_empty() {
            Vec::new()
        } else {
            vec![ApprovalDetail {
                label: "Details".to_string(),
                value: args_json.to_string(),
            }]
        };
    };
    let mut details = Vec::new();
    flatten_approval_details("", &value, &mut details);
    details
}

#[cfg(any(test, frontend))]
fn flatten_approval_details(
    path: &str,
    value: &serde_json::Value,
    details: &mut Vec<ApprovalDetail>,
) {
    if let serde_json::Value::Object(fields) = value {
        for (name, value) in fields {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            flatten_approval_details(&child_path, value, details);
        }
        return;
    }
    let label = approval_detail_label(path);
    let value = match value {
        serde_json::Value::String(value) => value.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    details.push(ApprovalDetail { label, value });
}

#[cfg(any(test, frontend))]
fn approval_detail_label(path: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_details_parse_nested_json() {
        assert_eq!(
            approval_details(
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
        assert!(approval_details("{}").is_empty());
    }
}
