use std::collections::HashMap;

use bevy::prelude::*;

use crate::client::page::strategy_components::StrategyKey;

#[derive(Resource, Default, Debug)]
pub struct PageStrategyIndex {
    by_key: HashMap<StrategyKey, Entity>,
}

impl PageStrategyIndex {
    pub fn insert(&mut self, key: StrategyKey, entity: Entity) {
        self.by_key.insert(key, entity);
    }

    pub fn remove(&mut self, key: &StrategyKey) -> Option<Entity> {
        self.by_key.remove(key)
    }

    pub fn get(&self, key: &StrategyKey) -> Option<Entity> {
        self.by_key.get(key).copied()
    }

    pub fn get_by_strs(&self, provider: &str, model: &str) -> Option<Entity> {
        self.by_key
            .get(&StrategyKey {
                provider: provider.to_string(),
                model: model.to_string(),
            })
            .copied()
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_round_trip() {
        let mut idx = PageStrategyIndex::default();
        let e = Entity::PLACEHOLDER;
        let key = StrategyKey {
            provider: "mistral".to_string(),
            model: "devstral-2".to_string(),
        };
        idx.insert(key.clone(), e);
        assert_eq!(idx.get(&key), Some(e));
        assert_eq!(idx.get_by_strs("mistral", "devstral-2"), Some(e));
        assert_eq!(idx.remove(&key), Some(e));
        assert_eq!(idx.get(&key), None);
    }
}
