use bevy_app::App;
use vmux_app::VmuxEcsPlugin;
use vmux_app::manifest::{
    CompositionPlugin, FeatureManifest, FeatureManifestSource, FeaturePlugin, Profile, Registry,
};

struct StorageFeature;

impl FeatureManifestSource for StorageFeature {
    const SOURCE: &'static str = r#"(
        metadata: Some((id: "example.storage", version: "1.2.0", provides: ["storage.files"])),
    )"#;
}

struct NotesFeature;

impl FeatureManifestSource for NotesFeature {
    const SOURCE: &'static str = r#"(
        metadata: Some((
            id: "example.notes",
            version: "1.0.0",
            dependencies: [(id: "example.storage", version: "^1.0")],
            capabilities: ["storage.files"],
            permissions: ["data.write"],
            targets: ["headless"],
            sdk: Some(">=0.0.0, <1.0.0"),
            bevy: Some("0.19"),
        )),
        pages: [(url: "notes://", title: "Notes", permissions: ["PageReady"])],
        commands: [(id: "notes.create", label: "Create Note", group: "Notes")],
    )"#;
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        CompositionPlugin(Profile::new("headless").permission("data.write")),
        VmuxEcsPlugin,
        FeaturePlugin::<StorageFeature>::from_manifest(
            FeatureManifest::try_parse(StorageFeature::SOURCE).unwrap(),
        ),
        FeaturePlugin::<NotesFeature>::from_manifest(
            FeatureManifest::try_parse(NotesFeature::SOURCE).unwrap(),
        ),
    ));
    app.finish();
    app.cleanup();

    let manifest = app
        .world()
        .resource::<Registry>()
        .manifest::<NotesFeature>()
        .unwrap();
    println!(
        "registered pages={} commands={}",
        manifest.pages.len(),
        manifest.commands.len(),
    );
}
