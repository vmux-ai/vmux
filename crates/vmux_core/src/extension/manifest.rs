use serde_json::Value;
use std::path::Path;

pub fn ensure_key(dir: &Path, key_b64: &str) -> Result<(), String> {
    let path = dir.join("manifest.json");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let Some(obj) = v.as_object_mut() else {
        return Err("manifest is not an object".into());
    };
    if obj.contains_key("key") {
        return Ok(());
    }
    obj.insert("key".to_string(), Value::String(key_b64.to_string()));
    let out = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtManifest {
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
}

pub fn parse(json: &str) -> Result<ExtManifest, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
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
    let action = v.get("action").or_else(|| v.get("browser_action"));
    let popup = action
        .and_then(|a| a.get("default_popup"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let icon = action
        .and_then(|a| a.get("default_icon"))
        .and_then(pick_icon);
    Ok(ExtManifest {
        name,
        version,
        popup,
        icon,
    })
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
    fn parses_mv2_browser_action_and_string_icon() {
        let m = parse(
            r#"{
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
        let m = parse(r#"{ "name": "bg", "version": "1", "icons": { "48": "x.png" } }"#).unwrap();
        assert!(m.popup.is_none());
        assert!(m.icon.is_none());
    }

    #[test]
    fn picks_largest_within_48() {
        let m = parse(
            r#"{ "name": "x", "version": "1", "action": { "default_icon": { "16": "a.png", "48": "b.png", "128": "c.png" } } }"#,
        )
        .unwrap();
        assert_eq!(m.icon.as_deref(), Some("b.png"));
    }

    #[test]
    fn falls_back_to_largest_above_48() {
        let m = parse(
            r#"{ "name": "x", "version": "1", "action": { "default_icon": { "64": "a.png", "128": "b.png" } } }"#,
        )
        .unwrap();
        assert_eq!(m.icon.as_deref(), Some("b.png"));
    }
}
