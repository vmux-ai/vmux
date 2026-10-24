use vmux_api::chat::{ChatActivityKind, ChatToolKind};
use vmux_ui::file_icon::{FileIcon, FilePath};
use vmux_ui::i18n::translate;
use vmux_ui::icon::LineIcon;

pub struct ToolPresentation {
    pub label: String,
}

impl ToolPresentation {
    pub fn for_call(kind: ChatToolKind, fallback: &str) -> Self {
        let label = match kind {
            ChatToolKind::Guardian => translate("agent-tool-guardian-review"),
            ChatToolKind::ReadSkill => "Read skill".into(),
            ChatToolKind::ReadFile => translate("agent-tool-read-files"),
            ChatToolKind::WriteFile => translate("agent-edited"),
            ChatToolKind::Layout => translate("schema-layout"),
            ChatToolKind::Worktree if fallback == "select project" => "Select project".into(),
            ChatToolKind::Worktree => translate("layout-worktree"),
            ChatToolKind::Image | ChatToolKind::Screenshot => translate("agent-tool-viewed-image"),
            ChatToolKind::OpenPage | ChatToolKind::Browser => translate("agent-tool-used-browser"),
            ChatToolKind::Search => translate("agent-tool-searched-files"),
            ChatToolKind::Command => translate("agent-tool-ran-commands"),
            ChatToolKind::Other => fallback.to_string(),
        };
        if label.trim().is_empty() {
            return Self {
                label: translate("agent-tool-calling"),
            };
        }
        Self { label }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityIcon {
    Thinking,
    Writing,
    Installing,
    Awaiting,
    Language(String),
    ReadFile,
    WriteFile,
    Layout,
    Worktree,
    Search,
    Image,
    Screenshot,
    OpenPage,
    Command,
    Browser,
    Guardian,
    Subagent,
    Tool,
    Output,
    Error,
    Plan,
    Diff,
    Reconnect,
}

impl ActivityIcon {
    pub fn line_icon(&self) -> Option<LineIcon> {
        let icon = match self {
            Self::Language(_) => return None,
            Self::Thinking => LineIcon::Brain,
            Self::Writing | Self::WriteFile => LineIcon::Pencil,
            Self::Installing => LineIcon::Package,
            Self::Awaiting => LineIcon::Clock,
            Self::ReadFile => LineIcon::BookOpen,
            Self::Layout => LineIcon::Layout,
            Self::Worktree => LineIcon::GitBranch,
            Self::Search => LineIcon::Search,
            Self::Image => LineIcon::Image,
            Self::Screenshot => LineIcon::Camera,
            Self::OpenPage => LineIcon::ExternalLink,
            Self::Command => LineIcon::Terminal,
            Self::Browser => LineIcon::Globe,
            Self::Guardian => LineIcon::ShieldCheck,
            Self::Subagent => LineIcon::Users,
            Self::Tool => LineIcon::Wrench,
            Self::Output => LineIcon::FileOutput,
            Self::Error => LineIcon::AlertCircle,
            Self::Plan => LineIcon::Notebook,
            Self::Diff => LineIcon::File,
            Self::Reconnect => LineIcon::Wifi,
        };
        Some(icon)
    }

    pub fn favicon(&self, accent: &str) -> String {
        if let Self::Language(path) = self
            && let FileIcon::Logo(logo) = FilePath(path).icon(false)
        {
            return Self::svg_data_url(&format!(
                "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect x='1' y='1' width='30' height='30' rx='8' fill='{accent}' fill-opacity='.15' stroke='{accent}' stroke-opacity='.45'/><g transform='translate(4 4)' fill='{colour}'><path d='{path}'/></g></svg>",
                colour = logo.colour,
                path = logo.path,
            ));
        }
        let mut paths = String::new();
        for path in self.paths() {
            paths.push_str("<path d='");
            paths.push_str(path);
            paths.push_str("'/>");
        }
        Self::svg_data_url(&format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect x='1' y='1' width='30' height='30' rx='8' fill='{accent}' fill-opacity='.15' stroke='{accent}' stroke-opacity='.45'/><g transform='translate(4 4)' fill='none' stroke='{accent}' stroke-width='1.9' stroke-linecap='round' stroke-linejoin='round'>{paths}</g></svg>"
        ))
    }

    fn svg_data_url(svg: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut encoded = String::with_capacity(svg.len() * 2);
        encoded.push_str("data:image/svg+xml,");
        for byte in svg.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                encoded.push(byte as char);
            } else {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
        encoded
    }

    pub fn paths(&self) -> &'static [&'static str] {
        match self.line_icon() {
            Some(icon) => icon.paths(),
            None => &[],
        }
    }
}

impl From<ChatActivityKind> for ActivityIcon {
    fn from(kind: ChatActivityKind) -> Self {
        match kind {
            ChatActivityKind::None | ChatActivityKind::Thinking => Self::Thinking,
            ChatActivityKind::Writing => Self::Writing,
            ChatActivityKind::Installing => Self::Installing,
            ChatActivityKind::Awaiting => Self::Awaiting,
            ChatActivityKind::Language(path) => Self::Language(path),
            ChatActivityKind::ReadFile => Self::ReadFile,
            ChatActivityKind::WriteFile => Self::WriteFile,
            ChatActivityKind::Layout => Self::Layout,
            ChatActivityKind::Worktree => Self::Worktree,
            ChatActivityKind::Search => Self::Search,
            ChatActivityKind::Image => Self::Image,
            ChatActivityKind::Screenshot => Self::Screenshot,
            ChatActivityKind::OpenPage => Self::OpenPage,
            ChatActivityKind::Command => Self::Command,
            ChatActivityKind::Browser => Self::Browser,
            ChatActivityKind::Guardian => Self::Guardian,
            ChatActivityKind::Subagent => Self::Subagent,
            ChatActivityKind::Tool => Self::Tool,
            ChatActivityKind::Output => Self::Output,
            ChatActivityKind::Error => Self::Error,
            ChatActivityKind::Plan => Self::Plan,
            ChatActivityKind::Diff => Self::Diff,
            ChatActivityKind::Reconnect => Self::Reconnect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_icons_follow_host_projection() {
        assert_eq!(
            ActivityIcon::from(ChatActivityKind::Guardian),
            ActivityIcon::Guardian
        );
        assert_eq!(
            ActivityIcon::from(ChatActivityKind::Language("language.py".into())),
            ActivityIcon::Language("language.py".into())
        );
        assert_eq!(
            ActivityIcon::from(ChatActivityKind::Reconnect),
            ActivityIcon::Reconnect
        );
    }

    #[test]
    fn tool_labels_translate_typed_kinds_and_keep_fallbacks() {
        assert_eq!(
            ToolPresentation::for_call(ChatToolKind::ReadSkill, "read file").label,
            "Read skill"
        );
        assert_eq!(
            ToolPresentation::for_call(ChatToolKind::Other, "custom tool").label,
            "custom tool"
        );
    }
}
