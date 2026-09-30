use crate::prompt_media::{ChatAttachment, ChatMediaEntry};
use vmux_core::prompt_media::MediaPath;
use vmux_ui::components::composer::PromptComposerAttachment;
use vmux_ui::components::prompt_media_options::PromptMediaOption;
use vmux_ui::file_icon::FilePath;

pub struct PromptMedia;

impl PromptMedia {
    pub fn options(entries: &[ChatMediaEntry]) -> Vec<PromptMediaOption> {
        let mut options = Vec::with_capacity(entries.len());
        for entry in entries {
            options.push(PromptMediaOption {
                key: format!("media-{}", entry.path),
                name: entry.name.clone(),
                display_path: MediaPath::new(entry).display(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FilePath(&entry.name).extension_label(),
                is_dir: entry.is_dir,
            });
        }
        options
    }

    pub fn composer_attachments(attachments: &[ChatAttachment]) -> Vec<PromptComposerAttachment> {
        PromptComposerAttachment::removable(attachments)
    }
}
