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
use vmux_core::profile::active_profile_name;
use vmux_layout::event::{CEF_RESERVED_HEIGHT_PX, SIDE_SHEET_WIDTH_PX, TabsCommandEvent};
use vmux_layout::native_view::{CurrentLayoutView, LayoutRenderer, NodeId};
use vmux_layout::protocol::parse_id;
use vmux_layout::scene::InteractionMode;

const PILL_Z: f64 = 200.0;
const PILL_H: f64 = 28.0;

#[derive(Clone)]
enum LayoutAction {
    SwitchTab(NodeId),
    NewTab,
}

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
    pills: Vec<Retained<NSGlassEffectView>>,
    actions: Vec<LayoutAction>,
    target: Option<Retained<TabTarget>>,
    click_rx: Option<Receiver<isize>>,
    last_width: f64,
}

/// A Clear glass pill (rounded) added directly onto the (already-glass) window content view.
fn glass_pill(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: NSRect,
    radius: f64,
    alpha: f64,
) -> Retained<NSGlassEffectView> {
    let pill: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    pill.setStyle(NSGlassEffectViewStyle::Clear);
    pill.setTintColor(Some(&NSColor::clearColor()));
    pill.setCornerRadius(radius);
    let view: &NSView = &pill;
    view.setFrame(frame);
    view.setWantsLayer(true);
    view.setAlphaValue(alpha);
    content.addSubview(view);
    if let Some(layer) = view.layer() {
        layer.setZPosition(PILL_Z);
    }
    pill
}

fn add_label(pill: &NSGlassEffectView, mtm: MainThreadMarker, text: &str, color: &NSColor) {
    let host: &NSView = pill;
    let b = host.bounds();
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    label.setTextColor(Some(color));
    let lview: &NSView = &label;
    lview.setFrame(NSRect::new(
        NSPoint::new(10.0, (b.size.height - 16.0) / 2.0),
        NSSize::new((b.size.width - 20.0).max(10.0), 16.0),
    ));
    host.addSubview(lview);
}

fn add_button(
    pill: &NSGlassEffectView,
    mtm: MainThreadMarker,
    target: &AnyObject,
    text: &str,
    tag: usize,
) {
    let host: &NSView = pill;
    let b = host.bounds();
    let button = NSButton::new(mtm);
    button.setTitle(&NSString::from_str(text));
    button.setBordered(false);
    button.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    unsafe { button.setTarget(Some(target)) };
    unsafe { button.setAction(Some(sel!(onTabClick:))) };
    button.setTag(tag as isize);
    let bview: &NSView = &button;
    bview.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), b.size));
    host.addSubview(bview);
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
        if !state.pills.is_empty() {
            for pill in state.pills.drain(..) {
                let view: &NSView = &pill;
                view.removeFromSuperview();
            }
            state.actions.clear();
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
    let flipped = content.isFlipped();
    let width = bounds.size.width;

    if state.target.is_none() {
        let (tx, rx) = channel();
        state.target = Some(TabTarget::new(tx));
        state.click_rx = Some(rx);
    }

    let resized = (width - state.last_width).abs() > 0.5;
    if !(current.is_changed() || state.pills.is_empty() || resized) {
        return;
    }
    state.last_width = width;
    rebuild(&mut state, &current, content, mtm, bounds, flipped);
}

