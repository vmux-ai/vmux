#[cfg(target_os = "ios")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, ProtocolObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_foundation::{
        NSDictionary, NSNotification, NSNotificationCenter, NSObjectProtocol, NSURL,
    };
    #[allow(deprecated)]
    use objc2_ui_kit::UIApplicationLaunchOptionsURLKey;
    use objc2_ui_kit::{
        UIApplication, UIApplicationDelegate, UIApplicationDidFinishLaunchingNotification,
    };

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxAppDelegate"]
        struct LinkDelegate;

        impl LinkDelegate {
            #[allow(deprecated)]
            #[unsafe(method(didFinishLaunching:))]
            fn did_finish_launching(&self, notification: &NSNotification) {
                let Some(info) = notification.userInfo() else {
                    return;
                };
                let key = unsafe { UIApplicationLaunchOptionsURLKey };
                let Some(url) = info.objectForKey(key) else {
                    return;
                };
                let url: Retained<NSURL> = unsafe { Retained::cast_unchecked(url) };
                offer(&url);
            }
        }

        unsafe impl NSObjectProtocol for LinkDelegate {}

        unsafe impl UIApplicationDelegate for LinkDelegate {
            #[unsafe(method(application:openURL:options:))]
            fn open_url(
                &self,
                _application: &UIApplication,
                url: &NSURL,
                _options: &NSDictionary,
            ) -> bool {
                offer(url)
            }
        }
    );

    impl LinkDelegate {
        fn new(marker: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![Self::alloc(marker), init] }
        }
    }

    fn offer(url: &NSURL) -> bool {
        let Some(text) = url.absoluteString() else {
            return false;
        };
        let text = text.to_string();
        let Ok(parsed) = url::Url::parse(&text) else {
            return false;
        };
        if parsed.scheme() != "vmux" || parsed.host_str() != Some("pair") {
            return false;
        }
        crate::offer_opened_url(text);
        true
    }

    thread_local! {
        static DELEGATE: std::cell::RefCell<Option<Retained<LinkDelegate>>> =
            const { std::cell::RefCell::new(None) };
    }

    pub fn install() {
        let Some(marker) = MainThreadMarker::new() else {
            tracing::error!("deep link: the observer must be installed on the main thread");
            return;
        };
        let delegate = LinkDelegate::new(marker);
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            center.addObserver_selector_name_object(
                &delegate,
                objc2::sel!(didFinishLaunching:),
                Some(UIApplicationDidFinishLaunchingNotification),
                None,
            );
        }
        DELEGATE.with_borrow_mut(|slot| *slot = Some(delegate));
    }

    pub fn adopt() {
        let Some(marker) = MainThreadMarker::new() else {
            return;
        };
        DELEGATE.with_borrow(|slot| {
            let Some(delegate) = slot.as_ref() else {
                tracing::error!("deep link: adopt ran before install");
                return;
            };
            let application = UIApplication::sharedApplication(marker);
            unsafe { application.setDelegate(Some(ProtocolObject::from_ref(&**delegate))) };
        });
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    #![allow(dead_code)]

    pub fn install() {}
    pub fn adopt() {}
}

#[cfg(target_os = "ios")]
pub(crate) use platform::adopt;
pub(crate) use platform::install;
