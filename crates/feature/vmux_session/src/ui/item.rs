use std::cell::RefCell;
use std::collections::HashSet;

use dioxus::prelude::*;
use vmux_api::chat::{
    ChatBlock, ChatDiffLineKind, ChatItem, ChatPlanStatus, ChatSubagentState, ChatSubagentStatus,
    ChatSubagentSummary, ChatToolArgument, ChatToolArgumentValue, ChatToolArguments, ChatToolCall,
    ChatToolChild, ChatToolChildCall, ChatTurn, ChatTurnRow, ExploreStep, WORKING_VERB_IDS,
};
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::{OpenTarget, PaneDirection, PaneOpenMode, PaneTarget};
use vmux_api::prompt_media::ChatAttachment;
use vmux_ui::components::avatar::{AgentThumbnail, Avatar};
use vmux_ui::components::code::ShellCode;
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::file_icon::{FilePath, TypeIcon};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::navigation::NavigationGesture;

use super::activity::{ActivityIconView, FileActivityIcon, ToolActivityIcon};
use crate::activity::{ActivityIcon, ToolPresentation};
use vmux_ui::clipboard::Clipboard;
use vmux_ui::platform::Platform;

use pulldown_cmark::{CowStr, Event, Options, Parser, html};

#[component]
pub fn UserBubble(
    avatar_name: String,
    avatar_color: String,
    #[props(default)] copy_text: String,
    #[props(default)] created_at_ms: u64,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "chat-message-enter chat-user-bubble group flex w-full flex-row-reverse items-start justify-start gap-3 px-1 py-1 text-sm [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]", "data-vmux-scroll-layout": "", ..attributes,
            Avatar {
                src: None,
                seed: avatar_name.clone(),
                background: avatar_color,
                alt: avatar_name.clone(),
                class: "mt-1 h-6 w-6 text-[8px]".to_string(),
            }
            div { class: "relative min-w-0 max-w-[80%] flex-none",
                div { class: "flex min-w-0 flex-col gap-2 rounded-2xl bg-foreground/[0.045] px-4 py-3 text-left", {children} }
                MessageMeta { text: copy_text, created_at_ms, right: true }
            }
        }
    }
}

#[component]
pub fn AssistantTurn(
    agent_id: String,
    name: String,
    avatar_src: Option<String>,
    avatar_background: String,
    #[props(default)] copy_text: String,
    #[props(default)] created_at_ms: u64,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "chat-message-enter chat-assistant-turn group flex w-full gap-3 px-1 py-1 [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]", "data-vmux-scroll-layout": "", ..attributes,
            AgentThumbnail {
                src: avatar_src,
                seed: agent_id,
                background: avatar_background,
                alt: name.clone(),
                class: "mt-0.5 h-6 w-6 text-[8px]".to_string(),
            }
            div { class: "relative min-w-0 flex-1",
                div { class: "mb-1 text-xs font-medium text-muted-foreground", "{name}" }
                div { class: "flex min-w-0 flex-col gap-2.5", {children} }
                MessageMeta { text: copy_text, created_at_ms }
            }
        }
    }
}

#[component]
fn MessageMeta(text: String, created_at_ms: u64, #[props(default)] right: bool) -> Element {
    let alignment = if right {
        "justify-end"
    } else {
        "justify-start"
    };
    let timestamp = message_timestamp(created_at_ms);
    rsx! {
        div { class: "mt-1 flex h-6 items-center gap-1 {alignment} text-[11px] text-muted-foreground/45",
            if let Some(timestamp) = timestamp {
                span { class: "pointer-events-none tabular-nums opacity-0 transition-opacity group-hover:opacity-100", "{timestamp}" }
            }
            if !text.is_empty() {
                MessageCopyButton { text }
            }
        }
    }
}

fn message_timestamp(created_at_ms: u64) -> Option<String> {
    if created_at_ms == 0 {
        return None;
    }
    let timestamp = i64::try_from(created_at_ms).ok()?;
    let utc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp)?;
    Some(
        utc.with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string(),
    )
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;

    #[test]
    fn message_timestamp_is_local_clock_time() {
        let timestamp = message_timestamp(1_788_979_740_000).expect("timestamp");

        assert_eq!(timestamp.len(), 5);
        assert_eq!(timestamp.as_bytes()[2], b':');
        assert!(!timestamp.contains('-'));
    }

    #[test]
    fn missing_message_timestamp_stays_hidden() {
        assert_eq!(message_timestamp(0), None);
    }

    #[test]
    fn streaming_markdown_animates_each_new_word() {
        let html = md_to_streaming_html("Hello brave world", "Hello ");

        assert_eq!(html.matches("chat-stream-word").count(), 2);
        assert!(html.contains("Hello "));
        assert!(html.contains("brave"));
        assert!(html.contains("world"));
    }

    #[test]
    fn streaming_markdown_keeps_existing_words_stable() {
        let html = md_to_streaming_html("One two three", "One two");

        assert_eq!(html.matches("chat-stream-word").count(), 1);
        assert!(html.contains("One two "));
    }
}

#[component]
pub fn MessageCopyButton(text: String) -> Element {
    let label = translate("agent-copy");
    rsx! {
        button {
            class: "pointer-events-none flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/60 opacity-0 transition group-hover:pointer-events-auto group-hover:opacity-100 hover:bg-foreground/[0.08] hover:text-foreground focus-visible:pointer-events-auto focus-visible:opacity-100",
            title: "{label}",
            aria_label: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                Clipboard::write(&text);
            },
            LineIconView { icon: LineIcon::Copy, class: "h-3.5 w-3.5" }
        }
    }
}

#[component]
pub fn ChatItemRow(
    absolute_index: usize,
    item: ChatItem,
    agent_id: String,
    agent_name: String,
    agent_avatar: Option<String>,
    agent_color: String,
    user_name: String,
    user_color: String,
    on_media_load: EventHandler<()>,
    #[props(default)] latest: bool,
) -> Element {
    let key = absolute_index;
    let item = &item;
    match item {
        ChatItem::User {
            text,
            context,
            attachments,
            created_at_ms,
        } => {
            let mut attachments = attachments.clone();
            vmux_api::prompt_media::PromptMediaPaths::append_videos(text, &mut attachments);
            rsx! {
            UserBubble {
                key: "{key}",
                avatar_name: user_name,
                avatar_color: user_color,
                copy_text: text.clone(),
                created_at_ms: *created_at_ms,
                if let Some(context) = context {
                    details { class: "disclosure user-context-panel rounded-xl border",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 px-2.5 py-2 text-xs list-none [&::-webkit-details-marker]:hidden",
                            DisclosureIcon {}
                            span { class: "flex h-5 w-5 shrink-0 items-center justify-center text-muted-foreground/75",
                                LineIconView { icon: LineIcon::Shield, class: "h-3 w-3" }
                            }
                            span { class: "font-medium", {translate("agent-prompt-context")} }
                            span {
                                class: "text-[10px] text-muted-foreground",
                                {translate_with(
                                    "agent-bytes",
                                    &[("count", TranslationValue::Number(context.len() as i64))],
                                )}
                            }
                        }
                        pre { class: "user-context-content max-h-72 overflow-auto whitespace-pre-wrap rounded-lg px-3 py-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{context}" }
                    }
                }
                if !text.is_empty() {
                    div { class: "whitespace-pre-wrap break-words", "{text}" }
                }
                if !attachments.is_empty() {
                    div { class: "flex w-full flex-col gap-2",
                        for attachment in attachments {
                            UserAttachment {
                                attachment: attachment.clone(),
                                onload: on_media_load,
                            }
                        }
                    }
                }
            }
            }
        }
        ChatItem::Turn(turn) => rsx! {
            TurnView {
                turn_index: key,
                turn: turn.clone(),
                agent_id,
                agent_name,
                agent_avatar,
                agent_color,
                latest,
            }
        },
    }
}

