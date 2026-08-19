//! Tool classification and the activity iconography every transcript row shares.
//!
//! Desktop and mobile previously each had their own `tool_presentation`, which is why the same
//! `mcp__vmux__run` call rendered as "Ran commands" in one client and "mcp vmux run" in the
//! other. This is the single implementation.

use crate::event::{ChatBlock, ChatItem};
use dioxus::prelude::*;
use vmux_ui::file_icon::{FileIcon, FilePath, TypeIcon};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_wire::chat::ToolName;

/// The glyph standing in for a kind of agent activity.
#[component]
pub fn ActivityIconView(kind: ActivityIcon) -> Element {
    if kind == ActivityIcon::Thinking {
        return rsx! {
            span { class: "flex h-6 w-6 shrink-0 items-center justify-center text-[17px] leading-none", aria_hidden: "true", "🧠" }
        };
    }
    if kind == ActivityIcon::Python {
        return rsx! {
            span { class: "python-activity-icon flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset", aria_hidden: "true",
                svg {
                    class: "h-[17px] w-[17px]",
                    view_box: "0 0 24 24",
                    path {
                        fill: "#3776ab",
                        d: "M11.7 2C7 2 7.3 4 7.3 4v2.1h4.5V7H5.5S2 6.6 2 12.2s3.1 5.4 3.1 5.4h1.8v-2.5s-.1-3 2.9-3h4.7s2.7 0 2.7-2.7V4.8S17.6 2 11.7 2Zm-2.5 1.5a.8.8 0 1 1 0 1.6.8.8 0 0 1 0-1.6Z",
                    }
                    path {
                        fill: "#ffd43b",
                        d: "M12.3 22c4.7 0 4.4-2 4.4-2v-2.1h-4.5V17h6.3s3.5.4 3.5-5.2-3.1-5.4-3.1-5.4h-1.8v2.5s.1 3-2.9 3H9.5s-2.7 0-2.7 2.7v4.6S6.4 22 12.3 22Zm2.5-1.5a.8.8 0 1 1 0-1.6.8.8 0 0 1 0 1.6Z",
                    }
                }
            }
        };
    }
    let tone = match kind {
        ActivityIcon::Thinking
        | ActivityIcon::Writing
        | ActivityIcon::Installing
        | ActivityIcon::Awaiting => "agent-themed-activity",
        ActivityIcon::Python => unreachable!(),
        ActivityIcon::ReadFile => "bg-sky-500/10 text-sky-600 ring-sky-500/20 dark:text-sky-300",
        ActivityIcon::WriteFile => {
            "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
        }
        ActivityIcon::Layout => {
            "bg-violet-500/10 text-violet-600 ring-violet-500/20 dark:text-violet-300"
        }
        ActivityIcon::Worktree => {
            "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
        }
        ActivityIcon::Search => "bg-cyan-500/10 text-cyan-600 ring-cyan-500/20 dark:text-cyan-300",
        ActivityIcon::Image => "bg-pink-500/10 text-pink-600 ring-pink-500/20 dark:text-pink-300",
        ActivityIcon::Screenshot => {
            "bg-fuchsia-500/10 text-fuchsia-600 ring-fuchsia-500/20 dark:text-fuchsia-300"
        }
        ActivityIcon::OpenPage => {
            "bg-blue-500/10 text-blue-600 ring-blue-500/20 dark:text-blue-300"
        }
        ActivityIcon::Command => {
            "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300"
        }
        ActivityIcon::Browser => "bg-blue-500/10 text-blue-600 ring-blue-500/20 dark:text-blue-300",
        ActivityIcon::Guardian => {
            "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
        }
        ActivityIcon::Subagent => {
            "bg-violet-500/10 text-violet-600 ring-violet-500/20 dark:text-violet-300"
        }
        ActivityIcon::Tool => {
            "bg-orange-500/10 text-orange-600 ring-orange-500/20 dark:text-orange-300"
        }
        ActivityIcon::Output => "bg-teal-500/10 text-teal-600 ring-teal-500/20 dark:text-teal-300",
        ActivityIcon::Error => "bg-red-500/10 text-red-600 ring-red-500/20 dark:text-red-300",
        ActivityIcon::Plan => {
            "bg-indigo-500/10 text-indigo-600 ring-indigo-500/20 dark:text-indigo-300"
        }
        ActivityIcon::Diff => {
            "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
        }
        ActivityIcon::Reconnect => {
            "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300"
        }
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            if let Some(icon) = kind.line_icon() {
                LineIconView { icon, class: "h-4 w-4" }
            }
        }
    }
}

