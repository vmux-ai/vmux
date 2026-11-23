#[cfg(target_os = "ios")]
pub use platform::ScannerSupport;
#[cfg(not(target_os = "ios"))]
pub use platform::UnsupportedScanner as ScannerSupport;

#[cfg(target_os = "ios")]
mod platform {
    use std::cell::{Cell, RefCell};
    use std::ptr;

    use bevy_ecs::entity::Entity;
    use block2::RcBlock;
    use dioxus::mobile::tao::platform::ios::WindowExtIOS;
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
    use vmux_ecs::EntityTarget;
    use vmux_ui::i18n::{TranslationValue, translate, translate_with};

    use crate::connection::{ActiveScan, CancelScan, ScanCompleted, ScanRequest, ScanTicket};
    use crate::runtime::RuntimeHandle;

    struct ScannerHost {
        root_controller: *mut UIViewController,
        root_generation: u64,
        active: Option<ScanTicket>,
        controller: Option<Retained<ScannerController>>,
        requesting: Option<ScanTicket>,
        runtime: Option<RuntimeHandle>,
    }

    impl ScannerHost {
        const fn empty() -> Self {
            Self {
                root_controller: ptr::null_mut(),
                root_generation: 0,
                active: None,
                controller: None,
                requesting: None,
                runtime: None,
            }
        }

        fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
            thread_local! {
                static HOST: RefCell<ScannerHost> = const { RefCell::new(ScannerHost::empty()) };
            }
            HOST.with_borrow(f)
        }

        fn with_mut<R>(f: impl FnOnce(&mut Self) -> R) -> R {
            thread_local! {
                static HOST: RefCell<ScannerHost> = const { RefCell::new(ScannerHost::empty()) };
            }
            HOST.with_borrow_mut(f)
        }

