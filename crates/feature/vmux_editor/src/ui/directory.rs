use std::collections::HashMap;

use base64::Engine;
use dioxus::prelude::*;
use vmux_ecs::event::{FileDirEntry, FileLine};
use vmux_ui::i18n::translate;
use vmux_ui::media::MediaElement;

use super::text_style::StyledSpanStyle;

#[derive(Clone, PartialEq)]
pub(super) enum Preview {
    None,
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image(String),
    Video {
        url: String,
        path: String,
        native: bool,
    },
    Info {
        size: u64,
        modified: String,
        kind: String,
    },
    Error(String),
}

impl Preview {
    pub(super) fn image(bytes: Vec<u8>, path: &str) -> Self {
        Self::Image(Self::image_url(&bytes, path))
    }

    pub(super) fn image_url(bytes: &[u8], path: &str) -> String {
        let mime =
            vmux_api::media::MediaKind::image_mime(path).unwrap_or("application/octet-stream");

        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    pub(super) fn clear(mut preview: Signal<Preview>, mut thumbs: Signal<HashMap<String, String>>) {
        preview.set(Self::None);
        thumbs.set(HashMap::new());
    }

    pub(super) fn toggle_video() {
        MediaElement::with_id("preview-video").toggle_playback();
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

const VIDEO_HOST_ID: &str = "vmux-video-host";

#[component]
pub(super) fn PreviewPane(preview: Preview) -> Element {
    let preview = &preview;
    match preview {
        Preview::None | Preview::Dir(_) => rsx! {
            div { class: "text-xs text-muted-foreground opacity-60", "" }
        },
        Preview::Image(url) => rsx! {
            img { src: "{url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_color-mix(in_oklab,var(--primary)_40%,transparent)] ring-1 ring-primary/20" }
        },
        Preview::Video { url, path, native } => {
            if *native {
                let path = path.clone();
                rsx! {
                    div {
                        key: "{path}",
                        id: VIDEO_HOST_ID,
                        class: "h-full w-full rounded-xl bg-black/40 ring-1 ring-primary/20",
                    }
                }
            } else {
                rsx! {
                    video {
                        id: "preview-video",
                        src: "{url}",
                        controls: true,
                        autoplay: false,
                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_color-mix(in_oklab,var(--primary)_40%,transparent)] ring-1 ring-primary/20",
                    }
                }
            }
        }
        Preview::Text(lines) => rsx! {
            div { class: "h-full w-full overflow-auto font-mono text-xs leading-snug",
                for line in lines.iter() {
                    div { key: "{line.line_no}", class: "whitespace-pre",
                        for (index, span) in line.spans.iter().enumerate() {
                            span { key: "{index}", style: "{StyledSpanStyle::of(span)}", "{span.text}" }
                        }
                    }
                }
            }
        },
        Preview::Info {
            size,
            modified,
            kind,
        } => rsx! {
            div { class: "space-y-1 text-center text-xs text-muted-foreground",
                div {
                    class: "uppercase tracking-wide text-foreground/80",
                    {match kind.as_str() {
                        "image (too large to preview)" => translate("editor-preview-large-image"),
                        "binary" => translate("editor-preview-binary"),
                        "file" => translate("editor-preview-file"),
                        _ => kind.clone(),
                    }}
                }
                div { "{format_size(*size)}" }
                if !modified.is_empty() {
                    div { class: "opacity-70", "{modified}" }
                }
            }
        },
        Preview::Error(message) => rsx! {
            div { class: "text-xs text-ansi-1", "{message}" }
        },
    }
}
