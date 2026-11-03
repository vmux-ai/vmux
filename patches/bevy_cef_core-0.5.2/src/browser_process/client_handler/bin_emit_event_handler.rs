use crate::browser_process::client_handler::ProcessMessageHandler;
use crate::prelude::{PROCESS_MESSAGE_BIN_DOCUMENT_CONTEXT, PROCESS_MESSAGE_BIN_JS_EMIT};
use crate::util::IntoString;
use async_channel::Sender;
use bevy::prelude::{Component, Entity};
use cef::{Browser, Frame, ImplBinaryValue, ImplFrame, ImplListValue, ListValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinIpcEventRaw {
    pub webview: Entity,
    pub page_url: String,
    pub surface_generation: Option<u64>,
    pub navigation_revision: Option<u64>,
    pub ipc_epoch: Option<u64>,
    pub id: String,
    pub payload: Vec<u8>,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinIpcNavigationRevision(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinIpcSurfaceGeneration(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinIpcBlocked;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinIpcEpoch(pub u64);

#[derive(Clone, Debug)]
pub struct BinIpcContext {
    surface_generation: u64,
    navigation_epoch: Arc<AtomicU64>,
    navigation_revision: Arc<AtomicU64>,
    minimum_navigation_revision: Arc<AtomicU64>,
    page_url: Arc<RwLock<String>>,
    document_token: Arc<RwLock<String>>,
    pending_document: Arc<Mutex<Option<PendingDocument>>>,
    blocked: Arc<AtomicBool>,
    ipc_epoch: Arc<AtomicU64>,
}

#[derive(Clone, Debug)]
struct PendingDocument {
    token: String,
    navigation_revision: u64,
    page_url: String,
}

impl BinIpcContext {
    pub fn new(
        surface_generation: u64,
        page_url: impl Into<String>,
        initially_blocked: bool,
    ) -> Self {
        Self {
            surface_generation,
            navigation_epoch: Arc::new(AtomicU64::new(1)),
            navigation_revision: Arc::new(AtomicU64::new(0)),
            minimum_navigation_revision: Arc::new(AtomicU64::new(1)),
            page_url: Arc::new(RwLock::new(page_url.into())),
            document_token: Arc::new(RwLock::new(String::new())),
            pending_document: Arc::new(Mutex::new(None)),
            blocked: Arc::new(AtomicBool::new(initially_blocked)),
            ipc_epoch: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn surface_generation(&self) -> u64 {
        self.surface_generation
    }

    pub fn navigation_revision(&self) -> u64 {
        self.navigation_revision.load(Ordering::Acquire)
    }

    pub fn navigation_epoch(&self) -> u64 {
        self.navigation_epoch.load(Ordering::Acquire)
    }

    pub fn ipc_epoch(&self) -> u64 {
        self.ipc_epoch.load(Ordering::Acquire)
    }

    pub fn accepts_epoch(&self, epoch: u64) -> bool {
        self.ipc_epoch() == epoch
    }

    pub fn authority_token(
        &self,
        surface_generation: u64,
        navigation_revision: u64,
        ipc_epoch: u64,
    ) -> Option<String> {
        if self.blocked.load(Ordering::Acquire)
            || self.surface_generation != surface_generation
            || self.navigation_revision() != navigation_revision
            || !self.accepts_epoch(ipc_epoch)
        {
            return None;
        }
        let token = self.document_token.read().ok()?.clone();
        let (_, token_generation, token_revision) = document_identity_token(&token)?;
        (token_generation == surface_generation && token_revision == navigation_revision)
            .then_some(token)
    }

    pub fn current_authority(&self) -> Option<(String, u64)> {
        let generation = self.surface_generation();
        let revision = self.navigation_revision();
        let epoch = self.ipc_epoch();
        self.authority_token(generation, revision, epoch)
            .map(|token| (token, epoch))
    }

    pub fn set_blocked(&self, blocked: bool) {
        if self.blocked.swap(blocked, Ordering::AcqRel) != blocked {
            self.ipc_epoch.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn navigate(&self, page_url: &str) -> u64 {
        if let Ok(mut current) = self.page_url.write() {
            current.clear();
            current.push_str(page_url);
        }
        self.invalidate()
    }

    pub fn reload(&self) -> u64 {
        self.invalidate()
    }

    pub fn history(&self) -> u64 {
        if let Ok(mut current) = self.page_url.write() {
            current.clear();
        }
        self.invalidate()
    }

    pub fn commit(&self, page_url: &str) -> u64 {
        if let Some(pending) = self.take_pending_document(page_url) {
            if let Ok(mut current) = self.page_url.write() {
                current.clear();
                current.push_str(page_url);
            }
            if self.accept_document(pending) {
                return self.navigation_revision();
            }
        }
        if let Ok(mut current) = self.page_url.write()
            && current.is_empty()
        {
            current.push_str(page_url);
            return self.navigation_revision();
        }
        if self.accepts(page_url) {
            self.navigation_revision()
        } else {
            self.navigate(page_url)
        }
    }

    fn invalidate(&self) -> u64 {
        self.navigation_epoch.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut token) = self.document_token.write() {
            token.clear();
        }
        if let Ok(mut pending) = self.pending_document.lock() {
            pending.take();
        }
        let current = self.navigation_revision.swap(0, Ordering::AcqRel);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        self.minimum_navigation_revision
            .fetch_max(now.max(current.wrapping_add(1).max(1)), Ordering::AcqRel);
        0
    }

    fn bind_document(
        &self,
        token: String,
        surface_generation: u64,
        navigation_revision: u64,
        page_url: &str,
    ) -> bool {
        if surface_generation != self.surface_generation || navigation_revision == 0 {
            return false;
        }
        let minimum = self.minimum_navigation_revision.load(Ordering::Acquire);
        if navigation_revision < minimum {
            return false;
        }
        let pending = PendingDocument {
            token,
            navigation_revision,
            page_url: page_url.to_string(),
        };
        if !self.can_bind(page_url) {
            if let Ok(mut stored) = self.pending_document.lock() {
                stored.replace(pending);
            }
            return false;
        }
        self.accept_document(pending)
    }

    fn accept_document(&self, pending: PendingDocument) -> bool {
        let mut current = self.navigation_revision();
        loop {
            if current > pending.navigation_revision {
                return false;
            }
            match self.navigation_revision.compare_exchange_weak(
                current,
                pending.navigation_revision,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
        self.minimum_navigation_revision
            .fetch_max(pending.navigation_revision, Ordering::AcqRel);
        if let Ok(mut current) = self.page_url.write() {
            current.clear();
            current.push_str(&pending.page_url);
        }
        if let Ok(mut token) = self.document_token.write() {
            token.clear();
            token.push_str(&pending.token);
        }
        if let Ok(mut stored) = self.pending_document.lock() {
            stored.take();
        }
        true
    }

    fn matches_token(&self, token: &str) -> bool {
        self.navigation_revision() != 0
            && self
                .document_token
                .read()
                .is_ok_and(|current| current.as_str() == token)
    }

    pub fn accepts(&self, page_url: &str) -> bool {
        self.page_url
            .read()
            .is_ok_and(|current| current.as_str() == page_url)
    }

    fn can_bind(&self, page_url: &str) -> bool {
        self.page_url
            .read()
            .is_ok_and(|current| current.is_empty() || current.as_str() == page_url)
    }

    fn take_pending_document(&self, page_url: &str) -> Option<PendingDocument> {
        let mut pending = self.pending_document.lock().ok()?;
        if pending
            .as_ref()
            .is_some_and(|pending| pending.page_url == page_url)
        {
            pending.take()
        } else {
            None
        }
    }
}

const BIN_IPC_ENVELOPE_MAGIC: &[u8] = b"vmux-bin-ipc-v1\0";

fn decode_bin_ipc_envelope(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    let id_len_start = BIN_IPC_ENVELOPE_MAGIC.len();
    let id_start = id_len_start + 4;
    if bytes.len() < id_start || !bytes.starts_with(BIN_IPC_ENVELOPE_MAGIC) {
        return None;
    }
    let id_len = u32::from_le_bytes(bytes[id_len_start..id_start].try_into().ok()?) as usize;
    let payload_start = id_start.checked_add(id_len)?;
    if bytes.len() < payload_start {
        return None;
    }
    let id = std::str::from_utf8(&bytes[id_start..payload_start])
        .ok()?
        .to_string();
    Some((id, bytes[payload_start..].to_vec()))
}

#[cfg(test)]
mod bin_ipc_envelope_tests {
    use super::*;

    fn encode_test_envelope(id: &str, payload: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(BIN_IPC_ENVELOPE_MAGIC);
        encoded.extend_from_slice(&(id.len() as u32).to_le_bytes());
        encoded.extend_from_slice(id.as_bytes());
        encoded.extend_from_slice(payload);
        encoded
    }

    #[test]
    fn decode_bin_ipc_envelope_extracts_id_and_payload() {
        let encoded = encode_test_envelope("event-id", &[1, 2, 3]);

        let (id, payload) = decode_bin_ipc_envelope(&encoded).expect("envelope");

        assert_eq!(id, "event-id");
        assert_eq!(payload, vec![1, 2, 3]);
    }

    #[test]
    fn decode_bin_ipc_envelope_ignores_legacy_payload() {
        assert!(decode_bin_ipc_envelope(&[1, 2, 3]).is_none());
    }
}

pub struct BinEmitEventHandler {
    webview: Entity,
    sender: Sender<BinIpcEventRaw>,
    context: BinIpcContext,
}

pub struct BinDocumentContextHandler {
    context: BinIpcContext,
}

impl BinDocumentContextHandler {
    pub fn new(context: BinIpcContext) -> Self {
        Self { context }
    }
}

impl ProcessMessageHandler for BinDocumentContextHandler {
    fn process_name(&self) -> &'static str {
        PROCESS_MESSAGE_BIN_DOCUMENT_CONTEXT
    }

    fn handle_message(&self, _browser: &mut Browser, frame: &mut Frame, args: Option<ListValue>) {
        let Some(args) = args else {
            return;
        };
        let Some((token, surface_generation, navigation_revision)) = document_identity(&args, 0)
        else {
            return;
        };
        self.context.bind_document(
            token,
            surface_generation,
            navigation_revision,
            &frame.url().into_string(),
        );
    }
}

impl BinEmitEventHandler {
    pub fn new(webview: Entity, sender: Sender<BinIpcEventRaw>, context: BinIpcContext) -> Self {
        Self {
            sender,
            webview,
            context,
        }
    }
}

impl ProcessMessageHandler for BinEmitEventHandler {
    fn process_name(&self) -> &'static str {
        PROCESS_MESSAGE_BIN_JS_EMIT
    }

    fn handle_message(&self, _browser: &mut Browser, frame: &mut Frame, args: Option<ListValue>) {
        let Some(args) = args else {
            return;
        };
        let page_url = frame.url().into_string();
        if !self.context.accepts(&page_url) {
            return;
        }
        let token = args.string(2).into_string();
        if !self.context.matches_token(&token) {
            return;
        }
        let Ok(ipc_epoch) = args.string(3).into_string().parse::<u64>() else {
            return;
        };
        if !self.context.accepts_epoch(ipc_epoch) {
            return;
        }
        let surface_generation = self.context.surface_generation();
        let navigation_revision = self.context.navigation_revision();
        let id = args.string(0).into_string();
        let payload_index = if id.is_empty() { 0 } else { 1 };
        let payload = match args.binary(payload_index) {
            Some(binary) => {
                let len = binary.size();
                let mut buf = vec![0u8; len];
                binary.data(Some(&mut buf), 0);
                buf
            }
            None => Vec::new(),
        };
        let (id, payload) = if id.is_empty() {
            decode_bin_ipc_envelope(&payload).unwrap_or((id, payload))
        } else {
            (id, payload)
        };
        let _ = self.sender.send_blocking(BinIpcEventRaw {
            webview: self.webview,
            page_url,
            surface_generation: Some(surface_generation),
            navigation_revision: Some(navigation_revision),
            ipc_epoch: Some(ipc_epoch),
            id,
            payload,
        });
    }
}

fn document_identity(args: &ListValue, index: usize) -> Option<(String, u64, u64)> {
    let token = args.string(index).into_string();
    document_identity_token(&token)
}

fn document_identity_token(token: &str) -> Option<(String, u64, u64)> {
    if token.len() != 32 {
        return None;
    }
    let surface_generation = u64::from_str_radix(&token[..16], 16).ok()?;
    let navigation_revision = u64::from_str_radix(&token[16..], 16).ok()?;
    (surface_generation != 0 && navigation_revision != 0).then_some((
        token.to_string(),
        surface_generation,
        navigation_revision,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    fn token(surface_generation: u64, navigation_revision: u64) -> String {
        format!("{surface_generation:016x}{navigation_revision:016x}")
    }

    #[test]
    fn bin_ipc_event_raw_construction() {
        let webview = Entity::PLACEHOLDER;
        let payload = vec![1, 2, 3, 4];
        let raw = BinIpcEventRaw {
            webview,
            page_url: "vmux://history/".to_string(),
            surface_generation: None,
            navigation_revision: None,
            ipc_epoch: None,
            id: "test-id".to_string(),
            payload: payload.clone(),
        };
        assert_eq!(raw.webview, webview);
        assert_eq!(raw.page_url, "vmux://history/");
        assert_eq!(raw.id, "test-id");
        assert_eq!(raw.payload, payload);
    }

    #[test]
    fn context_advances_navigation_without_changing_surface_generation() {
        let context = BinIpcContext::new(4, "vmux://projects/", false);

        assert_eq!(context.surface_generation(), 4);
        assert_eq!(context.navigation_epoch(), 1);
        assert_eq!(context.navigation_revision(), 0);
        assert!(context.accepts("vmux://projects/"));
        assert!(context.bind_document(token(4, 7), 4, 7, "vmux://projects/"));
        assert_eq!(context.navigation_revision(), 7);
        assert_eq!(context.navigate("vmux://sessions/"), 0);
        assert_eq!(context.navigation_epoch(), 2);
        assert_eq!(context.navigation_revision(), 0);
        assert!(!context.bind_document(token(4, 7), 4, 7, "vmux://projects/"));
        let sessions_revision = context.minimum_navigation_revision.load(Ordering::Acquire);
        let sessions_token = token(4, sessions_revision);
        assert!(context.bind_document(
            sessions_token.clone(),
            4,
            sessions_revision,
            "vmux://sessions/"
        ));
        assert_eq!(context.reload(), 0);
        assert!(!context.accepts("vmux://projects/"));
        assert!(context.accepts("vmux://sessions/"));
        assert_eq!(context.commit("vmux://sessions/"), 0);
        assert_eq!(context.commit("vmux://tools/"), 0);
        let tools_revision = context.minimum_navigation_revision.load(Ordering::Acquire);
        assert!(!context.bind_document(
            token(3, tools_revision),
            3,
            tools_revision,
            "vmux://tools/"
        ));
        let tools_token = token(4, tools_revision);
        assert!(context.bind_document(tools_token.clone(), 4, tools_revision, "vmux://tools/"));
        assert!(context.matches_token(&tools_token));
        assert!(!context.matches_token(&sessions_token));
        assert!(!context.matches_token(&token(4, 0)));
    }

    #[test]
    fn superseded_document_cannot_bind_after_a_newer_navigation() {
        let context = BinIpcContext::new(4, "vmux://a/", false);
        assert!(context.bind_document(token(4, 7), 4, 7, "vmux://a/"));
        context.navigate("vmux://b/");
        context.navigate("vmux://c/");
        let revision = context.minimum_navigation_revision.load(Ordering::Acquire);

        assert!(!context.bind_document(token(4, revision), 4, revision, "vmux://b/"));
        assert!(context.bind_document(token(4, revision), 4, revision, "vmux://c/"));
    }

    #[test]
    fn document_that_binds_before_self_navigation_commit_is_accepted_on_commit() {
        let context = BinIpcContext::new(4, "vmux://a/", false);
        assert!(context.bind_document(token(4, 7), 4, 7, "vmux://a/"));
        let next = 8;
        let next_token = token(4, next);

        assert!(!context.bind_document(next_token.clone(), 4, next, "vmux://b/"));
        assert_eq!(context.commit("vmux://b/"), next);
        assert!(context.matches_token(&next_token));
    }

    #[test]
    fn blocked_context_keeps_an_event_epoch_for_page_ready() {
        let context = BinIpcContext::new(4, "vmux://a/", false);
        let before = context.ipc_epoch();

        context.set_blocked(true);

        assert!(context.ipc_epoch() > before);
    }

    #[test]
    fn initially_blocked_context_does_not_invalidate_its_first_document() {
        let context = BinIpcContext::new(4, "vmux://a/", true);
        let initial = context.ipc_epoch();

        context.set_blocked(true);
        assert_eq!(context.ipc_epoch(), initial);

        context.set_blocked(false);
        assert!(context.ipc_epoch() > initial);
    }

    #[test]
    fn an_event_keeps_the_epoch_from_when_the_renderer_created_it() {
        let context = BinIpcContext::new(4, "vmux://a/", false);
        let generated = context.ipc_epoch();

        context.set_blocked(true);
        context.set_blocked(false);

        assert!(!context.accepts_epoch(generated));
        assert!(context.accepts_epoch(context.ipc_epoch()));
    }

    #[test]
    fn host_authority_is_bound_to_the_current_document_and_epoch() {
        let context = BinIpcContext::new(4, "vmux://a/", false);
        let document = token(4, 7);
        assert!(context.bind_document(document.clone(), 4, 7, "vmux://a/"));
        let epoch = context.ipc_epoch();

        assert_eq!(context.authority_token(4, 7, epoch), Some(document));
        assert!(context.authority_token(4, 6, epoch).is_none());

        context.set_blocked(true);

        assert!(context.authority_token(4, 7, epoch).is_none());
    }
}