/// A file's own icon, tinted by whether the agent read it or wrote it.
#[component]
pub fn FileActivityIcon(path: String, write: bool) -> Element {
    let tone = if write {
        "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
    } else {
        "bg-sky-500/10 text-sky-600 ring-sky-500/20 dark:text-sky-300"
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            TypeIcon { path, is_dir: false, class: "h-4 w-4" }
        }
    }
}

/// A tool call's icon: the file it touches when it names one, else the activity glyph.
#[component]
pub fn ToolActivityIcon(name: String, args: String, fallback: ActivityIcon) -> Element {
    let activity = ToolActivity::of(&name);
    if matches!(
        activity,
        ToolActivity::ReadFile | ToolActivity::WriteFile | ToolActivity::Other
    ) && let Some(path) = tool_file_path(&args)
    {
        let write = activity == ToolActivity::WriteFile;
        return rsx! { FileActivityIcon { path, write } };
    }
    if matches!(FilePath(&name).icon(false), FileIcon::Logo(_)) {
        return rsx! { FileActivityIcon { path: name, write: false } };
    }
    rsx! { ActivityIconView { kind: fallback } }
}

/// How a tool call announces itself in the transcript.
pub struct ToolPresentation {
    pub icon: ActivityIcon,
    pub label: String,
}

impl ToolPresentation {
    pub fn of(name: &str, args: &str) -> Self {
        let icon = ActivityIcon::for_tool(name, args);
        let label = match ToolActivity::of(name) {
            ToolActivity::Guardian => translate("agent-tool-guardian-review"),
            ToolActivity::ReadFile if tool_args_read_skill(args) => "Read skill".into(),
            ToolActivity::ReadFile => translate("agent-tool-read-files"),
            ToolActivity::WriteFile => translate("agent-edited"),
            ToolActivity::Layout => translate("schema-layout"),
            ToolActivity::Worktree if name.ends_with("select_project") => "Select project".into(),
            ToolActivity::Worktree => translate("layout-worktree"),
            ToolActivity::Image | ToolActivity::Screenshot => translate("agent-tool-viewed-image"),
            ToolActivity::OpenPage | ToolActivity::Browser => translate("agent-tool-used-browser"),
            ToolActivity::Search => translate("agent-tool-searched-files"),
            ToolActivity::Command => translate("agent-tool-ran-commands"),
            ToolActivity::Other => name
                .rsplit(['.', ':'])
                .next()
                .unwrap_or(name)
                .replace('_', " "),
        };
        Self { icon, label }
    }
}

/// What kind of work a tool call represents, before any glyph is chosen for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolActivity {
    Guardian,
    ReadFile,
    WriteFile,
    Layout,
    Worktree,
    Image,
    Screenshot,
    OpenPage,
    Browser,
    Search,
    Command,
    Other,
}

impl ToolActivity {
    /// Classify by name, since that is all every agent reliably gives us.
    pub fn of(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if ToolName(name).is_guardian() {
            ToolActivity::Guardian
        } else if lower.contains("read_file")
            || lower.contains("read file")
            || lower.contains("open_file")
            || lower.contains("open file")
        {
            ToolActivity::ReadFile
        } else if matches!(lower.as_str(), "edit" | "write")
            || lower.contains("editing file")
            || lower.contains("edited file")
            || lower.contains("write file")
            || lower.contains("apply_patch")
            || lower.contains("edit_file")
            || lower.contains("write_file")
            || lower.contains("multi_edit")
        {
            ToolActivity::WriteFile
        } else if lower.contains("worktree")
            || lower.contains("workspace")
            || lower == "select_project"
            || lower.contains("repository")
        {
            ToolActivity::Worktree
        } else if lower.contains("layout")
            || lower.contains("list_spaces")
            || lower.contains("create_space")
            || lower.contains("rename_space")
            || lower.contains("delete_space")
        {
            ToolActivity::Layout
        } else if lower.contains("screenshot") {
            ToolActivity::Screenshot
        } else if lower.contains("open_page") || lower.contains("open page") {
            ToolActivity::OpenPage
        } else if lower.contains("view_image") || lower.contains("view image") {
            ToolActivity::Image
        } else if lower.contains("browser") || lower.contains("navigate") || lower.contains("web_")
        {
            ToolActivity::Browser
        } else if lower.contains("grep") || lower.contains("search") || lower.contains("find") {
            ToolActivity::Search
        } else if lower.contains("run")
            || lower.contains("exec")
            || lower.contains("command")
            || lower.contains("shell")
            || lower.contains("terminal")
        {
            ToolActivity::Command
        } else {
            ToolActivity::Other
        }
    }

