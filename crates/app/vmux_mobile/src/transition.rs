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
pub const ROTATE: &str = "\u{27f3}";
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub const ROTATE_BACK: &str = "\u{27f2}";

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct Level {
    pub key: u64,
    pub page: &'static NativePage,
    pub title: String,
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

    use block2::RcBlock;
    use dispatch2::{DispatchQueue, DispatchTime};
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{
        ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel,
    };
    use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
    use objc2_foundation::{NSArray, NSObjectProtocol, NSString};
    use objc2_quartz_core::{CATransform3D, kCACornerCurveContinuous};
    use objc2_ui_kit::{
        NSTextAlignment, UIAction, UIAdaptivePresentationControllerDelegate, UIButton,
        UIButtonType, UIColor, UIControlEvents, UIControlState, UIEdgeInsets, UIFont,
        UIGestureRecognizer, UIGestureRecognizerDelegate, UIGestureRecognizerState,
        UIGlassContainerEffect, UIGlassEffect, UIImage, UILabel, UILayoutConstraintAxis, UIMenu,
        UIMenuElement, UIMenuElementAttributes, UIModalPresentationStyle, UINavigationController,
        UINavigationControllerDelegate, UIPanGestureRecognizer, UIPresentationController,
        UISheetPresentationController, UISheetPresentationControllerDelegate,
        UISheetPresentationControllerDetent, UISheetPresentationControllerDetentResolutionContext,
        UIStackView, UIStackViewDistribution, UITapGestureRecognizer, UIUserInterfaceStyle, UIView,
        UIViewAnimationOptions, UIViewAutoresizing, UIViewController,
        UIViewControllerTransitionCoordinator, UIViewControllerTransitionCoordinatorContext,
        UIViewKeyframeAnimationOptions, UIVisualEffectView,
    };
    use vmux_native::WebView;

    use super::{Level, Presentation, TabItem};
    use crate::surface::Surfaces;

    const TAB_BAR_HEIGHT: f64 = 56.0;
    const TAB_BAR_EDGE: f64 = 16.0;
    const TAB_BAR_GAP: f64 = 10.0;
    const MARK_BAR_HEIGHT: f64 = 26.0;
    const MARK_HEIGHT: f64 = 4.0;
    const MARK_GAP: f64 = 6.0;
    const MARK_INSET: f64 = 14.0;
    const MARK_LIT: f64 = 0.9;
    const MARK_DIM: f64 = 0.25;
    const MARK_CLEARANCE: f64 = TAB_BAR_GAP + MARK_BAR_HEIGHT;
    const MARK_GLIDE: f64 = 0.28;
    const MARK_MOST: usize = 8;
    const MARK_TALLY: f64 = 11.0;
    const MARK_EDGE: usize = 1;
    const DIP: f64 = 0.06;
    const TAB_TRAIL: f64 = 0.5;
    const SHEET_FADE: f64 = 0.55;
    const SHEET_RECEDE: f64 = 0.92;
    const SHEET_CORNER: f64 = 30.0;
    const OVERVIEW_SCALE: f64 = 0.54;
    const OVERVIEW_TILT: f64 = 0.95;
    const OVERVIEW_SPREAD: f64 = 0.82;
    const OVERVIEW_TIGHT: f64 = 0.22;
    const OVERVIEW_DEPTH: f64 = 190.0;
    const OVERVIEW_EYE: f64 = -1.0 / 900.0;
    const OVERVIEW_GAP: f64 = 14.0;
    const OVERVIEW_GLIDE: f64 = 0.35;
    const OVERVIEW_RESIST: f64 = 0.3;

    thread_local! {
        static STACK: RefCell<Option<NativeStack>> = const { RefCell::new(None) };
        static POPPED: Cell<usize> = const { Cell::new(0) };
        static DISMISSED: Cell<usize> = const { Cell::new(0) };
        static TAPPED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        static PICKED: RefCell<Option<String>> = const { RefCell::new(None) };
        static ACTIONS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        static CLOSED: Cell<bool> = const { Cell::new(false) };
        static CLOSING: RefCell<Option<String>> = const { RefCell::new(None) };
        static SLIDING: RefCell<Option<Slide>> = const { RefCell::new(None) };
    }

    struct Held {
        controller: Retained<UIViewController>,
        web: Option<WebView>,
        title: String,
    }

    impl Held {
        fn draw(
            level: Level,
            root_view: &UIView,
            backdrop: (u8, u8, u8, u8),
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
            Some(Self {
                controller,
                web: Some(web),
                title: level.title,
            })
        }
    }

    struct Bar;

    impl Bar {
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
    }

    struct Column {
        key: u64,
        navigation: Retained<UINavigationController>,
        levels: Vec<Held>,
        watched: bool,
    }

    impl Column {
        fn over(key: u64, root: Held, delegate: &NavDelegate, marker: MainThreadMarker) -> Self {
            let navigation = UINavigationController::initWithRootViewController(
                UINavigationController::alloc(marker),
                &root.controller,
            );
            navigation.setNavigationBarHidden(true);
            unsafe {
                navigation.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(delegate)));
            }
            if let Some(gesture) = navigation.interactivePopGestureRecognizer() {
                gesture.setEnabled(true);
                gesture.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(delegate)));
            }
            Self {
                key,
                navigation,
                levels: vec![root],
                watched: false,
            }
        }

        fn inset(navigation: &UINavigationController, top: f64) {
            navigation.setAdditionalSafeAreaInsets(UIEdgeInsets {
                top,
                left: 0.0,
                bottom: 0.0,
                right: 0.0,
            });
        }

        fn owns(&self, navigation: &UINavigationController) -> bool {
            std::ptr::eq(
                &*self.navigation as *const UINavigationController,
                navigation as *const UINavigationController,
            )
        }
    }

    struct Pane;

    impl Pane {
        fn glass(height: f64, marker: MainThreadMarker) -> Retained<UIVisualEffectView> {
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

        fn glyph(
            name: &str,
            delegate: &NavDelegate,
            action: objc2::runtime::Sel,
            marker: MainThreadMarker,
        ) -> Retained<UIButton> {
            let button = UIButton::buttonWithType(UIButtonType::System, marker);
            if let Some(glyph) = UIImage::systemImageNamed(&NSString::from_str(name)) {
                button.setImage_forState(Some(&glyph), UIControlState::Normal);
            }
            unsafe { button.setTintColor(Some(&UIColor::labelColor())) };
            unsafe {
                button.addTarget_action_forControlEvents(
                    Some(delegate),
                    action,
                    UIControlEvents::TouchUpInside,
                );
            }
            button
        }

        fn fill(button: &UIButton, host: &UIVisualEffectView) {
            button.setFrame(host.bounds());
            button.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            host.contentView().addSubview(button);
        }
    }

    enum Rung {
        Mark(usize),
        Gap(usize),
    }

    impl Rung {
        fn over(count: usize, start: usize) -> Vec<Self> {
            let mut rungs = Vec::new();
            if count <= MARK_MOST {
                for index in 0..count {
                    rungs.push(Self::Mark(index));
                }
                return rungs;
            }
            let room = MARK_MOST - 2;
            let start = start.min(count - room);
            if start > 0 {
                rungs.push(Self::Gap(start));
            }
            for index in start..start + room {
                rungs.push(Self::Mark(index));
            }
            let after = count - (start + room);
            if after > 0 {
                rungs.push(Self::Gap(after));
            }
            rungs
        }

        fn slot(rungs: &[Self], wanted: usize) -> Option<usize> {
            for (slot, rung) in rungs.iter().enumerate() {
                if let Self::Mark(index) = rung
                    && *index == wanted
                {
                    return Some(slot);
                }
            }
            None
        }

        fn tab(rungs: &[Self], slot: usize) -> Option<usize> {
            match rungs.get(slot)? {
                Self::Mark(index) => Some(*index),
                Self::Gap(_) => None,
            }
        }
    }

    struct Indicator {
        capsule: Retained<UIVisualEffectView>,
        lines: Retained<UIView>,
        glow: Retained<UIView>,
        anchor: Cell<usize>,
    }

    impl Indicator {
        fn over(root_view: &UIView, delegate: &NavDelegate, marker: MainThreadMarker) -> Self {
            let bounds = root_view.bounds();
            let above = root_view.safeAreaInsets().top;
            let width = bounds.size.width;

            let capsule = Pane::glass(MARK_BAR_HEIGHT, marker);
            capsule.setFrame(CGRect {
                origin: CGPoint {
                    x: TAB_BAR_EDGE,
                    y: above + TAB_BAR_GAP,
                },
                size: CGSize {
                    width: width - TAB_BAR_EDGE * 2.0,
                    height: MARK_BAR_HEIGHT,
                },
            });
            capsule.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleBottomMargin,
            );
            capsule.setHidden(true);

            let lines = UIView::initWithFrame(
                UIView::alloc(marker),
                CGRect {
                    origin: CGPoint {
                        x: MARK_INSET,
                        y: 0.0,
                    },
                    size: CGSize {
                        width: width - TAB_BAR_EDGE * 2.0 - MARK_INSET * 2.0,
                        height: MARK_BAR_HEIGHT,
                    },
                },
            );
            lines.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth);
            capsule.contentView().addSubview(&lines);

            let glow = UIView::initWithFrame(UIView::alloc(marker), CGRect::default());
            glow.layer().setCornerRadius(MARK_HEIGHT / 2.0);
            glow.setBackgroundColor(Some(&UIColor::colorWithWhite_alpha(1.0, MARK_LIT)));
            lines.addSubview(&glow);

            let tap = unsafe {
                UITapGestureRecognizer::initWithTarget_action(
                    UITapGestureRecognizer::alloc(marker),
                    Some(delegate),
                    Some(sel!(linesTapped:)),
                )
            };
            capsule.addGestureRecognizer(&tap);

            match root_view.window() {
                Some(window) => window.addSubview(&capsule),
                None => root_view.addSubview(&capsule),
            }
            Self {
                capsule,
                lines,
                glow,
                anchor: Cell::new(0),
            }
        }

        fn show(&self, tabs: usize, index: usize) {
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            self.settle(tabs, index);
            let rungs = self.seats(tabs);
            let wanted = tabs >= 2;
            if wanted && self.capsule.isHidden() {
                self.capsule.setAlpha(0.0);
                self.capsule.setHidden(false);
            }
            self.fill(&rungs, marker);
            let seat = match Rung::slot(&rungs, index) {
                Some(slot) => self.spot(rungs.len(), slot),
                None => self.glow.frame(),
            };
            if self.capsule.alpha() < 0.5 {
                self.glow.setFrame(seat);
            }

            let (capsule, glow) = (self.capsule.clone(), self.glow.clone());
            let dressing = RcBlock::new(move || {
                capsule.setAlpha(if wanted { 1.0 } else { 0.0 });
                glow.setFrame(seat);
            });
            let capsule = self.capsule.clone();
            let dressed = RcBlock::new(move |_| capsule.setHidden(!wanted));
            UIView::animateWithDuration_delay_options_animations_completion(
                MARK_GLIDE,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &dressing,
                Some(&dressed),
                marker,
            );
        }

        fn seats(&self, count: usize) -> Vec<Rung> {
            Rung::over(count, self.anchor.get())
        }

        fn settle(&self, count: usize, at: usize) {
            let room = MARK_MOST - 2;
            if count <= MARK_MOST {
                self.anchor.set(0);
                return;
            }
            let mut start = self.anchor.get().min(count - room);
            if at < start + MARK_EDGE {
                start = at.saturating_sub(MARK_EDGE);
            }
            if at + MARK_EDGE >= start + room {
                start = (at + MARK_EDGE + 1).saturating_sub(room);
            }
            self.anchor.set(start.min(count - room));
        }

        fn fill(&self, rungs: &[Rung], marker: MainThreadMarker) {
            for spent in self.lines.subviews().iter() {
                if std::ptr::eq(&*spent as *const UIView, &*self.glow as *const UIView) {
                    continue;
                }
                spent.removeFromSuperview();
            }
            for (slot, rung) in rungs.iter().enumerate() {
                let seat = self.spot(rungs.len(), slot);
                match rung {
                    Rung::Mark(_) => {
                        let mark = UIView::initWithFrame(UIView::alloc(marker), seat);
                        mark.layer().setCornerRadius(MARK_HEIGHT / 2.0);
                        mark.setBackgroundColor(Some(&UIColor::colorWithWhite_alpha(
                            1.0, MARK_DIM,
                        )));
                        self.lines.addSubview(&mark);
                    }
                    Rung::Gap(hidden) => {
                        let tally = UILabel::initWithFrame(
                            UILabel::alloc(marker),
                            CGRect {
                                origin: CGPoint {
                                    x: seat.origin.x,
                                    y: 0.0,
                                },
                                size: CGSize {
                                    width: seat.size.width,
                                    height: MARK_BAR_HEIGHT,
                                },
                            },
                        );
                        tally.setText(Some(&NSString::from_str(&format!("+{hidden}"))));
                        tally.setTextAlignment(NSTextAlignment(1));
                        unsafe { tally.setFont(Some(&UIFont::systemFontOfSize(MARK_TALLY))) };
                        unsafe {
                            tally.setTextColor(Some(&UIColor::colorWithWhite_alpha(
                                1.0,
                                MARK_DIM * 2.0,
                            )))
                        };
                        self.lines.addSubview(&tally);
                    }
                }
            }
            self.lines.bringSubviewToFront(&self.glow);
        }

        fn spot(&self, slots: usize, slot: usize) -> CGRect {
            let across = self.lines.bounds().size.width;
            if slots == 0 || across <= 0.0 {
                return CGRect::default();
            }
            let width = (across - MARK_GAP * (slots - 1) as f64) / slots as f64;
            CGRect {
                origin: CGPoint {
                    x: slot as f64 * (width + MARK_GAP),
                    y: (MARK_BAR_HEIGHT - MARK_HEIGHT) / 2.0,
                },
                size: CGSize {
                    width,
                    height: MARK_HEIGHT,
                },
            }
        }

        fn track(&self, tabs: usize, from: usize, to: usize, progress: f64) {
            let gone = progress.clamp(0.0, 1.0);
            let rungs = self.seats(tabs);
            let (Some(here), Some(there)) = (Rung::slot(&rungs, from), Rung::slot(&rungs, to))
            else {
                return;
            };
            let (here, there) = (self.spot(rungs.len(), here), self.spot(rungs.len(), there));
            let mut seat = here;
            seat.origin.x = here.origin.x + (there.origin.x - here.origin.x) * gone;
            self.glow.setFrame(seat);
        }

        fn reached(&self, sender: &UITapGestureRecognizer, slots: usize) -> Option<usize> {
            if slots == 0 {
                return None;
            }
            let across = self.lines.bounds().size.width;
            if across <= 0.0 {
                return None;
            }
            let x = sender.locationInView(Some(&self.lines)).x;
            let step = across / slots as f64;
            Some(((x / step).floor().max(0.0) as usize).min(slots - 1))
        }

        fn front(&self) {
            let Some(parent) = self.capsule.superview() else {
                return;
            };
            parent.bringSubviewToFront(&self.capsule);
        }
    }

    enum Retreat {
        Close,
        Pop(Retained<UINavigationController>),
    }

    #[derive(Clone, Copy, PartialEq)]
    struct Slots {
        centre: bool,
        browse: bool,
        back: bool,
    }

    struct Tabs {
        strip: Retained<UIVisualEffectView>,
        back: Retained<UIVisualEffectView>,
        capsule: Retained<UIVisualEffectView>,
        row: Retained<UIStackView>,
        circle: Retained<UIVisualEffectView>,
        browse: Retained<UIVisualEffectView>,
        ids: Vec<String>,
        at: usize,
    }

    impl Tabs {
        fn under(root_view: &UIView, delegate: &NavDelegate, marker: MainThreadMarker) -> Self {
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

            let back = Pane::glass(TAB_BAR_HEIGHT, marker);
            back.setFrame(CGRect {
                origin: CGPoint {
                    x: TAB_BAR_EDGE,
                    y: 0.0,
                },
                size: CGSize {
                    width: TAB_BAR_HEIGHT,
                    height: TAB_BAR_HEIGHT,
                },
            });
            back.setHidden(true);
            Pane::fill(
                &Pane::glyph("chevron.left", delegate, sel!(backTapped:), marker),
                &back,
            );

            let circle = Pane::glass(TAB_BAR_HEIGHT, marker);
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

            let browse = Pane::glass(TAB_BAR_HEIGHT, marker);
            browse.setFrame(CGRect {
                origin: CGPoint {
                    x: width - TAB_BAR_EDGE - TAB_BAR_HEIGHT * 2.0 - TAB_BAR_GAP,
                    y: 0.0,
                },
                size: CGSize {
                    width: TAB_BAR_HEIGHT,
                    height: TAB_BAR_HEIGHT,
                },
            });
            browse.setAutoresizingMask(UIViewAutoresizing::FlexibleLeftMargin);

            let capsule = Pane::glass(TAB_BAR_HEIGHT, marker);
            let across = width - TAB_BAR_EDGE * 2.0 - TAB_BAR_HEIGHT * 2.0 - TAB_BAR_GAP * 2.0;
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
            strip.contentView().addSubview(&back);
            strip.contentView().addSubview(&browse);
            strip.contentView().addSubview(&circle);
            match root_view.window() {
                Some(window) => window.addSubview(&strip),
                None => root_view.addSubview(&strip),
            }
            Self {
                strip,
                back,
                capsule,
                row,
                circle,
                browse,
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
            let closable = entries.len() > 1;
            let mut showing = None;
            for entry in entries {
                if entry.here {
                    self.at = self.ids.len();
                    showing = Some((entry.name, entry.id.clone()));
                }
                self.ids.push(entry.id);
            }
            if let Some((name, id)) = showing {
                let button = UIButton::buttonWithType(UIButtonType::System, marker);
                button.setTitle_forState(Some(&NSString::from_str(&name)), UIControlState::Normal);
                if let Some(label) = button.titleLabel() {
                    unsafe { label.setFont(Some(&UIFont::systemFontOfSize(15.0))) };
                }
                button.setTitleColor_forState(Some(&UIColor::labelColor()), UIControlState::Normal);
                unsafe {
                    button.addTarget_action_forControlEvents(
                        Some(delegate),
                        sel!(tabsTapped:),
                        UIControlEvents::TouchUpInside,
                    );
                }
                if closable {
                    button.setMenu(Some(&Self::menu(&id, marker)));
                }
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
            for spent in self.browse.contentView().subviews().iter() {
                spent.removeFromSuperview();
            }
            let counter = UIButton::buttonWithType(UIButtonType::System, marker);
            match UIImage::systemImageNamed(&NSString::from_str("square.on.square")) {
                Some(glyph) => counter.setImage_forState(Some(&glyph), UIControlState::Normal),
                None => counter.setTitle_forState(
                    Some(&NSString::from_str(&self.ids.len().to_string())),
                    UIControlState::Normal,
                ),
            }
            unsafe { counter.setTintColor(Some(&UIColor::labelColor())) };
            counter.setTitleColor_forState(Some(&UIColor::labelColor()), UIControlState::Normal);
            unsafe {
                counter.addTarget_action_forControlEvents(
                    Some(delegate),
                    sel!(browseTapped:),
                    UIControlEvents::TouchUpInside,
                );
            }
            counter.setFrame(self.browse.bounds());
            counter.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            self.browse.contentView().addSubview(&counter);
            self.capsule.setHidden(self.ids.is_empty());
            self.strip
                .setHidden(self.ids.is_empty() && centre.is_none());

            let wanted = Slots {
                centre: centre.is_some(),
                browse: self.ids.len() >= 2,
                back: !self.back.isHidden(),
            };
            if self.slots() == wanted {
                self.lay_out(wanted);
                return;
            }
            for (pane, shown) in [(&self.circle, wanted.centre), (&self.browse, wanted.browse)] {
                if shown && pane.isHidden() {
                    pane.setAlpha(0.0);
                    pane.setHidden(false);
                }
            }

            let (back, circle, browse) =
                (self.back.clone(), self.circle.clone(), self.browse.clone());
            let (capsule, strip) = (self.capsule.clone(), self.strip.clone());
            let dressing = block2::RcBlock::new(move || {
                circle.setAlpha(if wanted.centre { 1.0 } else { 0.0 });
                browse.setAlpha(if wanted.browse { 1.0 } else { 0.0 });
                Tabs::place(&strip, &capsule, &back, &circle, &browse, wanted);
            });
            let (circle, browse) = (self.circle.clone(), self.browse.clone());
            let dressed = block2::RcBlock::new(move |_| {
                circle.setHidden(!wanted.centre);
                browse.setHidden(!wanted.browse);
            });
            UIView::animateWithDuration_delay_options_animations_completion(
                0.24,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &dressing,
                Some(&dressed),
                marker,
            );
        }

        fn slots(&self) -> Slots {
            Slots {
                centre: !self.circle.isHidden(),
                browse: !self.browse.isHidden(),
                back: !self.back.isHidden(),
            }
        }

        fn reveal(&self, back: bool) {
            let mut wanted = self.slots();
            if wanted.back == back {
                return;
            }
            if back {
                self.back.setAlpha(0.0);
                self.back.setHidden(false);
            }
            wanted.back = back;
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let showing = self.back.clone();
            let (capsule, strip) = (self.capsule.clone(), self.strip.clone());
            let (back_pane, circle, browse) =
                (self.back.clone(), self.circle.clone(), self.browse.clone());
            let dressing = block2::RcBlock::new(move || {
                showing.setAlpha(if back { 1.0 } else { 0.0 });
                Tabs::place(&strip, &capsule, &back_pane, &circle, &browse, wanted);
            });
            let hiding = self.back.clone();
            let dressed = block2::RcBlock::new(move |_| hiding.setHidden(!back));
            UIView::animateWithDuration_delay_options_animations_completion(
                0.24,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &dressing,
                Some(&dressed),
                marker,
            );
        }

        fn lay_out(&self, wanted: Slots) {
            Self::place(
                &self.strip,
                &self.capsule,
                &self.back,
                &self.circle,
                &self.browse,
                wanted,
            );
        }

        fn place(
            strip: &UIVisualEffectView,
            capsule: &UIVisualEffectView,
            back: &UIVisualEffectView,
            circle: &UIVisualEffectView,
            browse: &UIVisualEffectView,
            wanted: Slots,
        ) {
            let width = strip.bounds().size.width;
            let square = CGSize {
                width: TAB_BAR_HEIGHT,
                height: TAB_BAR_HEIGHT,
            };
            let mut right = TAB_BAR_EDGE;
            for (pane, shown) in [(circle, wanted.centre), (browse, wanted.browse)] {
                if !shown {
                    continue;
                }
                pane.setFrame(CGRect {
                    origin: CGPoint {
                        x: width - right - TAB_BAR_HEIGHT,
                        y: 0.0,
                    },
                    size: square,
                });
                right += TAB_BAR_HEIGHT + TAB_BAR_GAP;
            }
            let mut left = TAB_BAR_EDGE;
            if wanted.back {
                back.setFrame(CGRect {
                    origin: CGPoint { x: left, y: 0.0 },
                    size: square,
                });
                left += TAB_BAR_HEIGHT + TAB_BAR_GAP;
            }
            capsule.setFrame(CGRect {
                origin: CGPoint { x: left, y: 0.0 },
                size: CGSize {
                    width: width - left - right,
                    height: TAB_BAR_HEIGHT,
                },
            });
        }

        fn menu(id: &str, marker: MainThreadMarker) -> Retained<UIMenu> {
            let id = id.to_string();
            let shut = RcBlock::new(move |_: std::ptr::NonNull<UIAction>| {
                CLOSING.with_borrow_mut(|slot| *slot = Some(id.clone()));
            });
            let action = unsafe {
                UIAction::actionWithTitle_image_identifier_handler(
                    &NSString::from_str(&vmux_ui::i18n::translate("layout-close-tab")),
                    None,
                    None,
                    RcBlock::as_ptr(&shut),
                    marker,
                )
            };
            action.setAttributes(UIMenuElementAttributes::Destructive);
            let children = NSArray::from_slice(&[action.as_super() as &UIMenuElement]);
            UIMenu::menuWithTitle_children(&NSString::from_str(""), &children, marker)
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

        fn name(&self, text: &str) {
            let Some(showing) = self.row.arrangedSubviews().iter().next() else {
                return;
            };
            let Ok(button) = showing.downcast::<UIButton>() else {
                return;
            };
            button.setTitle_forState(Some(&NSString::from_str(text)), UIControlState::Normal);
        }
    }

    pub struct NativeStack {
        root_controller: Retained<UIViewController>,
        pager: Retained<UIView>,
        rootless: Retained<UIView>,
        backdrop: (u8, u8, u8, u8),
        stacks: HashMap<String, Column>,
        sheets: Vec<Column>,
        kept: HashMap<u64, Column>,
        pending: HashMap<String, Vec<Level>>,
        ghosts: HashMap<String, Parallax>,
        arriving: Option<Parallax>,
        seated: Option<String>,
        dragging: Option<Drag>,
        tabs: Tabs,
        indicator: Indicator,
        delegate: Retained<NavDelegate>,
        dismissal: Retained<Dismissal>,
        overviewing: bool,
        leave: Retained<UITapGestureRecognizer>,
        sweep: Retained<UIPanGestureRecognizer>,
        snaps: HashMap<String, Retained<UIView>>,
        row: Vec<Card>,
    }

    struct Card {
        at: usize,
        view: Retained<UIView>,
        snapshot: bool,
    }

    struct Overview;

    impl Overview {
        fn toggle() {
            let parked = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                Some((Parallax::of(stack)?, !stack.overviewing))
            });
            if let Some((sheets, leaving)) = parked
                && let Some(marker) = MainThreadMarker::new()
            {
                sheets.settle(leaving, marker);
            }
            let plan = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                if stack.tabs.ids.len() < 2 {
                    return None;
                }
                stack.overviewing = !stack.overviewing;
                let on = stack.overviewing;
                stack.leave.setEnabled(on);
                stack.sweep.setEnabled(on);
                stack.pager.setClipsToBounds(!on);
                let places = if on {
                    Self::capture(stack);
                    Self::deal(stack);
                    Self::plan(stack, 0.0)
                } else {
                    for card in &stack.row {
                        if card.at != stack.tabs.at {
                            card.view.setHidden(true);
                        }
                    }
                    Vec::new()
                };
                Some((stack.pager.clone(), places, on))
            });
            let Some((pager, places, on)) = plan else {
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let scale = if on { OVERVIEW_SCALE } else { 1.0 };
            let settled = block2::RcBlock::new(move |_| {
                if !on {
                    Self::clear();
                }
            });
            let settling = block2::RcBlock::new(move || {
                pager.setTransform(Drag::moved(0.0, scale));
                for (view, shape) in &places {
                    view.layer().setTransform(*shape);
                    Overview::round(view, on);
                }
            });
            UIView::animateWithDuration_animations_completion(
                0.3,
                &settling,
                Some(&settled),
                marker,
            );
        }

        fn plate(stack: &NativeStack) -> Option<Retained<UIView>> {
            let marker = MainThreadMarker::new()?;
            let (red, green, blue, alpha) = stack.backdrop;
            let plate = UIView::initWithFrame(UIView::alloc(marker), stack.pager.bounds());
            plate.setBackgroundColor(Some(&UIColor::colorWithRed_green_blue_alpha(
                red as f64 / 255.0,
                green as f64 / 255.0,
                blue as f64 / 255.0,
                alpha as f64 / 255.0,
            )));
            size_to_parent(&plate, &stack.pager);
            stack.pager.addSubview(&plate);
            Some(plate)
        }

        fn capture(stack: &mut NativeStack) {
            let Some(showing) = stack.tabs.ids.get(stack.tabs.at).cloned() else {
                return;
            };
            if let Some(view) = stack
                .stacks
                .get(&showing)
                .and_then(|column| column.navigation.view())
                && !view.isHidden()
                && let Some(shot) = view.snapshotViewAfterScreenUpdates(false)
            {
                stack.snaps.insert(showing, shot);
            }
        }

        fn deal(stack: &mut NativeStack) {
            let mut fresh = Vec::new();
            for (at, id) in stack.tabs.ids.iter().enumerate() {
                let live = stack
                    .stacks
                    .get(id)
                    .and_then(|column| column.navigation.view());
                let (view, snapshot) = match stack.snaps.get(id) {
                    Some(shot) => {
                        if let Some(live) = live.as_ref() {
                            live.setHidden(true);
                        }
                        (shot.clone(), true)
                    }
                    None => match live {
                        Some(view) => (view, false),
                        None => match Self::plate(stack) {
                            Some(plate) => (plate, true),
                            None => continue,
                        },
                    },
                };
                if snapshot && view.superview().is_none() {
                    size_to_parent(&view, &stack.pager);
                    stack.pager.addSubview(&view);
                }
                view.setHidden(false);
                view.setUserInteractionEnabled(false);
                Self::round(&view, true);
                fresh.push(Card { at, view, snapshot });
            }
            for card in stack.row.drain(..) {
                if !card.snapshot {
                    continue;
                }
                let kept = fresh
                    .iter()
                    .any(|other| std::ptr::eq(&*other.view, &*card.view));
                if !kept {
                    card.view.removeFromSuperview();
                }
            }
            stack.row = fresh;
        }

        fn round(view: &UIView, on: bool) {
            let layer = view.layer();
            layer.setMasksToBounds(true);
            layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
            layer.setCornerRadius(match on {
                true => SHEET_CORNER / OVERVIEW_SCALE,
                false => 0.0,
            });
        }

        fn clear() {
            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                for card in stack.row.drain(..) {
                    card.view.setUserInteractionEnabled(true);
                    card.view.layer().setTransform(Self::flat());
                    Self::round(&card.view, false);
                    if card.snapshot {
                        card.view.removeFromSuperview();
                    }
                }
                let showing = stack.tabs.ids.get(stack.tabs.at).cloned();
                for (id, column) in stack.stacks.iter() {
                    let Some(view) = column.navigation.view() else {
                        continue;
                    };
                    view.layer().setTransform(Self::flat());
                    view.setTransform(Drag::sideways(0.0));
                    view.setUserInteractionEnabled(true);
                    view.setHidden(Some(id) != showing.as_ref());
                }
            });
        }

        fn plan(stack: &NativeStack, shifted: f64) -> Vec<(Retained<UIView>, CATransform3D)> {
            Self::plan_from(stack, stack.tabs.at, shifted)
        }

        fn plan_from(
            stack: &NativeStack,
            at: usize,
            shifted: f64,
        ) -> Vec<(Retained<UIView>, CATransform3D)> {
            let width = stack.pager.bounds().size.width;
            let step = width + OVERVIEW_GAP / OVERVIEW_SCALE;
            let mut places = Vec::new();
            for card in &stack.row {
                let delta = card.at as f64 - at as f64 + shifted / step;
                card.view.layer().setZPosition(-delta.abs());
                places.push((card.view.clone(), Self::tilt(delta, width)));
            }
            places
        }

        fn tilt(delta: f64, width: f64) -> CATransform3D {
            let near = delta.clamp(-1.0, 1.0);
            let (sin, cos) = (-near * OVERVIEW_TILT).sin_cos();
            let scale = 1.0 - 0.18 * near.abs();
            let x = near * width * OVERVIEW_SPREAD + (delta - near) * width * OVERVIEW_TIGHT;
            let z = -OVERVIEW_DEPTH * near.abs();
            CATransform3D {
                m11: cos * scale,
                m12: 0.0,
                m13: sin * scale,
                m14: sin * scale * OVERVIEW_EYE,
                m21: 0.0,
                m22: scale,
                m23: 0.0,
                m24: 0.0,
                m31: -sin * scale,
                m32: 0.0,
                m33: cos * scale,
                m34: cos * scale * OVERVIEW_EYE,
                m41: x,
                m42: 0.0,
                m43: z,
                m44: z * OVERVIEW_EYE + 1.0,
            }
        }

        fn flat() -> CATransform3D {
            CATransform3D {
                m11: 1.0,
                m12: 0.0,
                m13: 0.0,
                m14: 0.0,
                m21: 0.0,
                m22: 1.0,
                m23: 0.0,
                m24: 0.0,
                m31: 0.0,
                m32: 0.0,
                m33: 1.0,
                m34: 0.0,
                m41: 0.0,
                m42: 0.0,
                m43: 0.0,
                m44: 1.0,
            }
        }

        fn follow(shifted: f64) {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                let step = stack.pager.bounds().size.width + OVERVIEW_GAP / OVERVIEW_SCALE;
                let travelled = -shifted / OVERVIEW_SCALE;
                let last = stack.tabs.ids.len().saturating_sub(1) as f64;
                let reached = stack.tabs.at as f64 - travelled / step;
                let held = if reached < 0.0 {
                    reached * OVERVIEW_RESIST
                } else if reached > last {
                    last + (reached - last) * OVERVIEW_RESIST
                } else {
                    reached
                };
                let eased = (stack.tabs.at as f64 - held) * step;
                for (view, shape) in Self::plan(stack, eased) {
                    view.layer().setTransform(shape);
                }
            });
        }

        fn release(shifted: f64, speed: f64) {
            let plan = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let step = stack.pager.bounds().size.width + OVERVIEW_GAP / OVERVIEW_SCALE;
                let travelled = -shifted / OVERVIEW_SCALE;
                let coasted = travelled - speed * OVERVIEW_GLIDE / OVERVIEW_SCALE;
                let mut hops = -(coasted / step).round() as isize;
                let flicked = speed.abs() > 80.0 && travelled.abs() > 3.0;
                if hops == 0 && (travelled.abs() > step / 16.0 || flicked) {
                    hops = if coasted < 0.0 { 1 } else { -1 };
                }
                let last = stack.tabs.ids.len() as isize - 1;
                let landing = (stack.tabs.at as isize + hops).clamp(0, last);
                if landing as usize != stack.tabs.at {
                    let id = stack.tabs.ids[landing as usize].clone();
                    PICKED.with_borrow_mut(|slot| *slot = Some(id));
                    return None;
                }
                Some((Self::plan(stack, 0.0), 0usize))
            });
            let Some((places, moved)) = plan else {
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let settling = block2::RcBlock::new(move || {
                for (view, shape) in &places {
                    view.layer().setTransform(*shape);
                }
            });
            let gliding = (0.42 + 0.11 * moved as f64).min(1.4);
            UIView::animateWithDuration_delay_options_animations_completion(
                gliding,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &settling,
                None,
                marker,
            );
        }
    }

    impl NativeStack {
        pub fn seat(tab: String, levels: Vec<Level>) {
            let departing = STACK.with_borrow(|stack| Parallax::of(stack.as_ref()?));
            let vacated = STACK.with_borrow(|stack| stack.as_ref()?.seated.clone());
            if let Some(departing) = departing.as_ref() {
                departing.haunt(vacated.clone());
            }
            let (pushed, presented) = Self::split(levels);
            Self::raise(&tab, pushed);
            let plan = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let arriving = stack.stacks.get(&tab)?.navigation.view()?;
                stack.pager.bringSubviewToFront(&arriving);
                stack.rootless.setHidden(true);
                if stack.overviewing {
                    stack.seated = Some(tab.clone());
                    arriving.setHidden(false);
                    return None;
                }
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
                if let Some(view) = leaving.as_ref()
                    && !view.isHidden()
                    && let Some(shot) = view.snapshotViewAfterScreenUpdates(false)
                {
                    stack.snaps.insert(vacated.clone(), shot);
                }
                arriving.setTransform(Drag::sideways(entering * TAB_TRAIL));
                arriving.setHidden(false);
                if let Some(leaving) = leaving.as_ref() {
                    stack.pager.bringSubviewToFront(leaving);
                }
                Some((leaving, arriving, entering))
            });
            Self::front();
            if Self::already(&presented) {
                return;
            }
            let Some((leaving, arriving, entering)) = plan else {
                Self::shed(move || Self::present_all(presented, Entry::Fresh));
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let vacating = leaving.clone();
            let done = block2::RcBlock::new(move |_| {
                let Some(vacating) = vacating.as_ref() else {
                    return;
                };
                vacating.setHidden(true);
                vacating.setTransform(Drag::sideways(0.0));
            });
            Drag::glide(leaving, arriving, 0.0, entering, -entering, done, marker);
            let Some(departing) = departing else {
                Self::shed(move || Self::present_all(presented, Entry::Rising));
                return;
            };
            departing.depart(marker, move || {
                NativeStack::present_all(presented, Entry::Rising);
            });
        }

        fn split(levels: Vec<Level>) -> (Vec<Level>, Vec<Level>) {
            let mut pushed = Vec::new();
            let mut presented = Vec::new();
            for level in levels {
                if presented.is_empty() && level.presentation.pushes() {
                    pushed.push(level);
                    continue;
                }
                presented.push(level);
            }
            (pushed, presented)
        }

        fn already(wanted: &[Level]) -> bool {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return false;
                };
                if stack.sheets.len() != wanted.len() {
                    return false;
                }
                for (column, level) in stack.sheets.iter().zip(wanted) {
                    if column.key != level.key {
                        return false;
                    }
                }
                !wanted.is_empty()
            })
        }

        pub fn warm(wanted: Vec<(String, Vec<Level>)>) {
            let mut keep = Vec::new();
            for (tab, levels) in wanted {
                let (pushed, presented) = Self::split(levels);
                Self::raise(&tab, pushed);
                STACK.with_borrow_mut(|stack| {
                    let Some(stack) = stack.as_mut() else {
                        return;
                    };
                    stack.pending.insert(tab.clone(), presented);
                });
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
                    if let Some(view) = column.navigation.view()
                        && !view.isHidden()
                        && let Some(shot) = view.snapshotViewAfterScreenUpdates(false)
                    {
                        stack.snaps.insert(id.clone(), shot);
                    }
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

            let settling = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                if !stack.overviewing {
                    return None;
                }
                let marker = MainThreadMarker::new()?;
                let before = stack.row.len();
                let entered = stack.tabs.at.saturating_sub(1);
                Overview::deal(stack);
                if stack.row.len() == before {
                    for (view, shape) in Overview::plan(stack, 0.0) {
                        view.layer().setTransform(shape);
                    }
                    return None;
                }
                for (view, shape) in Overview::plan_from(stack, entered, 0.0) {
                    view.layer().setTransform(shape);
                }
                Some((Overview::plan(stack, 0.0), marker))
            });
            let Some((places, marker)) = settling else {
                return;
            };
            let sliding = block2::RcBlock::new(move || {
                for (view, shape) in &places {
                    view.layer().setTransform(*shape);
                }
            });
            UIView::animateWithDuration_delay_options_animations_completion(
                0.22,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &sliding,
                None,
                marker,
            );
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
                let Some(drawn) = Held::draw(level, &pager, backdrop, marker) else {
                    continue;
                };
                held.push(drawn);
            }
            if held.is_empty() {
                return;
            }
            let mut column = Column::over(0, held.remove(0), &delegate, marker);
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
                Self::chrome();
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
            Self::chrome();
        }

        pub fn present(level: Level) {
            Self::present_all(vec![level], Entry::Fresh);
        }

        fn present_all(mut levels: Vec<Level>, entry: Entry) {
            if levels.is_empty() {
                Self::chrome();
                match entry {
                    Entry::Rising => Parallax::arrive(),
                    Entry::Seated => Parallax::unghost(),
                    Entry::Fresh => {}
                }
                return;
            }
            let level = levels.remove(0);
            let (key, presentation, detents) = (level.key, level.presentation, level.detents);
            let reused = STACK.with_borrow_mut(|stack| stack.as_mut()?.kept.remove(&key));
            if let Some(column) = reused {
                Self::mount(column, presentation, detents, levels, entry);
                return;
            }
            let drawn = Self::draw(level);
            after_paint(move || {
                let Some(drawn) = drawn else {
                    return;
                };
                let built = STACK.with_borrow(|stack| {
                    let stack = stack.as_ref()?;
                    let marker = MainThreadMarker::new()?;
                    Some(Column::over(key, drawn, &stack.delegate, marker))
                });
                let Some(column) = built else {
                    return;
                };
                NativeStack::mount(column, presentation, detents, levels, entry);
            });
        }

        fn chrome() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                stack.tabs.reveal(Self::retreats(stack));
                if let Some(showing) = Self::showing(stack) {
                    stack.tabs.name(showing);
                }
                let sheets = stack.sheets.len();
                let lines = match sheets > 1 {
                    true => sheets,
                    false => stack.tabs.ids.len(),
                };
                match sheets > 1 {
                    true => stack.indicator.show(sheets, sheets - 1),
                    false => stack.indicator.show(lines, stack.tabs.at),
                }
                let clearance = match lines > 1 {
                    true => MARK_CLEARANCE,
                    false => 0.0,
                };
                let mut clearing = Vec::new();
                for column in stack.stacks.values().chain(stack.sheets.iter()) {
                    clearing.push(column.navigation.clone());
                }
                let Some(marker) = MainThreadMarker::new() else {
                    return;
                };
                let sliding = RcBlock::new(move || {
                    for navigation in &clearing {
                        Column::inset(navigation, clearance);
                    }
                });
                UIView::animateWithDuration_delay_options_animations_completion(
                    MARK_GLIDE,
                    0.0,
                    UIViewAnimationOptions::CurveEaseOut,
                    &sliding,
                    None,
                    marker,
                );
            });
        }

        fn showing(stack: &NativeStack) -> Option<&str> {
            let column = match stack.sheets.last() {
                Some(sheet) => sheet,
                None => stack.stacks.get(stack.seated.as_ref()?)?,
            };
            Some(column.levels.last()?.title.as_str())
        }

        fn retreats(stack: &NativeStack) -> bool {
            if !stack.sheets.is_empty() {
                return true;
            }
            let Some(seated) = stack.seated.as_ref() else {
                return false;
            };
            stack
                .stacks
                .get(seated)
                .is_some_and(|column| column.levels.len() > 1)
        }

        fn retreat() {
            let leaving = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                if !stack.sheets.is_empty() {
                    return Some(Retreat::Close);
                }
                let seated = stack.seated.as_ref()?;
                let column = stack.stacks.get(seated)?;
                match column.levels.len() > 1 {
                    true => Some(Retreat::Pop(column.navigation.clone())),
                    false => None,
                }
            });
            match leaving {
                Some(Retreat::Close) => CLOSED.set(true),
                Some(Retreat::Pop(navigation)) => {
                    let _ = navigation.popViewControllerAnimated(true);
                }
                None => {}
            }
        }

        fn mount(
            mut column: Column,
            presentation: Presentation,
            detents: &'static [f64],
            rest: Vec<Level>,
            entry: Entry,
        ) {
            if let Some(view) = column.navigation.view() {
                view.setTransform(Drag::sideways(0.0));
                view.setHidden(false);
                view.setAlpha(if entry.sinks() { 0.0 } else { 1.0 });
            }
            let pending = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let marker = MainThreadMarker::new()?;
                let presenter = Self::topmost(stack)?.navigation.clone();
                let sheet = column.navigation.clone();
                sheet.setModalPresentationStyle(match presentation {
                    Presentation::FullScreenModal | Presentation::TransparentModal => {
                        UIModalPresentationStyle::OverFullScreen
                    }
                    Presentation::FormSheet => UIModalPresentationStyle::FormSheet,
                    Presentation::Modal | Presentation::Card => UIModalPresentationStyle::PageSheet,
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
                if !column.watched
                    && let Some(view) = sheet.view()
                {
                    stack.dismissal.clone().watch(&view, marker);
                    column.watched = true;
                }
                stack.sheets.push(column);
                Some((presenter, sheet))
            });
            let Some((presenter, sheet)) = pending else {
                return;
            };
            if entry == Entry::Fresh {
                Parallax::recede(&presenter, false);
            }
            let queued = RefCell::new(Some(rest));
            let raised = RcBlock::new(move || {
                if entry.sinks() {
                    Parallax::sink();
                }
                Parallax::atop();
                NativeStack::front();
                if let Some(rest) = queued.borrow_mut().take() {
                    NativeStack::present_all(rest, entry);
                }
            });
            presenter.presentViewController_animated_completion(
                &sheet,
                entry == Entry::Fresh,
                Some(&raised),
            );
            NativeStack::ride(&sheet);
            Parallax::atop();
            NativeStack::front();
            next_turn(|| {
                Parallax::atop();
                NativeStack::front();
            });
            if entry.sinks() {
                Parallax::sink();
                next_turn(Parallax::sink);
            }
        }

        pub fn dismiss() {
            let leaving = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let departing = stack.sheets.pop()?;
                let presenter = Self::topmost(stack).map(|column| column.navigation.clone());
                Some((departing, presenter))
            });
            let Some((departing, presenter)) = leaving else {
                return;
            };
            if let Some(presenter) = presenter {
                Parallax::recede(&presenter, true);
            }
            departing
                .navigation
                .dismissViewControllerAnimated_completion(true, None);
            Self::chrome();
            Self::front();
        }

        fn shed(then: impl FnOnce() + 'static) {
            let sheets = STACK.with_borrow_mut(|stack| match stack.as_mut() {
                Some(stack) => std::mem::take(&mut stack.sheets),
                None => Vec::new(),
            });
            let base = match sheets.first() {
                Some(first) => first.navigation.presentingViewController(),
                None => None,
            };
            STACK.with_borrow_mut(|stack| {
                let Some(stack) = stack.as_mut() else {
                    return;
                };
                for column in sheets {
                    stack.kept.insert(column.key, column);
                }
            });
            let Some(base) = base else {
                then();
                return;
            };
            Parallax::recede(&base, true);
            let next = RefCell::new(Some(then));
            let shed = RcBlock::new(move || {
                let Some(next) = next.borrow_mut().take() else {
                    return;
                };
                next();
            });
            base.dismissViewControllerAnimated_completion(false, Some(&shed));
        }

        pub fn tabs(entries: Vec<TabItem>, centre: Option<&'static str>) {
            let settling = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let marker = MainThreadMarker::new()?;
                let delegate = stack.delegate.clone();
                stack.tabs.show(entries, centre, &delegate, marker);
                stack.tabs.front();
                stack.indicator.front();
                if !stack.overviewing {
                    return None;
                }
                Overview::deal(stack);
                Some((Overview::plan(stack, 0.0), marker))
            });
            Self::chrome();
            let Some((places, marker)) = settling else {
                return;
            };
            let sliding = block2::RcBlock::new(move || {
                for (view, shape) in &places {
                    view.layer().setTransform(*shape);
                }
            });
            UIView::animateWithDuration_delay_options_animations_completion(
                0.45,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &sliding,
                None,
                marker,
            );
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

        fn beneath(stack: &NativeStack) -> Option<Retained<UIView>> {
            if let Some(under) = stack.sheets.len().checked_sub(2) {
                return stack.sheets[under].navigation.view();
            }
            let seated = stack.seated.as_ref()?;
            stack.stacks.get(seated)?.navigation.view()
        }

        fn ride(sheet: &UINavigationController) {
            let Some(coordinator) = sheet.transitionCoordinator() else {
                return;
            };
            let raising = RcBlock::new(
                move |_: std::ptr::NonNull<
                    objc2::runtime::ProtocolObject<
                        dyn UIViewControllerTransitionCoordinatorContext,
                    >,
                >| {
                    NativeStack::front();
                },
            );
            let raised = RcBlock::new(
                move |_: std::ptr::NonNull<
                    objc2::runtime::ProtocolObject<
                        dyn UIViewControllerTransitionCoordinatorContext,
                    >,
                >| {
                    NativeStack::front();
                },
            );
            coordinator.animateAlongsideTransition_completion(Some(&raising), Some(&raised));
        }

        fn front() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                stack.tabs.front();
                stack.indicator.front();
            });
        }

        fn draw(level: Level) -> Option<Held> {
            let (pager, backdrop, marker) = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                Some((
                    stack.pager.clone(),
                    stack.backdrop,
                    MainThreadMarker::new()?,
                ))
            })?;
            Held::draw(level, &pager, backdrop, marker)
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

    #[derive(Clone, Copy, PartialEq)]
    enum Entry {
        Fresh,
        Rising,
        Seated,
    }

    impl Entry {
        fn sinks(self) -> bool {
            self == Entry::Rising
        }
    }

    struct Drag {
        to: String,
        incoming: Column,
        entering: f64,
        across: f64,
        at: Cell<f64>,
        sheet: Option<Parallax>,
        rising: Option<Parallax>,
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
                drag.at.set(travelled.abs() / drag.across);
                if let Some(leaving) = Self::leaving(stack) {
                    leaving.setTransform(Self::moved(travelled, scale));
                }
                if let Some(arriving) = drag.incoming.navigation.view() {
                    arriving
                        .setTransform(Self::moved((travelled + drag.entering) * TAB_TRAIL, scale));
                }
                if let Some(sheet) = drag.sheet.as_ref() {
                    sheet.follow(travelled.abs() / drag.across);
                }
                if let Some(rising) = drag.rising.as_ref() {
                    rising.follow(1.0 - travelled.abs() / drag.across);
                }
                let mut landing = None;
                for (index, id) in stack.tabs.ids.iter().enumerate() {
                    if *id == drag.to {
                        landing = Some(index);
                    }
                }
                if let Some(landing) = landing {
                    stack.indicator.track(
                        stack.tabs.ids.len(),
                        stack.tabs.at,
                        landing,
                        travelled.abs() / drag.across,
                    );
                }
            });
        }

        fn begin(shifted: f64) -> Option<Drag> {
            STACK.with_borrow_mut(|stack| {
                let sheet = Parallax::of(stack.as_ref()?);
                let to = stack.as_ref()?.tabs.neighbour(shifted)?;
                let stack = stack.as_mut()?;
                let incoming = stack.stacks.remove(&to)?;
                let across = stack.pager.bounds().size.width;
                let entering = if shifted > 0.0 { across } else { -across };
                let view = incoming.navigation.view()?;
                view.setTransform(Self::sideways(entering * TAB_TRAIL));
                view.setHidden(false);
                if let Some(leaving) = Self::leaving(stack) {
                    stack.pager.bringSubviewToFront(&leaving);
                }
                if let Some(sheet) = sheet.as_ref()
                    && let Some(seated) = stack.seated.clone()
                    && let Some(ghosts) = sheet.ghosts()
                    && let Some(stale) = stack.ghosts.insert(seated, ghosts)
                {
                    stale.discard();
                }
                let parked = stack
                    .pending
                    .get(&to)
                    .is_some_and(|levels| !levels.is_empty());
                let rising = match parked {
                    true => stack.ghosts.remove(&to),
                    false => None,
                };
                if let Some(rising) = rising.as_ref() {
                    rising.follow(1.0);
                    rising.show();
                }
                Some(Drag {
                    to,
                    incoming,
                    entering,
                    across,
                    at: Cell::new(0.0),
                    sheet,
                    rising,
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
                let sheet = drag.sheet.clone();
                Some((
                    leaving,
                    arriving,
                    landing,
                    drag.entering,
                    commit,
                    from,
                    sheet,
                ))
            });
            let Some((leaving, arriving, landing, entering, commit, from, sheet)) = plan else {
                return;
            };
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            if let Some(sheet) = sheet {
                sheet.settle(commit, marker);
            }
            let rising =
                STACK.with_borrow(|stack| stack.as_ref()?.dragging.as_ref()?.rising.clone());
            if let Some(rising) = rising.as_ref() {
                rising.settle(!commit, marker);
            }
            if commit {
                Self::hand_over();
            }
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

        fn hand_over() {
            let arriving = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let drag = stack.dragging.as_ref()?;
                let to = drag.to.clone();
                stack.arriving = drag.rising.clone();
                let mut landed = None;
                for (index, id) in stack.tabs.ids.iter().enumerate() {
                    if *id == to {
                        landed = Some(index);
                    }
                }
                if let Some(landed) = landed {
                    stack.tabs.at = landed;
                }
                stack.pending.remove(&to)
            });
            let standing = STACK
                .with_borrow(|stack| stack.as_ref().is_some_and(|stack| stack.arriving.is_some()));
            NativeStack::shed(move || {
                let Some(arriving) = arriving else {
                    Parallax::unghost();
                    return;
                };
                let entry = match standing {
                    true => Entry::Seated,
                    false => Entry::Rising,
                };
                NativeStack::present_all(arriving, entry);
            });
        }

        fn land(commit: bool) {
            let arrived = STACK.with_borrow_mut(|stack| {
                let stack = stack.as_mut()?;
                let drag = stack.dragging.take()?;
                if !commit && let Some(rising) = drag.rising {
                    rising.stow();
                    stack.ghosts.insert(drag.to.clone(), rising);
                }
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
            NativeStack::chrome();
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
                    arriving.setTransform(Drag::moved((midway + entering) * TAB_TRAIL, 1.0 - DIP));
                });
                UIView::addKeyframeWithRelativeStartTime_relativeDuration_animations(
                    0.0, 0.5, &dip, marker,
                );
                let (leaving, arriving) = (dipping.clone(), dipped.clone());
                let rise = block2::RcBlock::new(move || {
                    if let Some(leaving) = leaving.as_ref() {
                        leaving.setTransform(Drag::sideways(landing));
                    }
                    arriving.setTransform(Drag::sideways((landing + entering) * TAB_TRAIL));
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

    #[derive(Clone)]
    struct Parallax {
        views: Vec<Retained<UIView>>,
        fall: f64,
    }

    impl Parallax {
        fn of(stack: &NativeStack) -> Option<Parallax> {
            let mut views = Vec::new();
            for sheet in &stack.sheets {
                let Some(view) = sheet.navigation.view() else {
                    continue;
                };
                let Some(container) = Self::container(view) else {
                    continue;
                };
                views.push(container);
            }
            if views.is_empty() {
                return None;
            }
            let fall = stack.pager.bounds().size.height;
            Some(Parallax { views, fall })
        }

        fn container(view: Retained<UIView>) -> Option<Retained<UIView>> {
            let mut top = view;
            loop {
                let parent = top.superview()?;
                if parent.superview().is_none() {
                    return Some(top);
                }
                top = parent;
            }
        }

        fn ghosts(&self) -> Option<Parallax> {
            let mut views = Vec::new();
            for view in &self.views {
                let Some(window) = view.window() else {
                    continue;
                };
                let Some(shot) = view.snapshotViewAfterScreenUpdates(false) else {
                    continue;
                };
                shot.setFrame(window.convertRect_fromView(view.bounds(), Some(view)));
                shot.setHidden(true);
                window.addSubview(&shot);
                views.push(shot);
            }
            if views.is_empty() {
                return None;
            }
            Some(Parallax {
                views,
                fall: self.fall,
            })
        }

        fn haunt(&self, owner: Option<String>) {
            let Some(owner) = owner else {
                return;
            };
            let Some(ghosts) = self.ghosts() else {
                return;
            };
            let stale = STACK.with_borrow_mut(|stack| stack.as_mut()?.ghosts.insert(owner, ghosts));
            if let Some(stale) = stale {
                stale.discard();
            }
        }

        fn atop() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                let Some(arriving) = stack.arriving.as_ref() else {
                    return;
                };
                arriving.show();
            });
        }

        fn show(&self) {
            for view in &self.views {
                if let Some(parent) = view.superview() {
                    parent.bringSubviewToFront(view);
                }
                view.setHidden(false);
            }
        }

        fn stow(&self) {
            for view in &self.views {
                view.setHidden(true);
            }
        }

        fn discard(&self) {
            for view in &self.views {
                view.removeFromSuperview();
            }
        }

        fn unghost() {
            let spent = STACK.with_borrow_mut(|stack| stack.as_mut()?.arriving.take());
            let Some(spent) = spent else {
                return;
            };
            Self::opaque();
            spent.discard();
        }

        fn follow(&self, progress: f64) {
            let gone = progress.clamp(0.0, 1.0);
            for view in &self.views {
                view.setTransform(Self::fallen(gone * self.fall));
                view.setAlpha(1.0 - gone * SHEET_FADE);
            }
        }

        fn arrive() {
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let Some(risen) = STACK.with_borrow(|stack| Parallax::of(stack.as_ref()?)) else {
                return;
            };
            risen.follow(1.0);
            Self::opaque();
            let rising = risen.clone();
            let motion = block2::RcBlock::new(move || rising.follow(0.0));
            UIView::animateWithDuration_delay_options_animations_completion(
                0.34,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &motion,
                None,
                marker,
            );
        }

        fn recede(presenter: &UIViewController, back: bool) {
            let Some(view) = presenter.view() else {
                return;
            };
            Self::glide_to(&view, if back { 1.0 } else { SHEET_RECEDE });
        }

        fn dress(view: &UIView, scale: f64) {
            view.setTransform(Drag::moved(0.0, scale));
            let sunk = ((1.0 - scale) / (1.0 - SHEET_RECEDE)).clamp(0.0, 1.0);
            let layer = view.layer();
            layer.setMasksToBounds(true);
            layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
            layer.setCornerRadius(SHEET_CORNER * sunk);
        }

        fn glide_to(view: &UIView, scale: f64) {
            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let view = view.retain();
            let motion = block2::RcBlock::new(move || {
                Parallax::dress(&view, scale);
            });
            UIView::animateWithDuration_delay_options_animations_completion(
                0.34,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &motion,
                None,
                marker,
            );
        }

        fn sink() {
            let placed = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                let risen = match stack.dragging.as_ref() {
                    Some(drag) => drag.at.get(),
                    None => 0.0,
                };
                Some((Parallax::of(stack)?, risen))
            });
            let Some((sunk, risen)) = placed else {
                return;
            };
            sunk.follow(1.0 - risen);
            Self::opaque();
        }

        fn opaque() {
            STACK.with_borrow(|stack| {
                let Some(stack) = stack.as_ref() else {
                    return;
                };
                for column in &stack.sheets {
                    let Some(view) = column.navigation.view() else {
                        continue;
                    };
                    view.setAlpha(1.0);
                }
            });
        }

        fn depart(&self, marker: MainThreadMarker, then: impl FnOnce() + 'static) {
            let falling = self.clone();
            let motion = RcBlock::new(move || falling.follow(1.0));
            let leaving = RefCell::new(Some(then));
            let done = RcBlock::new(move |_| {
                let Some(then) = leaving.borrow_mut().take() else {
                    return;
                };
                NativeStack::shed(then);
            });
            UIView::animateWithDuration_delay_options_animations_completion(
                0.34,
                0.0,
                UIViewAnimationOptions::CurveEaseIn,
                &motion,
                Some(&done),
                marker,
            );
        }

        fn settle(&self, commit: bool, marker: MainThreadMarker) {
            let landing = if commit { 1.0 } else { 0.0 };
            let settling = self.clone();
            let motion = block2::RcBlock::new(move || settling.follow(landing));
            UIView::animateWithDuration_delay_options_animations_completion(
                0.34,
                0.0,
                UIViewAnimationOptions::CurveEaseOut,
                &motion,
                None,
                marker,
            );
        }

        fn fallen(by: f64) -> CGAffineTransform {
            CGAffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                tx: 0.0,
                ty: by,
            }
        }
    }

    struct Slide {
        sheet: Retained<UINavigationController>,
        moving: Retained<UIView>,
        under: Retained<UIView>,
        resting: f64,
        fall: f64,
    }

    impl Slide {
        fn starting() -> Option<Self> {
            let (sheet, moving, under) = STACK.with_borrow(|stack| {
                let stack = stack.as_ref()?;
                let sheet = stack.sheets.last()?.navigation.clone();
                let moving = sheet.presentationController()?.presentedView()?;
                let under = NativeStack::beneath(stack)?;
                Some((sheet, moving, under))
            })?;
            let window = moving.window()?;
            let resting = moving.frame().origin.y;
            let fall = window.bounds().size.height - resting;
            if fall <= 0.0 {
                return None;
            }
            Some(Self {
                sheet,
                moving,
                under,
                resting,
                fall,
            })
        }

        fn fallen(&self) -> f64 {
            ((self.moving.frame().origin.y - self.resting) / self.fall).clamp(0.0, 1.0)
        }

        fn scale(&self) -> f64 {
            SHEET_RECEDE + (1.0 - SHEET_RECEDE) * self.fallen()
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxSheetDismissal"]
        struct Dismissal;

        impl Dismissal {
            #[unsafe(method(dragged:))]
            fn dragged(&self, sender: &UIPanGestureRecognizer) {
                match sender.state() {
                    UIGestureRecognizerState::Began => Self::begin(),
                    UIGestureRecognizerState::Changed => Self::follow(),
                    UIGestureRecognizerState::Ended
                    | UIGestureRecognizerState::Cancelled
                    | UIGestureRecognizerState::Failed => Self::release(),
                    _ => {}
                }
            }
        }

        unsafe impl NSObjectProtocol for Dismissal {}

        unsafe impl UIGestureRecognizerDelegate for Dismissal {
            #[unsafe(method(gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:))]
            fn simultaneous(
                &self,
                _gesture: &UIGestureRecognizer,
                _other: &UIGestureRecognizer,
            ) -> bool {
                true
            }
        }
    );

    impl Dismissal {
        fn new(marker: MainThreadMarker) -> Retained<Self> {
            unsafe { objc2::msg_send![Self::alloc(marker), init] }
        }

        fn watch(&self, sheet: &UIView, marker: MainThreadMarker) {
            let pan = unsafe {
                UIPanGestureRecognizer::initWithTarget_action(
                    UIPanGestureRecognizer::alloc(marker),
                    Some(self),
                    Some(sel!(dragged:)),
                )
            };
            pan.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(self)));
            sheet.addGestureRecognizer(&pan);
        }

        fn begin() {
            SLIDING.with_borrow_mut(|held| *held = Slide::starting());
        }

        fn follow() {
            SLIDING.with_borrow(|held| {
                let Some(slide) = held.as_ref() else {
                    return;
                };
                Parallax::dress(&slide.under, slide.scale());
            });
        }

        fn release() {
            let Some(slide) = SLIDING.with_borrow_mut(Option::take) else {
                return;
            };
            next_turn(move || {
                let Some(coordinator) = slide.sheet.transitionCoordinator() else {
                    Parallax::glide_to(&slide.under, SHEET_RECEDE);
                    return;
                };
                let leaving = slide.under.clone();
                let riding = RcBlock::new(
                    move |_: std::ptr::NonNull<
                        objc2::runtime::ProtocolObject<
                            dyn UIViewControllerTransitionCoordinatorContext,
                        >,
                    >| {
                        Parallax::dress(&leaving, 1.0);
                    },
                );
                let staying = slide.under.clone();
                let landed = RcBlock::new(
                    move |context: std::ptr::NonNull<
                        objc2::runtime::ProtocolObject<
                            dyn UIViewControllerTransitionCoordinatorContext,
                        >,
                    >| {
                        if unsafe { context.as_ref() }.isCancelled() {
                            Parallax::glide_to(&staying, SHEET_RECEDE);
                        }
                    },
                );
                coordinator.animateAlongsideTransition_completion(Some(&riding), Some(&landed));
            });
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "VmuxNavigationDelegate"]
        struct NavDelegate;

        impl NavDelegate {
            #[unsafe(method(backTapped:))]
            fn back_tapped(&self, _sender: &UIButton) {
                NativeStack::retreat();
            }

            #[unsafe(method(linesTapped:))]
            fn lines_tapped(&self, sender: &UITapGestureRecognizer) {
                STACK.with_borrow(|stack| {
                    let Some(stack) = stack.as_ref() else {
                        return;
                    };
                    let sheets = stack.sheets.len();
                    if sheets > 1 {
                        let rungs = stack.indicator.seats(sheets);
                        let Some(hit) = stack.indicator.reached(sender, rungs.len()) else {
                            return;
                        };
                        let Some(wanted) = Rung::tab(&rungs, hit) else {
                            return;
                        };
                        TAPPED.with_borrow_mut(|queued| {
                            for _ in 0..sheets - 1 - wanted.min(sheets - 1) {
                                queued.push(super::ROTATE);
                            }
                        });
                        return;
                    }
                    let tabs = stack.tabs.ids.len();
                    let rungs = stack.indicator.seats(tabs);
                    let Some(hit) = stack.indicator.reached(sender, rungs.len()) else {
                        return;
                    };
                    let Some(wanted) = Rung::tab(&rungs, hit) else {
                        return;
                    };
                    let Some(id) = stack.tabs.ids.get(wanted) else {
                        return;
                    };
                    if wanted == stack.tabs.at {
                        return;
                    }
                    PICKED.with_borrow_mut(|slot| *slot = Some(id.clone()));
                });
            }

            #[unsafe(method(centreTapped:))]
            fn centre_tapped(&self, sender: &UIButton) {
                let Some(action) = Bar::recall(sender.tag()) else {
                    return;
                };
                TAPPED.with_borrow_mut(|queued| queued.push(action));
            }

            #[unsafe(method(tabsTapped:))]
            fn tabs_tapped(&self, _sender: &UIButton) {
                Overview::toggle();
            }

            #[unsafe(method(browseTapped:))]
            fn browse_tapped(&self, _sender: &UIButton) {
                Overview::toggle();
            }

            #[unsafe(method(pagerTapped:))]
            fn pager_tapped(&self, _sender: &UITapGestureRecognizer) {
                Overview::toggle();
            }

            #[unsafe(method(panned:))]
            fn panned(&self, sender: &UIPanGestureRecognizer) {
                let shifted = -sender.translationInView(None).x;
                let speed = -sender.velocityInView(None).x;
                let overviewing =
                    STACK.with_borrow(|stack| stack.as_ref().is_some_and(|stack| stack.overviewing));
                match sender.state() {
                    UIGestureRecognizerState::Changed if overviewing => Overview::follow(shifted),
                    UIGestureRecognizerState::Ended if overviewing => {
                        Overview::release(shifted, speed)
                    }
                    UIGestureRecognizerState::Cancelled | UIGestureRecognizerState::Failed
                        if overviewing =>
                    {
                        Overview::release(0.0, 0.0)
                    }
                    UIGestureRecognizerState::Changed => Drag::follow(shifted),
                    UIGestureRecognizerState::Ended => Drag::release(shifted, speed),
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
                        break;
                    }
                });
                NativeStack::chrome();
            }
        }

        unsafe impl UIAdaptivePresentationControllerDelegate for NavDelegate {
            #[unsafe(method(presentationControllerDidDismiss:))]
            fn did_dismiss(&self, _controller: &UIPresentationController) {
                let presenter = STACK.with_borrow_mut(|stack| {
                    let stack = stack.as_mut()?;
                    let departing = stack.sheets.pop()?;
                    DISMISSED.set(DISMISSED.get() + departing.levels.len());
                    Some(NativeStack::topmost(stack)?.navigation.clone())
                });
                let Some(presenter) = presenter else {
                    return;
                };
                Parallax::recede(&presenter, true);
                NativeStack::chrome();
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
        let tabs = Tabs::under(&root_view, &delegate, marker);
        tabs.swipeable(&delegate, marker);
        let indicator = Indicator::over(&root_view, &delegate, marker);
        let leave = unsafe {
            UITapGestureRecognizer::initWithTarget_action(
                UITapGestureRecognizer::alloc(marker),
                Some(&*delegate),
                Some(sel!(pagerTapped:)),
            )
        };
        leave.setEnabled(false);
        root_view.addGestureRecognizer(&leave);
        let sweep = unsafe {
            UIPanGestureRecognizer::initWithTarget_action(
                UIPanGestureRecognizer::alloc(marker),
                Some(&*delegate),
                Some(sel!(panned:)),
            )
        };
        sweep.setEnabled(false);
        root_view.addGestureRecognizer(&sweep);
        STACK.set(Some(NativeStack {
            root_controller,
            pager,
            rootless: web_view,
            backdrop: page.background_or(crate::root::webview_background()),
            stacks: HashMap::new(),
            sheets: Vec::new(),
            kept: HashMap::new(),
            pending: HashMap::new(),
            ghosts: HashMap::new(),
            arriving: None,
            seated: None,
            dragging: None,
            tabs,
            indicator,
            delegate,
            dismissal: Dismissal::new(marker),
            overviewing: false,
            leave,
            sweep,
            snaps: HashMap::new(),
            row: Vec::new(),
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

    pub fn take_closing() -> Option<String> {
        CLOSING.with_borrow_mut(Option::take)
    }

    fn size_to_parent(view: &UIView, parent: &UIView) {
        view.setFrame(parent.bounds());
        view.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
    }

    fn after_paint<F: FnOnce() + 'static>(work: F) {
        on_main(48, work);
    }

    fn next_turn<F: FnOnce() + 'static>(work: F) {
        on_main(0, work);
    }

    fn on_main<F: FnOnce() + 'static>(delay: u64, work: F) {
        let Ok(when) = DispatchTime::try_from(Duration::from_millis(delay)) else {
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

    pub fn take_closing() -> Option<String> {
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
    NativeStack, take_closed, take_closing, take_dismissed, take_picked, take_popped, take_tapped,
};
