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

const CARD_Z: f64 = 180.0;
const ROW_H: f64 = 30.0;

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
    header_card: Option<Retained<NSGlassEffectView>>,
    sidesheet_card: Option<Retained<NSGlassEffectView>>,
    buttons: Vec<Retained<NSButton>>,
    fills: Vec<Retained<NSView>>,
    actions: Vec<LayoutAction>,
    target: Option<Retained<TabTarget>>,
    click_rx: Option<Receiver<isize>>,
    last_size: (f64, f64),
}

/// A Regular Liquid Glass card — the navigation *material* a region sits on. Controls on it use
/// fills/vibrancy (not more glass), per the HIG.
fn glass_card(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: NSRect,
    radius: f64,
) -> Retained<NSGlassEffectView> {
    let card: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    card.setStyle(NSGlassEffectViewStyle::Regular);
    card.setCornerRadius(radius);
    let view: &NSView = &card;
    view.setFrame(frame);
    view.setWantsLayer(true);
    content.addSubview(view);
    if let Some(layer) = view.layer() {
        layer.setZPosition(CARD_Z);
    }
    card
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

/// A clickable fill button (text + optional rounded fill background) placed inside a card.
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

/// A non-interactive fill (rounded background + label) placed inside a card.
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
    {
        let fv: &NSView = &fill;
        fv.setFrame(frame);
    }
    rounded_bg(&fill, radius, bg);
    parent.addSubview(&fill);
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    label.setTextColor(Some(fg));
    let lview: &NSView = &label;
    lview.setFrame(NSRect::new(
        NSPoint::new(10.0, (frame.size.height - 16.0) / 2.0),
        NSSize::new((frame.size.width - 20.0).max(10.0), 16.0),
    ));
    fill.addSubview(lview);
    fill
}

