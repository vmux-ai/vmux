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

impl ChatMediaEntry {
    /// How this entry is written into a prompt after an `@`.
    ///
    /// Percent-encoded, because a space would otherwise end the token the composer is matching.
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

    /// How this entry is shown to a reader — the same path, unencoded.
    pub fn display_path(&self) -> String {
        if self.parent == "~" {
            format!("~/{}", self.name)
        } else {
            format!("{}/{}", self.parent.trim_end_matches('/'), self.name)
        }
    }
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

pub fn merge_chat_attachments(
    current: &[ChatAttachment],
    incoming: &[ChatAttachment],
) -> Vec<ChatAttachment> {
    let mut merged = current.to_vec();
    for attachment in incoming {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.path == attachment.path)
        {
            let mut replacement = attachment.clone();
            if replacement.preview_data_url.is_empty() {
                replacement.preview_data_url = existing.preview_data_url.clone();
            }
            *existing = replacement;
        } else {
            merged.push(attachment.clone());
        }
    }
    merged
}

#[cfg(test)]
#[path = "prompt_media.test.rs"]
mod tests;
