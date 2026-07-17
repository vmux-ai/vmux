use serde_json::Value;
use std::path::Path;

use crate::extension::match_pattern::{ChromeMatchPattern, is_match_pattern_candidate};

const MAX_PERMISSION_COUNT: usize = 256;
const MAX_PERMISSION_LENGTH: usize = 1024;
const MAX_PERMISSION_BYTES: usize = 64 * 1024;

pub fn prepare_unpacked(dir: &Path, key_b64: &str, popup: Option<&str>) -> Result<(), String> {
    let path = dir.join("manifest.json");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let Some(obj) = v.as_object_mut() else {
        return Err("manifest is not an object".into());
    };
    let mut changed = false;
    if !obj.contains_key("key") {
        obj.insert("key".to_string(), Value::String(key_b64.to_string()));
        changed = true;
    }
    if let Some(popup) = popup {
        add_web_accessible(obj, popup);
        changed = true;
    }
    if changed {
        let out = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        std::fs::write(&path, out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn add_web_accessible(obj: &mut serde_json::Map<String, Value>, popup: &str) {
    let key = "web_accessible_resources";
    let mv3 = obj.get("manifest_version").and_then(Value::as_u64) == Some(3);
    let new_entry = if mv3 {
        serde_json::json!({ "resources": [popup], "matches": ["<all_urls>"] })
    } else {
        Value::String(popup.to_string())
    };
    match obj.get_mut(key) {
        Some(Value::Array(arr)) => arr.push(new_entry),
        _ => {
            obj.insert(key.to_string(), Value::Array(vec![new_entry]));
        }
    }
}

pub fn resolve_name(dir: &Path, m: &ExtManifest) -> String {
    let raw = m.name.trim();
    if let Some(msg_key) = raw
        .strip_prefix("__MSG_")
        .and_then(|s| s.strip_suffix("__"))
        && let Some(locale) = m.default_locale.as_deref()
        && let Some(resolved) = read_message(dir, locale, msg_key)
    {
        return resolved;
    }
    raw.to_string()
}

fn read_message(dir: &Path, locale: &str, key: &str) -> Option<String> {
    let path = dir.join("_locales").join(locale).join("messages.json");
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let obj = v.as_object()?;
    let entry = obj.get(key).or_else(|| {
        obj.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    })?;
    entry.get("message")?.as_str().map(str::to_string)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtManifest {
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
    pub default_locale: Option<String>,
    pub permissions: Vec<String>,
    pub optional_permissions: Vec<String>,
    pub host_permissions: Vec<String>,
    pub optional_host_permissions: Vec<String>,
}

pub fn parse(json: &str) -> Result<ExtManifest, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let manifest_version = v
        .get("manifest_version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .filter(|version| matches!(version, 2 | 3))
        .ok_or("manifest_version must be 2 or 3")?;
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let version = v
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    validate_version(&version)?;
    let action = v.get("action").or_else(|| v.get("browser_action"));
    let popup = action
        .and_then(|a| a.get("default_popup"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let icon = action
        .and_then(|a| a.get("default_icon"))
        .and_then(pick_icon);
    let default_locale = v
        .get("default_locale")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(path) = popup.as_deref() {
        validate_resource_path("popup", path)?;
    }
    if let Some(path) = icon.as_deref() {
        validate_resource_path("icon", path)?;
    }
    if let Some(locale) = default_locale.as_deref()
        && (locale.is_empty()
            || !locale
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    {
        return Err("manifest default_locale is invalid".into());
    }
    let mut permissions = string_array(&v, "permissions")?;
    let mut optional_permissions = string_array(&v, "optional_permissions")?;
    let mut host_permissions = string_array(&v, "host_permissions")?;
    let mut optional_host_permissions = string_array(&v, "optional_host_permissions")?;
    if manifest_version == 2 {
        if !host_permissions.is_empty() || !optional_host_permissions.is_empty() {
            return Err("manifest version 2 cannot declare host_permissions fields".into());
        }
        host_permissions.extend(
            permissions
                .extract_if(.., |permission| is_host_permission(permission))
                .collect::<Vec<_>>(),
        );
        optional_host_permissions.extend(
            optional_permissions
                .extract_if(.., |permission| is_host_permission(permission))
                .collect::<Vec<_>>(),
        );
    } else if permissions
        .iter()
        .any(|permission| is_host_permission(permission))
        || optional_permissions
            .iter()
            .any(|permission| is_host_permission(permission))
    {
        return Err("manifest version 3 host patterns must use host_permissions fields".into());
    }
    for pattern in host_permissions
        .iter()
        .chain(optional_host_permissions.iter())
    {
        ChromeMatchPattern::parse(pattern)?;
    }
    host_permissions.sort();
    host_permissions.dedup();
    optional_host_permissions.sort();
    optional_host_permissions.dedup();
    Ok(ExtManifest {
        manifest_version,
        name,
        version,
        popup,
        icon,
        default_locale,
        permissions,
        optional_permissions,
        host_permissions,
        optional_host_permissions,
    })
}

fn validate_version(version: &str) -> Result<(), String> {
    let components = version.split('.').collect::<Vec<_>>();
    if components.is_empty() || components.len() > 4 {
        return Err("manifest version must contain one to four components".into());
    }
    for component in components {
        if component.is_empty()
            || !component.bytes().all(|byte| byte.is_ascii_digit())
            || (component.len() > 1 && component.starts_with('0'))
            || component.parse::<u16>().is_err()
        {
            return Err("manifest version component is invalid".into());
        }
    }
    Ok(())
}

fn validate_resource_path(kind: &str, path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || std::path::Path::new(path).components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(format!("manifest {kind} path is invalid"));
    }
    Ok(())
}

fn string_array(value: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = value.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("manifest {key} must be an array"))?;
    if values.len() > MAX_PERMISSION_COUNT {
        return Err(format!("manifest {key} exceeds permission count limit"));
    }
    let mut total = 0;
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .ok_or_else(|| format!("manifest {key} entries must be strings"))?;
            if value.len() > MAX_PERMISSION_LENGTH {
                return Err(format!("manifest {key} entry exceeds length limit"));
            }
            total += value.len();
            if total > MAX_PERMISSION_BYTES {
                return Err(format!("manifest {key} exceeds byte limit"));
            }
            Ok(value.to_string())
        })
        .collect()
}

fn is_host_permission(permission: &str) -> bool {
    is_match_pattern_candidate(permission)
}

fn pick_icon(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    let map = v.as_object()?;
    let mut best: Option<(u32, String)> = None;
    for (k, val) in map {
        let (Ok(size), Some(path)) = (k.parse::<u32>(), val.as_str()) else {
            continue;
        };
        let prefer = size <= 48;
        let better = match &best {
            None => true,
            Some((bsize, _)) => {
                let bprefer = *bsize <= 48;
                match (prefer, bprefer) {
                    (true, false) => true,
                    (false, true) => false,
                    (true, true) => size > *bsize,
                    (false, false) => size > *bsize,
                }
            }
        };
        if better {
            best = Some((size, path.to_string()));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mv3_action() {
        let m = parse(
            r#"{
            "manifest_version": 3,
            "name": "uBlock", "version": "1.6",
            "action": { "default_popup": "popup.html", "default_icon": { "16": "i16.png", "32": "i32.png" } }
        }"#,
        )
        .unwrap();
        assert_eq!(m.name, "uBlock");
        assert_eq!(m.version, "1.6");
        assert_eq!(m.popup.as_deref(), Some("popup.html"));
        assert_eq!(m.icon.as_deref(), Some("i32.png"));
    }

    #[test]
    fn rejects_unsafe_version_and_resource_paths() {
        for manifest in [
            r#"{"manifest_version":3,"name":"x","version":"../1"}"#,
            r#"{"manifest_version":3,"name":"x","version":"01"}"#,
            r#"{"manifest_version":3,"name":"x","version":"1.2.3.4.5"}"#,
            r#"{"manifest_version":3,"name":"x","version":"1","action":{"default_icon":"../secret"}}"#,
            r#"{"manifest_version":3,"name":"x","version":"1","action":{"default_popup":"/tmp/page.html"}}"#,
        ] {
            assert!(parse(manifest).is_err(), "accepted {manifest}");
        }
    }

    #[test]
    fn parses_api_and_host_permissions() {
        let m = parse(
            r#"{
                "name": "x",
                "version": "1",
                "manifest_version": 3,
                "permissions": ["storage"],
                "optional_permissions": ["history"],
                "host_permissions": ["https://example.com/*"],
                "optional_host_permissions": ["https://optional.example/*"]
            }"#,
        )
        .unwrap();

        assert_eq!(m.permissions, ["storage"]);
        assert_eq!(m.optional_permissions, ["history"]);
        assert_eq!(m.host_permissions, ["https://example.com/*"]);
        assert_eq!(m.optional_host_permissions, ["https://optional.example/*"]);
    }

    #[test]
    fn parses_mv2_host_permissions_from_permissions() {
        let m = parse(
            r#"{
                "manifest_version": 2,
                "name": "x",
                "version": "1",
                "permissions": ["storage", "https://legacy.example/*"],
                "optional_permissions": ["history", "https://legacy-optional.example/*"]
            }"#,
        )
        .unwrap();

        assert_eq!(m.permissions, ["storage"]);
        assert_eq!(m.optional_permissions, ["history"]);
        assert_eq!(m.host_permissions, ["https://legacy.example/*"]);
        assert_eq!(
            m.optional_host_permissions,
            ["https://legacy-optional.example/*"]
        );
    }

    #[test]
    fn parses_mv2_browser_action_and_string_icon() {
        let m = parse(
            r#"{
            "manifest_version": 2,
            "name": "x", "version": "2",
            "browser_action": { "default_popup": "p.html", "default_icon": "icon.png" }
        }"#,
        )
        .unwrap();
        assert_eq!(m.popup.as_deref(), Some("p.html"));
        assert_eq!(m.icon.as_deref(), Some("icon.png"));
    }

    #[test]
    fn no_action_means_no_icon() {
        let m = parse(r#"{ "manifest_version": 3, "name": "bg", "version": "1", "icons": { "48": "x.png" } }"#).unwrap();
        assert!(m.popup.is_none());
        assert!(m.icon.is_none());
    }

    #[test]
    fn picks_largest_within_48() {
        let m = parse(
            r#"{ "manifest_version": 3, "name": "x", "version": "1", "action": { "default_icon": { "16": "a.png", "48": "b.png", "128": "c.png" } } }"#,
        )
        .unwrap();
        assert_eq!(m.icon.as_deref(), Some("b.png"));
    }

    #[test]
    fn falls_back_to_largest_above_48() {
        let m = parse(
            r#"{ "manifest_version": 3, "name": "x", "version": "1", "action": { "default_icon": { "64": "a.png", "128": "b.png" } } }"#,
        )
        .unwrap();
        assert_eq!(m.icon.as_deref(), Some("b.png"));
    }
}
