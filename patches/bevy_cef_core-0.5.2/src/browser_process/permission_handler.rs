use async_channel::Sender;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, CefString, Frame, ImplMediaAccessCallback, ImplPermissionHandler,
    ImplPermissionPromptCallback, MediaAccessCallback, PermissionHandler, PermissionPromptCallback,
    PermissionRequestResult, WrapPermissionHandler, sys,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct MediaPermissionRequest {
    pub webview: Entity,
    pub request_id: u64,
    pub origin: String,
    pub wants_camera: bool,
    pub wants_microphone: bool,
    pub wants_screen: bool,
}

pub type MediaPermissionSenderInner = Sender<MediaPermissionRequest>;

const PROMPT_CAMERA_PAN_TILT_ZOOM: u32 =
    sys::cef_permission_request_types_t::CEF_PERMISSION_TYPE_CAMERA_PAN_TILT_ZOOM as u32;
const PROMPT_CAMERA_STREAM: u32 =
    sys::cef_permission_request_types_t::CEF_PERMISSION_TYPE_CAMERA_STREAM as u32;
const PROMPT_MIC_STREAM: u32 =
    sys::cef_permission_request_types_t::CEF_PERMISSION_TYPE_MIC_STREAM as u32;
const MEDIA_DEVICE_AUDIO: u32 =
    sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DEVICE_AUDIO_CAPTURE as u32;
const MEDIA_DEVICE_VIDEO: u32 =
    sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DEVICE_VIDEO_CAPTURE as u32;
const MEDIA_DESKTOP_AUDIO: u32 =
    sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DESKTOP_AUDIO_CAPTURE as u32;
const MEDIA_DESKTOP_VIDEO: u32 =
    sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DESKTOP_VIDEO_CAPTURE as u32;
const HANDLED_PROMPT_MASK: u32 =
    PROMPT_CAMERA_PAN_TILT_ZOOM | PROMPT_CAMERA_STREAM | PROMPT_MIC_STREAM;
const HANDLED_MEDIA_MASK: u32 =
    MEDIA_DEVICE_AUDIO | MEDIA_DEVICE_VIDEO | MEDIA_DESKTOP_AUDIO | MEDIA_DESKTOP_VIDEO;
const MAX_PENDING_PERMISSIONS: usize = 64;
const PENDING_PERMISSION_TTL: Duration = Duration::from_secs(120);

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

enum PendingCallback {
    Prompt(PermissionPromptCallback),
    Media {
        callback: MediaAccessCallback,
        granted_mask: u32,
    },
    #[cfg(test)]
    Probe(std::sync::Arc<std::sync::atomic::AtomicI8>),
}

impl PendingCallback {
    fn resolve(self, allow: bool) {
        match self {
            Self::Prompt(callback) => {
                callback.cont(if allow {
                    PermissionRequestResult::ACCEPT
                } else {
                    PermissionRequestResult::DENY
                });
            }
            Self::Media {
                callback,
                granted_mask,
            } => {
                callback.cont(if allow { granted_mask } else { 0 });
            }
            #[cfg(test)]
            Self::Probe(slot) => {
                slot.store(if allow { 1 } else { -1 }, Ordering::Release);
            }
        }
    }
}

struct PendingPermissionEntry {
    webview: Entity,
    created_at: Instant,
    callback: PendingCallback,
}

#[derive(Default)]
struct PendingPermissionRegistry {
    entries: HashMap<u64, PendingPermissionEntry>,
    order: VecDeque<u64>,
}

