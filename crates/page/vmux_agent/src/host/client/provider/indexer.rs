use bevy::prelude::*;

use crate::client::provider::index::ProviderStrategyIndex;
use crate::client::provider::strategy::StrategyKey;

pub fn on_strategy_added(
    trigger: On<Add, StrategyKey>,
    keys: Query<&StrategyKey>,
    mut idx: ResMut<ProviderStrategyIndex>,
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
    mut idx: ResMut<ProviderStrategyIndex>,
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
    use crate::client::provider::strategy::{EnvVarName, Strategy, StrategyKind, StrategyVariant};
    use crate::{AgentKind, AgentVariant};

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(ProviderStrategyIndex::default())
            .add_observer(on_strategy_added)
            .add_observer(on_strategy_removed);
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
    fn insert_strategy_updates_index() {
        let mut app = test_app();
        let e = app.world_mut().spawn(strategy_bundle("p", "m")).id();
        app.update();
        let idx = app.world().resource::<ProviderStrategyIndex>();
        assert_eq!(idx.get_by_strs("p", "m"), Some(e));
    }

    #[test]
    fn despawn_removes_from_index() {
        let mut app = test_app();
        let e = app.world_mut().spawn(strategy_bundle("p", "m")).id();
        app.update();
        app.world_mut().entity_mut(e).despawn();
        app.update();
        let idx = app.world().resource::<ProviderStrategyIndex>();
        assert!(idx.get_by_strs("p", "m").is_none());
    }
}
