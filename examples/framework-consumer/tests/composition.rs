use bevy_app::App;
use bevy_ecs::prelude::*;
use framework_consumer::{Document, DocumentId, DocumentsPlugin, RenameRequest};
use vmux_app::ecs::{Operation, OperationState, Revision};
use vmux_app::manifest::{
    CompositionPlugin, FeatureManifest, FeatureManifestSource, FeaturePlugin, Profile, Registry,
};

struct Notes;

impl FeatureManifestSource for Notes {
    const SOURCE: &'static str = r#"(
        metadata: Some((
            id: "notes", version: "1.0.0",
            dependencies: [(id: "storage", version: "^1.0")],
            capabilities: ["storage.files"], permissions: ["data.write"],
            targets: ["headless", "daemon"], sdk: Some(">=0.0.0, <1.0.0"), bevy: Some("0.19"),
            lifecycle: (startup: ["PreStartup", "Update"], shutdown: ["OnExit"]),
        )),
        pages: [(url: "notes://", title: "Notes", permissions: ["PageReady"])],
        commands: [(id: "notes.rename", label: "Rename", group: "Notes", mcp: Some((description: "Rename a note", allow_agent: true)))],
        tools: [(name: "notes.inspect", description: "Inspect notes", input_schema: (type: Object))],
        cli: Some((commands: [(id: "notes.rename", name: "rename")])),
        mcp_servers: [(id: "notes", name: "Notes", url: "http://localhost:8080/mcp", scopes: ["notes"])],
    )"#;
}

struct Storage;

impl FeatureManifestSource for Storage {
    const SOURCE: &'static str = r#"(
        metadata: Some((id: "storage", version: "1.2.0", provides: ["storage.files"])),
    )"#;
}

struct Consumer {
    app: App,
}

impl Consumer {
    fn new(profile: Profile, notes: &str, storage: Option<&str>) -> Self {
        let mut app = App::new();
        app.add_plugins((
            CompositionPlugin(profile),
            DocumentsPlugin,
            FeaturePlugin::<Notes>::from_manifest(FeatureManifest::try_parse(notes).unwrap()),
        ));
        if let Some(storage) = storage {
            app.add_plugins(FeaturePlugin::<Storage>::from_manifest(
                FeatureManifest::try_parse(storage).unwrap(),
            ));
        }
        Self { app }
    }

    fn finish(&mut self) -> Result<(), String> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.app.finish()));
        match result {
            Ok(()) => {
                self.app.cleanup();
                Ok(())
            }
            Err(error) => {
                if let Some(error) = error.downcast_ref::<String>() {
                    return Err(error.clone());
                }
                if let Some(error) = error.downcast_ref::<&str>() {
                    return Err((*error).into());
                }
                Err("non-string startup failure".into())
            }
        }
    }
}

#[test]
fn public_feature_contributions_compose_without_product_adapters_and_preserve_typed_mutation() {
    let mut consumer = Consumer::new(
        Profile::new("headless").permission("data.write"),
        Notes::SOURCE,
        Some(Storage::SOURCE),
    );
    consumer.finish().unwrap();
    let manifest = consumer
        .app
        .world()
        .resource::<Registry>()
        .manifest::<Notes>()
        .unwrap();
    assert_eq!(
        manifest
            .pages
            .iter()
            .map(|page| page.url.as_str())
            .collect::<Vec<_>>(),
        ["notes://"]
    );
    assert_eq!(
        manifest
            .commands
            .iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>(),
        ["notes.rename"]
    );
    assert_eq!(
        manifest
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["notes.inspect"]
    );
    assert_eq!(
        manifest
            .cli
            .as_ref()
            .unwrap()
            .commands
            .iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>(),
        ["notes.rename"]
    );
    assert_eq!(
        manifest
            .mcp_servers
            .iter()
            .map(|server| server.id.as_str())
            .collect::<Vec<_>>(),
        ["notes"]
    );
    let root = consumer
        .app
        .world_mut()
        .spawn(Name::new("Independent Notes"))
        .id();
    let id = DocumentId("welcome".into());
    let document = consumer
        .app
        .world_mut()
        .spawn((
            Document,
            id.clone(),
            Name::new("Welcome"),
            Revision(0),
            ChildOf(root),
        ))
        .id();
    consumer.app.update();
    let operation = consumer
        .app
        .world_mut()
        .spawn(Operation::bundle("notes.rename").unwrap())
        .id();
    consumer.app.world_mut().write_message(RenameRequest {
        operation,
        owner: root,
        id,
        name: "Renamed through the feature".into(),
    });
    consumer.app.update();
    assert_eq!(
        consumer.app.world().get::<Name>(document).unwrap().as_str(),
        "Renamed through the feature"
    );
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Succeeded)
    );
}

