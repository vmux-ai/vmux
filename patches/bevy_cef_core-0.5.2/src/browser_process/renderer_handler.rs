use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use cef::rc::{Rc, RcImpl};
use cef::*;
use cef_dll_sys::cef_paint_element_type_t;
use std::cell::Cell;
use std::os::raw::c_int;
use std::sync::Arc;

pub type TextureSender = Sender<RenderTextureMessage>;

pub type TextureReceiver = Receiver<RenderTextureMessage>;

pub type TextureWake = Arc<dyn Fn() + Send + Sync + 'static>;

/// The texture structure passed from [`CefRenderHandler::OnPaint`](https://cef-builds.spotifycdn.com/docs/106.1/classCefRenderHandler.html#a6547d5c9dd472e6b84706dc81d3f1741).
#[derive(Debug, Clone, PartialEq, Message)]
pub struct RenderTextureMessage {
    /// The entity of target rendering webview.
    pub webview: Entity,
    /// The type of the paint element.
    pub ty: RenderPaintElementType,
    /// The width of the texture.
    pub width: u32,
    /// The height of the texture.
    pub height: u32,
    /// This buffer will be `width` *`height` * 4 bytes in size and represents a BGRA image with an upper-left origin.
    ///
    /// Wrapped in `Arc` so the message survives Bevy's per-reader clone (3 consumers — mesh,
    /// extend-material, sprite) without copying the full BGRA buffer each time.
    pub buffer: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderPaintElementType {
    /// The main frame of the browser.
    View,
    /// The popup frame of the browser.
    Popup,
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
    texture_sender: TextureSender,
    texture_wake: Option<TextureWake>,
    size: SharedViewSize,
    device_scale: SharedDeviceScaleFactor,
}

impl RenderHandlerBuilder {
    pub fn build(
        webview: Entity,
        texture_sender: TextureSender,
        texture_wake: Option<TextureWake>,
        size: SharedViewSize,
        device_scale: SharedDeviceScaleFactor,
    ) -> RenderHandler {
        RenderHandler::new(Self {
            object: std::ptr::null_mut(),
            webview,
            texture_sender,
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
            texture_sender: self.texture_sender.clone(),
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

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn on_paint(
        &self,
        _browser: Option<&mut Browser>,
        type_: PaintElementType,
        _dirty_rects: Option<&[cef::Rect]>,
        buffer: *const u8,
        width: c_int,
        height: c_int,
    ) {
        let ty = match type_.as_ref() {
            cef_paint_element_type_t::PET_POPUP => RenderPaintElementType::Popup,
            _ => RenderPaintElementType::View,
        };
        let texture = RenderTextureMessage {
            webview: self.webview,
            ty,
            width: width as u32,
            height: height as u32,
            buffer: Arc::new(unsafe {
                std::slice::from_raw_parts(buffer, (width * height * 4) as usize).to_vec()
            }),
        };
        send_render_texture(&self.texture_sender, self.texture_wake.as_ref(), texture);
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_render_handler_t {
        self.object.cast()
    }
}

fn send_render_texture(
    texture_sender: &TextureSender,
    texture_wake: Option<&TextureWake>,
    texture: RenderTextureMessage,
) {
    let _ = texture_sender.send_blocking(texture);
    if let Some(wake) = texture_wake {
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn render_texture_delivery_wakes_consumer() {
        let (tx, rx) = async_channel::unbounded();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_for_callback = Arc::clone(&wakes);
        let wake: TextureWake = Arc::new(move || {
            wakes_for_callback.fetch_add(1, Ordering::Relaxed);
        });

        send_render_texture(
            &tx,
            Some(&wake),
            RenderTextureMessage {
                webview: Entity::from_bits(1),
                ty: RenderPaintElementType::View,
                width: 1,
                height: 1,
                buffer: Arc::new(vec![0, 0, 0, 0]),
            },
        );

        assert!(rx.try_recv().is_ok());
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }
}
