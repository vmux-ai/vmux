use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use vmux_agent::{ScreenshotRequest, ScreenshotResponse};
use vmux_setting::AppSettings;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const MAX_INLINE_EDGE: u32 = 1568;

pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PERMISSION_MSG: &str = "Screen Recording permission required - grant it in System Settings > \
Privacy & Security > Screen Recording, then call screenshot again.";

#[derive(Resource)]
pub(crate) struct ScreenshotBridge {
    tx: Sender<ScreenshotResponse>,
    rx: Receiver<ScreenshotResponse>,
}

impl Default for ScreenshotBridge {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

fn err_response(request_id: [u8; 16], message: impl Into<String>) -> ScreenshotResponse {
    ScreenshotResponse {
        request_id,
        result: Err(message.into()),
    }
}

fn resolve_crop(
    id: &str,
    node_q: &Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: &Query<&ChildOf>,
    img_w: u32,
    img_h: u32,
) -> Option<CropRect> {
    let (_, bits) = vmux_layout::protocol::parse_id(id).ok()?;
    let mut entity = Entity::from_bits(bits);
    for _ in 0..8 {
        if let Ok((computed, gt)) = node_q.get(entity) {
            let size = computed.size;
            let center = gt.transform_point2(Vec2::ZERO);
            return Some(crop_rect_from_node(
                center.x, center.y, size.x, size.y, img_w, img_h,
            ));
        }
        entity = child_of_q.get(entity).ok()?.get();
    }
    None
}

pub(crate) fn start_screenshots(
    _non_send: NonSendMarker,
    mut reader: MessageReader<ScreenshotRequest>,
    bridge: Res<ScreenshotBridge>,
    settings: Res<AppSettings>,
    window_q: Query<(Entity, &Window), With<PrimaryWindow>>,
    node_q: Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: Query<&ChildOf>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let base_dir = crate::recording::capture_output_dir(&settings);
    for req in reader.read() {
        let Ok((window_entity, window)) = window_q.single() else {
            let _ = bridge
                .tx
                .send(err_response(req.request_id, "no primary vmux window"));
            continue;
        };
        let img_w = window.resolution.physical_width();
        let img_h = window.resolution.physical_height();

        let crop = match &req.pane {
            Some(id) => match resolve_crop(id, &node_q, &child_of_q, img_w, img_h) {
                Some(rect) => Some(rect),
                None => {
                    let _ = bridge.tx.send(err_response(
                        req.request_id,
                        format!("pane not found: {id}"),
                    ));
                    continue;
                }
            },
            None => None,
        };

        let tx = bridge.tx.clone();
        let wake: Option<WakeFn> = proxy.as_ref().map(|p| {
            let proxy = (***p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as WakeFn
        });
        capture::capture(
            window_entity,
            img_w,
            img_h,
            crop,
            req.request_id,
            base_dir.clone(),
            tx,
            wake,
        );
    }
}

pub(crate) fn drain_screenshots(
    bridge: Res<ScreenshotBridge>,
    mut writer: MessageWriter<ScreenshotResponse>,
) {
    while let Ok(response) = bridge.rx.try_recv() {
        writer.write(response);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn downscale_dims(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long == 0 {
        return (1, 1);
    }
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let scale = max_edge as f64 / long as f64;
    (
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    )
}

pub(crate) fn crop_rect_from_node(
    center_x: f32,
    center_y: f32,
    size_x: f32,
    size_y: f32,
    img_w: u32,
    img_h: u32,
) -> CropRect {
    let left = (center_x - size_x * 0.5).round().max(0.0) as u32;
    let top = (center_y - size_y * 0.5).round().max(0.0) as u32;
    let left = left.min(img_w.saturating_sub(1));
    let top = top.min(img_h.saturating_sub(1));
    let w = (size_x.round().max(1.0) as u32).min(img_w - left);
    let h = (size_y.round().max(1.0) as u32).min(img_h - top);
    CropRect {
        x: left,
        y: top,
        w,
        h,
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn encode_downscaled_png(
    img: &image::RgbaImage,
    max_edge: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let (dw, dh) = downscale_dims(img.width(), img.height(), max_edge);
    let dynimg = image::DynamicImage::ImageRgba8(img.clone());
    let scaled = if (dw, dh) == (img.width(), img.height()) {
        dynimg
    } else {
        dynimg.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3)
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    scaled
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("png encode failed: {e}"))?;
    Ok((buf.into_inner(), dw, dh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_never_upscales() {
        assert_eq!(downscale_dims(800, 600, 1568), (800, 600));
        assert_eq!(downscale_dims(0, 0, 1568), (1, 1));
    }

    #[test]
    fn downscale_caps_long_edge() {
        assert_eq!(downscale_dims(3136, 1568, 1568), (1568, 784));
        assert_eq!(downscale_dims(1568, 3136, 1568), (784, 1568));
    }

    #[test]
    fn crop_rect_clamps_to_image() {
        let r = crop_rect_from_node(100.0, 100.0, 80.0, 60.0, 1000, 1000);
        assert_eq!(
            r,
            CropRect {
                x: 60,
                y: 70,
                w: 80,
                h: 60
            }
        );

        let r = crop_rect_from_node(990.0, 990.0, 40.0, 40.0, 1000, 1000);
        assert_eq!(
            r,
            CropRect {
                x: 970,
                y: 970,
                w: 30,
                h: 30
            }
        );
    }

    #[test]
    fn encode_downscaled_png_emits_png_header() {
        let img = image::RgbaImage::new(10, 10);
        let (png, w, h) = encode_downscaled_png(&img, 1568).unwrap();
        assert_eq!((w, h), (10, 10));
        assert_eq!(&png[..4], &[137, 80, 78, 71]);
    }
}

#[cfg(target_os = "macos")]
mod capture {
    use super::{
        CropRect, MAX_INLINE_EDGE, PERMISSION_MSG, WakeFn, encode_downscaled_png, err_response,
    };
    use bevy::prelude::Entity;
    use block2::RcBlock;
    use crossbeam_channel::Sender;
    use objc2::AllocAnyThread;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{CGBitmapContextCreate, CGImage, CGImageAlphaInfo};
    use objc2_foundation::NSError;
    use objc2_screen_capture_kit::{
        SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    };
    use std::ffi::c_void;
    use std::path::PathBuf;
    use vmux_agent::{ScreenshotImage, ScreenshotResponse};

    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    fn finish(
        tx: &Sender<ScreenshotResponse>,
        wake: &Option<WakeFn>,
        response: ScreenshotResponse,
    ) {
        let _ = tx.send(response);
        if let Some(w) = wake {
            w();
        }
    }

    fn window_number(window_entity: Entity) -> Option<u32> {
        use bevy::winit::WINIT_WINDOWS;
        use objc2_app_kit::NSView;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        WINIT_WINDOWS.with_borrow(|winit_windows| {
            let win = winit_windows.get_window(window_entity)?;
            let handle = win.window_handle().ok()?;
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return None;
            };
            let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
            let window = view.window()?;
            Some(window.windowNumber() as u32)
        })
    }

    #[allow(deprecated)]
    fn cgimage_to_rgba(image: &CGImage) -> Result<image::RgbaImage, String> {
        use objc2_core_graphics::{
            CGColorSpaceCreateDeviceRGB, CGContextDrawImage, CGImageGetHeight, CGImageGetWidth,
        };
        let width = CGImageGetWidth(Some(image)) as u32;
        let height = CGImageGetHeight(Some(image)) as u32;
        if width == 0 || height == 0 {
            return Err("captured image has zero dimension".into());
        }
        let bytes_per_row = width as usize * 4;
        let mut buf = vec![0u8; bytes_per_row * height as usize];
        let color_space = CGColorSpaceCreateDeviceRGB().ok_or("failed to create color space")?;
        let ctx = unsafe {
            CGBitmapContextCreate(
                buf.as_mut_ptr() as *mut c_void,
                width as usize,
                height as usize,
                8,
                bytes_per_row,
                Some(&color_space),
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        }
        .ok_or("failed to create bitmap context")?;
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContextDrawImage(Some(&ctx), rect, Some(image));
        drop(ctx);
        image::RgbaImage::from_raw(width, height, buf).ok_or_else(|| "pixel buffer mismatch".into())
    }

    fn encode_and_save(
        mut rgba: image::RgbaImage,
        crop: Option<CropRect>,
        request_id: [u8; 16],
        dir: PathBuf,
    ) -> ScreenshotResponse {
        if let Some(c) = crop {
            rgba = image::imageops::crop_imm(&rgba, c.x, c.y, c.w, c.h).to_image();
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return err_response(request_id, format!("cannot create {}: {e}", dir.display()));
        }
        let rid: String = request_id[..8].iter().map(|b| format!("{b:02x}")).collect();
        let path = dir.join(format!(
            "vmux-{}-{rid}.png",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        if let Err(e) = rgba.save(&path) {
            return err_response(request_id, format!("cannot save screenshot: {e}"));
        }
        match encode_downscaled_png(&rgba, MAX_INLINE_EDGE) {
            Ok((png, width, height)) => ScreenshotResponse {
                request_id,
                result: Ok(ScreenshotImage {
                    path: path.to_string_lossy().into_owned(),
                    png,
                    width,
                    height,
                }),
            },
            Err(e) => err_response(request_id, e),
        }
    }

    fn os_at_least_14() -> bool {
        use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
        let version = NSOperatingSystemVersion {
            majorVersion: 14,
            minorVersion: 0,
            patchVersion: 0,
        };
        NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
    }

    pub(crate) fn capture(
        window_entity: Entity,
        img_w: u32,
        img_h: u32,
        crop: Option<CropRect>,
        request_id: [u8; 16],
        base_dir: PathBuf,
        tx: Sender<ScreenshotResponse>,
        wake: Option<WakeFn>,
    ) {
        if !os_at_least_14() {
            finish(
                &tx,
                &wake,
                err_response(request_id, "screenshot requires macOS 14 or later"),
            );
            return;
        }
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            unsafe {
                CGRequestScreenCaptureAccess();
            }
            finish(&tx, &wake, err_response(request_id, PERMISSION_MSG));
            return;
        }
        let Some(window_id) = window_number(window_entity) else {
            finish(
                &tx,
                &wake,
                err_response(request_id, "cannot resolve native window"),
            );
            return;
        };

        let shareable_handler = RcBlock::new(
            move |content: *mut SCShareableContent, _err: *mut NSError| {
                if content.is_null() {
                    finish(
                        &tx,
                        &wake,
                        err_response(request_id, "SCShareableContent unavailable"),
                    );
                    return;
                }
                let content = unsafe { &*content };
                let windows = unsafe { content.windows() };
                let Some(window) = windows
                    .iter()
                    .find(|w| unsafe { w.windowID() } == window_id)
                else {
                    finish(
                        &tx,
                        &wake,
                        err_response(request_id, "vmux window not shareable"),
                    );
                    return;
                };

                let filter = unsafe {
                    SCContentFilter::initWithDesktopIndependentWindow(
                        SCContentFilter::alloc(),
                        &window,
                    )
                };
                let config = unsafe { SCStreamConfiguration::new() };
                unsafe {
                    config.setWidth(img_w as usize);
                    config.setHeight(img_h as usize);
                }

                let tx2 = tx.clone();
                let wake2 = wake.clone();
                let base_dir2 = base_dir.clone();
                let capture_handler =
                    RcBlock::new(move |image: *mut CGImage, _err: *mut NSError| {
                        if image.is_null() {
                            finish(
                                &tx2,
                                &wake2,
                                err_response(request_id, "capture returned no image"),
                            );
                            return;
                        }
                        let image_ref = unsafe { &*image };
                        let response = match cgimage_to_rgba(image_ref) {
                            Ok(rgba) => encode_and_save(rgba, crop, request_id, base_dir2.clone()),
                            Err(e) => err_response(request_id, e),
                        };
                        finish(&tx2, &wake2, response);
                    });

                unsafe {
                    SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                        &filter,
                        &config,
                        Some(&*capture_handler),
                    );
                }
            },
        );

        unsafe {
            SCShareableContent::getShareableContentWithCompletionHandler(&shareable_handler);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod capture {
    use super::{CropRect, WakeFn, err_response};
    use bevy::prelude::Entity;
    use crossbeam_channel::Sender;
    use std::path::PathBuf;
    use vmux_agent::ScreenshotResponse;

    pub(crate) fn capture(
        _window_entity: Entity,
        _img_w: u32,
        _img_h: u32,
        _crop: Option<CropRect>,
        request_id: [u8; 16],
        _base_dir: PathBuf,
        tx: Sender<ScreenshotResponse>,
        _wake: Option<WakeFn>,
    ) {
        let _ = tx.send(err_response(
            request_id,
            "screenshots are only supported on macOS",
        ));
    }
}