fn rebuild(
    state: &mut LayoutGlassState,
    current: &CurrentLayoutView,
    content: &NSView,
    mtm: MainThreadMarker,
    bounds: NSRect,
    flipped: bool,
) {
    for pill in state.pills.drain(..) {
        let view: &NSView = &pill;
        view.removeFromSuperview();
    }
    state.actions.clear();
    let Some(target) = state.target.clone() else {
        return;
    };
    let target_ref: &AnyObject = &target;

    let header_h = CEF_RESERVED_HEIGHT_PX as f64;
    let sheet_w = SIDE_SHEET_WIDTH_PX as f64;
    let width = bounds.size.width;
    let height = bounds.size.height;
    let header_band_top = if flipped { 0.0 } else { height - header_h };
    let pill_y = header_band_top + (header_h - PILL_H) / 2.0;
    let radius = PILL_H / 2.0;

    // Tab pills
    let tab_w = 132.0_f64;
    let mut x = 12.0_f64;
    for tab in &current.0.tabs {
        let tag = state.actions.len();
        let alpha = if tab.is_active { 1.0 } else { 0.5 };
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(x, pill_y), NSSize::new(tab_w, PILL_H)),
            radius,
            alpha,
        );
        add_button(&pill, mtm, target_ref, &tab.title, tag);
        state.pills.push(pill);
        state.actions.push(LayoutAction::SwitchTab(tab.id.clone()));
        x += tab_w + 8.0;
    }

    // New-tab pill
    {
        let tag = state.actions.len();
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(x, pill_y), NSSize::new(PILL_H, PILL_H)),
            radius,
            0.7,
        );
        add_button(&pill, mtm, target_ref, "+", tag);
        state.pills.push(pill);
        state.actions.push(LayoutAction::NewTab);
        x += PILL_H + 12.0;
    }

    // Profile pill (right)
    let profile = active_profile_name();
    let profile_w = 110.0_f64;
    let profile_x = width - profile_w - 12.0;
    if !profile.is_empty() {
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(
                NSPoint::new(profile_x, pill_y),
                NSSize::new(profile_w, PILL_H),
            ),
            radius,
            0.8,
        );
        add_label(&pill, mtm, profile, &NSColor::labelColor());
        state.pills.push(pill);
    }

    // Address pill (fills the gap between tabs and profile)
    if !current.0.address.is_empty() {
        let addr_x = x;
        let addr_w = (profile_x - addr_x - 12.0).max(80.0);
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(addr_x, pill_y), NSSize::new(addr_w, PILL_H)),
            radius,
            0.7,
        );
        add_label(
            &pill,
            mtm,
            &current.0.address,
            &NSColor::secondaryLabelColor(),
        );
        state.pills.push(pill);
    }

    // Stack cards (left column)
    let card_h = 30.0_f64;
    let card_gap = 8.0_f64;
    let cards_top = if flipped {
        header_h + 12.0
    } else {
        height - header_h - 12.0 - card_h
    };
    for (i, st) in current.0.stacks.iter().enumerate() {
        let cy = if flipped {
            cards_top + i as f64 * (card_h + card_gap)
        } else {
            cards_top - i as f64 * (card_h + card_gap)
        };
        let alpha = if st.is_active { 0.95 } else { 0.55 };
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(
                NSPoint::new(12.0, cy),
                NSSize::new((sheet_w - 24.0).max(40.0), card_h),
            ),
            10.0,
            alpha,
        );
        add_label(&pill, mtm, &st.title, &label_color(st.is_active));
        state.pills.push(pill);
    }
}

fn label_color(is_active: bool) -> Retained<NSColor> {
    if is_active {
        NSColor::labelColor()
    } else {
        NSColor::secondaryLabelColor()
    }
}

fn drain_tab_clicks(state: NonSendMut<LayoutGlassState>, mut commands: Commands) {
    let mut fired: Vec<LayoutAction> = Vec::new();
    if let Some(rx) = &state.click_rx {
        while let Ok(tag) = rx.try_recv() {
            if let Some(action) = state.actions.get(tag as usize) {
                fired.push(action.clone());
            }
        }
    }
    for action in fired {
        let payload = match action {
            LayoutAction::SwitchTab(id) => {
                let Ok((_, bits)) = parse_id(&id.0) else {
                    continue;
                };
                TabsCommandEvent {
                    command: "switch".to_string(),
                    tab_id: Some(bits.to_string()),
                }
            }
            LayoutAction::NewTab => TabsCommandEvent {
                command: "new".to_string(),
                tab_id: None,
            },
        };
        commands.trigger(bevy_cef::prelude::BinReceive::<TabsCommandEvent> {
            webview: Entity::PLACEHOLDER,
            payload,
        });
    }
}
