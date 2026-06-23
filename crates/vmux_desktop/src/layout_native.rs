use bevy::prelude::*;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSColor, NSFont, NSGlassEffectView, NSGlassEffectViewStyle, NSTextField, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use vmux_layout::event::CEF_RESERVED_HEIGHT_PX;
use vmux_layout::native_view::{CurrentLayoutView, LayoutRenderer};
use vmux_layout::scene::InteractionMode;

pub(crate) struct LayoutNativePlugin;

impl Plugin for LayoutNativePlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<LayoutGlassState>()
            .add_systems(Last, sync_layout_glass);
    }
}

#[derive(Default)]
struct LayoutGlassState {
    header: Option<Retained<NSGlassEffectView>>,
    tabs: Vec<Retained<NSTextField>>,
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
        for label in state.tabs.drain(..) {
            let view: &NSView = &label;
            view.removeFromSuperview();
        }
        if let Some(glass) = state.header.clone() {
            let host: &NSView = &glass;
            let label_h = 20.0_f64;
            let label_w = 160.0_f64;
            let y = (header_h - label_h) / 2.0;
            let mut x = 12.0_f64;
            for tab in &current.0.tabs {
                let label = NSTextField::labelWithString(&NSString::from_str(&tab.title), mtm);
                label.setFont(Some(&NSFont::systemFontOfSize(13.0)));
                let color = if tab.is_active {
                    NSColor::labelColor()
                } else {
                    NSColor::secondaryLabelColor()
                };
                label.setTextColor(Some(&color));
                let lview: &NSView = &label;
                lview.setFrame(NSRect::new(
                    NSPoint::new(x, y),
                    NSSize::new(label_w, label_h),
                ));
                host.addSubview(lview);
                state.tabs.push(label);
                x += label_w + 8.0;
            }
        }
    }
}
