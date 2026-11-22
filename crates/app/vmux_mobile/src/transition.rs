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

    use crate::session::{LeaveSession, OpenSession};

    struct SheetHost {
        root_controller: Retained<UIViewController>,
        root_view: Retained<UIView>,
        web_view: Retained<UIView>,
        presented: Option<Retained<UIViewController>>,
    }

    impl SheetHost {
        fn with_mut<R>(f: impl FnOnce(&mut Option<Self>) -> R) -> R {
            thread_local! {
                static SHEET: RefCell<Option<SheetHost>> = const { RefCell::new(None) };
            }
            SHEET.with_borrow_mut(f)
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
    }

    impl OpenSession {
        pub(crate) fn install_sheet(window: &dioxus::mobile::DesktopContext) {
            let controller: *mut UIViewController = window.window.ui_view_controller().cast();
            let root: *mut UIView = window.window.ui_view().cast();
            let webview = window.webview.webview();
            let web: &UIView = &webview;
            let adopted = unsafe {
                (
                    Retained::retain(controller),
                    Retained::retain(root),
                    Retained::retain(std::ptr::from_ref(web).cast_mut()),
                )
            };
            let (Some(root_controller), Some(root_view), Some(web_view)) = adopted else {
                return;
            };
            SheetHost::with_mut(|slot| {
                *slot = Some(SheetHost {
                    root_controller,
                    root_view,
                    web_view,
                    presented: None,
                });
            });
        }

        pub(crate) fn open_sheet(&self) {
            SheetHost::with_mut(|sheet| {
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
                SheetHost::size_to_parent(&sheet.web_view, &sheet.root_view);
                SheetHost::configure_sheet(&modal);
                sheet
                    .root_controller
                    .presentViewController_animated_completion(&modal, true, None);
                sheet.presented = Some(modal);
            });
        }
    }

    impl LeaveSession {
        pub(crate) fn close_sheet(&self) {
            let modal = SheetHost::with_mut(|sheet| {
                let sheet = sheet.as_mut()?;
                let modal = sheet.presented.take()?;
                let snapshot = sheet.web_view.snapshotViewAfterScreenUpdates(false);
                sheet.web_view.removeFromSuperview();
                if let Some(snapshot) = snapshot {
                    modal.setView(Some(&snapshot));
                }
                SheetHost::size_to_parent(&sheet.web_view, &sheet.root_view);
                sheet.root_view.addSubview(&sheet.web_view);
                Some(modal)
            });
            if let Some(modal) = modal {
                spawn(async move {
                    vmux_ui::platform::Platform::sleep(48).await;
                    modal.dismissViewControllerAnimated_completion(true, None);
                });
            }
        }
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    use crate::session::{LeaveSession, OpenSession};

    impl OpenSession {
        pub(crate) fn install_sheet(_: &dioxus::mobile::DesktopContext) {}

        pub(crate) fn open_sheet(&self) {}
    }

    impl LeaveSession {
        pub(crate) fn close_sheet(&self) {}
    }
}
