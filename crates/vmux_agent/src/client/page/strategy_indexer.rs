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
mod tests {
    use super::*;
    use crate::client::page::strategy_components::{
        EnvVarName, Strategy, StrategyKind, StrategyVariant,
    };
    use crate::{AgentKind, AgentVariant};

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app
    }

    fn strategy_bundle(provider: &str, model: &str) -> impl Bundle {
        (
            Strategy,
            StrategyKey {
                provider: provider.into(),
                model: model.into(),
            },
            EnvVarName("FAKE"),
            StrategyKind(AgentKind::Vibe),
            StrategyVariant(AgentVariant::Page),
        )
    }

    #[test]
    fn spawn_inserts_into_index() {
        let mut app = test_app();
        let e = app.world_mut().spawn(strategy_bundle("p", "m")).id();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert_eq!(idx.get_by_strs("p", "m"), Some(e));
    }

    #[test]
    fn despawn_removes_from_index() {
        let mut app = test_app();
        let e = app.world_mut().spawn(strategy_bundle("p", "m")).id();
        app.update();
        app.world_mut().entity_mut(e).despawn();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("p", "m").is_none());
    }
}
