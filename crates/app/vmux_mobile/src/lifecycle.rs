#[cfg(target_os = "ios")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::NSObjectProtocol;
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject};
    use objc2_ui_kit::{
        UIApplicationDidBecomeActiveNotification, UIApplicationDidEnterBackgroundNotification,
        UIApplicationWillEnterForegroundNotification,
    };

    use bevy_window::AppLifecycle;

    use crate::runtime::World;

    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::error!("world: the lifecycle observer must be installed on the main thread");
            return;
        };
        let observer = LifecycleObserver::new(mtm);
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
        tracing::info!("world: lifecycle observer installed");
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxWorldLifecycleObserver"]
        #[ivars = ()]
        struct LifecycleObserver;

        impl LifecycleObserver {
            #[unsafe(method(didEnterBackground:))]
            fn did_enter_background(&self, _notification: &NSNotification) {
                World::report(AppLifecycle::WillSuspend);
            }

            #[unsafe(method(willEnterForeground:))]
            fn will_enter_foreground(&self, _notification: &NSNotification) {
                World::report(AppLifecycle::WillResume);
            }

            #[unsafe(method(didBecomeActive:))]
            fn did_become_active(&self, _notification: &NSNotification) {
                World::report(AppLifecycle::Running);
            }
        }

        unsafe impl NSObjectProtocol for LifecycleObserver {}
    );

    impl LifecycleObserver {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(());
            unsafe { msg_send![super(this), init] }
        }
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    use bevy_window::AppLifecycle;

    use crate::runtime::World;

    pub fn install() {
        World::report(AppLifecycle::Running);
    }
}

pub use platform::*;
