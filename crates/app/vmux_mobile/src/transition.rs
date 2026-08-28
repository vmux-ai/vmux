use vmux_native::NativePage;

#[derive(Clone, Copy, PartialEq)]
pub enum Presentation {
    Card,
    Modal,
    FormSheet,
    FullScreenModal,
    TransparentModal,
}

impl Presentation {
    pub fn pushes(self) -> bool {
        matches!(self, Self::Card)
    }
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct Level {
    pub page: &'static NativePage,
    pub title: String,
    pub action: Option<&'static str>,
    pub presentation: Presentation,
    pub detents: &'static [f64],
    pub seat: vmux_native::Instance,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct TabItem {
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
        UIAdaptivePresentationControllerDelegate, UIBarButtonItem, UIBarButtonItemStyle,
        UIBarButtonSystemItem, UIButton, UIButtonType, UIColor, UIControlEvents, UIControlState,
        UIEdgeInsets, UIFont, UIGestureRecognizer, UIGestureRecognizerDelegate,
        UIGestureRecognizerState, UIGlassContainerEffect, UIGlassEffect, UILayoutConstraintAxis,
        UIModalPresentationStyle, UINavigationBarAppearance, UINavigationController,
        UINavigationControllerDelegate, UIPanGestureRecognizer, UIPresentationController,
        UISheetPresentationController, UISheetPresentationControllerDelegate,
        UISheetPresentationControllerDetent, UISheetPresentationControllerDetentResolutionContext,
        UIStackView, UIStackViewDistribution, UIUserInterfaceStyle, UIView, UIViewAutoresizing,
        UIViewController, UIViewKeyframeAnimationOptions, UIVisualEffectView,
    };
    use vmux_native::WebView;

    use super::{Level, Presentation, TabItem};
    use crate::surface::Surfaces;

    const TAB_BAR_HEIGHT: f64 = 56.0;
    const TAB_BAR_EDGE: f64 = 16.0;
    const TAB_BAR_GAP: f64 = 10.0;
    const DIP: f64 = 0.06;

    thread_local! {
        static STACK: RefCell<Option<NativeStack>> = const { RefCell::new(None) };
        static POPPED: Cell<usize> = const { Cell::new(0) };
        static DISMISSED: Cell<usize> = const { Cell::new(0) };
        static TAPPED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        static PICKED: RefCell<Option<String>> = const { RefCell::new(None) };
        static ACTIONS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        static CLOSED: Cell<bool> = const { Cell::new(false) };
    }

    struct Held {
        controller: Retained<UIViewController>,
        web: Option<WebView>,
    }

    impl Held {
        fn draw(
            level: Level,
            root_view: &UIView,
            backdrop: (u8, u8, u8, u8),
            delegate: &NavDelegate,
            marker: MainThreadMarker,
        ) -> Option<Self> {
            let web = Surfaces::build(level.page, level.seat)?;
            web.paint(level.page.background_or(backdrop));
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
            if !level.presentation.pushes() {
                item.setRightBarButtonItem(Some(&Bar::closer(delegate, marker)));
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

        fn closer(delegate: &NavDelegate, marker: MainThreadMarker) -> Retained<UIBarButtonItem> {
            unsafe {
                UIBarButtonItem::initWithBarButtonSystemItem_target_action(
                    UIBarButtonItem::alloc(marker),
                    UIBarButtonSystemItem::Close,
                    Some(delegate),
                    Some(sel!(closeTapped:)),
                )
            }
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
            let next = if towards > 0.0 {
                self.at + 1
            } else {
                self.at.checked_sub(1)?
            };
            self.ids.get(next).cloned()
        }

        fn after(&self, from: &str, to: &str) -> bool {
            let mut seen = None;
            for (at, id) in self.ids.iter().enumerate() {
                if id == from {
                    seen = Some(at);
                }
                if id == to
                    && let Some(seen) = seen
                {
                    return at > seen;
                }
            }
            false
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
            entries: Vec<TabItem>,
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
        root_controller: Retained<UIViewController>,
        pager: Retained<UIView>,
        rootless: Retained<UIView>,
        backdrop: (u8, u8, u8, u8),
        stacks: HashMap<String, Column>,
        sheets: Vec<Column>,
        seated: Option<String>,
        dragging: Option<Drag>,
        tabs: Tabs,
        delegate: Retained<NavDelegate>,
    }

    impl NativeStack {
        pub fn seat(tab: String, levels: Vec<Level>) {
            Self::shed();
            Self::raise(&tab, levels);
            let plan = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let arriving = stack.stacks.get(&tab)?.navigation.view()?;
                stack.pager.bringSubviewToFront(&arriving);
                stack.rootless.setHidden(true);
                let vacated = stack.seated.replace(tab.clone());
                let Some(vacated) = vacated.filter(|seen| *seen != tab) else {
                    arriving.setTransform(Drag::sideways(0.0));
                    arriving.setHidden(false);
                    return None;
                };
                let across = stack.pager.bounds().size.width;
                let entering = if stack.tabs.after(&vacated, &tab) {
                    across
                } else {
                    -across
                };
                let leaving = stack
                    .stacks
                    .get(&vacated)
                    .and_then(|column| column.navigation.view());
                arriving.setTransform(Drag::sideways(entering));
                arriving.setHidden(false);
                Some((leaving, arriving, entering))
            });
            Self::front();
            let Some((leaving, arriving, entering)) = plan else {
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let departing = leaving.clone();
            let done = block2::RcBlock::new(move |_| {
                let Some(departing) = departing.as_ref() else {
                    return;
                };
                departing.setHidden(true);
                departing.setTransform(Drag::sideways(0.0));
            });
            Drag::glide(leaving, arriving, 0.0, entering, -entering, done, marker);
        }

        pub fn warm(wanted: Vec<(String, Vec<Level>)>) {
            let mut keep = Vec::new();
            for (tab, levels) in wanted {
                Self::raise(&tab, levels);
                keep.push(tab);
            }
            let spent = STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return Vec::new();
                };
                if stack.dragging.is_some() {
                    return Vec::new();
                }
                let mut spent = Vec::new();
                let mut leaving = Vec::new();
                for id in stack.stacks.keys() {
                    if keep.contains(id) || stack.seated.as_ref() == Some(id) {
                        continue;
                    }
                    leaving.push(id.clone());
                }
                for id in leaving {
                    let Some(column) = stack.stacks.remove(&id) else {
                        continue;
                    };
                    spent.push(column);
                }
                spent
            });
            for column in spent {
                if let Some(view) = column.navigation.view() {
                    view.removeFromSuperview();
                }
                column.navigation.willMoveToParentViewController(None);
                column.navigation.removeFromParentViewController();
            }
        }

        fn raise(tab: &str, levels: Vec<Level>) {
            let known = STACK
                .with_borrow(|stack| stack.as_ref().is_some_and(|s| s.stacks.contains_key(tab)));
            if known {
                return;
            }
            let Some((root_controller, pager, backdrop, delegate, marker)) =
                STACK.with_borrow(|stack| {
                    let stack = stack.as_ref()?;
                    Some((
                        stack.root_controller.clone(),
                        stack.pager.clone(),
                        stack.backdrop,
                        stack.delegate.clone(),
                        MainThreadMarker::new()?,
                    ))
                })
            else {
                return;
            };

            let mut held = Vec::new();
            for level in levels {
                let Some(drawn) = Held::draw(level, &pager, backdrop, &delegate, marker) else {
                    continue;
                };
                held.push(drawn);
            }
            if held.is_empty() {
                return;
            }
            let mut column = Column::over(held.remove(0), &delegate, marker);
            column.levels.extend(held);

            root_controller.addChildViewController(&column.navigation);
            let Some(view) = column.navigation.view() else {
                return;
            };
            size_to_parent(&view, &pager);
            view.setHidden(true);
            pager.addSubview(&view);
            column
                .navigation
                .didMoveToParentViewController(Some(&root_controller));
            if column.levels.len() > 1 {
                let mut controllers = Vec::new();
                for level in &column.levels {
                    controllers.push(level.controller.clone());
                }
                column.navigation.setViewControllers_animated(
                    &objc2_foundation::NSArray::from_retained_slice(&controllers),
                    false,
                );
            }
            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                stack.stacks.insert(tab.to_string(), column);
            });
        }

        pub fn push(level: Level) {
            let drawn = Self::draw(level);
            after_paint(move || {
                let Some(drawn) = drawn else {
                    return;
                };
                let pending = STACK.with_borrow_mut(|stack| {
                    let column = Self::topmost(stack.as_mut()?)?;
                    let controller = drawn.controller.clone();
                    column.levels.push(drawn);
                    Some((column.navigation.clone(), controller))
                });
                let Some((navigation, top)) = pending else {
                    return;
                };
                navigation.pushViewController_animated(&top, true);
                Self::front();
            });
        }

        pub fn pop() {
            let navigation = STACK.with_borrow_mut(|stack| {
                let column = Self::topmost(stack.as_mut()?)?;
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
            let (presentation, detents) = (level.presentation, level.detents);
            let drawn = Self::draw(level);
            after_paint(move || {
                let Some(drawn) = drawn else {
                    return;
                };
                let pending = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let marker = MainThreadMarker::new()?;
                    let presenter = Self::topmost(stack)?.navigation.clone();
                    let column = Column::over(drawn, &stack.delegate, marker);
                    let sheet = column.navigation.clone();
                    sheet.setModalPresentationStyle(match presentation {
                        Presentation::FullScreenModal | Presentation::TransparentModal => {
                            UIModalPresentationStyle::OverFullScreen
                        }
                        Presentation::FormSheet => UIModalPresentationStyle::FormSheet,
                        Presentation::Modal | Presentation::Card => {
                            UIModalPresentationStyle::PageSheet
                        }
                    });
                    if let Some(controller) = sheet.sheetPresentationController() {
                        unsafe {
                            controller.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(
                                &*stack.delegate,
                            )));
                        }
                        if presentation == Presentation::FormSheet {
                            Self::detents(&controller, detents, marker);
                        }
                    }
                    stack.sheets.push(column);
                    Some((presenter, sheet))
                });
                let Some((presenter, sheet)) = pending else {
                    return;
                };
                presenter.presentViewController_animated_completion(&sheet, true, None);
                Self::front();
            });
        }

        pub fn dismiss() {
            let departing = STACK.with_borrow_mut(|stack| stack.as_mut()?.sheets.pop());
            let Some(departing) = departing else {
                return;
            };
            departing
                .navigation
                .dismissViewControllerAnimated_completion(true, None);
            Self::front();
        }

        fn shed() {
            let sheets = STACK.with_borrow_mut(|stack| match stack.as_mut() {
                Some(stack) => std::mem::take(&mut stack.sheets),
                None => Vec::new(),
            });
            for sheet in sheets {
                sheet
                    .navigation
                    .dismissViewControllerAnimated_completion(false, None);
            }
        }

        pub fn tabs(entries: Vec<TabItem>, centre: Option<&'static str>) {
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
                for column in stack.stacks.values().chain(stack.sheets.iter()) {
                    for level in &column.levels {
                        let Some(web) = level.web.as_ref() else {
                            continue;
                        };
                        web.render();
                    }
                }
                if let Some(drag) = stack.dragging.as_ref() {
                    for level in &drag.incoming.levels {
                        let Some(web) = level.web.as_ref() else {
                            continue;
                        };
                        web.render();
                    }
                }
            });
        }

        fn topmost(stack: &mut NativeStack) -> Option<&mut Column> {
            if !stack.sheets.is_empty() {
                return stack.sheets.last_mut();
            }
            let seated = stack.seated.clone()?;
            stack.stacks.get_mut(&seated)
        }

        fn front() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                stack.tabs.front();
            });
        }

        fn draw(level: Level) -> Option<Held> {
            let (pager, backdrop, delegate, marker) = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                Some((
                    stack.pager.clone(),
                    stack.backdrop,
                    stack.delegate.clone(),
                    MainThreadMarker::new()?,
                ))
            })?;
            Held::draw(level, &pager, backdrop, &delegate, marker)
        }

        fn detents(
            controller: &UISheetPresentationController,
            wanted: &'static [f64],
            marker: MainThreadMarker,
        ) {
            controller.setPrefersGrabberVisible(true);
            if wanted.is_empty() {
                return;
            }
            let mut listed = Vec::new();
            for (at, fraction) in wanted.iter().enumerate() {
                let fraction = *fraction;
                let resolve = block2::RcBlock::new(
                    move |context: std::ptr::NonNull<
                        objc2::runtime::ProtocolObject<
                            dyn UISheetPresentationControllerDetentResolutionContext,
                        >,
                    >| {
                        fraction * unsafe { context.as_ref() }.maximumDetentValue()
                    },
                );
                let named = NSString::from_str(&format!("vmux:{at}"));
                listed.push(
                    UISheetPresentationControllerDetent::customDetentWithIdentifier_resolver(
                        Some(&named),
                        &resolve,
                        marker,
                    ),
                );
            }
            controller.setDetents(&objc2_foundation::NSArray::from_retained_slice(&listed));
        }
    }

    struct Drag {
        to: String,
        incoming: Column,
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
                let travelled = -shifted.clamp(-drag.across, drag.across);
                let scale =
                    1.0 - DIP * (std::f64::consts::PI * travelled / drag.across).sin().abs();
                if let Some(leaving) = Self::leaving(stack) {
                    leaving.setTransform(Self::moved(travelled, scale));
                }
                if let Some(arriving) = drag.incoming.navigation.view() {
                    arriving.setTransform(Self::moved(travelled + drag.entering, scale));
                }
            });
        }

        fn begin(shifted: f64) -> Option<Drag> {
            STACK.with_borrow_mut(|stack| {
                if !stack.as_ref()?.sheets.is_empty() {
                    return None;
                }
                let to = stack.as_ref()?.tabs.neighbour(shifted)?;
                let stack = stack.as_mut()?;
                let incoming = stack.stacks.remove(&to)?;
                let across = stack.pager.bounds().size.width;
                let entering = if shifted > 0.0 { across } else { -across };
                let view = incoming.navigation.view()?;
                view.setTransform(Self::sideways(entering));
                view.setHidden(false);
                stack.pager.bringSubviewToFront(&view);
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
                let arriving = drag.incoming.navigation.view()?;
                let far = shifted.abs() > drag.across / 3.0 || speed.abs() > 600.0;
                let agreed = shifted != 0.0 && shifted.signum() == drag.entering.signum();
                let commit = far && agreed;
                let landing = if commit { -drag.entering } else { 0.0 };
                let from = -shifted.clamp(-drag.across, drag.across);
                Some((leaving, arriving, landing, drag.entering, commit, from))
            });
            let Some((leaving, arriving, landing, entering, commit, from)) = plan else {
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let done = block2::RcBlock::new(move |_| Drag::land(commit));
            Self::glide(
                Some(leaving),
                arriving,
                from,
                entering,
                landing,
                done,
                marker,
            );
        }

        fn land(commit: bool) {
            let arrived = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let drag = stack.dragging.take()?;
                if let Some(view) = drag.incoming.navigation.view() {
                    view.setTransform(Self::sideways(0.0));
                    view.setHidden(!commit);
                }
                if let Some(leaving) = Self::leaving(stack) {
                    leaving.setTransform(Self::sideways(0.0));
                    leaving.setHidden(commit);
                }
                let to = drag.to.clone();
                stack.stacks.insert(drag.to.clone(), drag.incoming);
                if commit {
                    stack.seated = Some(drag.to);
                }
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
            let seated = stack.seated.as_ref()?;
            stack.stacks.get(seated)?.navigation.view()
        }

        fn sideways(by: f64) -> CGAffineTransform {
            Self::moved(by, 1.0)
        }

        fn moved(by: f64, scale: f64) -> CGAffineTransform {
            CGAffineTransform {
                a: scale,
                b: 0.0,
                c: 0.0,
                d: scale,
                tx: by,
                ty: 0.0,
            }
        }

        fn glide(
            leaving: Option<Retained<UIView>>,
            arriving: Retained<UIView>,
            from: f64,
            entering: f64,
            landing: f64,
            settled: block2::RcBlock<dyn Fn(objc2::runtime::Bool)>,
            marker: MainThreadMarker,
        ) {
            let midway = (from + landing) / 2.0;
            let (dipping, dipped) = (leaving.clone(), arriving.clone());
            let slide = block2::RcBlock::new(move || {
                let (leaving, arriving) = (dipping.clone(), dipped.clone());
                let dip = block2::RcBlock::new(move || {
                    if let Some(leaving) = leaving.as_ref() {
                        leaving.setTransform(Drag::moved(midway, 1.0 - DIP));
                    }
                    arriving.setTransform(Drag::moved(midway + entering, 1.0 - DIP));
                });
                UIView::addKeyframeWithRelativeStartTime_relativeDuration_animations(
                    0.0, 0.5, &dip, marker,
                );
                let (leaving, arriving) = (dipping.clone(), dipped.clone());
                let rise = block2::RcBlock::new(move || {
                    if let Some(leaving) = leaving.as_ref() {
                        leaving.setTransform(Drag::sideways(landing));
                    }
                    arriving.setTransform(Drag::sideways(landing + entering));
                });
                UIView::addKeyframeWithRelativeStartTime_relativeDuration_animations(
                    0.5, 0.5, &rise, marker,
                );
            });
            UIView::animateKeyframesWithDuration_delay_options_animations_completion(
                0.34,
                0.0,
                UIViewKeyframeAnimationOptions::CalculationModeCubic,
                &slide,
                Some(&settled),
                marker,
            );
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

            #[unsafe(method(closeTapped:))]
            fn close_tapped(&self, _sender: &UIBarButtonItem) {
                CLOSED.set(true);
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
        }

        unsafe impl NSObjectProtocol for NavDelegate {}

        unsafe impl UIGestureRecognizerDelegate for NavDelegate {
            #[unsafe(method(gestureRecognizerShouldBegin:))]
            fn should_begin(&self, _gesture: &UIGestureRecognizer) -> bool {
                STACK.with_borrow(|stack| {
                    let Some(stack) = stack.as_ref() else {
                        return false;
                    };
                    let Some(seated) = stack.seated.as_ref() else {
                        return false;
                    };
                    let Some(column) = stack.stacks.get(seated) else {
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
                    for column in stack.stacks.values_mut().chain(stack.sheets.iter_mut()) {
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
                    let Some(departing) = stack.sheets.pop() else {
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

        let pager = UIView::initWithFrame(UIView::alloc(marker), root_view.bounds());
        size_to_parent(&pager, &root_view);
        pager.setClipsToBounds(true);
        root_view.addSubview(&pager);

        let delegate = NavDelegate::new(marker);
        let tabs = Tabs::under(&root_view, marker);
        tabs.swipeable(&delegate, marker);
        STACK.set(Some(NativeStack {
            root_controller,
            pager,
            rootless: web_view,
            backdrop: page.background_or(crate::root::webview_background()),
            stacks: HashMap::new(),
            sheets: Vec::new(),
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

    pub fn take_popped() -> usize {
        POPPED.replace(0)
    }

    pub fn take_dismissed() -> usize {
        DISMISSED.replace(0)
    }

    pub fn take_tapped() -> Vec<&'static str> {
        TAPPED.with_borrow_mut(std::mem::take)
    }

    pub fn take_closed() -> bool {
        CLOSED.replace(false)
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
    use super::{Level, TabItem};

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

    pub fn take_closed() -> bool {
        false
    }

    impl NativeStack {
        pub fn push(_level: Level) {}

        pub fn pop() {}

        pub fn present(_level: Level) {}

        pub fn dismiss() {}

        pub fn seat(_tab: String, _levels: Vec<Level>) {}

        pub fn tabs(_entries: Vec<TabItem>, _centre: Option<&'static str>) {}

        pub fn warm(_wanted: Vec<(String, Vec<Level>)>) {}

        pub fn render() {}
    }
}

#[cfg(target_os = "ios")]
pub use platform::install;
pub use platform::{
    NativeStack, take_closed, take_dismissed, take_picked, take_popped, take_tapped,
};
