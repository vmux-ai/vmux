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

    thread_local! {
        static STACK: RefCell<Option<NativeStack>> = const { RefCell::new(None) };
        static POPPED: Cell<usize> = const { Cell::new(0) };
        static DISMISSED: Cell<usize> = const { Cell::new(0) };
    }

    pub struct NativeStack {
        root_view: Retained<UIView>,
        web_view: Retained<UIView>,
        navigation: Retained<UINavigationController>,
        levels: Vec<Retained<UIViewController>>,
        sheets: Vec<Retained<UIViewController>>,
        covered: Option<Retained<UIView>>,
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
                    stack.uncover();
                    stack.occupy_top();
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
                    stack.uncover();
                    stack.occupy_top();
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
            covered: None,
            root_view,
            web_view,
            navigation,
            levels: vec![first],
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
        pub fn push() -> Pushing {
            Pushing(STACK.with_borrow_mut(|stack| stack.as_mut().is_some_and(NativeStack::cover)))
        }

        pub fn pop() -> Popping {
            Popping(STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return false;
                };
                if stack.levels.len() < 2 {
                    return false;
                }
                stack.cover()
            }))
        }

        pub fn present() -> Presenting {
            Presenting(
                STACK.with_borrow_mut(|stack| stack.as_mut().is_some_and(NativeStack::cover)),
            )
        }

        pub fn dismiss() -> Dismissing {
            Dismissing(STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return false;
                };
                if stack.sheets.is_empty() {
                    return false;
                }
                stack.cover()
            }))
        }

        fn cover(&mut self) -> bool {
            self.uncover();
            let Some(snapshot) = self.web_view.snapshotViewAfterScreenUpdates(false) else {
                return false;
            };
            size_to_parent(&snapshot, &self.root_view);
            self.web_view.addSubview(&snapshot);
            self.covered = Some(snapshot);
            true
        }

        fn uncover(&mut self) -> Option<Retained<UIView>> {
            let snapshot = self.covered.take()?;
            snapshot.removeFromSuperview();
            Some(snapshot)
        }

        fn occupy_top(&mut self) {
            let Some(top) = self.sheets.last().or_else(|| self.levels.last()) else {
                return;
            };
            size_to_parent(&self.web_view, &self.root_view);
            top.setView(Some(&*self.web_view));
        }

        fn presenter(&self) -> Retained<UIViewController> {
            match self.sheets.last() {
                Some(sheet) => sheet.clone(),
                None => Retained::into_super(self.navigation.clone()),
            }
        }
    }

    pub struct Pushing(bool);

    impl Pushing {
        pub fn finish(self, title: String) {
            if !self.0 {
                return;
            }
            after_paint(move || {
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let marker = MainThreadMarker::new()?;
                    let cover = stack.uncover()?;
                    stack.levels.last()?.setView(Some(&cover));

                    let next = UIViewController::initWithNibName_bundle(
                        UIViewController::alloc(marker),
                        None,
                        None,
                    );
                    next.navigationItem()
                        .setTitle(Some(&NSString::from_str(&title)));
                    stack.levels.push(next);
                    stack.occupy_top();
                    Some((stack.navigation.clone(), stack.levels.last()?.clone()))
                });
                let Some((navigation, top)) = pending else {
                    return;
                };
                navigation.pushViewController_animated(&top, true);
            });
        }
    }

    pub struct Popping(bool);

    impl Popping {
        pub fn finish(self) {
            if !self.0 {
                return;
            }
            after_paint(move || {
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let cover = stack.uncover()?;
                    let departing = stack.levels.pop()?;
                    departing.setView(Some(&cover));
                    stack.occupy_top();
                    Some(stack.navigation.clone())
                });
                let Some(navigation) = pending else {
                    return;
                };
                let _ = navigation.popViewControllerAnimated(true);
            });
        }
    }

    pub struct Presenting(bool);

    impl Presenting {
        pub fn finish(self, title: String) {
            if !self.0 {
                return;
            }
            after_paint(move || {
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let marker = MainThreadMarker::new()?;
                    let cover = stack.uncover()?;
                    let presenter = stack.presenter();
                    presenter.setView(Some(&cover));

                    let sheet = UIViewController::initWithNibName_bundle(
                        UIViewController::alloc(marker),
                        None,
                        None,
                    );
                    sheet
                        .navigationItem()
                        .setTitle(Some(&NSString::from_str(&title)));
                    sheet.setModalPresentationStyle(UIModalPresentationStyle::PageSheet);
                    if let Some(controller) = sheet.sheetPresentationController() {
                        unsafe {
                            controller.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(
                                &*stack._delegate,
                            )));
                        }
                        Self::detents(&controller, marker);
                    }
                    stack.sheets.push(sheet);
                    stack.occupy_top();
                    Some((presenter, stack.sheets.last()?.clone()))
                });
                let Some((presenter, sheet)) = pending else {
                    return;
                };
                presenter.presentViewController_animated_completion(&sheet, true, None);
            });
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

    pub struct Dismissing(bool);

    impl Dismissing {
        pub fn finish(self) {
            if !self.0 {
                return;
            }
            after_paint(move || {
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let cover = stack.uncover()?;
                    let departing = stack.sheets.pop()?;
                    departing.setView(Some(&cover));
                    stack.occupy_top();
                    Some(departing)
                });
                let Some(departing) = pending else {
                    return;
                };
                departing.dismissViewControllerAnimated_completion(true, None);
            });
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

    fn after_paint<F: Send + FnOnce() + 'static>(work: F) {
        let Ok(when) = DispatchTime::try_from(Duration::from_millis(48)) else {
            return;
        };
        let _ = DispatchQueue::main().after(when, work);
    }
}

#[cfg(not(target_os = "ios"))]
#[allow(dead_code)]
mod platform {

    pub struct NativeStack;
    pub struct Pushing;
    pub struct Popping;
    pub struct Presenting;
    pub struct Dismissing;

    pub fn install(_: &(), _: &(), _: &()) {}

    pub fn take_popped() -> usize {
        0
    }

    pub fn take_dismissed() -> usize {
        0
    }

    impl NativeStack {
        pub fn push() -> Pushing {
            Pushing
        }

        pub fn pop() -> Popping {
            Popping
        }

        pub fn present() -> Presenting {
            Presenting
        }

        pub fn dismiss() -> Dismissing {
            Dismissing
        }
    }

    impl Pushing {
        pub fn finish(self, _title: String) {}
    }

    impl Popping {
        pub fn finish(self) {}
    }

    impl Presenting {
        pub fn finish(self, _title: String) {}
    }

    impl Dismissing {
        pub fn finish(self) {}
    }
}

#[cfg(target_os = "ios")]
pub use platform::install;
pub use platform::{NativeStack, take_dismissed, take_popped};
