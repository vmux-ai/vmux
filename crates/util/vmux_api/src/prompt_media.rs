#[vmux_api::contract(Default, Eq)]
pub struct ChatAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    #[serde(default)]
    pub preview_data_url: String,
    #[serde(default)]
    pub media_url: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatSubmitAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

impl From<&ChatAttachment> for ChatSubmitAttachment {
    fn from(attachment: &ChatAttachment) -> Self {
        Self {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatAttachments {
    pub attachments: Vec<ChatAttachment>,
}

impl ChatAttachments {
    pub fn merge_into(&self, current: &mut Vec<ChatAttachment>) -> bool {
        let previous = current.clone();
        for attachment in &self.attachments {
            if let Some(existing) = current
                .iter_mut()
                .find(|existing| existing.path == attachment.path)
            {
                let mut replacement = attachment.clone();
                if replacement.preview_data_url.is_empty()
                    && replacement.name == existing.name
                    && replacement.mime_type == existing.mime_type
                    && replacement.size == existing.size
                {
                    replacement
                        .preview_data_url
                        .clone_from(&existing.preview_data_url);
                }
                if replacement.media_url.is_empty()
                    && replacement.name == existing.name
                    && replacement.mime_type == existing.mime_type
                    && replacement.size == existing.size
                {
                    replacement.media_url.clone_from(&existing.media_url);
                }
                *existing = replacement;
            } else {
                current.push(attachment.clone());
            }
        }
        *current != previous
    }
}

pub struct PromptMediaPaths;

impl PromptMediaPaths {
    pub fn videos(text: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((start, character)) = chars.next() {
            let boundary =
                start == 0 || text[..start].ends_with([' ', '\n', '\t', '"', '\'', '`', '(']);
            if !boundary
                || !(character == '/' || character == '~' || text[start..].starts_with("file://"))
            {
                continue;
            }
            let quoted = text[..start]
                .chars()
                .next_back()
                .filter(|c| matches!(c, '"' | '\'' | '`'));
            let mut token = String::from(character);
            while let Some(&(_, next)) = chars.peek() {
                if quoted == Some(next)
                    || (quoted.is_none()
                        && (next.is_whitespace() || matches!(next, '"' | '\'' | '`' | ')')))
                {
                    break;
                }
                chars.next();
                if next == '\\' {
                    if let Some((_, escaped)) = chars.next() {
                        token.push(escaped);
                    }
                } else {
                    token.push(next);
                }
            }
            let path = if token.starts_with("file://") {
                let Ok(url) = url::Url::parse(&token) else {
                    continue;
                };
                let Ok(path) = url.to_file_path() else {
                    continue;
                };
                path.to_string_lossy().into_owned()
            } else {
                token
            };
            if crate::media::MediaKind::from_path(&path) == Some(crate::media::MediaKind::Video)
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
        paths
    }

    pub fn append_videos(text: &str, attachments: &mut Vec<ChatAttachment>) {
        for path in Self::videos(text) {
            if attachments.iter().any(|attachment| attachment.path == path) {
                continue;
            }
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            let mime_type = crate::media::MediaKind::mime(&path)
                .unwrap_or_default()
                .to_string();
            attachments.push(ChatAttachment {
                path,
                name,
                mime_type,
                ..Default::default()
            });
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatMediaEntry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub mime_type: String,
    pub is_dir: bool,
    pub preview_data_url: String,
}

impl ChatMediaEntry {
    pub fn reference(&self) -> String {
        let encode = |value: &str| value.replace('%', "%25").replace(' ', "%20");
        if self.parent.is_empty() {
            return encode(&self.name);
        }
        if self.parent == "~" {
            return format!("~/{name}", name = encode(&self.name));
        }
        format!(
            "{parent}/{name}",
            parent = encode(&self.parent),
            name = encode(&self.name)
        )
    }

    pub fn display_path(&self) -> String {
        if self.parent.is_empty() {
            return self.name.clone();
        }
        if self.parent == "~" {
            return format!("~/{}", self.name);
        }
        format!("{}/{}", self.parent.trim_end_matches('/'), self.name)
    }
}

#[vmux_api::contract(Default)]
pub struct ChatMediaEntries {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<ChatMediaEntry>,
}

#[vmux_api::contract(Default, Eq)]
pub struct PromptMediaOption {
    pub key: String,
    pub name: String,
    pub display_path: String,
    pub preview_data_url: String,
    pub label: String,
    pub is_dir: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct PromptComposerAttachment {
    pub key: String,
    pub name: String,
    pub label: String,
    pub preview_data_url: String,
    pub remove_index: Option<u32>,
}

#[vmux_api::ui_event]
pub struct ChatPickFiles;

#[vmux_api::ui_event]
pub struct ChatPasteMedia;

#[vmux_api::ui_event(Default)]
pub struct ChatMediaListRequest {
    pub request_id: u64,
    pub query: String,
}

#[vmux_api::ui_event(Default)]
pub struct ChatAttachPaths {
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineMediaQuery<'a> {
    pub start: usize,
    pub query: &'a str,
}

impl<'a> InlineMediaQuery<'a> {
    pub fn parse(draft: &'a str) -> Option<Self> {
        draft.rmatch_indices('@').find_map(|(start, _)| {
            let boundary = start == 0
                || draft[..start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace);
            let query = &draft[start + 1..];
            (boundary && !query.chars().any(char::is_whitespace)).then_some(Self { start, query })
        })
    }

    pub fn replace(self, draft: &str, replacement: &str) -> String {
        let mut value = String::with_capacity(draft.len() + replacement.len());
        value.push_str(&draft[..self.start]);
        value.push_str(replacement);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_video_paths_handle_shell_escapes_quotes_and_file_urls() {
        let text = r#"animate /Users/test/Screen\ Recording\ 2026-11-10.mov and \"unrelated\" '/tmp/a clip.webm' ~/Movies/clip.mp4 file:///tmp/second%20clip.mov https://example.com/remote.mov"#;
        assert_eq!(
            PromptMediaPaths::videos(text),
            vec![
                "/Users/test/Screen Recording 2026-11-10.mov",
                "/tmp/a clip.webm",
                "~/Movies/clip.mp4",
                "/tmp/second clip.mov",
            ]
        );
        let mut attachments = vec![ChatAttachment {
            path: "/tmp/a clip.webm".into(),
            ..Default::default()
        }];
        PromptMediaPaths::append_videos(
            "'/tmp/a clip.webm' /tmp/b.mov /tmp/b.mov",
            &mut attachments,
        );
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[1].path, "/tmp/b.mov");
    }

    #[test]
    fn inline_media_query_requires_a_token_boundary_and_open_tail() {
        assert_eq!(
            InlineMediaQuery::parse("inspect @Pictures/scr"),
            Some(InlineMediaQuery {
                start: 8,
                query: "Pictures/scr",
            })
        );
        assert_eq!(
            InlineMediaQuery::parse("@"),
            Some(InlineMediaQuery {
                start: 0,
                query: "",
            })
        );
        assert_eq!(InlineMediaQuery::parse("mail@example.com"), None);
        assert_eq!(InlineMediaQuery::parse("inspect @image.png next"), None);
    }

    #[test]
    fn inline_media_replacement_preserves_prompt_prefix() {
        let draft = "inspect @Pictures/scr";
        let query = InlineMediaQuery::parse(draft).unwrap();
        assert_eq!(
            query.replace(draft, "@Pictures/photo.png "),
            "inspect @Pictures/photo.png "
        );
        assert_eq!(query.replace(draft, ""), "inspect ");
    }

    #[test]
    fn submit_attachment_drops_render_only_preview_data() {
        let first = ChatAttachment {
            path: "/tmp/one.png".into(),
            name: "one.png".into(),
            mime_type: "image/png".into(),
            size: 1,
            preview_data_url: "data:image/png;base64,preview".into(),
            media_url: "http://127.0.0.1:12345/render-only".into(),
        };
        let submitted = ChatSubmitAttachment::from(&first);

        assert_eq!(submitted.path, first.path);
        assert_eq!(submitted.name, first.name);
        assert_eq!(submitted.mime_type, first.mime_type);
        assert_eq!(submitted.size, first.size);
        let payload = serde_json::to_value(&submitted).unwrap();
        assert!(payload.get("preview_data_url").is_none());
        assert!(payload.get("media_url").is_none());
    }

    #[test]
    fn media_entry_paths_include_the_name() {
        let entry = ChatMediaEntry {
            name: "Accessibility".into(),
            parent: "~/Library".into(),
            ..Default::default()
        };
        assert_eq!(entry.display_path(), "~/Library/Accessibility");

        let root = ChatMediaEntry {
            name: "Pictures".into(),
            parent: "~".into(),
            ..Default::default()
        };
        assert_eq!(root.display_path(), "~/Pictures");
    }

    #[test]
    fn attachment_batches_preserve_loaded_previews() {
        let first = ChatAttachment {
            path: "/tmp/one.png".into(),
            name: "one.png".into(),
            mime_type: "image/png".into(),
            size: 1,
            preview_data_url: "data:image/png;base64,preview".into(),
            media_url: String::new(),
        };
        let refreshed = ChatAttachment {
            preview_data_url: String::new(),
            ..first.clone()
        };
        let mut current = vec![first.clone()];

        assert!(
            !ChatAttachments {
                attachments: vec![refreshed],
            }
            .merge_into(&mut current)
        );
        assert_eq!(current[0].preview_data_url, first.preview_data_url);

        assert!(
            ChatAttachments {
                attachments: vec![ChatAttachment {
                    size: 3,
                    preview_data_url: String::new(),
                    ..first
                }],
            }
            .merge_into(&mut current)
        );
        assert_eq!(current[0].size, 3);
        assert!(current[0].preview_data_url.is_empty());
    }
}
