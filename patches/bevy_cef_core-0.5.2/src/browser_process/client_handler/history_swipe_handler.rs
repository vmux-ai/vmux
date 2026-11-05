use crate::browser_process::client_handler::ProcessMessageHandler;
use crate::prelude::{IntoString, PROCESS_MESSAGE_HISTORY_SWIPE};
use async_channel::Sender;
use bevy::prelude::Entity;
use cef::{Browser, Frame, ImplFrame, ImplListValue, ListValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistorySwipeDirection {
    Back,
    Forward,
}

impl HistorySwipeDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            _ => None,
        }
    }
}

pub struct HistorySwipeRaw {
    pub webview: Entity,
    pub page_url: String,
    pub token: String,
    pub direction: HistorySwipeDirection,
}

impl HistorySwipeRaw {
    pub fn valid_token(token: &str) -> bool {
        token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

pub struct HistorySwipeHandler {
    webview: Entity,
    sender: Sender<HistorySwipeRaw>,
}

impl HistorySwipeHandler {
    pub fn new(webview: Entity, sender: Sender<HistorySwipeRaw>) -> Self {
        Self { webview, sender }
    }
}

impl ProcessMessageHandler for HistorySwipeHandler {
    fn process_name(&self) -> &'static str {
        PROCESS_MESSAGE_HISTORY_SWIPE
    }

    fn handle_message(&self, _browser: &mut Browser, frame: &mut Frame, args: Option<ListValue>) {
        if frame.is_main() == 0 {
            return;
        }
        let Some(args) = args else {
            return;
        };
        let Some(direction) = HistorySwipeDirection::parse(&args.string(0).into_string()) else {
            return;
        };
        let token = args.string(1).into_string();
        if !HistorySwipeRaw::valid_token(&token) {
            return;
        }
        let _ = self.sender.try_send(HistorySwipeRaw {
            webview: self.webview,
            page_url: frame.url().into_string(),
            token,
            direction,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_channel_accepts_only_a_direction_and_fixed_size_document_token() {
        assert_eq!(
            HistorySwipeDirection::parse("back"),
            Some(HistorySwipeDirection::Back)
        );
        assert_eq!(
            HistorySwipeDirection::parse("forward"),
            Some(HistorySwipeDirection::Forward)
        );
        for value in ["reload", "https://example.com", "", "back:forward"] {
            assert!(HistorySwipeDirection::parse(value).is_none());
        }
        assert!(HistorySwipeRaw::valid_token(
            "0123456789abcdef0123456789abcdef"
        ));
        for value in [
            "",
            "0123456789abcdef",
            "0123456789abcdef0123456789abcdeg",
            "0123456789abcdef0123456789abcdef00",
        ] {
            assert!(!HistorySwipeRaw::valid_token(value));
        }
    }
}
