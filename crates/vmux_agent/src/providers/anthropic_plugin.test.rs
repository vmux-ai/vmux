use super::*;
use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};
use serial_test::serial;

fn test_app() -> App {
    let mut app = App::new();
    app.insert_resource(PageStrategyIndex::default())
        .add_observer(on_strategy_added)
        .add_observer(on_strategy_removed)
        .add_plugins(AnthropicPlugin);
    app
}

#[test]
#[serial]
fn spawns_entity_when_env_var_set() {
    unsafe { std::env::set_var(super::super::anthropic::ENV_VAR, "x") };
    let mut app = test_app();
    app.update();
    let idx = app.world().resource::<PageStrategyIndex>();
    assert!(idx.get_by_strs("anthropic", "claude-sonnet-4-6").is_some());
    unsafe { std::env::remove_var(super::super::anthropic::ENV_VAR) };
}

#[test]
#[serial]
fn does_not_spawn_without_env_var() {
    unsafe { std::env::remove_var(super::super::anthropic::ENV_VAR) };
    let mut app = test_app();
    app.update();
    let idx = app.world().resource::<PageStrategyIndex>();
    assert!(idx.get_by_strs("anthropic", "claude-sonnet-4-6").is_none());
}
