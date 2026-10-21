use std::collections::HashMap;
use std::hash::Hash;

use dioxus::prelude::*;

use crate::platform::Platform;

const OPTIMISTIC_TIMEOUT_MS: u32 = 10_000;

#[derive(Clone)]
struct Pending<V> {
    baseline: V,
    expected: V,
    generation: u64,
}

pub struct OptimisticMap<K: 'static, V: 'static> {
    pending: Signal<HashMap<K, Pending<V>>>,
    generation: Signal<u64>,
}

impl<K: 'static, V: 'static> Clone for OptimisticMap<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: 'static, V: 'static> Copy for OptimisticMap<K, V> {}

impl<K, V> OptimisticMap<K, V>
where
    K: Clone + Eq + Hash + 'static,
    V: Clone + PartialEq + 'static,
{
    pub fn value(self, key: &K, observed: &V) -> V {
        self.pending
            .read()
            .get(key)
            .map(|pending| pending.expected.clone())
            .unwrap_or_else(|| observed.clone())
    }

    pub fn set(mut self, key: K, baseline: V, expected: V) {
        if baseline == expected {
            self.pending.write().remove(&key);
            return;
        }
        let generation = (self.generation)().wrapping_add(1).max(1);
        self.generation.set(generation);
        self.pending.write().insert(
            key.clone(),
            Pending {
                baseline,
                expected,
                generation,
            },
        );
        spawn(async move {
            Platform::sleep(OPTIMISTIC_TIMEOUT_MS).await;
            let expired = self
                .pending
                .peek()
                .get(&key)
                .is_some_and(|pending| pending.generation == generation);
            if expired {
                self.pending.write().remove(&key);
            }
        });
    }

    pub fn clear(mut self, key: &K) {
        self.pending.write().remove(key);
    }
}

pub fn use_optimistic_map<K, V>(observed: HashMap<K, V>) -> OptimisticMap<K, V>
where
    K: Clone + Eq + Hash + 'static,
    V: Clone + PartialEq + 'static,
{
    let mut pending = use_signal(HashMap::<K, Pending<V>>::new);
    let generation = use_signal(|| 0u64);
    use_effect(use_reactive!(|observed| {
        pending.write().retain(|key, pending| {
            observed
                .get(key)
                .is_some_and(|value| value != &pending.expected && value == &pending.baseline)
        });
    }));
    OptimisticMap {
        pending,
        generation,
    }
}
