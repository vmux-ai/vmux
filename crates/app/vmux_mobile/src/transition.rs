use vmux_native::NativePage;

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct Level {
    pub page: &'static NativePage,
    pub title: String,
    pub action: Option<&'static str>,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct TabEntry {
    pub id: String,
    pub name: String,
    pub here: bool,
}

#[cfg(target_os = "ios")]
mod platform {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::time::Duration;

    use dispatch2::{DispatchQueue, DispatchTime};
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{
        ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel,
    };
    use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
    use objc2_foundation::{NSObjectProtocol, NSString};
    use objc2_ui_kit::{
        UIAdaptivePresentationControllerDelegate, UIBarButtonItem, UIBarButtonItemStyle, UIButton,
        UIButtonType, UIColor, UIControlEvents, UIControlState, UIEdgeInsets, UIFont,
        UIGestureRecognizer, UIGestureRecognizerDelegate, UIGestureRecognizerState,
        UIGlassContainerEffect, UIGlassEffect, UILayoutConstraintAxis, UIModalPresentationStyle,
        UINavigationBarAppearance, UINavigationController, UINavigationControllerDelegate,
        UIPanGestureRecognizer, UIPresentationController, UISheetPresentationController,
        UISheetPresentationControllerDelegate, UISheetPresentationControllerDetent, UIStackView,
        UIStackViewDistribution, UIUserInterfaceStyle, UIView, UIViewAutoresizing,
        UIViewController, UIVisualEffectView,
    };
    use vmux_native::WebView;

    use super::{Level, TabEntry};
    use crate::surface::Surfaces;

    const TAB_BAR_HEIGHT: f64 = 56.0;
    const TAB_BAR_EDGE: f64 = 16.0;
    const TAB_BAR_GAP: f64 = 10.0;

    thread_local! {
        static STACK: RefCell<Option<NativeStack>> = const { RefCell::new(None) };
        static POPPED: Cell<usize> = const { Cell::new(0) };
        static DISMISSED: Cell<usize> = const { Cell::new(0) };
        static TAPPED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        static PICKED: RefCell<Option<String>> = const { RefCell::new(None) };
        static ACTIONS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    struct Held {
        controller: Retained<UIViewController>,
        web: Option<WebView>,
    }

    impl Held {
        fn draw(
            level: &Level,
            root_view: &UIView,
            delegate: &NavDelegate,
            marker: MainThreadMarker,
        ) -> Option<Self> {
            let web = Surfaces::build(level.page)?;
            if level.page.background.is_none() {
                web.paint(crate::shell::webview_background());
            }
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
            let item = controller.navigationItem();
            item.setTitle(Some(&NSString::from_str(&level.title)));
            if let Some(action) = level.action {
                item.setRightBarButtonItem(Some(&Bar::button(action, delegate, marker)));
            }
            Some(Self {
                controller,
                web: Some(web),
            })
        }
    }

    struct Bar;

    impl Bar {
        fn button(
            action: &'static str,
            delegate: &NavDelegate,
            marker: MainThreadMarker,
        ) -> Retained<UIBarButtonItem> {
            let item = unsafe {
                UIBarButtonItem::initWithTitle_style_target_action(
                    UIBarButtonItem::alloc(marker),
                    Some(&NSString::from_str(action)),
                    UIBarButtonItemStyle::Plain,
                    Some(delegate),
                    Some(sel!(barTapped:)),
                )
            };
            item.setTag(Self::remember(action));
            item
        }

        fn remember(action: &'static str) -> isize {
            ACTIONS.with_borrow_mut(|known| {
                for (at, seen) in known.iter().enumerate() {
                    if *seen == action {
                        return at as isize;
                    }
                }
                known.push(action);
                (known.len() - 1) as isize
            })
        }

        fn recall(tag: isize) -> Option<&'static str> {
            ACTIONS.with_borrow(|known| known.get(tag as usize).copied())
        }

        fn glassy(navigation: &UINavigationController, marker: MainThreadMarker) {
            navigation.setNavigationBarHidden(false);
            let appearance = UINavigationBarAppearance::new(marker);
            appearance.configureWithDefaultBackground();
            let bar = navigation.navigationBar();
            bar.setStandardAppearance(&appearance);
            bar.setScrollEdgeAppearance(Some(&appearance));
        }
    }

