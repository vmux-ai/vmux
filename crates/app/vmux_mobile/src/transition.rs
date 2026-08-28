use vmux_native::NativePage;

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct Level {
    pub page: &'static NativePage,
    pub title: String,
}

#[cfg(target_os = "ios")]
mod platform {
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    use dispatch2::{DispatchQueue, DispatchTime};
    use objc2::Message;
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_foundation::{NSObjectProtocol, NSString};
    use objc2_ui_kit::{
        UIAdaptivePresentationControllerDelegate, UIGestureRecognizer, UIGestureRecognizerDelegate,
        UIModalPresentationStyle, UINavigationController, UINavigationControllerDelegate,
        UIPresentationController, UISheetPresentationController,
        UISheetPresentationControllerDelegate, UISheetPresentationControllerDetent, UIView,
        UIViewAutoresizing, UIViewController,
    };
    use vmux_native::WebView;

    use super::Level;
    use crate::surface::Surfaces;

    thread_local! {
        static STACK: RefCell<Option<NativeStack>> = const { RefCell::new(None) };
        static POPPED: Cell<usize> = const { Cell::new(0) };
        static DISMISSED: Cell<usize> = const { Cell::new(0) };
    }

    struct Held {
        controller: Retained<UIViewController>,
        web: Option<WebView>,
    }

    impl Held {
        fn draw(level: &Level, root_view: &UIView, marker: MainThreadMarker) -> Option<Self> {
            let web = Surfaces::build(level.page)?;
            web.render();
            let view = web.ui_view();
            view.removeFromSuperview();
            size_to_parent(&view, root_view);
            let controller = UIViewController::initWithNibName_bundle(
                UIViewController::alloc(marker),
                None,
                None,
            );
            controller.setView(Some(&view));
            controller
                .navigationItem()
                .setTitle(Some(&NSString::from_str(&level.title)));
            Some(Self {
                controller,
                web: Some(web),
            })
        }
    }

    pub struct NativeStack {
        root_view: Retained<UIView>,
        navigation: Retained<UINavigationController>,
        levels: Vec<Held>,
        sheets: Vec<Held>,
        _delegate: Retained<NavDelegate>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxNavigationDelegate"]
        struct NavDelegate;

        impl NavDelegate {}

        unsafe impl NSObjectProtocol for NavDelegate {}

        unsafe impl UIGestureRecognizerDelegate for NavDelegate {
            #[unsafe(method(gestureRecognizerShouldBegin:))]
            fn should_begin(&self, _gesture: &UIGestureRecognizer) -> bool {
                STACK.with_borrow(|stack| stack.as_ref().is_some_and(|stack| stack.levels.len() > 1))
            }
        }

        unsafe impl UINavigationControllerDelegate for NavDelegate {
            #[unsafe(method(navigationController:didShowViewController:animated:))]
            fn did_show(
                &self,
                navigation: &UINavigationController,
                _shown: &UIViewController,
                _animated: bool,
            ) {
                let shown: usize = unsafe {
                    let controllers: Retained<objc2_foundation::NSArray<UIViewController>> =
                        msg_send![navigation, viewControllers];
                    controllers.count()
                };
                STACK.with_borrow_mut(|stack| {
                    let Some(stack) = stack.as_mut() else {
                        return;
                    };
                    if shown >= stack.levels.len() {
                        return;
                    }
                    let dropped = stack.levels.len() - shown;
                    stack.levels.truncate(shown);
                    POPPED.set(POPPED.get() + dropped);
                });
            }
        }

        unsafe impl UIAdaptivePresentationControllerDelegate for NavDelegate {
            #[unsafe(method(presentationControllerDidDismiss:))]
            fn did_dismiss(&self, _controller: &UIPresentationController) {
                STACK.with_borrow_mut(|stack| {
                    let Some(stack) = stack.as_mut() else {
                        return;
                    };
                    if stack.sheets.pop().is_none() {
                        return;
                    }
                    DISMISSED.set(DISMISSED.get() + 1);
                });
            }
        }

        unsafe impl UISheetPresentationControllerDelegate for NavDelegate {}
    );