#[component]
pub(super) fn UserAttachment(attachment: ChatAttachment, onload: EventHandler<()>) -> Element {
    let mut playing = use_signal(|| false);
    let mut started = use_signal(|| false);
    let mut failed = use_signal(|| false);
    let media_id = use_hook(|| format!("chat-video-{}", uuid::Uuid::new_v4()));
    if attachment.mime_type.starts_with("video/") && !failed() {
        let play_id = media_id.clone();
        return rsx! {
            figure {
                key: "message-attachment-{attachment.path}",
                class: "w-96 max-w-full min-w-0 overflow-hidden rounded-xl bg-background/50",
                div { class: "relative aspect-video w-full",
                    video {
                        id: media_id,
                        src: (!attachment.media_url.is_empty()).then(|| format!("{}#t=0.001", attachment.media_url)),
                        preload: "metadata",
                        playsinline: true,
                        controls: started(),
                        class: "h-full w-full object-contain",
                        onloadeddata: move |_| onload.call(()),
                        onplay: move |_| { started.set(true); playing.set(true); },
                        onpause: move |_| playing.set(false),
                        onended: move |_| playing.set(false),
                        onerror: move |_| { failed.set(true); onload.call(()); },
                    }
                    if !playing() {
                        button {
                            r#type: "button",
                            class: "absolute inset-0 flex cursor-pointer items-center justify-center bg-black/10",
                            aria_label: translate("agent-video-play"),
                            disabled: attachment.media_url.is_empty(),
                            onclick: move |_| vmux_ui::media::MediaElement::with_id(&play_id).toggle_playback(),
                            span { class: "flex size-12 items-center justify-center rounded-full bg-black/60 text-white backdrop-blur-sm transition-transform hover:scale-105 motion-reduce:transition-none",
                                svg { class: "size-6", view_box: "0 0 24 24", fill: "currentColor", "aria-hidden": "true",
                                    path { d: "M8 5v14l11-7z" }
                                }
                            }
                        }
                    }
                }
                figcaption { class: "truncate px-3 py-2 text-xs text-muted-foreground", ToolFileLink { path: attachment.path.clone() } }
            }
        };
    }
    let preview_data_url = &attachment.preview_data_url;
    if attachment.mime_type.starts_with("image/") && !preview_data_url.is_empty() {
        return rsx! {
            figure {
                key: "message-attachment-{attachment.path}",
                class: "w-full overflow-hidden rounded-xl",
                title: "{attachment.name}",
                img {
                    src: "{preview_data_url}",
                    alt: "{attachment.name}",
                    loading: "lazy",
                    decoding: "async",
                    class: "max-h-80 w-full object-cover",
                    onload: move |_| onload.call(()),
                }
            }
        };
    }
    rsx! {
        div {
            key: "message-attachment-{attachment.path}",
            class: "flex min-w-32 max-w-64 items-center gap-2 rounded-xl bg-foreground/[0.06] px-3 py-2 ring-1 ring-inset ring-foreground/10",
            span { class: "font-mono text-[10px] font-semibold tracking-wide text-muted-foreground", "{FilePath(&attachment.name).extension_label()}" }
            span { class: "truncate text-xs text-muted-foreground", ToolFileLink { path: attachment.path.clone() } }
        }
    }
}

struct ToolDisclosure<'a> {
    turn: &'a ChatTurn,
    latest: bool,
    last_text: Option<usize>,
    completed: HashSet<&'a str>,
}