    struct Column {
        navigation: Retained<UINavigationController>,
        levels: Vec<Held>,
    }

    impl Column {
        fn over(root: Held, delegate: &NavDelegate, marker: MainThreadMarker) -> Self {
            let navigation = UINavigationController::initWithRootViewController(
                UINavigationController::alloc(marker),
                &root.controller,
            );
            Bar::glassy(&navigation, marker);
            navigation.setAdditionalSafeAreaInsets(UIEdgeInsets {
                top: 0.0,
                left: 0.0,
                bottom: TAB_BAR_HEIGHT + TAB_BAR_GAP * 2.0,
                right: 0.0,
            });
            unsafe {
                navigation.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(delegate)));
            }
            if let Some(gesture) = navigation.interactivePopGestureRecognizer() {
                gesture.setEnabled(true);
                gesture.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(delegate)));
            }
            Self {
                navigation,
                levels: vec![root],
            }
        }

        fn owns(&self, navigation: &UINavigationController) -> bool {
            std::ptr::eq(
                &*self.navigation as *const UINavigationController,
                navigation as *const UINavigationController,
            )
        }
    }

    struct Tabs {
        strip: Retained<UIVisualEffectView>,
        capsule: Retained<UIVisualEffectView>,
        row: Retained<UIStackView>,
        circle: Retained<UIVisualEffectView>,
        ids: Vec<String>,
        at: usize,
    }

    impl Tabs {
        fn under(root_view: &UIView, marker: MainThreadMarker) -> Self {
            let bounds = root_view.bounds();
            let below = root_view.safeAreaInsets().bottom;
            let (width, height) = (bounds.size.width, bounds.size.height);

            let container = UIGlassContainerEffect::new(marker);
            container.setSpacing(TAB_BAR_GAP);
            let strip = UIVisualEffectView::initWithEffect(
                UIVisualEffectView::alloc(marker),
                Some(container.as_super()),
            );
            strip.setFrame(CGRect {
                origin: CGPoint {
                    x: 0.0,
                    y: height - TAB_BAR_HEIGHT - below - TAB_BAR_GAP,
                },
                size: CGSize {
                    width,
                    height: TAB_BAR_HEIGHT + below + TAB_BAR_GAP,
                },
            });
            strip.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleTopMargin,
            );

            let circle = Self::pane(TAB_BAR_HEIGHT, marker);
            circle.setFrame(CGRect {
                origin: CGPoint {
                    x: width - TAB_BAR_EDGE - TAB_BAR_HEIGHT,
                    y: 0.0,
                },
                size: CGSize {
                    width: TAB_BAR_HEIGHT,
                    height: TAB_BAR_HEIGHT,
                },
            });
            circle.setAutoresizingMask(UIViewAutoresizing::FlexibleLeftMargin);

            let capsule = Self::pane(TAB_BAR_HEIGHT, marker);
            let across = width - TAB_BAR_EDGE * 2.0 - TAB_BAR_HEIGHT - TAB_BAR_GAP;
            capsule.setFrame(CGRect {
                origin: CGPoint {
                    x: TAB_BAR_EDGE,
                    y: 0.0,
                },
                size: CGSize {
                    width: across,
                    height: TAB_BAR_HEIGHT,
                },
            });
            capsule.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth);

            let row = UIStackView::initWithFrame(
                UIStackView::alloc(marker),
                CGRect {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize {
                        width: across,
                        height: TAB_BAR_HEIGHT,
                    },
                },
            );
            row.setAxis(UILayoutConstraintAxis::Horizontal);
            row.setDistribution(UIStackViewDistribution::FillEqually);
            row.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            capsule.contentView().addSubview(&row);

            strip.contentView().addSubview(&capsule);
            strip.contentView().addSubview(&circle);
            match root_view.window() {
                Some(window) => window.addSubview(&strip),
                None => root_view.addSubview(&strip),
            }
            Self {
                strip,
                capsule,
                row,
                circle,
                ids: Vec::new(),
                at: 0,
            }
        }

        fn swipeable(&self, delegate: &NavDelegate, marker: MainThreadMarker) {
            let pan = unsafe {
                UIPanGestureRecognizer::initWithTarget_action(
                    UIPanGestureRecognizer::alloc(marker),
                    Some(delegate),
                    Some(sel!(panned:)),
                )
            };
            self.capsule.addGestureRecognizer(&pan);
        }

        fn neighbour(&self, towards: f64) -> Option<String> {
            if self.ids.len() < 2 {
                return None;
            }
            let next = if towards < 0.0 {
                self.at + 1
            } else {
                self.at.checked_sub(1)?
            };
            self.ids.get(next).cloned()
        }

        fn pane(height: f64, marker: MainThreadMarker) -> Retained<UIVisualEffectView> {
            let effect = UIGlassEffect::new(marker);
            effect.setInteractive(true);
            let pane = UIVisualEffectView::initWithEffect(
                UIVisualEffectView::alloc(marker),
                Some(effect.as_super()),
            );
            let layer = pane.layer();
            layer.setCornerRadius(height / 2.0);
            layer.setMasksToBounds(true);
            pane
        }

        fn show(
            &mut self,
            entries: Vec<TabEntry>,
            centre: Option<&'static str>,
            delegate: &NavDelegate,
            marker: MainThreadMarker,
        ) {
            for spent in self.row.arrangedSubviews().iter() {
                self.row.removeArrangedSubview(&spent);
                spent.removeFromSuperview();
            }
            self.ids.clear();
            for entry in entries {
                let button = UIButton::buttonWithType(UIButtonType::System, marker);
                button.setTitle_forState(
                    Some(&NSString::from_str(&entry.name)),
                    UIControlState::Normal,
                );
                if let Some(label) = button.titleLabel() {
                    unsafe { label.setFont(Some(&UIFont::systemFontOfSize(14.0))) };
                }
                let tone = if entry.here {
                    UIColor::labelColor()
                } else {
                    UIColor::secondaryLabelColor()
                };
                button.setTitleColor_forState(Some(&tone), UIControlState::Normal);
                button.setTag(self.ids.len() as isize);
                unsafe {
                    button.addTarget_action_forControlEvents(
                        Some(delegate),
                        sel!(tabTapped:),
                        UIControlEvents::TouchUpInside,
                    );
                }
                if entry.here {
                    self.at = self.ids.len();
                }
                self.ids.push(entry.id);
                self.row.addArrangedSubview(&button);
            }

            for spent in self.circle.contentView().subviews().iter() {
                spent.removeFromSuperview();
            }
            match centre {
                Some(centre) => {
                    let adder = Self::adder(centre, delegate, marker);
                    adder.setFrame(self.circle.bounds());
                    adder.setAutoresizingMask(
                        UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
                    );
                    self.circle.contentView().addSubview(&adder);
                    self.circle.setHidden(false);
                }
                None => self.circle.setHidden(true),
            }
            self.capsule.setHidden(self.ids.is_empty());
            self.strip
                .setHidden(self.ids.is_empty() && centre.is_none());
        }

        fn adder(
            centre: &'static str,
            delegate: &NavDelegate,
            marker: MainThreadMarker,
        ) -> Retained<UIButton> {
            let button = UIButton::buttonWithType(UIButtonType::System, marker);
            button.setTitle_forState(Some(&NSString::from_str(centre)), UIControlState::Normal);
            if let Some(label) = button.titleLabel() {
                unsafe { label.setFont(Some(&UIFont::systemFontOfSize(28.0))) };
            }
            button.setTitleColor_forState(Some(&UIColor::labelColor()), UIControlState::Normal);
            button.setTag(Bar::remember(centre));
            unsafe {
                button.addTarget_action_forControlEvents(
                    Some(delegate),
                    sel!(centreTapped:),
                    UIControlEvents::TouchUpInside,
                );
            }
            button
        }

        fn front(&self) {
            let Some(parent) = self.strip.superview() else {
                return;
            };
            parent.bringSubviewToFront(&self.strip);
        }
    }

    pub struct NativeStack {
        root_view: Retained<UIView>,
        columns: Vec<Column>,
        roots: HashMap<String, Held>,
        seated: Option<String>,
        dragging: Option<Drag>,
        tabs: Tabs,
        delegate: Retained<NavDelegate>,
    }

    struct Drag {
        to: String,
        incoming: Held,
        entering: f64,
        across: f64,
    }

    impl Drag {
        fn follow(shifted: f64) {
            let idle = STACK
                .with_borrow(|stack| stack.as_ref().is_some_and(|stack| stack.dragging.is_none()));
            if idle {
                if shifted.abs() < 8.0 {
                    return;
                }
                let started = Self::begin(shifted);
                STACK.with_borrow_mut(|stack| {
                    let Some(stack) = stack.as_mut() else {
                        return;
                    };
                    stack.dragging = started;
                });
            }
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                let Some(drag) = stack.dragging.as_ref() else {
                    return;
                };
                let shifted = shifted.clamp(-drag.across, drag.across);
                if let Some(leaving) = Self::leaving(stack) {
                    leaving.setTransform(Self::sideways(shifted));
                }
                if let Some(arriving) = drag.incoming.controller.view() {
                    arriving.setTransform(Self::sideways(shifted + drag.entering));
                }
            });
        }

        fn begin(shifted: f64) -> Option<Drag> {
            STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                if stack.columns.len() != 1 || stack.columns[0].levels.len() != 1 {
                    return None;
                }
                let to = stack.tabs.neighbour(shifted)?;
                let holder = stack.columns[0].navigation.view()?;
                let incoming = stack.roots.remove(&to)?;
                let across = holder.bounds().size.width;
                let entering = if shifted < 0.0 { across } else { -across };
                let view = incoming.controller.view()?;
                view.setFrame(holder.bounds());
                view.setTransform(Self::sideways(entering));
                holder.addSubview(&view);
                Some(Drag {
                    to,
                    incoming,
                    entering,
                    across,
                })
            })
        }

        fn release(shifted: f64, speed: f64) {
            let plan = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                let drag = stack.dragging.as_ref()?;
                let leaving = Self::leaving(stack)?;
                let arriving = drag.incoming.controller.view()?;
                let far = shifted.abs() > drag.across / 3.0 || speed.abs() > 600.0;
                let agreed = shifted != 0.0 && shifted.signum() == -drag.entering.signum();
                let commit = far && agreed;
                let landing = if commit { -drag.entering } else { 0.0 };
                Some((leaving, arriving, landing, drag.entering, commit))
            });
            let Some((leaving, arriving, landing, entering, commit)) = plan else {
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let slide = block2::RcBlock::new(move || {
                leaving.setTransform(Drag::sideways(landing));
                arriving.setTransform(Drag::sideways(landing + entering));
            });
            let done = block2::RcBlock::new(move |_| Drag::land(commit));
            UIView::animateWithDuration_animations_completion(0.3, &slide, Some(&done), marker);
        }

        fn land(commit: bool) {
            let arrived = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let drag = stack.dragging.take()?;
                if let Some(view) = drag.incoming.controller.view() {
                    view.setTransform(Self::sideways(0.0));
                    view.removeFromSuperview();
                }
                if let Some(leaving) = Self::leaving(stack) {
                    leaving.setTransform(Self::sideways(0.0));
                }
                let to = drag.to.clone();
                stack.roots.insert(drag.to, drag.incoming);
                Some(to)
            });
            let Some(to) = arrived else {
                return;
            };
            if commit {
                PICKED.with_borrow_mut(|slot| *slot = Some(to));
            }
        }

        fn leaving(stack: &NativeStack) -> Option<Retained<UIView>> {
            stack.columns.first()?.levels.first()?.controller.view()
        }

        fn sideways(by: f64) -> CGAffineTransform {
            CGAffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                tx: by,
                ty: 0.0,
            }
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxNavigationDelegate"]
        struct NavDelegate;

        impl NavDelegate {
            #[unsafe(method(barTapped:))]
            fn bar_tapped(&self, sender: &UIBarButtonItem) {
                let Some(action) = Bar::recall(sender.tag()) else {
                    return;
                };
                TAPPED.with_borrow_mut(|queued| queued.push(action));
            }

            #[unsafe(method(panned:))]
            fn panned(&self, sender: &UIPanGestureRecognizer) {
                let shifted = sender.translationInView(None).x;
                match sender.state() {
                    UIGestureRecognizerState::Changed => Drag::follow(shifted),
                    UIGestureRecognizerState::Ended => {
                        Drag::release(shifted, sender.velocityInView(None).x)
                    }
                    UIGestureRecognizerState::Cancelled | UIGestureRecognizerState::Failed => {
                        Drag::release(0.0, 0.0)
                    }
                    _ => {}
                }
            }

            #[unsafe(method(centreTapped:))]
            fn centre_tapped(&self, sender: &UIButton) {
                let Some(action) = Bar::recall(sender.tag()) else {
                    return;
                };
                TAPPED.with_borrow_mut(|queued| queued.push(action));
            }

            #[unsafe(method(tabTapped:))]
            fn tab_tapped(&self, sender: &UIButton) {
                let at = sender.tag() as usize;
                STACK.with_borrow(|stack| {
                    let Some(stack) = stack.as_ref() else {
                        return;
                    };
                    let Some(id) = stack.tabs.ids.get(at) else {
                        return;
                    };
                    PICKED.with_borrow_mut(|slot| *slot = Some(id.clone()));
                });
            }
        }

        unsafe impl NSObjectProtocol for NavDelegate {}

        unsafe impl UIGestureRecognizerDelegate for NavDelegate {
            #[unsafe(method(gestureRecognizerShouldBegin:))]
            fn should_begin(&self, _gesture: &UIGestureRecognizer) -> bool {
                STACK.with_borrow(|stack| {
                    let Some(stack) = stack.as_ref() else {
                        return false;
                    };
                    let Some(column) = stack.columns.last() else {
                        return false;
                    };
                    column.levels.len() > 1
                })
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
                    stack.tabs.front();
                    for column in stack.columns.iter_mut() {
                        if !column.owns(navigation) {
                            continue;
                        }
                        if shown >= column.levels.len() {
                            return;
                        }
                        let dropped = column.levels.len() - shown;
                        column.levels.truncate(shown);
                        POPPED.set(POPPED.get() + dropped);
                        return;
                    }
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
                    if stack.columns.len() < 2 {
                        return;
                    }
                    let Some(departing) = stack.columns.pop() else {
                        return;
                    };
                    DISMISSED.set(DISMISSED.get() + departing.levels.len());
                });
            }
        }

        unsafe impl UISheetPresentationControllerDelegate for NavDelegate {}
    );

    pub fn install(
        root_controller: &UIViewController,
        root_view: &UIView,
        web_view: &UIView,
        page: &'static vmux_native::NativePage,
    ) {
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

        if let Some(dark) = page.prefers_dark() {
            let style = if dark {
                UIUserInterfaceStyle::Dark
            } else {
                UIUserInterfaceStyle::Light
            };
            root_controller.setOverrideUserInterfaceStyle(style);
            if let Some(window) = root_view.window() {
                window.setOverrideUserInterfaceStyle(style);
            }
        }

        web_view.setHidden(true);
        let first =
            UIViewController::initWithNibName_bundle(UIViewController::alloc(marker), None, None);
        let delegate = NavDelegate::new(marker);
        let column = Column::over(
            Held {
                controller: first,
                web: None,
            },
            &delegate,
            marker,
        );

        root_controller.addChildViewController(&column.navigation);
        let Some(navigation_view) = column.navigation.view() else {
            return;
        };
        size_to_parent(&navigation_view, &root_view);
        root_view.addSubview(&navigation_view);
        column
            .navigation
            .didMoveToParentViewController(Some(&root_controller));

        let tabs = Tabs::under(&root_view, marker);
        tabs.swipeable(&delegate, marker);
        STACK.set(Some(NativeStack {
            root_view,
            columns: vec![column],
            roots: HashMap::new(),
            seated: None,
            dragging: None,
            tabs,
            delegate,
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
                    let column = stack.as_mut()?.columns.last_mut()?;
                    let controller = drawn.controller.clone();
                    column.levels.push(drawn);
                    Some((column.navigation.clone(), controller))
                });
                let Some((navigation, top)) = pending else {
                    return;
                };
                navigation.pushViewController_animated(&top, true);
            });
        }

        pub fn pop() {
            let navigation = STACK.with_borrow(|stack| {
                let column = stack.as_ref()?.columns.last()?;
                if column.levels.len() < 2 {
                    return None;
                }
                Some(column.navigation.clone())
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
                    let presenter = stack.columns.last()?.navigation.clone();
                    let column = Column::over(drawn, &stack.delegate, marker);
                    let sheet = column.navigation.clone();
                    sheet.setModalPresentationStyle(UIModalPresentationStyle::PageSheet);
                    if let Some(controller) = sheet.sheetPresentationController() {
                        unsafe {
                            controller.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(
                                &*stack.delegate,
                            )));
                        }
                        Self::detents(&controller, marker);
                    }
                    stack.columns.push(column);
                    Some((presenter, sheet))
                });
                let Some((presenter, sheet)) = pending else {
                    return;
                };
                presenter.presentViewController_animated_completion(&sheet, true, None);
                Self::raise();
            });
        }

        pub fn dismiss() {
            let departing = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                if stack.columns.len() < 2 {
                    return None;
                }
                stack.columns.pop()
            });
            let Some(departing) = departing else {
                return;
            };
            departing
                .navigation
                .dismissViewControllerAnimated_completion(true, None);
            Self::raise();
        }

        fn raise() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                stack.tabs.front();
            });
        }

        pub fn settle(tab: String, levels: Vec<Level>) {
            let mut carried = Vec::new();
            let cached = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                Some(stack.roots.contains_key(&tab))
            });
            for (at, level) in levels.into_iter().enumerate() {
                if at == 0 && cached == Some(true) {
                    carried.push(None);
                    continue;
                }
                carried.push(Self::draw(level));
            }

            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                while stack.columns.len() > 1 {
                    let Some(sheet) = stack.columns.pop() else {
                        break;
                    };
                    sheet
                        .navigation
                        .dismissViewControllerAnimated_completion(false, None);
                }
                let Some(column) = stack.columns.first_mut() else {
                    return;
                };
                if !column.levels.is_empty()
                    && let Some(seated) = stack.seated.take()
                {
                    let root = column.levels.remove(0);
                    stack.roots.insert(seated, root);
                }
                column.levels.clear();
                for (at, held) in carried.into_iter().enumerate() {
                    match held {
                        Some(held) => column.levels.push(held),
                        None if at == 0 => {
                            let Some(root) = stack.roots.remove(&tab) else {
                                continue;
                            };
                            column.levels.push(root);
                        }
                        None => {}
                    }
                }
                stack.seated = Some(tab);
                if column.levels.is_empty() {
                    return;
                }
                let mut controllers = Vec::new();
                for level in &column.levels {
                    controllers.push(level.controller.clone());
                }
                column.navigation.setViewControllers_animated(
                    &objc2_foundation::NSArray::from_retained_slice(&controllers),
                    false,
                );
            });
        }

        pub fn warm(wanted: Vec<(String, Level)>) {
            let mut fresh = Vec::new();
            for (id, level) in wanted {
                let held = STACK.with_borrow(|stack| {
                    stack
                        .as_ref()
                        .is_some_and(|stack| stack.roots.contains_key(&id))
                });
                if held {
                    fresh.push((id, None));
                    continue;
                }
                fresh.push((id, Self::draw(level)));
            }
            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                if stack.dragging.is_some() {
                    return;
                }
                let mut keep = Vec::new();
                for (id, held) in fresh {
                    if let Some(held) = held {
                        stack.roots.insert(id.clone(), held);
                    }
                    keep.push(id);
                }
                stack.roots.retain(|id, _| keep.contains(id));
            });
        }

        pub fn tabs(entries: Vec<TabEntry>, centre: Option<&'static str>) {
            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                let Some(marker) = MainThreadMarker::new() else {
                    return;
                };
                let delegate = stack.delegate.clone();
                stack.tabs.show(entries, centre, &delegate, marker);
                stack.tabs.front();
            });
        }

        pub fn render() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                for column in &stack.columns {
                    for level in &column.levels {
                        let Some(web) = level.web.as_ref() else {
                            continue;
                        };
                        web.render();
                    }
                }
                for root in stack.roots.values() {
                    let Some(web) = root.web.as_ref() else {
                        continue;
                    };
                    web.render();
                }
            });
        }

        fn draw(level: Level) -> Option<Held> {
            let (root_view, delegate, marker) = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                Some((
                    stack.root_view.clone(),
                    stack.delegate.clone(),
                    MainThreadMarker::new()?,
                ))
            })?;
            Held::draw(&level, &root_view, &delegate, marker)
        }

        fn detents(controller: &UISheetPresentationController, marker: MainThreadMarker) {
            let large = UISheetPresentationControllerDetent::largeDetent(marker);
            controller.setDetents(&objc2_foundation::NSArray::from_retained_slice(&[large]));
            controller.setPrefersGrabberVisible(true);
        }
    }

    pub fn take_popped() -> usize {
        POPPED.replace(0)
    }

    pub fn take_dismissed() -> usize {
        DISMISSED.replace(0)
    }

    pub fn take_tapped() -> Vec<&'static str> {
        TAPPED.with_borrow_mut(std::mem::take)
    }

    pub fn take_picked() -> Option<String> {
        PICKED.with_borrow_mut(Option::take)
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
    use super::{Level, TabEntry};

    pub struct NativeStack;

    pub fn install(_: &(), _: &(), _: &(), _: &'static vmux_native::NativePage) {}

    pub fn take_popped() -> usize {
        0
    }

    pub fn take_dismissed() -> usize {
        0
    }

    pub fn take_tapped() -> Vec<&'static str> {
        Vec::new()
    }

    pub fn take_picked() -> Option<String> {
        None
    }

    impl NativeStack {
        pub fn push(_level: Level) {}

        pub fn pop() {}

        pub fn present(_level: Level) {}

        pub fn dismiss() {}

        pub fn settle(_tab: String, _levels: Vec<Level>) {}

        pub fn tabs(_entries: Vec<TabEntry>, _centre: Option<&'static str>) {}

        pub fn warm(_wanted: Vec<(String, Level)>) {}

        pub fn render() {}
    }
}

#[cfg(target_os = "ios")]
pub use platform::install;
pub use platform::{NativeStack, take_dismissed, take_picked, take_popped, take_tapped};