fn accent(alpha: f64) -> Retained<NSColor> {
    NSColor::controlAccentColor().colorWithAlphaComponent(alpha)
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
        if let Some(card) = &state.header_card {
            let v: &NSView = card;
            v.setHidden(true);
        }
        if let Some(card) = &state.sidesheet_card {
            let v: &NSView = card;
            v.setHidden(true);
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
    let size = (bounds.size.width, bounds.size.height);

    if state.target.is_none() {
        let (tx, rx) = channel();
        state.target = Some(TabTarget::new(tx));
        state.click_rx = Some(rx);
    }

    let header_h = CEF_RESERVED_HEIGHT_PX as f64;
    let sheet_w = SIDE_SHEET_WIDTH_PX as f64;
    let width = bounds.size.width;
    let height = bounds.size.height;
    let top_inset = 38.0; // clear the traffic lights in the side column
    let pad = 8.0;

    // Persistent glass cards (the materials). Header is flush against the page top; the side
    // sheet is the full-height left column.
    let header_frame = NSRect::new(
        NSPoint::new(sheet_w + pad, if flipped { pad } else { height - header_h }),
        NSSize::new((width - sheet_w - pad * 2.0).max(10.0), header_h - pad),
    );
    let sheet_frame = NSRect::new(
        NSPoint::new(pad, if flipped { top_inset } else { pad }),
        NSSize::new(sheet_w - pad * 2.0, (height - top_inset - pad).max(10.0)),
    );
    if state.header_card.is_none() {
        state.header_card = Some(glass_card(mtm, content, header_frame, 14.0));
    }
    if state.sidesheet_card.is_none() {
        state.sidesheet_card = Some(glass_card(mtm, content, sheet_frame, 14.0));
    }
    if let Some(card) = &state.header_card {
        let v: &NSView = card;
        v.setFrame(header_frame);
        v.setHidden(false);
    }
    if let Some(card) = &state.sidesheet_card {
        let v: &NSView = card;
        v.setFrame(sheet_frame);
        v.setHidden(false);
    }

    let resized =
        (size.0 - state.last_size.0).abs() > 0.5 || (size.1 - state.last_size.1).abs() > 0.5;
    if !(current.is_changed() || state.buttons.is_empty() || resized) {
        return;
    }
    state.last_size = size;
    rebuild(&mut state, &current, mtm);
}

fn rebuild(state: &mut LayoutGlassState, current: &CurrentLayoutView, mtm: MainThreadMarker) {
    for b in state.buttons.drain(..) {
        let v: &NSView = &b;
        v.removeFromSuperview();
    }
    for f in state.fills.drain(..) {
        f.removeFromSuperview();
    }
    state.actions.clear();
    let (Some(header), Some(sheet), Some(target)) = (
        state.header_card.clone(),
        state.sidesheet_card.clone(),
        state.target.clone(),
    ) else {
        return;
    };
    let header_view: &NSView = &header;
    let sheet_view: &NSView = &sheet;
    let target_ref: &AnyObject = &target;

    // --- Header card (local coords). Two rows: tabs on top, nav/url/profile below. ---
    let hb = header_view.bounds();
    let hflip = header_view.isFlipped();
    let from_top = |off: f64, h: f64| {
        if hflip { off } else { hb.size.height - off - h }
    };
    let row1 = from_top(10.0, ROW_H);
    let row2 = from_top(10.0 + ROW_H + 6.0, ROW_H);

    // Row 1: tabs + new-tab
    let tab_w = 120.0_f64;
    let mut x = 12.0_f64;
    for tab in &current.0.tabs {
        let tag = state.actions.len();
        let bg = if tab.is_active {
            Some(accent(0.85))
        } else {
            Some(white(0.06))
        };
        let b = fill_button(
            header_view,
            mtm,
            target_ref,
            &tab.title,
            tag,
            NSRect::new(NSPoint::new(x, row1), NSSize::new(tab_w, ROW_H)),
            8.0,
            bg.as_deref(),
        );
        state.buttons.push(b);
        state.actions.push(LayoutAction::SwitchTab(tab.id.clone()));
        x += tab_w + 6.0;
    }
    {
        let tag = state.actions.len();
        let b = fill_button(
            header_view,
            mtm,
            target_ref,
            "+",
            tag,
            NSRect::new(NSPoint::new(x, row1), NSSize::new(ROW_H, ROW_H)),
            8.0,
            None,
        );
        state.buttons.push(b);
        state.actions.push(LayoutAction::NewTab);
    }

    // Row 2: nav + address + profile
    let mut nx = 12.0_f64;
    for (glyph, action) in [
        ("\u{2039}", LayoutAction::Back),
        ("\u{203a}", LayoutAction::Forward),
        ("\u{27f3}", LayoutAction::Reload),
    ] {
        let tag = state.actions.len();
        let b = fill_button(
            header_view,
            mtm,
            target_ref,
            glyph,
            tag,
            NSRect::new(NSPoint::new(nx, row2), NSSize::new(ROW_H, ROW_H)),
            ROW_H / 2.0,
            None,
        );
        state.buttons.push(b);
        state.actions.push(action);
        nx += ROW_H + 4.0;
    }

    let profile = active_profile_name();
    let profile_w = 100.0_f64;
    let profile_x = hb.size.width - profile_w - 12.0;
    if !profile.is_empty() {
        let f = fill_label(
            header_view,
            mtm,
            profile,
            NSRect::new(NSPoint::new(profile_x, row2), NSSize::new(profile_w, ROW_H)),
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
        header_view,
        mtm,
        addr_text,
        NSRect::new(NSPoint::new(addr_x, row2), NSSize::new(addr_w, ROW_H)),
        ROW_H / 2.0,
        &NSColor::secondaryLabelColor(),
        Some(&black(0.22)),
    );
    state.fills.push(f);

    // --- Side sheet card (local coords): stack rows + new-stack ---
    let sb = sheet_view.bounds();
    let sflip = sheet_view.isFlipped();
    let card_w = (sb.size.width - 16.0).max(40.0);
    let row_y = |row: usize| {
        let off = 12.0 + row as f64 * (ROW_H + 6.0);
        if sflip {
            off
        } else {
            sb.size.height - off - ROW_H
        }
    };
    for (i, st) in current.0.stacks.iter().enumerate() {
        let bg = if st.is_active {
            accent(0.85)
        } else {
            white(0.06)
        };
        let f = fill_label(
            sheet_view,
            mtm,
            &st.title,
            NSRect::new(NSPoint::new(8.0, row_y(i)), NSSize::new(card_w, ROW_H)),
            8.0,
            &NSColor::labelColor(),
            Some(&bg),
        );
        state.fills.push(f);
    }
    {
        let tag = state.actions.len();
        let b = fill_button(
            sheet_view,
            mtm,
            target_ref,
            "+ New Stack",
            tag,
            NSRect::new(
                NSPoint::new(8.0, row_y(current.0.stacks.len())),
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