impl PendingPermissionRegistry {
    fn with_current<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        thread_local! {
            static PENDING: RefCell<PendingPermissionRegistry> =
                RefCell::new(PendingPermissionRegistry::default());
        }
        PENDING.with(|pending| f(&mut pending.borrow_mut()))
    }

    fn insert(
        &mut self,
        request_id: u64,
        webview: Entity,
        now: Instant,
        callback: PendingCallback,
    ) {
        self.expire(now);
        while self.entries.len() >= MAX_PENDING_PERMISSIONS {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                evicted.callback.resolve(false);
            }
        }
        self.entries.insert(
            request_id,
            PendingPermissionEntry {
                webview,
                created_at: now,
                callback,
            },
        );
        self.order.push_back(request_id);
    }

    fn resolve(&mut self, request_id: u64, allow: bool) -> bool {
        self.resolve_at(request_id, allow, Instant::now())
    }

    fn resolve_at(&mut self, request_id: u64, allow: bool, now: Instant) -> bool {
        self.expire(now);
        let Some(entry) = self.entries.remove(&request_id) else {
            return false;
        };
        self.order.retain(|id| *id != request_id);
        entry.callback.resolve(allow);
        true
    }

    fn revoke_webview(&mut self, webview: Entity) -> usize {
        let mut removed = Vec::new();
        for (&request_id, entry) in &self.entries {
            if entry.webview == webview {
                removed.push(request_id);
            }
        }
        for request_id in &removed {
            if let Some(entry) = self.entries.remove(request_id) {
                entry.callback.resolve(false);
            }
        }
        if !removed.is_empty() {
            self.order.retain(|id| self.entries.contains_key(id));
        }
        removed.len()
    }

    fn revoke_all(&mut self) {
        for (_, entry) in self.entries.drain() {
            entry.callback.resolve(false);
        }
        self.order.clear();
    }

    fn expire(&mut self, now: Instant) {
        let mut expired = Vec::new();
        for (&request_id, entry) in &self.entries {
            if now.saturating_duration_since(entry.created_at) >= PENDING_PERMISSION_TTL {
                expired.push(request_id);
            }
        }
        for request_id in &expired {
            if let Some(entry) = self.entries.remove(request_id) {
                entry.callback.resolve(false);
            }
        }
        if !expired.is_empty() {
            self.order.retain(|id| self.entries.contains_key(id));
        }
    }
}

pub fn resolve_media_permission(request_id: u64, allow: bool) {
    PendingPermissionRegistry::with_current(|registry| {
        registry.resolve(request_id, allow);
    });
}

pub fn revoke_webview_permissions(webview: Entity) {
    PendingPermissionRegistry::with_current(|registry| {
        registry.revoke_webview(webview);
    });
}

pub fn revoke_all_permissions() {
    PendingPermissionRegistry::with_current(|registry| {
        registry.revoke_all();
    });
}

pub fn expire_pending_permissions() {
    PendingPermissionRegistry::with_current(|registry| {
        registry.expire(Instant::now());
    });
}

pub struct PermissionHandlerBuilder {
    object: *mut RcImpl<sys::cef_permission_handler_t, Self>,
    webview: Entity,
    sender: MediaPermissionSenderInner,
}

impl PermissionHandlerBuilder {
    pub fn build(webview: Entity, sender: MediaPermissionSenderInner) -> PermissionHandler {
        PermissionHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            sender,
        })
    }

    fn send(&self, request: MediaPermissionRequest) -> bool {
        self.sender.send_blocking(request).is_ok()
    }
}

impl Rc for PermissionHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for PermissionHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            webview: self.webview,
            sender: self.sender.clone(),
        }
    }
}

impl WrapPermissionHandler for PermissionHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::cef_permission_handler_t, Self>) {
        self.object = object;
    }
}

impl ImplPermissionHandler for PermissionHandlerBuilder {
    fn on_show_permission_prompt(
        &self,
        _browser: Option<&mut Browser>,
        _prompt_id: u64,
        requesting_origin: Option<&CefString>,
        requested_permissions: u32,
        callback: Option<&mut PermissionPromptCallback>,
    ) -> c_int {
        let Some(callback) = callback else {
            return 0;
        };
        if requested_permissions == 0 || requested_permissions & !HANDLED_PROMPT_MASK != 0 {
            return 0;
        }
        let wants_camera =
            requested_permissions & (PROMPT_CAMERA_STREAM | PROMPT_CAMERA_PAN_TILT_ZOOM) != 0;
        let wants_microphone = requested_permissions & PROMPT_MIC_STREAM != 0;
        let Some(origin) = requesting_origin
            .map(|origin| origin.to_string())
            .filter(|origin| !origin.is_empty())
        else {
            callback.cont(PermissionRequestResult::DENY);
            return 1;
        };
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        PendingPermissionRegistry::with_current(|registry| {
            registry.insert(
                request_id,
                self.webview,
                Instant::now(),
                PendingCallback::Prompt(callback.clone()),
            );
        });
        if !self.send(MediaPermissionRequest {
            webview: self.webview,
            request_id,
            origin,
            wants_camera,
            wants_microphone,
            wants_screen: false,
        }) {
            resolve_media_permission(request_id, false);
        }
        1
    }

