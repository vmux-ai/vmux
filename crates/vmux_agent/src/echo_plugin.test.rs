use super::*;
use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};

fn test_app() -> App {
    let mut app = App::new();
    app.insert_resource(PageStrategyIndex::default())
        .add_observer(on_strategy_added)
        .add_observer(on_strategy_removed)
        .add_plugins(EchoPlugin);
    app
}

#[test]
fn spawns_echo_entity_without_any_env_var() {
    let mut app = test_app();
    app.update();
    let idx = app.world().resource::<PageStrategyIndex>();
    assert!(idx.get_by_strs("echo", "echo").is_some());
}

#[test]
fn dedup_guard_does_not_double_spawn() {
    let mut app = test_app();
    app.update();
    app.update();
    let count = app
        .world_mut()
        .query::<&EchoProvider>()
        .iter(app.world())
        .count();
    assert_eq!(count, 1);
    let idx = app.world().resource::<PageStrategyIndex>();
    assert!(idx.get_by_strs("echo", "echo").is_some());
}
