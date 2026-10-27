use objc2::rc::Retained;
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, runtime::AnyClass};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
    NSGlassEffectViewStyle, NSPanel, NSView, NSWindow, NSWindowCollectionBehavior,
    NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CACornerMask;
use objc2_web_kit::WKWebViewConfiguration;
use std::cell::Cell;
use tracing::{error, warn};
use wry::WebViewExtMacOS;

use super::{Appearance, SiblingOrder, WebView};

objc2::define_class!(
    #[unsafe(super(NSPanel, NSWindow, objc2_app_kit::NSResponder, objc2::runtime::NSObject))]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "VmuxPageModalPanel"]
    struct ModalPanel;

    impl ModalPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

pub(super) struct ModalHost {
    parent: Retained<NSWindow>,
    panel: Retained<NSPanel>,
    glass: Option<Retained<NSGlassEffectView>>,
    visible: Cell<bool>,
}

impl ModalHost {
    fn new(webview: &wry::WebView) -> Option<Self> {
        let marker = MainThreadMarker::new()?;
        let wk = webview.webview();
        let view: &NSView = &wk;
        let parent = view.window()?;
        let panel: Retained<NSPanel> = {
            let panel = ModalPanel::alloc(marker);
            let panel: Retained<ModalPanel> = unsafe {
                objc2::msg_send![
                    panel,
                    initWithContentRect: NSRect::new(
                        NSPoint::new(0.0, 0.0),
                        NSSize::new(1.0, 1.0),
                    ),
                    styleMask: NSWindowStyleMask::Borderless
                        | NSWindowStyleMask::NonactivatingPanel,
                    backing: NSBackingStoreType::Buffered,
                    defer: false,
                ]
            };
            Retained::into_super(panel)
        };
        panel.setFloatingPanel(false);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel.setMovableByWindowBackground(false);
        panel.setHidesOnDeactivate(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(true);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::Transient,
        );
        unsafe { panel.setReleasedWhenClosed(false) };

        view.removeFromSuperview();
        let glass = if AnyClass::get(c"NSGlassEffectView").is_some() {
            let glass = NSGlassEffectView::new(marker);
            glass.setStyle(NSGlassEffectViewStyle::Clear);
            glass.setTintColor(Some(&NSColor::clearColor()));
            let glass_view: &NSView = glass.as_super();
            glass_view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0)));
            glass_view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            glass.setContentView(Some(view));
            panel.setContentView(Some(glass_view));
            Some(glass)
        } else {
            panel.setContentView(Some(view));
            None
        };
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        if let Some(content) = panel.contentView() {
            view.setFrame(content.bounds());
        }
        unsafe { parent.addChildWindow_ordered(&panel, NSWindowOrderingMode::Above) };
        panel.orderOut(None);

        Some(Self {
            parent,
            panel,
            glass,
            visible: Cell::new(false),
        })
    }

    pub(super) fn set_bounds(&self, webview: &wry::WebView, bounds: wry::Rect) {
        let scale = self.parent.backingScaleFactor();
        let position = bounds.position.to_logical::<f64>(scale);
        let size = bounds.size.to_logical::<f64>(scale);
        let parent_frame = self.parent.frame();
        let content_height = self
            .parent
            .contentView()
            .map(|view| view.frame().size.height)
            .unwrap_or(parent_frame.size.height);
        let frame = NSRect::new(
            NSPoint::new(
                parent_frame.origin.x + position.x,
                parent_frame.origin.y + content_height - position.y - size.height,
            ),
            NSSize::new(size.width, size.height),
        );
        self.panel.setFrame_display(frame, true);
        let wk = webview.webview();
        let view: &NSView = &wk;
        if let Some(content) = self.panel.contentView() {
            content.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
            view.setFrame(content.bounds());
        }
    }

    pub(super) fn set_visible(&self, visible: bool) {
        if self.visible.replace(visible) == visible {
            return;
        }
        if visible {
            self.panel.orderFront(None);
            self.panel.makeKeyWindow();
        } else {
            self.parent.makeKeyWindow();
            self.panel.orderOut(None);
        }
    }

    pub(super) fn set_corner_radius(&self, radius: f64, all_corners: bool) {
        if let Some(glass) = &self.glass {
            Self::apply_corner_radius(glass.as_super(), radius, all_corners);
            return;
        }
        let Some(content) = self.panel.contentView() else {
            return;
        };
        Self::apply_corner_radius(&content, radius, all_corners);
    }

    fn apply_corner_radius(view: &NSView, radius: f64, all_corners: bool) {
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            return;
        };
        let all = CACornerMask::LayerMinXMinYCorner
            | CACornerMask::LayerMaxXMinYCorner
            | CACornerMask::LayerMinXMaxYCorner
            | CACornerMask::LayerMaxXMaxYCorner;
        let bottom = CACornerMask::LayerMinXMinYCorner | CACornerMask::LayerMaxXMinYCorner;
        layer.setCornerRadius(radius.max(0.0));
        layer.setMasksToBounds(true);
        layer.setMaskedCorners(if all_corners { all } else { bottom });
    }
}

impl Drop for ModalHost {
    fn drop(&mut self) {
        self.parent.removeChildWindow(&self.panel);
        self.panel.orderOut(None);
        if let Some(glass) = &self.glass {
            glass.setContentView(None);
        }
        self.panel.setContentView(None);
    }
}

