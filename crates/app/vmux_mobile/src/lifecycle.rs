#[cfg(target_os = "ios")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::NSObjectProtocol;
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject};
    use objc2_ui_kit::{
        UIApplicationDidBecomeActiveNotification, UIApplicationDidEnterBackgroundNotification,
        UIApplicationWillEnterForegroundNotification,
    };

    use bevy_window::AppLifecycle;

    use crate::runtime::{RuntimeHandle, report_lifecycle};

    pub fn install(runtime: RuntimeHandle) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::error!("runtime: the lifecycle observer must be installed on the main thread");
            return;
        };
        let observer = LifecycleObserver::new(mtm, runtime);
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
        std::mem::forget(observer);
        tracing::info!("runtime: lifecycle observer installed");
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
                report_lifecycle(self.ivars(), AppLifecycle::WillSuspend);
            }

            #[unsafe(method(willEnterForeground:))]
            fn will_enter_foreground(&self, _notification: &NSNotification) {
                report_lifecycle(self.ivars(), AppLifecycle::WillResume);
            }

            #[unsafe(method(didBecomeActive:))]
            fn did_become_active(&self, _notification: &NSNotification) {
                report_lifecycle(self.ivars(), AppLifecycle::Running);
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

    use crate::runtime::{RuntimeHandle, report_lifecycle};

    pub fn install(runtime: RuntimeHandle) {
        report_lifecycle(&runtime, AppLifecycle::Running);
    }
}

pub use platform::*;
