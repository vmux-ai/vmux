#[cfg(target_os = "ios")]
mod platform {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::ptr;
    use std::sync::{LazyLock, Mutex};

    use dioxus::mobile::tao::platform::ios::WindowExtIOS;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObjectProtocol, ProtocolObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_av_foundation::{
        AVCaptureDevice, AVCaptureDeviceInput, AVCaptureMetadataOutput,
        AVCaptureMetadataOutputObjectsDelegate, AVCaptureSession, AVCaptureVideoPreviewLayer,
        AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo, AVMetadataMachineReadableCodeObject,
        AVMetadataObject, AVMetadataObjectTypeQRCode,
    };
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::{NSArray, NSString};
    use objc2_ui_kit::{
        NSTextAlignment, UIButton, UIButtonType, UIColor, UIControlEvents, UIControlState, UIFont,
        UILabel, UIModalPresentationStyle, UIViewController,
    };

    thread_local! {
        static ROOT_CONTROLLER: Cell<*mut UIViewController> = const { Cell::new(ptr::null_mut()) };
        static ACTIVE: Cell<bool> = const { Cell::new(false) };
    }

    static RESULTS: LazyLock<Mutex<VecDeque<Result<String, String>>>> =
        LazyLock::new(|| Mutex::new(VecDeque::new()));

    struct ScannerIvars {
        session: Retained<AVCaptureSession>,
        preview: Retained<AVCaptureVideoPreviewLayer>,
        title: Retained<UILabel>,
        cancel: Retained<UIButton>,
    }

    define_class!(
        #[unsafe(super(UIViewController))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxQrScannerController"]
        #[ivars = ScannerIvars]
        struct ScannerController;

        impl ScannerController {
            #[unsafe(method(viewDidLoad))]
            fn view_did_load(&self) {
                unsafe {
                    let _: () = msg_send![super(self), viewDidLoad];
                }
                let Some(view) = self.view() else { return };
                view.setBackgroundColor(Some(&UIColor::blackColor()));
                view.layer().addSublayer(&self.ivars().preview);
                view.addSubview(&self.ivars().title);
                view.addSubview(&self.ivars().cancel);
            }

            #[unsafe(method(viewDidLayoutSubviews))]
            fn view_did_layout_subviews(&self) {
                unsafe {
                    let _: () = msg_send![super(self), viewDidLayoutSubviews];
                }
                let Some(view) = self.view() else { return };
                let bounds = view.bounds();
                let safe = view.safeAreaInsets();
                self.ivars().preview.setFrame(bounds);
                self.ivars().title.setFrame(CGRect::new(
                    CGPoint::new(32.0, safe.top + 54.0),
                    CGSize::new((bounds.size.width - 64.0).max(0.0), 56.0),
                ));
                self.ivars().cancel.setFrame(CGRect::new(
                    CGPoint::new(18.0, safe.top + 8.0),
                    CGSize::new(76.0, 40.0),
                ));
            }

            #[unsafe(method(cancel))]
            fn cancel(&self) {
                self.close(None);
            }
        }

        unsafe impl NSObjectProtocol for ScannerController {}

        unsafe impl AVCaptureMetadataOutputObjectsDelegate for ScannerController {
            #[unsafe(method(captureOutput:didOutputMetadataObjects:fromConnection:))]
            fn capture_output(
                &self,
                _output: &AVCaptureMetadataOutput,
                metadata_objects: &NSArray<AVMetadataObject>,
                _connection: &objc2_av_foundation::AVCaptureConnection,
            ) {
                for object in metadata_objects {
                    let Some(code) = object.downcast_ref::<AVMetadataMachineReadableCodeObject>()
                    else {
                        continue;
                    };
                    let Some(value) = (unsafe { code.stringValue() }) else {
                        continue;
                    };
                    self.close(Some(Ok(value.to_string())));
                    return;
                }
            }
        }
    );

