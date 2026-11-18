use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::manifest::{FeatureManifestOwner, FeatureManifestSource, FeaturePlugin};

pub struct NativePlugin;

impl Plugin for NativePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<Native>::default())
            .add_message::<CountRequest>()
            .add_systems(Update, count);
        let world = app.world_mut();
        let entity = world
            .query_filtered::<Entity, With<FeatureManifestOwner<Native>>>()
            .single(world)
            .unwrap();
        world.entity_mut(entity).insert(Count(0));
    }
}

struct Native;

impl FeatureManifestSource for Native {
    const SOURCE: &'static str =
        r#"(metadata: Some((id: "native.count", version: "1.0.0", provides: ["native.count"])))"#;
}

#[vmux_api::ui_event]
#[derive(Message)]
pub struct CountRequest {
    pub amount: u32,
}

#[derive(Component)]
pub struct Count(pub u32);

fn count(
    mut requests: MessageReader<CountRequest>,
    mut values: Query<&mut Count, With<FeatureManifestOwner<Native>>>,
) {
    for request in requests.read() {
        for mut value in &mut values {
            value.0 += request.amount;
        }
    }
}
