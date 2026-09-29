use vmux_api::prompt_media::{ChatAttachment, ChatAttachments, ChatMediaEntry};

pub struct AttachmentSelection<'a> {
    current: &'a mut Vec<ChatAttachment>,
}

impl<'a> AttachmentSelection<'a> {
    pub fn new(current: &'a mut Vec<ChatAttachment>) -> Self {
        Self { current }
    }

    pub fn merge(&mut self, incoming: &ChatAttachments) -> bool {
        let previous = self.current.clone();
        for attachment in &incoming.attachments {
            if let Some(existing) = self
                .current
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
                *existing = replacement;
            } else {
                self.current.push(attachment.clone());
            }
        }
        *self.current != previous
    }
}

pub struct MediaPath<'a> {
    entry: &'a ChatMediaEntry,
}

impl<'a> MediaPath<'a> {
    pub fn new(entry: &'a ChatMediaEntry) -> Self {
        Self { entry }
    }

    pub fn reference(&self) -> String {
        let encode = |value: &str| value.replace('%', "%25").replace(' ', "%20");
        if self.entry.parent == "~" {
            format!("~/{name}", name = encode(&self.entry.name))
        } else {
            format!(
                "{parent}/{name}",
                parent = encode(&self.entry.parent),
                name = encode(&self.entry.name)
            )
        }
    }

    pub fn display(&self) -> String {
        if self.entry.parent == "~" {
            format!("~/{}", self.entry.name)
        } else {
            format!(
                "{}/{}",
                self.entry.parent.trim_end_matches('/'),
                self.entry.name
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_path_includes_the_entry_name() {
        let entry = ChatMediaEntry {
            name: "Accessibility".into(),
            parent: "~/Library".into(),
            ..Default::default()
        };
        assert_eq!(MediaPath::new(&entry).display(), "~/Library/Accessibility");

        let root_entry = ChatMediaEntry {
            name: "Pictures".into(),
            parent: "~".into(),
            ..Default::default()
        };
        assert_eq!(MediaPath::new(&root_entry).display(), "~/Pictures");
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
            !AttachmentSelection::new(&mut current).merge(&ChatAttachments {
                attachments: vec![refreshed],
            })
        );
        assert_eq!(current[0].preview_data_url, first.preview_data_url);

        assert!(
            AttachmentSelection::new(&mut current).merge(&ChatAttachments {
                attachments: vec![ChatAttachment {
                    size: 3,
                    preview_data_url: String::new(),
                    ..first
                }],
            })
        );
        assert_eq!(current[0].size, 3);
        assert!(current[0].preview_data_url.is_empty());
    }
}