#[cfg(test)]
mod disclosure_tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation};
    use dioxus::prelude::Event as UiEvent;
    use dioxus_html::{
        Code, EventData, Key, Location, Modifiers, SerializedHtmlEventConverter,
        SerializedKeyboardData, SerializedMouseData, SerializedResizeData,
    };
    use std::rc::Rc;
    use vmux_api::BinEvent;
    use vmux_ui::transport::{
        BytesListener, HostBinding, PageHost, event_listener::EventListenerError,
    };

    #[component]
    fn VideoAttachmentFixture() -> Element {
        rsx! { UserAttachment {
            attachment: ChatAttachment {
                path: "/tmp/clip.mov".into(),
                name: "clip.mov".into(),
                mime_type: "video/mp4".into(),
                media_url: "http://127.0.0.1:12345/capability".into(),
                ..Default::default()
            },
            onload: move |_| {},
        } }
    }

    #[test]
    fn video_attachment_has_a_preview_play_button_and_no_autoplay() {
        let mut dom = VirtualDom::new(VideoAttachmentFixture);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("<video"));
        assert!(html.contains("src=\"http://127.0.0.1:12345/capability#t=0.001\""));
        assert!(html.contains(&translate("agent-video-play")));
        assert!(!html.contains("autoplay"));
    }

    #[component]
    fn PastedVideoFixture() -> Element {
        rsx! { ChatItemRow {
            absolute_index: 0,
            item: ChatItem::User { text: r"inspect /tmp/Screen\ Recording.mov".into(), context: None, attachments: Vec::new(), created_at_ms: 0 },
            agent_id: String::new(), agent_name: String::new(), agent_avatar: None, agent_color: String::new(),
            user_name: "Junichi".to_string(), user_color: "#888888".to_string(),
            on_media_load: move |_| {},
        } }
    }

    #[test]
    fn pasted_video_reserves_its_preview_before_host_hydration() {
        let mut dom = VirtualDom::new(PastedVideoFixture);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("<video"));
        assert!(html.contains("disabled"));
        assert!(html.contains(r"inspect /tmp/Screen\ Recording.mov"));
    }

    #[derive(Default)]
    struct FileHost(RefCell<Vec<OpenRequest>>);

    impl PageHost for FileHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == OpenRequest::id() {
                self.0
                    .borrow_mut()
                    .push(rkyv::from_bytes::<OpenRequest, rkyv::rancor::Error>(bytes).unwrap());
            }
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[test]
    fn read_only_exploration_stays_inside_the_expandable_call_details() {
        let arguments = ChatToolArguments::Raw("cat /work/skills/caveman/SKILL.md".into());
        let mut call = VirtualDom::new_with_props(
            ToolCall,
            ToolCallProps {
                call: ChatToolCall {
                    kind: vmux_api::chat::ChatToolKind::Command,
                    arguments: arguments.clone(),
                    ..Default::default()
                },
            },
        );
        call.rebuild_in_place();
        let mut child = VirtualDom::new_with_props(
            ToolChildCall,
            ToolChildCallProps {
                call: ChatToolChildCall {
                    kind: vmux_api::chat::ChatToolKind::Command,
                    arguments,
                    ..Default::default()
                },
            },
        );
        child.rebuild_in_place();
        for dom in [&call, &child] {
            let html = dioxus_ssr::render(dom);
            let summary = html
                .split("<summary")
                .nth(1)
                .unwrap()
                .split("</summary>")
                .next()
                .unwrap();
            assert!(summary.contains(&translate("agent-tool-read-files")));
            assert!(summary.contains("caveman/SKILL.md"));
            let (details, following) = html.split_once("</details>").unwrap();
            let read = translate_with(
                "agent-tool-explore-read",
                &[("path", TranslationValue::String("caveman/SKILL.md"))],
            );
            assert!(details.contains(&read));
            assert!(!following.contains(&read));
        }
    }

    #[test]
    fn output_summaries_preview_one_line_and_keep_the_full_output_expandable() {
        for error in [false, true] {
            let content = "\nFirst result\nSecond result".to_string();
            let mut nested = VirtualDom::new_with_props(
                NestedToolResult,
                NestedToolResultProps {
                    result_key: 0,
                    content: content.clone(),
                    is_error: error,
                },
            );
            nested.rebuild_in_place();
            let mut standalone = VirtualDom::new_with_props(
                StandaloneToolResult,
                StandaloneToolResultProps {
                    result_key: 0,
                    content,
                    is_error: error,
                },
            );
            standalone.rebuild_in_place();
            for dom in [&nested, &standalone] {
                let html = dioxus_ssr::render(dom);
                let summary = html
                    .split("<summary")
                    .nth(1)
                    .unwrap()
                    .split("</summary>")
                    .next()
                    .unwrap();
                assert!(summary.contains("First result"));
                assert!(!summary.contains("Second result"));
                let full = html
                    .split("<pre")
                    .nth(1)
                    .unwrap()
                    .split("</pre>")
                    .next()
                    .unwrap();
                assert!(full.contains("First result\nSecond result"));
            }
        }
        let content = "Script completed\nWall time 0.2 seconds\nOutput:\n{\"exit_code\":0,\"output\":\"Actual output\\nRemaining output\"}";
        assert_eq!(OutputPreview::new(content).0, "Actual output");
        assert_eq!(OutputPreview::new("Script completed\n\nclean").0, "clean");
        assert_eq!(
            OutputPreview::new("Script completed successfully\n\nDetails").0,
            "Script completed successfully"
        );
        assert_eq!(
            OutputPreview::new("First line\n\nSecond line").0,
            "First line"
        );
    }

    #[test]
    fn folded_reads_and_batches_keep_clickable_file_names() {
        let path = "/work/skills/caveman/SKILL.md";
        let call = ChatToolCall {
            kind: vmux_api::chat::ChatToolKind::ReadFile,
            arguments: ChatToolArguments::Raw(format!(
                "text(await tools.exec_command({{cmd: \"cat {path}\"}}));"
            )),
            ..Default::default()
        };
        let mut row = VirtualDom::new_with_props(ToolCall, ToolCallProps { call: call.clone() });
        row.rebuild_in_place();
        let mut batch = VirtualDom::new_with_props(
            FinishedToolCalls,
            FinishedToolCallsProps {
                calls: vec![call],
                open: false,
            },
        );
        batch.rebuild_in_place();
        for dom in [&row, &batch] {
            let html = dioxus_ssr::render(dom);
            let summary = html
                .split("<summary")
                .nth(1)
                .unwrap()
                .split("</summary>")
                .next()
                .unwrap();
            assert!(summary.contains("caveman/SKILL.md"));
            assert!(summary.contains("href=\"file:///work/skills/caveman/SKILL.md\""));
        }
    }

    #[test]
    fn file_references_resolve_relative_paths_and_preserve_editor_locations() {
        let file = FileReference::new("src/a #?.rs", "/work/p root");
        assert_eq!(
            file.url.as_deref(),
            Some("file:///work/p%20root/src/a%20%23%3F.rs")
        );
        assert_eq!(file.label, "src/a #?.rs");
        let file = FileReference::new("file:///work/src/main.rs#L9", "/work");
        assert_eq!(file.url.as_deref(), Some("file:///work/src/main.rs#L9"));
        assert_eq!(file.label, "src/main.rs");
        assert!(FileReference::new("src/main.rs", "").url.is_none());
        assert!(
            FileReference::new("https://example.com/main.rs", "/work")
                .url
                .is_none()
        );
    }

    #[test]
    fn file_clicks_send_native_open_requests_without_toggling_the_disclosure() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        for (meta, shift, target) in [
            (false, false, OpenTarget::InNewStack),
            (true, false, OpenTarget::InNewStack),
            (true, true, OpenTarget::InNewTab),
        ] {
            let host = Rc::new(FileHost::default());
            let mut dom = VirtualDom::new_with_props(
                ToolFileLink,
                ToolFileLinkProps {
                    path: "/work/a #?.rs".into(),
                    read: false,
                },
            );
            dom.provide_root_context(HostBinding(host.clone()));
            let edits = dom.rebuild_to_vec();
            let link = edits
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "href",
                        value: AttributeValue::Text(url),
                        id,
                        ..
                    } if url == "file:///work/a%20%23%3F.rs" => Some(*id),
                    _ => None,
                })
                .unwrap();
            let mut data = serde_json::to_value(SerializedMouseData::default()).unwrap();
            data["meta_key"] = meta.into();
            data["shift_key"] = shift.into();
            let mouse = serde_json::from_value::<SerializedMouseData>(data).unwrap();
            let event = UiEvent::new(EventData::Mouse(mouse).into_any(), true);
            dom.runtime().handle_event("click", event.clone(), link);
            assert!(!event.default_action_enabled());
            assert!(!event.propagates());
            let opened = host.0.borrow();
            assert_eq!(opened.len(), 1);
            assert_eq!(opened[0].value, "file:///work/a%20%23%3F.rs");
            assert_eq!(opened[0].open, Some(target));
        }
    }

    #[component]
    fn ResponsiveFileLink() -> Element {
        let placement = FilePlacement::provide();
        rsx! {
            main {
                onresize: move |event: UiEvent<ResizeData>| placement.resize(&event.data()),
                ToolFileLink { path: "/work/main.rs" }
            }
        }
    }

    #[test]
    fn file_clicks_adapt_to_pane_resizing_and_keep_modifier_destinations() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(FileHost::default());
        let mut dom = VirtualDom::new(ResponsiveFileLink);
        dom.provide_root_context(HostBinding(host.clone()));
        let edits = dom.rebuild_to_vec();
        let resize = edits
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "resize" => Some(*id),
                _ => None,
            })
            .unwrap();
        let link = edits
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "href",
                    value: AttributeValue::Text(url),
                    id,
                    ..
                } if url == "file:///work/main.rs" => Some(*id),
                _ => None,
            })
            .unwrap();
        for (width, meta, shift, target) in [
            (800.0, false, false, OpenTarget::InNewStack),
            (
                1600.0,
                false,
                false,
                OpenTarget::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::Existing,
                    mode: PaneOpenMode::NewStack,
                },
            ),
            (900.0, false, false, OpenTarget::InNewStack),
            (1600.0, true, false, OpenTarget::InNewStack),
            (1600.0, true, true, OpenTarget::InNewTab),
        ] {
            let size = dioxus_html::geometry::PixelsSize::new(width, 900.0);
            dom.runtime().handle_event(
                "resize",
                UiEvent::new(
                    EventData::Resize(SerializedResizeData::new(size, size)).into_any(),
                    true,
                ),
                resize,
            );
            dom.render_immediate_to_vec();
            let mut data = serde_json::to_value(SerializedMouseData::default()).unwrap();
            data["meta_key"] = meta.into();
            data["shift_key"] = shift.into();
            let mouse = serde_json::from_value::<SerializedMouseData>(data).unwrap();
            let event = UiEvent::new(EventData::Mouse(mouse).into_any(), true);
            dom.runtime().handle_event("click", event.clone(), link);
            assert!(!event.default_action_enabled());
            assert!(!event.propagates());
            assert_eq!(host.0.borrow().last().unwrap().open, Some(target));
        }
        assert_eq!(host.0.borrow().len(), 5);
    }

    #[test]
    fn file_context_menu_opens_the_editor_in_the_chosen_container() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        for (index, target) in [
            OpenTarget::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
            },
            OpenTarget::InNewStack,
            OpenTarget::InNewTab,
        ]
        .into_iter()
        .enumerate()
        {
            let host = Rc::new(FileHost::default());
            let mut dom = VirtualDom::new(MenuFileLink);
            dom.provide_root_context(HostBinding(host.clone()));
            let edits = dom.rebuild_to_vec();
            let trigger = edits
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "href",
                        value: AttributeValue::Text(url),
                        id,
                        ..
                    } if url == "file:///work/main.rs" => Some(*id),
                    _ => None,
                })
                .unwrap();
            dom.runtime().handle_event(
                "contextmenu",
                UiEvent::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                trigger,
            );
            let opened = dom.render_immediate_to_vec();
            let html = dioxus_ssr::render(&dom);
            let (row, popup) = html.split_once("</details>").unwrap();
            assert!(!row.contains("role=\"menu\""));
            assert!(popup.contains("role=\"menu\""));
            let buttons = opened
                .edits
                .iter()
                .filter_map(|edit| match edit {
                    Mutation::NewEventListener { name, id } if name == "pointerenter" => Some(*id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(buttons.len(), 3);
            dom.runtime().handle_event(
                "keydown",
                UiEvent::new(
                    EventData::Keyboard(SerializedKeyboardData::new(
                        Key::Enter,
                        Code::Enter,
                        Location::Standard,
                        false,
                        Modifiers::empty(),
                        false,
                    ))
                    .into_any(),
                    true,
                ),
                buttons[index],
            );
            let opened = host.0.borrow();
            assert_eq!(opened.len(), 1);
            assert_eq!(opened[0].value, "file:///work/main.rs");
            assert_eq!(opened[0].open, Some(target));
        }
    }

    #[component]
    fn MenuFileLink() -> Element {
        rsx! {
            ToolFileMenu {
                details { summary { ToolFileLink { path: "/work/main.rs" } } }
            }
        }
    }

    #[test]
    fn command_rows_show_exploration_steps_and_real_commands_instead_of_the_wrapper() {
        let call = ChatToolCall {
            kind: vmux_api::chat::ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(r#"text(await tools.exec_command({cmd:"sed -n '1,10p' src/host.rs; rg -n 'fixed' crates"}));"#.into()),
            ..Default::default()
        };
        let mut dom = VirtualDom::new_with_props(ToolCall, ToolCallProps { call });
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(&translate("agent-tool-explored")));
        assert!(html.contains("host.rs"));
        assert!(html.contains("fixed"));
        assert!(!html.contains("tools.exec_command"));
    }

    #[component]
    fn ToolBatch(first_done: bool, second_done: bool, text: bool, latest: bool) -> Element {
        let mut blocks = Vec::new();
        let mut calls = Vec::new();
        for (index, call_id) in ["first", "second"].into_iter().enumerate() {
            blocks.push(ChatBlock::ToolUse {
                call_id: call_id.into(),
                name: "read_file".into(),
                args: "{}".into(),
                parent_call_id: None,

                kind: None,
            });
            calls.push(ChatToolCall {
                index: index as u32,
                name: "read_file".into(),
                fallback_label: call_id.into(),
                ..Default::default()
            });
        }
        for (done, call_id) in [(first_done, "first"), (second_done, "second")] {
            if done {
                blocks.push(ChatBlock::ToolResult {
                    call_id: call_id.into(),
                    content: "done".into(),
                    is_error: false,
                });
            }
        }
        let mut rows = vec![ChatTurnRow::FinishedTools { index: 0, calls }];
        if text {
            rows.push(ChatTurnRow::Text {
                index: blocks.len() as u32,
                text: "Continue".into(),
            });
            blocks.push(ChatBlock::Text("Continue".into()));
        }
        rsx! {
            TurnView {
                turn_index: 0,
                turn: ChatTurn { blocks, rows, ..Default::default() },
                agent_id: "codex", agent_name: "Codex",
                agent_avatar: None, agent_color: String::new(), latest,
            }
        }
    }

    #[test]
    fn latest_tool_batch_stays_open_until_all_results_and_the_following_text_arrive() {
        for (first_done, second_done, text, latest, open) in [
            (false, false, false, true, true),
            (true, true, false, true, true),
            (true, false, true, true, true),
            (true, true, true, true, false),
            (false, false, false, false, false),
        ] {
            let mut dom = VirtualDom::new_with_props(
                ToolBatch,
                ToolBatchProps {
                    first_done,
                    second_done,
                    text,
                    latest,
                },
            );
            dom.rebuild_in_place();
            let html = dioxus_ssr::render(&dom);
            let disclosure = html
                .split("<details")
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert_eq!(disclosure.contains(" open"), open, "{html}");
        }
    }
}

