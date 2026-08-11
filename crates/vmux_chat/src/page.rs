//! The chat page itself: one conversation's transcript, approvals and composer.
//!
//! Gated once here rather than per module, so what ships to wasm and iOS is this file and the
//! directory beside it. The desktop half that feeds it lives outside this crate, and speaks to
//! it only through the bin-ipc payloads in [`crate::event`].

#![allow(non_snake_case)]

use self::agent::ChatHeader;
use self::approval::ChatApprovalPanel;
use self::composer::ChatDock;
use self::keys::use_chat_keys;
use self::state::use_chat;
use self::transcript::ChatTranscript;
use crate::event::ChatAttachment;
use crate::transcript::MD_CSS;
use dioxus::prelude::*;
#[cfg(web)]
use vmux_terminal::matrix_rain::MatrixRain;

/// One agent conversation: its transcript, whatever it is waiting on, and the composer.
#[component]
pub fn Page(
    #[props(default)] agent_override: Option<String>,
    #[props(default)] transition_prompt: Option<String>,
    #[props(default)] transition_attachments: Option<Vec<ChatAttachment>>,
) -> Element {
    let chat = use_chat(agent_override, transition_prompt, transition_attachments);
    let keys = use_chat_keys(chat);
    use_context_provider(|| keys);
    let accent = chat.accent();

    rsx! {
        main {
            class: "agent-chat-page relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{accent.css};",
            // Focusable so a click on the transcript lands focus here rather than on the body,
            // which would put keystrokes out of reach of the handler below. Deliberately not
            // autofocused: `focus_prompt_end` already claims focus for the prompt on mount.
            tabindex: "-1",
            onkeydown: move |event| keys.on_root_keydown(event),
            style { dangerous_inner_html: MD_CSS }
            if chat.installing_splash() {
                InstallBackdrop { accent_rgb: accent.rgb, title: chat.header_name().to_uppercase() }
            }
            ChatHeader { chat }
            ChatTranscript { chat }
            ChatApprovalPanel { chat }
            ChatDock { chat }
        }
    }
}

/// The falling-glyphs backdrop shown while an agent installs.
///
/// `MatrixRain` is a canvas animation and exists only on the CEF host. Installing an agent is a
/// desktop act anyway, so a native host renders nothing rather than an approximation.
#[cfg(web)]
#[component]
fn InstallBackdrop(accent_rgb: String, title: String) -> Element {
    rsx! {
        div { class: "pointer-events-none absolute inset-0 z-0 overflow-hidden bg-background opacity-75",
            MatrixRain { accent_rgb, words: vec![title] }
        }
    }
}

#[cfg(not(web))]
#[component]
fn InstallBackdrop(accent_rgb: String, title: String) -> Element {
    // The prop names have to match the CEF impl, since callers name them.
    let _ = (accent_rgb, title);
    rsx! {}
}

pub mod agent;
mod approval;
pub mod composer;
mod error;
mod keys;
mod scroll;
mod state;
mod tab;
mod transcript;
