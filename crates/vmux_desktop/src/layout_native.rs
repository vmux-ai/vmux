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

const GLASS_Z: f64 = 200.0;
const ROW_H: f64 = 30.0;
const CARD_H: f64 = 38.0;

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
    glass: Vec<Retained<NSGlassEffectView>>,
    buttons: Vec<Retained<NSButton>>,
    fills: Vec<Retained<NSView>>,
    actions: Vec<LayoutAction>,
    target: Option<Retained<TabTarget>>,
    click_rx: Option<Receiver<isize>>,
    last_size: (f64, f64),
}

/// A Regular Liquid Glass element (active tab / stack card). `active` adds a subtle accent tint.
fn glass_pill(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: NSRect,
    radius: f64,
    active: bool,
) -> Retained<NSGlassEffectView> {
    let g: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    g.setStyle(NSGlassEffectViewStyle::Regular);
    if active {
        g.setTintColor(Some(
            &NSColor::controlAccentColor().colorWithAlphaComponent(0.5),
        ));
    }
    g.setCornerRadius(radius);
    let v: &NSView = &g;
    v.setFrame(frame);
    v.setWantsLayer(true);
    content.addSubview(v);
    if let Some(layer) = v.layer() {
        layer.setZPosition(GLASS_Z);
    }
    g
}

fn rounded_bg(view: &NSView, radius: f64, bg: Option<&NSColor>) {
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        layer.setCornerRadius(radius);
        if let Some(c) = bg {
            layer.setBackgroundColor(Some(&c.CGColor()));
        }
    }
}

fn add_label(parent: &NSView, mtm: MainThreadMarker, text: &str, frame: NSRect, fg: &NSColor) {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    label.setTextColor(Some(fg));
    let lview: &NSView = &label;
    lview.setFrame(frame);
    parent.addSubview(lview);
}

/// A clickable control (text + optional rounded fill) placed directly on the content view.
fn fill_button(
    parent: &NSView,
    mtm: MainThreadMarker,
    target: &AnyObject,
    text: &str,
    tag: usize,
    frame: NSRect,
    radius: f64,
    bg: Option<&NSColor>,
) -> Retained<NSButton> {
    let button = NSButton::new(mtm);
    button.setTitle(&NSString::from_str(text));
    button.setBordered(false);
    button.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    unsafe { button.setTarget(Some(target)) };
    unsafe { button.setAction(Some(sel!(onTabClick:))) };
    button.setTag(tag as isize);
    let bview: &NSView = &button;
    bview.setFrame(frame);
    rounded_bg(bview, radius, bg);
    parent.addSubview(bview);
    button
}

/// A non-interactive fill (rounded background + centred label) placed on the content view.
fn fill_label(
    parent: &NSView,
    mtm: MainThreadMarker,
    text: &str,
    frame: NSRect,
    radius: f64,
    fg: &NSColor,
    bg: Option<&NSColor>,
) -> Retained<NSView> {
    let fill: Retained<NSView> = NSView::new(mtm);
    let fv: &NSView = &fill;
    fv.setFrame(frame);
    rounded_bg(fv, radius, bg);
    parent.addSubview(fv);
    add_label(
        fv,
        mtm,
        text,
        NSRect::new(
            NSPoint::new(10.0, (frame.size.height - 16.0) / 2.0),
            NSSize::new((frame.size.width - 20.0).max(10.0), 16.0),
        ),
        fg,
    );
    fill
}

fn white(alpha: f64) -> Retained<NSColor> {
    NSColor::whiteColor().colorWithAlphaComponent(alpha)
}

fn black(alpha: f64) -> Retained<NSColor> {
    NSColor::blackColor().colorWithAlphaComponent(alpha)
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
        clear_all(&mut state);
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
    let size = (bounds.size.width, bounds.size.height);

    if state.target.is_none() {
        let (tx, rx) = channel();
        state.target = Some(TabTarget::new(tx));
        state.click_rx = Some(rx);
    }

    let resized =
        (size.0 - state.last_size.0).abs() > 0.5 || (size.1 - state.last_size.1).abs() > 0.5;
    let empty = state.glass.is_empty() && state.buttons.is_empty();
    if !(current.is_changed() || empty || resized) {
        return;
    }
    state.last_size = size;
    rebuild(&mut state, &current, content, mtm, bounds, flipped);
}

fn clear_all(state: &mut LayoutGlassState) {
    for g in state.glass.drain(..) {
        let v: &NSView = &g;
        v.removeFromSuperview();
    }
    for b in state.buttons.drain(..) {
        let v: &NSView = &b;
        v.removeFromSuperview();
    }
    for f in state.fills.drain(..) {
        f.removeFromSuperview();
    }
    state.actions.clear();
}

