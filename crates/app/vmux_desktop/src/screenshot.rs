use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
#[cfg(target_os = "macos")]
use bevy::winit::WinitUserEvent;
use crossbeam_channel::{Receiver, Sender};
#[cfg(target_os = "macos")]
use std::sync::Arc;
use vmux_input::{ScreenshotRequest, ScreenshotResponse};

#[cfg(any(target_os = "macos", test))]
use crate::capture_output::CaptureSize;
use crate::capture_output::{CaptureOutput, CaptureSource, CropRect};

pub(crate) struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_bridge).add_systems(
            Update,
            (start_screenshots, drain_screenshots)
                .chain()
                .after(vmux_command::WriteCommandRequests),
        );
    }
}

fn spawn_bridge(mut commands: Commands) {
    commands.spawn((Name::new("Screenshot capture"), ScreenshotBridge::default()));
}

#[derive(Component)]
struct ScreenshotBridge {
    tx: Sender<ScreenshotResponse>,
    rx: Receiver<ScreenshotResponse>,
}

impl ScreenshotBridge {
    fn err_response(request_id: [u8; 16], message: impl Into<String>) -> ScreenshotResponse {
        ScreenshotResponse {
            request_id,
            result: Err(message.into()),
        }
    }
}

impl Default for ScreenshotBridge {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

fn start_screenshots(
    _non_send: NonSendMarker,
    mut reader: MessageReader<ScreenshotRequest>,
    bridge: Query<&ScreenshotBridge>,
    output: CaptureOutput,
    source: CaptureSource,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let Ok(bridge) = bridge.single() else {
        return;
    };
    let base_dir = output.directory();
    for req in reader.read() {
        if !vmux_profile::Product::screen() {
            let _ = bridge.tx.send(ScreenshotBridge::err_response(
                req.request_id,
                "Screen capture is disabled by this application.",
            ));
            continue;
        }
        let capture = match source.resolve(req.pane.as_deref()) {
            Ok(capture) => capture,
            Err(message) => {
                let _ = bridge
                    .tx
                    .send(ScreenshotBridge::err_response(req.request_id, message));
                continue;
            }
        };
        let base_dir = match &base_dir {
            Ok(path) => path.clone(),
            Err(message) => {
                let _ = bridge.tx.send(ScreenshotBridge::err_response(
                    req.request_id,
                    message.clone(),
                ));
                continue;
            }
        };

        let tx = bridge.tx.clone();
        NativeScreenshot::capture(
            capture.window,
            capture.size.width,
            capture.size.height,
            capture.crop,
            req.request_id,
            base_dir,
            tx,
            proxy.as_deref(),
        );
    }
}

fn drain_screenshots(
    bridge: Query<&ScreenshotBridge>,
    mut writer: MessageWriter<ScreenshotResponse>,
) {
    let Ok(bridge) = bridge.single() else {
        return;
    };
    while let Ok(response) = bridge.rx.try_recv() {
        writer.write(response);
    }
}

struct NativeScreenshot;

impl NativeScreenshot {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    const MAX_INLINE_EDGE: u32 = 1568;
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    const PERMISSION_MSG: &'static str = "Screen Recording permission required - grant it in System Settings > \
Privacy & Security > Screen Recording, then call screenshot again.";

    #[cfg(target_os = "macos")]
    fn make_wake(proxy: Option<&EventLoopProxyWrapper>) -> Option<Arc<dyn Fn() + Send + Sync>> {
        proxy.map(|p| {
            let proxy = (**p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as Arc<dyn Fn() + Send + Sync>
        })
    }

    #[cfg(any(target_os = "macos", test))]
    fn encode_downscaled_png(
        img: &image::RgbaImage,
        max_edge: u32,
    ) -> Result<(Vec<u8>, u32, u32), String> {
        let size = CaptureSize::new(img.width(), img.height()).downscaled(max_edge);
        let dynimg = image::DynamicImage::ImageRgba8(img.clone());
        let scaled = if (size.width, size.height) == (img.width(), img.height()) {
            dynimg
        } else {
            dynimg.resize_exact(
                size.width,
                size.height,
                image::imageops::FilterType::Lanczos3,
            )
        };
        let mut buf = std::io::Cursor::new(Vec::new());
        scaled
            .write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("png encode failed: {e}"))?;
        Ok((buf.into_inner(), size.width, size.height))
    }

