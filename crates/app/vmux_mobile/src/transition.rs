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

    pub struct NativeSheet {
        root_controller: Retained<UIViewController>,
        root_view: Retained<UIView>,
        web_view: Retained<UIView>,
        presented: Option<Retained<UIViewController>>,
    }

    pub fn install(window: &dioxus::mobile::DesktopContext) {
        let controller: *mut UIViewController = window.window.ui_view_controller().cast();
        let root: *mut UIView = window.window.ui_view().cast();
        let webview = window.webview.webview();
        let web: &UIView = &webview;

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

    async fn wait_for_paint() {
        vmux_ui::platform::sleep_ms(48).await;
    }
}

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
