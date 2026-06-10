use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType, CGPreflightListenEventAccess,
    CGRequestListenEventAccess,
};
use vmux_command::AppCommand;

use crate::native_keyboard::{KeyAction, classify, key_code_from_vk, push_command};
use crate::shortcut::{KeyCombo, Modifiers};

struct TapState {
    port: CFRetained<CFMachPort>,
    wake: Box<dyn Fn()>,
}

thread_local! {
    static TAP_STATE: RefCell<Option<TapState>> = const { RefCell::new(None) };
}

fn modifiers_from_cg_flags(flags: CGEventFlags) -> Modifiers {
    Modifiers {
        ctrl: flags.contains(CGEventFlags::MaskControl),
        shift: flags.contains(CGEventFlags::MaskShift),
        alt: flags.contains(CGEventFlags::MaskAlternate),
        super_key: flags.contains(CGEventFlags::MaskCommand),
    }
}

fn combo_from_cg(keycode: u16, flags: CGEventFlags) -> Option<KeyCombo> {
    Some(KeyCombo {
        key: key_code_from_vk(keycode)?,
        modifiers: modifiers_from_cg_flags(flags),
    })
}

enum TapOutcome {
    Pass,
    Consume(Option<AppCommand>),
}

/// The session tap fires before any app, so it must hijack keys only while vmux is frontmost.
/// When not frontmost the classifier is never run, so no chord state is mutated and no command
/// is queued — other apps' keys pass through untouched.
fn gate(frontmost: bool, classify_fn: impl FnOnce() -> KeyAction) -> TapOutcome {
    if !frontmost {
        return TapOutcome::Pass;
    }
    match classify_fn() {
        KeyAction::Consume(cmd) => TapOutcome::Consume(cmd),
        KeyAction::PassThrough => TapOutcome::Pass,
    }
}

fn app_is_frontmost() -> bool {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(mtm).isActive()
}

unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    if event_type == CGEventType::TapDisabledByTimeout
        || event_type == CGEventType::TapDisabledByUserInput
    {
        TAP_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                CGEvent::tap_enable(&state.port, true);
            }
        });
        return event.as_ptr();
    }

    TAP_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            (state.wake)();
        }
    });

    if event_type != CGEventType::KeyDown {
        return event.as_ptr();
    }

    let ev = unsafe { event.as_ref() };
    let keycode = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode) as u16;
    let flags = CGEvent::flags(Some(ev));
    let Some(combo) = combo_from_cg(keycode, flags) else {
        return event.as_ptr();
    };

    match gate(app_is_frontmost(), || classify(combo)) {
        TapOutcome::Consume(cmd) => {
            if let Some(cmd) = cmd {
                push_command(cmd);
            }
            std::ptr::null_mut()
        }
        TapOutcome::Pass => event.as_ptr(),
    }
}

pub(crate) fn install_event_tap(proxy: Option<Res<EventLoopProxyWrapper>>) {
    let Some(proxy) = proxy else {
        return;
    };
    let proxy = (**proxy).clone();
    install(Box::new(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }));
}

fn install(wake: Box<dyn Fn()>) {
    // First run: show the Input Monitoring prompt. The grant only takes effect after a restart, so
    // the tap below still returns None this launch and shortcuts fall back to the local monitor.
    if !CGPreflightListenEventAccess() {
        CGRequestListenEventAccess();
    }

    let mask: CGEventMask =
        (1u64 << CGEventType::KeyDown.0) | (1u64 << CGEventType::FlagsChanged.0);
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(tap_callback),
            std::ptr::null_mut(),
        )
    };
    let Some(port) = port else {
        warn!(
            "event tap not created (Input Monitoring not granted?); shortcuts while a web page is focused need the grant + a restart"
        );
        return;
    };
    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        warn!("event tap: failed to create run loop source");
        return;
    };
    let Some(run_loop) = CFRunLoop::current() else {
        warn!("event tap: no current run loop");
        return;
    };
    let mode = unsafe { kCFRunLoopCommonModes };
    run_loop.add_source(Some(&source), mode);
    CGEvent::tap_enable(&port, true);
    std::mem::forget(source);
    TAP_STATE.with(|s| {
        *s.borrow_mut() = Some(TapState { port, wake });
    });
    info!("event tap installed (session keyDown/flagsChanged)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::KeyCode;
    use vmux_command::{LayoutCommand, PaneCommand};

    #[test]
    fn cg_flags_map_to_modifiers() {
        let m = modifiers_from_cg_flags(CGEventFlags::MaskCommand);
        assert!(m.super_key && !m.ctrl && !m.alt && !m.shift);

        let m = modifiers_from_cg_flags(CGEventFlags::MaskControl | CGEventFlags::MaskShift);
        assert!(m.ctrl && m.shift && !m.alt && !m.super_key);

        let m = modifiers_from_cg_flags(CGEventFlags::MaskAlternate);
        assert!(m.alt && !m.ctrl && !m.shift && !m.super_key);

        let m = modifiers_from_cg_flags(CGEventFlags::empty());
        assert!(!m.ctrl && !m.shift && !m.alt && !m.super_key);
    }

    #[test]
    fn combo_from_cg_resolves_known_keycode() {
        // 0x09 == kVK_ANSI_V
        let combo = combo_from_cg(0x09, CGEventFlags::MaskCommand).expect("combo");
        assert_eq!(combo.key, KeyCode::KeyV);
        assert!(combo.modifiers.super_key);
    }

    #[test]
    fn combo_from_cg_rejects_unknown_keycode() {
        assert!(combo_from_cg(0xFFFF, CGEventFlags::empty()).is_none());
    }

    #[test]
    fn gate_passes_without_classifying_when_not_frontmost() {
        let outcome = gate(false, || panic!("must not classify when not frontmost"));
        assert!(matches!(outcome, TapOutcome::Pass));
    }

    #[test]
    fn gate_consumes_when_frontmost_and_classifier_consumes() {
        let cmd = AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SelectLeft));
        let outcome = gate(true, || KeyAction::Consume(Some(cmd.clone())));
        match outcome {
            TapOutcome::Consume(Some(c)) => assert_eq!(c, cmd),
            _ => panic!("expected consume"),
        }
    }

    #[test]
    fn gate_passes_when_frontmost_and_classifier_passes() {
        let outcome = gate(true, || KeyAction::PassThrough);
        assert!(matches!(outcome, TapOutcome::Pass));
    }
}