    #[allow(clippy::too_many_arguments)]
    fn capture(
        window_entity: Entity,
        img_w: u32,
        img_h: u32,
        crop: Option<CropRect>,
        request_id: [u8; 16],
        base_dir: std::path::PathBuf,
        tx: Sender<ScreenshotResponse>,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        #[cfg(target_os = "macos")]
        Self::capture_macos(
            window_entity,
            img_w,
            img_h,
            crop,
            request_id,
            base_dir,
            tx,
            Self::make_wake(proxy),
        );
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (window_entity, img_w, img_h, crop, base_dir, proxy);
            let _ = tx.send(ScreenshotBridge::err_response(
                request_id,
                "screenshots are only supported on macOS",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_setting::AppSettings;

    #[test]
    fn missing_profile_refuses_screenshot_before_native_capture() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ScreenshotPlugin))
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .insert_resource(AppSettings::default());
        app.world_mut().spawn((Window::default(), vmux_ecs::Active));
        app.world_mut().write_message(ScreenshotRequest {
            request_id: [41; 16],
            pane: None,
        });
        app.update();
        let responses: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<ScreenshotResponse>>()
            .drain()
            .collect();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, [41; 16]);
        match &responses[0].result {
            Err(error) => assert_eq!(error, "no active capture profile"),
            Ok(_) => panic!("a missing Profile must refuse before native capture"),
        }
    }

    #[test]
    fn encode_downscaled_png_emits_png_header() {
        let img = image::RgbaImage::new(10, 10);
        let (png, w, h) = NativeScreenshot::encode_downscaled_png(&img, 1568).unwrap();
        assert_eq!((w, h), (10, 10));
        assert_eq!(&png[..4], &[137, 80, 78, 71]);
    }
}