#[test]
fn composition_refuses_missing_dependencies_versions_capabilities_permissions_and_targets() {
    for (storage, profile, source, expected) in [
        (
            false,
            Profile::new("headless").permission("data.write"),
            Notes::SOURCE.to_string(),
            "missing feature storage",
        ),
        (
            true,
            Profile::new("headless").permission("data.write"),
            Notes::SOURCE.replace("^1.0", "^2.0"),
            "requires storage ^2.0",
        ),
        (
            true,
            Profile::new("headless").permission("data.write"),
            Notes::SOURCE.replace("storage.files", "network.http"),
            "requires capability network.http",
        ),
        (
            true,
            Profile::new("headless"),
            Notes::SOURCE.to_string(),
            "host policy permission data.write",
        ),
        (
            true,
            Profile::new("web").permission("data.write"),
            Notes::SOURCE.to_string(),
            "does not support target web",
        ),
        (
            true,
            Profile::new("headless").permission("data.write"),
            Notes::SOURCE.replace(">=0.0.0, <1.0.0", "^99.0"),
            "requires SDK ^99.0",
        ),
        (
            true,
            Profile::new("headless").permission("data.write"),
            Notes::SOURCE.replace("0.19", "0.20"),
            "requires Bevy 0.20",
        ),
    ] {
        let mut consumer = Consumer::new(
            profile,
            &source,
            if storage { Some(Storage::SOURCE) } else { None },
        );
        let error = consumer.finish().unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
}

#[test]
fn host_capabilities_satisfy_explicit_requirements_without_an_extra_feature_registry() {
    let mut consumer = Consumer::new(
        Profile::new("daemon")
            .capability("network.http")
            .permission("data.write"),
        &Notes::SOURCE.replace("storage.files", "network.http"),
        Some(Storage::SOURCE),
    );
    consumer.finish().unwrap();
    assert!(
        consumer
            .app
            .world()
            .resource::<Registry>()
            .manifest::<Storage>()
            .is_some()
    );
}

#[test]
fn duplicate_feature_identity_and_dependency_cycles_are_refused_before_startup() {
    for (source, expected) in [
        (
            r#"(metadata: Some((id: "notes", version: "1.2.0", provides: ["storage.files"])))"#,
            "duplicate feature notes",
        ),
        (
            r#"(metadata: Some((id: "storage", version: "1.2.0", provides: ["storage.files"], dependencies: [(id: "notes", version: "^1.0")])))"#,
            "cyclic feature dependencies",
        ),
    ] {
        let mut consumer = Consumer::new(
            Profile::new("headless").permission("data.write"),
            Notes::SOURCE,
            Some(source),
        );
        let error = consumer.finish().unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn external_manifest_decoding_refuses_malformed_contracts_and_keeps_legacy_sources() {
    for source in [
        r#"(metadata: Some((id: "notes", version: "invalid")))"#,
        r#"(metadata: Some((id: "notes", version: "1.0.0", unexpected: true)))"#,
        r#"(metadata: Some((id: "notes", version: "1.0.0", sdk: Some("invalid"))))"#,
        r#"(metadata: Some((id: "notes", version: "1.0.0", dependencies: [(id: "notes", version: "^1")])))"#,
        r#"(metadata: Some((id: "notes", version: "1.0.0", provides: ["filesystem", "filesystem"])))"#,
    ] {
        assert!(FeatureManifest::try_parse(source).is_err(), "{source}");
    }
    let mut app = App::new();
    app.add_plugins(FeaturePlugin::<Storage>::default());
    app.finish();
    assert_eq!(app.world().resource::<Profile>().target(), "headless");
    assert!(FeatureManifest::try_parse("(name: \"Legacy\", commands: [])").is_ok());
}

#[test]
fn external_manifest_registration_cannot_replace_an_existing_feature() {
    let mut app = App::new();
    app.add_plugins(FeaturePlugin::<Storage>::default());
    let existing = app
        .world_mut()
        .query::<&FeatureManifest>()
        .single(app.world())
        .unwrap()
        .metadata
        .as_ref()
        .unwrap()
        .version
        .clone();
    let source = Storage::SOURCE.replace("1.2.0", "9.0.0");
    let manifest = FeatureManifest::try_parse(&source).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.add_plugins(FeaturePlugin::<Storage>::from_manifest(manifest));
    }));
    assert!(result.is_err());
    let current = app
        .world_mut()
        .query::<&FeatureManifest>()
        .single(app.world())
        .unwrap()
        .metadata
        .as_ref()
        .unwrap()
        .version
        .clone();
    assert_eq!(current, existing);
}
