use crate::NativeKey;
use crate::NativeKeyInput;
use crate::keyboard::{
    KeyboardEffect, KeyboardInitializeSet, KeyboardPress, KeyboardPublishSet, KeyboardRouteSet,
    KeyboardStream, KeyboardSuppression,
};
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::prelude::{
    App, Entity, MessageReader, MessageWriter, NonSend, NonSendMut, Plugin, Query, Res, With,
};
use bevy::winit::{EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSApp, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSView};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Instant;
use vmux_ecs::UnixMillis;

pub(crate) struct KeyboardNativePlugin;

impl Plugin for KeyboardNativePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, drain.after(KeyboardInitializeSet))
            .add_systems(Update, (retire, apply).chain().after(KeyboardRouteSet))
            .add_systems(Last, publish.after(KeyboardPublishSet))
            .insert_non_send(NativeKeyboardTransport::default());
    }
}

#[derive(Clone, Default)]
struct NativeKeyboardQueue(Rc<RefCell<NativeKeyboardMailbox>>);

#[derive(Default)]
struct NativeKeyboardMailbox {
    hint: Option<KeyboardSuppression>,
    next: u64,
    pending: VecDeque<KeyboardPress>,
    held: HashMap<u64, Retained<NSEvent>>,
    forwarded: HashMap<usize, Retained<NSEvent>>,
    overflowed: bool,
}

#[derive(Default)]
struct NativeKeyboardTransport {
    queue: NativeKeyboardQueue,
    monitor: Option<Retained<AnyObject>>,
}

impl NativeKeyboardQueue {
    fn enqueue(&self, event: &NSEvent) -> bool {
        let pointer = event as *const NSEvent as usize;
        let mut mailbox = self.0.borrow_mut();
        if mailbox.forwarded.remove(&pointer).is_some() {
            return false;
        }
        if event.r#type() != NSEventType::KeyDown {
            return false;
        }
        let Some(hint) = mailbox.hint.as_ref() else {
            return false;
        };
        let input = NativeEvent::input(event);
        let combo = input.key.map(|key| NativeKey {
            key,
            modifiers: input.modifiers,
        });
        let interested = hint.all_keys
            || !mailbox.held.is_empty()
            || !mailbox.forwarded.is_empty()
            || combo
                .as_ref()
                .is_some_and(|combo| hint.keys.contains(combo));
        if !interested {
            return false;
        }
        let stream = hint.stream;
        if mailbox.held.len() + mailbox.forwarded.len() >= 256 {
            mailbox.pending.clear();
            mailbox.held.clear();
            mailbox.overflowed = true;
            return false;
        }
        let Some(retained) = (unsafe { Retained::retain(event as *const NSEvent as *mut NSEvent) })
        else {
            return false;
        };
        mailbox.next = mailbox.next.wrapping_add(1).max(1);
        let token = mailbox.next;
        mailbox.held.insert(token, retained);
        mailbox.pending.push_back(KeyboardPress {
            stream,
            token,
            window: NativeEvent::owner(event),
            input,
            observed_at: Instant::now(),
            overflowed: false,
        });
        true
    }

    fn reset(&self) {
        let mut mailbox = self.0.borrow_mut();
        mailbox.hint = None;
        mailbox.pending.clear();
        mailbox.held.clear();
        mailbox.overflowed = false;
    }

    fn drain(&self) -> Vec<KeyboardPress> {
        let mut mailbox = self.0.borrow_mut();
        let mut events = Vec::new();
        if mailbox.overflowed {
            mailbox.overflowed = false;
            if let Some(hint) = &mailbox.hint {
                events.push(KeyboardPress {
                    stream: hint.stream,
                    token: 0,
                    window: None,
                    input: NativeEvent::empty(),
                    observed_at: Instant::now(),
                    overflowed: true,
                });
            }
        }
        events.extend(mailbox.pending.drain(..));
        events
    }

    fn apply(&self, effect: &KeyboardEffect) {
        let Some(event) = self.0.borrow_mut().held.remove(&effect.token) else {
            return;
        };
        if !effect.forward
            || self
                .0
                .borrow()
                .hint
                .as_ref()
                .is_none_or(|hint| hint.stream != effect.stream)
        {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if event.window(mtm).is_some_and(|window| !window.isVisible()) {
            return;
        }
        let pointer = &*event as *const NSEvent as usize;
        self.0.borrow_mut().forwarded.insert(pointer, event.clone());
        NSApp(mtm).postEvent_atStart(&event, false);
    }
}

