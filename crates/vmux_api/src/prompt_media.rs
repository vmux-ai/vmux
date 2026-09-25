#[vmux_api::contract(Default, Eq)]
pub struct ChatAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    #[serde(default)]
    pub preview_data_url: String,
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
                    replacement.preview_data_url = existing.preview_data_url.clone();
                }
                *existing = replacement;
            } else {
                current.push(attachment.clone());
            }
        }
        *current != previous
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
        if self.parent == "~" {
            format!("~/{name}", name = encode(&self.name))
        } else {
            format!(
                "{parent}/{name}",
                parent = encode(&self.parent),
                name = encode(&self.name)
            )
        }
    }

    pub fn display_path(&self) -> String {
        if self.parent == "~" {
            format!("~/{}", self.name)
        } else {
            format!("{}/{}", self.parent.trim_end_matches('/'), self.name)
        }
    }
}

#[vmux_api::contract(Default)]
pub struct ChatMediaEntries {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<ChatMediaEntry>,
}

#[vmux_api::ui_event(Default, targets = ["command-bar", "start", "layout", "sessions", "agent"])]
pub struct ChatPickFiles;

#[vmux_api::ui_event(Default, targets = ["command-bar", "start", "layout", "sessions", "agent"])]
pub struct ChatPasteMedia;

#[vmux_api::ui_event(Default, targets = ["command-bar", "start", "layout", "sessions", "agent"])]
pub struct ChatMediaListRequest {
    pub request_id: u64,
    pub query: String,
}

#[vmux_api::ui_event(Default, targets = ["command-bar", "start", "layout", "sessions", "agent"])]
pub struct ChatAttachPaths {
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineMediaQuery<'a> {
    pub start: usize,
    pub query: &'a str,
}

pub fn inline_media_query(draft: &str) -> Option<InlineMediaQuery<'_>> {
    draft.rmatch_indices('@').find_map(|(start, _)| {
        let boundary = start == 0
            || draft[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let query = &draft[start + 1..];
        (boundary && !query.chars().any(char::is_whitespace))
            .then_some(InlineMediaQuery { start, query })
    })
}

pub fn replace_inline_media_query(
    draft: &str,
    query: InlineMediaQuery<'_>,
    replacement: &str,
) -> String {
    let mut value = String::with_capacity(draft.len() + replacement.len());
    value.push_str(&draft[..query.start]);
    value.push_str(replacement);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_media_query_requires_a_token_boundary_and_open_tail() {
        assert_eq!(
            inline_media_query("inspect @Pictures/scr"),
            Some(InlineMediaQuery {
                start: 8,
                query: "Pictures/scr",
            })
        );
        assert_eq!(
            inline_media_query("@"),
            Some(InlineMediaQuery {
                start: 0,
                query: "",
            })
        );
        assert_eq!(inline_media_query("mail@example.com"), None);
        assert_eq!(inline_media_query("inspect @image.png next"), None);
    }

    #[test]
    fn inline_media_replacement_preserves_prompt_prefix() {
        let draft = "inspect @Pictures/scr";
        let query = inline_media_query(draft).unwrap();
        assert_eq!(
            replace_inline_media_query(draft, query, "@Pictures/photo.png "),
            "inspect @Pictures/photo.png "
        );
        assert_eq!(replace_inline_media_query(draft, query, ""), "inspect ");
    }

    #[test]
    fn media_display_path_includes_entry_name() {
        let entry = ChatMediaEntry {
            name: "Accessibility".into(),
            parent: "~/Library".into(),
            ..Default::default()
        };
        assert_eq!(entry.display_path(), "~/Library/Accessibility");

        let root_entry = ChatMediaEntry {
            name: "Pictures".into(),
            parent: "~".into(),
            ..Default::default()
        };
        assert_eq!(root_entry.display_path(), "~/Pictures");
    }

    #[test]
    fn submit_attachment_drops_render_only_preview_data() {
        let first = ChatAttachment {
            path: "/tmp/one.png".into(),
            name: "one.png".into(),
            mime_type: "image/png".into(),
            size: 1,
            preview_data_url: "data:image/png;base64,preview".into(),
        };
        let submitted = ChatSubmitAttachment::from(&first);

        assert_eq!(submitted.path, first.path);
        assert_eq!(submitted.name, first.name);
        assert_eq!(submitted.mime_type, first.mime_type);
        assert_eq!(submitted.size, first.size);
    }

    #[test]
    fn attachment_batches_deduplicate_and_preserve_loaded_previews() {
        let first = ChatAttachment {
            path: "/tmp/one.png".into(),
            name: "one.png".into(),
            mime_type: "image/png".into(),
            size: 1,
            preview_data_url: "data:image/png;base64,preview".into(),
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