    pub fn install(root_controller: &UIViewController, root_view: &UIView, web_view: &UIView) {
        if STACK.with_borrow(Option::is_some) {
            return;
        }
        let Some(marker) = MainThreadMarker::new() else {
            return;
        };
        let (root_controller, root_view, web_view) = (
            root_controller.retain(),
            root_view.retain(),
            web_view.retain(),
        );

        let first =
            UIViewController::initWithNibName_bundle(UIViewController::alloc(marker), None, None);
        web_view.removeFromSuperview();
        first.setView(Some(&*web_view));
        let navigation = UINavigationController::initWithRootViewController(
            UINavigationController::alloc(marker),
            &first,
        );
        let delegate = NavDelegate::new(marker);
        unsafe {
            navigation.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*delegate)));
        }

        navigation.setNavigationBarHidden(true);
        if let Some(gesture) = navigation.interactivePopGestureRecognizer() {
            gesture.setEnabled(true);
            gesture.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*delegate)));
        }

        root_controller.addChildViewController(&navigation);
        let Some(navigation_view) = navigation.view() else {
            return;
        };
        size_to_parent(&navigation_view, &root_view);
        root_view.addSubview(&navigation_view);
        navigation.didMoveToParentViewController(Some(&root_controller));

        STACK.set(Some(NativeStack {
            root_view,
            navigation,
            levels: vec![Held {
                controller: first,
                web: None,
            }],
            sheets: Vec::new(),
            _delegate: delegate,
        }));
    }

    impl NavDelegate {
        fn new(marker: MainThreadMarker) -> Retained<Self> {
            unsafe { objc2::msg_send![Self::alloc(marker), init] }
        }
    }

    impl NativeStack {
        pub fn push(level: Level) {
            let drawn = Self::draw(level);
            after_paint(move || {
                let Some(drawn) = drawn else {
                    return;
                };
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let controller = drawn.controller.clone();
                    stack.levels.push(drawn);
                    Some((stack.navigation.clone(), controller))
                });
                let Some((navigation, top)) = pending else {
                    return;
                };
                navigation.pushViewController_animated(&top, true);
            });
        }

        pub fn pop() {
            let navigation = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                if stack.levels.len() < 2 {
                    return None;
                }
                Some(stack.navigation.clone())
            });
            let Some(navigation) = navigation else {
                return;
            };
            let _ = navigation.popViewControllerAnimated(true);
        }

        pub fn present(level: Level) {
            let drawn = Self::draw(level);
            after_paint(move || {
                let Some(drawn) = drawn else {
                    return;
                };
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let marker = MainThreadMarker::new()?;
                    let presenter = stack.presenter();
                    let sheet = drawn.controller.clone();
                    sheet.setModalPresentationStyle(UIModalPresentationStyle::PageSheet);
                    if let Some(controller) = sheet.sheetPresentationController() {
                        unsafe {
                            controller.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(
                                &*stack._delegate,
                            )));
                        }
                        Self::detents(&controller, marker);
                    }
                    stack.sheets.push(drawn);
                    Some((presenter, sheet))
                });
                let Some((presenter, sheet)) = pending else {
                    return;
                };
                presenter.presentViewController_animated_completion(&sheet, true, None);
            });
        }

        pub fn dismiss() {
            let departing = STACK.with_borrow_mut(|stack| stack.as_mut()?.sheets.pop());
            let Some(departing) = departing else {
                return;
            };
            departing
                .controller
                .dismissViewControllerAnimated_completion(true, None);
        }

        pub fn settle(levels: Vec<Level>) {
            let drawn: Vec<Held> = levels.into_iter().filter_map(Self::draw).collect();
            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                for sheet in std::mem::take(&mut stack.sheets) {
                    sheet
                        .controller
                        .dismissViewControllerAnimated_completion(false, None);
                }
                stack.levels.truncate(1);
                stack.levels.extend(drawn);
                let controllers = objc2_foundation::NSArray::from_retained_slice(
                    &stack
                        .levels
                        .iter()
                        .map(|level| level.controller.clone())
                        .collect::<Vec<_>>(),
                );
                stack
                    .navigation
                    .setViewControllers_animated(&controllers, false);
            });
        }

        pub fn render() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                for level in stack.levels.iter().chain(stack.sheets.iter()) {
                    let Some(web) = level.web.as_ref() else {
                        continue;
                    };
                    web.render();
                }
            });
        }

        fn draw(level: Level) -> Option<Held> {
            let (root_view, marker) = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                Some((stack.root_view.clone(), MainThreadMarker::new()?))
            })?;
            Held::draw(&level, &root_view, marker)
        }

        fn presenter(&self) -> Retained<UIViewController> {
            match self.sheets.last() {
                Some(sheet) => sheet.controller.clone(),
                None => Retained::into_super(self.navigation.clone()),
            }
        }

        fn detents(controller: &UISheetPresentationController, marker: MainThreadMarker) {
            let medium = UISheetPresentationControllerDetent::mediumDetent(marker);
            let large = UISheetPresentationControllerDetent::largeDetent(marker);
            controller.setDetents(&objc2_foundation::NSArray::from_retained_slice(&[
                medium, large,
            ]));
            controller.setPrefersGrabberVisible(true);
        }
    }

    pub fn take_popped() -> usize {
        POPPED.replace(0)
    }

    pub fn take_dismissed() -> usize {
        DISMISSED.replace(0)
    }

    fn size_to_parent(view: &UIView, parent: &UIView) {
        view.setFrame(parent.bounds());
        view.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
    }

    fn after_paint<F: FnOnce() + 'static>(work: F) {
        let Ok(when) = DispatchTime::try_from(Duration::from_millis(48)) else {
            return;
        };
        let work = OnMain(work);
        let _ = DispatchQueue::main().after(when, move || work.run());
    }

    struct OnMain<F>(F);

    unsafe impl<F> Send for OnMain<F> {}

    impl<F: FnOnce()> OnMain<F> {
        fn run(self) {
            (self.0)();
        }
    }
}

#[cfg(not(target_os = "ios"))]
#[allow(dead_code)]
mod platform {
    use super::Level;

    pub struct NativeStack;

    pub fn install(_: &(), _: &(), _: &()) {}

    pub fn take_popped() -> usize {
        0
    }

    pub fn take_dismissed() -> usize {
        0
    }

    impl NativeStack {
        pub fn push(_level: Level) {}

        pub fn pop() {}

        pub fn present(_level: Level) {}

        pub fn dismiss() {}

        pub fn settle(_levels: Vec<Level>) {}

        pub fn render() {}
    }
}

#[cfg(target_os = "ios")]
pub use platform::install;
pub use platform::{NativeStack, take_dismissed, take_popped};
