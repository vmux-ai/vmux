#[cfg(target_os = "macos")]
use super::keepalive::{IoSurfaceKeepAlive, RealIoSurfaceOps};
use bevy::prelude::*;
use cef::osr_texture_import::SharedTextureHandle;
use cef::rc::{Rc, RcImpl};
use cef::*;
use cef_dll_sys::cef_paint_element_type_t;
use smallvec::SmallVec;
use std::any::Any;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_int;
use std::sync::{Arc, Mutex};

/// Inline dirty-rectangle storage for CEF paints.
pub type WebviewDirtyRects = SmallVec<[WebviewDirtyRect; 8]>;
/// Inline pixel-patch storage for CPU CEF paints.
pub type WebviewPaintPatches = SmallVec<[WebviewPaintPatch; 4]>;

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
    /// Packed BGRA pixel patches for the changed regions.
    pub patches: Arc<WebviewPaintPatches>,
    /// Changed regions in pixels with an upper-left origin.
    /// Empty means treat the whole frame as dirty (full upload).
    pub dirty: WebviewDirtyRects,
}

/// Packed BGRA pixels for one changed surface region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebviewPaintPatch {
    /// Destination region in the webview surface.
    pub rect: WebviewDirtyRect,
    /// Tightly packed rows with `rect.width * 4` bytes per row.
    pub buffer: Arc<Vec<u8>>,
}

/// A changed sub-region of a webview paint, in pixels with an upper-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebviewDirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn webview_dirty_rects(rects: Option<&[cef::Rect]>, width: u32, height: u32) -> WebviewDirtyRects {
    let Some(rects) = rects else {
        return WebviewDirtyRects::new();
    };
    let surface_w = width as i32;
    let surface_h = height as i32;
    optimize_webview_dirty_rects(
        rects.iter().filter_map(|r| {
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
        }),
        width,
        height,
    )
}

fn dirty_rect_union(left: WebviewDirtyRect, right: WebviewDirtyRect) -> Option<WebviewDirtyRect> {
    let left_right = left.x.saturating_add(left.width);
    let left_bottom = left.y.saturating_add(left.height);
    let right_right = right.x.saturating_add(right.width);
    let right_bottom = right.y.saturating_add(right.height);
    if left_right < right.x
        || right_right < left.x
        || left_bottom < right.y
        || right_bottom < left.y
    {
        return None;
    }
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    Some(WebviewDirtyRect {
        x,
        y,
        width: left_right.max(right_right).saturating_sub(x),
        height: left_bottom.max(right_bottom).saturating_sub(y),
    })
}

/// Clamp and merge dirty rectangles, using an empty result for a full-frame update.
pub fn optimize_webview_dirty_rects(
    rects: impl IntoIterator<Item = WebviewDirtyRect>,
    width: u32,
    height: u32,
) -> WebviewDirtyRects {
    let mut merged = WebviewDirtyRects::new();
    for rect in rects {
        let right = rect.x.saturating_add(rect.width).min(width);
        let bottom = rect.y.saturating_add(rect.height).min(height);
        if rect.x >= width || rect.y >= height || right <= rect.x || bottom <= rect.y {
            continue;
        }
        let mut rect = WebviewDirtyRect {
            x: rect.x,
            y: rect.y,
            width: right - rect.x,
            height: bottom - rect.y,
        };
        let mut index = 0;
        while index < merged.len() {
            if let Some(union) = dirty_rect_union(merged[index], rect) {
                rect = union;
                merged.swap_remove(index);
                index = 0;
            } else {
                index += 1;
            }
        }
        merged.push(rect);
    }
    let area = merged.iter().fold(0_u64, |total, rect| {
        total.saturating_add(rect.width as u64 * rect.height as u64)
    });
    let full_area = width as u64 * height as u64;
    if merged.len() > 16 || area.saturating_mul(2) >= full_area {
        merged.clear();
    }
    merged
}

