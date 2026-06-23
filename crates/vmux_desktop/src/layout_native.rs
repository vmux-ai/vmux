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
use vmux_command::{AppCommand, BrowserCommand, BrowserNavigationCommand, OpenCommand};
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
    Back,
    Forward,
    Reload,
    NewStack,
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

/// A Regular Liquid Glass pill. Regular adapts to whatever is behind it for legibility (HIG);
/// the active state is conveyed with a subtle accent tint rather than by dimming opacity.
fn glass_pill(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: NSRect,
    radius: f64,
    active: bool,
) -> Retained<NSGlassEffectView> {
    let pill: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    pill.setStyle(NSGlassEffectViewStyle::Regular);
    if active {
        let accent = NSColor::controlAccentColor();
        pill.setTintColor(Some(&accent.colorWithAlphaComponent(0.55)));
    }
    pill.setCornerRadius(radius);
    let view: &NSView = &pill;
    view.setFrame(frame);
    view.setWantsLayer(true);
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
    let band_top = if flipped { 0.0 } else { height - header_h };
    let row1_y = band_top + 10.0;
    let row2_y = band_top + 10.0 + PILL_H + 6.0;
    let radius = PILL_H / 2.0;
    // Header lives in the main column, aligned with the page (right of the side sheet).
    let main_x0 = sheet_w + 12.0;

    // Row 1: tab pills + new-tab
    let tab_w = 120.0_f64;
    let mut x = main_x0;
    for tab in &current.0.tabs {
        let tag = state.actions.len();
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(x, row1_y), NSSize::new(tab_w, PILL_H)),
            radius,
            tab.is_active,
        );
        add_button(&pill, mtm, target_ref, &tab.title, tag);
        state.pills.push(pill);
        state.actions.push(LayoutAction::SwitchTab(tab.id.clone()));
        x += tab_w + 8.0;
    }
    {
        let tag = state.actions.len();
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(x, row1_y), NSSize::new(PILL_H, PILL_H)),
            radius,
            false,
        );
        add_button(&pill, mtm, target_ref, "+", tag);
        state.pills.push(pill);
        state.actions.push(LayoutAction::NewTab);
    }

    // Row 2: nav + address + profile
    let mut nx = main_x0;
    for (glyph, action) in [
        ("\u{2039}", LayoutAction::Back),
        ("\u{203a}", LayoutAction::Forward),
        ("\u{27f3}", LayoutAction::Reload),
    ] {
        let tag = state.actions.len();
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(nx, row2_y), NSSize::new(PILL_H, PILL_H)),
            radius,
            false,
        );
        add_button(&pill, mtm, target_ref, glyph, tag);
        state.pills.push(pill);
        state.actions.push(action);
        nx += PILL_H + 6.0;
    }

    let profile = active_profile_name();
    let profile_w = 110.0_f64;
    let profile_x = width - profile_w - 12.0;
    if !profile.is_empty() {
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(
                NSPoint::new(profile_x, row2_y),
                NSSize::new(profile_w, PILL_H),
            ),
            radius,
            false,
        );
        add_label(&pill, mtm, profile, &NSColor::labelColor());
        state.pills.push(pill);
    }

    let addr_x = nx + 4.0;
    let addr_w = (profile_x - addr_x - 12.0).max(80.0);
    let addr_text = if current.0.address.is_empty() {
        " "
    } else {
        current.0.address.as_str()
    };
    let pill = glass_pill(
        mtm,
        content,
        NSRect::new(NSPoint::new(addr_x, row2_y), NSSize::new(addr_w, PILL_H)),
        radius,
        false,
    );
    add_label(&pill, mtm, addr_text, &NSColor::secondaryLabelColor());
    state.pills.push(pill);

    // Side sheet (left column, anchored top-left): stack cards + new-stack
    let card_h = 30.0_f64;
    let card_gap = 8.0_f64;
    let cards_top = if flipped {
        12.0
    } else {
        height - 12.0 - card_h
    };
    let card_y = |row: usize| {
        if flipped {
            cards_top + row as f64 * (card_h + card_gap)
        } else {
            cards_top - row as f64 * (card_h + card_gap)
        }
    };
    let card_w = (sheet_w - 24.0).max(40.0);
    for (i, st) in current.0.stacks.iter().enumerate() {
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(12.0, card_y(i)), NSSize::new(card_w, card_h)),
            10.0,
            st.is_active,
        );
        add_label(&pill, mtm, &st.title, &NSColor::labelColor());
        state.pills.push(pill);
    }
    {
        let tag = state.actions.len();
        let pill = glass_pill(
            mtm,
            content,
            NSRect::new(
                NSPoint::new(12.0, card_y(current.0.stacks.len())),
                NSSize::new(card_w, card_h),
            ),
            10.0,
            false,
        );
        add_button(&pill, mtm, target_ref, "+ New Stack", tag);
        state.pills.push(pill);
        state.actions.push(LayoutAction::NewStack);
    }
}

fn drain_tab_clicks(
    state: NonSendMut<LayoutGlassState>,
    mut commands: Commands,
    mut app_commands: MessageWriter<AppCommand>,
) {
    let mut fired: Vec<LayoutAction> = Vec::new();
    if let Some(rx) = &state.click_rx {
        while let Ok(tag) = rx.try_recv() {
            if let Some(action) = state.actions.get(tag as usize) {
                fired.push(action.clone());
            }
        }
    }
    for action in fired {
        match action {
            LayoutAction::SwitchTab(id) => {
                let Ok((_, bits)) = parse_id(&id.0) else {
                    continue;
                };
                trigger_tabs(&mut commands, "switch", Some(bits.to_string()));
            }
            LayoutAction::NewTab => trigger_tabs(&mut commands, "new", None),
            LayoutAction::Back => {
                app_commands.write(AppCommand::Browser(BrowserCommand::Navigation(
                    BrowserNavigationCommand::PrevPage,
                )));
            }
            LayoutAction::Forward => {
                app_commands.write(AppCommand::Browser(BrowserCommand::Navigation(
                    BrowserNavigationCommand::NextPage,
                )));
            }
            LayoutAction::Reload => {
                app_commands.write(AppCommand::Browser(BrowserCommand::Navigation(
                    BrowserNavigationCommand::Reload,
                )));
            }
            LayoutAction::NewStack => {
                app_commands.write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InNewStack { url: None },
                )));
            }
        }
    }
}

fn trigger_tabs(commands: &mut Commands, command: &str, tab_id: Option<String>) {
    commands.trigger(bevy_cef::prelude::BinReceive::<TabsCommandEvent> {
        webview: Entity::PLACEHOLDER,
        payload: TabsCommandEvent {
            command: command.to_string(),
            tab_id,
        },
    });
}
