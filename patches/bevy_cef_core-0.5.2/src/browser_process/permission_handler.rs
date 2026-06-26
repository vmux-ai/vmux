use async_channel::Sender;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, CefString, Frame, ImplMediaAccessCallback, ImplPermissionHandler,
    ImplPermissionPromptCallback, MediaAccessCallback, PermissionHandler, PermissionPromptCallback,
    PermissionRequestResult, WrapPermissionHandler, sys,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU64, Ordering};

/// A page asked for camera/microphone (getUserMedia) or screen (getDisplayMedia) access. The
/// requested device categories are translated here so the Bevy side never has to know CEF's two
/// distinct permission enums.
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

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

enum PendingCallback {
    Prompt(PermissionPromptCallback),
    Media {
        callback: MediaAccessCallback,
        granted_mask: u32,
    },
}

// CEF permission callbacks are !Send and must continue on the thread that received them. Under
// `external_message_pump` that is the main thread, which is also where Bevy systems run, so the
// callback never crosses threads: the handler stashes it here and a Bevy system pops it.
thread_local! {
    static PENDING: RefCell<HashMap<u64, PendingCallback>> = RefCell::new(HashMap::new());
}

/// Continue a pending media request with the user's decision. Must run on the CEF UI thread
/// (== Bevy main thread). Unknown ids are ignored.
pub fn resolve_media_permission(request_id: u64, allow: bool) {
    let pending = PENDING.with(|pending| pending.borrow_mut().remove(&request_id));
    match pending {
        Some(PendingCallback::Prompt(callback)) => {
            callback.cont(if allow {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
        }
        Some(PendingCallback::Media {
            callback,
            granted_mask,
        }) => {
            callback.cont(if allow { granted_mask } else { 0 });
        }
        None => {}
    }
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
        PENDING.with(|pending| {
            pending
                .borrow_mut()
                .insert(request_id, PendingCallback::Prompt(callback.clone()));
        });
        if !self.send(MediaPermissionRequest {
            webview: self.webview,
            request_id,
            origin,
            wants_camera,
            wants_microphone,
            wants_screen: false,
        }) {
            PENDING.with(|pending| {
                pending.borrow_mut().remove(&request_id);
            });
            callback.cont(PermissionRequestResult::DENY);
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
        PENDING.with(|pending| {
            pending.borrow_mut().insert(
                request_id,
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
            PENDING.with(|pending| {
                pending.borrow_mut().remove(&request_id);
            });
            callback.cont(0);
        }
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::cef_permission_handler_t {
        self.object.cast()
    }
}
