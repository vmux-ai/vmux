use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_command::island::{ISLAND_RENDER_EVENT, IslandRenderEvent};

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
    let payload = IslandRenderEvent {
        seq: *seq,
        state: machine.render_state(),
    };
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        ISLAND_RENDER_EVENT,
        &payload,
    ));
}