#[cfg(target_os = "macos")]
mod capture {
    use super::{CropRect, NativeScreenshot, ScreenshotBridge};
    use bevy::prelude::Entity;
    use bevy::winit::WINIT_WINDOWS;
    use block2::RcBlock;
    use crossbeam_channel::Sender;
    use objc2::AllocAnyThread;
    use objc2_app_kit::NSView;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    };
    use objc2_foundation::{NSError, NSOperatingSystemVersion, NSProcessInfo};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::sync::Arc;
    use vmux_input::{ScreenshotImage, ScreenshotResponse};

    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    impl NativeScreenshot {
        fn finish(
            tx: &Sender<ScreenshotResponse>,
            wake: &Option<Arc<dyn Fn() + Send + Sync>>,
            response: ScreenshotResponse,
        ) {
            let _ = tx.send(response);
            if let Some(w) = wake {
                w();
            }
        }

        fn window_number(window_entity: Entity) -> Option<u32> {
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

        fn cgimage_to_rgba(image: &CGImage) -> Result<image::RgbaImage, String> {
            let width = CGImage::width(Some(image)) as u32;
            let height = CGImage::height(Some(image)) as u32;
            if width == 0 || height == 0 {
                return Err("captured image has zero dimension".into());
            }
            let bytes_per_row = width as usize * 4;
            let mut buf = vec![0u8; bytes_per_row * height as usize];
            let color_space =
                CGColorSpace::new_device_rgb().ok_or("failed to create color space")?;
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
            CGContext::draw_image(Some(&ctx), rect, Some(image));
            drop(ctx);
            image::RgbaImage::from_raw(width, height, buf)
                .ok_or_else(|| "pixel buffer mismatch".into())
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
                return ScreenshotBridge::err_response(
                    request_id,
                    format!("cannot create {}: {e}", dir.display()),
                );
            }
            let rid: String = request_id[..8].iter().map(|b| format!("{b:02x}")).collect();
            let path = dir.join(format!(
                "vmux-{}-{rid}.png",
                chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
            ));
            if let Err(e) = rgba.save(&path) {
                return ScreenshotBridge::err_response(
                    request_id,
                    format!("cannot save screenshot: {e}"),
                );
            }
            match Self::encode_downscaled_png(&rgba, Self::MAX_INLINE_EDGE) {
                Ok((png, width, height)) => ScreenshotResponse {
                    request_id,
                    result: Ok(ScreenshotImage {
                        path: path.to_string_lossy().into_owned(),
                        png,
                        width,
                        height,
                    }),
                },
                Err(e) => ScreenshotBridge::err_response(request_id, e),
            }
        }

        fn os_at_least_14() -> bool {
            let version = NSOperatingSystemVersion {
                majorVersion: 14,
                minorVersion: 0,
                patchVersion: 0,
            };
            NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
        }

        #[allow(clippy::too_many_arguments)]
        fn capture_window(
            window: &objc2_screen_capture_kit::SCWindow,
            img_w: u32,
            img_h: u32,
            crop: Option<CropRect>,
            request_id: [u8; 16],
            base_dir: PathBuf,
            tx: Sender<ScreenshotResponse>,
            wake: Option<Arc<dyn Fn() + Send + Sync>>,
        ) {
            let filter = unsafe {
                SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), window)
            };
            let config = unsafe { SCStreamConfiguration::new() };
            unsafe {
                config.setWidth(img_w as usize);
                config.setHeight(img_h as usize);
            }
            let capture_handler = RcBlock::new(move |image: *mut CGImage, _err: *mut NSError| {
                if image.is_null() {
                    Self::finish(
                        &tx,
                        &wake,
                        ScreenshotBridge::err_response(request_id, "capture returned no image"),
                    );
                    return;
                }
                let image_ref = unsafe { &*image };
                let response = match Self::cgimage_to_rgba(image_ref) {
                    Ok(rgba) => Self::encode_and_save(rgba, crop, request_id, base_dir.clone()),
                    Err(e) => ScreenshotBridge::err_response(request_id, e),
                };
                Self::finish(&tx, &wake, response);
            });
            unsafe {
                SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                    &filter,
                    &config,
                    Some(&*capture_handler),
                );
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn capture_macos(
            window_entity: Entity,
            img_w: u32,
            img_h: u32,
            crop: Option<CropRect>,
            request_id: [u8; 16],
            base_dir: PathBuf,
            tx: Sender<ScreenshotResponse>,
            wake: Option<Arc<dyn Fn() + Send + Sync>>,
        ) {
            if !Self::os_at_least_14() {
                Self::finish(
                    &tx,
                    &wake,
                    ScreenshotBridge::err_response(
                        request_id,
                        "screenshot requires macOS 14 or later",
                    ),
                );
                return;
            }
            if !unsafe { CGPreflightScreenCaptureAccess() } {
                unsafe {
                    CGRequestScreenCaptureAccess();
                }
                Self::finish(
                    &tx,
                    &wake,
                    ScreenshotBridge::err_response(request_id, Self::PERMISSION_MSG),
                );
                return;
            }
            let Some(window_id) = Self::window_number(window_entity) else {
                Self::finish(
                    &tx,
                    &wake,
                    ScreenshotBridge::err_response(request_id, "cannot resolve native window"),
                );
                return;
            };
            let shareable_handler = RcBlock::new(
                move |content: *mut SCShareableContent, _err: *mut NSError| {
                    if content.is_null() {
                        Self::finish(
                            &tx,
                            &wake,
                            ScreenshotBridge::err_response(
                                request_id,
                                "SCShareableContent unavailable",
                            ),
                        );
                        return;
                    }
                    let content = unsafe { &*content };
                    let windows = unsafe { content.windows() };
                    let Some(window) = windows
                        .iter()
                        .find(|w| unsafe { w.windowID() } == window_id)
                    else {
                        Self::finish(
                            &tx,
                            &wake,
                            ScreenshotBridge::err_response(request_id, "vmux window not shareable"),
                        );
                        return;
                    };
                    Self::capture_window(
                        &window,
                        img_w,
                        img_h,
                        crop,
                        request_id,
                        base_dir.clone(),
                        tx.clone(),
                        wake.clone(),
                    );
                },
            );
            unsafe {
                SCShareableContent::getShareableContentWithCompletionHandler(&shareable_handler);
            }
        }
    }
}
