use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_command::island::{ISLAND_RENDER_EVENT, IslandRenderEvent, IslandState};

use super::event::*;
use super::state::{IslandInput, IslandMachine};

/// Bridge `SummonCommandBar` (global hotkey / `Cmd+K`) → an `ExpandSearch` intent.
pub fn summon_to_expand(
    mut summon: MessageReader<SummonCommandBar>,
    mut out: MessageWriter<IslandEvent>,
) {
    if summon.read().next().is_some() {
        out.write(IslandEvent::ExpandSearch);
    }
}

/// Reduce `IslandEvent`s into the morph state and, on change, push the new render state to the
/// island webview page via the host-emit bridge.
pub fn drive_island_state(
    mut machine: Local<IslandMachine>,
    mut seq: Local<u64>,
    mut events: MessageReader<IslandEvent>,
    island_q: Query<Entity, With<Island>>,
    mut commands: Commands,
    mut resize: MessageWriter<IslandPanelResize>,
) {
    let mut changed = false;
    for ev in events.read() {
        match ev.clone() {
            IslandEvent::ExpandSearch => machine.apply(IslandInput::ExpandSearch),
            IslandEvent::Collapse => machine.apply(IslandInput::Collapse),
            IslandEvent::Activity(a) => machine.apply(IslandInput::Activity(a)),
            IslandEvent::ActivityEnded(k) => machine.apply(IslandInput::ActivityEnded(k)),
            IslandEvent::Notify(_n) => {}
        }
        changed = true;
    }
    if !changed {
        return;
    }
    let Ok(entity) = island_q.single() else {
        return;
    };
    *seq += 1;
    let state = machine.render_state();
    let (width, height) = island_preset_size(&state);
    let payload = IslandRenderEvent { seq: *seq, state };
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        ISLAND_RENDER_EVENT,
        &payload,
    ));
    resize.write(IslandPanelResize { width, height });
}

/// Preset panel sizes per state for P1. (Page-reported content sizing is a follow-up.)
fn island_preset_size(state: &IslandState) -> (f32, f32) {
    match state {
        IslandState::Idle => (260.0, 40.0),
        IslandState::Search => (640.0, 420.0),
        IslandState::Activity(_) => (420.0, 44.0),
        IslandState::Notify(_) => (360.0, 44.0),
    }
}
