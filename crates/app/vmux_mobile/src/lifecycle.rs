#[cfg(target_os = "ios")]
mod platform {
    use bevy_window::AppLifecycle;
    use objc2::rc::Retained;
    use objc2::runtime::NSObjectProtocol;
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject};
    use objc2_ui_kit::{
        UIApplicationDidBecomeActiveNotification, UIApplicationDidEnterBackgroundNotification,
        UIApplicationWillEnterForegroundNotification,
    };

    use crate::runtime::RuntimeHandle;

    pub(crate) struct LifecycleGuard {
        observer: Option<Retained<LifecycleObserver>>,
    }

    impl Drop for LifecycleGuard {
        fn drop(&mut self) {
            let Some(observer) = self.observer.take() else {
                return;
            };
            unsafe {
                NSNotificationCenter::defaultCenter().removeObserver(&observer);
            }
        }
    }

    impl RuntimeHandle {
        pub(crate) fn install_lifecycle(&self) -> LifecycleGuard {
            let Some(mtm) = MainThreadMarker::new() else {
                tracing::error!(
                    "runtime: the lifecycle observer must be installed on the main thread"
                );
                return LifecycleGuard { observer: None };
            };
            let observer = LifecycleObserver::new(mtm, self.clone());
            let center = NSNotificationCenter::defaultCenter();
            unsafe {
                center.addObserver_selector_name_object(
                    &observer,
                    sel!(didEnterBackground:),
                    Some(UIApplicationDidEnterBackgroundNotification),
                    None,
                );
                center.addObserver_selector_name_object(
                    &observer,
                    sel!(willEnterForeground:),
                    Some(UIApplicationWillEnterForegroundNotification),
                    None,
                );
                center.addObserver_selector_name_object(
                    &observer,
                    sel!(didBecomeActive:),
                    Some(UIApplicationDidBecomeActiveNotification),
                    None,
                );
            }
            tracing::info!("runtime: lifecycle observer installed");
            LifecycleGuard {
                observer: Some(observer),
            }
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxLifecycleObserver"]
        #[ivars = RuntimeHandle]
        struct LifecycleObserver;

        impl LifecycleObserver {
            #[unsafe(method(didEnterBackground:))]
            fn did_enter_background(&self, _notification: &NSNotification) {
                self.ivars().report_lifecycle(AppLifecycle::WillSuspend);
            }

            #[unsafe(method(willEnterForeground:))]
            fn will_enter_foreground(&self, _notification: &NSNotification) {
                self.ivars().report_lifecycle(AppLifecycle::WillResume);
            }

            #[unsafe(method(didBecomeActive:))]
            fn did_become_active(&self, _notification: &NSNotification) {
                self.ivars().report_lifecycle(AppLifecycle::Running);
            }
        }

        unsafe impl NSObjectProtocol for LifecycleObserver {}
    );

    impl LifecycleObserver {
        fn new(mtm: MainThreadMarker, runtime: RuntimeHandle) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(runtime);
            unsafe { msg_send![super(this), init] }
        }
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    use bevy_window::AppLifecycle;

    use crate::runtime::RuntimeHandle;

    pub(crate) struct UnsupportedLifecycleGuard;

    impl RuntimeHandle {
        pub(crate) fn install_lifecycle(&self) -> UnsupportedLifecycleGuard {
            self.report_lifecycle(AppLifecycle::Running);
            UnsupportedLifecycleGuard
        }
    }
}