impl<'a> ToolDisclosure<'a> {
    fn new(turn: &'a ChatTurn, latest: bool) -> Self {
        let mut last_text = None;
        let mut completed = HashSet::new();
        for (index, block) in turn.blocks.iter().enumerate() {
            match block {
                ChatBlock::Text(text) if !text.trim().is_empty() => last_text = Some(index),
                ChatBlock::ToolResult { call_id, .. } => {
                    completed.insert(call_id.as_str());
                }
                _ => {}
            }
        }
        Self {
            turn,
            latest,
            last_text,
            completed,
        }
    }

    fn expanded(&self, row: &ChatTurnRow) -> bool {
        if !self.latest {
            return false;
        }
        let ChatTurnRow::FinishedTools { calls, .. } = row else {
            return false;
        };
        for call in calls {
            if self
                .last_text
                .is_none_or(|index| call.index as usize > index)
            {
                return true;
            }
            if let Some(ChatBlock::ToolUse { call_id, .. }) =
                self.turn.blocks.get(call.index as usize)
                && !self.completed.contains(call_id.as_str())
            {
                return true;
            }
        }
        false
    }
}

#[component]
pub fn TurnView(
    turn_index: usize,
    turn: ChatTurn,
    agent_id: String,
    agent_name: String,
    agent_avatar: Option<String>,
    agent_color: String,
    #[props(default)] latest: bool,
) -> Element {
    let key = turn_index;
    let turn = &turn;
    let tools = ToolDisclosure::new(turn, latest);
    let reconnecting = matches!(turn.rows.last(), Some(ChatTurnRow::Reconnect { .. }));
    let live_text_index = turn
        .running
        .then(|| {
            turn.rows.iter().rev().find_map(|row| match row {
                ChatTurnRow::Text { index, .. } => Some(*index),
                _ => None,
            })
        })
        .flatten();
    let duration_label = turn.duration_secs.map(|duration| {
        if turn.step_count == 0 {
            let elapsed = Elapsed(duration).to_string();
            translate_with(
                "agent-worked-for",
                &[("duration", TranslationValue::String(&elapsed))],
            )
        } else if turn.step_count == 1 {
            let elapsed = Elapsed(duration).to_string();
            translate_with(
                "agent-worked-for-steps",
                &[
                    ("duration", TranslationValue::String(&elapsed)),
                    ("count", TranslationValue::Number(1)),
                ],
            )
        } else {
            let elapsed = Elapsed(duration).to_string();
            translate_with(
                "agent-worked-for-steps",
                &[
                    ("duration", TranslationValue::String(&elapsed)),
                    ("count", TranslationValue::Number(turn.step_count as i64)),
                ],
            )
        }
    });
    rsx! {
        div {
            key: "{key}",
            class: "flex w-full flex-col gap-2 [contain-intrinsic-size:auto_180px] [content-visibility:auto]",
            "data-vmux-scroll-layout": "",
            if !turn.rows.is_empty() || turn.running || duration_label.is_some() {
                AssistantTurn {
                    agent_id,
                    name: agent_name,
                    avatar_src: agent_avatar,
                    avatar_background: agent_color,
                    copy_text: turn.copy_text.clone(),
                    created_at_ms: turn.created_at_ms,
                    for row in turn.rows.iter().cloned() {
                        TurnBlock {
                            key: "{row.index()}",
                            streaming: live_text_index == Some(row.index()),
                            expanded: tools.expanded(&row),
                            row,
                        }
                    }
                    if turn.running && !reconnecting {
                        WorkingIndicator {}
                    } else if let Some(label) = duration_label {
                        div { class: "flex items-center gap-2 text-xs text-muted-foreground/70",
                            span { class: "h-1.5 w-1.5 rounded-full bg-[color:var(--agent-accent)]" }
                            span { class: "tabular-nums", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

const ROW_SUMMARY: &str = "flex min-h-6 cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden";

#[component]
fn DisclosureIcon() -> Element {
    rsx! {
        span { class: "disclosure-icon flex size-3 shrink-0 items-center justify-center text-muted-foreground", aria_hidden: "true",
            LineIconView { icon: LineIcon::ChevronDown, class: "size-3" }
        }
    }
}

#[component]
pub fn WorkingIndicator() -> Element {
    let mut elapsed = use_signal(|| 0u32);
    let mut verb = use_signal(|| translate("agent-working-working"));
    use_future(move || async move {
        loop {
            Platform::sleep(1000).await;
            elapsed.set(elapsed() + 1);
        }
    });
    use_future(move || async move {
        loop {
            Platform::sleep(2500).await;
            verb.set(translate(
                WORKING_VERB_IDS[Platform::random_index(WORKING_VERB_IDS.len())],
            ));
        }
    });
    let verb_text = verb();
    let elapsed_text = Elapsed(elapsed()).to_string();
    rsx! {
        div { class: "flex items-center gap-2 px-1 text-xs text-muted-foreground",
            span { class: "animate-pulse font-medium motion-reduce:animate-none", "{verb_text}" }
            span { class: "flex items-end gap-0.5 text-[color:var(--agent-accent)]",
                span { class: "h-1 w-1 animate-bounce rounded-full bg-current motion-reduce:animate-none" }
                span { class: "h-1 w-1 animate-bounce rounded-full bg-current [animation-delay:120ms] motion-reduce:animate-none" }
                span { class: "h-1 w-1 animate-bounce rounded-full bg-current [animation-delay:240ms] motion-reduce:animate-none" }
            }
            span { class: "tabular-nums text-xs", "{elapsed_text}" }
        }
    }
}

#[component]
fn ToolArg(argument: ChatToolArgument) -> Element {
    let key = argument.name;
    let label = argument.label;
    let row_class = "flex min-w-0 items-center gap-3 py-1.5";
    let label_class =
        "shrink-0 text-[10px] font-medium uppercase tracking-[0.1em] text-muted-foreground/80";
    match argument.value {
        ChatToolArgumentValue::Path(text) => rsx! {
            div { class: "{row_class}",
                {rsx! { TypeIcon { path: text.to_string(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-85" } }}
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        ChatToolArgumentValue::Code(text) => rsx! {
            div { class: "py-1.5",
                if !key.is_empty() {
                    div { class: "mb-1.5 flex items-center gap-1.5 {label_class}",
                        span { class: "h-1.5 w-1.5 rounded-full bg-success/70" }
                        "{label}"
                    }
                }
                pre { class: "max-h-56 overflow-auto whitespace-pre-wrap break-words rounded-md bg-foreground/[0.035] px-2 py-1.5 font-mono text-[11px] leading-relaxed text-foreground/80", "{text}" }
            }
        },
        ChatToolArgumentValue::Text(text) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        ChatToolArgumentValue::Bool(value) => {
            let tone = if value {
                "bg-success/10 text-success ring-success/20"
            } else {
                "bg-foreground/[0.04] text-muted-foreground ring-foreground/10"
            };
            rsx! {
                div { class: "{row_class}",
                    if !key.is_empty() {
                        span { class: "{label_class}", "{label}" }
                    }
                    span { class: "rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ring-inset {tone}", "{value}" }
                }
            }
        }
        ChatToolArgumentValue::Number(value) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "ml-auto font-mono text-[11px] tabular-nums text-cyan-600 dark:text-cyan-300", "{value}" }
            }
        },
        ChatToolArgumentValue::List(values) => rsx! {
            div { class: "py-1",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "flex flex-col rounded-md bg-foreground/[0.025] px-2",
                    for value in values {
                        ToolArg { argument: value }
                    }
                }
            }
        },
        ChatToolArgumentValue::Object(values) => rsx! {
            div { class: "py-1",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "flex flex-col rounded-md bg-foreground/[0.025] px-2",
                    for value in values {
                        ToolArg { argument: value }
                    }
                }
            }
        },
        ChatToolArgumentValue::Null => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                span { class: "ml-auto text-[10px] italic text-muted-foreground/70", "None" }
            }
        },
    }
}

#[component]
fn ToolArguments(arguments: ChatToolArguments) -> Element {
    match arguments {
        ChatToolArguments::None => rsx! {},
        ChatToolArguments::Fields(fields) => rsx! {
            div { class: "mt-2 flex flex-col rounded-lg bg-foreground/[0.035] px-3 py-1", aria_label: "Tool arguments",
                for argument in fields {
                    ToolArg { argument }
                }
            }
        },
        ChatToolArguments::Value(value) => rsx! {
            div { class: "mt-2 rounded-lg bg-foreground/[0.035] px-3 py-1",
                ToolArg { argument: ChatToolArgument { name: String::new(), label: String::new(), value } }
            }
        },
        ChatToolArguments::Raw(raw) => rsx! {
            pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{raw}" }
        },
    }
}

#[component]
fn FinishedToolCalls(calls: Vec<ChatToolCall>, open: bool) -> Element {
    let count = calls.len() as i64;
    let mut paths = Vec::new();
    for call in &calls {
        for path in call.file_paths() {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    rsx! {
        details { class: "disclosure", open,
            summary { class: "flex cursor-pointer select-none items-center gap-2 rounded-xl px-2 py-1 text-xs text-muted-foreground list-none transition-colors hover:bg-foreground/[0.025] [&::-webkit-details-marker]:hidden",
                DisclosureIcon {}
                span { class: "font-medium",
                    {translate_with("agent-tool-calls", &[("count", TranslationValue::Number(count))])}
                }
                ToolFiles { paths }
            }
            div { class: "mt-1 flex flex-col gap-1",
                for call in calls {
                    ToolCall { call }
                }
            }
        }
    }
}

#[component]
fn ToolFiles(paths: Vec<String>) -> Element {
    rsx! {
        span { class: "flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5",
            for path in paths {
                span { class: "min-w-0 max-w-full", ToolFileLink { key: "{path}", path } }
            }
        }
    }
}

#[component]
fn ToolFileLink(path: String, #[props(default)] read: bool) -> Element {
    let placement = try_consume_context::<FilePlacement>();
    let menu = try_consume_context::<FileMenu>();
    let binding =
        try_consume_context::<vmux_ui::hooks::UiStateBinding<crate::state::ChatUiState>>();
    let cwd = binding
        .map(|binding| binding.state.read().composer.cwd.clone())
        .unwrap_or_default();
    let file = FileReference::new(&path, &cwd);
    let label = if read {
        translate_with(
            "agent-tool-explore-read",
            &[("path", TranslationValue::String(&file.label))],
        )
    } else {
        file.label.clone()
    };
    let Some(url) = file.url else {
        return rsx! { span { class: "truncate font-mono text-[11px]", title: file.path, "{label}" } };
    };
    let clicked_url = url.clone();
    let link = rsx! {
        a {
                    href: url.clone(),
                    title: file.path,
                    class: "inline-block max-w-full truncate align-middle font-mono text-[11px] text-foreground/65 underline-offset-2 transition-colors hover:text-primary hover:underline",
                    onclick: move |event: MouseEvent| {
                        event.prevent_default();
                        event.stop_propagation();
                        let width = placement.map(|placement| *placement.0.peek()).unwrap_or(0.0);
                        let target = FilePlacement::target(width, NavigationGesture::from_mouse(&event));
                        let _ = vmux_ui::hooks::send(&OpenRequest { value: clicked_url.clone(), open: Some(target) });
                    },
                    oncontextmenu: move |_| {
                        if let Some(menu) = menu {
                            menu.select(url.clone());
                        }
                    },
                    "{label}"
        }
    };
    if menu.is_some() {
        rsx! { ContextMenuTrigger { attributes: vec![], {link} } }
    } else {
        link
    }
}

#[component]
pub(super) fn ToolFileMenu(children: Element) -> Element {
    let menu = FileMenu(use_signal(String::new));
    use_context_provider(|| menu);
    let url = (menu.0)();
    rsx! {
        ContextMenu { attributes: vec![],
            {children}
            ContextMenuContent { attributes: vec![],
                if !url.is_empty() {
                ToolFileOpenItem {
                    index: 0usize, url: url.clone(),
                    target: OpenTarget::InPane { direction: PaneDirection::Right, target: PaneTarget::NewSplit, mode: PaneOpenMode::NewStack },
                    label: "agent-tool-open-in-pane-right",
                }
                ToolFileOpenItem { index: 1usize, url: url.clone(), target: OpenTarget::InNewStack, label: "agent-tool-open-in-stack" }
                ToolFileOpenItem { index: 2usize, url, target: OpenTarget::InNewTab, label: "agent-tool-open-in-tab" }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct FileMenu(Signal<String>);

impl FileMenu {
    fn select(mut self, url: String) {
        self.0.set(url);
    }
}

#[derive(Clone, Copy)]
pub(super) struct FilePlacement(Signal<f64>);

impl FilePlacement {
    pub(super) fn provide() -> Self {
        let placement = Self(use_signal(|| 0.0));
        use_context_provider(|| placement)
    }

    pub(super) fn resize(mut self, data: &ResizeData) {
        if let Ok(size) = data.get_content_box_size() {
            self.0.set(size.width);
        }
    }

    fn target(width: f64, gesture: NavigationGesture) -> OpenTarget {
        let requested = gesture.open_target();
        if requested != OpenTarget::InPlace {
            return requested;
        }
        if width.is_finite() && width >= 1280.0 {
            return OpenTarget::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::NewStack,
            };
        }
        OpenTarget::InNewStack
    }
}

#[component]
fn ToolFileOpenItem(index: usize, url: String, target: OpenTarget, label: &'static str) -> Element {
    let value = use_signal(|| label.to_string());
    rsx! {
        ContextMenuItem {
            index,
            value: Into::<ReadSignal<String>>::into(value),
            attributes: vec![],
            on_select: move |_: String| {
                let _ = vmux_ui::hooks::send(&OpenRequest { value: url.clone(), open: Some(target) });
            },
            {translate(label)}
        }
    }
}

struct FileReference {
    path: String,
    label: String,
    url: Option<String>,
}

impl FileReference {
    fn new(path: &str, cwd: &str) -> Self {
        let existing = url::Url::parse(path).ok();
        let path = if let Some(url) = &existing {
            let Ok(path) = url.to_file_path() else {
                return Self {
                    path: path.to_string(),
                    label: path.to_string(),
                    url: None,
                };
            };
            path
        } else {
            let path = std::path::PathBuf::from(path);
            if path.is_absolute() || cwd.is_empty() {
                path
            } else {
                std::path::Path::new(cwd).join(path)
            }
        };
        let displayed = if !cwd.is_empty() {
            path.strip_prefix(cwd).unwrap_or(&path)
        } else {
            &path
        };
        let label = if displayed.components().count() > 2 {
            let name = displayed.file_name().unwrap_or_default();
            match displayed.parent().and_then(std::path::Path::file_name) {
                Some(parent) => std::path::Path::new(parent)
                    .join(name)
                    .to_string_lossy()
                    .into_owned(),
                None => name.to_string_lossy().into_owned(),
            }
        } else {
            displayed.to_string_lossy().into_owned()
        };
        let url = match existing {
            Some(url) => Some(url.to_string()),
            None => url::Url::from_file_path(&path)
                .ok()
                .map(|url| url.to_string()),
        };
        Self {
            path: path.to_string_lossy().into_owned(),
            label,
            url,
        }
    }
}

#[component]
fn ToolExploration(steps: Vec<ExploreStep>) -> Element {
    rsx! {
        div { class: "mt-1 flex min-w-0 flex-col gap-0.5 text-[11px] text-muted-foreground",
            for step in steps {
                {
                    match step {
                        ExploreStep::Read { path } => rsx! { ToolFileLink { path, read: true } },
                        ExploreStep::Search { query, path } => {
                            let message = translate_with("agent-tool-explore-search", &[("query", TranslationValue::String(&query)), ("path", TranslationValue::String(&path))]);
                            rsx! { div { class: "truncate", title: message.clone(), "{message}" } }
                        }
                        ExploreStep::List { path } => {
                            let message = translate_with("agent-tool-explore-list", &[("path", TranslationValue::String(&path))]);
                            rsx! { div { class: "truncate", title: message.clone(), "{message}" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ToolCall(call: ChatToolCall) -> Element {
    let key = call.index;
    let paths = call.file_paths();
    let edits = call.edited_paths();
    let command = if edits.is_empty() {
        call.command()
    } else {
        None
    };
    let exploration = command.as_ref().and_then(|command| command.exploration());
    let label = if !edits.is_empty() {
        ToolPresentation::for_call(
            vmux_api::chat::ChatToolKind::WriteFile,
            &call.fallback_label,
        )
        .label
    } else if let Some(steps) = &exploration {
        ToolPresentation::for_exploration(steps).label
    } else {
        ToolPresentation::for_call(call.kind, &call.fallback_label).label
    };
    let children = call.children.clone();
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-lg px-2 py-1.5 transition-colors hover:bg-foreground/[0.025]",
            ToolActivityIcon {
                name: call.name.clone(),
                file_path: call.file_path.clone(),
                activity: if !edits.is_empty() { vmux_api::chat::ChatActivityKind::WriteFile } else if exploration.is_some() { vmux_api::chat::ChatActivityKind::Search } else { call.activity },
            }
            div { class: "min-w-0",
                details { open: false, class: "disclosure text-xs text-muted-foreground",
                    summary { class: ROW_SUMMARY,
                        DisclosureIcon {}
                        span { class: "shrink-0 font-medium", "{label}" }
                        ToolFiles { paths }
                        if exploration.is_none() && let Some(command) = &command {
                            div { class: "min-w-0 flex-1 overflow-hidden",
                                ShellCode {
                                    source: command.preview.clone(),
                                    inline: true,
                                    class: "block truncate".to_string(),
                                }
                            }
                        }
                    }
                    if let Some(command) = command {
                        div { class: "mt-1.5 max-h-56 overflow-auto rounded-lg bg-foreground/[0.035] px-2.5 py-2",
                            ShellCode { source: command.source }
                        }
                        ToolArguments { arguments: command.metadata }
                    } else {
                        div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{call.name}" }
                        ToolArguments { arguments: call.arguments.clone() }
                    }
                    if let Some(steps) = exploration { ToolExploration { steps } }
                }
                if !children.is_empty() {
                    div { class: "mt-1.5 flex flex-col gap-1",
                        for child in children {
                            ToolChild { key: "{child.index()}", child }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SubagentRow(subagent: ChatSubagentState) -> Element {
    let key = subagent.index;
    let status_label = subagent_status_label(subagent.status);
    let status_class = subagent_status_class(subagent.status);
    let title = if subagent.title.is_empty() {
        translate("agent-subagent")
    } else {
        subagent.title.clone()
    };
    let children = subagent.children.clone();
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl bg-violet-500/[0.025] px-2 py-1.5 ring-1 ring-inset ring-violet-500/10 transition-colors hover:bg-violet-500/[0.05]",
            ActivityIconView { kind: ActivityIcon::Subagent }
            div { class: "min-w-0",
                details { open: subagent.status == ChatSubagentStatus::Running, class: "disclosure text-sm text-muted-foreground",
                    summary { class: "{ROW_SUMMARY} flex-wrap",
                        DisclosureIcon {}
                        span { class: "font-medium text-foreground/85", "{title}" }
                        span { class: "rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                    }
                    div { class: "mt-2 flex flex-wrap gap-1.5 text-[10px]",
                        span { class: "rounded-full bg-violet-500/10 px-2 py-0.5 font-semibold text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                        if !subagent.activity.is_empty() {
                            span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{subagent.activity}" }
                        }
                        if let Some(agent_name) = &subagent.agent_name {
                            span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{agent_name}" }
                        }
                        if let Some(model) = &subagent.model {
                            span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 font-mono text-foreground/60", "{model}" }
                        }
                        if let Some(effort) = &subagent.reasoning_effort {
                            span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{effort}" }
                        }
                    }
                    if let Some(prompt) = &subagent.prompt {
                        div { class: "mt-2 rounded-lg bg-foreground/[0.025] p-2 text-xs leading-relaxed text-foreground/75 ring-1 ring-inset ring-foreground/10",
                            div { class: "mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground/70", {translate("agent-prompt")} }
                            div { class: "whitespace-pre-wrap", "{prompt}" }
                        }
                    }
                    div { class: "mt-2 grid gap-1 text-[10px] text-muted-foreground/75",
                        if let Some(thread_id) = &subagent.thread_id {
                            div { span { class: "font-semibold", {format!("{} ", translate("agent-thread"))} } code { class: "font-mono", "{thread_id}" } }
                        }
                        if let Some(parent_thread_id) = &subagent.parent_thread_id {
                            div { span { class: "font-semibold", {format!("{} ", translate("agent-parent"))} } code { class: "font-mono", "{parent_thread_id}" } }
                        }
                        if !subagent.child_threads.is_empty() {
                            div { span { class: "font-semibold", {format!("{} ", translate("agent-children"))} } code { class: "break-all font-mono", "{subagent.child_threads}" } }
                        }
                        div { span { class: "font-semibold", {format!("{} ", translate("agent-call"))} } code { class: "font-mono", "{subagent.call_id}" } }
                    }
                    if !subagent.raw_input.is_empty() && subagent.raw_input != "{}" {
                        details { class: "disclosure mt-2 text-[11px] text-muted-foreground",
                            summary { class: ROW_SUMMARY,
                                DisclosureIcon {}
                                span { class: "font-medium", {translate("agent-raw-event")} }
                            }
                            pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground", "{subagent.raw_input}" }
                        }
                    }
                }
                if !children.is_empty() {
                    div { class: "mt-2 flex flex-col gap-1 rounded-lg bg-foreground/[0.025] px-2 py-1",
                        for child in children {
                            ToolChild { key: "{child.index()}", child }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TurnBlock(
    row: ChatTurnRow,
    #[props(default)] streaming: bool,
    #[props(default)] expanded: bool,
) -> Element {
    let key = row.index();
    match row {
        ChatTurnRow::Text { text, .. } => {
            rsx! {
                StreamingMarkdown { key: "{key}", text, streaming }
            }
        }
        ChatTurnRow::Thinking { text, .. } => rsx! {
            div { key: "{key}", class: "agent-row-hover grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors",
                ActivityIconView { kind: ActivityIcon::Thinking }
                details { open: false, class: "disclosure min-w-0 text-xs text-muted-foreground",
                    summary { class: ROW_SUMMARY,
                        DisclosureIcon {}
                        span { class: "font-medium", {translate("agent-thinking")} }
                    }
                    div { class: "mt-2 whitespace-pre-wrap border-l border-foreground/15 pl-3 text-xs leading-relaxed", "{text}" }
                }
            }
        },
        ChatTurnRow::Tool(call) => rsx! { ToolCall { call } },
        ChatTurnRow::FinishedTools { calls, .. } => {
            rsx! { FinishedToolCalls { calls, open: expanded } }
        }
        ChatTurnRow::Subagent(subagent) => rsx! { SubagentRow { subagent } },
        ChatTurnRow::Plan { steps, .. } => {
            let n = steps.len();
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-indigo-500/[0.035]",
                    ActivityIconView { kind: ActivityIcon::Plan }
                    details { open: true, class: "disclosure min-w-0 text-sm",
                        summary { class: ROW_SUMMARY,
                            DisclosureIcon {}
                            span { class: "font-medium text-foreground/80", {translate("agent-plan")} }
                            span {
                                class: "text-xs text-muted-foreground",
                                {translate_with(
                                    "agent-tasks",
                                    &[("count", TranslationValue::Number(n as i64))],
                                )}
                            }
                        }
                        ul { class: "mt-2 flex flex-col gap-1.5 border-l border-indigo-500/20 pl-3",
                            for (i , step) in steps.iter().enumerate() {
                                li { key: "{i}", class: "flex items-start gap-2 text-xs",
                                    span { class: "mt-px {plan_glyph_class(step.status)}", "{plan_glyph(step.status)}" }
                                    span { class: plan_text_class(step.status), "{step.content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatTurnRow::Diff(diff) => {
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-success/[0.035]",
                    FileActivityIcon { path: diff.path.clone(), write: true }
                    details { class: "disclosure min-w-0 text-sm text-muted-foreground",
                        summary { class: ROW_SUMMARY,
                            DisclosureIcon {}
                            span { class: "font-medium", {format!("{} ", translate("agent-edited"))} }
                            code { class: "truncate font-mono text-xs text-foreground/70", "{diff.name}" }
                        }
                        div { class: "mt-2 overflow-hidden rounded-lg ring-1 ring-inset ring-foreground/10",
                            div { class: "overflow-x-auto bg-foreground/[0.02] py-1 font-mono text-[11px] leading-relaxed",
                                for (i , line) in diff.lines.iter().enumerate() {
                                    div { key: "{i}", class: diff_line_class(line.kind), "{diff_line_prefix(line.kind)} {line.text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatTurnRow::ToolResult {
            content, is_error, ..
        } => rsx! {
            StandaloneToolResult { result_key: key as usize, content, is_error }
        },
        ChatTurnRow::Reconnect { attempt, total, .. } => rsx! {
            div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-2.5 rounded-xl px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-amber-500/[0.035]",
                ActivityIconView { kind: ActivityIcon::Reconnect }
                span {
                    class: "font-medium tabular-nums",
                    {translate_with(
                        "agent-reconnecting",
                        &[
                            ("attempt", TranslationValue::Number(attempt as i64)),
                            ("total", TranslationValue::Number(total as i64)),
                        ],
                    )}
                }
            }
        },
    }
}

#[component]
fn ToolChild(child: ChatToolChild) -> Element {
    match child {
        ChatToolChild::Tool(call) => rsx! { ToolChildCall { call } },
        ChatToolChild::Subagent(subagent) => rsx! { SubagentChild { subagent } },
        ChatToolChild::Result {
            index,
            content,
            is_error,
        } => rsx! {
            NestedToolResult { result_key: index as usize, content, is_error }
        },
    }
}

#[component]
fn ToolChildCall(call: ChatToolChildCall) -> Element {
    let paths = call.file_paths();
    let edits = call.edited_paths();
    let command = if edits.is_empty() {
        call.command()
    } else {
        None
    };
    let exploration = command.as_ref().and_then(|command| command.exploration());
    let label = if !edits.is_empty() {
        ToolPresentation::for_call(
            vmux_api::chat::ChatToolKind::WriteFile,
            &call.fallback_label,
        )
        .label
    } else if let Some(steps) = &exploration {
        ToolPresentation::for_exploration(steps).label
    } else {
        ToolPresentation::for_call(call.kind, &call.fallback_label).label
    };
    rsx! {
        details { class: "disclosure text-xs text-muted-foreground",
            summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                DisclosureIcon {}
                span { class: "shrink-0 font-medium", "{label}" }
                ToolFiles { paths }
                if exploration.is_none() && let Some(command) = &command {
                    div { class: "min-w-0 flex-1 overflow-hidden",
                        ShellCode {
                            source: command.preview.clone(),
                            inline: true,
                            class: "block truncate".to_string(),
                        }
                    }
                }
            }
            if let Some(command) = command {
                div { class: "mt-1.5 max-h-56 overflow-auto rounded-lg bg-foreground/[0.035] px-2.5 py-2",
                    ShellCode { source: command.source }
                }
                ToolArguments { arguments: command.metadata }
            } else {
                div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{call.name}" }
                ToolArguments { arguments: call.arguments }
            }
            if let Some(steps) = exploration { ToolExploration { steps } }
        }
    }
}

#[component]
fn SubagentChild(subagent: ChatSubagentSummary) -> Element {
    let status_label = subagent_status_label(subagent.status);
    let status_class = subagent_status_class(subagent.status);
    let title = if subagent.title.is_empty() {
        translate("agent-subagent")
    } else {
        subagent.title.clone()
    };
    rsx! {
        details { class: "disclosure text-xs text-muted-foreground",
            summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                DisclosureIcon {}
                span { class: "font-medium", "{title}" }
                span { class: "rounded-full px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
            }
            div { class: "mt-1 flex flex-wrap gap-1 text-[10px]",
                span { class: "rounded-full bg-violet-500/10 px-1.5 py-0.5 text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                if let Some(agent_name) = &subagent.agent_name {
                    span { class: "rounded-full bg-foreground/[0.055] px-1.5 py-0.5", "{agent_name}" }
                }
            }
            if let Some(prompt) = &subagent.prompt {
                div { class: "mt-1.5 whitespace-pre-wrap rounded-lg bg-foreground/[0.025] p-2 text-[11px] leading-relaxed ring-1 ring-inset ring-foreground/10", "{prompt}" }
            }
        }
    }
}

fn subagent_status_label(status: ChatSubagentStatus) -> String {
    match status {
        ChatSubagentStatus::Running => translate("agent-status-running"),
        ChatSubagentStatus::Complete => translate("agent-status-done"),
        ChatSubagentStatus::Failed => translate("agent-status-failed"),
        ChatSubagentStatus::Pending => translate("agent-status-pending"),
    }
}

fn subagent_status_class(status: ChatSubagentStatus) -> &'static str {
    match status {
        ChatSubagentStatus::Running => "bg-violet-500/10 text-violet-700 dark:text-violet-300",
        ChatSubagentStatus::Complete => "bg-success/10 text-success",
        ChatSubagentStatus::Failed => "bg-red-500/10 text-red-700 dark:text-red-300",
        ChatSubagentStatus::Pending => "bg-amber-500/10 text-amber-700 dark:text-amber-300",
    }
}

#[component]
fn NestedToolResult(result_key: usize, content: String, is_error: bool) -> Element {
    let key = result_key;
    let content = content.as_str();
    let preview = OutputPreview::new(content);
    let tone = if is_error {
        "text-red-500"
    } else {
        "text-foreground/70"
    };
    let panel = if is_error {
        "bg-red-500/[0.045]"
    } else {
        "bg-foreground/[0.03]"
    };
    let label = if is_error {
        translate("common-error")
    } else {
        translate("common-output")
    };
    rsx! {
        details { key: "{key}", class: "disclosure group/output rounded-lg px-0 py-1 text-xs {tone} {panel}",
            summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                DisclosureIcon {}
                span { class: "font-medium", "{label}" }
                span { class: "min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground group-open/output:hidden", "{preview.0}" }
            }
            pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap px-0.5 pb-1 font-mono text-[11px] leading-relaxed text-muted-foreground", "{content}" }
        }
    }
}

#[component]
fn StandaloneToolResult(result_key: usize, content: String, is_error: bool) -> Element {
    let key = result_key;
    let content = content.as_str();
    let preview = OutputPreview::new(content);
    let tone = if is_error {
        "text-red-500"
    } else {
        "text-foreground/70"
    };
    let panel = if is_error {
        "bg-red-500/[0.045]"
    } else {
        "bg-foreground/[0.03]"
    };
    let row = if is_error {
        "hover:bg-red-500/[0.035]"
    } else {
        "hover:bg-foreground/[0.025]"
    };
    let label = if is_error {
        translate("common-error")
    } else {
        translate("common-output")
    };
    let icon = if is_error {
        ActivityIcon::Error
    } else {
        ActivityIcon::Output
    };
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors {row}",
            ActivityIconView { kind: icon }
            details { class: "disclosure group/output min-w-0 rounded-lg px-0 py-1 text-sm {tone} {panel}",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    DisclosureIcon {}
                    span { class: "font-medium", "{label}" }
                    span { class: "min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground group-open/output:hidden", "{preview.0}" }
                }
                pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap px-0.5 pb-1 font-mono text-[11px] leading-relaxed text-muted-foreground", "{content}" }
            }
        }
    }
}

struct OutputPreview(String);

impl OutputPreview {
    fn new(content: &str) -> Self {
        let mut body = content.trim();
        if (body.starts_with("Script completed") || body.starts_with("Script running"))
            && let Some((_, output)) = body.split_once("\nOutput:\n")
        {
            body = output.trim();
        } else if let Some(output) = body.strip_prefix("Script completed\n\n") {
            body = output.trim();
        }
        if let Ok(serde_json::Value::Object(value)) = serde_json::from_str(body)
            && (value.contains_key("exit_code") || value.contains_key("chunk_id"))
            && let Some(output) = value.get("output").and_then(serde_json::Value::as_str)
        {
            return Self::line(output);
        }
        Self::line(body)
    }

    fn line(content: &str) -> Self {
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() {
                return Self(line.to_string());
            }
        }
        Self(String::new())
    }
}

struct Elapsed(u32);

impl std::fmt::Display for Elapsed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 >= 60 {
            return write!(formatter, "{}:{:02}", self.0 / 60, self.0 % 60);
        }
        write!(formatter, "{}s", self.0)
    }
}

fn plan_glyph(status: ChatPlanStatus) -> &'static str {
    match status {
        ChatPlanStatus::Complete => "✓",
        ChatPlanStatus::Active => "◐",
        ChatPlanStatus::Pending => "○",
    }
}

fn plan_glyph_class(status: ChatPlanStatus) -> &'static str {
    match status {
        ChatPlanStatus::Complete => "text-success",
        ChatPlanStatus::Active => "text-amber-500",
        ChatPlanStatus::Pending => "text-muted-foreground",
    }
}

fn plan_text_class(status: ChatPlanStatus) -> &'static str {
    match status {
        ChatPlanStatus::Complete => "text-muted-foreground line-through",
        ChatPlanStatus::Active => "text-foreground",
        ChatPlanStatus::Pending => "text-muted-foreground",
    }
}

fn diff_line_class(kind: ChatDiffLineKind) -> &'static str {
    match kind {
        ChatDiffLineKind::Removed => "px-3 bg-red-500/10 text-red-300",
        ChatDiffLineKind::Added => "px-3 bg-success/10 text-success",
    }
}

fn diff_line_prefix(kind: ChatDiffLineKind) -> &'static str {
    match kind {
        ChatDiffLineKind::Removed => "-",
        ChatDiffLineKind::Added => "+",
    }
}

const CHAT_MD_CLASS: &str = "chat-md px-0.5 text-sm text-foreground/95 leading-[1.6] break-words \
    [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 \
    [&_:is(h1,h2,h3,h4)]:font-semibold [&_:is(h1,h2,h3,h4)]:leading-[1.3] \
    [&_:is(h1,h2,h3,h4)]:[margin:0.9em_0_0.35em] \
    [&_h1]:text-[1.35em] [&_h2]:text-[1.2em] [&_h3]:text-[1.05em] [&_h4]:text-[1em] \
    [&_p]:my-[0.5em] \
    [&_:is(ul,ol)]:pl-[1.4em] [&_:not(li)>:is(ul,ol)]:my-[0.4em] [&_li>:is(ul,ol)]:my-[0.15em] \
    [&_ul]:list-disc [&_ol]:list-decimal [&_li]:my-[0.15em] \
    [&_strong]:font-semibold [&_em]:italic \
    [&_a]:text-[color-mix(in_srgb,var(--agent-accent)_82%,currentColor)] \
    [&_a]:decoration-[color-mix(in_srgb,var(--agent-accent)_45%,transparent)] \
    [&_a]:underline-offset-[0.16em] \
    [&_:not(pre)>code]:font-mono [&_:not(pre)>code]:text-[0.88em] \
    [&_:not(pre)>code]:bg-foreground/[0.055] \
    [&_:not(pre)>code]:[padding:0.1em_0.35em] [&_:not(pre)>code]:rounded-[0.4em] \
    [&_pre]:bg-foreground/[0.045] \
    [&_pre]:[padding:0.7em_0.9em] [&_pre]:rounded-[0.7em] [&_pre]:overflow-x-auto \
    [&_pre]:my-[0.6em] [&_pre]:font-mono [&_pre_code]:text-[0.85em] \
    [&_blockquote]:border-l-2 \
    [&_blockquote]:border-l-[color-mix(in_srgb,var(--agent-accent)_48%,transparent)] \
    [&_blockquote]:pl-[0.8em] [&_blockquote]:my-[0.5em] [&_blockquote]:opacity-85 \
    [&_hr]:border-0 [&_hr]:border-t [&_hr]:border-t-[rgba(127,127,127,0.25)] [&_hr]:my-[0.9em] \
    [&_table]:border-collapse [&_table]:my-[0.5em] [&_table]:text-[0.95em] \
    [&_:is(th,td)]:border [&_:is(th,td)]:border-[rgba(127,127,127,0.3)] \
    [&_:is(th,td)]:[padding:0.3em_0.6em] [&_:is(th,td)]:text-left";

fn md_to_html(src: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(src, opts)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[component]
fn StreamingMarkdown(text: String, streaming: bool) -> Element {
    let previous = use_hook(|| RefCell::new(String::new()));
    let html = if streaming {
        let previous_text = previous.borrow().clone();
        md_to_streaming_html(&text, &previous_text)
    } else {
        md_to_html(&text)
    };
    previous.replace(text.clone());
    rsx! {
        div {
            class: CHAT_MD_CLASS,
            dangerous_inner_html: html,
        }
    }
}

fn md_to_streaming_html(src: &str, previous: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let stable = shared_visible_prefix(
        &markdown_visible_text(previous),
        &markdown_visible_text(src),
    );
    let mut visible_offset = 0;
    let mut word_index = 0;
    let mut events = Vec::new();
    for event in Parser::new_ext(src, opts)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)))
    {
        match event {
            Event::Text(text) => {
                let text = text.to_string();
                let end = visible_offset + text.len();
                if end <= stable {
                    events.push(Event::Text(CowStr::Boxed(text.into_boxed_str())));
                } else {
                    let split = stable.saturating_sub(visible_offset).min(text.len());
                    if split > 0 {
                        events.push(Event::Text(CowStr::Boxed(
                            text[..split].to_string().into_boxed_str(),
                        )));
                    }
                    push_animated_words(&mut events, &text[split..], &mut word_index);
                }
                visible_offset = end;
            }
            Event::Code(text) => {
                visible_offset += text.len();
                events.push(Event::Code(text));
            }
            event => events.push(event),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

fn markdown_visible_text(src: &str) -> String {
    let mut visible = String::new();
    for event in Parser::new_ext(src, Options::empty()) {
        match event {
            Event::Text(text) | Event::Code(text) => visible.push_str(&text),
            _ => {}
        }
    }
    visible
}

fn shared_visible_prefix(previous: &str, current: &str) -> usize {
    let mut shared = 0;
    for (left, right) in previous.chars().zip(current.chars()) {
        if left != right {
            break;
        }
        shared += left.len_utf8();
    }
    shared
}

fn push_animated_words<'a>(events: &mut Vec<Event<'a>>, text: &str, word_index: &mut usize) {
    let mut start = 0;
    while start < text.len() {
        let whitespace = text[start..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace);
        let end = text[start..]
            .char_indices()
            .skip(1)
            .find(|(_, character)| character.is_whitespace() != whitespace)
            .map(|(index, _)| start + index)
            .unwrap_or(text.len());
        let segment = &text[start..end];
        if whitespace {
            events.push(Event::Text(CowStr::Boxed(
                segment.to_string().into_boxed_str(),
            )));
        } else {
            let delay = (*word_index * 35).min(280);
            let opening =
                format!("<span class=\"chat-stream-word\" style=\"animation-delay:{delay}ms\">");
            events.push(Event::Html(CowStr::Boxed(opening.into_boxed_str())));
            events.push(Event::Text(CowStr::Boxed(
                segment.to_string().into_boxed_str(),
            )));
            events.push(Event::Html(CowStr::Borrowed("</span>")));
            *word_index += 1;
        }
        start = end;
    }
}

pub const MD_CSS: &str = r#"
.session-chat-page{background-image:none}
@keyframes chat-message-enter{from{opacity:0;transform:translateY(4px)}to{opacity:1;transform:translateY(0)}}
.chat-message-enter{animation:chat-message-enter 180ms cubic-bezier(.16,1,.3,1) both}
@keyframes chat-stream-word-enter{from{opacity:.18;filter:blur(1.5px)}to{opacity:1;filter:blur(0)}}
.chat-stream-word{animation:chat-stream-word-enter 180ms cubic-bezier(.16,1,.3,1) both}
.chat-assistant-turn .disclosure>summary{transition:color 160ms ease}
.chat-assistant-turn .disclosure>summary:hover{color:color-mix(in srgb,currentColor 68%,var(--agent-accent))}
.agent-themed-activity{color:var(--agent-accent);background:color-mix(in srgb,var(--agent-accent) 11%,transparent);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--agent-accent) 18%,transparent)}
.agent-working-label{color:color-mix(in srgb,var(--agent-accent) 82%,currentColor)}
.agent-row-hover:hover{background:color-mix(in srgb,var(--agent-accent) 4%,transparent)}
.agent-code-panel,.user-context-content{background:rgba(127,127,127,0.07)}
.agent-context-tree{border-color:color-mix(in srgb,var(--agent-accent) 22%,transparent)}
.agent-turn-meta{color:color-mix(in srgb,var(--agent-accent) 72%,currentColor);border-color:color-mix(in srgb,var(--agent-accent) 13%,transparent);background:color-mix(in srgb,var(--agent-accent) 7%,transparent)}
.agent-turn-meta-dot{background:var(--agent-accent)}
.user-context-panel{border-color:color-mix(in srgb,var(--agent-accent) 14%,transparent);background:color-mix(in srgb,var(--agent-accent) 5%,rgba(127,127,127,0.025))}
.user-context-panel>summary:hover{color:color-mix(in srgb,currentColor 65%,var(--agent-accent))}
@media (prefers-reduced-motion:reduce){.agent-chat-caret,.chat-message-enter,.chat-stream-word{animation:none}}
"#;
