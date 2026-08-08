//! Opening a session as a native sheet rather than a page swap.
//!
//! Dioxus has no notion of native modal presentation, so this is UIKit directly: the app's one
//! webview is reparented into a `UIViewController` presented as a page sheet. The webview never
//! changes — only which controller owns it — so the sheet animates over the list with the grabber,
//! corner radius and swipe-down a native screen would have, while the content is still the same
//! Dioxus tree.
//!
//! Closing is the awkward half. Handing the webview back to the root controller repaints it as the
//! list, and dismissing before that paint lands shows a frame of the wrong screen. So the sheet
//! keeps a snapshot of what it looked like, and the real dismissal waits two animation frames.
//!
//! So [`NativeSheet::close`] hands back a guard the caller finishes after switching the page,
//! where [`NativeSheet::open`] needs no such step: the sheet rises over content that is still
//! valid until it covers it.

#[cfg(target_os = "ios")]
mod platform {
    use std::cell::RefCell;

    use dioxus::mobile::tao::platform::ios::WindowExtIOS;
    use dioxus::mobile::wry::WebViewExtIOS;
    use dioxus::prelude::*;
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_ui_kit::{
        UIModalPresentationStyle, UIModalTransitionStyle, UIView, UIViewAutoresizing,
        UIViewController,
    };

    thread_local! {
        static SHEET: RefCell<Option<NativeSheet>> = const { RefCell::new(None) };
    }

    /// The views a sheet moves between, and the modal currently holding them.
    pub struct NativeSheet {
        root_controller: Retained<UIViewController>,
        root_view: Retained<UIView>,
        web_view: Retained<UIView>,
        presented: Option<Retained<UIViewController>>,
    }

    /// Adopt the window's views. Everything else is a no-op until this has run.
    pub fn install(window: &dioxus::mobile::DesktopContext) {
        let controller: *mut UIViewController = window.window.ui_view_controller().cast();
        let root: *mut UIView = window.window.ui_view().cast();
        let webview = window.webview.webview();
        let web: &UIView = &webview;

        // The only unsafe here: adopting three pointers UIKit owns for the app's whole life. Held
        // as strong references afterwards, so nothing downstream has to reason about them.
        let adopted = unsafe {
            (
                Retained::retain(controller),
                Retained::retain(root),
                Retained::retain(ptr_to_mut(web)),
            )
        };
        let (Some(root_controller), Some(root_view), Some(web_view)) = adopted else {
            return;
        };
        SHEET.set(Some(NativeSheet {
            root_controller,
            root_view,
            web_view,
            presented: None,
        }));
    }

    fn ptr_to_mut(view: &UIView) -> *mut UIView {
        std::ptr::from_ref(view).cast_mut()
    }

    impl NativeSheet {
        /// Present the webview as a sheet. Does nothing if one is already up.
        pub fn open() {
            SHEET.with_borrow_mut(|sheet| {
                let Some(sheet) = sheet.as_mut() else {
                    return;
                };
                if sheet.presented.is_some() {
                    return;
                }
                let Some(modal) = sheet.modal_controller() else {
                    return;
                };
                sheet.web_view.removeFromSuperview();
                size_to_parent(&sheet.web_view, &sheet.root_view);
                configure_sheet(&modal);
                sheet
                    .root_controller
                    .presentViewController_animated_completion(&modal, true, None);
                sheet.presented = Some(modal);
            });
        }

        /// Hand the webview back to the root controller, leaving the sheet showing a snapshot.
        ///
        /// The dismissal itself is deferred to the guard: the list has to paint first.
        pub fn close() -> Dismissing {
            Dismissing(SHEET.with_borrow_mut(|sheet| {
                let sheet = sheet.as_mut()?;
                let modal = sheet.presented.take()?;
                let snapshot = sheet.web_view.snapshotViewAfterScreenUpdates(false);
                sheet.web_view.removeFromSuperview();
                if let Some(snapshot) = snapshot {
                    modal.setView(Some(&snapshot));
                }
                size_to_parent(&sheet.web_view, &sheet.root_view);
                sheet.root_view.addSubview(&sheet.web_view);
                Some(modal)
            }))
        }

        fn modal_controller(&self) -> Option<Retained<UIViewController>> {
            let marker = MainThreadMarker::new()?;
            let controller = UIViewController::initWithNibName_bundle(
                UIViewController::alloc(marker),
                None,
                None,
            );
            controller.setView(Some(&*self.web_view));
            Some(controller)
        }
    }

    /// A sheet showing a snapshot, waiting for the list underneath to paint.
    pub struct Dismissing(Option<Retained<UIViewController>>);

    impl Dismissing {
        pub fn finish(self) {
            let Some(modal) = self.0 else {
                return;
            };
            spawn(async move {
                wait_for_paint().await;
                modal.dismissViewControllerAnimated_completion(true, None);
            });
        }
    }

    fn configure_sheet(controller: &UIViewController) {
        controller.setModalPresentationStyle(UIModalPresentationStyle::PageSheet);
        controller.setModalTransitionStyle(UIModalTransitionStyle::CoverVertical);
        controller.setModalInPresentation(true);
        if let Some(sheet) = controller.sheetPresentationController() {
            sheet.setPrefersGrabberVisible(true);
            sheet.setPreferredCornerRadius(24.0);
            sheet.setPrefersEdgeAttachedInCompactHeight(true);
        }
    }

    fn size_to_parent(view: &UIView, parent: &UIView) {
        view.setFrame(parent.bounds());
        view.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
    }

    /// Roughly three frames at 60Hz, to let the list behind the sheet draw before it descends.
    ///
    /// A guess, and knowingly so. The thing being waited for is a WebKit paint, which happens in
    /// another process — `CATransaction` and a main-queue hop both return before it, so neither
    /// answers the question. The webview itself could (a double `requestAnimationFrame` is exact)
    /// but that means evaluating script from Rust, which this app does not do.
    ///
    /// Being early costs one frame of the session showing behind the descending snapshot. Being
    /// late costs nothing visible, so the value errs long.
    async fn wait_for_paint() {
        vmux_chat::platform::sleep_ms(48).await;
    }
}

/// Everywhere else a session is just a page swap, so the sheet is inert.
#[cfg(not(target_os = "ios"))]
mod platform {
    pub struct NativeSheet;
    pub struct Dismissing;

    pub fn install(_: &dioxus::mobile::DesktopContext) {}

    impl NativeSheet {
        pub fn open() {}

        pub fn close() -> Dismissing {
            Dismissing
        }
    }

    impl Dismissing {
        pub fn finish(self) {}
    }
}

pub use platform::{NativeSheet, install};
