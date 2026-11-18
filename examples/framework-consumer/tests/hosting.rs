use bevy_app::App;
use bevy_ecs::message::MessageCursor;
use bevy_ecs::prelude::*;
use framework_consumer::counter::{Count, CountRequest, NativePlugin};
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{EffectiveStartupUrl, Ready};
use vmux_app::manifest::hosting::{
    Contract, HostedApplication, HostedApplicationReady, Package, WebHostPlugin,
};

const WEB: &str = r#"(
    schema_version: 1,
    sdk_revision: "1111111111111111111111111111111111111111",
    entry: "https://example.test/app/",
    manifest: (
        metadata: Some((id: "example.web", version: "1.0.0", targets: ["headless"])),
        pages: [(url: "https://example.test/app/", title: "Example", startup: true, permissions: ["PageReady"])],
    ),
)"#;

struct Consumer {
    app: App,
}

impl Consumer {
    fn new(source: &str, contract: Contract, native: bool) -> Self {
        let mut app = App::new();
        app.add_plugins(VmuxPlugin::builder().ecs(true).build());
        if native {
            app.add_plugins(NativePlugin);
        }
        app.add_plugins(WebHostPlugin::new(
            contract,
            Package::try_parse(source).unwrap(),
        ));
        Self { app }
    }

    fn finish(&mut self) -> Result<(), String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.app.finish())) {
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
                Err("non-string package failure".into())
            }
        }
    }

    fn native_source() -> String {
        WEB.replace("version: \"1.0.0\", targets:", "version: \"1.0.0\", dependencies: [(id: \"native.count\", version: \"^1.0\")], capabilities: [\"native.count\"], targets:")
            .replace("\"PageReady\"", "\"PageReady\", \"CountRequest\"")
    }
}

#[test]
fn a_web_package_publishes_its_canonical_startup_once_without_a_renderer() {
    let mut consumer = Consumer::new(
        WEB,
        Contract::new("1111111111111111111111111111111111111111"),
        false,
    );
    consumer.finish().unwrap();
    let world = consumer.app.world_mut();
    let (entity, url, _) = world
        .query_filtered::<(Entity, &EffectiveStartupUrl, &Ready), With<HostedApplication>>()
        .single(world)
        .unwrap();
    assert_eq!(url.0, "https://example.test/app/");
    let mut cursor = MessageCursor::<HostedApplicationReady>::default();
    consumer.app.update();
    let messages = consumer
        .app
        .world()
        .resource::<Messages<HostedApplicationReady>>();
    assert_eq!(
        cursor
            .read(messages)
            .map(|event| event.entity)
            .collect::<Vec<_>>(),
        [entity]
    );
    consumer.app.update();
    let messages = consumer
        .app
        .world()
        .resource::<Messages<HostedApplicationReady>>();
    assert_eq!(cursor.read(messages).count(), 0);
}

#[test]
fn packages_refuse_incompatible_hosts_privileged_urls_and_native_payloads() {
    for (source, expected) in [
        (WEB.replace("schema_version: 1", "schema_version: 9"), "unsupported package schema"),
        (WEB.replace("1111111111111111111111111111111111111111", "2222222222222222222222222222222222222222"), "revision does not match"),
        (WEB.replace("1111111111111111111111111111111111111111", "main"), "immutable Git revision"),
        (WEB.replace("startup: true", "startup: false"), "declared startup page"),
        (WEB.replace("https://example.test/app/", "file:///private/data"), "HTTP(S) URLs"),
        (WEB.replace("https://example.test/app/", "https://name:secret@example.test/app/"), "credential-free"),
        (WEB.replace("title: \"Example\"", "title: \"Example\", asset_host: \"vmux_ui\""), "embedded asset hosts"),
        (WEB.replace("title: \"Example\"", "title: \"Example\", replaces_command: \"native.delete\""), "native command replacements"),
        (WEB.replace("\"PageReady\"", "\"DeleteSecretRequest\""), "host does not provide page event"),
        (WEB.replace("version: \"1.0.0\", targets:", "version: \"1.0.0\", provides: [\"native.count\"], targets:"), "linked feature plugins"),
        (WEB.replace("pages: [", "commands: [(id: \"native.delete\", label: \"Delete\", group: \"Native\")], pages: ["), "linked feature plugins"),
        (WEB.replace("pages: [", "root: true, pages: ["), "linked feature plugins"),
        (WEB.replace("pages: [", "flags: [(key: \"remote.live\", type: Boolean, class: Release, default: Boolean(true), owner: \"example\")], pages: ["), "linked feature plugins"),
        (WEB.replace("pages: [", "auth_providers: [(id: \"external\", label: \"External\")], pages: ["), "linked feature plugins"),
    ] {
        let mut consumer = Consumer::new(&source, Contract::new("1111111111111111111111111111111111111111"), false);
        let error = consumer.finish().unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
        assert!(consumer.app.world().resource::<Messages<HostedApplicationReady>>().is_empty());
    }
}

#[test]
fn linked_native_plugins_extend_the_same_web_package_through_typed_contracts() {
    let source = Consumer::native_source();
    let mut consumer = Consumer::new(
        &source,
        Contract::new("1111111111111111111111111111111111111111").event::<CountRequest>(),
        true,
    );
    consumer.finish().unwrap();
    consumer
        .app
        .world_mut()
        .write_message(CountRequest { amount: 7 });
    consumer.app.update();
    let world = consumer.app.world_mut();
    assert_eq!(world.query::<&Count>().single(world).unwrap().0, 7);
    assert_eq!(
        world
            .query_filtered::<&EffectiveStartupUrl, With<HostedApplication>>()
            .single(world)
            .unwrap()
            .0,
        "https://example.test/app/"
    );
    for (native, contract, expected) in [
        (
            false,
            Contract::new("1111111111111111111111111111111111111111").event::<CountRequest>(),
            "missing feature native.count",
        ),
        (
            true,
            Contract::new("1111111111111111111111111111111111111111"),
            "host does not provide page event CountRequest",
        ),
    ] {
        let mut consumer = Consumer::new(&source, contract, native);
        let error = consumer.finish().unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn external_package_decoding_refuses_unknown_fields_and_oversized_payloads() {
    assert!(
        Package::try_parse(&WEB.replace("schema_version: 1", "schema_version: 1, unknown: true"))
            .is_err()
    );
    assert!(Package::try_parse(&" ".repeat(1024 * 1024 + 1)).is_err());
}