impl NativeKeyboardTransport {
    fn install(&mut self, hint: KeyboardSuppression, proxy: &EventLoopProxyWrapper) {
        if self
            .queue
            .0
            .borrow()
            .hint
            .as_ref()
            .is_some_and(|old| old.stream != hint.stream)
        {
            self.close();
        }
        self.queue.0.borrow_mut().hint = Some(hint);
        if self.monitor.is_some() {
            return;
        }
        let queue = self.queue.clone();
        let proxy = (**proxy).clone();
        let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let consumed = queue.enqueue(unsafe { event.as_ref() });
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
            if consumed {
                std::ptr::null_mut()
            } else {
                event.as_ptr()
            }
        });
        let mask = NSEventMask::KeyDown | NSEventMask::KeyUp | NSEventMask::FlagsChanged;
        self.monitor =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    }

    fn close(&mut self) {
        self.queue.reset();
        if !self.queue.0.borrow().forwarded.is_empty() {
            return;
        }
        if let Some(monitor) = self.monitor.take() {
            unsafe {
                NSEvent::removeMonitor(&monitor);
            }
        }
    }
}

impl Drop for NativeKeyboardTransport {
    fn drop(&mut self) {
        self.queue.0.borrow_mut().forwarded.clear();
        self.close();
    }
}

fn drain(transport: NonSend<NativeKeyboardTransport>, mut inputs: MessageWriter<KeyboardPress>) {
    for input in transport.queue.drain() {
        inputs.write(input);
    }
}

fn retire(
    mut transport: NonSendMut<NativeKeyboardTransport>,
    streams: Query<(), With<KeyboardStream>>,
) {
    let owner = transport
        .queue
        .0
        .borrow()
        .hint
        .as_ref()
        .map(|hint| hint.stream);
    if owner.is_none_or(|owner| !streams.contains(owner)) {
        transport.close();
    }
}

fn apply(transport: NonSend<NativeKeyboardTransport>, mut effects: MessageReader<KeyboardEffect>) {
    for effect in effects.read() {
        transport.queue.apply(effect);
    }
}

fn publish(
    mut transport: NonSendMut<NativeKeyboardTransport>,
    mut hints: MessageReader<KeyboardSuppression>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let Some(proxy) = proxy else {
        hints.clear();
        return;
    };
    for hint in hints.read() {
        transport.install(hint.clone(), &proxy);
    }
}

struct NativeEvent;

impl NativeEvent {
    fn empty() -> NativeKeyInput {
        NativeKeyInput {
            key: None,
            native_code: 0,
            text: String::new(),
            modifiers: Default::default(),
            repeat: false,
            captured: false,
            claim: None,
            pressed_at_ms: 0,
        }
    }

    fn input(event: &NSEvent) -> NativeKeyInput {
        let flags = event.modifierFlags();
        NativeKeyInput {
            key: Self::key(event.keyCode()),
            native_code: event.keyCode(),
            text: event
                .charactersIgnoringModifiers()
                .map(|text| text.to_string())
                .unwrap_or_default(),
            modifiers: vmux_api::input::KeyModifiers {
                ctrl: flags.contains(NSEventModifierFlags::Control),
                shift: flags.contains(NSEventModifierFlags::Shift),
                alt: flags.contains(NSEventModifierFlags::Option),
                super_key: flags.contains(NSEventModifierFlags::Command),
            },
            repeat: event.isARepeat(),
            captured: false,
            claim: None,
            pressed_at_ms: UnixMillis::now().0,
        }
    }

