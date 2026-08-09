use super::*;

#[test]
fn strategy_key_equality_is_provider_then_model() {
    let a = StrategyKey {
        provider: "mistral".into(),
        model: "devstral-2".into(),
    };
    let b = StrategyKey {
        provider: "mistral".into(),
        model: "devstral-2".into(),
    };
    let c = StrategyKey {
        provider: "mistral".into(),
        model: "other".into(),
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}
