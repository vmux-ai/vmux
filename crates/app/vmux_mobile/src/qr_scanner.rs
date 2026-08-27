#[cfg(target_os = "ios")]
mod platform {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::ptr;
    use std::sync::{LazyLock, Mutex};

    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObjectProtocol, ProtocolObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureMetadataOutput,
        AVCaptureMetadataOutputObjectsDelegate, AVCaptureSession,
        AVCaptureSessionRuntimeErrorNotification, AVCaptureVideoPreviewLayer,
        AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo, AVMetadataMachineReadableCodeObject,
        AVMetadataObject, AVMetadataObjectTypeQRCode,
    };
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::{
        NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSString, NSURL,
    };
    use objc2_ui_kit::{
        NSTextAlignment, UIApplication, UIApplicationOpenSettingsURLString, UIButton, UIButtonType,
        UIColor, UIControlEvents, UIControlState, UIFont, UILabel, UIModalPresentationStyle,
        UIViewController,
    };
    use vmux_ui::i18n::{TranslationValue, translate, translate_with};

    thread_local! {
        static ROOT_CONTROLLER: Cell<*mut UIViewController> = const { Cell::new(ptr::null_mut()) };
        static ACTIVE: Cell<bool> = const { Cell::new(false) };
        static REQUESTING: Cell<bool> = const { Cell::new(false) };
    }

    static RESULTS: LazyLock<Mutex<VecDeque<Result<String, String>>>> =
        LazyLock::new(|| Mutex::new(VecDeque::new()));

    struct ScannerIvars {
        capture: Option<Capture>,
        title: Retained<UILabel>,
        cancel: Retained<UIButton>,
        settings: Retained<UIButton>,
    }

    struct Capture {
        session: Retained<AVCaptureSession>,
        preview: Retained<AVCaptureVideoPreviewLayer>,
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
                match &self.ivars().capture {
                    Some(capture) => view.layer().addSublayer(&capture.preview),
                    None => view.addSubview(&self.ivars().settings),
                }
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
                let width = (bounds.size.width - 64.0).max(0.0);
                if let Some(capture) = &self.ivars().capture {
                    capture.preview.setFrame(bounds);
                }
                let title_top = if self.ivars().capture.is_some() {
                    safe.top + 54.0
                } else {
                    (bounds.size.height * 0.32).max(safe.top + 54.0)
                };
                let title_height = if self.ivars().capture.is_some() {
                    56.0
                } else {
                    140.0
                };
                self.ivars().title.setFrame(CGRect::new(
                    CGPoint::new(32.0, title_top),
                    CGSize::new(width, title_height),
                ));
                self.ivars().cancel.setFrame(CGRect::new(
                    CGPoint::new(18.0, safe.top + 8.0),
                    CGSize::new(76.0, 40.0),
                ));
                if self.ivars().capture.is_none() {
                    self.ivars().settings.setFrame(CGRect::new(
                        CGPoint::new(32.0, title_top + title_height + 24.0),
                        CGSize::new(width, 48.0),
                    ));
                }
            }

            #[unsafe(method(cancel))]
            fn cancel(&self) {
                self.close(None);
            }

            #[unsafe(method(openSettings))]
            fn open_settings(&self) {
                let Some(marker) = MainThreadMarker::new() else {
                    return;
                };
                let Some(url) = (unsafe { NSURL::URLWithString(UIApplicationOpenSettingsURLString) })
                else {
                    return;
                };
                unsafe {
                    UIApplication::sharedApplication(marker)
                        .openURL_options_completionHandler(&url, &NSDictionary::new(), None);
                }
                self.close(None);
            }

            #[unsafe(method(sessionRuntimeError:))]
            fn session_runtime_error(&self, _notification: &NSNotification) {
                self.close(Some(Err(translate("mobile-qr-session-error"))));
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
            capture: Option<Capture>,
            message: &str,
        ) -> Retained<Self> {
            let denied = capture.is_none();

            let title = UILabel::initWithFrame(UILabel::alloc(marker), CGRect::ZERO);
            title.setText(Some(&NSString::from_str(message)));
            unsafe {
                title.setTextColor(Some(&UIColor::whiteColor()));
                title.setFont(Some(&UIFont::boldSystemFontOfSize(17.0)));
            }
            title.setTextAlignment(NSTextAlignment(1));
            title.setNumberOfLines(if denied { 0 } else { 2 });

            let cancel = UIButton::buttonWithType(UIButtonType::System, marker);
            cancel.setTitle_forState(
                Some(&NSString::from_str(&translate("mobile-qr-cancel"))),
                UIControlState::Normal,
            );
            cancel.setTitleColor_forState(Some(&UIColor::whiteColor()), UIControlState::Normal);
            cancel.setBackgroundColor(Some(&UIColor::colorWithWhite_alpha(0.0, 0.45)));
            cancel.layer().setCornerRadius(16.0);

            let settings = UIButton::buttonWithType(UIButtonType::System, marker);
            settings.setTitle_forState(
                Some(&NSString::from_str(&translate("mobile-qr-open-settings"))),
                UIControlState::Normal,
            );
            settings.setTitleColor_forState(Some(&UIColor::blackColor()), UIControlState::Normal);
            settings.setBackgroundColor(Some(&UIColor::whiteColor()));
            settings.layer().setCornerRadius(16.0);

            let this = Self::alloc(marker).set_ivars(ScannerIvars {
                capture,
                title,
                cancel,
                settings,
            });
            let this: Retained<Self> = unsafe { msg_send![super(this), init] };
            unsafe {
                this.ivars().cancel.addTarget_action_forControlEvents(
                    Some(&this),
                    sel!(cancel),
                    UIControlEvents::TouchUpInside,
                );
                this.ivars().settings.addTarget_action_forControlEvents(
                    Some(&this),
                    sel!(openSettings),
                    UIControlEvents::TouchUpInside,
                );
                if let Some(capture) = &this.ivars().capture {
                    NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                        &this,
                        sel!(sessionRuntimeError:),
                        Some(AVCaptureSessionRuntimeErrorNotification),
                        Some(&capture.session),
                    );
                }
            }
            this
        }

        fn close(&self, result: Option<Result<String, String>>) {
            if !ACTIVE.replace(false) {
                return;
            }
            if let Some(capture) = &self.ivars().capture {
                unsafe {
                    NSNotificationCenter::defaultCenter().removeObserver(self);
                    capture.session.stopRunning();
                }
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

    pub fn install(root_controller: &UIViewController) {
        ROOT_CONTROLLER.set((root_controller as *const UIViewController).cast_mut());
    }

    pub enum ScannerSupport {
        Available,
        Unavailable(String),
    }

    impl ScannerSupport {
        pub fn detect() -> Self {
            let Some(media_type) = (unsafe { AVMediaTypeVideo }) else {
                return Self::Unavailable(translate("mobile-qr-camera-unavailable"));
            };
            let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
            if matches!(
                status,
                AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted
            ) {
                return Self::Available;
            }
            match unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media_type) } {
                Some(_) => Self::Available,
                None => Self::Unavailable(translate("mobile-qr-camera-unavailable")),
            }
        }
    }

    pub fn open() -> Result<(), String> {
        if ACTIVE.get() || REQUESTING.get() {
            return Ok(());
        }
        let marker = MainThreadMarker::new()
            .ok_or_else(|| "QR scanner must be opened from the main thread.".to_string())?;
        let media_type =
            unsafe { AVMediaTypeVideo }.ok_or_else(|| translate("mobile-qr-camera-unavailable"))?;

        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) } {
            AVAuthorizationStatus::Authorized => present(marker),
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                present_denied(marker)
            }
            _ => {
                REQUESTING.set(true);
                let handler = RcBlock::new(move |granted: Bool| {
                    DispatchQueue::main().exec_async(move || {
                        REQUESTING.set(false);
                        let Some(marker) = MainThreadMarker::new() else {
                            return;
                        };
                        let outcome = if granted.as_bool() {
                            present(marker)
                        } else {
                            present_denied(marker)
                        };
                        if let Err(message) = outcome {
                            RESULTS
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .push_back(Err(message));
                        }
                    });
                });
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(
                        media_type, &handler,
                    );
                }
                Ok(())
            }
        }
    }

    fn present_denied(marker: MainThreadMarker) -> Result<(), String> {
        let root = ROOT_CONTROLLER
            .with(|pointer| unsafe { Retained::retain(pointer.get()) })
            .ok_or_else(|| translate("mobile-qr-unavailable"))?;
        let controller = ScannerController::new(marker, None, &translate("mobile-qr-denied"));
        controller.setModalPresentationStyle(UIModalPresentationStyle::FullScreen);
        ACTIVE.set(true);
        root.presentViewController_animated_completion(&controller, true, None);
        Ok(())
    }

    fn present(marker: MainThreadMarker) -> Result<(), String> {
        let root = ROOT_CONTROLLER
            .with(|pointer| unsafe { Retained::retain(pointer.get()) })
            .ok_or_else(|| translate("mobile-qr-unavailable"))?;
        let media_type =
            unsafe { AVMediaTypeVideo }.ok_or_else(|| translate("mobile-qr-camera-unavailable"))?;
        let device = unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media_type) }
            .ok_or_else(|| translate("mobile-qr-camera-unavailable"))?;
        let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }.map_err(
            |error| {
                translate_with(
                    "mobile-qr-camera-failed",
                    &[("error", TranslationValue::String(&error.to_string()))],
                )
            },
        )?;
        let session = unsafe { AVCaptureSession::new() };
        if !unsafe { session.canAddInput(&input) } {
            return Err(translate("mobile-qr-camera-input-unavailable"));
        }
        unsafe {
            session.addInput(&input);
        }

        let output = unsafe { AVCaptureMetadataOutput::new() };
        if !unsafe { session.canAddOutput(&output) } {
            return Err(translate("mobile-qr-unsupported-device"));
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
        let controller = ScannerController::new(
            marker,
            Some(Capture {
                session: session.clone(),
                preview,
            }),
            &translate("mobile-qr-title"),
        );
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
    #![allow(dead_code)]

    use vmux_ui::i18n::translate;

    pub fn install(_: &()) {}

    pub enum ScannerSupport {
        Available,
        Unavailable(String),
    }

    impl ScannerSupport {
        pub fn detect() -> Self {
            Self::Unavailable(translate("mobile-qr-unsupported-platform"))
        }
    }

    pub fn open() -> Result<(), String> {
        Err(translate("mobile-qr-unsupported-platform"))
    }

    pub fn take_result() -> Option<Result<String, String>> {
        None
    }
}

pub use platform::*;
