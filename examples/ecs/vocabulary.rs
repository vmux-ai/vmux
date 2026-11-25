use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::ecs::{
    Icon, Loading, OperationPlugin, Order, OrderPlugin, Pin, PrimitivesPlugin, Ready, RenamePlugin,
    RenameRequest, ReorderRequest, StatusPlugin, Toggle, TogglesPlugin,
};

fn main() {
    let mut app = App::new();
    app.add_plugins((
        PrimitivesPlugin,
        OperationPlugin,
        RenamePlugin,
        OrderPlugin,
        TogglesPlugin,
        StatusPlugin,
    ));

    let root = app
        .world_mut()
        .spawn((Name::new("Workspace"), Icon::new("folder"), Loading))
        .id();
    let a = app
        .world_mut()
        .spawn((Name::new("Alpha"), Order(10), ChildOf(root)))
        .id();
    let b = app
        .world_mut()
        .spawn((Name::new("Beta"), Order(2), ChildOf(root)))
        .id();

    app.world_mut()
        .trigger(RenameRequest::new(root, "  Notes  "));
    app.world_mut().trigger(ReorderRequest::before(a, b));
    app.world_mut().trigger(Toggle::<Pin>::new(a));
    app.world_mut().entity_mut(root).insert(Ready);
    app.update();

    println!(
        "root={} ready={} loading={}",
        app.world().get::<Name>(root).unwrap().as_str(),
        app.world().get::<Ready>(root).is_some(),
        app.world().get::<Loading>(root).is_some(),
    );
    println!(
        "orders: alpha={:?} (pinned={}) beta={:?}",
        app.world().get::<Order>(a).unwrap().0,
        app.world().get::<Pin>(a).is_some(),
        app.world().get::<Order>(b).unwrap().0,
    );
}
