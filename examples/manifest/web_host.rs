use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{EffectiveStartupUrl, Ready};
use vmux_app::manifest::hosting::{
    Contract, HostedApplication, HostedApplicationReady, Package, WebHostPlugin,
};

const PACKAGE: &str = r#"(
    schema_version: 1,
    sdk_revision: "1111111111111111111111111111111111111111",
    entry: "https://example.test/app/",
    manifest: (
        metadata: Some((id: "example.web", version: "1.0.0", targets: ["headless"])),
        pages: [(url: "https://example.test/app/", title: "Example", startup: true, permissions: ["PageReady"])],
    ),
)"#;

fn main() {
    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().ecs(true).build(),
        WebHostPlugin::new(
            Contract::new("1111111111111111111111111111111111111111"),
            Package::try_parse(PACKAGE).unwrap(),
        ),
    ));
    app.finish();
    app.cleanup();
    app.update();

    let world = app.world_mut();
    let (entity, url) = world
        .query_filtered::<(Entity, &EffectiveStartupUrl), (With<HostedApplication>, With<Ready>)>()
        .single(world)
        .unwrap();
    let published = !world
        .resource::<Messages<HostedApplicationReady>>()
        .is_empty();
    println!(
        "hosted entity={entity:?} startup_url={} ready_message={published}",
        url.0,
    );
}