fn rebuild(
    state: &mut LayoutGlassState,
    current: &CurrentLayoutView,
    content: &NSView,
    mtm: MainThreadMarker,
    bounds: NSRect,
    flipped: bool,
) {
    clear_all(state);
    let Some(target) = state.target.clone() else {
        return;
    };
    let target_ref: &AnyObject = &target;

    let header_h = CEF_RESERVED_HEIGHT_PX as f64;
    let sheet_w = SIDE_SHEET_WIDTH_PX as f64;
    let width = bounds.size.width;
    let height = bounds.size.height;
    let band_top = if flipped { 0.0 } else { height - header_h };
    // Two header rows stacked flush against the page top.
    let (row1_y, row2_y) = if flipped {
        let r2 = header_h - ROW_H;
        (r2 - ROW_H, r2)
    } else {
        (band_top + ROW_H, band_top)
    };
    let main_x0 = sheet_w + 12.0;

    // Row 1: tabs (active = glass pill, others = subtle fill) + new-tab
    let tab_w = 120.0_f64;
    let mut x = main_x0;
    for tab in &current.0.tabs {
        let tag = state.actions.len();
        let frame = NSRect::new(NSPoint::new(x, row1_y), NSSize::new(tab_w, ROW_H));
        if tab.is_active {
            let pill = glass_pill(mtm, content, frame, ROW_H / 2.0, false);
            let pv: &NSView = &pill;
            let b = fill_button(
                pv,
                mtm,
                target_ref,
                &tab.title,
                tag,
                NSRect::new(NSPoint::new(0.0, 0.0), frame.size),
                0.0,
                None,
            );
            state.glass.push(pill);
            state.buttons.push(b);
        } else {
            let b = fill_button(
                content,
                mtm,
                target_ref,
                &tab.title,
                tag,
                frame,
                8.0,
                Some(&white(0.05)),
            );
            state.buttons.push(b);
        }
        state.actions.push(LayoutAction::SwitchTab(tab.id.clone()));
        x += tab_w + 6.0;
    }
    {
        let tag = state.actions.len();
        let b = fill_button(
            content,
            mtm,
            target_ref,
            "+",
            tag,
            NSRect::new(NSPoint::new(x, row1_y), NSSize::new(ROW_H, ROW_H)),
            ROW_H / 2.0,
            None,
        );
        state.buttons.push(b);
        state.actions.push(LayoutAction::NewTab);
    }

    // Row 2: nav + address + profile (all fills on the window glass)
    let mut nx = main_x0;
    for (glyph, action) in [
        ("\u{2039}", LayoutAction::Back),
        ("\u{203a}", LayoutAction::Forward),
        ("\u{27f3}", LayoutAction::Reload),
    ] {
        let tag = state.actions.len();
        let b = fill_button(
            content,
            mtm,
            target_ref,
            glyph,
            tag,
            NSRect::new(NSPoint::new(nx, row2_y), NSSize::new(ROW_H, ROW_H)),
            ROW_H / 2.0,
            None,
        );
        state.buttons.push(b);
        state.actions.push(action);
        nx += ROW_H + 4.0;
    }

    let profile = active_profile_name();
    let profile_w = 100.0_f64;
    let profile_x = width - profile_w - 12.0;
    if !profile.is_empty() {
        let f = fill_label(
            content,
            mtm,
            profile,
            NSRect::new(
                NSPoint::new(profile_x, row2_y),
                NSSize::new(profile_w, ROW_H),
            ),
            ROW_H / 2.0,
            &NSColor::labelColor(),
            Some(&white(0.08)),
        );
        state.fills.push(f);
    }

    let addr_x = nx + 6.0;
    let addr_w = (profile_x - addr_x - 10.0).max(80.0);
    let addr_text = if current.0.address.is_empty() {
        " "
    } else {
        current.0.address.as_str()
    };
    let f = fill_label(
        content,
        mtm,
        addr_text,
        NSRect::new(NSPoint::new(addr_x, row2_y), NSSize::new(addr_w, ROW_H)),
        ROW_H / 2.0,
        &NSColor::secondaryLabelColor(),
        Some(&black(0.22)),
    );
    state.fills.push(f);

    // Side sheet: one glass card per stack + new-stack
    let top_inset = 40.0;
    let card_w = (sheet_w - 24.0).max(40.0);
    let card_y = |row: usize| {
        let off = top_inset + row as f64 * (CARD_H + 8.0);
        if flipped { off } else { height - off - CARD_H }
    };
    for (i, st) in current.0.stacks.iter().enumerate() {
        let card = glass_pill(
            mtm,
            content,
            NSRect::new(NSPoint::new(12.0, card_y(i)), NSSize::new(card_w, CARD_H)),
            10.0,
            st.is_active,
        );
        let cv: &NSView = &card;
        add_label(
            cv,
            mtm,
            &st.title,
            NSRect::new(
                NSPoint::new(10.0, (CARD_H - 16.0) / 2.0),
                NSSize::new(card_w - 20.0, 16.0),
            ),
            &NSColor::labelColor(),
        );
        state.glass.push(card);
    }
    {
        let tag = state.actions.len();
        let b = fill_button(
            content,
            mtm,
            target_ref,
            "+ New Stack",
            tag,
            NSRect::new(
                NSPoint::new(12.0, card_y(current.0.stacks.len())),
                NSSize::new(card_w, ROW_H),
            ),
            8.0,
            Some(&white(0.04)),
        );
        state.buttons.push(b);
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
