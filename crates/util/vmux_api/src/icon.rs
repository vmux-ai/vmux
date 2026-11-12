#[cfg_attr(bevy_linked, derive(bevy_reflect::Reflect))]
#[cfg_attr(bevy_linked, type_path = "vmux_api::icon")]
#[vmux_api::contract(Copy, Eq)]
pub enum BuiltinIcon {
    Terminal,
    Files,
    Code,
    Project,
    Brain,
    Server,
    Settings,
    Clock,
    Layers,
    Planet,
    Users,
    Sparkles,
    Activity,
    Puzzle,
    Nushell,
    Bash,
    Zsh,
    Hammer,
    Toolbox,
    Vault,
    Smartphone,
    Keyboard,
    GitBranch,
    Bookmark,
    CheckCircle,
    MessagesRound,
    ListTodo,
    Search,
    Globe,
    Archive,
    GitCommit,
    Branch,
}

impl BuiltinIcon {
    pub fn for_shell(command: &str) -> Option<BuiltinIcon> {
        let lower = command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(command)
            .to_ascii_lowercase();
        match lower.trim_end_matches(".exe") {
            "nu" | "nushell" => Some(BuiltinIcon::Nushell),
            "bash" | "sh" => Some(BuiltinIcon::Bash),
            "zsh" => Some(BuiltinIcon::Zsh),
            _ => None,
        }
    }
}

#[cfg_attr(bevy_linked, derive(bevy_reflect::Reflect))]
#[cfg_attr(bevy_linked, type_path = "vmux_api::icon")]
#[vmux_api::contract(Default, Eq)]
pub enum PageIcon {
    #[default]
    None,
    Favicon(String),
    Builtin(BuiltinIcon),
}

impl PageIcon {
    pub fn favicon(url: impl Into<String>) -> Self {
        let url = url.into();
        if url.is_empty() {
            Self::None
        } else {
            Self::Favicon(url)
        }
    }

    pub fn favicon_url(&self) -> &str {
        match self {
            Self::Favicon(url) => url.as_str(),
            _ => "",
        }
    }

    pub fn builtin(&self) -> Option<BuiltinIcon> {
        match self {
            Self::Builtin(icon) => Some(*icon),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

pub struct StoredPageIcons<'a>(pub &'a str);

impl StoredPageIcons<'_> {
    pub fn normalize(self) -> Option<(String, Vec<String>)> {
        let mut normalized = String::with_capacity(self.0.len());
        let mut unknown = Vec::new();
        for line in self.0.split_inclusive('\n') {
            let (content, newline) = match line.strip_suffix('\n') {
                Some(content) => (content, "\n"),
                None => (line, ""),
            };
            let trimmed = content.trim();
            let name = trimmed
                .strip_prefix("icon: Builtin(")
                .and_then(|value| value.strip_suffix("),"));
            let Some(name) = name else {
                normalized.push_str(line);
                continue;
            };
            if serde_json::from_value::<BuiltinIcon>(serde_json::Value::String(name.to_string()))
                .is_ok()
            {
                normalized.push_str(line);
                continue;
            }
            let indent = &content[..content.len() - content.trim_start().len()];
            normalized.push_str(indent);
            normalized.push_str("icon: r#None,");
            normalized.push_str(newline);
            unknown.push(name.to_string());
        }
        if unknown.is_empty() {
            None
        } else {
            Some((normalized, unknown))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_shell_maps_known_shells() {
        assert_eq!(
            BuiltinIcon::for_shell("/opt/homebrew/bin/nu"),
            Some(BuiltinIcon::Nushell)
        );
        assert_eq!(BuiltinIcon::for_shell("/bin/bash"), Some(BuiltinIcon::Bash));
        assert_eq!(BuiltinIcon::for_shell("/bin/zsh"), Some(BuiltinIcon::Zsh));
        assert_eq!(BuiltinIcon::for_shell("nu"), Some(BuiltinIcon::Nushell));
        assert_eq!(BuiltinIcon::for_shell("/usr/bin/fish"), None);
    }

    #[test]
    fn favicon_constructor_collapses_empty_to_none() {
        assert_eq!(PageIcon::favicon(""), PageIcon::None);
        assert_eq!(
            PageIcon::favicon("https://x/fav.ico"),
            PageIcon::Favicon("https://x/fav.ico".to_string())
        );
    }

    #[test]
    fn accessors() {
        assert_eq!(PageIcon::Favicon("u".into()).favicon_url(), "u");
        assert_eq!(PageIcon::Builtin(BuiltinIcon::Users).favicon_url(), "");
        assert_eq!(
            PageIcon::Builtin(BuiltinIcon::Users).builtin(),
            Some(BuiltinIcon::Users)
        );
        assert!(PageIcon::None.is_none());
        assert_eq!(PageIcon::default(), PageIcon::None);
    }

    #[test]
    fn persisted_tool_and_vault_icons_deserialize() {
        for (json, expected) in [
            (r#"{"Builtin":"Hammer"}"#, BuiltinIcon::Hammer),
            (r#"{"Builtin":"Vault"}"#, BuiltinIcon::Vault),
        ] {
            assert_eq!(
                serde_json::from_str::<PageIcon>(json).unwrap(),
                PageIcon::Builtin(expected)
            );
        }
    }

    #[test]
    fn persisted_git_page_icon_remains_loadable() {
        assert_eq!(
            serde_json::from_str::<PageIcon>(r#"{"Builtin":"GitBranch"}"#).unwrap(),
            PageIcon::Builtin(BuiltinIcon::GitBranch)
        );
    }
}