    impl ScannerController {
        fn new(
            marker: MainThreadMarker,
            session: Retained<AVCaptureSession>,
            preview: Retained<AVCaptureVideoPreviewLayer>,
        ) -> Retained<Self> {
            let title = UILabel::initWithFrame(UILabel::alloc(marker), CGRect::ZERO);
            title.setText(Some(&NSString::from_str("Scan the QR code shown by Vmux")));
            unsafe {
                title.setTextColor(Some(&UIColor::whiteColor()));
                title.setFont(Some(&UIFont::boldSystemFontOfSize(17.0)));
            }
            title.setTextAlignment(NSTextAlignment(1));
            title.setNumberOfLines(2);

            let cancel = UIButton::buttonWithType(UIButtonType::System, marker);
            cancel.setTitle_forState(Some(&NSString::from_str("Cancel")), UIControlState::Normal);
            cancel.setTitleColor_forState(Some(&UIColor::whiteColor()), UIControlState::Normal);
            cancel.setBackgroundColor(Some(&UIColor::colorWithWhite_alpha(0.0, 0.45)));
            cancel.layer().setCornerRadius(16.0);

            let this = Self::alloc(marker).set_ivars(ScannerIvars {
                session,
                preview,
                title,
                cancel,
            });
            let this: Retained<Self> = unsafe { msg_send![super(this), init] };
            unsafe {
                this.ivars().cancel.addTarget_action_forControlEvents(
                    Some(&this),
                    sel!(cancel),
                    UIControlEvents::TouchUpInside,
                );
            }
            this
        }

        fn close(&self, result: Option<Result<String, String>>) {
            if !ACTIVE.replace(false) {
                return;
            }
            unsafe {
                self.ivars().session.stopRunning();
            }
            if let Some(result) = result {
                RESULTS
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push_back(result);
            }
            self.dismissViewControllerAnimated_completion(true, None);
        }
    }

    pub fn install(window: &dioxus::mobile::DesktopContext) {
        ROOT_CONTROLLER.set(window.window.ui_view_controller().cast());
    }

    pub fn open() -> Result<(), String> {
        if ACTIVE.get() {
            return Ok(());
        }
        let marker = MainThreadMarker::new()
            .ok_or_else(|| "QR scanner must be opened from the main thread.".to_string())?;
        let root = ROOT_CONTROLLER
            .with(|pointer| unsafe { Retained::retain(pointer.get()) })
            .ok_or_else(|| "QR scanner is unavailable.".to_string())?;
        let media_type = unsafe { AVMediaTypeVideo }
            .ok_or_else(|| "Camera unavailable. Enter the pairing link instead.".to_string())?;
        let device = unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media_type) }
            .ok_or_else(|| "Camera unavailable. Enter the pairing link instead.".to_string())?;
        let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }
            .map_err(|error| format!("Could not open camera: {error}"))?;
        let session = unsafe { AVCaptureSession::new() };
        if !unsafe { session.canAddInput(&input) } {
            return Err("Camera input is unavailable.".to_string());
        }
        unsafe {
            session.addInput(&input);
        }

        let output = unsafe { AVCaptureMetadataOutput::new() };
        if !unsafe { session.canAddOutput(&output) } {
            return Err("QR scanning is unavailable on this device.".to_string());
        }
        unsafe {
            session.addOutput(&output);
        }

        let preview = unsafe { AVCaptureVideoPreviewLayer::layerWithSession(&session) };
        if let Some(gravity) = unsafe { AVLayerVideoGravityResizeAspectFill } {
            unsafe {
                preview.setVideoGravity(gravity);
            }
        }
        let controller = ScannerController::new(marker, session.clone(), preview);
        unsafe {
            output.setMetadataObjectsDelegate_queue(
                Some(ProtocolObject::from_ref(&*controller)),
                Some(DispatchQueue::main()),
            );
            let types = NSArray::from_slice(&[AVMetadataObjectTypeQRCode]);
            output.setMetadataObjectTypes(Some(&types));
            session.startRunning();
        }
        controller.setModalPresentationStyle(UIModalPresentationStyle::FullScreen);
        ACTIVE.set(true);
        root.presentViewController_animated_completion(&controller, true, None);
        Ok(())
    }

    pub fn take_result() -> Option<Result<String, String>> {
        RESULTS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    pub fn install(_: &dioxus::mobile::DesktopContext) {}

    pub fn open() -> Result<(), String> {
        Err(
            "QR scanning is not available on this platform yet. Enter the pairing link instead."
                .to_string(),
        )
    }

    pub fn take_result() -> Option<Result<String, String>> {
        None
    }
}

pub use platform::*;
