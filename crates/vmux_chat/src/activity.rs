//! Tool classification and the activity iconography every transcript row shares.
//!
//! Desktop and mobile previously each had their own `tool_presentation`, which is why the same
//! `mcp__vmux__run` call rendered as "Ran commands" in one client and "mcp vmux run" in the
//! other. This is the single implementation.

use dioxus::prelude::*;
use vmux_ui::file_icon::{FileIcon, TypeIcon, file_icon_kind};
use vmux_ui::i18n::translate;
use vmux_wire::chat::is_guardian_tool;

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
    let paths = kind.paths();
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
            svg {
                class: "h-4 w-4",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                for path in paths {
                    path { d: "{path}" }
                }
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
    if matches!(file_icon_kind(&name, false), FileIcon::Logo(_)) {
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
        if is_guardian_tool(name) {
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

pub fn should_expand_thinking(block_index: usize, block_count: usize) -> bool {
    block_index + 1 == block_count
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

    /// The SVG path data drawn inside the glyph's box.
    pub fn paths(self) -> &'static [&'static str] {
        match self {
            ActivityIcon::Thinking => &[
                "M9.5 4.5a3.2 3.2 0 0 1 5.35 1.05 3.35 3.35 0 0 1 2.8 3.35 3.5 3.5 0 0 1 .55 6.45A3.4 3.4 0 0 1 15 18.5H9a4 4 0 0 1-3.75-5.4 3.5 3.5 0 0 1 1.2-6.3A3.2 3.2 0 0 1 9.5 4.5Z",
                "M14.5 18.5c0 1.4.9 2.5 2.5 2.5v-4.4",
                "M9.4 4.7c-.9 1.2-.8 2.8.3 3.8",
                "M6.2 9.4c1.3-.7 2.8-.4 3.8.6",
                "M13.9 5.8c-.7 1-.6 2.2.2 3.1",
                "M14.1 9c1.4-.2 2.6.6 3.1 1.7",
                "M8.5 13.2c1-.7 2.4-.5 3.2.4",
                "M12.6 11.9c-.1 1.9.8 3.6 2.4 4.4",
            ],
            ActivityIcon::Writing => &["M12 20h9", "M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"],
            ActivityIcon::Installing => &[
                "m7.5 4.27 9 5.15",
                "M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z",
                "M3.3 7 12 12l8.7-5",
                "M12 22V12",
            ],
            ActivityIcon::Awaiting => &["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M12 6v6l4 2"],
            ActivityIcon::Python => &[],
            ActivityIcon::ReadFile => &[
                "M12 7v14",
                "M3 18a1 1 0 0 1-1-1V5a2 2 0 0 1 2-2h5a3 3 0 0 1 3 3v15a3 3 0 0 0-3-3Z",
                "M21 18a1 1 0 0 0 1-1V5a2 2 0 0 0-2-2h-5a3 3 0 0 0-3 3v15a3 3 0 0 1 3-3Z",
            ],
            ActivityIcon::WriteFile => {
                &["M12 20h9", "M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"]
            }
            ActivityIcon::Layout => &["M4 4h9v16H4Z", "M15 4h5v7h-5Z", "M15 13h5v7h-5Z"],
            ActivityIcon::Worktree => &[
                "M6 3v12",
                "M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
                "M6 6a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
                "M6 15c0 3 2 5 5 5h4",
            ],
            ActivityIcon::Search => &["M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z", "m21 21-4.35-4.35"],
            ActivityIcon::Image => &[
                "M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z",
                "M10.5 8.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z",
                "m21 15-5-5L5 21",
            ],
            ActivityIcon::Screenshot => &[
                "M9 4 7.5 6H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4Z",
                "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
            ],
            ActivityIcon::OpenPage => &[
                "M14 3h7v7",
                "m21 3-9 9",
                "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
            ],
            ActivityIcon::Command => &["m4 17 6-6-6-6", "M12 19h8"],
            ActivityIcon::Browser => &[
                "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z",
                "M2 12h20",
                "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z",
            ],
            ActivityIcon::Guardian => &[
                "M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z",
                "m9 12 2 2 4-4",
            ],
            ActivityIcon::Subagent => &[
                "M12 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
                "M5 21v-2a7 7 0 0 1 14 0v2",
                "M5.5 11a2.5 2.5 0 1 0 0-5",
                "M18.5 11a2.5 2.5 0 1 1 0-5",
            ],
            ActivityIcon::Tool => &[
                "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76Z",
            ],
            ActivityIcon::Output => &[
                "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5Z",
                "M14 2v6h6",
                "m10 17 3-3-3-3",
                "M13 14H7",
            ],
            ActivityIcon::Error => &[
                "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z",
                "M12 8v4",
                "M12 16h.01",
            ],
            ActivityIcon::Plan => &[
                "M4 19.5A2.5 2.5 0 0 1 6.5 17H20",
                "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z",
            ],
            ActivityIcon::Diff => &[
                "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",
                "M14 2v4a2 2 0 0 0 2 2h4",
            ],
            ActivityIcon::Reconnect => &[
                "M5 12.55a11 11 0 0 1 14.08 0",
                "M1.42 9a16 16 0 0 1 21.16 0",
                "M8.53 16.11a6 6 0 0 1 6.95 0",
                "M12 20h.01",
            ],
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
    fn thinking_expands_only_until_the_next_block() {
        assert!(should_expand_thinking(0, 1));
        assert!(!should_expand_thinking(0, 2));
    }

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
