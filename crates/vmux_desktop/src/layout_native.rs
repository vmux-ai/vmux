use std::sync::mpsc::{Receiver, Sender, channel};

use bevy::prelude::*;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, define_class, msg_send, sel};
use objc2_app_kit::{
    NSButton, NSColor, NSControl, NSFont, NSGlassEffectView, NSGlassEffectViewStyle, NSTextField,
    NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use vmux_layout::event::{CEF_RESERVED_HEIGHT_PX, TabsCommandEvent};
use vmux_layout::native_view::{CurrentLayoutView, LayoutRenderer, NodeId};
use vmux_layout::protocol::parse_id;
use vmux_layout::scene::InteractionMode;

pub(crate) struct LayoutNativePlugin;

impl Plugin for LayoutNativePlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<LayoutGlassState>()
            .add_systems(Update, drain_tab_clicks)
            .add_systems(Last, sync_layout_glass);
    }
}

struct TabTargetIvars {
    sender: Sender<isize>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VmuxLayoutTabTarget"]
    #[ivars = TabTargetIvars]
    struct TabTarget;

    impl TabTarget {
        #[unsafe(method(onTabClick:))]
        fn on_tab_click(&self, sender: &NSControl) {
            let _ = self.ivars().sender.send(sender.tag());
        }
    }
);

impl TabTarget {
    fn new(sender: Sender<isize>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(TabTargetIvars { sender });
        unsafe { msg_send![super(this), init] }
    }
}

#[derive(Default)]
struct LayoutGlassState {
    header: Option<Retained<NSGlassEffectView>>,
    tabs: Vec<Retained<NSButton>>,
    tab_ids: Vec<NodeId>,
    address: Option<Retained<NSTextField>>,
    target: Option<Retained<TabTarget>>,
    click_rx: Option<Receiver<isize>>,
    shown: bool,
}

fn sync_layout_glass(
    mut state: NonSendMut<LayoutGlassState>,
    renderer: Res<LayoutRenderer>,
    mode: Res<InteractionMode>,
    current: Res<CurrentLayoutView>,
    window_q: Query<Entity, With<bevy::window::PrimaryWindow>>,
) {
    let want = *renderer == LayoutRenderer::Native && *mode == InteractionMode::User;

    if !want {
        if state.shown {
            if let Some(glass) = &state.header {
                let view: &NSView = glass;
                view.setHidden(true);
            }
            state.shown = false;
        }
        return;
    }

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Ok(entity) = window_q.single() else {
        return;
    };
    let Some(ns_view) = crate::glass::primary_content_view_ptr(entity) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    let bounds = content.bounds();
    let header_h = CEF_RESERVED_HEIGHT_PX as f64;
    let top_y = if content.isFlipped() {
        0.0
    } else {
        bounds.size.height - header_h
    };
    let frame = NSRect::new(
        NSPoint::new(0.0, top_y),
        NSSize::new(bounds.size.width, header_h),
    );

    if state.target.is_none() {
        let (tx, rx) = channel();
        state.target = Some(TabTarget::new(tx));
        state.click_rx = Some(rx);
    }

    let header_created = state.header.is_none();
    if header_created {
        let glass: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
        glass.setStyle(NSGlassEffectViewStyle::Clear);
        glass.setTintColor(Some(&NSColor::clearColor()));
        let view: &NSView = &glass;
        view.setWantsLayer(true);
        content.addSubview(view);
        if let Some(layer) = view.layer() {
            layer.setZPosition(200.0);
        }
        state.header = Some(glass);
    }

    if let Some(glass) = &state.header {
        let view: &NSView = glass;
        view.setFrame(frame);
        view.setHidden(false);
    }
    state.shown = true;

    if current.is_changed() || header_created {
        for button in state.tabs.drain(..) {
            let view: &NSView = &button;
            view.removeFromSuperview();
        }
        state.tab_ids.clear();
        if let Some(label) = state.address.take() {
            let view: &NSView = &label;
            view.removeFromSuperview();
        }
        let (Some(glass), Some(target)) = (state.header.clone(), state.target.clone()) else {
            return;
        };
        let host: &NSView = &glass;
        let target_ref: &AnyObject = &target;
        let label_h = 24.0_f64;
        let label_w = 160.0_f64;
        let y = (header_h - label_h) / 2.0;
        let mut x = 12.0_f64;
        for (idx, tab) in current.0.tabs.iter().enumerate() {
            let button = NSButton::new(mtm);
            button.setTitle(&NSString::from_str(&tab.title));
            button.setBordered(false);
            button.setFont(Some(&NSFont::systemFontOfSize(13.0)));
            unsafe { button.setTarget(Some(target_ref)) };
            unsafe { button.setAction(Some(sel!(onTabClick:))) };
            button.setTag(idx as isize);
            let bview: &NSView = &button;
            bview.setAlphaValue(if tab.is_active { 1.0 } else { 0.55 });
            bview.setFrame(NSRect::new(
                NSPoint::new(x, y),
                NSSize::new(label_w, label_h),
            ));
            host.addSubview(bview);
            state.tabs.push(button);
            state.tab_ids.push(tab.id.clone());
            x += label_w + 8.0;
        }
        if !current.0.address.is_empty() {
            let addr = NSTextField::labelWithString(&NSString::from_str(&current.0.address), mtm);
            addr.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            addr.setTextColor(Some(&NSColor::secondaryLabelColor()));
            let aview: &NSView = &addr;
            let addr_w = (bounds.size.width - x - 16.0).max(120.0);
            aview.setFrame(NSRect::new(
                NSPoint::new(x + 16.0, y),
                NSSize::new(addr_w, label_h),
            ));
            host.addSubview(aview);
            state.address = Some(addr);
        }
    }
}

fn drain_tab_clicks(state: NonSendMut<LayoutGlassState>, mut commands: Commands) {
    let mut clicked: Vec<NodeId> = Vec::new();
    if let Some(rx) = &state.click_rx {
        while let Ok(tag) = rx.try_recv() {
            if let Some(id) = state.tab_ids.get(tag as usize) {
                clicked.push(id.clone());
            }
        }
    }
    for id in clicked {
        if let Ok((_, bits)) = parse_id(&id.0) {
            commands.trigger(bevy_cef::prelude::BinReceive::<TabsCommandEvent> {
                webview: Entity::PLACEHOLDER,
                payload: TabsCommandEvent {
                    command: "switch".to_string(),
                    tab_id: Some(bits.to_string()),
                },
            });
        }
    }
}