    fn on_request_media_access_permission(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        requesting_origin: Option<&CefString>,
        requested_permissions: u32,
        callback: Option<&mut MediaAccessCallback>,
    ) -> c_int {
        let Some(callback) = callback else {
            return 0;
        };
        let granted_mask = requested_permissions & HANDLED_MEDIA_MASK;
        if granted_mask == 0 {
            return 0;
        }
        let wants_camera = granted_mask & MEDIA_DEVICE_VIDEO != 0;
        let wants_microphone = granted_mask & MEDIA_DEVICE_AUDIO != 0;
        let wants_screen = granted_mask & (MEDIA_DESKTOP_AUDIO | MEDIA_DESKTOP_VIDEO) != 0;
        let Some(origin) = requesting_origin
            .map(|origin| origin.to_string())
            .filter(|origin| !origin.is_empty())
        else {
            callback.cont(0);
            return 1;
        };
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        PendingPermissionRegistry::with_current(|registry| {
            registry.insert(
                request_id,
                self.webview,
                Instant::now(),
                PendingCallback::Media {
                    callback: callback.clone(),
                    granted_mask,
                },
            );
        });
        if !self.send(MediaPermissionRequest {
            webview: self.webview,
            request_id,
            origin,
            wants_camera,
            wants_microphone,
            wants_screen,
        }) {
            resolve_media_permission(request_id, false);
        }
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::cef_permission_handler_t {
        self.object.cast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicI8;

    #[test]
    fn revoking_a_webview_denies_only_its_pending_permissions() {
        let mut registry = PendingPermissionRegistry::default();
        let owner_a = Entity::from_bits(1);
        let owner_b = Entity::from_bits(2);
        let probe_a = Arc::new(AtomicI8::new(0));
        let probe_b = Arc::new(AtomicI8::new(0));
        let now = Instant::now();

        registry.insert(1, owner_a, now, PendingCallback::Probe(probe_a.clone()));
        registry.insert(2, owner_b, now, PendingCallback::Probe(probe_b.clone()));

        assert_eq!(registry.revoke_webview(owner_a), 1);
        assert_eq!(probe_a.load(Ordering::Acquire), -1);
        assert_eq!(probe_b.load(Ordering::Acquire), 0);
        assert!(!registry.resolve(1, true));
        assert!(registry.resolve(2, true));
        assert_eq!(probe_b.load(Ordering::Acquire), 1);
    }

    #[test]
    fn expired_and_overflow_permissions_are_denied_and_evicted() {
        let mut registry = PendingPermissionRegistry::default();
        let owner = Entity::from_bits(9);
        let old_probe = Arc::new(AtomicI8::new(0));
        let now = Instant::now();
        let stale = now - PENDING_PERMISSION_TTL - Duration::from_secs(1);

        registry.insert(1, owner, stale, PendingCallback::Probe(old_probe.clone()));
        let fresh_probe = Arc::new(AtomicI8::new(0));
        registry.insert(2, owner, now, PendingCallback::Probe(fresh_probe.clone()));

        assert_eq!(old_probe.load(Ordering::Acquire), -1);
        assert_eq!(fresh_probe.load(Ordering::Acquire), 0);
        assert_eq!(registry.entries.len(), 1);

        let mut probes = vec![fresh_probe];
        for id in 3..=(MAX_PENDING_PERMISSIONS as u64 + 3) {
            let probe = Arc::new(AtomicI8::new(0));
            registry.insert(id, owner, now, PendingCallback::Probe(probe.clone()));
            probes.push(probe);
        }
        assert_eq!(registry.entries.len(), MAX_PENDING_PERMISSIONS);
        assert_eq!(probes[0].load(Ordering::Acquire), -1);
    }

    #[test]
    fn resolve_denies_expired_permission_without_new_insertion() {
        let mut registry = PendingPermissionRegistry::default();
        let owner = Entity::from_bits(12);
        let probe = Arc::new(AtomicI8::new(0));
        let created = Instant::now();

        registry.insert(1, owner, created, PendingCallback::Probe(probe.clone()));

        let after_ttl = created + PENDING_PERMISSION_TTL + Duration::from_secs(1);
        assert!(!registry.resolve_at(1, true, after_ttl));
        assert_eq!(probe.load(Ordering::Acquire), -1);
        assert!(registry.entries.is_empty());
    }
}
