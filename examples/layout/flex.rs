use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_math::Vec2;
use vmux_app::flex::{FlexDirection, UiRect, Val};
use vmux_app::prelude::{ComputedNode, FlexViewport, LayoutContext, Node, VmuxPlugin};

fn main() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().flex(true).build());

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
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(Val::Px(10.0)),
                ..Default::default()
            },
            FlexViewport(viewport),
        ))
        .id();
    let sidebar = app
        .world_mut()
        .spawn((
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
            Node {
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ChildOf(root),
        ))
        .id();

    app.update();
    println!(
        "logical sizes: sidebar={:?} content={:?}",
        app.world()
            .get::<ComputedNode>(sidebar)
            .unwrap()
            .to_logical()
            .size,
        app.world()
            .get::<ComputedNode>(content)
            .unwrap()
            .to_logical()
            .size,
    );
}
