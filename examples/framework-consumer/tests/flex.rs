use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_math::Vec2;
use vmux_app::prelude::{ComputedNode, FlexViewport, LayoutContext, Node, VmuxPlugin};
use vmux_flex::{FlexDirection, UiRect, Val};

#[test]
fn a_public_facade_lays_out_owned_nodes_without_a_window_or_profile() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().flex(true).build());
    let owner = app
        .world_mut()
        .spawn(Name::new("Independent application"))
        .id();
    let viewport = app
        .world_mut()
        .spawn(LayoutContext {
            physical_size: Vec2::new(1200.0, 800.0),
            scale_factor: 2.0,
        })
        .id();
    let root = app
        .world_mut()
        .spawn((
            Name::new("Workspace"),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(Val::Px(10.0)),
                ..Default::default()
            },
            FlexViewport(viewport),
            ChildOf(owner),
        ))
        .id();
    let sidebar = app
        .world_mut()
        .spawn((
            Name::new("Sidebar"),
            Node {
                width: Val::Px(200.0),
                height: Val::Percent(100.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
            ChildOf(root),
        ))
        .id();
    let content = app
        .world_mut()
        .spawn((
            Name::new("Content"),
            Node {
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ChildOf(root),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world()
            .get::<ComputedNode>(sidebar)
            .unwrap()
            .to_logical()
            .size,
        Vec2::new(200.0, 380.0)
    );
    assert_eq!(
        app.world()
            .get::<ComputedNode>(content)
            .unwrap()
            .to_logical()
            .size,
        Vec2::new(380.0, 380.0)
    );
    app.world_mut()
        .get_mut::<LayoutContext>(viewport)
        .unwrap()
        .physical_size = Vec2::new(1600.0, 1000.0);
    app.update();
    assert_eq!(
        app.world()
            .get::<ComputedNode>(content)
            .unwrap()
            .to_logical()
            .size,
        Vec2::new(580.0, 480.0)
    );
}
