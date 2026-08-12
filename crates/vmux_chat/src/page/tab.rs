//! How a conversation presents itself: the colour it is painted with, and the title and favicon
//! of the tab holding it.
//!
//! A pane is a browser tab, so its title and favicon are how the conversation identifies itself in
//! the layout. A native host has no tab and shows this in its own chrome instead, so applying an
//! identity there is a no-op.

use crate::activity::ActivityIcon;
use crate::event::{ChatBlock, ChatItem};
use vmux_ui::favicon::favicon_src_for_url;

/// The colour a conversation paints itself with: the profile's, when that is a usable hex, else
/// the agent's built-in fallback.
#[derive(Clone, PartialEq)]
pub struct Accent {
    /// A CSS colour, for `--agent-accent` and the generated favicon.
    pub css: String,
    /// The same colour as space-separated `r g b` channels, for the install backdrop.
    pub rgb: String,
}

impl Accent {
    /// Resolve a profile colour against the agent's fallback, so both forms agree on which one won.
    pub fn resolve(profile_color: &str, fallback_rgb: &str) -> Self {
        let Some((red, green, blue)) = Self::channels(profile_color) else {
            return Accent {
                css: format!("rgb({fallback_rgb})"),
                rgb: fallback_rgb.to_string(),
            };
        };
        Accent {
            css: profile_color.to_string(),
            rgb: format!("{red} {green} {blue}"),
        }
    }

    fn channels(color: &str) -> Option<(u8, u8, u8)> {
        let hex = color.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        Some((
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ))
    }
}

/// What the tab holding a conversation should read as, right now.
pub struct TabIdentity {
    title: String,
    favicon: String,
}

impl TabIdentity {
    /// The conversation's title, plus an icon tracking what the agent is doing — falling back to
    /// the agent's own icon when it is doing nothing worth showing.
    pub fn of(
        title: String,
        items: &[ChatItem],
        status: &str,
        icon_url: &str,
        agent: &str,
        accent: &Accent,
    ) -> Self {
        let favicon = Self::activity(items, status)
            .map(|activity| Self::activity_favicon(activity, &accent.css))
            .or_else(|| favicon_src_for_url(icon_url, &format!("vmux://agent/{agent}")))
            .unwrap_or_else(|| Self::activity_favicon(ActivityIcon::Tool, &accent.css));
        TabIdentity { title, favicon }
    }

    /// What the agent is doing, when that is worth showing in the tab.
    fn activity(items: &[ChatItem], status: &str) -> Option<ActivityIcon> {
        match status {
            "installing" => Some(ActivityIcon::Installing),
            "awaiting" => Some(ActivityIcon::Awaiting),
            "errored" => Some(ActivityIcon::Error),
            "streaming" => {
                let block = items.iter().rev().find_map(|item| match item {
                    ChatItem::Turn(turn) if turn.running => turn.blocks.last(),
                    _ => None,
                });
                Some(match block {
                    Some(ChatBlock::Text(_)) => ActivityIcon::Writing,
                    Some(ChatBlock::Thinking(_)) | None => ActivityIcon::Thinking,
                    Some(ChatBlock::ToolUse { name, args, .. }) => {
                        ActivityIcon::for_tool(name, args)
                    }
                    Some(ChatBlock::Subagent(_)) => ActivityIcon::Subagent,
                    Some(ChatBlock::Diff { path, .. }) => {
                        ActivityIcon::for_language(path).unwrap_or(ActivityIcon::Diff)
                    }
                    Some(ChatBlock::Plan { .. }) => ActivityIcon::Plan,
                    Some(ChatBlock::ToolResult { is_error: true, .. }) => ActivityIcon::Error,
                    Some(ChatBlock::ToolResult { .. }) => ActivityIcon::Output,
                    Some(ChatBlock::Reconnect { .. }) => ActivityIcon::Reconnect,
                })
            }
            _ => None,
        }
    }

    fn activity_favicon(kind: ActivityIcon, accent: &str) -> String {
        if kind == ActivityIcon::Python {
            return Self::svg_data_url(
                "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect x='1' y='1' width='30' height='30' rx='8' fill='#151515' stroke='#3776ab' stroke-opacity='.7'/><path fill='#3776ab' d='M15.6 4C9.3 4 9.7 6.7 9.7 6.7v2.8h6v1.2H7.3s-4.6-.5-4.6 6.9 4.1 7.1 4.1 7.1h2.4v-3.3s-.1-4 3.9-4h6.3s3.6 0 3.6-3.6V7.7S23.4 4 15.6 4Zm-3.3 2a1.1 1.1 0 1 1 0 2.2 1.1 1.1 0 0 1 0-2.2Z'/><path fill='#ffd43b' d='M16.4 28c6.3 0 5.9-2.7 5.9-2.7v-2.8h-6v-1.2h8.4s4.6.5 4.6-6.9-4.1-7.1-4.1-7.1h-2.4v3.3s.1 4-3.9 4h-6.3S9 14.6 9 18.2v6.1S8.6 28 16.4 28Zm3.3-2a1.1 1.1 0 1 1 0-2.2 1.1 1.1 0 0 1 0 2.2Z'/></svg>",
            );
        }
        let mut paths = String::new();
        for path in kind.paths() {
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

    /// Reflect this identity in the tab that holds the page.
    #[cfg(web)]
    pub fn apply(&self) {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        if document.title() != self.title {
            document.set_title(&self.title);
        }
        let link = document
            .query_selector("link[rel~='icon']")
            .ok()
            .flatten()
            .or_else(|| {
                let link = document.create_element("link").ok()?;
                link.set_attribute("rel", "icon").ok()?;
                document
                    .query_selector("head")
                    .ok()
                    .flatten()?
                    .append_child(&link)
                    .ok()?;
                Some(link)
            });
        if let Some(link) = link {
            let _ = link.set_attribute("href", &self.favicon);
        }
    }

    /// A native host has no tab to write to, so the identity is computed and then dropped.
    #[cfg(not(web))]
    pub fn apply(&self) {
        let _ = (&self.title, &self.favicon);
    }
}
