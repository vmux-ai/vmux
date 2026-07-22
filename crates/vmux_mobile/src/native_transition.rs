#[cfg(target_os = "ios")]
mod platform {
    use std::cell::{Cell, RefCell};
    use std::ptr;

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
        static ROOT_CONTROLLER: Cell<*mut UIViewController> = const { Cell::new(ptr::null_mut()) };
        static ROOT_VIEW: Cell<*mut UIView> = const { Cell::new(ptr::null_mut()) };
        static WEB_VIEW: Cell<*mut UIView> = const { Cell::new(ptr::null_mut()) };
        static ACTIVE_MODAL: RefCell<Option<Retained<UIViewController>>> = const { RefCell::new(None) };
    }

    pub struct OpenTransition;

    pub struct CloseTransition {
        modal: Retained<UIViewController>,
    }

    pub fn install(window: &dioxus::mobile::DesktopContext) {
        ROOT_CONTROLLER.set(window.window.ui_view_controller().cast());
        ROOT_VIEW.set(window.window.ui_view().cast());
        let webview = window.webview.webview();
        let view: &UIView = &webview;
        WEB_VIEW.set(ptr::from_ref(view).cast_mut());
    }

    pub fn begin_open() -> Option<OpenTransition> {
        if ACTIVE_MODAL.with(|active| active.borrow().is_some()) {
            return None;
        }
        let root_controller = root_controller()?;
        let root_view = root_view()?;
        let web_view = web_view()?;
        let modal = modal_controller(&web_view)?;
        web_view.removeFromSuperview();
        size_to_parent(&web_view, &root_view);
        configure_sheet(&modal);
        root_controller.presentViewController_animated_completion(&modal, true, None);
        ACTIVE_MODAL.with(|active| *active.borrow_mut() = Some(modal));
        Some(OpenTransition)
    }

    pub fn finish_open(_: Option<OpenTransition>) {}

    pub fn begin_close() -> Option<CloseTransition> {
        let modal = ACTIVE_MODAL.with(|active| active.borrow_mut().take())?;
        let root_view = root_view()?;
        let web_view = web_view()?;
        let snapshot = web_view.snapshotViewAfterScreenUpdates(false);
        web_view.removeFromSuperview();
        if let Some(snapshot) = snapshot {
            modal.setView(Some(&snapshot));
        }
        size_to_parent(&web_view, &root_view);
        root_view.addSubview(&web_view);
        Some(CloseTransition { modal })
    }

    pub fn finish_close(transition: Option<CloseTransition>) {
        let Some(transition) = transition else {
            return;
        };
        spawn(async move {
            wait_for_paint().await;
            transition
                .modal
                .dismissViewControllerAnimated_completion(true, None);
        });
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

    fn root_controller() -> Option<Retained<UIViewController>> {
        ROOT_CONTROLLER.with(|pointer| unsafe { Retained::retain(pointer.get()) })
    }

    fn root_view() -> Option<Retained<UIView>> {
        ROOT_VIEW.with(|pointer| unsafe { Retained::retain(pointer.get()) })
    }

    fn web_view() -> Option<Retained<UIView>> {
        WEB_VIEW.with(|pointer| unsafe { Retained::retain(pointer.get()) })
    }

    fn modal_controller(view: &UIView) -> Option<Retained<UIViewController>> {
        let marker = MainThreadMarker::new()?;
        let controller =
            UIViewController::initWithNibName_bundle(UIViewController::alloc(marker), None, None);
        controller.setView(Some(view));
        Some(controller)
    }

    async fn wait_for_paint() {
        let mut evaluator = document::eval(
            "requestAnimationFrame(() => requestAnimationFrame(() => dioxus.send(true)));",
        );
        let _ = evaluator.recv::<bool>().await;
    }
}

#[cfg(not(target_os = "ios"))]
mod platform {
    pub struct OpenTransition;
    pub struct CloseTransition;

    pub fn install(_: &dioxus::mobile::DesktopContext) {}

    pub fn begin_open() -> Option<OpenTransition> {
        None
    }

    pub fn finish_open(_: Option<OpenTransition>) {}

    pub fn begin_close() -> Option<CloseTransition> {
        None
    }

    pub fn finish_close(_: Option<CloseTransition>) {}
}

pub use platform::*;
