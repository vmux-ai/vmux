use bevy::prelude::*;

use crate::client::page::strategy_components::StrategyKey;
use crate::client::page::strategy_index::PageStrategyIndex;

pub fn on_strategy_added(
    trigger: On<Add, StrategyKey>,
    keys: Query<&StrategyKey>,
    mut idx: ResMut<PageStrategyIndex>,
) {
    let e = trigger.event_target();
    let Ok(key) = keys.get(e) else {
        return;
    };
    idx.insert(key.clone(), e);
}

pub fn on_strategy_removed(
    trigger: On<Remove, StrategyKey>,
    keys: Query<&StrategyKey>,
    mut idx: ResMut<PageStrategyIndex>,
) {
    let e = trigger.event_target();
    let Ok(key) = keys.get(e) else {
        return;
    };
    idx.remove(key);
}

#[cfg(test)]
#[path = "strategy_indexer.test.rs"]
mod tests;