    /// The glyph for this activity, ignoring anything the arguments might say.
    pub fn icon(self) -> ActivityIcon {
        match self {
            Self::Guardian => ActivityIcon::Guardian,
            Self::ReadFile => ActivityIcon::ReadFile,
            Self::WriteFile => ActivityIcon::WriteFile,
            Self::Layout => ActivityIcon::Layout,
            Self::Worktree => ActivityIcon::Worktree,
            Self::Image => ActivityIcon::Image,
            Self::Screenshot => ActivityIcon::Screenshot,
            Self::OpenPage => ActivityIcon::OpenPage,
            Self::Browser => ActivityIcon::Browser,
            Self::Search => ActivityIcon::Search,
            Self::Command => ActivityIcon::Command,
            Self::Other => ActivityIcon::Tool,
        }
    }
}

fn tool_args_read_skill(args: &str) -> bool {
    fn skill_path(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
                matches!(key.as_str(), "path" | "file" | "file_path" | "filename")
                    && value.as_str().is_some_and(|path| {
                        std::path::Path::new(path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
                    })
                    || skill_path(value)
            }),
            serde_json::Value::Array(values) => values.iter().any(skill_path),
            _ => false,
        }
    }

    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(args) else {
        return false;
    };
    while let serde_json::Value::Object(map) = &value {
        let Some(arguments) = map.get("arguments") else {
            break;
        };
        if map.contains_key("server") || map.contains_key("tool") || map.contains_key("name") {
            value = arguments.clone();
        } else {
            break;
        }
    }
    skill_path(&value)
}

/// The glyph standing in for a kind of agent activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityIcon {
    Thinking,
    Writing,
    Installing,
    Awaiting,
    Python,
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
    /// The glyph for a tool call, preferring what the arguments say over what the name does.
    ///
    /// A `run` that executes Python is a Python call first and a command second.
    pub fn for_tool(name: &str, args: &str) -> Self {
        if let Some(icon) = Self::for_language(args) {
            return icon;
        }
        if let Some(icon) = Self::for_language(name) {
            return icon;
        }
        ToolActivity::of(name).icon()
    }

    /// The glyph for a language named anywhere in `value`, if it is one we draw.
    pub fn for_language(value: &str) -> Option<Self> {
        let lower = value.to_ascii_lowercase();
        (lower.contains(".py") || lower == "py" || lower.contains("python")).then_some(Self::Python)
    }

    /// The shared outline this activity draws, when it is one of the generic glyphs.
    ///
    /// `Python` has none: it is a two-tone brand mark drawn by [`ActivityIconView`] itself.
    pub fn line_icon(self) -> Option<LineIcon> {
        let icon = match self {
            Self::Python => return None,
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

    /// The SVG path data drawn inside the glyph's box.
    pub fn current(items: &[ChatItem], status: &str) -> Option<Self> {
        match status {
            "installing" => Some(Self::Installing),
            "awaiting" => Some(Self::Awaiting),
            "errored" => Some(Self::Error),
            "streaming" => {
                let block = items.iter().rev().find_map(|item| match item {
                    ChatItem::Turn(turn) if turn.running => turn.blocks.last(),
                    _ => None,
                });
                Some(match block {
                    Some(ChatBlock::Text(_)) => Self::Writing,
                    Some(ChatBlock::Thinking(_)) | None => Self::Thinking,
                    Some(ChatBlock::ToolUse { name, args, .. }) => Self::for_tool(name, args),
                    Some(ChatBlock::Subagent(_)) => Self::Subagent,
                    Some(ChatBlock::Diff { path, .. }) => {
                        Self::for_language(path).unwrap_or(Self::Diff)
                    }
                    Some(ChatBlock::Plan { .. }) => Self::Plan,
                    Some(ChatBlock::ToolResult { is_error: true, .. }) => Self::Error,
                    Some(ChatBlock::ToolResult { .. }) => Self::Output,
                    Some(ChatBlock::Reconnect { .. }) => Self::Reconnect,
                })
            }
            _ => None,
        }
    }

    pub fn favicon(self, accent: &str) -> String {
        if self == Self::Python {
            return Self::svg_data_url(
                "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect x='1' y='1' width='30' height='30' rx='8' fill='#151515' stroke='#3776ab' stroke-opacity='.7'/><path fill='#3776ab' d='M15.6 4C9.3 4 9.7 6.7 9.7 6.7v2.8h6v1.2H7.3s-4.6-.5-4.6 6.9 4.1 7.1 4.1 7.1h2.4v-3.3s-.1-4 3.9-4h6.3s3.6 0 3.6-3.6V7.7S23.4 4 15.6 4Zm-3.3 2a1.1 1.1 0 1 1 0 2.2 1.1 1.1 0 0 1 0-2.2Z'/><path fill='#ffd43b' d='M16.4 28c6.3 0 5.9-2.7 5.9-2.7v-2.8h-6v-1.2h8.4s4.6.5 4.6-6.9-4.1-7.1-4.1-7.1h-2.4v3.3s.1 4-3.9 4h-6.3S9 14.6 9 18.2v6.1S8.6 28 16.4 28Zm3.3-2a1.1 1.1 0 1 1 0-2.2 1.1 1.1 0 0 1 0 2.2Z'/></svg>",
            );
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

    pub fn paths(self) -> &'static [&'static str] {
        match self.line_icon() {
            Some(icon) => icon.paths(),
            None => &[],
        }
    }
}

/// The file a tool call names, from its arguments as JSON or as raw text.
fn tool_file_path(args: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str(args)
        && let Some(path) = file_path_from_value(&value)
    {
        return Some(path);
    }
    file_path_from_text(args)
}

fn file_path_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["path", "file_path", "filename", "file"] {
                if let Some(path) = map.get(key).and_then(serde_json::Value::as_str)
                    && !path.trim().is_empty()
                {
                    return Some(path.to_string());
                }
            }
            map.values().find_map(file_path_from_value)
        }
        serde_json::Value::Array(values) => values.iter().find_map(file_path_from_value),
        serde_json::Value::String(text) => file_path_from_text(text),
        _ => None,
    }
}