/// Combine damage from dropped frames, using an empty result for a full-frame update.
pub fn coalesce_webview_dirty_rects(
    width: u32,
    height: u32,
    previous: &[WebviewDirtyRect],
    latest: &[WebviewDirtyRect],
    same_surface: bool,
) -> WebviewDirtyRects {
    if !same_surface || previous.is_empty() || latest.is_empty() {
        return WebviewDirtyRects::new();
    }
    optimize_webview_dirty_rects(previous.iter().chain(latest).copied(), width, height)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderPaintElementType {
    /// The main frame of the browser.
    View,
    /// The popup frame of the browser.
    Popup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PaintFrameKey {
    webview: Entity,
    ty: RenderPaintElementType,
}

#[derive(Default)]
struct TextureMailboxState {
    pending: HashMap<PaintFrameKey, PendingTexturePaint>,
    sizes: HashMap<PaintFrameKey, (u32, u32)>,
    force_full: HashSet<PaintFrameKey>,
    next_generation: u64,
    next_sequence: u64,
}

struct PendingTexturePaint {
    generation: u64,
    sequence: u64,
    width: u32,
    height: u32,
    dirty: WebviewDirtyRects,
    message: Option<RenderTextureMessage>,
}

/// Latest CPU paint per webview and paint element.
#[derive(Clone, Default)]
pub struct TextureMailbox(Arc<Mutex<TextureMailboxState>>);

pub type TextureSender = TextureMailbox;
pub type TextureReceiver = TextureMailbox;

impl TextureMailbox {
    /// Create producer and consumer handles for one mailbox.
    pub fn channel() -> (TextureSender, TextureReceiver) {
        let mailbox = Self::default();
        (mailbox.clone(), mailbox)
    }

    fn publish_paint(
        &self,
        webview: Entity,
        ty: RenderPaintElementType,
        width: u32,
        height: u32,
        dirty: WebviewDirtyRects,
        buffer: *const u8,
    ) -> bool {
        let key = PaintFrameKey { webview, ty };
        let (generation, sequence, dirty) = {
            let mut state = self
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let same_surface = state.sizes.get(&key) == Some(&(width, height));
            state.sizes.insert(key, (width, height));
            let dirty = if state.force_full.remove(&key) {
                WebviewDirtyRects::new()
            } else if let Some(previous) = state.pending.get(&key) {
                coalesce_webview_dirty_rects(
                    width,
                    height,
                    &previous.dirty,
                    &dirty,
                    previous.width == width && previous.height == height,
                )
            } else if same_surface {
                dirty
            } else {
                WebviewDirtyRects::new()
            };
            state.next_generation = state.next_generation.wrapping_add(1);
            state.next_sequence = state.next_sequence.wrapping_add(1);
            let generation = state.next_generation;
            let sequence = state.next_sequence;
            state.pending.insert(
                key,
                PendingTexturePaint {
                    generation,
                    sequence,
                    width,
                    height,
                    dirty: dirty.clone(),
                    message: None,
                },
            );
            (generation, sequence, dirty)
        };
        let patches = Arc::new(copy_paint_patches(buffer, width, height, &dirty));
        let message = RenderTextureMessage {
            webview,
            ty,
            width,
            height,
            patches,
            dirty,
        };
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(pending) = state
            .pending
            .get_mut(&key)
            .filter(|pending| pending.generation == generation && pending.sequence == sequence)
        else {
            return false;
        };
        pending.message = Some(message);
        true
    }

    /// Drain all latest pending paints.
    pub fn drain(&self) -> Vec<RenderTextureMessage> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut ready = Vec::new();
        state.pending.retain(|_, pending| {
            let Some(message) = pending.message.take() else {
                return true;
            };
            ready.push((pending.sequence, message));
            false
        });
        ready.sort_unstable_by_key(|(sequence, _)| *sequence);
        ready.into_iter().map(|(_, message)| message).collect()
    }

    /// Force the next CPU paint for one surface to carry the complete frame.
    pub fn request_full(&self, webview: Entity, ty: RenderPaintElementType) {
        let key = PaintFrameKey { webview, ty };
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending.remove(&key);
        state.force_full.insert(key);
    }

    /// Remove pending state for a closed webview.
    pub fn discard(&self, webview: Entity) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending.retain(|key, _| key.webview != webview);
        state.sizes.retain(|key, _| key.webview != webview);
        state.force_full.retain(|key| key.webview != webview);
    }
}

