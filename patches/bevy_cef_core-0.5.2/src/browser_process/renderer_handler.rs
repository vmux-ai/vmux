#[cfg(target_os = "macos")]
use super::keepalive::{IoSurfaceKeepAlive, RealIoSurfaceOps};
use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use cef::osr_texture_import::SharedTextureHandle;
use cef::rc::{Rc, RcImpl};
use cef::*;
use cef_dll_sys::cef_paint_element_type_t;
use std::any::Any;
use std::cell::Cell;
use std::os::raw::c_int;
use std::sync::Arc;

pub type TextureSender = Sender<RenderTextureMessage>;

pub type TextureReceiver = Receiver<RenderTextureMessage>;

pub type TextureWake = Arc<dyn Fn() + Send + Sync + 'static>;
pub type AcceleratedFramePresenter = Arc<dyn Fn(AcceleratedFrame) + Send + Sync + 'static>;

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
    /// Sub-regions of `buffer` that changed this paint, in pixels with an upper-left origin.
    /// Empty means treat the whole frame as dirty (full upload).
    pub dirty: Vec<WebviewDirtyRect>,
}

/// A changed sub-region of a webview paint, in pixels with an upper-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebviewDirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn webview_dirty_rects(
    rects: Option<&[cef::Rect]>,
    width: u32,
    height: u32,
) -> Vec<WebviewDirtyRect> {
    let Some(rects) = rects else {
        return Vec::new();
    };
    let surface_w = width as i32;
    let surface_h = height as i32;
    rects
        .iter()
        .filter_map(|r| {
            let left = r.x.max(0);
            let top = r.y.max(0);
            let right = r.x.saturating_add(r.width).min(surface_w);
            let bottom = r.y.saturating_add(r.height).min(surface_h);
            if right <= left || bottom <= top {
                return None;
            }
            Some(WebviewDirtyRect {
                x: left as u32,
                y: top as u32,
                width: (right - left) as u32,
                height: (bottom - top) as u32,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderPaintElementType {
    /// The main frame of the browser.
    View,
    /// The popup frame of the browser.
    Popup,
}

pub struct SendSharedTextureHandle(pub SharedTextureHandle);
unsafe impl Send for SendSharedTextureHandle {}
unsafe impl Sync for SendSharedTextureHandle {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratedPixelFormat {
    Rgba8,
    Bgra8,
}

pub struct AcceleratedFrame {
    pub webview: Entity,
    pub ty: RenderPaintElementType,
    pub width: u32,
    pub height: u32,
    pub format: AcceleratedPixelFormat,
    pub handle: SendSharedTextureHandle,
    /// Raw `IOSurfaceRef` (as `usize`) backing this frame, kept alive by `keepalive`. Lets a native
    /// overlay set it directly as a `CALayer`'s `contents` instead of importing into a GPU texture.
    pub io_surface: usize,
    pub keepalive: Arc<dyn Any + Send + Sync>,
    pub dirty: Vec<WebviewDirtyRect>,
}

pub type AcceleratedSender = Sender<AcceleratedFrame>;
pub type AcceleratedReceiver = Receiver<AcceleratedFrame>;

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
    accel_sender: AcceleratedSender,
    texture_wake: Option<TextureWake>,
    accelerated_presenter: Option<AcceleratedFramePresenter>,
    size: SharedViewSize,
    device_scale: SharedDeviceScaleFactor,
}

impl RenderHandlerBuilder {
    pub fn build(
        webview: Entity,
        texture_sender: TextureSender,
        accel_sender: AcceleratedSender,
        texture_wake: Option<TextureWake>,
        accelerated_presenter: Option<AcceleratedFramePresenter>,
        size: SharedViewSize,
        device_scale: SharedDeviceScaleFactor,
    ) -> RenderHandler {
        RenderHandler::new(Self {
            object: std::ptr::null_mut(),
            webview,
            texture_sender,
            accel_sender,
            texture_wake,
            accelerated_presenter,
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
            accel_sender: self.accel_sender.clone(),
            texture_wake: self.texture_wake.clone(),
            accelerated_presenter: self.accelerated_presenter.clone(),
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
        dirty_rects: Option<&[cef::Rect]>,
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
            dirty: webview_dirty_rects(dirty_rects, width as u32, height as u32),
        };
        send_render_texture(&self.texture_sender, self.texture_wake.as_ref(), texture);
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn on_accelerated_paint(
        &self,
        _browser: Option<&mut Browser>,
        type_: PaintElementType,
        dirty_rects: Option<&[cef::Rect]>,
        info: Option<&AcceleratedPaintInfo>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let Some(info) = info else {
                return;
            };
            let ty = match type_.as_ref() {
                cef_paint_element_type_t::PET_POPUP => RenderPaintElementType::Popup,
                _ => RenderPaintElementType::View,
            };
            let width = info.extra.coded_size.width as u32;
            let height = info.extra.coded_size.height as u32;
            let keepalive: Arc<dyn Any + Send + Sync> =
                Arc::new(IoSurfaceKeepAlive::<RealIoSurfaceOps>::retain(
                    info.shared_texture_io_surface,
                ));
            let io_surface = info.shared_texture_io_surface as usize;
            let frame = AcceleratedFrame {
                webview: self.webview,
                ty,
                width,
                height,
                format: if info.format == ColorType::RGBA_8888 {
                    AcceleratedPixelFormat::Rgba8
                } else {
                    AcceleratedPixelFormat::Bgra8
                },
                handle: SendSharedTextureHandle(SharedTextureHandle::new(info)),
                io_surface,
                keepalive,
                dirty: webview_dirty_rects(dirty_rects, width, height),
            };
            if let Some(presenter) = self.accelerated_presenter.as_ref() {
                presenter(frame);
            } else {
                let _ = self.accel_sender.send_blocking(frame);
                if let Some(wake) = self.texture_wake.as_ref() {
                    wake();
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (type_, dirty_rects, info);
        }
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
                dirty: Vec::new(),
            },
        );

        assert!(rx.try_recv().is_ok());
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn accelerated_native_presenter_bypasses_bevy_wake() {
        let source = include_str!("renderer_handler.rs");
        let callback = source
            .split("fn on_accelerated_paint")
            .nth(1)
            .and_then(|tail| tail.split("fn get_raw").next())
            .unwrap_or_default();

        assert!(callback.contains("presenter(frame)"));
        assert!(callback.contains("else {"));
        assert!(callback.contains("self.accel_sender.send_blocking(frame)"));
        assert!(callback.contains("wake()"));
    }

    #[test]
    fn dirty_rects_are_clamped_to_surface_bounds() {
        let rects = [
            cef::Rect {
                x: -5,
                y: 0,
                width: 100,
                height: 100,
            },
            cef::Rect {
                x: 90,
                y: 90,
                width: 100,
                height: 100,
            },
        ];
        let dirty = webview_dirty_rects(Some(&rects), 100, 100);
        assert_eq!(
            dirty,
            vec![
                WebviewDirtyRect {
                    x: 0,
                    y: 0,
                    width: 95,
                    height: 100,
                },
                WebviewDirtyRect {
                    x: 90,
                    y: 90,
                    width: 10,
                    height: 10,
                },
            ]
        );
    }

    #[test]
    fn missing_dirty_rects_mean_full_frame() {
        assert!(webview_dirty_rects(None, 100, 100).is_empty());
    }
}
