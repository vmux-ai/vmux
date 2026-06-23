use bevy::prelude::*;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use vmux_layout::event::CEF_RESERVED_HEIGHT_PX;
use vmux_layout::native_view::LayoutRenderer;
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
    shown: bool,
}

fn sync_layout_glass(
    mut state: NonSendMut<LayoutGlassState>,
    renderer: Res<LayoutRenderer>,
    mode: Res<InteractionMode>,
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

    if state.header.is_none() {
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
}