fn copy_paint_patches(
    buffer: *const u8,
    width: u32,
    height: u32,
    dirty: &[WebviewDirtyRect],
) -> WebviewPaintPatches {
    let full = WebviewDirtyRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let rects = if dirty.is_empty() {
        std::slice::from_ref(&full)
    } else {
        dirty
    };
    let source_stride = width as usize * 4;
    rects
        .iter()
        .filter(|rect| rect.width > 0 && rect.height > 0)
        .map(|rect| {
            let row_bytes = rect.width as usize * 4;
            let mut bytes = vec![0_u8; row_bytes * rect.height as usize];
            for row in 0..rect.height as usize {
                let source_offset = (rect.y as usize + row) * source_stride + rect.x as usize * 4;
                let destination_offset = row * row_bytes;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        buffer.add(source_offset),
                        bytes.as_mut_ptr().add(destination_offset),
                        row_bytes,
                    );
                }
            }
            WebviewPaintPatch {
                rect: *rect,
                buffer: Arc::new(bytes),
            }
        })
        .collect()
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
    pub dirty: WebviewDirtyRects,
}

#[derive(Default)]
struct AcceleratedMailboxState {
    pending: HashMap<PaintFrameKey, (u64, AcceleratedFrame)>,
    next_sequence: u64,
}

/// Latest accelerated paint per webview and paint element.
#[derive(Clone, Default)]
pub struct AcceleratedMailbox(Arc<Mutex<AcceleratedMailboxState>>);

pub type AcceleratedSender = AcceleratedMailbox;
pub type AcceleratedReceiver = AcceleratedMailbox;

impl AcceleratedMailbox {
    /// Create producer and consumer handles for one mailbox.
    pub fn channel() -> (AcceleratedSender, AcceleratedReceiver) {
        let mailbox = Self::default();
        (mailbox.clone(), mailbox)
    }