        fn next_root_generation() -> u64 {
            thread_local! {
                static NEXT_ROOT_GENERATION: Cell<u64> = const { Cell::new(0) };
            }
            NEXT_ROOT_GENERATION.with(|cell| {
                let next = cell.get().wrapping_add(1).max(1);
                cell.set(next);
                next
            })
        }
    }

    struct ScannerIvars {
        ticket: ScanTicket,
        root_generation: u64,
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
        #[ivars = (EntityTarget<ActiveScan>, ScannerIvars)]
        struct ScannerController;

        impl ScannerController {
            #[unsafe(method(viewDidLoad))]
            fn view_did_load(&self) {
                unsafe {
                    let _: () = msg_send![super(self), viewDidLoad];
                }
                let Some(view) = self.view() else { return };
                view.setBackgroundColor(Some(&UIColor::blackColor()));
                match &self.ivars().1.capture {
                    Some(capture) => view.layer().addSublayer(&capture.preview),
                    None => view.addSubview(&self.ivars().1.settings),
                }
                view.addSubview(&self.ivars().1.title);
                view.addSubview(&self.ivars().1.cancel);
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
                if let Some(capture) = &self.ivars().1.capture {
                    capture.preview.setFrame(bounds);
                }
                let title_top = if self.ivars().1.capture.is_some() {
                    safe.top + 54.0
                } else {
                    (bounds.size.height * 0.32).max(safe.top + 54.0)
                };
                let title_height = if self.ivars().1.capture.is_some() {
                    56.0
                } else {
                    140.0
                };
                self.ivars().1.title.setFrame(CGRect::new(
                    CGPoint::new(32.0, title_top),
                    CGSize::new(width, title_height),
                ));
                self.ivars().1.cancel.setFrame(CGRect::new(
                    CGPoint::new(18.0, safe.top + 8.0),
                    CGSize::new(76.0, 40.0),
                ));
                if self.ivars().1.capture.is_none() {
                    self.ivars().1.settings.setFrame(CGRect::new(
                        CGPoint::new(32.0, title_top + title_height + 24.0),
                        CGSize::new(width, 48.0),
                    ));
                }
            }

            #[unsafe(method(cancel))]
            fn cancel(&self) {
                ScannerHost::close_controller(self, None);
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
                ScannerHost::close_controller(self, None);
            }

            #[unsafe(method(sessionRuntimeError:))]
            fn session_runtime_error(&self, _notification: &NSNotification) {
                ScannerHost::close_controller(self, Some(Err(translate("mobile-qr-session-error"))));
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
                    ScannerHost::close_controller(self, Some(Ok(value.to_string())));
                    return;
                }
            }
        }
    );

    impl ScannerHost {
        fn new_controller(
            marker: MainThreadMarker,
            source: Entity,
            ticket: ScanTicket,
            root_generation: u64,
            capture: Option<Capture>,
            message: &str,
        ) -> Retained<ScannerController> {
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

            let this = ScannerController::alloc(marker).set_ivars((
                EntityTarget::new(source),
                ScannerIvars {
                    ticket,
                    root_generation,
                    capture,
                    title,
                    cancel,
                    settings,
                },
            ));
            let this: Retained<ScannerController> = unsafe { msg_send![super(this), init] };
            Self::bind_controller(&this);
            this
        }

        fn bind_controller(controller: &ScannerController) {
            unsafe {
                controller
                    .ivars()
                    .1
                    .cancel
                    .addTarget_action_forControlEvents(
                        Some(controller),
                        sel!(cancel),
                        UIControlEvents::TouchUpInside,
                    );
                controller
                    .ivars()
                    .1
                    .settings
                    .addTarget_action_forControlEvents(
                        Some(controller),
                        sel!(openSettings),
                        UIControlEvents::TouchUpInside,
                    );
                if let Some(capture) = &controller.ivars().1.capture {
                    NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                        controller,
                        sel!(sessionRuntimeError:),
                        Some(AVCaptureSessionRuntimeErrorNotification),
                        Some(&capture.session),
                    );
                }
            }
        }

        fn stop_capture(controller: &ScannerController) {
            if let Some(capture) = &controller.ivars().1.capture {
                unsafe {
                    NSNotificationCenter::defaultCenter().removeObserver(controller);
                    capture.session.stopRunning();
                }
            }
        }

        fn close_controller(
            controller: &ScannerController,
            result: Option<Result<String, String>>,
        ) {
            let ticket = controller.ivars().1.ticket;
            let root_generation = controller.ivars().1.root_generation;
            let deliver = Self::with_mut(|host| {
                if host.active != Some(ticket) {
                    return false;
                }
                host.active = None;
                host.controller = None;
                host.root_generation == root_generation
            });
            Self::stop_capture(controller);
            if deliver && let Some(result) = result {
                report(ticket, result);
            }
            controller.dismissViewControllerAnimated_completion(true, None);
        }
    }

    impl CancelScan {
        pub(crate) fn cancel_native() {
            let displaced = ScannerHost::with_mut(|host| {
                host.active = None;
                host.requesting = None;
                host.controller.take()
            });
            if let Some(previous) = displaced {
                ScannerHost::stop_capture(&previous);
                previous.dismissViewControllerAnimated_completion(true, None);
            }
        }
    }

    pub struct ScannerSupport(Option<String>);

    impl ScannerSupport {
        pub fn detect(request: &ScanRequest) -> Self {
            if !request.permitted() {
                return Self(Some(translate("mobile-qr-camera-unavailable")));
            }
            let Some(media_type) = (unsafe { AVMediaTypeVideo }) else {
                return Self(Some(translate("mobile-qr-camera-unavailable")));
            };
            let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
            if matches!(
                status,
                AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted
            ) {
                return Self(None);
            }
            match unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media_type) } {
                Some(_) => Self(None),
                None => Self(Some(translate("mobile-qr-camera-unavailable"))),
            }
        }

        pub fn unavailable(self) -> Option<String> {
            self.0
        }
    }

    impl ScanRequest {
        pub(crate) fn install_host(
            window: &dioxus::mobile::DesktopContext,
            runtime: RuntimeHandle,
        ) {
            let generation = ScannerHost::next_root_generation();
            let controller: *mut UIViewController = window.window.ui_view_controller().cast();
            let displaced = ScannerHost::with_mut(|host| {
                host.root_controller = controller;
                host.root_generation = generation;
                host.active = None;
                host.requesting = None;
                host.runtime = Some(runtime);
                host.controller.take()
            });
            if let Some(previous) = displaced {
                ScannerHost::stop_capture(&previous);
                previous.dismissViewControllerAnimated_completion(false, None);
            }
        }

        pub(crate) fn open_with_ticket(
            &self,
            source: Entity,
            ticket: ScanTicket,
        ) -> Result<(), String> {
            if !self.permitted() {
                return Err(translate("mobile-qr-camera-unavailable"));
            }
            let (busy, root_generation) = ScannerHost::with(|host| {
                (
                    host.active.is_some() || host.requesting.is_some(),
                    host.root_generation,
                )
            });
            if busy {
                return Ok(());
            }
            let marker = MainThreadMarker::new()
                .ok_or_else(|| "QR scanner must be opened from the main thread.".to_string())?;
            let media_type = unsafe { AVMediaTypeVideo }
                .ok_or_else(|| translate("mobile-qr-camera-unavailable"))?;

            match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) } {
                AVAuthorizationStatus::Authorized => {
                    present(marker, source, ticket, root_generation)
                }
                AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                    present_denied(marker, source, ticket, root_generation)
                }
                _ => request_permission(media_type, source, ticket, root_generation),
            }
        }
    }

    fn request_permission(
        media_type: &'static objc2_av_foundation::AVMediaType,
        source: Entity,
        ticket: ScanTicket,
        root_generation: u64,
    ) -> Result<(), String> {
        ScannerHost::with_mut(|host| host.requesting = Some(ticket));
        let handler = RcBlock::new(move |granted: Bool| {
            DispatchQueue::main().exec_async(move || {
                let current = ScannerHost::with_mut(|host| {
                    if host.requesting != Some(ticket) || host.root_generation != root_generation {
                        return false;
                    }
                    host.requesting = None;
                    true
                });
                if !current {
                    return;
                }
                let Some(marker) = MainThreadMarker::new() else {
                    return;
                };
                let outcome = if granted.as_bool() {
                    present(marker, source, ticket, root_generation)
                } else {
                    present_denied(marker, source, ticket, root_generation)
                };
                if let Err(message) = outcome {
                    report(ticket, Err(message));
                }
            });
        });
        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
        }
        Ok(())
    }

    fn present_denied(
        marker: MainThreadMarker,
        source: Entity,
        ticket: ScanTicket,
        root_generation: u64,
    ) -> Result<(), String> {
        let root = current_root(root_generation)?;
        let controller = ScannerHost::new_controller(
            marker,
            source,
            ticket,
            root_generation,
            None,
            &translate("mobile-qr-denied"),
        );
        controller.setModalPresentationStyle(UIModalPresentationStyle::FullScreen);
        ScannerHost::with_mut(|host| {
            host.active = Some(ticket);
            host.controller = Some(controller.clone());
        });
        root.presentViewController_animated_completion(&controller, true, None);
        Ok(())
    }

    fn present(
        marker: MainThreadMarker,
        source: Entity,
        ticket: ScanTicket,
        root_generation: u64,
    ) -> Result<(), String> {
        let root = current_root(root_generation)?;
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
        unsafe { session.addInput(&input) };
        let output = unsafe { AVCaptureMetadataOutput::new() };
        if !unsafe { session.canAddOutput(&output) } {
            return Err(translate("mobile-qr-unsupported-device"));
        }
        unsafe { session.addOutput(&output) };
        let preview = unsafe { AVCaptureVideoPreviewLayer::layerWithSession(&session) };
        if let Some(gravity) = unsafe { AVLayerVideoGravityResizeAspectFill } {
            unsafe { preview.setVideoGravity(gravity) };
        }
        let controller = ScannerHost::new_controller(
            marker,
            source,
            ticket,
            root_generation,
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
        ScannerHost::with_mut(|host| {
            host.active = Some(ticket);
            host.controller = Some(controller.clone());
        });
        root.presentViewController_animated_completion(&controller, true, None);
        Ok(())
    }

    fn current_root(root_generation: u64) -> Result<Retained<UIViewController>, String> {
        ScannerHost::with(|host| {
            if host.root_generation != root_generation {
                return None;
            }
            unsafe { Retained::retain(host.root_controller) }
        })
        .ok_or_else(|| translate("mobile-qr-unavailable"))
    }

    fn report(ticket: ScanTicket, result: Result<String, String>) {
        ScannerHost::with(|host| {
            let Some(runtime) = host.runtime.as_ref() else {
                return;
            };
            runtime.send(ScanCompleted { ticket, result });
        });
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    use bevy_ecs::entity::Entity;
    use vmux_ui::i18n::translate;

    use crate::connection::{CancelScan, ScanRequest, ScanTicket};
    use crate::runtime::RuntimeHandle;

    pub struct UnsupportedScanner(Option<String>);

    impl UnsupportedScanner {
        pub fn detect(request: &ScanRequest) -> Self {
            if !request.permitted() {
                return Self(Some(translate("mobile-qr-camera-unavailable")));
            }
            Self(Some(translate("mobile-qr-unsupported-platform")))
        }

        pub fn unavailable(self) -> Option<String> {
            self.0
        }
    }

    impl CancelScan {
        pub(crate) fn cancel_native() {}
    }

    impl ScanRequest {
        pub(crate) fn install_host(_: &dioxus::mobile::DesktopContext, _: RuntimeHandle) {}

        pub(crate) fn open_with_ticket(
            &self,
            _source: Entity,
            _ticket: ScanTicket,
        ) -> Result<(), String> {
            if !self.permitted() {
                return Err(translate("mobile-qr-camera-unavailable"));
            }
            Err(translate("mobile-qr-unsupported-platform"))
        }
    }
}