struct SurfaceBackground;

impl SurfaceBackground {
    fn apply(webview: &wry::WebView, appearance: Appearance, transparent: bool) {
        let wk = webview.webview();
        let (red, green, blue, alpha) = appearance.surface_rgba(transparent);
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            red as f64 / 255.0,
            green as f64 / 255.0,
            blue as f64 / 255.0,
            alpha as f64 / 255.0,
        );
        unsafe {
            wk.setUnderPageBackgroundColor(Some(&color));
        }
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setOpaque(!transparent);
            layer.setBackgroundColor(Some(&color.CGColor()));
        }
    }
}

impl WebView {
    pub fn host_as_modal(&self) -> bool {
        if self.modal.borrow().is_some() {
            return true;
        }
        let Some(host) = ModalHost::new(&self.webview) else {
            return false;
        };
        self.modal.replace(Some(host));
        true
    }

    pub fn order_among_siblings(&self, order: SiblingOrder) {
        if self.modal.borrow().is_some() {
            return;
        }
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(parent) = (unsafe { view.superview() }) else {
            return;
        };
        let subviews = parent.subviews();
        let occupant = match order {
            SiblingOrder::Front => subviews.lastObject(),
            SiblingOrder::Back => subviews.firstObject(),
        };
        if occupant.is_some_and(|held| std::ptr::eq(&*held, view)) {
            return;
        }
        let mode = match order {
            SiblingOrder::Front => NSWindowOrderingMode::Above,
            SiblingOrder::Back => NSWindowOrderingMode::Below,
        };

        parent.addSubview_positioned_relativeTo(view, mode, None);
    }

    pub fn raise_above_layers(&self) {
        if self.modal.borrow().is_some() {
            return;
        }
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            error!("vmux_page: the view has no layer, it will paint under its siblings");
            return;
        };
        layer.setZPosition(500.0);
    }

    pub fn set_corner_radius(&self, radius: f64, all_corners: bool) {
        if let Some(modal) = self.modal.borrow().as_ref() {
            modal.set_corner_radius(radius, all_corners);
        }
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            warn!("vmux_page: the view has no layer, its corners will stay square");
            return;
        };
        let all = CACornerMask::LayerMinXMinYCorner
            | CACornerMask::LayerMaxXMinYCorner
            | CACornerMask::LayerMinXMaxYCorner
            | CACornerMask::LayerMaxXMaxYCorner;
        let bottom = if view.isFlipped() {
            CACornerMask::LayerMinXMaxYCorner | CACornerMask::LayerMaxXMaxYCorner
        } else {
            CACornerMask::LayerMinXMinYCorner | CACornerMask::LayerMaxXMinYCorner
        };
        layer.setCornerRadius(radius.max(0.0));
        layer.setMasksToBounds(true);
        layer.setMaskedCorners(if all_corners { all } else { bottom });
    }
    pub fn set_focus_ring(&self, width: f64, color_rgb: [f32; 3]) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            warn!("vmux_page: the view has no layer, it cannot show a focus ring");
            return;
        };
        layer.setBorderWidth(width.max(0.0));
        if width <= 0.0 {
            return;
        }
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            color_rgb[0].clamp(0.0, 1.0) as f64,
            color_rgb[1].clamp(0.0, 1.0) as f64,
            color_rgb[2].clamp(0.0, 1.0) as f64,
            1.0,
        );
        layer.setBorderColor(Some(&color.CGColor()));
    }

    pub fn set_appearance(&self, appearance: Appearance) {
        let named = match appearance {
            Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
            Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
            Appearance::System => None,
        };
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setAppearance(named.as_deref());
        SurfaceBackground::apply(&self.webview, appearance, self.page.get().transparent);
    }

    pub fn take_first_responder(&self) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(window) = view.window() else {
            return;
        };
        let holds_it = window
            .firstResponder()
            .is_some_and(|current| std::ptr::eq(&*current as *const _ as *const NSView, view));
        if holds_it {
            return;
        }
        if !window.makeFirstResponder(Some(view)) {
            warn!("vmux_page: the window refused first responder, this page cannot be typed in");
        }
    }
}

pub struct LinkPreviewPolicy;

impl LinkPreviewPolicy {
    pub fn forbid(webview: &wry::WebView) {
        unsafe { webview.webview().setAllowsLinkPreview(false) };
    }
}

pub struct SharedWebProcess;

impl SharedWebProcess {
    pub fn configuration() -> Option<Retained<WKWebViewConfiguration>> {
        let marker = MainThreadMarker::new()?;
        let config = unsafe { WKWebViewConfiguration::new(marker) };
        pool::SharedPool::attach_to(&config, marker);
        Some(config)
    }
}

#[allow(deprecated)]
mod pool {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_web_kit::{WKProcessPool, WKWebViewConfiguration};
    use std::cell::OnceCell;

    pub struct SharedPool;

    impl SharedPool {
        pub fn attach_to(config: &WKWebViewConfiguration, marker: MainThreadMarker) {
            POOL.with(|pool| {
                let shared = pool.get_or_init(|| unsafe { WKProcessPool::new(marker) });
                unsafe { config.setProcessPool(shared) };
            });
        }
    }

    thread_local! {
        static POOL: OnceCell<Retained<WKProcessPool>> = const { OnceCell::new() };
    }
}
