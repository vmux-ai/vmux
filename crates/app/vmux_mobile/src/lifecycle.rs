//! Telling the world when iOS puts the app away.
//!
//! On the desktop this arrives from winit, which owns the application object and turns its
//! callbacks into `AppLifecycle`. Here tao owns it, and what tao reports is too coarse to park a
//! world on: it maps `applicationWillResignActive` to `Suspended`, which also fires for a
//! pulled-down notification shade or an incoming call. So the states come from `UIApplication`'s
//! own notifications instead, picked to mean what Bevy means.

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

    /// Start reporting the app's lifecycle.
    ///
    /// The observer is leaked on purpose: it lives as long as the process, and there is nowhere to
    /// hold it that a `LaunchBuilder` which never returns would not outlive anyway.
    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::error!("world: the lifecycle observer must be installed on the main thread");
            return;
        };
        let observer = LifecycleObserver::new(mtm);
        let center = NSNotificationCenter::defaultCenter();
        // Safety: the names are UIKit's own statics, and the observer outlives the process.
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
            /// The app is going away. Owed one frame, then the world parks.
            ///
            /// `UIApplicationWillResignActive` is deliberately not observed: it also fires for a
            /// pulled-down notification shade or an incoming call, where the app is still on
            /// screen and stopping the world would strand whatever the user was looking at.
            #[unsafe(method(didEnterBackground:))]
            fn did_enter_background(&self, _notification: &NSNotification) {
                World::report(AppLifecycle::WillSuspend);
            }

            /// Coming back. Bevy gives this a state of its own before `Running`, so a plugin can
            /// re-establish whatever it let go of on the way down.
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

/// Off the phone nothing suspends the app, so it is running and stays running.
///
/// Said once rather than not at all: a world that waits for its first lifecycle before running
/// anything would otherwise wait forever on a platform with no `UIApplication` to ask.
#[cfg(not(target_os = "ios"))]
mod platform {
    use bevy_window::AppLifecycle;

    use crate::runtime::World;

    pub fn install() {
        World::report(AppLifecycle::Running);
    }
}

pub use platform::*;