    /// Replace the pending frame and return whether the mailbox became non-empty for its key.
    pub fn publish(&self, mut frame: AcceleratedFrame) -> bool {
        let key = PaintFrameKey {
            webview: frame.webview,
            ty: frame.ty,
        };
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, previous)) = state.pending.get(&key) {
            frame.dirty = coalesce_webview_dirty_rects(
                frame.width,
                frame.height,
                &previous.dirty,
                &frame.dirty,
                previous.width == frame.width
                    && previous.height == frame.height
                    && previous.format == frame.format,
            );
        }
        state.next_sequence = state.next_sequence.wrapping_add(1);
        let sequence = state.next_sequence;
        state.pending.insert(key, (sequence, frame)).is_none()
    }

    /// Drain all latest pending frames.
    pub fn drain(&self) -> Vec<AcceleratedFrame> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut frames = state
            .pending
            .drain()
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        frames.sort_unstable_by_key(|(sequence, _)| *sequence);
        frames.into_iter().map(|(_, frame)| frame).collect()
    }

    /// Remove pending state for a closed webview.
    pub fn discard(&self, webview: Entity) {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending
            .retain(|key, _| key.webview != webview);
    }
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
        let width = width as u32;
        let height = height as u32;
        let dirty = webview_dirty_rects(dirty_rects, width, height);
        if self
            .texture_sender
            .publish_paint(self.webview, ty, width, height, dirty, buffer)
            && let Some(wake) = self.texture_wake.as_ref()
        {
            wake();
        }
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
                if self.accel_sender.publish(frame)
                    && let Some(wake) = self.texture_wake.as_ref()
                {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_mailbox_keeps_latest_frame() {
        let (tx, rx) = TextureMailbox::channel();
        let first = vec![1_u8; 4 * 4 * 4];
        let latest = vec![2_u8; 4 * 4 * 4];
        assert!(tx.publish_paint(
            Entity::from_bits(1),
            RenderPaintElementType::View,
            4,
            4,
            WebviewDirtyRects::new(),
            first.as_ptr(),
        ));
        assert!(tx.publish_paint(
            Entity::from_bits(1),
            RenderPaintElementType::View,
            4,
            4,
            smallvec::smallvec![WebviewDirtyRect {
                x: 1,
                y: 1,
                width: 1,
                height: 1,
            }],
            latest.as_ptr(),
        ));

        let frames = rx.drain();
        assert_eq!(frames.len(), 1);
        assert!(frames[0].dirty.is_empty());
        assert_eq!(frames[0].patches[0].buffer.as_slice(), latest.as_slice());
    }

    #[test]
    fn texture_mailbox_copies_only_dirty_pixels_after_initial_frame() {
        let (tx, rx) = TextureMailbox::channel();
        let first = vec![1_u8; 4 * 4 * 4];
        tx.publish_paint(
            Entity::from_bits(1),
            RenderPaintElementType::View,
            4,
            4,
            WebviewDirtyRects::new(),
            first.as_ptr(),
        );
        rx.drain();
        let latest = vec![2_u8; 4 * 4 * 4];
        tx.publish_paint(
            Entity::from_bits(1),
            RenderPaintElementType::View,
            4,
            4,
            smallvec::smallvec![WebviewDirtyRect {
                x: 1,
                y: 1,
                width: 2,
                height: 1,
            }],
            latest.as_ptr(),
        );

        let frames = rx.drain();
        assert_eq!(frames[0].patches.len(), 1);
        assert_eq!(frames[0].patches[0].buffer.len(), 8);
        assert!(frames[0].patches[0].buffer.iter().all(|byte| *byte == 2));
    }

    #[test]
    fn requested_full_paint_repairs_consumer_reinitialization() {
        let (tx, rx) = TextureMailbox::channel();
        let first = vec![1_u8; 4 * 4 * 4];
        tx.publish_paint(
            Entity::from_bits(1),
            RenderPaintElementType::View,
            4,
            4,
            WebviewDirtyRects::new(),
            first.as_ptr(),
        );
        rx.drain();
        tx.request_full(Entity::from_bits(1), RenderPaintElementType::View);

        let latest = vec![2_u8; 4 * 4 * 4];
        tx.publish_paint(
            Entity::from_bits(1),
            RenderPaintElementType::View,
            4,
            4,
            smallvec::smallvec![WebviewDirtyRect {
                x: 1,
                y: 1,
                width: 1,
                height: 1,
            }],
            latest.as_ptr(),
        );

        let frames = rx.drain();
        assert!(frames[0].dirty.is_empty());
        assert_eq!(frames[0].patches[0].buffer.len(), latest.len());
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
        assert!(callback.contains("self.accel_sender.publish(frame)"));
        assert!(callback.contains("wake()"));
    }

    #[test]
    fn dirty_rects_are_clamped_to_surface_bounds() {
        let rects = [
            cef::Rect {
                x: -5,
                y: 0,
                width: 10,
                height: 10,
            },
            cef::Rect {
                x: 90,
                y: 90,
                width: 20,
                height: 20,
            },
        ];
        let dirty = webview_dirty_rects(Some(&rects), 100, 100);
        assert_eq!(
            dirty.as_slice(),
            &[
                WebviewDirtyRect {
                    x: 0,
                    y: 0,
                    width: 5,
                    height: 10,
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

    #[test]
    fn overlapping_dirty_rects_are_merged() {
        let dirty = optimize_webview_dirty_rects(
            [
                WebviewDirtyRect {
                    x: 10,
                    y: 10,
                    width: 10,
                    height: 10,
                },
                WebviewDirtyRect {
                    x: 15,
                    y: 15,
                    width: 10,
                    height: 10,
                },
            ],
            100,
            100,
        );
        assert_eq!(
            dirty.as_slice(),
            &[WebviewDirtyRect {
                x: 10,
                y: 10,
                width: 15,
                height: 15,
            }]
        );
    }
}
