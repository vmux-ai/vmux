pub const CHAT_ATTACHMENTS_EVENT: &str = "chat_attachments";
pub const CHAT_ATTACHMENT_PREVIEWS_EVENT: &str = "chat_attachment_previews";
pub const CHAT_MEDIA_ENTRIES_EVENT: &str = "chat_media_entries";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub preview_data_url: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatSubmitAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatAttachments {
    pub attachments: Vec<ChatAttachment>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatMediaEntry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub mime_type: String,
    pub is_dir: bool,
    pub preview_data_url: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatMediaEntries {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<ChatMediaEntry>,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatPickFiles;

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatPasteMedia;

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatMediaListRequest {
    pub request_id: u64,
    pub query: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatAttachPaths {
    pub paths: Vec<String>,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatAttachmentPreviewRequest {
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

pub fn media_reference(entry: &ChatMediaEntry) -> String {
    let encode = |value: &str| value.replace('%', "%25").replace(' ', "%20");
    if entry.parent == "~" {
        format!("~/{name}", name = encode(&entry.name))
    } else {
        format!(
            "{parent}/{name}",
            parent = encode(&entry.parent),
            name = encode(&entry.name)
        )
    }
}

pub fn media_display_path(entry: &ChatMediaEntry) -> String {
    if entry.parent == "~" {
        format!("~/{}", entry.name)
    } else {
        format!("{}/{}", entry.parent.trim_end_matches('/'), entry.name)
    }
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
        assert_eq!(media_display_path(&entry), "~/Library/Accessibility");

        let root_entry = ChatMediaEntry {
            name: "Pictures".into(),
            parent: "~".into(),
            ..Default::default()
        };
        assert_eq!(media_display_path(&root_entry), "~/Pictures");
    }
}
