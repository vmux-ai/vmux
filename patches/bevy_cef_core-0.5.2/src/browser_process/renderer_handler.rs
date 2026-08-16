use bevy::prelude::*;
use cef::rc::{Rc, RcImpl};
use cef::*;
use smallvec::SmallVec;
use std::cell::Cell;
use std::os::raw::c_int;
use std::sync::Arc;

/// Inline dirty-rectangle storage for CEF paints.
pub type WebviewDirtyRects = SmallVec<[WebviewDirtyRect; 8]>;

pub type TextureWake = Arc<dyn Fn() + Send + Sync + 'static>;

/// A changed sub-region of a webview paint, in pixels with an upper-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebviewDirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub type SharedViewSize = std::rc::Rc<Cell<Vec2>>;

/// Window / backing-store scale passed to CEF as [`ScreenInfo::device_scale_factor`].
pub type SharedDeviceScaleFactor = std::rc::Rc<Cell<f32>>;

/// ## Reference
///
/// - [`CefRenderHandler Class Reference`](https://cef-builds.spotifycdn.com/docs/106.1/classCefRenderHandler.html)
pub struct RenderHandlerBuilder {
    object: *mut RcImpl<sys::cef_render_handler_t, Self>,
    webview: Entity,
    texture_wake: Option<TextureWake>,
    size: SharedViewSize,
    device_scale: SharedDeviceScaleFactor,
}

impl RenderHandlerBuilder {
    pub fn build(
        webview: Entity,
        texture_wake: Option<TextureWake>,
        size: SharedViewSize,
        device_scale: SharedDeviceScaleFactor,
    ) -> RenderHandler {
        RenderHandler::new(Self {
            object: std::ptr::null_mut(),
            webview,
            texture_wake,
            size,
            device_scale,
        })
    }
}

impl Rc for RenderHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapRenderHandler for RenderHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_render_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for RenderHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            webview: self.webview,
            texture_wake: self.texture_wake.clone(),
            size: self.size.clone(),
            device_scale: self.device_scale.clone(),
        }
    }
}

impl ImplRenderHandler for RenderHandlerBuilder {
    fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut cef::Rect>) {
        if let Some(rect) = rect {
            let size = self.size.get();
            rect.width = size.x as _;
            rect.height = size.y as _;
        }
    }

    fn screen_info(
        &self,
        _browser: Option<&mut Browser>,
        screen_info: Option<&mut ScreenInfo>,
    ) -> c_int {
        let Some(si) = screen_info else {
            return 0;
        };
        let scale = self.device_scale.get();
        if !scale.is_finite() || scale <= 0.0 {
            return 0;
        }
        let mut out = ScreenInfo::default();
        out.device_scale_factor = scale;
        *si = out;
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_render_handler_t {
        self.object.cast()
    }
}