fn file_path_from_text(text: &str) -> Option<String> {
    for marker in ["*** Update File: ", "*** Add File: ", "*** Delete File: "] {
        if let Some(path) = text.lines().find_map(|line| line.strip_prefix(marker)) {
            return Some(path.trim().to_string());
        }
    }
    text.split_whitespace()
        .map(|token| token.trim_matches(['"', '\'', ',', ':', ';', '(', ')']))
        .find(|token| {
            if token.contains("://") {
                return false;
            }
            let name = token.rsplit('/').next().unwrap_or(token);
            name.rsplit_once('.')
                .is_some_and(|(_, ext)| !ext.is_empty() && ext.len() <= 12)
        })
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_activity_classifies_timeline_icons() {
        assert_eq!(ToolActivity::of("guardian_review"), ToolActivity::Guardian);
        assert_eq!(ToolActivity::of("read_file"), ToolActivity::ReadFile);
        assert_eq!(ToolActivity::of("apply_patch"), ToolActivity::WriteFile);
        assert_eq!(ToolActivity::of("read_layout"), ToolActivity::Layout);
        assert_eq!(ToolActivity::of("create_worktree"), ToolActivity::Worktree);
        assert_eq!(ToolActivity::of("select_project"), ToolActivity::Worktree);
        assert_eq!(ToolActivity::of("view_image"), ToolActivity::Image);
        assert_eq!(
            ToolActivity::of("vmux_screenshot"),
            ToolActivity::Screenshot
        );
        assert_eq!(ToolActivity::of("vmux_open_page"), ToolActivity::OpenPage);
        assert_eq!(ToolActivity::of("vmux_open_file"), ToolActivity::ReadFile);
        assert_eq!(ToolActivity::of("browser_navigate"), ToolActivity::Browser);
        assert_eq!(ToolActivity::of("search_files"), ToolActivity::Search);
        assert_eq!(ToolActivity::of("exec_command"), ToolActivity::Command);
        assert_eq!(ToolActivity::of("custom_tool"), ToolActivity::Other);
    }

    /// The arguments outrank the name: a `run` that executes Python is a Python call.
    #[test]
    fn a_tool_icon_prefers_the_language_in_its_arguments() {
        assert_eq!(
            ActivityIcon::for_tool("run", r#"{"cmd":"python main.py"}"#),
            ActivityIcon::Python
        );
        assert_eq!(
            ActivityIcon::for_tool("run", r#"{"cmd":"ls"}"#),
            ActivityIcon::Command
        );
    }

    #[test]
    fn skill_reads_are_identified_from_nested_tool_arguments() {
        assert!(tool_args_read_skill(
            r#"{"arguments":{"path":"/tmp/skills/caveman/SKILL.md"},"server":"vmux","tool":"read_file"}"#
        ));
        assert!(!tool_args_read_skill(
            r#"{"arguments":{"path":"/tmp/src/lib.rs"},"server":"vmux","tool":"read_file"}"#
        ));
    }
}
