use bevy::prelude::*;
use bevy::prelude::{App, MessageWriter, Plugin, Query, Res, Window, With};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use vmux_api::input::KeyModifiers;

pub struct WheelPlugin;

impl Plugin for WheelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WheelInbox>()
            .add_message::<NativeWheelInput>()
            .add_systems(PreUpdate, receive.in_set(WheelInputSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WheelInputSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelPhase {
    Begin,
    Changed,
    End,
    Cancel,
    Momentum,
    Unsupported,
}

#[derive(Message, Clone, Copy)]
pub struct NativeWheelInput {
    pub window: Option<Entity>,
    pub position: Vec2,
    pub delta: Vec2,
    pub phase: WheelPhase,
    pub at: Instant,
    pub has_native_event: bool,
    pub precise: bool,
    pub modifiers: KeyModifiers,
}

#[derive(Resource, Clone, Default)]
pub struct WheelInbox {
    pending: Arc<Mutex<VecDeque<NativeWheelInput>>>,
    lost: Arc<AtomicBool>,
}

impl WheelInbox {
    pub fn record(&self, input: NativeWheelInput) {
        let Ok(mut pending) = self.pending.try_lock() else {
            self.lost.store(true, Ordering::SeqCst);
            return;
        };
        if pending.len() == 256 {
            pending.clear();
            self.lost.store(true, Ordering::SeqCst);
            return;
        }
        pending.push_back(input);
    }

    fn drain(&self) -> (bool, VecDeque<NativeWheelInput>) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let lost = self.lost.swap(false, Ordering::SeqCst);
        if lost {
            pending.clear();
        }
        (lost, std::mem::take(&mut pending))
    }
}

fn receive(
    inbox: Res<WheelInbox>,
    windows: Query<(), With<Window>>,
    mut inputs: MessageWriter<NativeWheelInput>,
) {
    let (lost, pending) = inbox.drain();
    if lost {
        inputs.write(NativeWheelInput {
            window: None,
            position: Vec2::ZERO,
            delta: Vec2::ZERO,
            phase: WheelPhase::Cancel,
            at: Instant::now(),
            has_native_event: false,
            precise: false,
            modifiers: KeyModifiers::default(),
        });
    }
    for input in pending {
        if input.window.is_some_and(|window| !windows.contains(window)) {
            continue;
        }
        inputs.write(input);
    }
}