    fn owner(event: &NSEvent) -> Option<Entity> {
        let mtm = MainThreadMarker::new()?;
        let mut window = event.window(mtm)?;
        for _ in 0..8 {
            let found = WINIT_WINDOWS.with_borrow(|native| {
                for entity in native.entity_to_winit.keys() {
                    let Some(native) = native.get_window(*entity) else {
                        continue;
                    };
                    let Ok(handle) = native.window_handle() else {
                        continue;
                    };
                    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                        continue;
                    };
                    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
                    if view
                        .window()
                        .is_some_and(|current| std::ptr::eq(&*current, &*window))
                    {
                        return Some(*entity);
                    }
                }
                None
            });
            if found.is_some() {
                return found;
            }
            window = window.parentWindow()?;
        }
        None
    }

    fn key(vk: u16) -> Option<KeyCode> {
        if vk < 0x40 {
            Self::low(vk)
        } else {
            Self::high(vk)
        }
    }

    fn low(vk: u16) -> Option<KeyCode> {
        Some(match vk {
            0x00 => KeyCode::KeyA,
            0x01 => KeyCode::KeyS,
            0x02 => KeyCode::KeyD,
            0x03 => KeyCode::KeyF,
            0x04 => KeyCode::KeyH,
            0x05 => KeyCode::KeyG,
            0x06 => KeyCode::KeyZ,
            0x07 => KeyCode::KeyX,
            0x08 => KeyCode::KeyC,
            0x09 => KeyCode::KeyV,
            0x0A => KeyCode::IntlBackslash,
            0x0B => KeyCode::KeyB,
            0x0C => KeyCode::KeyQ,
            0x0D => KeyCode::KeyW,
            0x0E => KeyCode::KeyE,
            0x0F => KeyCode::KeyR,
            0x10 => KeyCode::KeyY,
            0x11 => KeyCode::KeyT,
            0x12 => KeyCode::Digit1,
            0x13 => KeyCode::Digit2,
            0x14 => KeyCode::Digit3,
            0x15 => KeyCode::Digit4,
            0x16 => KeyCode::Digit6,
            0x17 => KeyCode::Digit5,
            0x18 => KeyCode::Equal,
            0x19 => KeyCode::Digit9,
            0x1A => KeyCode::Digit7,
            0x1B => KeyCode::Minus,
            0x1C => KeyCode::Digit8,
            0x1D => KeyCode::Digit0,
            0x1E => KeyCode::BracketRight,
            0x1F => KeyCode::KeyO,
            0x20 => KeyCode::KeyU,
            0x21 => KeyCode::BracketLeft,
            0x22 => KeyCode::KeyI,
            0x23 => KeyCode::KeyP,
            0x24 => KeyCode::Enter,
            0x25 => KeyCode::KeyL,
            0x26 => KeyCode::KeyJ,
            0x27 => KeyCode::Quote,
            0x28 => KeyCode::KeyK,
            0x29 => KeyCode::Semicolon,
            0x2A => KeyCode::Backslash,
            0x2B => KeyCode::Comma,
            0x2C => KeyCode::Slash,
            0x2D => KeyCode::KeyN,
            0x2E => KeyCode::KeyM,
            0x2F => KeyCode::Period,
            0x30 => KeyCode::Tab,
            0x31 => KeyCode::Space,
            0x32 => KeyCode::Backquote,
            0x33 => KeyCode::Backspace,
            0x35 => KeyCode::Escape,
            _ => return None,
        })
    }

    fn high(vk: u16) -> Option<KeyCode> {
        Some(match vk {
            0x40 => KeyCode::F17,
            0x41 => KeyCode::NumpadDecimal,
            0x43 => KeyCode::NumpadMultiply,
            0x45 => KeyCode::NumpadAdd,
            0x47 => KeyCode::NumpadClear,
            0x4B => KeyCode::NumpadDivide,
            0x4C => KeyCode::NumpadEnter,
            0x4E => KeyCode::NumpadSubtract,
            0x4F => KeyCode::F18,
            0x50 => KeyCode::F19,
            0x51 => KeyCode::NumpadEqual,
            0x52 => KeyCode::Numpad0,
            0x53 => KeyCode::Numpad1,
            0x54 => KeyCode::Numpad2,
            0x55 => KeyCode::Numpad3,
            0x56 => KeyCode::Numpad4,
            0x57 => KeyCode::Numpad5,
            0x58 => KeyCode::Numpad6,
            0x59 => KeyCode::Numpad7,
            0x5A => KeyCode::F20,
            0x5B => KeyCode::Numpad8,
            0x5C => KeyCode::Numpad9,
            0x5D => KeyCode::IntlYen,
            0x5E => KeyCode::IntlRo,
            0x5F => KeyCode::NumpadComma,
            0x60 => KeyCode::F5,
            0x61 => KeyCode::F6,
            0x62 => KeyCode::F7,
            0x63 => KeyCode::F3,
            0x64 => KeyCode::F8,
            0x65 => KeyCode::F9,
            0x66 => KeyCode::Lang2,
            0x67 => KeyCode::F11,
            0x68 => KeyCode::Lang1,
            0x69 => KeyCode::F13,
            0x6A => KeyCode::F16,
            0x6B => KeyCode::F14,
            0x6D => KeyCode::F10,
            0x6F => KeyCode::F12,
            0x71 => KeyCode::F15,
            0x72 => KeyCode::Insert,
            0x73 => KeyCode::Home,
            0x74 => KeyCode::PageUp,
            0x75 => KeyCode::Delete,
            0x76 => KeyCode::F4,
            0x77 => KeyCode::End,
            0x78 => KeyCode::F2,
            0x79 => KeyCode::PageDown,
            0x7A => KeyCode::F1,
            0x7B => KeyCode::ArrowLeft,
            0x7C => KeyCode::ArrowRight,
            0x7D => KeyCode::ArrowDown,
            0x7E => KeyCode::ArrowUp,
            _ => return None,
        })
    }
}
