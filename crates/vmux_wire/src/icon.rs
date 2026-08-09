use serde::{Deserialize, Serialize};

#[cfg_attr(all(feature = "bevy", not(web)), derive(bevy_reflect::Reflect))]
#[cfg_attr(all(feature = "bevy", not(web)), type_path = "vmux_core::icon")]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum BuiltinIcon {
    Terminal,
    Files,
    Server,
    Settings,
    Clock,
    Layers,
    Users,
    Sparkles,
    Activity,
    Puzzle,
    Nushell,
    Bash,
    Zsh,
    Hammer,
    Vault,
}

impl BuiltinIcon {
    /// Map a shell binary path/name to its brand icon, e.g. `/opt/homebrew/bin/nu`
    /// -> `Nushell`. Returns `None` for unrecognized shells (caller falls back to
    /// the generic terminal icon).
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

#[cfg_attr(all(feature = "bevy", not(web)), derive(bevy_reflect::Reflect))]
#[cfg_attr(all(feature = "bevy", not(web)), type_path = "vmux_core::icon")]
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[cfg(test)]
#[path = "icon.test.rs"]
mod tests;
