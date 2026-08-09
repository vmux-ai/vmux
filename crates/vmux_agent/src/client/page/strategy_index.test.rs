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
