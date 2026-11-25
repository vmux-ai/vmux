use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemState;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::LastActivatedAt;
use vmux_app::layout::{Hierarchy, Pane, PaneId, PaneSplit, PaneSplitDirection, SelectRequest};

fn main() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().layout(true).build());

    let root = app.world_mut().spawn(Name::new("Workspace")).id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(root),
        ))
        .id();
    let left = app
        .world_mut()
        .spawn((Pane, PaneId("editor".into()), ChildOf(split)))
        .id();
    let right = app
        .world_mut()
        .spawn((Pane, PaneId("terminal".into()), ChildOf(split)))
        .id();

    app.world_mut()
        .write_message(SelectRequest { root, pane: right });
    app.update();

    let mut state = SystemState::<Hierarchy>::new(app.world_mut());
    let hierarchy = state.get(app.world()).unwrap();
    let left_neighbor = hierarchy.neighbor_in(root, right, PaneSplitDirection::Row, false);
    println!(
        "selected right active={} left_neighbor_is_left={}",
        app.world().get::<LastActivatedAt>(right).is_some(),
        left_neighbor == Some(left),
    );
}
