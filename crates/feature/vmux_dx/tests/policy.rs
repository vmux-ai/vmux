use compiler_inputs::CompilerInputs;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::ext::IdentExt;
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, Item, Type};

#[path = "../src/compiler_inputs.rs"]
mod compiler_inputs;

const FROZEN_REVISION: &str = "a59db6addc6195f4673b5aacd2d604a96b50272c";

const UPSTREAM_SETS: &[(&str, &str)] = &[
    ("bevy_cef", "cf3bf141760a42d3"),
    ("bevy_cef_core", "d551cf781692e304"),
    ("bevy_window", "cc0c3170ee3db084"),
    ("bevy_world_serialization", "c106988fe826b720"),
    ("cargo-packager", "7f182f909cfa41e1"),
    ("bevy_cef_debug_render_process", "315c42c431210bdc"),
];

#[test]
fn newly_loaded_implementations_prove_only_their_qualified_owner_references() {
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/util/ownership/src/lib.rs",
        "mod first { pub struct Owner; } mod second { pub struct Owner; } pub struct Value; mod access;",
    );
    for owner in ["first::Owner", "second::Owner"] {
        let owner = inventory
            .resolve("ownership", owner, &mut BTreeSet::new())
            .unwrap();
        let value = inventory
            .resolve("ownership", "Value", &mut BTreeSet::new())
            .unwrap();
        assert!(!inventory.owner_references(owner, value));
    }
    inventory.source(
        "crates/util/ownership/src/access.rs",
        "impl crate::first::Owner { fn value() -> crate::Value { crate::Value } }",
    );
    for (owner, expected) in [("first::Owner", true), ("second::Owner", false)] {
        let owner = inventory
            .resolve("ownership", owner, &mut BTreeSet::new())
            .unwrap();
        let value = inventory
            .resolve("ownership", "Value", &mut BTreeSet::new())
            .unwrap();
        assert_eq!(inventory.owner_references(owner, value), expected);
    }
}

#[test]
fn cached_namespaces_preserve_cyclic_glob_ambiguity_in_both_query_orders() {
    for reverse in [false, true] {
        let inventory = Inventory::fixture(
            "mod left { pub mod leaf {} } mod right { pub mod leaf {} } mod a { pub use crate::b::*; pub use crate::left::*; } mod b { pub use crate::a::*; pub use crate::right::*; }",
        );
        let mut ambiguous = ["a::leaf", "b::leaf"];
        if reverse {
            ambiguous.reverse();
        }
        for _ in 0..2 {
            for prefix in ambiguous {
                assert_eq!(
                    inventory.glob_namespace("fixture", prefix, &mut BTreeSet::new()),
                    None,
                    "{prefix}"
                );
            }
            assert_eq!(
                inventory
                    .glob_namespace("fixture", "left::leaf", &mut BTreeSet::new())
                    .as_deref(),
                Some("fixture::left::leaf")
            );
            assert_eq!(
                inventory
                    .glob_namespace("fixture", "right::leaf", &mut BTreeSet::new())
                    .as_deref(),
                Some("fixture::right::leaf")
            );
        }
    }
}

#[test]
fn cached_missing_modules_are_resolved_after_source_arrives() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/cache_fixture/src/lib.rs", "pub mod child;");
    for _ in 0..2 {
        assert_eq!(
            inventory.module_binding("cache_fixture", "child", &mut BTreeSet::new()),
            None
        );
        assert_eq!(
            inventory.glob_namespace("cache_fixture", "child", &mut BTreeSet::new()),
            None
        );
        assert!(
            inventory
                .resolve("cache_fixture", "child::Value", &mut BTreeSet::new())
                .is_none()
        );
    }
    inventory.source(
        "crates/util/cache_fixture/src/child.rs",
        "pub mod leaf {} pub struct Value;",
    );
    inventory.resolve_usage();
    for _ in 0..2 {
        assert_eq!(
            inventory
                .module_binding("cache_fixture", "child", &mut BTreeSet::new())
                .as_deref(),
            Some("cache_fixture::child")
        );
        assert_eq!(
            inventory
                .glob_namespace("cache_fixture", "child::leaf", &mut BTreeSet::new())
                .as_deref(),
            Some("cache_fixture::child::leaf")
        );
        assert_eq!(
            inventory
                .resolve("cache_fixture", "child::Value", &mut BTreeSet::new())
                .unwrap()
                .key,
            "crates/util/cache_fixture/src/child.rs||struct:Value"
        );
    }
}

#[test]
fn cached_declaration_lookup_keeps_the_callers_cycle_refusal() {
    let inventory =
        Inventory::fixture("mod first { pub struct Value; } mod second { pub struct Value; }");
    for _ in 0..2 {
        for (path, expected) in [
            ("first::Value", "fixture.rs|first|struct:Value"),
            ("second::Value", "fixture.rs|second|struct:Value"),
        ] {
            let mut visiting = BTreeSet::new();
            assert_eq!(
                inventory
                    .resolve("fixture", path, &mut visiting)
                    .unwrap()
                    .key,
                expected
            );
            assert!(inventory.resolve("fixture", path, &mut visiting).is_none());
        }
    }
}

#[test]
fn bevy_plugins_use_descriptive_names_with_plugin_suffix() {
    let inventory = Inventory::workspace(&Workspace::current().0);
    let violations = inventory.plugin_names();
    assert!(
        violations.is_empty(),
        "Bevy plugin types and aliases must have descriptive names ending in Plugin:\n{}",
        violations.join("\n")
    );
}

#[test]
fn plugin_names_follow_framework_and_type_aliases() {
    for (source, expected) in [
        (
            "use bevy_app::Plugin; struct HttpPlugin; impl Plugin for HttpPlugin {}",
            vec![],
        ),
        (
            "struct Plugin; impl bevy_app::Plugin for Plugin {}",
            vec!["Plugin"],
        ),
        (
            "use bevy_app::Plugin as Engine; struct Http; impl Engine for Http {}",
            vec!["Http"],
        ),
        (
            "struct Http; type HttpPlugin = Http; impl bevy_app::Plugin for HttpPlugin {}",
            vec!["Http"],
        ),
        (
            "mod actual { pub struct Http; } use actual::Http as HttpPlugin; impl bevy_app::Plugin for HttpPlugin {}",
            vec!["Http"],
        ),
        (
            "mod actual { pub struct HttpPlugin; impl bevy_app::Plugin for HttpPlugin {} } pub use actual::HttpPlugin as Plugin;",
            vec!["Plugin"],
        ),
        (
            "trait Plugin {} struct Http; impl Plugin for Http {}",
            vec![],
        ),
        (
            "const SOURCE: &str = \"struct Plugin; impl bevy_app::Plugin for Plugin {}\";",
            vec![],
        ),
    ] {
        let inventory = Inventory::fixture(source);
        let violations = inventory.plugin_names();
        let names: Vec<_> = violations
            .iter()
            .map(|violation| violation.rsplit_once(": ").unwrap().1)
            .collect();
        assert_eq!(names, expected, "{source}");
    }
}

#[test]
fn plugin_naming_covers_owned_source_and_preserves_vendored_types() {
    for (path, rejected) in [
        ("crates/sample/src/lib.rs", true),
        ("examples/sample/src/lib.rs", true),
        ("website/src/lib.rs", true),
        ("scripts/sample.rs", true),
        ("patches/sample/src/lib.rs", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(path, "struct Http; impl bevy_app::Plugin for Http {}");
        assert_eq!(!inventory.plugin_names().is_empty(), rejected, "{path}");
    }
    for (name, rejected) in [("Plugin", true), ("Http", true), ("HttpPlugin", false)] {
        let mut inventory = Inventory::default();
        inventory.source(
            "patches/vendor/src/lib.rs",
            "pub struct Http; impl bevy_app::Plugin for Http {}",
        );
        inventory.source(
            "crates/sample/src/lib.rs",
            &format!("pub use vendor::Http as {name};"),
        );
        assert_eq!(!inventory.plugin_names().is_empty(), rejected, "{name}");
    }
    for (name, rejected) in [("Http", true), ("HttpPlugin", false)] {
        let mut inventory = Inventory::default();
        inventory.source(
            "patches/vendor/src/lib.rs",
            &format!("pub struct {name}; impl bevy_app::Plugin for {name} {{}}"),
        );
        inventory.source("crates/sample/src/lib.rs", "pub use vendor::*;");
        assert_eq!(!inventory.plugin_names().is_empty(), rejected, "{name}");
    }
}

#[test]
fn compiler_inputs_require_reviewed_data_flow_and_cannot_be_product_modules() {
    for change in [
        "valid",
        "unused",
        "shadowed",
        "assigned",
        "renamed",
        "receiver",
        "disabled",
        "disconnected",
        "cargo",
        "macro",
        "content",
        "module",
        "plain_module",
        "inline_module",
        "target",
    ] {
        let fixture = CompilerInputFixture::new();
        let source = fixture
            .0
            .path()
            .join("crates/feature/vmux_dx/lints/src/registrations.rs");
        let original = std::fs::read_to_string(&source).unwrap();
        match change {
            "unused" => std::fs::write(
                &source,
                original.replace(
                    "fixture.check(name, source, expected);",
                    "fixture.check(name, \"\", expected);",
                ),
            )
            .unwrap(),
            "shadowed" => std::fs::write(
                &source,
                original.replace(
                    "fixture.check(name, source, expected);",
                    "let source = \"\"; fixture.check(name, source, expected);",
                ),
            )
            .unwrap(),
            "cargo" => {
                std::fs::write(
                    &source,
                    original.replace("Command::new(\"cargo\")", "Command::new(\"echo\")"),
                )
                .unwrap();
                fixture.refresh_owner();
            }
            "renamed" => {
                let source_text = original
                    .replace("source", "code")
                    .replace("fn check(", "fn compile(")
                    .replace("fixture.check(", "fixture.compile(")
                    .replace("fn new(", "fn start(")
                    .replace("Fixture::new()", "Fixture::start()");
                std::fs::write(&source, source_text).unwrap();
                fixture.refresh_owner();
            }
            "assigned" => std::fs::write(
                &source,
                original
                    .replace(
                        "for (name, source, expected)",
                        "for (name, mut source, expected)",
                    )
                    .replace(
                        "fixture.check(name, source, expected);",
                        "source = \"\"; fixture.check(name, source, expected);",
                    ),
            )
            .unwrap(),
            "receiver" => std::fs::write(
                &source,
                original.replace(
                    "for (name, source, expected)",
                    "let fixture = Other; for (name, source, expected)",
                ),
            )
            .unwrap(),
            "disabled" => std::fs::write(
                &source,
                original.replace("#[test]", "#[test] #[cfg(any())]"),
            )
            .unwrap(),
            "disconnected" => {
                let source_text = original.replace(
                    "let output = Command::new(\"cargo\")",
                    "let _unused = Command::new(\"cargo\"); let output = Other",
                );
                std::fs::write(&source, source_text).unwrap();
                fixture.refresh_owner();
            }
            "macro" => std::fs::write(
                &source,
                format!("macro_rules! include_str {{ ($path:literal) => {{ \"\" }} }}\n{original}"),
            )
            .unwrap(),
            "content" => std::fs::write(fixture.input("private"), "fn main() {}\n").unwrap(),
            "module" => std::fs::write(
                source.parent().unwrap().join("lib.rs"),
                "#[path=\"../ui/private.rs\"] mod input;",
            )
            .unwrap(),
            "plain_module" => {
                let input = source.parent().unwrap().join("domain.rs");
                std::fs::rename(fixture.input("private"), &input).unwrap();
                std::fs::write(&source, original.replace("../ui/private.rs", "domain.rs")).unwrap();
                let root = fixture.0.path().join("crates/feature/vmux_dx");
                let mut policy = Policy::read(&root.join("ecs-policy.ron")).unwrap();
                let record = policy
                    .boundaries
                    .iter_mut()
                    .find(|record| record.key.ends_with("/ui/private.rs||script"))
                    .unwrap();
                record.key = "crates/feature/vmux_dx/lints/src/domain.rs||script".into();
                std::fs::write(
                    root.join("ecs-policy.ron"),
                    ron::to_string(&policy).unwrap(),
                )
                .unwrap();
                std::fs::write(source.parent().unwrap().join("lib.rs"), "mod domain;").unwrap();
            }
            "inline_module" => {
                std::fs::create_dir(source.parent().unwrap().join("nested")).unwrap();
                std::fs::write(
                    source.parent().unwrap().join("lib.rs"),
                    "mod nested { #[path=\"../../ui/private.rs\"] mod input; }",
                )
                .unwrap();
            }
            "target" => std::fs::write(
                source
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("Cargo.toml"),
                "[package]\nname=\"fixture\"\nversion=\"0.0.0\"\n[lib]\npath=\"ui/private.rs\"\n",
            )
            .unwrap(),
            "valid" => {}
            _ => unreachable!(),
        }
        let result = CompilerInputs::read(fixture.0.path());
        if change != "valid" && change != "renamed" {
            assert!(result.is_err(), "{change}");
            continue;
        }
        let inputs = result.unwrap();
        for name in [
            "adapters",
            "aliases",
            "block_results",
            "branch_join",
            "exported",
            "inside_branch",
            "loops",
            "overwritten",
            "private",
            "qualified",
            "reassigned",
            "uninitialized",
            "unrelated",
        ] {
            assert_eq!(
                inputs.owner(&fixture.input(name)),
                Some("crates/feature/vmux_dx/lints/src/registrations.rs||impl:Fixture"),
                "{name}"
            );
        }
    }
}

struct CompilerInputFixture(tempfile::TempDir);

impl CompilerInputFixture {
    fn new() -> Self {
        let workspace = Workspace::current().0;
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("crates/feature/vmux_dx/lints");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("ui")).unwrap();
        let original = workspace.join("crates/feature/vmux_dx/lints");
        std::fs::copy(
            original.join("src/registrations.rs"),
            root.join("src/registrations.rs"),
        )
        .unwrap();
        for entry in std::fs::read_dir(original.join("ui")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), root.join("ui").join(entry.file_name())).unwrap();
        }
        let policy =
            Policy::read(&workspace.join("crates/feature/vmux_dx/ecs-policy.ron")).unwrap();
        let policy = Policy {
            boundaries: policy
                .boundaries
                .into_iter()
                .filter(|record| {
                    record.interface == "Rust compiler input data"
                        || record.key
                            == "crates/feature/vmux_dx/lints/src/registrations.rs||impl:Fixture"
                })
                .collect(),
            ..Default::default()
        };
        std::fs::write(
            root.parent().unwrap().join("ecs-policy.ron"),
            ron::to_string(&policy).unwrap(),
        )
        .unwrap();
        Self(directory)
    }

    fn input(&self, name: &str) -> PathBuf {
        self.0
            .path()
            .join("crates/feature/vmux_dx/lints/ui")
            .join(format!("{name}.rs"))
    }

    fn refresh_owner(&self) {
        let root = self.0.path().join("crates/feature/vmux_dx");
        let source = std::fs::read_to_string(root.join("lints/src/registrations.rs")).unwrap();
        let file = syn::parse_file(&source).unwrap();
        let implementation = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Impl(value) => Some(value),
                _ => None,
            })
            .unwrap();
        let mut policy = Policy::read(&root.join("ecs-policy.ron")).unwrap();
        let owner = policy
            .boundaries
            .iter_mut()
            .find(|record| record.key.ends_with("||impl:Fixture"))
            .unwrap();
        owner.fingerprint = Declaration::fingerprint(&implementation.to_token_stream().to_string());
        std::fs::write(
            root.join("ecs-policy.ron"),
            ron::to_string(&policy).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn renderer_helpers_require_a_current_typed_production_parent() {
    for (attributes, body, expected) in [
        ("#[component]", "rsx! { Child { text: view.text } }", true),
        (
            "#[component] #[cfg(test)]",
            "rsx! { Child { text: view.text } }",
            false,
        ),
        ("#[component]", "rsx! { div { {view.text} } }", false),
        (
            "#[component]",
            "let Child = view.text; rsx! { div { Child } }",
            false,
        ),
        ("#[test]", "let _ = (view, Child);", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Message)] struct View {{ text:String }} {attributes} fn Parent(view:View)->Element {{ {body} }} #[component] fn Child(text:String)->Element {{ rsx! {{ div {{ text }} }} }} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ app.add_message::<View>(); }} }}"
        ));
        let input = format!("ECS: {}", inventory.key("struct:View"));
        let mut parent = Boundary::fixture(&inventory, "fn:Parent");
        parent.input = input.clone();
        parent.output = "external: Dioxus DOM".into();
        let mut child = Boundary::fixture(&inventory, "fn:Child");
        child.input = input;
        child.output = "external: Dioxus DOM".into();
        child.owner = Some(parent.key.clone());
        let policy = Policy {
            boundaries: vec![parent, child.clone()],
            ..Default::default()
        };
        assert_eq!(
            child.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{body}"
        );
    }
}

#[test]
fn renderer_aliases_keep_the_identity_of_the_mounted_child() {
    for (binding, expected) in [
        ("use self::actual::Child as Card;", true),
        ("use self::unrelated::Child as Card;", false),
        (
            "#[cfg(feature=\"one\")] use self::actual::Child as Card; #[cfg(feature=\"two\")] use self::unrelated::Child as Card;",
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/sample/src/lib.rs", &format!("mod actual; mod unrelated; {binding} #[derive(Message)] struct View {{ text:String }} #[component] fn Parent(view:View)->Element {{ rsx! {{ Card {{ text:view.text }} }} }} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ app.add_message::<View>(); }} }}"));
        for name in ["actual", "unrelated"] {
            inventory.source(
                &format!("crates/sample/src/{name}.rs"),
                "#[component] pub fn Child(text:String)->Element { rsx! { div { text } } }",
            );
        }
        inventory.resolve_usage();
        let mut parent = Boundary::fixture(&inventory, "fn:Parent");
        parent.input = format!("ECS: {}", inventory.key("struct:View"));
        parent.output = "external: Dioxus DOM".into();
        let key = "crates/sample/src/actual.rs||fn:Child".to_string();
        let mut child = parent.clone();
        child.key = key.clone();
        child.fingerprint = inventory.entries[&key].fingerprint.clone();
        child.owner = Some(parent.key.clone());
        let policy = Policy {
            boundaries: vec![parent, child.clone()],
            ..Default::default()
        };
        assert_eq!(
            child.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{binding}"
        );
    }
}

#[test]
fn renderer_boundaries_follow_rust_expressions_instead_of_literal_text() {
    for (body, expected) in [
        ("button { onclick:move |_| emit(Intent { value:1 }) }", true),
        ("Child { value:Intent { value:1 } }", true),
        ("div { { read::<Intent>() } }", true),
        ("if check::<Intent>() { span {} }", true),
        ("for item in read::<Intent>() { span {} }", true),
        ("div { ..attrs::<Intent>() }", true),
        ("div { title:\"{label::<Intent>()}\" }", true),
        ("div { \"Intent\" }", false),
        ("div { \"{{Intent}}\" }", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Message)] struct Intent {{ value:u64 }} #[component] fn Render()->Element {{ rsx! {{ {body} }} }} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ app.add_message::<Intent>(); }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "fn:Render");
        boundary.input = format!("ECS: {}", inventory.key("struct:Intent"));
        boundary.output = "external: mounted DOM".into();
        assert_eq!(boundary.references(&inventory), expected, "{body}");
    }
}

#[test]
fn adapter_value_endpoints_require_an_actual_used_canonical_owner() {
    let inventory = Inventory::fixture(
        "enum State { Active } #[derive(Message)] struct Observed { value:State } #[derive(Message)] struct Unused { value:State } struct Codec; impl Codec { fn decode()->State { State::Active } } struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_message::<Observed>(); } }",
    );
    let key = inventory.key("enum:State");
    let mut boundary = Boundary::fixture(&inventory, "impl:Codec");
    boundary.input = "external: provider bytes".into();
    boundary.output = format!("ECS: {key}");
    let mut policy = Policy::default();
    assert!(!boundary.references_from(&inventory, &policy, &mut BTreeSet::new()));
    policy.values.push(ValueOwner {
        key,
        owner: inventory.key("struct:Observed"),
        lifetime: "one typed ingress observation".into(),
    });
    assert!(boundary.references_from(&inventory, &policy, &mut BTreeSet::new()));
    policy.values[0].owner = inventory.key("struct:Unused");
    assert!(!boundary.references_from(&inventory, &policy, &mut BTreeSet::new()));
}

#[test]
fn adapter_endpoints_follow_nested_values_to_a_used_canonical_owner() {
    for (registration, expected) in [("app.add_systems(Update, read);", true), ("", false)] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Component)] struct Root {{ source:Source }} struct Source {{ file:File }} struct File; struct Codec; impl Codec {{ fn decode()->File {{ File }} }} fn read(roots:Query<&Root>) {{}} struct Plugin; impl BevyPlugin for Plugin {{ fn build(&self,app:&mut App) {{ {registration} }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "impl:Codec");
        boundary.input = "external: native file bytes".into();
        boundary.output = format!("ECS: {}", inventory.key("struct:File"));
        let policy = Policy {
            values: vec![
                ValueOwner {
                    key: inventory.key("struct:Source"),
                    owner: inventory.key("struct:Root"),
                    lifetime: "root-owned native source".into(),
                },
                ValueOwner {
                    key: inventory.key("struct:File"),
                    owner: inventory.key("struct:Source"),
                    lifetime: "source-owned native file".into(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            boundary.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected
        );
    }
}

#[test]
fn pre_world_dom_fixtures_require_a_test_only_framework_component_edge() {
    for (attributes, body, expected) in [
        ("#[cfg(test)] #[component]", "rsx! { Child {} }", true),
        ("#[component]", "rsx! { Child {} }", false),
        ("#[cfg(test)]", "let _ = Child;", false),
        ("#[cfg(test)] #[component]", "rsx! { div {} }", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/feature/example/src/lib.rs", &format!("{attributes} fn Fixture()->Element {{ {body} }} #[component] fn Child()->Element {{ rsx! {{ div {{}} }} }}"));
        inventory.resolve_usage();
        let mut boundary = Boundary::fixture(&inventory, "fn:Fixture");
        boundary.interface = "Dioxus VirtualDom fixture".into();
        assert_eq!(boundary.references(&inventory), expected);
    }
}

#[test]
fn current_crate_contracts_preserve_the_declaring_crate_identity() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/vmux_api/src/address.rs", "#[crate::contract] pub struct Address { url:String } #[crate::ui_state] pub struct Snapshot { value:String }");
    inventory.source(
        "crates/feature/consumer/src/lib.rs",
        "#[crate::contract] struct Forged { value:String }",
    );
    inventory.resolve_usage();
    for (name, expected) in [
        ("struct:Address", true),
        ("struct:Snapshot", true),
        ("struct:Forged", false),
    ] {
        let key = inventory.key(name);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn plugin_boundaries_use_only_their_own_production_system_parameters() {
    for (registration, expected) in [
        ("app.add_systems(Update, read);", true),
        ("#[cfg(test)] { app.add_systems(Update, read); }", false),
        ("", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_ecs::prelude::*; use bevy_app::{{App, Plugin}}; #[derive(Component)] struct State; fn read(state:Query<&State>) {{}} struct Other; impl Plugin for Other {{ fn build(&self, app:&mut App) {{ app.add_systems(Update, read); }} }} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ {registration} }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "struct:Root");
        boundary.input = "external: Bevy App registration".into();
        boundary.output = format!("ECS: {}", inventory.key("struct:State"));
        assert_eq!(boundary.references(&inventory), expected, "{registration}");
    }
}

#[test]
fn scheduled_sets_require_a_registered_schedule_edge() {
    for (registration, expected) in [
        ("app.add_systems(Update, read.in_set(Phase::Input));", true),
        (
            "#[cfg(test)] { app.add_systems(Update, read.in_set(Phase::Input)); }",
            false,
        ),
        ("", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(SystemSet)] enum Phase {{ Input }} fn read() {{}} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ {registration} }} }}"
        ));
        let key = inventory.key("enum:Phase");
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected,
            "{registration}"
        );
    }
}

#[test]
fn renderer_owner_chains_reach_the_original_typed_parent() {
    for (rendered, expected) in [("rsx! { Leaf {} }", true), ("rsx! { div {} }", false)] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Message)] struct State {{ value:String }} #[component] fn Parent(state:State)->Element {{ rsx! {{ Middle {{}} }} }} #[component] fn Middle()->Element {{ {rendered} }} #[component] fn Leaf()->Element {{ rsx! {{ div {{}} }} }} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ app.add_message::<State>(); }} }}"
        ));
        let mut parent = Boundary::fixture(&inventory, "fn:Parent");
        parent.input = format!("ECS: {}", inventory.key("struct:State"));
        parent.output = "external: Dioxus DOM".into();
        let mut middle = parent.clone();
        middle.key = inventory.key("fn:Middle");
        middle.fingerprint = inventory.entries[&middle.key].fingerprint.clone();
        middle.owner = Some(parent.key.clone());
        let mut leaf = parent.clone();
        leaf.key = inventory.key("fn:Leaf");
        leaf.fingerprint = inventory.entries[&leaf.key].fingerprint.clone();
        leaf.owner = Some(middle.key.clone());
        let mut policy = Policy {
            boundaries: vec![parent, middle, leaf.clone()],
            ..Default::default()
        };
        assert_eq!(
            leaf.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{rendered}"
        );
        policy.boundaries[1].fingerprint = "stale".into();
        assert!(!leaf.references_from(&inventory, &policy, &mut BTreeSet::new()));
    }
}

#[test]
fn io_adapters_can_name_an_actual_canonical_component_owner() {
    for (registration, expected) in [("app.add_systems(Update, read);", true), ("", false)] {
        let inventory = Inventory::fixture(&format!(
            "struct Reader {{ bytes:Vec<u8> }} #[derive(Component)] struct State {{ reader:Reader }} fn read(state:Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ {registration} }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "struct:Reader");
        boundary.owner = Some(inventory.key("struct:State"));
        boundary.input = format!("ECS: {}", inventory.key("struct:State"));
        boundary.output = "external: bounded file bytes".into();
        let policy = Policy {
            boundaries: vec![boundary.clone()],
            ..Default::default()
        };
        assert_eq!(
            boundary.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{registration}"
        );
    }
}

#[test]
fn local_policy_fragments_preserve_global_adapter_checks_and_duplicate_refusal() {
    let repository = Repository::new();
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/util/alpha/src/lib.rs",
        "#[vmux_api::ui_event] pub struct State { pub bytes: Vec<u8> }",
    );
    inventory.source(
        "crates/util/beta/src/lib.rs",
        "use alpha::State; pub struct Codec { bytes: Vec<u8> } impl Codec { pub fn decode(self) -> State { State { bytes: self.bytes } } }",
    );
    inventory.resolve_usage();
    let mut boundaries = Vec::new();
    for key in [
        "crates/util/beta/src/lib.rs||struct:Codec",
        "crates/util/beta/src/lib.rs||impl:Codec",
    ] {
        let mut boundary = Boundary::fixture(&inventory, key);
        boundary.interface = "network: bounded wire codec".into();
        boundary.input = "external: framed bytes".into();
        boundary.output = "ECS: crates/util/alpha/src/lib.rs||struct:State".into();
        boundaries.push(boundary);
    }
    let fragment = Policy {
        boundaries,
        ..Default::default()
    };
    repository.write(
        "crates/util/beta/ecs-policy.ron",
        &ron::to_string(&fragment).unwrap(),
    );
    let policy = Policy::load(repository.0.path()).unwrap();
    let errors = policy.audit(&inventory);
    assert!(errors.is_empty(), "{errors:?}");
    let mut duplicate = fragment;
    duplicate.boundaries.push(duplicate.boundaries[0].clone());
    repository.write(
        "crates/util/beta/ecs-policy.ron",
        &ron::to_string(&duplicate).unwrap(),
    );
    let policy = Policy::load(repository.0.path()).unwrap();
    assert!(
        policy
            .audit(&inventory)
            .iter()
            .any(|error| error.contains("duplicate policy entry"))
    );
    duplicate.boundaries.pop();
    duplicate.boundaries[0].fingerprint = "stale".into();
    repository.write(
        "crates/util/beta/ecs-policy.ron",
        &ron::to_string(&duplicate).unwrap(),
    );
    let policy = Policy::load(repository.0.path()).unwrap();
    assert!(
        policy
            .audit(&inventory)
            .iter()
            .any(|error| error.contains("stale/incomplete adapter"))
    );
}

#[test]
fn policy_fragments_reject_foreign_records_provenance_and_malformed_files() {
    for (path, source, expected) in [
        (
            "crates/util/alpha/ecs-policy.ron",
            "(revision: \"changed\")",
            "Global provenance",
        ),
        (
            "crates/util/alpha/ecs-policy.ron",
            "(unknown: [])",
            "Unexpected field",
        ),
        (
            "crates/util/alpha/src/ecs-policy.ron",
            "()",
            "Misplaced policy fragment",
        ),
        (
            "website/ecs-policy.ron",
            "(legacy: [(key: \"scripts/fixture.sh||script\", fingerprint: \"fixed\", issue: \"VMX-253\")])",
            "Foreign policy record",
        ),
        (
            "crates/ecs-policy.ron",
            "(values: [(key: \"crates/util/alpha/src/lib.rs||struct:Value\", owner: \"crates/util/alpha/src/lib.rs||struct:State\", lifetime: \"one operation\")])",
            "Foreign policy record",
        ),
    ] {
        let repository = Repository::new();
        repository.write(path, source);
        let error = Policy::load(repository.0.path())
            .err()
            .expect("invalid fragment must fail");
        assert!(error.contains(expected), "{path}: {error}");
    }
    let repository = Repository::new();
    repository.write("ecs-policy.ron", "(revision: \"fixed\", legacy: [(key: \"fixture.rs||struct:Value\", fingerprint: \"fixed\", issue: \"VMX-253\")])");
    assert!(
        Policy::load(repository.0.path())
            .err()
            .unwrap()
            .contains("Root policy holds only")
    );
}

#[test]
#[cfg(unix)]
fn policy_fragments_refuse_symlinks_instead_of_reading_another_location() {
    let repository = Repository::new();
    repository.write("website/source.ron", "()");
    std::os::unix::fs::symlink(
        "source.ron",
        repository.0.path().join("website/ecs-policy.ron"),
    )
    .unwrap();
    let error = Policy::load(repository.0.path())
        .err()
        .expect("symlink fragment must fail");
    assert!(error.contains("regular file"), "{error}");
}

#[test]
fn shape_fragments_compare_debt_with_the_fetched_tree_instead_of_local_files() {
    let repository = Repository::new();
    repository.write(
        "ecs-policy.ron",
        &format!(
            "(revision: \"{FROZEN_REVISION}\", shape: (revision: \"{FROZEN_SHAPE_REVISION}\"))"
        ),
    );
    let path = "crates/util/alpha/src/lib.rs";
    let mut inventory = Inventory::default();
    inventory.source(path, "enum A { One, Two } enum B { One, Two }");
    let a = inventory.key("enum:A");
    let b = inventory.key("enum:B");
    let mut fragment = Policy {
        shape: ShapePolicy {
            debt: vec![ShapeDebt {
                key: a.clone(),
                fingerprint: inventory.entries[&a].fingerprint.clone(),
                issue: "VMX-253".into(),
                finding: "AU-13".into(),
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    repository.write(
        "crates/util/alpha/ecs-policy.ron",
        &ron::to_string(&fragment).unwrap(),
    );
    repository.snapshot();
    let unchanged = Policy::load(repository.0.path()).unwrap();
    assert!(
        unchanged
            .shape
            .monotonic_from(&Policy::fetched(repository.0.path()).shape)
    );
    fragment.shape.debt.push(ShapeDebt {
        key: b.clone(),
        fingerprint: inventory.entries[&b].fingerprint.clone(),
        issue: "VMX-253".into(),
        finding: "AU-13".into(),
    });
    repository.write(
        "crates/util/alpha/ecs-policy.ron",
        &ron::to_string(&fragment).unwrap(),
    );
    let restored = Policy::load(repository.0.path()).unwrap();
    let panic = std::panic::catch_unwind(|| restored.shape.prove(repository.0.path()))
        .expect_err("restored debt must be refused before historical lookup");
    let message = if let Some(message) = panic.downcast_ref::<String>() {
        message.as_str()
    } else {
        panic.downcast_ref::<&str>().unwrap()
    };
    assert!(message.contains("retired shape debt"), "{message}");
    fragment.shape.debt.pop();
    fragment.shape.debt.push(ShapeDebt {
        key: a.clone(),
        fingerprint: inventory.entries[&a].fingerprint.clone(),
        issue: "VMX-253".into(),
        finding: "AU-13".into(),
    });
    repository.write(
        "crates/util/alpha/ecs-policy.ron",
        &ron::to_string(&fragment).unwrap(),
    );
    let duplicate = Policy::load(repository.0.path()).unwrap();
    assert!(
        duplicate
            .shape
            .audit(&inventory, &Policy::default())
            .iter()
            .any(|error| error.contains("stale/invalid shape debt"))
    );
}

#[test]
fn shape_fragments_refuse_foreign_records_and_fragment_provenance() {
    let debt = "debt: [(key: \"crates/util/alpha/src/lib.rs||enum:State\", fingerprint: \"fixed\", issue: \"VMX-253\", finding: \"AU-13\")]";
    let purpose = "enums: [(key: \"crates/util/alpha/src/lib.rs||enum:State\", fingerprint: \"fixed\", role: Scalar, owner: \"crates/util/alpha/src/lib.rs||struct:Owner\", reason: \"canonical scalar\")]";
    let body = "bodies: [(key: \"crates/util/alpha/src/lib.rs||impl:Reader::read\", fingerprint: \"fixed\", boundary: \"crates/util/alpha/src/lib.rs||impl:Reader\", reason: \"bounded input\")]";
    for records in [debt, purpose, body] {
        let repository = Repository::new();
        repository.write("website/ecs-policy.ron", &format!("(shape: ({records}))"));
        let error = Policy::load(repository.0.path()).err().unwrap();
        assert!(error.contains("Foreign policy record"), "{error}");
        repository.snapshot();
        let error = Policy::from_git(repository.0.path(), "origin/main")
            .err()
            .unwrap();
        assert!(error.contains("Foreign policy record"), "{error}");
        std::fs::remove_file(repository.0.path().join("website/ecs-policy.ron")).unwrap();
        repository.write("ecs-policy.ron", &format!("(shape: ({records}))"));
        let error = Policy::load(repository.0.path()).err().unwrap();
        assert!(error.contains("Root policy holds only"), "{error}");
    }
    let repository = Repository::new();
    repository.write("website/ecs-policy.ron", "(shape: (revision: \"changed\"))");
    let error = Policy::load(repository.0.path()).err().unwrap();
    assert!(error.contains("Global provenance"), "{error}");
    repository.snapshot();
    let error = Policy::from_git(repository.0.path(), "origin/main")
        .err()
        .unwrap();
    assert!(error.contains("Global provenance"), "{error}");
}

#[test]
#[cfg(unix)]
fn fetched_policy_refuses_symlink_fragments_in_its_own_tree() {
    let repository = Repository::new();
    repository.write("website/source.ron", "()");
    std::os::unix::fs::symlink(
        "source.ron",
        repository.0.path().join("website/ecs-policy.ron"),
    )
    .unwrap();
    repository.snapshot();
    let error = Policy::from_git(repository.0.path(), "origin/main")
        .err()
        .expect("Git tree symlinks must not be decoded as policy");
    assert!(error.contains("regular file"), "{error}");
}

#[test]
fn fetched_policy_refuses_the_same_misplaced_fragments_as_the_filesystem() {
    let repository = Repository::new();
    repository.write("crates/misc/ecs-policy.ron", "()");
    let error = Policy::load(repository.0.path()).err().unwrap();
    assert!(error.contains("Misplaced policy fragment"), "{error}");
    repository.snapshot();
    let error = Policy::from_git(repository.0.path(), "origin/main")
        .err()
        .unwrap();
    assert!(error.contains("Misplaced policy fragment"), "{error}");
}

#[test]
fn new_declarations_have_canonical_ownership_or_an_explicit_boundary() {
    let root = Workspace::current();
    let policy = Policy::load(&root.0).expect("valid ownership policy");
    let architecture = std::fs::read_to_string(root.0.join("docs/dx/architecture.md"))
        .expect("architecture boundary contract");
    assert!(
        architecture
            .lines()
            .any(|line| line == "### ECS ownership boundaries"),
        "document the policy's adapter boundary"
    );
    let inventory = Inventory::workspace(&root.0);
    let errors = policy.audit(&inventory);
    assert!(
        errors.is_empty(),
        "ECS ownership violations:\n{}",
        errors.join("\n")
    );
    policy.prove_legacy(&root.0);
    policy.shape.prove(&root.0);
}

#[test]
fn reviewed_app_extensions_reference_only_their_actual_framework_composition() {
    for (binding, owner, installed, expected) in [
        ("use bevy_app::App;", "App", "Root", true),
        ("use bevy_app::App as Host;", "Host", "Root", true),
        ("struct App;", "App", "Root", false),
        ("use native::App;", "App", "Root", false),
        ("use bevy_app::App;", "App", "Other", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::Plugin; use bevy_ecs::prelude::*; {binding} #[derive(Component)] struct State; fn read(state: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut bevy_app::App) {{ app.add_systems(Update, read); }} }} struct Other; impl Plugin for Other {{ fn build(&self, app: &mut bevy_app::App) {{}} }} trait Install {{}} impl Install for {owner} {{ fn install(&mut self) {{ self.add_plugins({installed}); }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, &format!("impl:Install for {owner}"));
        boundary.interface = "Bevy App extension plugin installation".into();
        boundary.input = "external: Bevy App composition".into();
        boundary.output = format!("ECS: {}", inventory.key("struct:State"));
        assert_eq!(
            boundary.references(&inventory),
            expected,
            "{binding} {installed}"
        );
    }
}

#[test]
fn component_aliases_require_used_canonical_components_and_preserve_field_ownership() {
    for (declaration, registered, expected) in [
        ("#[derive(Component)] struct Owner;", true, true),
        ("#[derive(Component)] struct Owner;", false, false),
        ("struct Owner;", true, false),
        (
            "struct Business { entries: Vec<String> } #[derive(Component)] struct Owner { business: Business }",
            true,
            false,
        ),
    ] {
        let registration = if registered {
            "app.add_systems(Update, read);"
        } else {
            ""
        };
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App, Plugin, Update}}; use bevy_ecs::prelude::*; {declaration} type Alias = Owner; fn read(input: Query<&Alias>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ {registration} }} }}"
        ));
        let key = inventory.key("alias:Alias");
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected,
            "{declaration}; registered={registered}"
        );
    }
}

#[test]
fn component_aliases_do_not_approve_wrappers_shadowed_names_or_cycles() {
    for declaration in [
        "struct Wrapper<T>(T); #[derive(Component)] struct Owner; type Alias = Wrapper<Owner>;",
        "mod native { #[derive(Component)] pub struct Owner; } mod shadow { pub struct Owner; } use shadow::Owner; type Alias = Owner;",
        "type Alias = Other; type Other = Alias;",
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App, Plugin, Update}}; use bevy_ecs::prelude::*; {declaration} fn read(input: Query<&Alias>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));
        let key = inventory.key("alias:Alias");
        assert!(
            !inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            "{declaration}"
        );
    }
}

#[test]
fn stateless_plugin_defaults_require_real_markers_and_pure_standard_construction() {
    for (fields, construction, bindings, expected) in [
        (
            "(PhantomData<fn()->T>)",
            "Self(PhantomData)",
            "use std::marker::PhantomData;",
            true,
        ),
        (
            "{ marker: PhantomData<T> }",
            "Self { marker: PhantomData }",
            "use std::marker::PhantomData;",
            true,
        ),
        (
            "(Marker<T>)",
            "Self(Marker)",
            "use std::marker::PhantomData as Marker;",
            true,
        ),
        (
            "(Scope<T>)",
            "Self(PhantomData)",
            "use std::marker::PhantomData; type Scope<T> = PhantomData<fn()->T>;",
            true,
        ),
        (
            "(Scope<T>)",
            "Self(PhantomData)",
            "use std::marker::PhantomData; type Scope<T> = std::sync::Mutex<T>;",
            false,
        ),
        (
            "(Scope<T>)",
            "Self(PhantomData)",
            "use std::marker::PhantomData; type Scope<T> = Other<T>; type Other<T> = Scope<T>;",
            false,
        ),
        (
            "(PhantomData<T>)",
            "Self(PhantomData)",
            "struct PhantomData<T>(T);",
            false,
        ),
        (
            "(PhantomData<T>)",
            "work(); Self(PhantomData)",
            "use std::marker::PhantomData;",
            false,
        ),
        (
            "(PhantomData<T>)",
            "Self(make())",
            "use std::marker::PhantomData;",
            false,
        ),
        (
            "(PhantomData<T>,u64)",
            "Self(PhantomData,0)",
            "use std::marker::PhantomData;",
            false,
        ),
        (
            "(PhantomData<T>)",
            "Self(PhantomData)",
            "use std::marker::PhantomData; trait Default { fn default()->Self; }",
            false,
        ),
    ] {
        let terminator = if fields.starts_with('{') { "" } else { ";" };
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; {bindings} #[derive(Component)] struct State; struct BridgePlugin<T>{fields}{terminator} impl<T> Default for BridgePlugin<T> {{ fn default()->Self {{ {construction} }} }} impl<T> Plugin for BridgePlugin<T> {{ fn build(&self, app:&mut App) {{ app.add_systems(Update,read); }} }} fn read(state:Query<&State>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_plugins(BridgePlugin::<State>::default()); }} }}"
        ));
        for key in ["impl:Default for BridgePlugin < T >", "struct:RootPlugin"] {
            assert_eq!(
                inventory.automatic_boundary(&inventory.entries[&inventory.key(key)]),
                expected,
                "{key}: {bindings} {construction}"
            );
        }
    }
}

#[test]
fn stateless_plugin_defaults_preserve_registration_and_cycle_refusal() {
    for (build, expected) in [
        ("app.add_systems(Update,read);", true),
        ("", false),
        ("app.add_systems(Update,read); work();", false),
        (
            "app.add_plugins(BridgePlugin::<T>::default()).add_systems(Update,read);",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use std::marker::PhantomData; #[derive(Component)] struct State; struct BridgePlugin<T>(PhantomData<T>); impl<T> Default for BridgePlugin<T> {{ fn default()->Self {{ Self(PhantomData) }} }} impl<T> Plugin for BridgePlugin<T> {{ fn build(&self,app:&mut App) {{ {build} }} }} fn read(state:Query<&State>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_plugins(BridgePlugin::<State>::default()); }} }}"
        ));
        assert_eq!(
            Policy::default().audit(&inventory).is_empty(),
            expected,
            "{build}"
        );
    }
}

#[test]
fn stateless_framework_bootstraps_require_real_interfaces_and_registration() {
    for (binding, fields, body, expected) in [
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "app.add_systems(Update, read);",
            true,
        ),
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "if !app.is_plugin_added::<OtherPlugin>() { app.add_systems(Update, read); }",
            true,
        ),
        (
            "use bevy_app::{App, Plugin}; use bevy_cef::prelude::UiEventPlugin;",
            ";",
            "app.add_plugins(UiEventPlugin::default()).add_systems(Update, read);",
            true,
        ),
        (
            "use bevy_app::{App, Plugin}; struct UiEventPlugin;",
            ";",
            "app.add_plugins(UiEventPlugin::default()).add_systems(Update, read);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "app.insert_resource(State).add_systems(Update, read);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "{ state: State }",
            "app.add_systems(Update, read);",
            false,
        ),
        (
            "trait Plugin {} struct App;",
            ";",
            "app.add_systems(Update, read);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "app.world_mut().insert_resource(State);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "loop { app.add_systems(Update, read); }",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "app.add_systems(Update, || read());",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            ";",
            "app.add_plugins(hidden_workflow()).add_systems(Update, read);",
            false,
        ),
        ("use bevy_app::{App, Plugin};", ";", "", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "{binding} use bevy_ecs::prelude::*; #[derive(Component)] struct State; fn read(state: Query<&State>) {{}} struct OtherPlugin; struct RootPlugin {fields} impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ {body} }} }}"
        ));
        for key in ["struct:RootPlugin", "impl:Plugin for RootPlugin"] {
            let key = inventory.key(key);
            assert_eq!(
                inventory.canonical(
                    &inventory.entries[&key],
                    &Policy::default(),
                    &mut BTreeSet::new()
                ),
                expected,
                "{binding} {fields} {body} {key}"
            );
        }
    }
}

#[test]
fn stateless_framework_bootstraps_do_not_approve_other_implementations_or_test_only_builds() {
    let inventory = Inventory::fixture(
        "use bevy_app::{App, Plugin}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; fn read(state: Query<&State>) {} struct RootPlugin; impl Plugin for RootPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } impl RootPlugin { fn workflow(&self) { loop {} } }",
    );
    let key = inventory.key("impl:RootPlugin");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let inventory = Inventory::fixture(
        "use bevy_app::{App, Plugin}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; fn read(state: Query<&State>) {} struct RootPlugin; #[cfg(test)] impl Plugin for RootPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    let key = inventory.key("struct:RootPlugin");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn child_only_framework_composition_requires_actual_interfaces_and_bounded_installation() {
    for (binding, child, body, expected) in [
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.add_plugins(ChildPlugin);",
            true,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "if !app.is_plugin_added::<ChildPlugin>() { app.add_plugins(ChildPlugin); }",
            true,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "",
            "app.add_plugins(ChildPlugin);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "#[cfg(test)] impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.add_plugins(ChildPlugin);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "struct FakeApp; impl Plugin for ChildPlugin { fn build(&self, app: &mut FakeApp) { app.add_systems(Update, read); } }",
            "app.add_plugins(ChildPlugin);",
            false,
        ),
        (
            "use bevy_app::Plugin; struct App;",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.add_plugins(ChildPlugin);",
            false,
        ),
        (
            "use bevy_app::App; trait Plugin {}",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.add_plugins(ChildPlugin);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.add_plugins(ChildPlugin::default());",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.add_plugins(factory());",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "loop { app.add_plugins(ChildPlugin); }",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "app.world_mut(); app.add_plugins(ChildPlugin);",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "if false { app.add_plugins(ChildPlugin); }",
            false,
        ),
        (
            "use bevy_app::{App, Plugin};",
            "impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            "",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(&format!(
            "{binding} use bevy_ecs::prelude::*; #[derive(Component)] struct State; fn read(state: Query<&State>) {{}} struct ChildPlugin; {child} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ {body} }} }} impl RootPlugin {{ fn workflow(&self) {{ loop {{}} }} }}"
        ));
        for key in ["struct:RootPlugin", "impl:Plugin for RootPlugin"] {
            assert_eq!(
                inventory.canonical(
                    &inventory.entries[&inventory.key(key)],
                    &Policy::default(),
                    &mut BTreeSet::new()
                ),
                expected,
                "{binding}; {child}; {body}; {key}"
            );
        }
        assert!(!inventory.canonical(
            &inventory.entries[&inventory.key("impl:RootPlugin")],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn child_only_composition_preserves_registered_endpoints_and_child_workflow_audits() {
    let inventory = Inventory::fixture(
        "use bevy_app::{App, Plugin}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; #[derive(Component)] struct Unrelated; fn read(state: Query<&State>) {} struct LeafPlugin; impl Plugin for LeafPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); app.world_mut(); } } impl LeafPlugin { fn workflow(&self) { loop {} } } struct ChildPlugin; impl Plugin for ChildPlugin { fn build(&self, app: &mut App) { app.add_plugins(LeafPlugin); } } struct RootPlugin; impl Plugin for RootPlugin { fn build(&self, app: &mut App) { app.add_plugins(ChildPlugin); } }",
    );
    let root = &inventory.entries[&inventory.key("struct:RootPlugin")];
    assert!(inventory.canonical(root, &Policy::default(), &mut BTreeSet::new()));
    for (name, expected) in [("State", true), ("Unrelated", false)] {
        assert_eq!(
            inventory.boundary_references(
                root,
                &inventory.entries[&inventory.key(&format!("struct:{name}"))],
                &mut BTreeSet::new()
            ),
            expected
        );
    }
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("impl:LeafPlugin")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("impl:Plugin for LeafPlugin")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn child_only_composition_refuses_cycles_without_rejecting_shared_registered_children() {
    for (composition, expected) in [
        (
            "struct FirstPlugin; impl Plugin for FirstPlugin { fn build(&self, app: &mut App) { app.add_plugins(SecondPlugin); } } struct SecondPlugin; impl Plugin for SecondPlugin { fn build(&self, app: &mut App) { app.add_plugins(FirstPlugin); } }",
            false,
        ),
        (
            "struct FirstPlugin; impl Plugin for FirstPlugin { fn build(&self, app: &mut App) { app.add_plugins(LeafPlugin); } } struct SecondPlugin; impl Plugin for SecondPlugin { fn build(&self, app: &mut App) { app.add_plugins(LeafPlugin); } }",
            true,
        ),
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App, Plugin}}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; fn read(state: Query<&State>) {{}} struct LeafPlugin; impl Plugin for LeafPlugin {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }} {composition} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ app.add_plugins((FirstPlugin, SecondPlugin)); }} }}"
        ));
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("struct:RootPlugin")],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected
        );
    }
}

#[test]
fn child_only_composition_checks_the_installed_value_without_parent_type_fallback() {
    for (binding, child, expected) in [
        ("", "bevy_cef::prelude::UiEventPlugin", [true, true]),
        (
            "mod bevy_cef { pub mod prelude { pub struct UiEventPlugin; } }",
            "bevy_cef::prelude::UiEventPlugin",
            [false, false],
        ),
        (
            "struct FactoryPlugin; impl Plugin for FactoryPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } impl FactoryPlugin { fn make() -> RootPlugin { RootPlugin } }",
            "FactoryPlugin::make()",
            [false, false],
        ),
        (
            "struct FactoryPlugin; impl Plugin for FactoryPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } impl Default for FactoryPlugin { fn default() -> Self { Self } }",
            "FactoryPlugin::default()",
            [true, true],
        ),
        (
            "struct FactoryPlugin; impl Plugin for FactoryPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } impl Default for FactoryPlugin { fn default() -> Self { work(); Self } }",
            "FactoryPlugin::default()",
            [false, false],
        ),
        (
            "struct FactoryPlugin; impl Plugin for FactoryPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } impl Default for FactoryPlugin { fn default() -> Self { Self } } impl FactoryPlugin { fn default() -> RootPlugin { RootPlugin } }",
            "FactoryPlugin::default()",
            [false, false],
        ),
        (
            "struct FactoryPlugin; impl Plugin for FactoryPlugin { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } impl FactoryPlugin { fn make() -> Self { Self } }",
            "FactoryPlugin::make()",
            [false, false],
        ),
    ] {
        for nested in [false, true] {
            let installed = if nested { "ChildPlugin" } else { child };
            let inventory = Inventory::fixture(&format!(
                "use bevy_app::{{App, Plugin}}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; fn read(state: Query<&State>) {{}} {binding} struct ChildPlugin; impl Plugin for ChildPlugin {{ fn build(&self, app: &mut App) {{ app.add_plugins({child}); }} }} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ app.add_plugins({installed}); }} }}"
            ));
            for key in ["struct:RootPlugin", "impl:Plugin for RootPlugin"] {
                assert_eq!(
                    inventory.canonical(
                        &inventory.entries[&inventory.key(key)],
                        &Policy::default(),
                        &mut BTreeSet::new()
                    ),
                    expected[usize::from(nested)],
                    "{binding}; {child}; nested={nested}; {key}"
                );
            }
        }
    }
}

#[test]
fn generic_plugin_implementations_match_concrete_repeated_nested_and_const_patterns() {
    for (patterns, installed, expected) in [
        ("MarkerA,T", "MarkerB,State", false),
        ("MarkerA,T", "MarkerA,State", true),
        ("T,T", "Other,State", false),
        ("T,T", "State,State", true),
        ("Option<T>,Other", "Option<State>,Other", true),
        ("1,T", "2,State", false),
        ("1,T", "1,State", true),
    ] {
        let definition = if patterns.starts_with('1') {
            "struct Bridge<const N:u8,T>(std::marker::PhantomData<T>);"
        } else {
            "struct Bridge<A,B>(std::marker::PhantomData<(A,B)>);"
        };
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App,Plugin}}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; #[derive(Component)] struct Other; struct MarkerA; struct MarkerB; {definition} impl<T> Plugin for Bridge<{patterns}> {{ fn build(&self,app:&mut App) {{ app.add_observer(receive::<T>); }} }} fn receive<T>(event:On<Add,T>) {{}} struct Root; impl Plugin for Root {{ fn build(&self,app:&mut App) {{ app.add_plugins(Bridge::<{installed}>::default()); }} }}"
        ));
        assert_eq!(
            inventory.used(&inventory.entries[&inventory.key("struct:State")]),
            expected,
            "{patterns}: {installed}"
        );
    }
}

#[test]
fn generic_jobs_keep_shared_lifecycle_component_boundaries() {
    for (declaration, parameter, expected) in [
        ("#[derive(Component)] struct Ready;", "Query<&Ready>", true),
        (
            "#[derive(Message)] struct Ready<T>(std::marker::PhantomData<T>);",
            "MessageWriter<Ready<T>>",
            true,
        ),
        (
            "#[derive(Message)] struct Ready<T>(std::marker::PhantomData<T>);",
            "MessageReader<Ready<T>>",
            true,
        ),
        (
            "#[derive(Message)] struct Ready<T>(std::marker::PhantomData<T>);",
            "MessageMutator<Ready<T>>",
            true,
        ),
        (
            "#[derive(Message)] struct Ready<T>(std::marker::PhantomData<T>); struct MessageWriter<T>(T);",
            "MessageWriter<Ready<T>>",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App,Plugin}}; use bevy_ecs::prelude::*; {declaration} struct Job; struct Bridge<T>(std::marker::PhantomData<T>); impl<T> Plugin for Bridge<T> {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive::<T>); }} }} type Inputs<T> = {parameter}; fn receive<T>(inputs:Inputs<T>) {{}} struct Root; impl Plugin for Root {{ fn build(&self,app:&mut App) {{ app.add_plugins(Bridge::<Job>::default()); }} }}"
        ));
        let owner = &inventory.entries[&inventory.key("struct:Root")];
        let ready = &inventory.entries[&inventory.key("struct:Ready")];
        assert_eq!(
            inventory.boundary_references(owner, ready, &mut BTreeSet::new()),
            expected,
            "{parameter}"
        );
        assert!(!inventory.used(&inventory.entries[&inventory.key("struct:Job")]));
    }
}

#[test]
fn job_result_components_follow_registered_receivers_and_actual_associated_outputs() {
    for (parameter, registered, expected) in [
        ("Query<&JobResult<Input>>", true, true),
        ("Inputs<Input>", true, true),
        ("Query<&JobResult<Input>>", false, false),
        ("Query<&OtherResult<Input>>", true, false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        let installation = if registered {
            "app.add_systems(Update, receive);"
        } else {
            ""
        };
        inventory.source("crates/feature/receiver/src/lib.rs", &format!(
            "use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; struct Input; #[derive(Component)] struct Bytes(Vec<u8>); impl Job for Input {{ type Output=Result<Option<Bytes>,String>; }} type Inputs<T> = Query<&'static JobResult<T>>; #[derive(Component)] struct OtherResult<T>(std::marker::PhantomData<T>); fn receive(input:{parameter}) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ {installation} }} }}"
        ));
        inventory.resolve_usage();
        let output = inventory
            .entries
            .values()
            .find(|entry| {
                entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes"
            })
            .unwrap();
        assert_eq!(
            inventory.used(output),
            expected,
            "{parameter}, registered={registered}"
        );
        assert!(
            !inventory.canonical(
                inventory
                    .entries
                    .values()
                    .find(|entry| entry.path == "crates/feature/receiver/src/lib.rs"
                        && entry.name == "Input")
                    .unwrap(),
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            "output ownership cannot approve a plain Job input or its native workflow"
        );
    }
}

#[test]
fn job_output_inference_refuses_shadowed_queries_traits_and_result_envelopes() {
    for (shadow, imports, expected) in [
        (
            "",
            "use bevy_ecs::prelude::Query; use vmux_ecs::jobs::{Job,JobResult};",
            true,
        ),
        (
            "struct Query<T>(T);",
            "use vmux_ecs::jobs::{Job,JobResult};",
            false,
        ),
        (
            "struct JobResult<T>(T);",
            "use bevy_ecs::prelude::Query; use vmux_ecs::jobs::Job;",
            false,
        ),
        (
            "trait Job { type Output; }",
            "use bevy_ecs::prelude::Query; use vmux_ecs::jobs::JobResult;",
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!(
            "use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::Component; {imports} {shadow} struct Input; #[derive(Component)] struct Bytes(Vec<u8>); impl Job for Input {{ type Output=Bytes; }} fn receive(input:Query<&JobResult<Input>>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"
        ));
        inventory.resolve_usage();
        let output = inventory
            .entries
            .values()
            .find(|entry| {
                entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes"
            })
            .unwrap();
        assert_eq!(inventory.used(output), expected, "{shadow}");
    }
}

#[test]
fn job_outputs_require_their_own_bevy_borrow_within_mixed_parameters() {
    for parameter in [
        "Query<(&JobResult<Input>, &PhantomData<JobResult<Other>>)>",
        "ParamSet<(Query<&JobResult<Input>>, Query<&PhantomData<JobResult<Other>>>)>",
        "Mixed<'_, '_>",
        "ParamSet<(Inputs<Input>, Query<&PhantomData<JobResult<Other>>>)>",
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use bevy_ecs::system::SystemParam; use std::marker::PhantomData; use vmux_ecs::jobs::{{Job,JobResult}}; struct Input; struct Other; #[derive(Component)] struct Bytes(Vec<u8>); #[derive(Component)] struct Unborrowed(Vec<u8>); impl Job for Input {{ type Output=Bytes; }} impl Job for Other {{ type Output=Unborrowed; }} type Inputs<T> = Query<&'static JobResult<T>>; #[derive(SystemParam)] struct Mixed<'w,'s> {{ input:Query<'w,'s,&'static JobResult<Input>>, ignored:Query<'w,'s,&'static PhantomData<JobResult<Other>>> }} fn receive(input:{parameter}) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"));
        inventory.resolve_usage();
        for (name, expected) in [("Bytes", true), ("Unborrowed", false)] {
            let output = inventory
                .entries
                .values()
                .find(|entry| {
                    entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == name
                })
                .unwrap();
            assert_eq!(inventory.used(output), expected, "{parameter}: {name}");
        }
    }
}

#[test]
fn job_result_consumers_audit_the_payload_without_approving_the_indexed_input() {
    for (parameter, output, expected) in [
        ("Query<&JobResult<Input>>", "Bytes", true),
        ("Query<&JobResult<Input>>", "Result<Bytes,String>", true),
        ("Query<&JobResult<Input>>", "OutputAlias", true),
        ("Query<&JobResult<Input>>", "Callback", false),
        ("Query<&JobResult<Input>>", "Self", false),
        ("Query<&JobResult<Input>>", "()", true),
        ("Query<&JobResult<Input>>", "Unowned", false),
        ("Query<(&JobResult<Input>,&Input)>", "Bytes", false),
        ("Query<&JobRequest<Input>>", "Bytes", false),
        ("Query<&OtherResult<Input>>", "Bytes", false),
        ("Query<Entity,With<JobResult<Input>>>", "Bytes", false),
        ("Args<Input>", "Bytes", true),
        ("Mixed<Input>", "Bytes", false),
        ("Params<Input>", "Bytes", true),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>); #[derive(Component)] pub struct JobRequest<J:Job>(pub J);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use bevy_ecs::system::SystemParam; use vmux_ecs::jobs::{{Job,JobResult,JobRequest}}; struct Input; struct Unowned; #[derive(Component)] struct Bytes(Vec<u8>); #[derive(Component)] struct OtherResult<T>(T); type OutputAlias=Result<Bytes,String>; type Callback=fn(); type Args<T>=Query<&'static JobResult<T>>; type Mixed<T>=Query<(&'static JobResult<T>,&'static T)>; #[derive(SystemParam)] struct Params<'w,'s,T:Job>{{ values:Query<'w,'s,&'static JobResult<T>> }} impl Job for Input{{ type Output={output}; }} fn receive(input:{parameter}){{}} struct RootPlugin; impl Plugin for RootPlugin{{ fn build(&self,app:&mut App){{app.add_systems(Update,receive);}} }}"));
        inventory.resolve_usage();
        let policy = Policy::default();
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("fn:receive")],
                &policy,
                &mut BTreeSet::new(),
            ),
            expected,
            "{parameter}: {output}"
        );
        for key in ["struct:Input", "impl:Job for Input"] {
            assert!(
                !inventory.canonical(
                    &inventory.entries[&inventory.key(key)],
                    &policy,
                    &mut BTreeSet::new(),
                ),
                "{parameter}: {key}"
            );
        }
    }
}

#[test]
fn indexed_result_evidence_requires_an_output_projection_and_preserves_concrete_borrows() {
    for (fields, parameter, expected) in [
        ("Result<J::Output,String>", "Query<&JobResult<Input>>", true),
        (
            "Result<J::Output,String>,J",
            "Query<&JobResult<Input>>",
            false,
        ),
        ("J", "Query<&JobResult<Input>>", false),
        ("Result<J::Output,J>", "Query<&JobResult<Input>>", false),
        (
            "std::marker::PhantomData<J>",
            "Query<&JobResult<Input>>",
            false,
        ),
        ("Result<J::Output,String>", "Mixed<Input>", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", &format!("use bevy_ecs::prelude::*; pub trait Job {{type Output;}} #[derive(Component)] pub struct JobResult<J:Job>({fields});"));
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; use crate::actual::Input; mod actual{{pub struct Input; impl vmux_ecs::jobs::Job for Input{{type Output=super::Bytes;}}}} #[derive(Component)] struct Bytes(Vec<u8>); type Mixed<T>=Query<(&'static JobResult<T>,&'static crate::actual::Input)>; fn receive(input:{parameter}){{}} struct RootPlugin; impl Plugin for RootPlugin{{fn build(&self,app:&mut App){{app.add_systems(Update,receive);}}}}"));
        inventory.resolve_usage();
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("fn:receive")],
                &Policy::default(),
                &mut BTreeSet::new(),
            ),
            expected,
            "{fields}: {parameter}"
        );
    }
    let mut inventory = Inventory::default();
    inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job{type Output;} #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
    inventory.source("crates/feature/receiver/src/lib.rs", "use bevy_app::{App,Plugin,Update}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{Job,JobResult}; struct Input<T>(std::marker::PhantomData<T>); struct End; impl<T> Job for Input<T>{type Output=JobResult<Input<Vec<T>>>;} fn receive(input:Query<&JobResult<Input<End>>>){} struct RootPlugin; impl Plugin for RootPlugin{fn build(&self,app:&mut App){app.add_systems(Update,receive);}}");
    inventory.resolve_usage();
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("fn:receive")],
        &Policy::default(),
        &mut BTreeSet::new(),
    ));
}

#[test]
fn indexed_job_outputs_cannot_turn_ui_projections_into_domain_state() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job {type Output;} #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
    inventory.source(
        "crates/util/vmux_ecs/src/ui_state.rs",
        "use bevy_ecs::prelude::*; #[derive(Component)] pub struct UiState{pub text:String}",
    );
    inventory.source("crates/feature/receiver/src/lib.rs", "use bevy_app::{App,Plugin,Update}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{Job,JobResult}; use vmux_ecs::ui_state::UiState; struct Input; impl Job for Input{type Output=UiState;} fn receive(input:Query<&JobResult<Input>>){} fn view(input:Query<&UiState>){} struct RootPlugin; impl Plugin for RootPlugin{fn build(&self,app:&mut App){app.add_systems(Update,(receive,view));}}");
    inventory.resolve_usage();
    let output = &inventory.entries["crates/util/vmux_ecs/src/ui_state.rs||struct:UiState"];
    assert!(inventory.used(output));
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("fn:receive")],
        &Policy::default(),
        &mut BTreeSet::new(),
    ));
}

#[test]
fn indexed_result_inference_does_not_bind_unresolved_parameters_to_shadow_types() {
    for (definition, parameter, registration) in [
        (
            "struct Input; struct T; impl Job for Input{type Output=UiState;} impl Job for T{type Output=Bytes;}",
            "fn receive<T:Job>(input:Query<&JobResult<T>>){}",
            "receive::<Input>",
        ),
        (
            "struct Input<T>(std::marker::PhantomData<T>); struct Read; impl Job for Read{type Output=UiState;} impl<T:Job> Job for Input<T>{type Output=T::Output;}",
            "fn receive(input:Query<&JobResult<Input<Read>>>){}",
            "receive",
        ),
        (
            "struct Input; impl Job for Input{type Output=Box<dyn Fn()>;}",
            "fn receive(input:Query<&JobResult<Input>>){}",
            "receive",
        ),
        (
            "struct Input; impl Job for Input{type Output=fn();}",
            "fn receive(input:Query<&JobResult<Input>>){}",
            "receive",
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job{type Output;} #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source(
            "crates/util/vmux_ecs/src/ui_state.rs",
            "use bevy_ecs::prelude::*; #[derive(Component)] pub struct UiState{pub text:String}",
        );
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; use vmux_ecs::ui_state::UiState; #[derive(Component)] struct Bytes(Vec<u8>); {definition} {parameter} fn bytes(input:Query<&Bytes>){{}} struct RootPlugin; impl Plugin for RootPlugin{{fn build(&self,app:&mut App){{app.add_systems(Update,({registration},bytes));}}}}"));
        inventory.resolve_usage();
        assert!(
            !inventory.canonical(
                &inventory.entries[&inventory.key("fn:receive")],
                &Policy::default(),
                &mut BTreeSet::new(),
            ),
            "{definition}"
        );
    }
}

#[test]
fn canonical_job_inputs_cannot_hide_unowned_output_values() {
    for (output, legacy, expected) in [
        ("Unowned", false, false),
        ("Unowned", true, true),
        ("Self", false, true),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job{type Output;} #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; #[derive(Component)] struct Input; struct Unowned; impl Job for Input{{type Output={output};}} fn own(input:Query<&Input>){{}} fn receive(input:Query<&JobResult<Input>>){{}} struct RootPlugin; impl Plugin for RootPlugin{{fn build(&self,app:&mut App){{app.add_systems(Update,(own,receive));}}}}"));
        inventory.resolve_usage();
        let mut policy = Policy::default();
        if legacy {
            let key = inventory.key("struct:Unowned");
            policy.legacy.push(Legacy {
                key: key.clone(),
                fingerprint: inventory.entries[&key].fingerprint.clone(),
                issue: "VMX-253".into(),
            });
        }
        assert!(inventory.canonical(
            &inventory.entries[&inventory.key("struct:Input")],
            &policy,
            &mut BTreeSet::new()
        ));
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("fn:receive")],
                &policy,
                &mut BTreeSet::new()
            ),
            expected,
            "{output}: legacy={legacy}"
        );
    }
}

#[test]
fn unit_job_outputs_do_not_turn_presence_filters_into_payload_reads() {
    for parameter in [
        "Query<Entity,With<JobResult<Input>>>",
        "Query<Has<JobResult<Input>>>",
        "ParamSet<(Query<&JobResult<Other>>,Query<Entity,With<JobResult<Input>>>)>",
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job{type Output;} #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; struct Input; struct Other; impl Job for Input{{type Output=();}} impl Job for Other{{type Output=();}} fn receive(input:{parameter}){{}} fn payload(input:Query<&JobResult<Other>>){{}} struct RootPlugin; impl Plugin for RootPlugin{{fn build(&self,app:&mut App){{app.add_systems(Update,(receive,payload));}}}}"));
        inventory.resolve_usage();
        assert!(
            inventory
                .used(&inventory.entries["crates/util/vmux_ecs/src/jobs.rs||struct:JobResult"])
        );
        assert!(
            !inventory.canonical(
                &inventory.entries[&inventory.key("fn:receive")],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            "{parameter}"
        );
    }
}

#[test]
fn indexed_job_result_evidence_refuses_shadowing_cycles_and_unmatched_implementations() {
    for (binding, implementation, definition, argument) in [
        (
            "use vmux_ecs::jobs::Job;",
            "impl Job for Input {type Output=Bytes;}",
            "struct JobResult<T>(T);",
            "Input",
        ),
        (
            "use vmux_ecs::jobs::JobResult; trait Job {type Output;}",
            "impl Job for Input {type Output=Bytes;}",
            "",
            "Input",
        ),
        ("use vmux_ecs::jobs::{Job,JobResult};", "", "", "Input"),
        (
            "use vmux_ecs::jobs::{Job,JobResult};",
            "impl Job for Input {type Output=Bytes;}",
            "type Alias=Other; type Other=Alias;",
            "Alias",
        ),
        (
            "use vmux_ecs::jobs::{Job,JobResult};",
            "impl Job for Input {type Output=Alias;}",
            "type Alias=Other; type Other=Alias;",
            "Input",
        ),
        (
            "use vmux_ecs::jobs::{Job,JobResult};",
            "impl Job for Input {type Output=JobResult<Input>;}",
            "",
            "Input",
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; {binding} struct Input; #[derive(Component)] struct Bytes(Vec<u8>); {definition} {implementation} fn receive(input:Query<&JobResult<{argument}>>){{}} struct RootPlugin; impl Plugin for RootPlugin{{ fn build(&self,app:&mut App){{app.add_systems(Update,receive);}} }}"));
        inventory.resolve_usage();
        assert!(
            !inventory.canonical(
                &inventory.entries[&inventory.key("fn:receive")],
                &Policy::default(),
                &mut BTreeSet::new(),
            ),
            "{binding} {implementation} {definition} {argument}"
        );
    }
}

#[test]
fn job_output_ownership_requires_payload_reads_instead_of_query_filters() {
    for (parameter, expected) in [
        ("Query<&JobResult<Input>>", true),
        ("Query<Option<&JobResult<Input>>>", true),
        ("Query<Entity,With<JobResult<Input>>>", false),
        ("Query<Entity,Without<JobResult<Input>>>", false),
        ("Query<Entity,Changed<JobResult<Input>>>", false),
        ("Query<Has<JobResult<Input>>>", false),
        ("Query<&JobResult<Other>,With<JobResult<Input>>>", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; struct Input; struct Other; #[derive(Component)] struct Bytes(Vec<u8>); impl Job for Input {{ type Output=Bytes; }} impl Job for Other {{ type Output=(); }} fn receive(input:{parameter}) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"));
        inventory.resolve_usage();
        let output = inventory
            .entries
            .values()
            .find(|entry| {
                entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes"
            })
            .unwrap();
        assert_eq!(inventory.used(output), expected, "{parameter}");
    }
}

#[test]
fn job_output_ownership_resolves_concrete_aliases_and_refuses_alias_cycles() {
    for (aliases, argument, expected) in [
        ("type Request=Input<Left>;", "Request", true),
        ("type Request<T>=Input<T>;", "Request<Left>", true),
        (
            "type Request=Other; type Other=Input<Left>;",
            "Request",
            true,
        ),
        ("type Request=Other; type Other=Request;", "Request", false),
        ("type Request=Missing;", "Request", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; struct Left; struct Input<T>(std::marker::PhantomData<T>); #[derive(Component)] struct Bytes(Vec<u8>); impl Job for Input<Left> {{ type Output=Bytes; }} {aliases} fn receive(input:Query<&JobResult<{argument}>>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"));
        inventory.resolve_usage();
        let output = inventory
            .entries
            .values()
            .find(|entry| {
                entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes"
            })
            .unwrap();
        assert_eq!(inventory.used(output), expected, "{aliases}");
    }
}

#[test]
fn job_outputs_own_component_values_without_crediting_phantom_type_arguments() {
    for (output_type, expected) in [
        ("Bytes", true),
        ("Result<Option<Bytes>,String>", true),
        ("OutputAlias", true),
        ("std::marker::PhantomData<Bytes>", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; struct Input; #[derive(Component)] struct Bytes(Vec<u8>); type OutputAlias=Result<Bytes,String>; impl Job for Input {{ type Output={output_type}; }} fn receive(input:Query<&JobResult<Input>>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"));
        inventory.resolve_usage();
        let output = inventory
            .entries
            .values()
            .find(|entry| {
                entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes"
            })
            .unwrap();
        assert_eq!(inventory.used(output), expected, "{output_type}");
    }
}

#[test]
fn job_output_components_follow_concrete_generic_system_instantiations() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
    inventory.source("crates/feature/receiver/src/lib.rs", "use bevy_app::{App,Plugin,Update}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{Job,JobResult}; struct Input; struct T; #[derive(Component)] struct Bytes(Vec<u8>); #[derive(Component)] struct Shadowed(Vec<u8>); impl Job for Input { type Output=Bytes; } impl Job for T { type Output=Shadowed; } fn receive<T:Job>(input:Query<&JobResult<T>>) {} struct RootPlugin; impl Plugin for RootPlugin { fn build(&self,app:&mut App) { app.add_systems(Update,receive::<Input>); } }");
    inventory.resolve_usage();
    let output = inventory
        .entries
        .values()
        .find(|entry| entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes")
        .unwrap();
    assert!(inventory.used(output));
    let shadowed = inventory
        .entries
        .values()
        .find(|entry| {
            entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Shadowed"
        })
        .unwrap();
    assert!(!inventory.used(shadowed));
}

#[test]
fn job_outputs_require_a_matching_implementation_for_concrete_input_arguments() {
    for (implementation, argument, expected) in [
        (
            "impl Job for Input<Left> { type Output=Bytes; }",
            "Left",
            true,
        ),
        (
            "impl Job for Input<Left> { type Output=Bytes; }",
            "Right",
            false,
        ),
        (
            "impl<T> Job for Input<T> { type Output=Bytes; }",
            "Right",
            true,
        ),
        (
            "impl Job for Input<Left> { type Output=Bytes; } impl Job for Input<Right> { type Output=OtherBytes; }",
            "Left",
            true,
        ),
        (
            "impl Job for Input<Left> { type Output=Bytes; } impl Job for Input<Right> { type Output=OtherBytes; }",
            "Right",
            false,
        ),
        (
            "impl<T> Job for Input<T> { type Output=Bytes; } impl Job for Input<Left> { type Output=OtherBytes; }",
            "Left",
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source("crates/feature/receiver/src/lib.rs", &format!("use bevy_app::{{App,Plugin,Update}}; use bevy_ecs::prelude::*; use vmux_ecs::jobs::{{Job,JobResult}}; struct Left; struct Right; struct Input<T>(std::marker::PhantomData<T>); #[derive(Component)] struct Bytes(Vec<u8>); #[derive(Component)] struct OtherBytes(Vec<u8>); {implementation} fn receive(input:Query<&JobResult<Input<{argument}>>>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"));
        inventory.resolve_usage();
        let output = inventory
            .entries
            .values()
            .find(|entry| {
                entry.path == "crates/feature/receiver/src/lib.rs" && entry.name == "Bytes"
            })
            .unwrap();
        assert_eq!(
            inventory.used(output),
            expected,
            "{implementation}: {argument}"
        );
    }
}

#[test]
fn stateless_framework_bootstraps_keep_fixture_and_enum_edges_without_manual_records() {
    for (installed, expected) in [("RootPlugin", true), ("OtherPlugin", false)] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App, Plugin}}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; #[derive(SystemSet)] enum Flow {{ Read }} fn read(state: Query<&State>) {{}} struct RootPlugin; impl Plugin for RootPlugin {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read.in_set(Flow::Read)); }} }} struct OtherPlugin; #[cfg(test)] mod tests {{ use super::*; struct Fixture; impl Fixture {{ fn new() {{ let mut app = App::new(); app.add_plugins({installed}); }} }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "struct:Fixture");
        boundary.interface = "headless production-plugin fixture".into();
        boundary.input = "external: one isolated App".into();
        boundary.output = format!("ECS: {}", inventory.key("struct:State"));
        assert_eq!(boundary.references(&inventory), expected);
        let key = inventory.key("enum:Flow");
        let purpose = EnumPurpose {
            key: key.clone(),
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            role: EnumRole::Boundary,
            owner: inventory.key("struct:RootPlugin"),
            reason: "real registered Bevy schedule ordering".into(),
        };
        assert!(purpose.valid(&inventory.entries[&key], &inventory, &Policy::default()));
    }
}

#[test]
fn generic_registration_boundaries_keep_their_instantiation_paths() {
    let inventory = Inventory::fixture(
        "use bevy_app::{App,Plugin}; use bevy_ecs::prelude::*; #[derive(Component)] struct A; #[derive(Component)] struct B; #[derive(Component)] struct Shared; struct Bridge<T>(std::marker::PhantomData<T>); impl<T> Plugin for Bridge<T> { fn build(&self,app:&mut App) { app.add_observer(receive::<T>); } } fn receive<T>(event:Query<(&T,&Shared)>) {} struct First<T>(std::marker::PhantomData<T>); impl<T> Plugin for First<T> { fn build(&self,app:&mut App) { app.add_plugins(Bridge::<T>::default()); } } struct Second<T>(std::marker::PhantomData<T>); impl<T> Plugin for Second<T> { fn build(&self,app:&mut App) { app.add_plugins(Bridge::<T>::default()); } } struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_plugins((First::<A>::default(),Second::<B>::default())); } }",
    );
    for (name, owns) in [("First", "A"), ("Second", "B")] {
        let owner = &inventory.entries[&inventory.key(&format!("struct:{name}"))];
        for endpoint in ["A", "B", "Shared"] {
            let endpoint_value = &inventory.entries[&inventory.key(&format!("struct:{endpoint}"))];
            assert_eq!(
                inventory.boundary_references(owner, endpoint_value, &mut BTreeSet::new()),
                endpoint == owns || endpoint == "Shared",
                "{name}: {endpoint}"
            );
        }
    }
}

#[test]
fn generic_registration_uses_struct_expressions_aliases_and_component_hooks() {
    for parameter in ["Query<&T>", "Data<T>", "Args<T>"] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::{{App,Plugin}}; use bevy_ecs::prelude::*; #[derive(Component)] #[component(on_add=added)] struct State; fn added() {{}} type Data<T> = Query<&T>; #[derive(SystemParam)] struct Args<T> {{ values:Query<&T> }} struct Bridge<T> {{ marker:std::marker::PhantomData<T> }} impl<T> Plugin for Bridge<T> {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive::<T>); }} }} fn receive<T>(values:{parameter}) {{}} struct Root; impl Plugin for Root {{ fn build(&self,app:&mut App) {{ app.add_plugins(Bridge::<State> {{ marker:std::marker::PhantomData }}); }} }}"
        ));
        assert!(
            inventory.used(&inventory.entries[&inventory.key("struct:State")]),
            "{parameter}"
        );
        assert!(
            inventory
                .registered_keys
                .contains(&inventory.key("fn:added")),
            "{parameter}"
        );
    }
}

#[test]
fn generic_plugin_arguments_need_actual_registered_ecs_parameters() {
    for (interface, app, body, constructor, production, expected) in [
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "app.add_observer(receive::<J>);",
            "crate::jobs::Bridge",
            true,
            true,
        ),
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "app.add_observer(receive::<J>);",
            "crate::jobs::Alias",
            true,
            true,
        ),
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "app.add_systems(Update, read::<J>.in_set(Stage));",
            "crate::jobs::Bridge",
            true,
            true,
        ),
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "let _ = std::marker::PhantomData::<J>;",
            "crate::jobs::Bridge",
            true,
            false,
        ),
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "app.add_observer(phantom::<J>);",
            "crate::jobs::Bridge",
            true,
            false,
        ),
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "app.add_systems(Update, fake::<J>);",
            "crate::jobs::Bridge",
            true,
            false,
        ),
        (
            "crate::jobs::Plugin",
            "bevy_app::App",
            "app.add_observer(receive::<J>);",
            "crate::jobs::Bridge",
            true,
            false,
        ),
        (
            "bevy_app::Plugin",
            "crate::jobs::App",
            "app.add_observer(receive::<J>);",
            "crate::jobs::Bridge",
            true,
            false,
        ),
        (
            "bevy_app::Plugin",
            "bevy_app::App",
            "app.add_observer(receive::<J>);",
            "crate::jobs::Bridge",
            false,
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/fixture/src/values.rs", "use bevy_ecs::prelude::*; #[derive(Component)] pub struct Registered; #[derive(Component)] pub struct Unused;");
        inventory.source("crates/util/fixture/src/jobs.rs", &format!("use bevy_ecs::prelude::*; pub trait Plugin {{}} pub struct App; struct FakeQuery<T>(std::marker::PhantomData<T>); pub struct Bridge<J,N>(std::marker::PhantomData<(J,N)>); pub use Bridge as Alias; impl<J,N> {interface} for Bridge<J,N> {{ fn build(&self, app: &mut {app}) {{ {body} }} }} fn receive<T>(event: On<Add,T>) {{}} fn read<T>(values: Query<&T>) {{}} fn phantom<T>(marker: std::marker::PhantomData<T>) {{}} fn fake<T>(values: FakeQuery<T>) {{}}"));
        let attribute = if production { "" } else { "#[cfg(test)]" };
        inventory.source("crates/util/fixture/src/lib.rs", &format!("mod jobs; mod values; struct Root; {attribute} impl bevy_app::Plugin for Root {{ fn build(&self, app: &mut bevy_app::App) {{ app.add_plugins({constructor}::<crate::values::Registered,crate::values::Unused>::default()); }} }}"));
        inventory.resolve_usage();
        for (name, used) in [("Registered", expected), ("Unused", false)] {
            let key = format!("crates/util/fixture/src/values.rs||struct:{name}");
            assert_eq!(
                inventory.used(&inventory.entries[&key]),
                used,
                "{interface} {app} {body} {production} {name}"
            );
        }
    }
}

#[test]
fn nested_generic_plugins_preserve_argument_positions_and_boundary_edges() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/fixture/src/values.rs", "use bevy_ecs::prelude::*; #[derive(Component)] pub struct Registered; #[derive(Component)] pub struct Unused;");
    inventory.source("crates/util/fixture/src/jobs.rs", "use bevy_app::{App,Plugin}; use bevy_ecs::prelude::*; pub struct Outer<A,B>(std::marker::PhantomData<(A,B)>); pub struct Inner<A,B>(std::marker::PhantomData<(A,B)>); impl<J,N> Plugin for Outer<J,N> { fn build(&self,app:&mut App) { app.add_plugins(Inner::<N,J>::default()); } } impl<J,N> Plugin for Inner<N,J> { fn build(&self,app:&mut App) { app.add_observer(receive::<J>); } } fn receive<T>(event: On<Add,T>) {}");
    inventory.source("crates/util/fixture/src/lib.rs", "mod jobs; mod values; struct Root; impl bevy_app::Plugin for Root { fn build(&self,app:&mut bevy_app::App) { app.add_plugins(crate::jobs::Outer::<crate::values::Registered,crate::values::Unused>::default()); } }");
    inventory.resolve_usage();
    let registered = &inventory.entries["crates/util/fixture/src/values.rs||struct:Registered"];
    let unused = &inventory.entries["crates/util/fixture/src/values.rs||struct:Unused"];
    assert!(inventory.used(registered));
    assert!(!inventory.used(unused));
    let receiver = &inventory.entries["crates/util/fixture/src/jobs.rs||struct:Inner"];
    assert!(inventory.boundary_references(receiver, registered, &mut BTreeSet::new()));
    assert!(!inventory.boundary_references(receiver, unused, &mut BTreeSet::new()));
}

#[test]
fn generic_example_registrations_resolve_hyphenated_package_directories() {
    let mut inventory = Inventory::default();
    let path = "examples/framework-consumer/tests/generic.rs";
    inventory.source(path, "use bevy_app::{App,Plugin}; use bevy_ecs::prelude::*; #[derive(Component)] struct State; struct Bridge<T>(std::marker::PhantomData<T>); impl<J> Plugin for Bridge<J> { fn build(&self,app:&mut App) { app.add_observer(receive::<J>); } } fn receive<T>(event:On<Add,T>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_plugins(Bridge::<State>::default()); } }");
    inventory.resolve_usage();
    assert!(inventory.used(&inventory.entries[&format!("{path}||struct:State")]));
}

#[test]
fn compiler_boundaries_require_external_framework_or_real_compiler_input_io() {
    for (source, accepted) in [
        (
            "use rustc_lint::LateLintPass as Pass; struct Adapter; impl Pass for Adapter {}",
            true,
        ),
        (
            "use rustc_hir::intravisit::Visitor; struct Adapter; impl Visitor for Adapter {}",
            true,
        ),
        (
            "trait LateLintPass {} struct Adapter; impl LateLintPass for Adapter {}",
            false,
        ),
        (
            "mod rustc_lint { pub trait LateLintPass {} } struct Adapter; impl rustc_lint::LateLintPass for Adapter {}",
            false,
        ),
        (
            "struct Adapter { data: Vec<String> } impl Adapter { fn choose(&self) {} }",
            false,
        ),
        (
            "use std::process::Command; #[cfg(test)] struct Adapter { directory: tempfile::TempDir } #[cfg(test)] impl Adapter { fn check(&self) { Command::new(\"cargo\"); } } #[test] fn fixture() { let _ = include_str!(\"case.rs\"); Adapter::check(); }",
            true,
        ),
    ] {
        let inventory = Inventory::fixture(source);
        let adapter = &inventory.entries["fixture.rs||struct:Adapter"];
        assert_eq!(inventory.compiler_boundary(adapter), accepted, "{source}");
    }
}

#[test]
fn ownership_rejects_parallel_models_unused_derives_and_hidden_state() {
    for source in [
        "struct Record { id: String, message: String }",
        "#[derive(Component)] struct Unused { position: f32 }",
        "#[derive(Component)] struct Unused { position: f32 } fn unregistered(state: Query<&Unused>) {}",
        "struct Row { id: String } #[derive(Resource)] struct Registry { rows: std::collections::HashMap<String, Row> } fn install(app: &mut App) { app.init_resource::<Registry>(); }",
        "trait Engine { fn run(&mut self); } #[derive(Resource)] struct State { engine: Box<dyn Engine> } fn install(app: &mut App) { app.init_resource::<State>(); }",
        "fn install(app: &mut App) { app.add_systems(Update, advance); } fn advance(mut state: Local<u64>) { *state += 1; }",
        "static STATE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());",
        "#[derive(Component)] struct State { graph: Graph } struct Graph { records: Vec<Record> } struct Record { id: String } fn advance(state: Query<&State>) {} fn install(app: &mut App) { app.add_systems(Update, advance); }",
        "#[vmux_api::ui_state] struct View { rows: Vec<String> } impl View { fn authorize(&self) -> bool { !self.rows.is_empty() } }",
    ] {
        let inventory = Inventory::fixture(source);
        assert!(
            !Policy::default().audit(&inventory).is_empty(),
            "accepted hidden ownership: {source}"
        );
    }
}

#[test]
fn private_helpers_count_qualified_aliases_callbacks_and_other_modules() {
    for extra in [
        "",
        "fn second(state: Query<&State>) { crate::helper(state); }",
        "fn second(state: Option<Query<&State>>) { state.map(crate::helper); }",
        "use crate::helper as callback; fn second(state: Option<Query<&State>>) { state.map(callback); }",
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/feature/example/src/lib.rs", "mod other; #[derive(Component)] struct State { value: u32 } fn helper(state: Query<&State>) {} fn first(state: Query<&State>) { self::helper(state); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, first); } }");
        inventory.source("crates/feature/example/src/other.rs", extra);
        inventory.resolve_usage();
        let helper = &inventory.entries["crates/feature/example/src/lib.rs||fn:helper"];
        assert_eq!(
            inventory.canonical(helper, &Policy::default(), &mut BTreeSet::new()),
            extra.is_empty(),
            "{extra}"
        );
    }
}

#[test]
fn nested_owned_geometry_values_require_a_complete_referenced_component_chain() {
    for dynamic in [false, true] {
        let fields = if dynamic {
            "Row(Box<dyn Engine>)"
        } else {
            "Row"
        };
        let inventory = Inventory::fixture(&format!(
            "trait Engine {{}} #[derive(Component)] struct Split {{ direction: Direction }} enum Direction {{ {fields} }} struct Gaps {{ size: f32 }} impl Direction {{ fn gaps(&self) -> Gaps {{ Gaps {{ size: 4.0 }} }} }} fn read(split: Query<&Split>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));

        let mut policy = Policy::default();
        for (key, owner) in [
            ("enum:Direction", "struct:Split"),
            ("struct:Gaps", "enum:Direction"),
        ] {
            policy.values.push(ValueOwner {
                key: inventory.key(key),
                owner: inventory.key(owner),
                lifetime: "one immutable component-owned geometry value".into(),
            });
        }
        let gaps = &inventory.entries[&inventory.key("struct:Gaps")];
        assert_eq!(
            inventory.canonical(gaps, &policy, &mut BTreeSet::new()),
            !dynamic
        );
        policy.values.remove(0);
        assert!(!inventory.canonical(gaps, &policy, &mut BTreeSet::new()));
    }
}

#[test]
fn fixture_mutex_boundaries_cannot_approve_product_state_or_shadowed_standard_types() {
    for (source, allowed) in [
        (
            "#[cfg(test)] static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());",
            true,
        ),
        (
            "static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());",
            false,
        ),
        (
            "#[cfg(test)] static GUARD: std::sync::Mutex<fn()> = std::sync::Mutex::new(callback);",
            false,
        ),
        (
            "#[cfg(test)] static GUARD: std::sync::Mutex<&()> = std::sync::Mutex::new(&());",
            false,
        ),
        (
            "#[cfg(test)] static GUARD: std::sync::Mutex<[(); 1]> = std::sync::Mutex::new([()]);",
            false,
        ),
        (
            "#[cfg(test)] static GUARD: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());",
            false,
        ),
        (
            "mod std {} #[cfg(test)] static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(source);
        let guard = &inventory.entries[&inventory.key("static:GUARD")];
        assert_eq!(inventory.fixture_lock(guard), allowed, "{source}");
    }
}

#[test]
fn querying_frozen_components_does_not_approve_new_component_models() {
    for copied in [false, true] {
        let extra = if copied {
            "#[derive(Component)] struct Copy { model: Model } fn copy(state: Query<&Copy>) {}"
        } else {
            ""
        };
        let systems = if copied { "(read, copy)" } else { "read" };
        let inventory = Inventory::fixture(&format!(
            "struct Model {{ value: String }} #[derive(Component)] struct LegacyState {{ model: Model }} fn read(state: Query<&LegacyState>) {{}} {extra} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, {systems}); }} }}"
        ));
        let mut policy = Policy::default();
        for key in ["struct:Model", "struct:LegacyState"] {
            let key = inventory.key(key);
            policy.legacy.push(Legacy {
                fingerprint: inventory.entries[&key].fingerprint.clone(),
                key,
                issue: "VMX-253".into(),
            });
        }
        let read = &inventory.entries[&inventory.key("fn:read")];
        assert!(inventory.canonical(read, &policy, &mut BTreeSet::new()));
        if copied {
            let copy = &inventory.entries[&inventory.key("struct:Copy")];
            assert!(!inventory.canonical(copy, &policy, &mut BTreeSet::new()));
        }
    }
}

#[test]
fn app_composition_metadata_reaches_only_its_declared_plugin_payloads() {
    for (attribute, allowed) in [("app_plugin", true), ("vmux_api::app_plugin", false)] {
        let inventory = Inventory::fixture(&format!(
            "use vmux_macro::app_plugin; #[derive(Component)] struct Position {{ x: f32 }} fn read(position: Query<&Position>) {{}} struct Child; impl Plugin for Child {{ fn build(&self, app: &mut App) {{ app.register_type::<Position>().add_systems(Update, read); }} }} #[{attribute}] enum Composition {{ #[plugin] Child(Child) }}"
        ));
        let adapter = &inventory.entries[&inventory.key("enum:Composition")];
        let endpoint = &inventory.entries[&inventory.key("struct:Position")];
        assert_eq!(
            inventory.boundary_references(adapter, endpoint, &mut BTreeSet::new()),
            allowed
        );
    }
}

#[test]
fn manifest_metadata_boundaries_require_the_real_interface_and_a_stateless_marker() {
    for (fields, interface, allowed) in [
        ("", "vmux_feature::manifest::FeatureManifestSource", true),
        (
            "{ state: String }",
            "vmux_feature::manifest::FeatureManifestSource",
            false,
        ),
        (
            "{ callback: fn() }",
            "vmux_feature::manifest::FeatureManifestSource",
            false,
        ),
        (
            "{ unit: () }",
            "vmux_feature::manifest::FeatureManifestSource",
            false,
        ),
        ("", "LocalSource", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/feature/vmux_feature/src/manifest.rs",
            "pub trait FeatureManifestSource { const SOURCE: &'static str; }",
        );
        let terminator = if fields.is_empty() { ";" } else { "" };
        inventory.source("crates/feature/example/src/lib.rs", &format!("trait LocalSource {{ const SOURCE: &'static str; }} struct Feature {fields}{terminator} impl {interface} for Feature {{ const SOURCE: &'static str = \"manifest\"; }}"));
        inventory.resolve_usage();
        let marker = &inventory.entries["crates/feature/example/src/lib.rs||struct:Feature"];
        assert_eq!(inventory.manifest_source(marker), allowed);
        assert_eq!(
            inventory.canonical(marker, &Policy::default(), &mut BTreeSet::new()),
            allowed
        );
        let implementation = inventory
            .entries
            .values()
            .find(|entry| matches!(entry.kind, Kind::Impl))
            .unwrap();
        assert_eq!(inventory.manifest_source(implementation), allowed);
    }
}

#[test]
fn manifest_sources_do_not_approve_inherent_workflows_or_test_only_implementations() {
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/feature/vmux_feature/src/manifest.rs",
        "pub trait FeatureManifestSource { const SOURCE: &'static str; }",
    );
    inventory.source("crates/feature/example/src/lib.rs", "struct Feature; impl vmux_feature::manifest::FeatureManifestSource for Feature { const SOURCE: &'static str = \"manifest\"; } impl Feature { fn workflow(&self) { loop {} } }");
    inventory.resolve_usage();
    let marker = &inventory.entries["crates/feature/example/src/lib.rs||struct:Feature"];
    let workflow = &inventory.entries["crates/feature/example/src/lib.rs||impl:Feature"];
    assert!(inventory.manifest_source(marker));
    assert!(!inventory.manifest_source(workflow));
    assert!(!inventory.canonical(workflow, &Policy::default(), &mut BTreeSet::new()));
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/feature/vmux_feature/src/manifest.rs",
        "pub trait FeatureManifestSource { const SOURCE: &'static str; }",
    );
    inventory.source("crates/feature/example/src/lib.rs", "struct Feature; #[cfg(test)] impl vmux_feature::manifest::FeatureManifestSource for Feature { const SOURCE: &'static str = \"manifest\"; }");
    inventory.resolve_usage();
    let marker = &inventory.entries["crates/feature/example/src/lib.rs||struct:Feature"];
    assert!(!inventory.manifest_source(marker));
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/feature/vmux_feature/src/manifest.rs",
        "pub trait FeatureManifestSource { const SOURCE: &'static str; }",
    );
    inventory.source("crates/feature/example/src/lib.rs", "#[cfg(test)] mod tests { struct Feature; impl vmux_feature::manifest::FeatureManifestSource for Feature { const SOURCE: &'static str = \"fixture manifest\"; } }");
    inventory.resolve_usage();
    let marker = &inventory.entries["crates/feature/example/src/lib.rs|tests|struct:Feature"];
    assert!(inventory.manifest_source(marker));
}

#[test]
fn manifest_sources_keep_bounded_multiline_declarations_as_immutable_metadata() {
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/feature/vmux_feature/src/manifest.rs",
        "pub trait FeatureManifestSource { const SOURCE: &'static str; }",
    );
    let keywords = (0..40)
        .map(|index| format!("\"keyword{index}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let manifest = format!(
        "(pages: [(name: \"main\", url: \"vmux://example/\", title: \"Example\", keywords: [{keywords}])])"
    );
    inventory.source("crates/feature/example/src/lib.rs", &format!("struct Feature; impl vmux_feature::manifest::FeatureManifestSource for Feature {{ const SOURCE: &'static str = {manifest:?}; }}"));
    inventory.resolve_usage();
    let key = inventory.key("impl:vmux_feature :: manifest :: FeatureManifestSource for Feature");
    assert!(inventory.manifest_source(&inventory.entries[&key]));
    assert!(inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn reciprocal_relationship_targets_require_a_used_source() {
    for (parameter, inverse, expected) in [
        ("binding: Query<&Binding>", "Binding", true),
        ("", "Binding", false),
        ("binding: Query<&Binding>", "Other", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Component)] #[relationship(relationship_target = Users)] struct Binding(Entity); #[derive(Component)] #[relationship_target(relationship = {inverse})] struct Users(Vec<Entity>); #[derive(Component)] struct Other(Entity); fn read({parameter}) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));
        let users = &inventory.entries[&inventory.key("struct:Users")];
        assert_eq!(
            inventory.canonical(users, &Policy::default(), &mut BTreeSet::new()),
            expected
        );
    }
}

#[test]
fn registered_query_filter_aliases_are_not_plain_tuple_models() {
    for (alias, argument, expected) in [
        (
            "(With<Marker>, Changed<State>)",
            "state: Query<&State, Scope>",
            true,
        ),
        (
            "(State, Marker)",
            "state: Query<&State>, other: Scope",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Component)] struct Marker; #[derive(Component)] struct State {{ value: u32 }} type Scope = {alias}; fn read({argument}) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));
        let scope = &inventory.entries[&inventory.key("alias:Scope")];
        assert_eq!(
            inventory.canonical(scope, &Policy::default(), &mut BTreeSet::new()),
            expected
        );
    }
}

#[test]
fn component_hooks_require_a_used_component_and_real_hook_metadata() {
    for (derive, metadata, parameter, expected) in [
        (
            "Component",
            "component(on_add = hook)",
            "state: Query<&State>",
            true,
        ),
        (
            "Component",
            "component(on_discard = hook)",
            "state: Query<&State>",
            true,
        ),
        ("Component", "component(on_discard = hook)", "", false),
        (
            "Component",
            "component(on_discard = hook())",
            "state: Query<&State>",
            true,
        ),
        (
            "Component",
            "component(on_discard = ::callbacks::capture())",
            "state: Query<&State>",
            false,
        ),
        ("Component", "component(on_add = hook)", "", false),
        (
            "Resource",
            "component(on_add = hook)",
            "state: Res<State>",
            false,
        ),
        (
            "Component",
            "unrelated(on_add = hook)",
            "state: Query<&State>",
            false,
        ),
        (
            "Component",
            "component(unrelated = hook)",
            "state: Query<&State>",
            false,
        ),
        (
            "Component",
            "cfg_attr(test, component(on_add = hook))",
            "state: Query<&State>",
            false,
        ),
        (
            "Component",
            "component(on_add = hook::<State>)",
            "state: Query<&State>",
            true,
        ),
        (
            "Component",
            "component(on_add = ::callbacks::capture::<State>)",
            "state: Query<&State>",
            false,
        ),
    ] {
        let generics = if metadata.contains("::<") { "<T>" } else { "" };
        let inventory = Inventory::fixture(&format!(
            "mod callbacks {{ pub fn capture{generics}(world: bevy_ecs::world::DeferredWorld, context: bevy_ecs::lifecycle::HookContext) {{}} }} use callbacks::capture as hook; #[derive({derive})] #[{metadata}] struct State {{ value: u32 }} fn read({parameter}) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));
        let hook = &inventory.entries[&inventory.key("callbacks|fn:capture")];
        assert_eq!(
            inventory.canonical(hook, &Policy::default(), &mut BTreeSet::new()),
            expected
        );
    }
}

#[test]
fn canonical_components_and_registered_systems_share_one_owner() {
    let inventory = Inventory::fixture(
        "#[derive(Component)] struct Position { x: f32 } impl Position { fn move_by(&mut self, x: f32) { self.x += x; } } fn move_position(mut positions: Query<&mut Position>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, move_position); } }",
    );
    let mut policy = Policy::default();
    for key in ["struct:Root", "impl:Plugin for Root"] {
        policy.boundaries.push(Boundary::fixture(&inventory, key));
    }
    assert!(policy.audit(&inventory).is_empty());
}

#[test]
fn entity_events_resolve_through_module_aliases_and_barrel_exports() {
    let inventory = Inventory::fixture(
        "mod events { #[derive(EntityEvent)] pub struct Received { entity: Entity } } mod bridge { pub use crate::events::*; } pub use bridge as input; fn receive(event: On<crate::input::Received>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_observer(receive); } }",
    );
    let event = inventory.key("events|struct:Received");
    let system = inventory.key("fn:receive");
    for key in [event, system] {
        assert!(inventory.canonical(
            &inventory.entries[&key],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
    let mut shadowed = Inventory::default();
    shadowed.source(
        "crates/feature/input/src/lib.rs",
        "extern crate self as input; #[derive(EntityEvent)] pub struct Received { entity:Entity }",
    );
    shadowed.source(
        "crates/feature/consumer/src/lib.rs",
        "struct Domain { value:String } mod domain { #[derive(EntityEvent)] pub struct Received { value:crate::Domain } } use domain as input; fn receive(event:On<input::Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_observer(receive); } }",
    );
    shadowed.source(
        "crates/feature/client/src/lib.rs",
        "fn accepted(event:On<input::Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_observer(accepted); } }",
    );
    shadowed.resolve_usage();
    let accepted = shadowed.key("fn:accepted");
    assert!(shadowed.canonical(
        &shadowed.entries[&accepted],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let system = shadowed.key("fn:receive");
    assert!(!shadowed.canonical(
        &shadowed.entries[&system],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let declarations = Inventory::fixture(
        "struct Domain { value:String } #[derive(EntityEvent)] struct Received { entity:Entity } mod input { #[cfg(target_os=\"linux\")] #[derive(EntityEvent)] pub struct Received { value:crate::Domain } #[cfg(target_os=\"macos\")] #[derive(EntityEvent)] pub struct Received { value:crate::Domain } } pub use input as events; fn receive(event:On<events::Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_observer(receive); } }",
    );
    let system = declarations.key("fn:receive");
    assert!(!declarations.canonical(
        &declarations.entries[&system],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let ambiguous = Inventory::fixture(
        "struct Domain { value:String } mod domain { #[derive(EntityEvent)] pub struct Received { value:crate::Domain } } mod events { #[derive(EntityEvent)] pub struct Received { entity:Entity } } mod bridge { #[cfg(target_os=\"linux\")] pub use crate::domain::Received; #[cfg(target_os=\"macos\")] pub use crate::events::Received; } pub use bridge as input; fn receive(event:On<crate::input::Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_observer(receive); } }",
    );
    let system = ambiguous.key("fn:receive");
    assert!(!ambiguous.canonical(
        &ambiguous.entries[&system],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    for source in [
        "#[derive(EntityEvent)] struct Received { entity:Entity }",
        "#[derive(EntityEvent)] struct Received { entity:Entity } fn receive(event:On<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { #[cfg(test)] { app.add_observer(receive); } } }",
    ] {
        let inventory = Inventory::fixture(source);
        let key = inventory.key("struct:Received");
        assert!(!inventory.canonical(
            &inventory.entries[&key],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn shallow_barrel_probes_do_not_consume_the_recursion_budget() {
    let mut source = String::from("mod bag {");
    for index in 0..200 {
        source.push_str(&format!(
            "mod a{index:03} {{}} pub use self::a{index:03}::*;"
        ));
    }
    source.push_str("mod zed { #[derive(EntityEvent)] pub struct Received { entity:Entity } } pub use self::zed::*; } use bag::*; fn receive(event:On<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_observer(receive); } }");
    let inventory = Inventory::fixture(&source);
    let system = inventory.key("fn:receive");
    assert!(inventory.canonical(
        &inventory.entries[&system],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let cyclic = Inventory::fixture(
        "mod inaccessible { #[derive(EntityEvent)] pub struct Received { entity:Entity } } mod one { pub use crate::two::*; } mod two { pub use crate::one::*; } use one::*; fn receive(event:On<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_observer(receive); } }",
    );
    let system = cyclic.key("fn:receive");
    assert!(!cyclic.canonical(
        &cyclic.entries[&system],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn boundary_records_require_current_code_and_a_real_lifetime() {
    let inventory = Inventory::fixture("struct Codec { bytes: Vec<u8> }");
    let mut policy = Policy::default();
    let boundary = Boundary::fixture(&inventory, "struct:Codec");
    policy.boundaries.push(boundary.clone());
    assert!(policy.audit(&inventory).is_empty());
    policy.boundaries[0].lifetime.clear();
    assert!(!policy.audit(&inventory).is_empty());
    policy.boundaries[0] = boundary;
    policy.boundaries[0].fingerprint = "obsolete".into();
    assert!(!policy.audit(&inventory).is_empty());
}

#[test]
fn shared_product_build_adapters_do_not_approve_runtime_configuration_models() {
    for (path, accepted) in [
        ("crates/build_product.rs", true),
        ("crates/build_cfg_product.rs", true),
        ("crates/util/vmux_profile/src/product.rs", false),
        ("crates/feature/vmux_setting/src/product.rs", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            path,
            "pub struct Metadata; impl Metadata { pub fn read() {} }",
        );
        inventory.resolve_usage();
        let mut policy = Policy::default();
        policy
            .boundaries
            .push(Boundary::fixture(&inventory, "struct:Metadata"));
        policy
            .boundaries
            .push(Boundary::fixture(&inventory, "impl:Metadata"));
        assert_eq!(policy.audit(&inventory).is_empty(), accepted, "{path}");
    }
}

#[test]
fn bevy_window_reexports_bind_the_pinned_component_without_accepting_shadow_modules() {
    for (prefix, allowed) in [
        ("", true),
        ("mod bevy { pub mod window { pub struct Window; } }", false),
        (
            "mod other { pub mod window { pub struct Window; } } use other as bevy;",
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            "patches/bevy_window-0.19.0/src/lib.rs",
            "pub mod window; pub use window::Window;",
        );
        inventory.source(
            "patches/bevy_window-0.19.0/src/window.rs",
            "#[derive(Component)] pub struct Window;",
        );
        inventory.source("crates/app/example/src/lib.rs", &format!("{prefix} use bevy::window::Window as NativeWindow; pub struct Factory; impl Factory {{ pub fn create() -> NativeWindow {{ NativeWindow }} }} fn inspect(windows: Query<&NativeWindow>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ app.add_systems(Update, inspect); }} }}"));
        inventory.resolve_usage();
        let mut boundary = Boundary::fixture(&inventory, "impl:Factory");
        boundary.input = "external: native window metadata".into();
        boundary.output = "ECS: patches/bevy_window-0.19.0/src/window.rs||struct:Window".into();
        assert_eq!(boundary.references(&inventory), allowed, "{prefix}");
    }
}

#[test]
fn registered_systems_borrow_exact_upstream_components_without_approving_local_copies() {
    for (target, expected) in [("NativeWindow", true), ("Model", false), ("Copy", false)] {
        let mut inventory = Inventory::default();
        inventory.source(
            "patches/bevy_window-0.19.0/src/lib.rs",
            "pub mod window; pub use window::Window;",
        );
        inventory.source(
            "patches/bevy_window-0.19.0/src/window.rs",
            "struct Handle; #[derive(Component)] pub struct Window { handle: Handle }",
        );
        inventory.source("crates/app/example/src/lib.rs", &format!("use bevy::window::Window as NativeWindow; struct Model {{ value: String }} #[derive(Component)] struct Copy {{ model: Model }} fn inspect(windows: Query<&{target}>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app:&mut App) {{ app.add_systems(Update, inspect); }} }}"));
        inventory.resolve_usage();
        let window = inventory.key("struct:Window");
        let mut policy = Policy {
            upstream: vec![Upstream {
                package: "bevy_window".into(),
                version: "0.19.0".into(),
                archive_sha256: String::new(),
                entries: BTreeMap::from([(
                    window.clone(),
                    inventory.entries[&window].fingerprint.clone(),
                )]),
            }],
            ..Default::default()
        };
        let inspect = &inventory.entries[&inventory.key("fn:inspect")];
        assert_eq!(
            inventory.canonical(inspect, &policy, &mut BTreeSet::new()),
            expected,
            "{target}"
        );
        policy.upstream[0].entries.insert(window, "changed".into());
        assert!(!inventory.canonical(inspect, &policy, &mut BTreeSet::new()));
    }
}

#[test]
fn legacy_revision_cannot_be_moved_to_refresh_domain_debt() {
    let policy = Policy {
        revision: "0000000000000000000000000000000000000000".into(),
        ..Policy::default()
    };
    assert!(std::panic::catch_unwind(|| policy.prove_legacy(Path::new("."))).is_err());
}

#[test]
fn test_only_and_dead_registration_cannot_establish_production_ownership() {
    for source in [
        "#[derive(Component)] struct State { x: f32 } #[test] fn fixture() { app.world_mut().spawn(State { x: 0.0 }); }",
        "#[derive(Component)] struct State { x: f32 } fn dead(app: &mut App) { app.add_systems(Update, read); } fn read(state: Query<&State>) {}",
        "#[derive(Component)] struct State { x: f32 } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } mod forgotten { #[derive(Component)] struct State { x: f32 } fn read(state: Query<&State>) {} }",
    ] {
        let inventory = Inventory::fixture(source);
        let mut policy = Policy::default();
        for entry in inventory.entries.values() {
            if !entry.ecs && !matches!(entry.kind, Kind::Function) {
                policy.boundaries.push(Boundary::fixture(
                    &inventory,
                    entry.key.rsplit('|').next().unwrap(),
                ));
            }
        }
        let mut rejected = false;
        for entry in inventory.entries.values() {
            if entry.name == "State"
                && matches!(entry.kind, Kind::Struct)
                && !inventory.canonical(entry, &policy, &mut BTreeSet::new())
            {
                rejected = true;
            }
        }
        assert!(rejected, "accepted unowned production state: {source}");
    }
}

#[test]
fn same_named_components_resolve_within_their_registered_module() {
    let inventory = Inventory::fixture(
        "mod one { #[derive(Component)] struct State { x: f32 } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } } mod two { #[derive(Component)] struct State { y: f32 } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } } }",
    );
    let mut policy = Policy::default();
    for entry in inventory.entries.values() {
        if entry.name == "Root" || entry.owner.as_deref() == Some("Root") {
            let key = entry.key.split_once('|').unwrap().1;
            policy.boundaries.push(Boundary::fixture(&inventory, key));
        }
    }
    assert!(
        policy.audit(&inventory).is_empty(),
        "{:?}",
        policy.audit(&inventory)
    );
}

#[test]
fn standard_result_does_not_resolve_to_an_unrelated_module_alias() {
    for source in [
        "mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed { result: Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed { result: core::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod local { pub struct Other; } use local::*; mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
    ] {
        let inventory = Inventory::fixture(source);
        let entry = &inventory.entries[&inventory.key("struct:Completed")];
        assert!(inventory.canonical(entry, &Policy::default(), &mut BTreeSet::new()));
    }
    for source in [
        "struct Result<T, E> { payload: T, error: E } #[derive(Message)] struct Completed { result: Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } use missing::Result; #[derive(Message)] struct Completed { result: Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } use missing as std; #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod std { pub mod result {} } #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } extern crate missing as core; #[derive(Message)] struct Completed { result: core::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod local { pub use missing::Result; } use local::*; #[derive(Message)] struct Completed { result: Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod one { pub struct Result<T,E> { value: T, error: E } } mod two { pub struct Result<T,E> { value: T, error: E } } use one::*; use two::*; #[derive(Message)] struct Completed { result: Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod local { pub mod std { pub mod result {} } } use local::*; #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod inner { pub mod core { pub mod result {} } } mod outer { pub use super::inner::*; } use outer::*; #[derive(Message)] struct Completed { result: core::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod owner { pub mod inner { pub mod std { pub mod result {} } } } use owner::*; use inner::*; #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod source { pub mod std { pub mod result {} } pub mod bridge { pub use super::*; } } use source::bridge::*; #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod local { pub mod surface { pub mod core { pub mod result {} } } } mod facade { pub use crate::local::*; } use facade::surface::*; #[derive(Message)] struct Completed { result: core::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed<Result: Send + Sync + 'static> { result: Result } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed<LegacyDomain>>(); } }",
        "mod unrelated { type Result<T> = T; } mod one { pub mod inner {} } mod two { pub mod inner { pub mod std { pub mod result {} } } } use one::*; use two::*; use inner::*; #[derive(Message)] struct Completed { result: std::result::Result<u64, String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod domain { pub struct Result<T,E> { value: T, error: E } } #[cfg(target_os=\"linux\")] use domain::Result; #[cfg(target_os=\"macos\")] use std::result::Result; #[derive(Message)] struct Completed { result: Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } mod domain { pub mod std { pub mod result {} } } mod neutral {} #[cfg(target_os=\"linux\")] use domain as fixture; #[cfg(target_os=\"macos\")] use neutral as fixture; use fixture::*; #[derive(Message)] struct Completed { result: std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod domain { pub mod result { pub struct Result<T,E> { value:T,error:E } } } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { use domain as std; #[derive(Message)] struct Completed { result:std::result::Result<u64,String> } app.add_message::<Completed>(); } }",
        "mod owner { pub mod std { pub mod result {} } } use owner::std::{self}; mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed { result:std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod owner { pub mod inner { pub mod std { pub mod result {} } } } use owner::inner::{self}; use inner::*; mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed { result:std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod r#std { pub mod result { pub struct Result<T,E> { value:T,error:E } } } #[derive(Message)] struct Completed { result:std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod domain { pub mod result { pub struct Result<T,E> { value:T,error:E } } } use domain as r#std; #[derive(Message)] struct Completed { result:std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }",
        "mod unrelated { type Result<T> = T; } #[derive(Message)] struct Completed<r#Result: Send + Sync + 'static> { result:Result } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed<LegacyDomain>>(); } }",
    ] {
        let inventory = Inventory::fixture(source);
        let entry = &inventory.entries[&inventory.key("struct:Completed")];
        assert!(!inventory.canonical(entry, &Policy::default(), &mut BTreeSet::new()));
    }
}

#[test]
fn io_results_require_a_proven_standard_library_binding() {
    for path in ["std::io::Result", "io::Result", "Result"] {
        let inventory = Inventory::fixture(&format!(
            "use std::io; use std::io::Result; #[derive(Component)] struct Work {{ result: {path}<()> }} fn read(work: Query<&Work>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));
        let key = inventory.key("struct:Work");
        assert!(inventory.canonical(
            &inventory.entries[&key],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
    let inventory = Inventory::fixture(
        "mod io { pub struct Result<T> { pub value:T } } struct Domain { value:String } #[derive(Component)] struct Work { result:io::Result<Domain> } fn read(work:Query<&Work>) {} struct Root; impl Plugin for Root { fn build(&self, app:&mut App) { app.add_systems(Update,read); } }",
    );
    let key = inventory.key("struct:Work");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let inventory = Inventory::fixture(
        "use std::io; mod domain { pub mod io { pub struct Result<T> { value:T } } } mod consumer { use crate::domain::*; #[derive(Component)] pub struct Work { result:io::Result<()> } fn read(rows:Query<&Work>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } } }",
    );
    let key = inventory.key("struct:Work");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn query_data_fields_are_valid_registered_boundary_endpoints() {
    for field in [
        "&'static Position",
        "Option<&'static Position>",
        "std::option::Option<&'static Position>",
        "core::option::Option<&'static Position>",
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_ecs::prelude::*; use bevy_app::{{App,Plugin}}; use bevy_ecs::query::QueryData; #[derive(Component)] struct Position {{ x:f32 }} #[derive(QueryData)] struct View {{ position:{field} }} fn read(rows:Query<View>) {{}} struct Root; impl Plugin for Root {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,read); }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "fn:read");
        boundary.input = format!("ECS: {}", inventory.key("struct:Position"));
        boundary.output = "external: observed positions".into();
        assert!(
            boundary.references_from(&inventory, &Policy::default(), &mut BTreeSet::new()),
            "{field}"
        );
    }
    let inventory = Inventory::fixture(
        "use bevy_app::{App,Plugin}; use bevy_ecs::query::QueryData; mod model { #[derive(bevy_ecs::prelude::Component)] pub struct Position { x:f32 } } use model::*; #[derive(QueryData)] struct View { position:Option<&'static Position> } fn read(rows:bevy_ecs::system::Query<View>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(bevy_app::Update,read); } }",
    );
    let mut boundary = Boundary::fixture(&inventory, "fn:read");
    boundary.input = format!("ECS: {}", inventory.key("struct:Position"));
    boundary.output = "external: observed positions".into();
    assert!(boundary.references_from(&inventory, &Policy::default(), &mut BTreeSet::new()));
    for source in [
        "use bevy_ecs::prelude::*; use bevy_app::{App,Plugin}; use bevy_ecs::query::QueryData; #[derive(Component)] struct Position { x:f32 } #[derive(QueryData)] struct View { position:Option<Position> } fn read(rows:Query<View>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } }",
        "use bevy_ecs::prelude::*; use bevy_app::{App,Plugin}; use bevy_ecs::query::QueryData; struct Option<T> { value:T } #[derive(Component)] struct Position { x:f32 } #[derive(QueryData)] struct View { position:Option<&'static Position> } fn read(rows:Query<View>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } }",
        "use foreign::*; use bevy_app::{App,Plugin}; use bevy_ecs::query::QueryData; #[derive(bevy_ecs::prelude::Component)] struct Position { x:f32 } #[derive(QueryData)] struct View { position:Option<&'static Position> } fn read(rows:bevy_ecs::system::Query<View>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(bevy_app::Update,read); } }",
    ] {
        let inventory = Inventory::fixture(source);
        let mut boundary = Boundary::fixture(&inventory, "fn:read");
        boundary.input = format!("ECS: {}", inventory.key("struct:Position"));
        boundary.output = "external: observed positions".into();
        assert!(!boundary.references_from(&inventory, &Policy::default(), &mut BTreeSet::new()));
    }
}

#[test]
fn binary_child_namespaces_do_not_depend_on_source_order() {
    for reverse in [false, true] {
        let mut sources = [
            (
                "crates/util/fixture/src/bin/alpha.rs",
                "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
            ),
            (
                "crates/util/fixture/src/bin/zeta.rs",
                "use bevy_app::App; use bevy_ecs::prelude::*; mod alpha; fn read(rows:Query<&alpha::State>) {} fn main() { let mut app=App::new(); app.add_systems(Update,read); }",
            ),
        ];
        if reverse {
            sources.reverse();
        }
        let mut inventory = Inventory::default();
        for (path, source) in sources {
            inventory.source(path, source);
        }
        inventory.resolve_usage();
        assert!(inventory.canonical(
            &inventory.entries[&inventory.key("struct:State")],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn a_nested_bin_directory_preserves_ordinary_child_module_resolution() {
    let mut inventory = Inventory::default();
    for (path, source) in [
        (
            "crates/util/fixture/src/lib.rs",
            "use bevy_app::{App,Plugin}; use bevy_ecs::prelude::*; mod tools; fn read(rows:Query<&tools::bin::worker::state::State>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } }",
        ),
        ("crates/util/fixture/src/tools.rs", "pub mod bin;"),
        ("crates/util/fixture/src/tools/bin.rs", "pub mod worker;"),
        (
            "crates/util/fixture/src/tools/bin/worker.rs",
            "pub mod state;",
        ),
        (
            "crates/util/fixture/src/tools/bin/worker/state.rs",
            "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
        ),
    ] {
        inventory.source(path, source);
    }
    inventory.resolve_usage();
    assert!(inventory.canonical(
        &inventory.entries[&inventory.key("struct:State")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn outlined_binary_children_use_their_resolved_logical_parent() {
    for (path, expected) in [
        ("crates/util/fixture/src/bin/alpha/state.rs", true),
        ("crates/util/fixture/src/bin/state.rs", false),
    ] {
        for reverse in [false, true] {
            let mut sources = [
                ("crates/util/fixture/src/bin/alpha.rs", "pub mod state;"),
                (
                    "crates/util/fixture/src/bin/zeta.rs",
                    "use bevy_app::App; use bevy_ecs::prelude::*; mod alpha; fn read(rows:Query<&alpha::state::State>) {} fn main() { let mut app=App::new(); app.add_systems(Update,read); }",
                ),
                (
                    path,
                    "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
                ),
            ];
            if reverse {
                sources.reverse();
            }
            let mut inventory = Inventory::default();
            for (path, source) in sources {
                inventory.source(path, source);
            }
            inventory.resolve_usage();
            assert_eq!(
                inventory.canonical(
                    &inventory.entries[&inventory.key("struct:State")],
                    &Policy::default(),
                    &mut BTreeSet::new()
                ),
                expected,
                "{path}, reverse={reverse}"
            );
        }
    }
}

#[test]
fn query_data_is_owned_only_through_registered_queries() {
    for (registered, suffix) in [(true, "struct:View"), (false, "struct:Unused")] {
        let source = if registered {
            "use bevy_ecs::query::QueryData; #[derive(Component)] struct Position { x:f32 } #[derive(QueryData)] struct View { position:&'static Position } fn read(rows:Query<View>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } }"
        } else {
            "use bevy_ecs::query::QueryData; #[derive(QueryData)] struct Unused { value:&'static String }"
        };
        let inventory = Inventory::fixture(source);
        let key = inventory.key(suffix);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            registered
        );
    }
}

#[test]
fn a_binary_import_cannot_replace_its_library_export() {
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/util/fixture/src/lib.rs",
        "mod settings; mod engine; pub use settings::Settings;",
    );
    inventory.source(
        "crates/util/fixture/src/settings.rs",
        "#[derive(Component)] pub struct Settings { value:u32 }",
    );
    inventory.source("crates/util/fixture/src/engine.rs", "use crate::Settings; fn read(state:Query<&Settings>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } }");
    inventory.source(
        "crates/util/fixture/src/main.rs",
        "use fixture::Settings; struct Unowned { value:String } fn main() {}",
    );
    inventory.resolve_usage();
    let key = inventory.key("struct:Settings");
    assert!(inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let key = inventory.key("struct:Unowned");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn binary_module_ownership_requires_the_declared_source_and_one_crate_context() {
    for (source, shared, accepted) in [
        ("state.rs", false, true),
        ("different.rs", false, false),
        ("state.rs", true, false),
    ] {
        let mut inventory = Inventory::default();
        if shared {
            inventory.source(
                "crates/util/fixture/src/lib.rs",
                "mod state; pub use state::State;",
            );
        }
        inventory.source("crates/util/fixture/src/main.rs", "use bevy_app::App; use bevy_ecs::prelude::*; mod state; fn read(rows:Query<&state::State>) {} fn main() { let mut app=App::new(); app.add_systems(Update,read); }");
        inventory.source(
            &format!("crates/util/fixture/src/{source}"),
            "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
        );
        inventory.resolve_usage();
        let key = inventory.key("struct:State");
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            accepted,
            "{source}, shared={shared}"
        );
    }
}

#[test]
fn ordinary_main_and_lib_modules_resolve_children_under_their_stem() {
    for module in ["main", "lib"] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/util/fixture/src/lib.rs",
            &format!("mod tools; fn read(rows:Query<&tools::{module}::state::State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,read); }} }}"),
        );
        inventory.source(
            "crates/util/fixture/src/tools.rs",
            &format!("pub mod {module};"),
        );
        inventory.source(
            &format!("crates/util/fixture/src/tools/{module}.rs"),
            "pub mod state;",
        );
        inventory.source(
            &format!("crates/util/fixture/src/tools/{module}/state.rs"),
            "#[derive(Component)] pub struct State { value:u32 }",
        );
        inventory.resolve_usage();
        let key = inventory.key("struct:State");
        assert!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            "ordinary {module}.rs"
        );
    }
}

#[test]
fn ambiguous_module_files_cannot_prove_ownership_through_a_unique_symbol() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/fixture/src/main.rs", "use bevy_app::App; use bevy_ecs::prelude::*; mod state; fn read(rows:Query<&state::State>) {} fn main() { let mut app=App::new(); app.add_systems(Update,read); }");
    inventory.source(
        "crates/util/fixture/src/state.rs",
        "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
    );
    inventory.source("crates/util/fixture/src/state/mod.rs", "pub struct Other;");
    inventory.resolve_usage();
    let key = inventory.key("struct:State");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn module_resolution_preserves_named_binary_roots_and_raw_inline_names() {
    for raw in [false, true] {
        let mut inventory = Inventory::default();
        if raw {
            inventory.source("crates/util/fixture/src/main.rs", "use bevy_app::App; use bevy_ecs::prelude::*; mod r#async { pub mod state; } fn read(rows:Query<&r#async::state::State>) {} fn main() { let mut app=App::new(); app.add_systems(Update,read); }");
            inventory.source(
                "crates/util/fixture/src/async/state.rs",
                "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
            );
        } else {
            inventory.source("crates/util/fixture/src/bin/example.rs", "use bevy_app::App; use bevy_ecs::prelude::*; #[derive(Component)] struct State { value:u32 } fn read(rows:Query<&crate::State>) {} fn main() { let mut app=App::new(); app.add_systems(Update,read); }");
        }
        inventory.resolve_usage();
        let key = inventory.key("struct:State");
        assert!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            "raw={raw}"
        );
    }
}

#[test]
fn module_barrel_reexports_can_revisit_a_completed_prefix_without_a_false_cycle() {
    let mut inventory = Inventory::default();
    inventory.source("crates/util/fixture/src/lib.rs", "use bevy_app::{App,Plugin}; use bevy_ecs::prelude::*; mod host; pub use host::{stack,active_pane}; fn read(rows:Query<&stack::State>) {} struct Root; impl Plugin for Root { fn build(&self,app:&mut App) { app.add_systems(Update,read); } }");
    inventory.source(
        "crates/util/fixture/src/host.rs",
        "pub mod stack; pub mod active_pane;",
    );
    inventory.source(
        "crates/util/fixture/src/host/stack.rs",
        "pub use crate::active_pane::State;",
    );
    inventory.source(
        "crates/util/fixture/src/host/active_pane.rs",
        "use bevy_ecs::prelude::*; #[derive(Component)] pub struct State { value:u32 }",
    );
    inventory.resolve_usage();
    let key = inventory.key("struct:State");
    assert!(inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn adapter_owner_values_require_a_used_canonical_root_and_real_field_edges() {
    for (field, registered, accepted) in [
        ("page:Page", true, true),
        ("value:u32", true, false),
        ("page:Page", false, false),
    ] {
        let registration = if registered {
            "app.add_systems(Update, read);"
        } else {
            ""
        };
        let inventory = Inventory::fixture(&format!(
            "#[derive(Component)] struct Root {{ {field} }} struct Page {{ value:u32 }} impl Page {{ fn render(&self) {{ let _codec=Codec; }} }} struct Codec; fn read(rows:Query<&Root>) {{}} struct PluginRoot; impl Plugin for PluginRoot {{ fn build(&self,app:&mut App) {{ {registration} }} }}"
        ));
        let mut boundary = Boundary::fixture(&inventory, "struct:Codec");
        boundary.owner = Some(inventory.key("struct:Page"));
        boundary.input = format!("ECS: {}", inventory.key("struct:Root"));
        boundary.output = "external: scoped native codec output".into();
        let mut policy = Policy::default();
        policy.values.push(ValueOwner {
            key: inventory.key("struct:Page"),
            owner: inventory.key("struct:Root"),
            lifetime: "immutable page metadata retained by the root entity".into(),
        });
        policy.boundaries.push(boundary.clone());
        assert_eq!(
            boundary.references_from(&inventory, &policy, &mut BTreeSet::new()),
            accepted,
            "{field}, registered={registered}"
        );
    }
}

#[test]
fn self_qualified_contracts_belong_to_the_contract_crate() {
    for (path, accepted) in [
        ("crates/util/vmux_api/src/lib.rs", true),
        ("crates/feature/sample/src/lib.rs", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            path,
            "#[crate::contract] pub struct Payload { value:String }",
        );
        inventory.resolve_usage();
        let key = inventory.key("struct:Payload");
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&key],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            accepted
        );
    }
}

#[test]
fn renderer_ownership_follows_component_nodes_and_ignores_labels() {
    for (body, referenced) in [
        ("rsx! { Child {} }", true),
        ("rsx! { div { \"Child\" } }", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[component] fn Child() -> Element {{ rsx! {{ div {{}} }} }} #[component] fn Parent() -> Element {{ {body} }}"
        ));
        let parent = inventory.key("fn:Parent");
        let child = inventory.key("fn:Child");
        assert_eq!(
            inventory.owner_references(&inventory.entries[&parent], &inventory.entries[&child]),
            referenced
        );
    }
}

#[test]
fn nested_adapter_owners_retain_the_root_contract() {
    let inventory = Inventory::fixture(
        "#[vmux_api::contract] struct View { value:u32 } struct Root { view:View, middle:Middle } struct Middle { leaf:Leaf } struct Leaf;",
    );
    let mut policy = Policy::default();
    for (name, owner) in [
        ("struct:Root", None),
        ("struct:Middle", Some("struct:Root")),
        ("struct:Leaf", Some("struct:Middle")),
    ] {
        let mut boundary = Boundary::fixture(&inventory, name);
        boundary.input = format!("ECS: {}", inventory.key("struct:View"));
        boundary.output = "external: bounded rendering".into();
        boundary.owner = owner.map(|owner| inventory.key(owner));
        policy.boundaries.push(boundary);
    }
    let leaf = policy.boundaries.last().unwrap();
    assert!(leaf.references_from(&inventory, &policy, &mut BTreeSet::new()));
    policy.boundaries[1].owner = Some(inventory.key("struct:Leaf"));
    assert!(!policy.boundaries.last().unwrap().references_from(
        &inventory,
        &policy,
        &mut BTreeSet::new()
    ));
}

#[test]
fn embedded_script_ownership_requires_an_actual_include() {
    for (body, included) in [("include_str!(\"boot.sh\")", true), ("\"boot.sh\"", false)] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/util/fixture/src/adapter.rs",
            &format!("struct Adapter; impl Adapter {{ fn script() -> &'static str {{ {body} }} }}"),
        );
        inventory.script("crates/util/fixture/src/boot.sh", "exit 0");
        let owner = inventory.key("struct:Adapter");
        let script = inventory.key("boot.sh||script");
        assert_eq!(
            inventory.owner_references(&inventory.entries[&owner], &inventory.entries[&script]),
            included
        );
    }
}

#[test]
fn codec_boundaries_use_their_declared_canonical_endpoint() {
    let inventory = Inventory::fixture(
        "#[vmux_api::ui_event] struct Received { bytes: Vec<u8> } #[vmux_api::ui_state] struct Theme { dark: bool } struct Codec { bytes: Vec<u8> } impl Codec { fn decode(self) -> Received { Received { bytes: self.bytes } } }",
    );
    let mut policy = Policy::default();
    for key in ["struct:Codec", "impl:Codec"] {
        let mut boundary = Boundary::fixture(&inventory, key);
        boundary.interface = "network: external byte frame codec".into();
        boundary.input = "external: framed bytes".into();
        boundary.output = format!("ECS: {}", inventory.key("struct:Received"));
        policy.boundaries.push(boundary);
    }
    assert!(policy.audit(&inventory).is_empty());
    for boundary in &mut policy.boundaries {
        boundary.output = format!("ECS: {}", inventory.key("struct:Theme"));
    }
    assert!(!policy.audit(&inventory).is_empty());
}

#[test]
fn trait_adapter_contracts_follow_the_same_owner_and_module() {
    let inventory = Inventory::fixture(
        "#[vmux_api::ui_event] struct Received { bytes: Vec<u8> } struct Codec { bytes: Vec<u8> } impl Codec { fn decode(self) -> Received { Received { bytes:self.bytes } } } impl Drop for Codec { fn drop(&mut self) {} }",
    );
    let mut boundary = Boundary::fixture(&inventory, "impl:Drop for Codec");
    boundary.input = "external: owned byte buffer".into();
    boundary.output = format!("ECS: {}", inventory.key("struct:Received"));
    assert!(boundary.references(&inventory));
    let unrelated = Inventory::fixture(
        "#[vmux_api::ui_event] struct Received { bytes:Vec<u8> } mod one { struct Codec; impl Drop for Codec { fn drop(&mut self) {} } } mod two { struct Codec; impl Codec { fn decode(self) -> super::Received { super::Received { bytes:vec![] } } } }",
    );
    boundary.key = unrelated.key("one|impl:Drop for Codec");
    boundary.fingerprint = unrelated.entries[&boundary.key].fingerprint.clone();
    boundary.output = format!("ECS: {}", unrelated.key("struct:Received"));
    assert!(!boundary.references(&unrelated));
}

#[test]
fn qualified_impl_owners_cannot_borrow_another_types_boundary_endpoint() {
    for source in [
        "#[vmux_api::ui_event] struct Received { bytes:Vec<u8> } mod one { pub struct Codec; } mod two { pub struct Codec; } impl Drop for one::Codec { fn drop(&mut self) {} } impl two::Codec { fn decode(self) -> Received { Received { bytes:vec![] } } }",
        "#[vmux_api::ui_event] struct Received { bytes:Vec<u8> } mod one { pub struct Codec; } mod two { pub struct Codec; } use two::Codec as Other; impl Drop for one::Codec { fn drop(&mut self) {} } impl Other { fn decode(self) -> Received { Received { bytes:vec![] } } }",
    ] {
        let inventory = Inventory::fixture(source);
        let mut boundary = Boundary::fixture(&inventory, "impl:Drop for one :: Codec");
        boundary.input = "external: byte buffer".into();
        boundary.output = format!("ECS: {}", inventory.key("struct:Received"));
        assert!(!boundary.references(&inventory));
    }
    let inventory = Inventory::fixture(
        "#[vmux_api::ui_event] struct Received { bytes:Vec<u8> } mod one { pub struct Codec; } use one::Codec as Same; impl Drop for one::Codec { fn drop(&mut self) {} } impl Same { fn decode(self) -> Received { Received { bytes:vec![] } } }",
    );
    let mut boundary = Boundary::fixture(&inventory, "impl:Drop for one :: Codec");
    boundary.input = "external: byte buffer".into();
    boundary.output = format!("ECS: {}", inventory.key("struct:Received"));
    assert!(boundary.references(&inventory));
}

#[test]
fn path_overridden_module_exports_cannot_establish_standard_type_bindings() {
    for attribute in [
        "#[path=\"implementation.rs\"]",
        "#[cfg_attr(target_os=\"linux\",path=\"implementation.rs\")]",
        "#[cfg_attr(feature=\"host\",cfg_attr(target_os=\"linux\",path=\"implementation.rs\"))]",
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/sample/src/lib.rs", &format!("{attribute} mod adapter; use adapter::*; #[derive(Message)] struct Completed {{ result: std::result::Result<u64,String> }} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_message::<Completed>(); }} }}"));
        inventory.source("crates/sample/src/adapter.rs", "");
        inventory.source(
            "crates/sample/src/implementation.rs",
            "pub mod std { pub mod result { pub struct Result<T,E> { value:T,error:E } } }",
        );
        inventory.resolve_usage();
        let entry = &inventory.entries[&inventory.key("struct:Completed")];
        assert!(!inventory.canonical(entry, &Policy::default(), &mut BTreeSet::new()));
    }
}

#[test]
fn module_directory_exports_keep_their_logical_namespace() {
    let mut inventory = Inventory::default();
    inventory.source("crates/sample/src/lib.rs", "mod adapter; use adapter::*; #[derive(Message)] struct Completed { result: std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }");
    inventory.source(
        "crates/sample/src/adapter/mod.rs",
        "pub mod std { pub mod result { pub struct Result<T,E> { value:T,error:E } } }",
    );
    inventory.resolve_usage();
    let entry = &inventory.entries[&inventory.key("struct:Completed")];
    assert!(!inventory.canonical(entry, &Policy::default(), &mut BTreeSet::new()));
}

#[test]
fn inline_module_path_overrides_block_unproven_descendant_exports() {
    let mut inventory = Inventory::default();
    inventory.source("crates/sample/src/lib.rs", "#[path=\"alternate\"] mod facade { mod inner; pub use inner::*; } use facade::*; #[derive(Message)] struct Completed { result:std::result::Result<u64,String> } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Completed>(); } }");
    inventory.source(
        "crates/sample/src/alternate/inner.rs",
        "pub mod std { pub mod result { pub struct Result<T,E> { value:T,error:E } } }",
    );
    inventory.resolve_usage();
    let entry = &inventory.entries[&inventory.key("struct:Completed")];
    assert!(!inventory.canonical(entry, &Policy::default(), &mut BTreeSet::new()));
}

#[test]
fn ui_projection_inputs_require_a_rendering_or_transport_boundary() {
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/util/vmux_ecs/src/ui_state.rs",
        "#[derive(Component)] pub struct UiState<T>(T);",
    );
    inventory.source("fixture.rs", "use vmux_ecs::ui_state::UiState; #[vmux_api::ui_state] struct View { allowed: bool } fn authorize(state: Query<&UiState<View>>) -> bool { true } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, authorize); } }");
    inventory.resolve_usage();
    let key = inventory.key("fn:authorize");
    assert!(!inventory.canonical(
        &inventory.entries[&key],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn test_only_expressions_and_nested_items_do_not_prove_production_ownership() {
    for attribute in ["#[cfg(test)]", "#[cfg_attr(not(test), cfg(test))]"] {
        let source = format!(
            "#[derive(Message)] struct Received; struct Child; impl Plugin for Child {{ fn build(&self, app: &mut App) {{ app.add_message::<Received>(); }} }} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ {attribute} app.add_plugins(Child); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let mut boundary = Boundary::fixture(&inventory, "struct:Root");
        boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
        boundary.output = "external: installed production plugin graph".into();
        assert!(!boundary.references(&inventory));
        let source = format!(
            "enum Mode {{ Active }} #[derive(Component)] struct State; impl State {{ fn production(&self) {{ {attribute} fn accept(mode: Mode) {{}} }} }} fn read(states: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let mut policy = Policy::default();
        policy.values.push(ValueOwner {
            key: inventory.key("enum:Mode"),
            owner: inventory.key("struct:State"),
            lifetime: "one production state operation".into(),
        });
        assert!(!inventory.canonical(
            &inventory.entries[&policy.values[0].key],
            &policy,
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn owned_constants_resolve_their_qualified_declarations() {
    let inventory = Inventory::fixture(
        "mod one { pub const LIMIT: usize = 1; } mod two { pub const LIMIT: usize = 2; } #[derive(Component)] struct State; impl State { fn limit(&self) -> usize { one::LIMIT } } fn read(states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    let mut policy = Policy::default();
    policy.values.push(ValueOwner {
        key: inventory.key("one|const:LIMIT"),
        owner: inventory.key("struct:State"),
        lifetime: "one canonical capability lookup".into(),
    });
    assert!(inventory.canonical(
        &inventory.entries[&policy.values[0].key],
        &policy,
        &mut BTreeSet::new()
    ));
    policy.values[0].key = inventory.key("two|const:LIMIT");
    assert!(!inventory.canonical(
        &inventory.entries[&policy.values[0].key],
        &policy,
        &mut BTreeSet::new()
    ));
}

#[test]
fn plugin_values_exclude_attributed_tuple_members_and_follow_only_struct_constructors() {
    for (value, expected) in [
        ("(#[cfg(test)] Child, Empty)", false),
        ("(#[cfg_attr(not(test),cfg(test))] Child, Empty)", false),
        ("Child { marker: PhantomData::<Empty> }", true),
        ("Empty { marker: PhantomData::<Child> }", false),
    ] {
        let source = format!(
            "#[derive(Message)] struct Received; struct Child; impl Plugin for Child {{ fn build(&self, app: &mut App) {{ app.add_message::<Received>(); }} }} struct Empty; impl Plugin for Empty {{ fn build(&self, app: &mut App) {{}} }} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_plugins({value}); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let mut boundary = Boundary::fixture(&inventory, "struct:Root");
        boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
        boundary.output = "external: installed production graph".into();
        assert_eq!(boundary.references(&inventory), expected, "{value}");
    }
}

#[test]
fn imported_system_aliases_retain_exclusive_constructor_usage() {
    let inventory = Inventory::fixture(
        "use bevy_ecs::system::SystemState; use self::read as run; #[derive(Component)] struct State; #[derive(SystemParam)] struct Params { states: Query<&State> } fn read(world: &mut World) { let mut state = SystemState::<Params>::new(world); state.get_mut(world); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, run); } }",
    );
    assert!(inventory.canonical(
        &inventory.entries[&inventory.key("struct:State")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let inventory = Inventory::fixture(
        "use bevy_ecs::system::SystemState; #[derive(Component)] struct State; #[derive(SystemParam)] struct Params { states: Query<&State> } fn read(world: &mut World) { let mut state = SystemState::<Params>::new(world); state.get_mut(world); }",
    );
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("struct:State")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn possible_test_attributes_do_not_exempt_production_functions() {
    let inventory =
        Inventory::fixture("#[cfg_attr(feature = \"debug\", test)] fn unregistered() {}");
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("fn:unregistered")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let inventory = Inventory::fixture("#[test] fn unit_test() {}");
    assert!(inventory.canonical(
        &inventory.entries[&inventory.key("fn:unit_test")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn test_only_local_initializers_and_match_arms_do_not_compose_plugins() {
    for attribute in ["#[cfg(test)]", "#[cfg_attr(not(test), cfg(test))]"] {
        for body in [
            format!("{attribute} let _ = app.add_plugins(Child);"),
            format!("match 1 {{ {attribute} 0 => app.add_plugins(Child), _ => app }};"),
        ] {
            let source = format!(
                "#[derive(Message)] struct Received; struct Child; impl Plugin for Child {{ fn build(&self, app: &mut App) {{ app.add_message::<Received>(); }} }} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ {body} }} }}"
            );
            let inventory = Inventory::fixture(&source);
            let mut boundary = Boundary::fixture(&inventory, "struct:Root");
            boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
            boundary.output = "external: installed production plugin graph".into();
            assert!(!boundary.references(&inventory));
        }
    }
}

#[test]
fn test_only_implementations_cannot_establish_production_owner_references() {
    for attribute in ["#[cfg(test)]", "#[cfg_attr(not(test), cfg(test))]"] {
        let source = format!(
            "#[derive(Message)] struct Received; struct Child; impl Plugin for Child {{ fn build(&self, app: &mut App) {{ app.add_message::<Received>(); }} }} struct Root; {attribute} impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_plugins(Child); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let mut boundary = Boundary::fixture(&inventory, "struct:Root");
        boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
        boundary.output = "external: installed production plugin graph".into();
        assert!(!boundary.references(&inventory), "{attribute}");
        let source = format!(
            "enum Mode {{ Active }} #[derive(Component)] struct State; {attribute} impl State {{ fn accept(&self, mode: Mode) {{}} }} fn read(states: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let mut policy = Policy::default();
        policy.values.push(ValueOwner {
            key: inventory.key("enum:Mode"),
            owner: inventory.key("struct:State"),
            lifetime: "one production state operation".into(),
        });
        assert!(
            !inventory.canonical(
                &inventory.entries[&policy.values[0].key],
                &policy,
                &mut BTreeSet::new()
            ),
            "{attribute}"
        );
    }
    let inventory = Inventory::fixture(
        "enum Mode { Active } #[derive(Component)] struct State; #[cfg(test)] mod helpers { impl super::State { fn accept(&self, mode: super::Mode) {} } } fn read(states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    let mut policy = Policy::default();
    policy.values.push(ValueOwner {
        key: inventory.key("enum:Mode"),
        owner: inventory.key("struct:State"),
        lifetime: "one production state operation".into(),
    });
    assert!(!inventory.canonical(
        &inventory.entries[&policy.values[0].key],
        &policy,
        &mut BTreeSet::new()
    ));
}

#[test]
fn generic_plugin_arguments_do_not_install_their_phantom_parameters() {
    let inventory = Inventory::fixture(
        "#[derive(Message)] struct Received; struct Child; impl Plugin for Child { fn build(&self, app: &mut App) { app.add_message::<Received>(); } } struct Wrapper<T>(PhantomData<T>); impl<T> Plugin for Wrapper<T> { fn build(&self, app: &mut App) {} } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_plugins(Wrapper::<Child>(PhantomData)); } }",
    );
    let mut boundary = Boundary::fixture(&inventory, "struct:Root");
    boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
    boundary.output = "external: installed production plugin graph".into();
    assert!(!boundary.references(&inventory));
}

#[test]
fn contract_macros_resolve_imports_after_all_local_shadows_are_known() {
    for source in [
        "mod vmux_macro { pub use other_macros::string_id; } use vmux_macro::string_id; #[string_id] struct Id { registry: Vec<String> }",
        "use vmux_macro::string_id; #[string_id] struct Id { registry: Vec<String> } mod vmux_macro { pub use other_macros::string_id; }",
        "#[vmux_macro::string_id] struct Id { registry: Vec<String> } mod vmux_macro { pub use other_macros::string_id; }",
    ] {
        let inventory = Inventory::fixture(source);
        assert!(!inventory.canonical(
            &inventory.entries[&inventory.key("struct:Id")],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
    let inventory = Inventory::fixture(
        "use vmux_macro::string_id as native_id; #[native_id] struct Id(String);",
    );
    assert!(inventory.canonical(
        &inventory.entries[&inventory.key("struct:Id")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn service_message_attributes_prove_only_registered_messages_with_owned_fields() {
    for (source, expected) in [
        (
            "#[vmux_api::service_message(Ready)] struct Received { value: u64 } fn receive(messages: MessageReader<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_service_message::<Received>().add_systems(Update, receive); } }",
            true,
        ),
        (
            "use vmux_api::service_message as incoming; #[incoming(Ready)] struct Received { value: u64 } fn receive(messages: MessageReader<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, receive); } }",
            true,
        ),
        (
            "#[vmux_api::service_message(Ready)] struct Received { value: u64 }",
            false,
        ),
        (
            "struct Domain { rows: Vec<String> } #[vmux_api::service_message(Ready)] struct Received { value: Domain } fn receive(messages: MessageReader<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, receive); } }",
            false,
        ),
        (
            "mod vmux_api { pub use unrelated::service_message; } #[vmux_api::service_message(Ready)] struct Received { value: u64 } fn receive(messages: MessageReader<Received>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, receive); } }",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(source);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("struct:Received")],
                &Policy::default(),
                &mut BTreeSet::new(),
            ),
            expected,
            "{source}",
        );
    }
}

#[test]
fn query_aliases_borrow_exact_frozen_components_without_approving_plain_models() {
    for (target, expected) in [("LegacyState", true), ("Model", false), ("Copy", false)] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_ecs::prelude::*; struct Model {{ value: String }} #[derive(Component)] struct LegacyState {{ model: Model }} #[derive(Component)] struct Copy {{ model: Model }} type Borrowed<'w,'s> = Query<'w,'s,&'static {target}>; fn read(state: Borrowed) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        ));
        let mut policy = Policy {
            revision: FROZEN_REVISION.into(),
            ..Policy::default()
        };
        for name in ["struct:Model", "struct:LegacyState"] {
            let key = inventory.key(name);
            policy.legacy.push(Legacy {
                fingerprint: inventory.entries[&key].fingerprint.clone(),
                key,
                issue: "VMX-253".into(),
            });
        }
        let read = &inventory.entries[&inventory.key("fn:read")];
        assert_eq!(
            inventory.canonical(read, &policy, &mut BTreeSet::new()),
            expected,
            "{target}"
        );
        policy.legacy[1].fingerprint = "0000000000000000".into();
        assert!(!inventory.canonical(read, &policy, &mut BTreeSet::new()));
    }
}

#[test]
fn timezone_values_do_not_masquerade_as_local_system_state() {
    let inventory = Inventory::fixture(
        "#[derive(Component)] struct State; impl State { fn timestamp(&self) { let timezone = &chrono::Local; } } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    assert!(inventory.canonical(
        &inventory.entries[&inventory.key("impl:State")],
        &Policy::default(),
        &mut BTreeSet::new(),
    ));
    let local = Inventory::fixture(
        "fn read(state: Local<u64>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    assert!(!local.canonical(
        &local.entries[&local.key("fn:read")],
        &Policy::default(),
        &mut BTreeSet::new(),
    ));
}

#[test]
fn system_state_constructors_require_the_bevy_binding() {
    for (source, expected) in [
        (
            "use bevy_ecs::system::SystemState as Capture; #[derive(Component)] struct State; #[derive(SystemParam)] struct Params { state: Query<&State> } fn read(world: &mut World) { let mut state = Capture::<Params>::new(world); state.get_mut(world); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            true,
        ),
        (
            "struct SystemState<T>(PhantomData<T>); #[derive(Component)] struct State; #[derive(SystemParam)] struct Params { state: Query<&State> } fn read(world: &mut World) { let mut state = SystemState::<Params>::new(world); state.get_mut(world); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(source);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("struct:State")],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected
        );
    }
}

#[test]
fn cached_exclusive_system_parameters_require_registered_bevy_ownership() {
    for (binding, registration, expected) in [
        (
            "use bevy_ecs::system::SystemState;",
            "app.add_systems(Update, read);",
            true,
        ),
        (
            "use bevy_ecs::system::SystemState as Cached;",
            "app.add_systems(Update, read);",
            true,
        ),
        (
            "struct SystemState<T>(PhantomData<T>);",
            "app.add_systems(Update, read);",
            false,
        ),
        ("use bevy_ecs::system::SystemState;", "", false),
    ] {
        let parameter = if binding.contains("as Cached") {
            "Cached"
        } else {
            "SystemState"
        };
        let inventory = Inventory::fixture(&format!(
            "use bevy_ecs::prelude::*; {binding} #[derive(Component)] struct State; #[derive(Resource)] struct Settings; fn read(world: &mut World, cached: &mut {parameter}<(Query<&State>, Res<Settings>)>) {{ let _ = cached.get(world); }} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ {registration} }} }}"
        ));
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("fn:read")],
                &Policy::default(),
                &mut BTreeSet::new(),
            ),
            expected,
            "{binding} {registration}"
        );
    }
}

#[test]
fn production_configuration_keeps_unknown_platform_and_feature_branches() {
    for (condition, expected) in [
        ("not(feature = \"host\")", true),
        ("not(target_os = \"ios\")", true),
        ("any(test, feature = \"host\")", true),
        ("all(test, feature = \"host\")", false),
        ("not(not(test))", false),
        ("any()", false),
        ("all()", true),
    ] {
        let source = format!(
            "#[derive(Component)] struct State; #[cfg({condition})] fn read(state: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("struct:State")],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected,
            "{condition}"
        );
    }
}

#[test]
fn opaque_owned_modules_cannot_be_reclassified_as_foreign_types() {
    let mut inventory = Inventory::default();
    inventory.source("crates/sample/src/lib.rs", "#[path=\"elsewhere.rs\"] mod adapter; mod unrelated { struct Commands; } #[derive(Component)] struct State { commands: adapter::Commands } fn read(states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }");
    inventory.source(
        "crates/sample/src/elsewhere.rs",
        "pub struct Commands { registry: Vec<String> }",
    );
    inventory.resolve_usage();
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("struct:State")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn foreign_framework_names_do_not_borrow_unrelated_local_declarations() {
    for (source, expected) in [
        (
            "use bevy_ecs::prelude::*; mod other { pub struct Commands { registry: Vec<String> } } #[derive(Component)] struct State; fn read(commands: Commands, states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            true,
        ),
        (
            "use bevy_ecs::prelude::*; struct Commands { registry: Vec<String> } #[derive(Component)] struct State; fn read(commands: Commands, states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            false,
        ),
        (
            "use bevy_ecs::prelude::*; use crate::one::*; use crate::two::*; mod one { pub struct Commands { registry: Vec<String> } } mod two { pub struct Commands { registry: Vec<String> } } #[derive(Component)] struct State; fn read(commands: Commands, states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(source);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("fn:read")],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected
        );
    }
}

#[test]
fn component_constructors_and_schedule_sets_require_production_registration() {
    let inventory = Inventory::fixture(
        "#[derive(Component)] struct State; #[derive(SystemSet)] struct Ready; fn start(mut commands: Commands) { commands.spawn(State::new()); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.configure_sets(Update, Ready); app.add_systems(Update, start); } }",
    );
    for name in ["struct:State", "struct:Ready"] {
        assert!(inventory.canonical(
            &inventory.entries[&inventory.key(name)],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
    let inventory = Inventory::fixture(
        "#[derive(Component)] struct State; #[derive(SystemSet)] struct Ready; fn start(mut commands: Commands) { commands.spawn(State::new()); } fn install(app: &mut App) { app.configure_sets(Update, Ready); app.add_systems(Update, start); }",
    );
    for name in ["struct:State", "struct:Ready"] {
        assert!(!inventory.canonical(
            &inventory.entries[&inventory.key(name)],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn adapter_helpers_require_a_current_connected_parent_boundary() {
    let inventory = Inventory::fixture(
        "#[derive(Message)] struct Received; struct Decoder; impl Decoder { fn decode(&self) -> Received { Frame::read(); Received } } struct Frame; impl Frame { fn read() { Decoder::decode(); } } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Received>(); } }",
    );
    let mut parent = Boundary::fixture(&inventory, "struct:Decoder");
    parent.input = "external: bounded native byte stream".into();
    parent.output = format!("ECS: {}", inventory.key("struct:Received"));
    let mut helper = Boundary::fixture(&inventory, "struct:Frame");
    helper.input = parent.input.clone();
    helper.output = parent.output.clone();
    helper.owner = Some(parent.key.clone());
    let mut policy = Policy::default();
    assert!(!helper.references_from(&inventory, &policy, &mut BTreeSet::new()));
    policy.boundaries.push(parent.clone());
    assert!(helper.references_from(&inventory, &policy, &mut BTreeSet::new()));
    policy.boundaries[0].fingerprint = "stale".into();
    assert!(!helper.references_from(&inventory, &policy, &mut BTreeSet::new()));
    policy.boundaries[0] = parent;
    policy.boundaries[0].owner = Some(helper.key.clone());
    policy.boundaries.push(helper.clone());
    assert!(!helper.references_from(&inventory, &policy, &mut BTreeSet::new()));
}

#[test]
fn plugin_boundaries_follow_only_production_composition_to_canonical_endpoints() {
    for (body, expected) in [
        ("app.add_plugins(Child);", true),
        ("app.add_plugins(Child::default());", true),
        ("let child = PhantomData::<Child>;", false),
        ("#[cfg(test)] { app.add_plugins(Child); }", false),
    ] {
        let source = format!(
            "#[derive(Message)] struct Received; struct Child; impl Plugin for Child {{ fn build(&self, app: &mut App) {{ app.add_message::<Received>(); }} }} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ {body} }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let mut boundary = Boundary::fixture(&inventory, "struct:Root");
        boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
        boundary.output = "external: installed production plugin graph".into();
        assert_eq!(boundary.references(&inventory), expected);
    }
    let inventory = Inventory::fixture(
        "#[derive(Message)] struct Received; struct First; impl Plugin for First { fn build(&self, app: &mut App) { app.add_plugins(Second); } } struct Second; impl Plugin for Second { fn build(&self, app: &mut App) { app.add_plugins(First); } }",
    );
    let mut boundary = Boundary::fixture(&inventory, "struct:First");
    boundary.input = format!("ECS: {}", inventory.key("struct:Received"));
    boundary.output = "external: installed production plugin graph".into();
    assert!(!boundary.references(&inventory));
}

#[test]
fn boundary_inputs_follow_only_their_plugins_actual_production_registrations() {
    let prefix = "use bevy_ecs::prelude::*; use bevy_app::{App, Plugin}; #[derive(Component)] struct State; fn render(states: Query<&State>) {} struct Root;";
    for (body, expected) in [
        ("app.add_systems(Update, render);", true),
        ("app.add_systems(Update, (render,).chain());", true),
        ("let unused = render;", false),
        ("app.add_systems(Update, other.after(render));", false),
        ("app.add_systems(Update, other.run_if(render));", false),
        ("#[cfg(test)] app.add_systems(Update, render);", false),
        ("#[cfg(any())] app.add_systems(Update, render);", false),
        (
            "let unused = || { app.add_systems(Update, render); };",
            false,
        ),
        (
            "let unused = async { app.add_systems(Update, render); };",
            false,
        ),
        ("let unused = install;", false),
        ("if false { app.add_systems(Update, render); }", false),
        (
            "let mut fake = FakeApp; fake.add_systems(Update, render);",
            false,
        ),
    ] {
        let source = format!(
            "{prefix} fn other() {{}} fn install(app: &mut App) {{ app.add_systems(Update, render); }} struct Other; impl Plugin for Other {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, render); }} }} struct FakeApp; impl FakeApp {{ fn add_systems<T, S>(&mut self, schedule: T, system: S) {{}} }} impl Plugin for Root {{ fn build(&self, app: &mut App) {{ {body} }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let key = inventory.key("struct:Root");
        let mut boundary = Boundary::fixture(&inventory, &key);
        boundary.input = format!("ECS: {}", inventory.key("struct:State"));
        boundary.output = "external: registered renderer".into();
        assert_eq!(boundary.references(&inventory), expected, "{body}");
    }
}

#[test]
fn boundary_inputs_reach_typed_queries_without_traversing_owned_models() {
    let prefix = "use bevy_ecs::prelude::*; use bevy_app::{App, Plugin}; #[derive(Component)] struct State; #[derive(Component)] struct OtherState;";
    for (definition, expected) in [
        (
            "type Rows<'w, 's> = Query<'w, 's, &'static State>; #[derive(SystemParam)] struct Inner<'w, 's> { rows: Rows<'w, 's> } #[derive(SystemParam)] struct Params<'w, 's> { inner: Inner<'w, 's> }",
            true,
        ),
        (
            "#[derive(SystemParam)] struct Params<'w, 's> { rows: Query<'w, 's, Entity, With<State>> }",
            true,
        ),
        (
            "#[derive(SystemParam)] struct Params<'w, 's> { rows: Query<'w, 's, Ref<'static, State>> }",
            true,
        ),
        (
            "#[derive(Resource)] struct Holder { owned: State } #[derive(SystemParam)] struct Params<'w> { holder: Res<'w, Holder> }",
            false,
        ),
        (
            "#[derive(SystemParam)] struct Params<'s> { owned: Local<'s, State> }",
            false,
        ),
        (
            "mod fake { pub struct Query<T>(std::marker::PhantomData<T>); } type Rows = fake::Query<State>; #[derive(SystemParam)] struct Params { rows: Rows }",
            false,
        ),
        (
            "struct Plain { owned: State } #[derive(SystemParam)] struct Params { owned: Plain }",
            false,
        ),
        (
            "#[derive(SystemParam)] struct Params<'w, 's> { rows: Query<'w, 's, &'static OtherState>, owned: State }",
            false,
        ),
        (
            "#[derive(SystemParam)] struct Params<'w, 's> { rows: Query<'w, 's, State> }",
            false,
        ),
        (
            "type Owned = (State,); #[derive(SystemParam)] struct Params<'w, 's> { rows: Query<'w, 's, Owned> }",
            false,
        ),
        (
            "#[derive(SystemParam)] struct Params<'w, 's> { #[cfg(test)] rows: Query<'w, 's, &'static State>, other: Query<'w, 's, &'static OtherState> }",
            false,
        ),
        (
            "#[derive(SystemParam)] struct Params<'w, 's> { #[cfg(any())] rows: Query<'w, 's, &'static State>, other: Query<'w, 's, &'static OtherState> }",
            false,
        ),
    ] {
        let source = format!(
            "{prefix} {definition} fn render(params: Params) {{}} fn prove(states: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, (render, prove)); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let key = inventory.key("fn:render");
        let mut boundary = Boundary::fixture(&inventory, &key);
        boundary.input = format!("ECS: {}", inventory.key("struct:State"));
        boundary.output = "external: one-way renderer output".into();
        assert_eq!(boundary.references(&inventory), expected, "{definition}");
    }
}

#[test]
fn boundary_inputs_follow_production_system_state_and_actual_observer_registration() {
    let source = "use bevy_ecs::prelude::*; use bevy_app::{App, Plugin}; use bevy_ecs::system::SystemState; #[derive(Component)] struct State; #[derive(EntityEvent)] struct Changed { #[event_target] entity: Entity } #[derive(SystemParam)] struct Params<'w, 's> { states: Query<'w, 's, &'static State> } fn render(world: &mut World) { let mut state = SystemState::<Params>::new(world); state.get_mut(world); } fn observe(event: On<Changed>, states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, render).add_observer(observe); } }";
    let inventory = Inventory::fixture(source);
    for suffix in ["fn:render", "struct:Root"] {
        let key = inventory.key(suffix);
        let mut boundary = Boundary::fixture(&inventory, &key);
        boundary.input = format!("ECS: {}", inventory.key("struct:State"));
        boundary.output = "external: registered one-way render effect".into();
        assert!(boundary.references(&inventory), "{suffix}");
    }
}

#[test]
fn registered_exclusive_systems_reach_nested_query_parameters() {
    for source in [
        "use bevy_ecs::system::SystemState; #[derive(Component)] struct State { value: u32 } #[derive(SystemParam)] struct Inner { states: Query<&State> } #[derive(SystemParam)] struct Outer { inner: Inner } fn read(world: &mut World) { let mut state = SystemState::<Outer>::new(world); state.get_mut(world); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
        "#[derive(Component)] struct State { value: u32 } #[derive(SystemParam)] struct Inner { states: Query<&State> } #[derive(SystemParam)] struct Outer { inner: Inner } fn read(outer: Outer) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    ] {
        let inventory = Inventory::fixture(source);
        for name in ["struct:State", "struct:Inner", "struct:Outer", "fn:read"] {
            assert!(
                inventory.canonical(
                    &inventory.entries[&inventory.key(name)],
                    &Policy::default(),
                    &mut BTreeSet::new()
                ),
                "{name}"
            );
        }
    }
    for source in [
        "#[derive(Component)] struct State { value: u32 } #[derive(SystemParam)] struct Inner { states: Query<&State> } fn read(world: &mut World) { let state = SystemState::<Inner>::new(world); }",
        "#[derive(Component)] struct State { value: u32 } #[derive(SystemParam)] struct Inner { states: Query<&State> } #[cfg(test)] fn read(world: &mut World) { let state = SystemState::<Inner>::new(world); } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
        "#[derive(Component)] struct State { value: u32 } #[derive(Component)] struct Wrapper { state: State } fn read(wrappers: Query<&Wrapper>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    ] {
        let inventory = Inventory::fixture(source);
        assert!(!inventory.canonical(
            &inventory.entries[&inventory.key("struct:State")],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn production_conditional_derives_and_imported_contracts_preserve_type_ownership() {
    let inventory = Inventory::fixture(
        "#[cfg_attr(feature = \"bevy\", derive(Component))] struct Id(String); fn read(ids: Query<&Id>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    assert!(inventory.canonical(
        &inventory.entries[&inventory.key("struct:Id")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    let inventory = Inventory::fixture(
        "#[cfg_attr(test, derive(Component))] struct Id(String); fn read(ids: Query<&Id>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    assert!(!inventory.canonical(
        &inventory.entries[&inventory.key("struct:Id")],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    for (source, expected) in [
        (
            "use vmux_macro::string_id; #[string_id] struct Id(String);",
            true,
        ),
        (
            "use local::string_id; #[string_id] struct Id(String);",
            false,
        ),
    ] {
        let inventory = Inventory::fixture(source);
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("struct:Id")],
                &Policy::default(),
                &mut BTreeSet::new()
            ),
            expected
        );
    }
}

#[test]
fn method_owned_values_compare_qualified_declaration_identity() {
    let inventory = Inventory::fixture(
        "mod one { pub enum Mode { Active } } mod two { pub enum Mode { Active } } #[derive(Component)] struct Marker; #[derive(SystemParam)] struct State { markers: Query<&Marker> } impl State { fn accept(&self, mode: one::Mode) {} } fn read(state: State) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    let mut policy = Policy::default();
    policy.values.push(ValueOwner {
        key: inventory.key("one|enum:Mode"),
        owner: inventory.key("struct:State"),
        lifetime: "one synchronous canonical capability operation".into(),
    });
    assert!(inventory.canonical(
        &inventory.entries[&policy.values[0].key],
        &policy,
        &mut BTreeSet::new()
    ));
    policy.values[0].key = inventory.key("two|enum:Mode");
    assert!(!inventory.canonical(
        &inventory.entries[&policy.values[0].key],
        &policy,
        &mut BTreeSet::new()
    ));
}

#[test]
fn owned_values_require_an_actual_canonical_field_or_operation_reference() {
    let inventory = Inventory::fixture(
        "#[derive(Component)] struct State { mode: Mode } enum Mode { Active, Idle } fn read(states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    let mut policy = Policy::default();
    for key in ["struct:Root", "impl:Plugin for Root"] {
        policy.boundaries.push(Boundary::fixture(&inventory, key));
    }
    policy.values.push(ValueOwner {
        key: inventory.key("enum:Mode"),
        owner: inventory.key("struct:State"),
        lifetime: "value of the State component".into(),
    });
    assert!(policy.audit(&inventory).is_empty());
    policy.values[0].owner = inventory.key("struct:Root");
    assert!(!policy.audit(&inventory).is_empty());
}

#[test]
fn deleted_or_modified_legacy_entries_cannot_silently_remain() {
    let inventory = Inventory::fixture("struct Record { id: String }");
    let mut policy = Policy::default();
    policy.legacy.push(Legacy {
        key: inventory.key("struct:Record"),
        fingerprint: inventory
            .entries
            .values()
            .next()
            .unwrap()
            .fingerprint
            .clone(),
        issue: "VMX-253".into(),
    });
    assert!(policy.audit(&inventory).is_empty());
    assert!(
        !policy
            .audit(&Inventory::fixture("struct Record { id: u64 }"))
            .is_empty()
    );
    assert!(!policy.audit(&Inventory::fixture("")).is_empty());
}

#[test]
fn dom_fixture_callers_require_the_real_constructor_and_a_production_renderer() {
    for (import, fixture, call, attributes, expected) in [
        (
            "use dioxus::core::VirtualDom;",
            "#[component] fn Fixture() -> Element { rsx! { View {} } }",
            "VirtualDom::new(Fixture)",
            "#[test]",
            true,
        ),
        (
            "struct VirtualDom;",
            "#[component] fn Fixture() -> Element { rsx! { View {} } }",
            "VirtualDom::new(Fixture)",
            "#[test]",
            false,
        ),
        (
            "use dioxus::core::VirtualDom;",
            "fn Fixture() {}",
            "VirtualDom::new(Fixture)",
            "#[test]",
            false,
        ),
        (
            "use dioxus::core::VirtualDom;",
            "#[component] fn Fixture() -> Element { rsx! { div {} } }",
            "VirtualDom::new(Fixture)",
            "#[test]",
            false,
        ),
        (
            "use dioxus::core::VirtualDom;",
            "#[component] fn Fixture() -> Element { rsx! { View {} } }",
            "\"VirtualDom::new(Fixture)\"",
            "#[test]",
            false,
        ),
        (
            "use dioxus::core::VirtualDom;",
            "#[component] fn Fixture() -> Element { rsx! { View {} } }",
            "VirtualDom::new(Fixture)",
            "",
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/feature/example/src/lib.rs",
            &format!("#[component] fn View() -> Element {{ rsx! {{ div {{}} }} }} #[cfg(test)] mod tests {{ use super::View; {import} {fixture} {attributes} fn mount() {{ {call}; }} }}"),
        );
        inventory.resolve_usage();
        let mut boundary = Boundary::fixture(&inventory, "tests|fn:mount");
        boundary.interface = "Dioxus VirtualDom fixture".into();
        assert_eq!(
            boundary.references(&inventory),
            expected,
            "{import}; {fixture}; {call}; {attributes}"
        );
    }
}

#[test]
fn plugin_adapter_helpers_follow_only_the_owners_registered_systems() {
    for (system, body, expected) in [
        ("read", "Reader::bytes();", true),
        ("unrelated", "Reader::bytes();", false),
        ("read", "", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_ecs::prelude::*; use bevy_app::{{App, Plugin}}; #[derive(Component)] struct State; struct Reader; fn read(state: Query<&State>) {{ {body} }} fn unrelated(state: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, {system}); }} }}"
        ));
        let mut parent = Boundary::fixture(&inventory, "impl:Plugin for Root");
        parent.input = format!("ECS: {}", inventory.key("struct:State"));
        parent.output = "external: installed native adapter systems".into();
        let child = Boundary {
            key: inventory.key("struct:Reader"),
            owner: Some(parent.key.clone()),
            fingerprint: inventory.entries[&inventory.key("struct:Reader")]
                .fingerprint
                .clone(),
            ..parent.clone()
        };
        let mut policy = Policy::default();
        policy.boundaries.push(parent);
        policy.boundaries.push(child.clone());
        assert_eq!(
            child.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{system}; {body}",
        );
    }
}

#[test]
fn page_metadata_requires_the_actual_attribute_macro_and_a_typed_input() {
    for (macro_attribute, local_macro, input, expected) in [
        ("#[proc_macro_attribute]", "", "Metadata", true),
        ("", "", "Metadata", false),
        (
            "#[proc_macro_attribute]",
            "mod vmux_page { pub fn page() {} }",
            "Metadata",
            false,
        ),
        ("#[proc_macro_attribute]", "", "\"Metadata\"", false),
        ("#[proc_macro_attribute]", "", "Other", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/util/vmux_macro/src/lib.rs",
            &format!("pub struct MacroPackage; {macro_attribute} pub fn page() {{}}"),
        );
        inventory.source(
            "crates/util/vmux_page/src/lib.rs",
            "pub struct Page; pub use vmux_macro::page;",
        );
        inventory.source(
            "crates/feature/example/src/lib.rs",
            &format!("{local_macro} #[vmux_api::ui_state] struct Metadata {{ id: u64 }} struct Other; #[vmux_page::page(component = Root, states = [], takes = {input})] struct Entry;"),
        );
        inventory.resolve_usage();
        assert_eq!(
            inventory.owner_references(
                &inventory.entries["crates/feature/example/src/lib.rs||struct:Entry"],
                &inventory.entries["crates/feature/example/src/lib.rs||struct:Metadata"],
            ),
            expected,
            "{macro_attribute}; {local_macro}; {input}",
        );
    }
}

#[test]
fn typed_page_boundaries_require_real_macros_renderers_and_complete_inputs() {
    for (actual, renderer, input, fields, expected) in [
        (
            true,
            "#[dioxus::prelude::component] fn Root() {}",
            "Metadata",
            ";",
            true,
        ),
        (
            false,
            "#[dioxus::prelude::component] fn Root() {}",
            "Metadata",
            ";",
            false,
        ),
        (true, "fn Root() {}", "Metadata", ";", false),
        (true, "", "Metadata", ";", false),
        (
            true,
            "#[dioxus::prelude::component] fn Root() {}",
            "\"Metadata\"",
            ";",
            false,
        ),
        (
            true,
            "#[dioxus::prelude::component] fn Root() {}",
            "Metadata, \"Other\"",
            ";",
            false,
        ),
        (
            true,
            "#[dioxus::prelude::component] fn Root() {}",
            "Other",
            ";",
            false,
        ),
        (
            true,
            "#[dioxus::prelude::component] fn Root() {}",
            "Metadata",
            "{ state: u64 }",
            false,
        ),
    ] {
        let mut inventory = Inventory::default();
        let attribute = if actual {
            "#[proc_macro_attribute]"
        } else {
            ""
        };
        inventory.source(
            "crates/util/vmux_macro/src/lib.rs",
            &format!(
                "{attribute} pub fn page() {{}} #[proc_macro_attribute] pub fn ui_state() {{}}"
            ),
        );
        inventory.source(
            "crates/util/vmux_page/src/lib.rs",
            "pub use vmux_macro::page;",
        );
        inventory.source(
            "crates/util/vmux_api/src/lib.rs",
            "pub use vmux_macro::ui_state;",
        );
        inventory.source("crates/feature/example/src/lib.rs", &format!("#[vmux_api::ui_state] struct Metadata {{ id: u64 }} struct Other; {renderer} #[vmux_page::page(component = Root, states = [{input}])] struct Entry {fields}"));
        inventory.resolve_usage();
        let entry = &inventory.entries["crates/feature/example/src/lib.rs||struct:Entry"];
        assert_eq!(
            inventory.typed_page(entry),
            expected,
            "{actual} {renderer} {input} {fields}"
        );
        assert_eq!(
            inventory.canonical(entry, &Policy::default(), &mut BTreeSet::new()),
            expected
        );
    }
}

#[test]
fn inventory_metadata_proves_the_constructed_type_and_rejects_spoofed_macros() {
    for (body, local, expected) in [
        ("inventory::collect!(Metadata);", "", true),
        ("inventory::submit!(Metadata::of());", "", true),
        ("inventory::submit!(Metadata {});", "", true),
        ("inventory::submit!(Other::of::<Metadata>());", "", false),
        ("inventory::collect!(\"Metadata\");", "", false),
        ("inventory::collect!(Metadata);", "mod inventory {}", false),
    ] {
        let inventory =
            Inventory::fixture(&format!("struct Metadata; struct Other; {local} {body}"));
        let metadata = &inventory.entries[&inventory.key("struct:Metadata")];
        let registration = inventory
            .entries
            .values()
            .find(|entry| matches!(entry.kind, Kind::Macro))
            .unwrap();
        assert_eq!(
            inventory.owner_references(registration, metadata),
            expected,
            "{body}; {local}"
        );
    }
}

#[test]
fn command_components_require_the_actual_default_unit_binding_attribute() {
    for (attribute, fields, actual, expected) in [
        ("vmux_command::command", ";", true, true),
        ("vmux_command::command(message)", ";", true, false),
        ("vmux_command::command(binding)", ";", true, false),
        ("vmux_command::command", "{ hidden: String }", true, false),
        ("vmux_command::command", ";", false, false),
    ] {
        let mut inventory = Inventory::default();
        let implementation = if actual {
            "#[proc_macro_attribute] pub fn command() {}"
        } else {
            "pub fn command() {}"
        };
        inventory.source("crates/util/vmux_macro/src/lib.rs", implementation);
        inventory.source(
            "crates/feature/vmux_command/src/lib.rs",
            "pub use vmux_macro::command;",
        );
        inventory.source("crates/feature/example/src/lib.rs", &format!(
            "use bevy_app::{{App, Plugin, Update}}; use bevy_ecs::prelude::*; #[{attribute}] struct Binding {fields} fn consume(bindings: Query<(), With<Binding>>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, consume); }} }}"
        ));
        inventory.resolve_usage();
        let binding = &inventory.entries["crates/feature/example/src/lib.rs||struct:Binding"];
        assert_eq!(
            inventory.canonical(binding, &Policy::default(), &mut BTreeSet::new()),
            expected
        );
    }
}

#[test]
fn page_subscriptions_prove_only_typed_inputs_of_the_actual_attribute_macro() {
    for (field, input, actual, expected) in [
        ("states", "crate::Metadata", true, true),
        ("events", "crate::Metadata", true, true),
        ("states", "\"Metadata\"", true, false),
        ("states", "crate::Other", true, false),
        ("states", "crate::Metadata", false, false),
    ] {
        let mut inventory = Inventory::default();
        let attribute = if actual {
            "#[proc_macro_attribute]"
        } else {
            ""
        };
        inventory.source(
            "crates/util/vmux_macro/src/lib.rs",
            &format!("{attribute} pub fn page() {{}}"),
        );
        inventory.source(
            "crates/util/vmux_page/src/lib.rs",
            "pub use vmux_macro::page;",
        );
        inventory.source("crates/feature/example/src/lib.rs", &format!(
            "#[vmux_api::ui_state] struct Metadata {{ id: u64 }} struct Other; #[vmux_page::page({field} = [{input}])] struct Entry;"
        ));
        inventory.resolve_usage();
        assert_eq!(
            inventory.owner_references(
                &inventory.entries["crates/feature/example/src/lib.rs||struct:Entry"],
                &inventory.entries["crates/feature/example/src/lib.rs||struct:Metadata"],
            ),
            expected
        );
    }
}

#[test]
fn absolute_page_attributes_bind_the_external_macro_despite_local_globs() {
    for (prefix, expected) in [("::", true), ("", false)] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/util/vmux_macro/src/lib.rs",
            "#[proc_macro_attribute] pub fn page() {}",
        );
        inventory.source(
            "crates/util/vmux_page/src/lib.rs",
            "pub use vmux_macro::page;",
        );
        inventory.source("crates/feature/example/src/lib.rs", &format!(
            "mod local {{ pub mod vmux_page {{ pub fn page() {{}} }} }} use local::*; #[vmux_api::ui_state] struct Metadata {{ id: u64 }} #[{prefix}vmux_page::page(states = [Metadata])] struct Entry;"
        ));
        inventory.resolve_usage();
        assert_eq!(
            inventory.owner_references(
                &inventory.entries["crates/feature/example/src/lib.rs||struct:Entry"],
                &inventory.entries["crates/feature/example/src/lib.rs||struct:Metadata"],
            ),
            expected
        );
    }
}

#[test]
fn new_source_exports_update_previously_missing_owner_references() {
    let mut inventory = Inventory::default();
    inventory.source(
        "crates/util/example/src/lib.rs",
        "mod values; mod bridge; use bridge::Value; struct Owner { value: Value }",
    );
    inventory.source("crates/util/example/src/values.rs", "pub struct Value;");
    inventory.resolve_usage();
    let owner = "crates/util/example/src/lib.rs||struct:Owner";
    let value = "crates/util/example/src/values.rs||struct:Value";
    assert!(!inventory.owner_references(&inventory.entries[owner], &inventory.entries[value]));
    inventory.source(
        "crates/util/example/src/bridge.rs",
        "pub use crate::values::Value;",
    );
    inventory.resolve_usage();
    assert!(inventory.owner_references(&inventory.entries[owner], &inventory.entries[value]));
}

#[test]
fn fixture_helpers_require_a_current_referenced_boundary_with_the_same_endpoint() {
    for (attributes, body, stale, other_endpoint, expected) in [
        ("#[cfg(test)]", "Source::bytes()", false, false, true),
        ("#[cfg(test)]", "", false, false, false),
        ("#[cfg(test)]", "Source::bytes()", true, false, false),
        ("#[cfg(test)]", "Source::bytes()", false, true, false),
        ("", "Source::bytes()", false, false, false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[vmux_api::ui_state] struct State {{ x: u32 }} #[vmux_api::ui_state] struct Other {{ x: u32 }} struct Source; impl Source {{ fn bytes(state: &State) {{}} }} {attributes} struct Harness; {attributes} impl Harness {{ fn run() {{ {body}; }} }}"
        ));
        let endpoint = format!("ECS: {}", inventory.key("struct:State"));
        let mut parent = Boundary::fixture(&inventory, "impl:Source");
        parent.input = if other_endpoint {
            format!("ECS: {}", inventory.key("struct:Other"))
        } else {
            endpoint.clone()
        };
        parent.output = "external: fixture bytes".into();
        if stale {
            parent.fingerprint = "stale".into();
        }
        let mut child = Boundary::fixture(&inventory, "impl:Harness");
        child.input = endpoint;
        child.output = "external: isolated observation".into();
        let mut policy = Policy::default();
        policy.boundaries.push(parent);
        policy.boundaries.push(child.clone());
        assert_eq!(
            child.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{attributes}; {body}; stale={stale}; other_endpoint={other_endpoint}",
        );
    }
}

#[test]
fn embedded_scripts_require_a_literal_source_edge_and_a_current_parent_boundary() {
    for (inclusion, production_consumer, expected) in [
        ("include_str!(\"fixture.js\")", "", true),
        ("\"fixture.js\"", "", false),
        ("include_str!(\"different.js\")", "", false),
        ("include_str!(\"../fixture.js\")", "", false),
        (
            "include_str!(\"fixture.js\")",
            "macro_rules! include_str { ($path:literal) => { \"unrelated\" } }",
            false,
        ),
        (
            "include_str!(\"fixture.js\")",
            "fn run() { include_str!(\"fixture.js\"); }",
            false,
        ),
    ] {
        let mut inventory = Inventory::fixture(&format!(
            "#[vmux_api::ui_state] struct State {{ x: u32 }} #[cfg(test)] struct Harness; #[cfg(test)] impl Harness {{ fn run(state: &State) {{ {inclusion}; }} }} {production_consumer}"
        ));
        inventory.script("fixture.js", "globalThis.ready = true;");
        let mut parent = Boundary::fixture(&inventory, "impl:Harness");
        parent.input = format!("ECS: {}", inventory.key("struct:State"));
        parent.output = "external: isolated DOM fixture".into();
        let child = Boundary {
            key: "fixture.js||script".into(),
            owner: Some(parent.key.clone()),
            fingerprint: inventory.entries["fixture.js||script"].fingerprint.clone(),
            ..parent.clone()
        };
        let mut policy = Policy::default();
        policy.boundaries.push(parent);
        policy.boundaries.push(child.clone());
        assert_eq!(
            child.references_from(&inventory, &policy, &mut BTreeSet::new()),
            expected,
            "{inclusion}; {production_consumer}",
        );
    }
}

#[test]
fn test_only_system_parameters_do_not_change_production_ownership() {
    for (parameter, expected) in [
        ("#[cfg(test)] fixture: Res<Recorder>", true),
        ("fixture: Res<Recorder>", false),
    ] {
        let inventory = Inventory::fixture(&format!(
            "#[derive(Component)] struct State; struct Recorder(Vec<String>); fn project(state: Query<&State>, {parameter}) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, project); }} }}"
        ));
        assert_eq!(
            inventory.canonical(
                &inventory.entries[&inventory.key("fn:project")],
                &Policy::default(),
                &mut BTreeSet::new(),
            ),
            expected,
            "{parameter}",
        );
    }
}

#[test]
fn opaque_scripts_and_unparsed_macros_require_explicit_boundaries() {
    let inventory = Inventory::fixture(
        "thread_local! { static STATE: RefCell<Vec<String>> = RefCell::default(); }",
    );
    assert!(!Policy::default().audit(&inventory).is_empty());
    let mut inventory = Inventory::default();
    inventory.script("fixture.js", "globalThis.registry = new Map();");
    assert!(!Policy::default().audit(&inventory).is_empty());
}

#[test]
fn invalid_source_is_an_error_instead_of_missing_inventory() {
    assert!(std::panic::catch_unwind(|| Inventory::fixture("struct")).is_err());
}

#[test]
fn system_import_aliases_preserve_canonical_ownership() {
    let inventory = Inventory::fixture(
        "use self::read as update; #[derive(Component)] struct State { x: f32 } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, update); } }",
    );
    let state = inventory.key("struct:State");
    let read = inventory.key("fn:read");
    assert!(inventory.canonical(
        &inventory.entries[&state],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
    assert!(inventory.canonical(
        &inventory.entries[&read],
        &Policy::default(),
        &mut BTreeSet::new()
    ));
}

#[test]
fn test_only_blocks_and_methods_do_not_register_production_systems() {
    for source in [
        "#[derive(Component)] struct State { x: f32 } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { #[cfg(test)] { app.add_systems(Update, read); } } }",
        "#[derive(Component)] struct State { x: f32 } fn read(state: Query<&State>) {} struct Root; impl Plugin for Root { #[cfg(test)] fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    ] {
        let inventory = Inventory::fixture(source);
        let state = inventory.key("struct:State");
        assert!(!inventory.canonical(
            &inventory.entries[&state],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

#[test]
fn adding_component_to_a_ui_payload_does_not_create_business_ownership() {
    let inventory = Inventory::fixture(
        "#[derive(Component)] #[vmux_api::ui_state] struct View { allowed: bool } impl View { fn authorize(&self) -> bool { self.allowed } } fn authorize(state: Query<&View>) -> bool { true } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, authorize); } }",
    );
    for suffix in ["struct:View", "impl:View", "fn:authorize"] {
        let key = inventory.key(suffix);
        assert!(!inventory.canonical(
            &inventory.entries[&key],
            &Policy::default(),
            &mut BTreeSet::new()
        ));
    }
}

struct Workspace(PathBuf);

struct Repository(tempfile::TempDir);

impl Repository {
    fn new() -> Self {
        let repository = Self(tempfile::tempdir().unwrap());
        repository.write(
            "ecs-policy.ron",
            &format!("(revision: \"{FROZEN_REVISION}\")"),
        );
        repository
    }

    fn write(&self, path: &str, source: &str) {
        let path = self.0.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }

    fn snapshot(&self) {
        for args in [
            vec!["init", "--quiet"],
            vec!["add", "."],
            vec![
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Policy fixture",
            ],
            vec!["update-ref", "refs/remotes/origin/main", "HEAD"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(self.0.path())
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .env("GIT_AUTHOR_NAME", "Policy fixture")
                .env("GIT_AUTHOR_EMAIL", "policy@example.invalid")
                .env("GIT_COMMITTER_NAME", "Policy fixture")
                .env("GIT_COMMITTER_EMAIL", "policy@example.invalid")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

impl Workspace {
    fn current() -> Self {
        let current = std::env::current_dir().expect("current directory");
        for root in current.ancestors() {
            if root.join("crates").is_dir() && root.join("Cargo.toml").is_file() {
                return Self(root.to_path_buf());
            }
        }
        panic!("ownership checks must run inside the workspace");
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Policy {
    revision: String,
    legacy: Vec<Legacy>,
    boundaries: Vec<Boundary>,
    values: Vec<ValueOwner>,
    upstream: Vec<Upstream>,
    #[serde(default)]
    shape: ShapePolicy,
}

impl Policy {
    fn load(root: &Path) -> Result<Self, String> {
        let mut policy = Self::read(&root.join("ecs-policy.ron"))?;
        policy.check_root()?;
        let mut files = Vec::new();
        for directory in ["crates", "examples", "website", "patches", "scripts"] {
            Self::paths(&root.join(directory), &mut files)?;
        }
        files.sort();
        for path in files {
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            policy.append(relative, Self::read(&path)?)?;
        }
        Ok(policy)
    }

    fn check_root(&self) -> Result<(), String> {
        if !self.legacy.is_empty()
            || !self.boundaries.is_empty()
            || !self.values.is_empty()
            || !self.shape.debt.is_empty()
            || !self.shape.enums.is_empty()
            || !self.shape.bodies.is_empty()
        {
            return Err(
                "Root policy holds only historical revisions and upstream references".into(),
            );
        }
        Ok(())
    }

    fn append(&mut self, relative: &Path, fragment: Self) -> Result<(), String> {
        let directory = relative
            .parent()
            .ok_or("Policy fragment needs a directory")?;
        if Self::directory(relative).as_deref() != Some(directory) {
            return Err(format!("Misplaced policy fragment {}", relative.display()));
        }
        if !fragment.revision.is_empty()
            || !fragment.upstream.is_empty()
            || !fragment.shape.revision.is_empty()
        {
            return Err(format!(
                "Global provenance belongs in root policy: {}",
                relative.display()
            ));
        }
        for record in &fragment.legacy {
            Self::check_record(directory, &record.key)?;
        }
        for record in &fragment.boundaries {
            Self::check_record(directory, &record.key)?;
        }
        for record in &fragment.values {
            Self::check_record(directory, &record.key)?;
        }
        for record in &fragment.shape.debt {
            Self::check_record(directory, &record.key)?;
        }
        for record in &fragment.shape.enums {
            Self::check_record(directory, &record.key)?;
        }
        for record in &fragment.shape.bodies {
            Self::check_record(directory, &record.key)?;
        }
        self.legacy.extend(fragment.legacy);
        self.boundaries.extend(fragment.boundaries);
        self.values.extend(fragment.values);
        self.shape.debt.extend(fragment.shape.debt);
        self.shape.enums.extend(fragment.shape.enums);
        self.shape.bodies.extend(fragment.shape.bodies);
        Ok(())
    }

    fn check_record(directory: &Path, key: &str) -> Result<(), String> {
        let source = Path::new(key.split('|').next().unwrap());
        if Self::directory(source).as_deref() != Some(directory) {
            return Err(format!(
                "Foreign policy record {key} in {}",
                directory.display()
            ));
        }
        Ok(())
    }

    fn read(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("Policy must be a regular file: {}", path.display()));
        }
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        ron::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))
    }

    fn paths(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("{}: {error}", directory.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_dir() {
                if matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(
                        "target" | "node_modules" | ".git" | ".worktrees" | "dist" | ".build-tools"
                    )
                ) {
                    continue;
                }
                Self::paths(&path, files)?;
            } else if path
                .file_name()
                .is_some_and(|name| name == "ecs-policy.ron")
            {
                files.push(path);
            }
        }
        Ok(())
    }

    fn directory(path: &Path) -> Option<PathBuf> {
        for container in [
            "crates/app",
            "crates/feature",
            "crates/util",
            "examples",
            "patches",
        ] {
            if let Ok(relative) = path.strip_prefix(container) {
                return Some(Path::new(container).join(relative.components().next()?.as_os_str()));
            }
        }
        for directory in ["website", "scripts"] {
            if path.starts_with(directory) {
                return Some(directory.into());
            }
        }
        if path.parent() == Some(Path::new("crates")) {
            return Some("crates".into());
        }
        None
    }

    fn audit(&self, inventory: &Inventory) -> Vec<String> {
        let mut errors = Vec::new();
        let mut records = BTreeSet::new();
        let mut legacy_by_key = BTreeMap::new();
        let mut upstream_keys = BTreeSet::new();
        for upstream in &self.upstream {
            let pinned = UPSTREAM_SETS.iter().any(|(package, digest)| {
                *package == upstream.package && *digest == upstream.digest()
            });
            if !pinned || upstream.archive_sha256.len() != 64 {
                errors.push(format!(
                    "unverified upstream reference {}",
                    upstream.package
                ));
            }
            for (key, fingerprint) in &upstream.entries {
                if !key.starts_with("patches/") || !upstream_keys.insert(key) {
                    errors.push(format!("invalid/duplicate upstream entry {key}"));
                }
                if inventory.matches(key, fingerprint) {
                    records.insert(key);
                    legacy_by_key.insert(key.as_str(), fingerprint.as_str());
                }
            }
        }
        for legacy in &self.legacy {
            legacy_by_key.insert(legacy.key.as_str(), legacy.fingerprint.as_str());
            if !records.insert(&legacy.key) {
                errors.push(format!("duplicate policy entry {}", legacy.key));
            }
            if legacy.issue != "VMX-253" {
                errors.push(format!(
                    "legacy entry needs its migration issue: {}",
                    legacy.key
                ));
            }
            if !inventory.matches(&legacy.key, &legacy.fingerprint) {
                errors.push(format!(
                    "remove changed/deleted legacy entry {}",
                    legacy.key
                ));
            }
        }
        for boundary in &self.boundaries {
            if !records.insert(&boundary.key) {
                errors.push(format!("duplicate policy entry {}", boundary.key));
            }
            let referenced = boundary.references_from(inventory, self, &mut BTreeSet::new());
            if !inventory.matches(&boundary.key, &boundary.fingerprint)
                || !boundary.valid()
                || !referenced
            {
                errors.push(format!(
                    "stale/incomplete adapter boundary {} [current fingerprint: {}; references: {}]",
                    boundary.key,
                    inventory.entries.get(&boundary.key).map_or("missing", |entry| entry.fingerprint.as_str()),
                    referenced
                ));
            }
        }
        for value in &self.values {
            if !records.insert(&value.key) {
                errors.push(format!("duplicate policy entry {}", value.key));
            }
            let Some(entry) = inventory.entries.get(&value.key) else {
                errors.push(format!("remove deleted owned value {}", value.key));
                continue;
            };
            let Some(owner) = inventory.entries.get(&value.owner) else {
                errors.push(format!("missing canonical value owner {}", value.key));
                continue;
            };
            if value.lifetime.trim().is_empty()
                || !inventory.owner_references(owner, entry)
                || !matches!(
                    entry.kind,
                    Kind::Struct | Kind::Enum | Kind::Alias | Kind::Const
                )
                || !inventory.canonical(owner, self, &mut BTreeSet::new())
            {
                errors.push(format!(
                    "value lacks actual canonical ownership {}",
                    value.key
                ));
            }
        }
        for entry in inventory.entries.values() {
            if legacy_by_key
                .get(entry.key.as_str())
                .is_some_and(|fingerprint| *fingerprint == entry.fingerprint)
            {
                continue;
            }
            if !inventory.canonical(entry, self, &mut BTreeSet::new()) {
                errors.push(format!(
                    "{}: declare its canonical owner or actual external boundary [fingerprint: {}]",
                    entry.key, entry.fingerprint
                ));
            }
        }
        errors.extend(self.shape.audit(inventory, self));
        errors
    }

    fn fetched(root: &Path) -> Self {
        Self::from_git(root, "origin/main")
            .expect("fetch valid origin main before policy validation")
    }

    fn from_git(root: &Path, revision: &str) -> Result<Self, String> {
        let output = Command::new("git")
            .args(["rev-parse", "--verify", &format!("{revision}^{{commit}}")])
            .current_dir(root)
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("Cannot resolve policy revision {revision}"));
        }
        let captured = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
        let revision = captured.trim();
        let output = Command::new("git")
            .args(["ls-tree", "-r", "-z", revision])
            .current_dir(root)
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("Cannot read policy tree {revision}"));
        }
        let source = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
        let mut paths = BTreeSet::new();
        for entry in source.split('\0') {
            if entry.is_empty() {
                continue;
            }
            let (header, path) = entry.split_once('\t').ok_or("Invalid Git tree entry")?;
            let path = Path::new(path);
            if path.file_name().is_none_or(|name| name != "ecs-policy.ron") {
                continue;
            }
            if path != Path::new("ecs-policy.ron")
                && (!["crates", "examples", "website", "patches", "scripts"]
                    .iter()
                    .any(|directory| path.starts_with(directory))
                    || path.components().any(|part| {
                        matches!(
                            part.as_os_str().to_str(),
                            Some(
                                "target"
                                    | "node_modules"
                                    | ".git"
                                    | ".worktrees"
                                    | "dist"
                                    | ".build-tools"
                            )
                        )
                    }))
            {
                continue;
            }
            if header
                .split_whitespace()
                .next()
                .is_none_or(|mode| !matches!(mode, "100644" | "100755"))
            {
                return Err(format!("Policy must be a regular file: {}", path.display()));
            }
            paths.insert(path.to_path_buf());
        }
        if !paths.remove(Path::new("ecs-policy.ron")) {
            return Err("Missing root policy in Git tree".into());
        }
        let mut policy = Self::read_git(root, revision, Path::new("ecs-policy.ron"))?;
        policy.check_root()?;
        for path in paths {
            policy.append(&path, Self::read_git(root, revision, &path)?)?;
        }
        Ok(policy)
    }

    fn read_git(root: &Path, revision: &str, path: &Path) -> Result<Self, String> {
        let output = Command::new("git")
            .args(["show", &format!("{revision}:{}", path.display())])
            .current_dir(root)
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "Cannot read policy {} at {revision}",
                path.display()
            ));
        }
        ron::from_str(&String::from_utf8(output.stdout).map_err(|error| error.to_string())?)
            .map_err(|error| format!("{}: {error}", path.display()))
    }

    fn prove_legacy(&self, root: &Path) {
        assert_eq!(
            self.revision, FROZEN_REVISION,
            "the historical audit revision is immutable"
        );
        assert!(
            self.revision.len() == 40 && self.revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "exact historical audit revision required"
        );
        let mut paths = BTreeSet::new();
        for legacy in &self.legacy {
            paths.insert(legacy.key.split('|').next().unwrap());
        }
        let mut historical = Inventory::default();
        for path in paths {
            let output = Command::new("git")
                .args(["show", &format!("{}:{path}", self.revision)])
                .current_dir(root)
                .output()
                .expect("historical source must be available");
            assert!(
                output.status.success(),
                "cannot prove frozen source {path}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let source = String::from_utf8(output.stdout).expect("historical source is UTF-8");
            historical.source(path, &source);
        }
        for legacy in &self.legacy {
            assert!(
                historical.matches(&legacy.key, &legacy.fingerprint),
                "legacy baseline cannot grow or be refreshed: {}",
                legacy.key
            );
        }
    }
}

#[test]
fn domain_signal_enums_require_explicit_external_interface_purposes() {
    let inventory = Inventory::fixture(
        "#[derive(Message)] enum Commands { Build(String), Run(String) } struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_message::<Commands>(); } }",
    );
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    let errors = shape.audit(&inventory, &Policy::default());
    assert!(errors.iter().any(|error| error.contains("enum:Commands")));
}

#[test]
fn canonical_phase_purposes_require_real_owners_and_cannot_cover_signal_enums() {
    for signal in [false, true] {
        let derive = if signal { "Message" } else { "Clone" };
        let source = format!(
            "#[derive({derive})] enum Phase {{ Pending, Complete }} #[derive(Component)] struct State {{ phase: Phase }} fn read(states: Query<&State>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let key = inventory.key("enum:Phase");
        let purpose = EnumPurpose {
            key: key.clone(),
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            role: EnumRole::Phase,
            owner: inventory.key("struct:State"),
            reason: "One state owner's pending/completed lifecycle".into(),
        };
        let policy = Policy {
            values: vec![ValueOwner {
                key: key.clone(),
                owner: inventory.key("struct:State"),
                lifetime: "One canonical state lifecycle".into(),
            }],
            ..Default::default()
        };
        assert_eq!(
            purpose.valid(&inventory.entries[&key], &inventory, &policy),
            !signal
        );
        let mut bogus = purpose;
        bogus.owner = key.clone();
        assert!(!bogus.valid(&inventory.entries[&key], &inventory, &policy));
    }
}

#[test]
fn required_phase_purposes_follow_used_bevy_components_and_production_metadata() {
    for (owner, attributes, phase, import, consumers, expected) in [
        (
            "Component",
            "#[require(Phase)]",
            "Component",
            "",
            "&Input, &mut Phase",
            true,
        ),
        (
            "Component",
            "#[require(Phase = Phase::Start)]",
            "Component",
            "",
            "&Input, &mut Phase",
            true,
        ),
        (
            "Component",
            "#[require(Other)]",
            "Component",
            "",
            "&Input, &mut Phase",
            false,
        ),
        (
            "Component",
            "#[fake::require(Phase)]",
            "Component",
            "",
            "&Input, &mut Phase",
            false,
        ),
        (
            "Component",
            "#[cfg_attr(test, require(Phase))]",
            "Component",
            "",
            "&Input, &mut Phase",
            false,
        ),
        (
            "Resource",
            "#[require(Phase)]",
            "Component",
            "",
            "&Input, &mut Phase",
            false,
        ),
        (
            "Component",
            "#[require(Phase)]",
            "Message",
            "",
            "&Input, &mut Phase",
            false,
        ),
        (
            "Component",
            "#[require(Phase)]",
            "Component",
            "struct Component;",
            "&Input, &mut Phase",
            false,
        ),
        (
            "Component",
            "#[require(Phase)]",
            "Component",
            "",
            "&Input",
            false,
        ),
        (
            "Component",
            "#[require(Phase)]",
            "Component",
            "",
            "&mut Phase",
            false,
        ),
    ] {
        let source = format!(
            "use bevy_app::{{App, Plugin, Update}}; use bevy_ecs::prelude::*; {import} #[derive({phase}, Default)] enum Phase {{ #[default] Start, Done }} #[derive({owner})] {attributes} struct Input; fn advance(state: Query<({consumers})>) {{}} struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, advance); }} }}"
        );
        let inventory = Inventory::fixture(&source);
        let key = inventory.key("enum:Phase");
        let purpose = EnumPurpose {
            key: key.clone(),
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            role: EnumRole::Phase,
            owner: inventory.key("struct:Input"),
            reason: "One accepted input requires its own coherent lifecycle state".into(),
        };
        assert_eq!(
            purpose.valid(&inventory.entries[&key], &inventory, &Policy::default()),
            expected,
            "{source}"
        );
    }
}

#[test]
fn scalar_vocabularies_are_owned_values_rather_than_command_payloads() {
    let inventory = Inventory::fixture(
        "enum Tone { Plain, Strong, Quiet } #[derive(Component)] struct State { tone: Tone } fn read(states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    let key = inventory.key("enum:Tone");
    let purpose = EnumPurpose {
        key: key.clone(),
        fingerprint: inventory.entries[&key].fingerprint.clone(),
        role: EnumRole::Scalar,
        owner: inventory.key("struct:State"),
        reason: "One canonical state's display tone".into(),
    };
    let policy = Policy {
        values: vec![ValueOwner {
            key: key.clone(),
            owner: inventory.key("struct:State"),
            lifetime: "One canonical display tone".into(),
        }],
        ..Default::default()
    };
    assert!(purpose.valid(&inventory.entries[&key], &inventory, &policy));
    let changed = Inventory::fixture(
        "enum Tone { Build(String), Run(String) } #[derive(Component)] struct State { tone: Tone } fn read(states: Query<&State>) {} struct Root; impl Plugin for Root { fn build(&self, app: &mut App) { app.add_systems(Update, read); } }",
    );
    assert!(!purpose.valid(&changed.entries[&key], &changed, &Policy::default()));
}

#[test]
fn body_budget_counts_code_tokens_and_excludes_comments_and_test_only_blocks() {
    let source = "fn read() {\n let value = 1;\n // ignored comment\n\n #[cfg(test)] {\n  let a = 2;\n  let b = 3;\n }\n let other = 4;\n}\n";
    let inventory = Inventory::fixture(source);
    let entry = &inventory.entries[&inventory.key("fn:read")];
    assert_eq!(entry.bodies[0].lines, 4);
    let changed = Inventory::fixture(
        "fn read() {\n let value = 1;\n /* many\n comment\n lines */\n let other = 4;\n}\n",
    );
    assert_eq!(changed.entries[&changed.key("fn:read")].bodies[0].lines, 4);
}

#[test]
fn body_budget_preserves_production_tokens_on_test_only_shared_lines() {
    let statements = "let production = 1; #[cfg(test)] { let testing = 2; }\n".repeat(101);
    let inventory = Inventory::fixture(&format!("fn read() {{\n{statements}}}"));
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    assert!(
        shape
            .audit(&inventory, &Policy::default())
            .iter()
            .any(|error| error.contains("fn:read: body"))
    );
}

#[test]
fn trait_default_bodies_share_the_budget_without_counting_test_only_defaults() {
    let statements = "let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "trait Behavior {{ fn required(); fn dispatch(&self) {{\n{statements}}} #[cfg(test)] fn fixture(&self) {{\n{statements}}} }} struct Owner; impl Behavior for Owner {{ fn dispatch(&self) {{\n{statements}}} }}"
    ));
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    let errors = shape.audit(&inventory, &Policy::default());
    assert!(
        errors
            .iter()
            .any(|error| error.contains("trait:Behavior::dispatch: body"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("impl:Behavior for Owner::dispatch: body"))
    );
    assert!(!errors.iter().any(|error| error.contains("::fixture: body")));
}

#[test]
fn a_trait_body_waiver_cannot_cover_another_platforms_same_named_default() {
    let statements = "let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "trait Behavior {{ #[cfg(unix)] fn dispatch(&self) {{\n{statements}}} #[cfg(windows)] fn dispatch(&self) {{\n{statements}}} }}"
    ));
    let entry = &inventory.entries[&inventory.key("trait:Behavior")];
    let first = &entry.bodies[0];
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        debt: vec![ShapeDebt {
            key: first.key.clone(),
            fingerprint: first.fingerprint.clone(),
            issue: "VMX-253".into(),
            finding: "AU-14".into(),
        }],
        ..Default::default()
    };
    assert!(
        shape
            .audit(&inventory, &Policy::default())
            .iter()
            .any(|error| error.contains("trait:Behavior::dispatch#2: body"))
    );
}

#[test]
fn installation_arguments_cannot_hide_oversized_computation() {
    let statements = "let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "use bevy_app::Plugin; struct Root; struct State; impl Plugin for Root {{ fn build(&self,app:&mut bevy_app::App) {{ app.insert_resource({{\n{statements}State }}); }} }}"
    ));
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    assert!(
        shape
            .audit(&inventory, &Policy::default())
            .iter()
            .any(|error| error.contains("::build: body"))
    );
}

#[test]
fn body_budget_rejects_oversized_registered_systems_and_associated_dispatchers() {
    let statements = " let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "fn read(state: Query<&State>) {{\n{statements}}}\n#[derive(Component)] struct State; struct Root; impl Plugin for Root {{ fn build(&self, app: &mut App) {{ app.add_systems(Update, read); }} }} impl State {{ fn dispatch(&self) {{\n{statements}}} }}"
    ));
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    let errors = shape.audit(&inventory, &Policy::default());
    assert!(errors.iter().any(|error| error.contains("fn:read: body")));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("::dispatch: body"))
    );
}

#[test]
fn plugin_and_renderer_names_do_not_bypass_body_budgets_without_framework_bindings() {
    let statements = " let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "trait Plugin {{}} struct Root; impl Plugin for Root {{ fn build(&self) {{\n{statements}}} }} #[component] fn Screen() {{\n{statements}}}"
    ));
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    let errors = shape.audit(&inventory, &Policy::default());
    assert!(errors.iter().any(|error| error.contains("::build: body")));
    assert!(errors.iter().any(|error| error.contains("fn:Screen: body")));
}

#[test]
fn shape_debt_is_exact_and_cannot_cover_changed_or_deleted_items() {
    let source = "enum Commands { Build, Run }";
    let inventory = Inventory::fixture(source);
    let key = inventory.key("enum:Commands");
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        debt: vec![ShapeDebt {
            key: key.clone(),
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            issue: "VMX-253".into(),
            finding: "AU-13".into(),
        }],
        ..Default::default()
    };
    assert!(shape.audit(&inventory, &Policy::default()).is_empty());
    assert!(
        shape
            .audit(
                &Inventory::fixture("enum Commands { Build, Run, Stop }"),
                &Policy::default()
            )
            .iter()
            .any(|error| error.contains("stale/invalid shape debt"))
    );
    assert!(
        shape
            .audit(&Inventory::fixture(""), &Policy::default())
            .iter()
            .any(|error| error.contains("stale/invalid shape debt"))
    );
}

#[test]
fn shape_debt_fingerprints_are_proven_against_the_fixed_git_snapshot() {
    let root = Workspace::current();
    let path = "crates/feature/vmux_dx/src/work.rs";
    let output = Command::new("git")
        .args(["show", &format!("{FROZEN_SHAPE_REVISION}:{path}")])
        .current_dir(&root.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "required frozen Git fixture is missing"
    );
    let source = String::from_utf8(output.stdout).unwrap();
    let mut inventory = Inventory::default();
    inventory.source(path, &source);
    let key = inventory.key("enum:Work");
    let mut shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        debt: vec![ShapeDebt {
            key: key.clone(),
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            issue: "VMX-253".into(),
            finding: "AU-13".into(),
        }],
        ..Default::default()
    };
    shape.prove(&root.0);
    shape.debt[0].fingerprint = "0000000000000000".into();
    assert!(std::panic::catch_unwind(|| shape.prove(&root.0)).is_err());
    inventory.source("new.rs", "enum NewCommand { Build, Run }");
    let key = inventory.key("enum:NewCommand");
    shape.debt[0].key = key.clone();
    shape.debt[0].fingerprint = inventory.entries[&key].fingerprint.clone();
    assert!(std::panic::catch_unwind(|| shape.prove(&root.0)).is_err());
}

#[test]
fn real_plugin_build_only_exempts_framework_installation_statements() {
    let installations = "app.init_resource::<State>();\n".repeat(101);
    let business = "let value = 1;\n".repeat(101);
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    for (body, rejected) in [(installations, false), (business, true)] {
        let inventory = Inventory::fixture(&format!(
            "use bevy_app::Plugin; struct Root; struct State; impl Plugin for Root {{ fn build(&self, app: &mut bevy_app::App) {{\n{body}}} }}"
        ));
        assert_eq!(
            shape
                .audit(&inventory, &Policy::default())
                .iter()
                .any(|error| error.contains("::build: body")),
            rejected
        );
    }
}

#[test]
fn inline_system_closures_cannot_hide_inside_large_plugin_installation_bodies() {
    let statements = "let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "use bevy_app::Plugin; struct Root; impl Plugin for Root {{ fn build(&self, app: &mut bevy_app::App) {{ app.add_systems(Update, || {{\n{statements}}}); }} }}"
    ));
    let shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        ..Default::default()
    };
    assert!(
        shape
            .audit(&inventory, &Policy::default())
            .iter()
            .any(|error| error.contains("::build: body"))
    );
}

#[test]
fn mechanical_body_exceptions_are_exact_not_global_or_stale_waivers() {
    let statements = "let value = 1;\n".repeat(101);
    let inventory = Inventory::fixture(&format!(
        "fn read() {{\n{statements}}}\nfn other() {{\n{statements}}}"
    ));
    let key = inventory.key("fn:read");
    let boundary = Boundary::fixture(&inventory, "fn:read");
    let policy = Policy {
        boundaries: vec![boundary],
        ..Default::default()
    };
    let mut shape = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        bodies: vec![BodyException {
            key: key.clone(),
            fingerprint: inventory.entries[&key].bodies[0].fingerprint.clone(),
            boundary: key.clone(),
            reason: "one bounded native format operation".into(),
        }],
        ..Default::default()
    };
    let errors = shape.audit(&inventory, &policy);
    assert!(!errors.iter().any(|error| error.contains("fn:read")));
    assert!(errors.iter().any(|error| error.contains("fn:other: body")));
    shape.bodies[0].fingerprint = "stale".into();
    assert!(
        shape
            .audit(&inventory, &policy)
            .iter()
            .any(|error| error.contains("invalid mechanical body exception"))
    );
    shape.bodies[0].fingerprint = inventory.entries[&key].bodies[0].fingerprint.clone();
    shape.bodies[0].boundary = inventory.key("fn:other");
    assert!(
        shape
            .audit(&inventory, &policy)
            .iter()
            .any(|error| error.contains("invalid mechanical body exception"))
    );
}

#[test]
fn ui_encoder_purposes_require_the_real_framework_attribute_and_exact_boundary() {
    for fake in [false, true] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/util/vmux_api/src/lib.rs",
            "pub use vmux_macro::ui_event_variants;",
        );
        let local = if fake {
            "mod vmux_api { pub use fake_macros::ui_event_variants; }"
        } else {
            ""
        };
        inventory.source(
            "fixture.rs",
            &format!("{local} #[vmux_api::ui_event_variants] enum Operation {{ Read, Write }}"),
        );
        inventory.resolve_usage();
        let key = inventory.key("enum:Operation");
        let boundary = Boundary::fixture(&inventory, "enum:Operation");
        let policy = Policy {
            boundaries: vec![boundary],
            ..Default::default()
        };
        let purpose = EnumPurpose {
            key: key.clone(),
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            role: EnumRole::UiEncoder,
            owner: key.clone(),
            reason: "fixed translation to separately typed UI events".into(),
        };
        assert_eq!(
            purpose.valid(&inventory.entries[&key], &inventory, &policy),
            !fake
        );
    }
}

#[test]
fn retired_shape_debt_cannot_return_even_if_the_old_declaration_still_exists() {
    let inventory = Inventory::fixture("enum A { One, Two } enum B { One, Two }");
    let a = inventory.key("enum:A");
    let b = inventory.key("enum:B");
    let make = |key: &str| ShapeDebt {
        key: key.into(),
        fingerprint: inventory.entries[key].fingerprint.clone(),
        issue: "VMX-253".into(),
        finding: "AU-13".into(),
    };
    let main = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        debt: vec![make(&a)],
        ..Default::default()
    };
    let mut branch = ShapePolicy {
        revision: FROZEN_SHAPE_REVISION.into(),
        debt: vec![make(&a)],
        ..Default::default()
    };
    assert!(branch.monotonic_from(&main));
    branch.debt.push(make(&b));
    assert!(!branch.monotonic_from(&main));
    branch.debt.pop();
    branch.debt[0].fingerprint = "changed".into();
    assert!(!branch.monotonic_from(&main));
}

#[test]
fn io_component_multiplexers_cannot_claim_a_foreign_format_boundary_without_format_evidence() {
    let inventory = Inventory::fixture(
        "#[derive(Component)] enum Effect { Read(String), Copy(String, String) }",
    );
    let key = inventory.key("enum:Effect");
    let boundary = Boundary::fixture(&inventory, "enum:Effect");
    let policy = Policy {
        boundaries: vec![boundary],
        ..Default::default()
    };
    let purpose = EnumPurpose {
        key: key.clone(),
        fingerprint: inventory.entries[&key].fingerprint.clone(),
        role: EnumRole::Boundary,
        owner: key.clone(),
        reason: "generic IO dispatcher".into(),
    };
    assert!(!purpose.valid(&inventory.entries[&key], &inventory, &policy));
}

const FROZEN_SHAPE_REVISION: &str = "963f3f40e33c78f61bc55ead5e9a6a9f01b65081";

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct ShapePolicy {
    revision: String,
    debt: Vec<ShapeDebt>,
    enums: Vec<EnumPurpose>,
    bodies: Vec<BodyException>,
}

impl ShapePolicy {
    fn audit(&self, inventory: &Inventory, policy: &Policy) -> Vec<String> {
        if self.revision.is_empty() {
            return Vec::new();
        }
        let mut errors = Vec::new();
        let mut covered = BTreeSet::new();
        for debt in &self.debt {
            let kind_matches = match debt.finding.as_str() {
                "AU-13" => inventory
                    .entries
                    .get(&debt.key)
                    .is_some_and(|entry| entry.enum_shape.is_some()),
                "AU-14" => inventory.body_owner(&debt.key).is_some(),
                _ => false,
            };
            if !kind_matches
                || !covered.insert(debt.key.clone())
                || debt.issue != "VMX-253"
                || !matches!(debt.finding.as_str(), "AU-13" | "AU-14")
                || !inventory.shape_matches(&debt.key, &debt.fingerprint)
            {
                errors.push(format!("remove stale/invalid shape debt {}", debt.key));
            }
        }
        for purpose in &self.enums {
            let valid = inventory.entries.get(&purpose.key).is_some_and(|entry| {
                entry.fingerprint == purpose.fingerprint && purpose.valid(entry, inventory, policy)
            });
            if !covered.insert(purpose.key.clone()) || !valid {
                errors.push(format!("invalid enum purpose {}", purpose.key));
            }
        }
        for exception in &self.bodies {
            let valid = inventory.shape_matches(&exception.key, &exception.fingerprint)
                && !exception.reason.trim().is_empty()
                && policy.boundaries.iter().any(|boundary| {
                    boundary.key == exception.boundary
                        && boundary.valid()
                        && inventory.matches(&boundary.key, &boundary.fingerprint)
                        && boundary.references_from(inventory, policy, &mut BTreeSet::new())
                        && inventory.body_owner(&exception.key).is_some_and(|entry| {
                            entry.key == boundary.key
                                || inventory
                                    .entries
                                    .get(&boundary.key)
                                    .is_some_and(|owner| inventory.owner_references(owner, entry))
                        })
                });
            if !covered.insert(exception.key.clone()) || !valid {
                errors.push(format!(
                    "invalid mechanical body exception {}",
                    exception.key
                ));
            }
        }
        for entry in inventory.entries.values() {
            if !entry.shape_source()
                || policy
                    .upstream
                    .iter()
                    .any(|upstream| upstream.entries.get(&entry.key) == Some(&entry.fingerprint))
            {
                continue;
            }
            if let Some(shape) = &entry.enum_shape
                && !covered.contains(&entry.key)
            {
                errors.push(format!(
                    "{}: declare its enum purpose ({} variants) [fingerprint: {}]",
                    entry.key, shape.variants, entry.fingerprint
                ));
            }
            if entry.renderer
                && !inventory.registered_keys.contains(&entry.key)
                && entry.contract_paths.iter().any(|path| {
                    inventory
                        .framework_handle(&entry.namespace, path, &mut BTreeSet::new())
                        .as_deref()
                        == Some("component")
                })
            {
                continue;
            }
            for body in &entry.bodies {
                if !body.production
                    || body.lines <= 100
                    || covered.contains(&body.key)
                    || body.composition
                        && body.name == "build"
                        && entry.plugin_path.as_deref().is_some_and(|path| {
                            inventory
                                .framework_handle(&entry.namespace, path, &mut BTreeSet::new())
                                .as_deref()
                                == Some("Plugin")
                        })
                {
                    continue;
                }
                errors.push(format!(
                    "{}: body has {} production token lines; maximum is 100 [fingerprint: {}]",
                    body.key, body.lines, body.fingerprint
                ));
            }
        }
        errors
    }

    fn monotonic_from(&self, main: &Self) -> bool {
        if main.revision.is_empty() {
            return true;
        }
        let allowed: BTreeMap<_, _> = main
            .debt
            .iter()
            .map(|debt| (&debt.key, &debt.fingerprint))
            .collect();
        self.debt.iter().all(|debt| {
            allowed
                .get(&debt.key)
                .is_some_and(|fingerprint| *fingerprint == &debt.fingerprint)
        })
    }

    fn prove(&self, root: &Path) {
        assert_eq!(
            self.revision, FROZEN_SHAPE_REVISION,
            "shape debt revision is immutable"
        );
        let main = Policy::fetched(root);
        assert!(
            self.monotonic_from(&main.shape),
            "retired shape debt cannot be restored or refreshed"
        );
        let mut paths = BTreeSet::new();
        for debt in &self.debt {
            paths.insert(debt.key.split('|').next().unwrap());
        }
        let mut historical = Inventory::default();
        for path in paths {
            let output = Command::new("git")
                .args(["show", &format!("{}:{path}", self.revision)])
                .current_dir(root)
                .output()
                .expect("frozen shape source must be available");
            assert!(
                output.status.success(),
                "missing frozen shape source {path}"
            );
            historical.source(
                path,
                &String::from_utf8(output.stdout).expect("UTF-8 shape source"),
            );
        }
        for debt in &self.debt {
            assert!(
                historical.shape_matches(&debt.key, &debt.fingerprint),
                "shape debt cannot grow or be refreshed: {}",
                debt.key
            );
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ShapeDebt {
    key: String,
    fingerprint: String,
    issue: String,
    finding: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EnumPurpose {
    key: String,
    fingerprint: String,
    role: EnumRole,
    owner: String,
    reason: String,
}

impl EnumPurpose {
    fn valid(&self, entry: &Declaration, inventory: &Inventory, policy: &Policy) -> bool {
        let Some(shape) = &entry.enum_shape else {
            return false;
        };
        if self.fingerprint != entry.fingerprint
            || self.reason.trim().is_empty()
            || !entry.shape_source()
        {
            return false;
        }
        match self.role {
            EnumRole::UiEncoder => {
                entry.contract_paths.iter().any(|path| {
                    inventory
                        .contract_kind(&entry.namespace, path, &mut BTreeSet::new())
                        .as_deref()
                        == Some("ui_event_variants")
                }) && policy.boundaries.iter().any(|boundary| {
                    boundary.key == self.owner
                        && boundary.valid()
                        && inventory.matches(&boundary.key, &boundary.fingerprint)
                        && boundary.references_from(inventory, policy, &mut BTreeSet::new())
                        && inventory.entries.get(&boundary.key).is_some_and(|owner| {
                            owner.key == entry.key || inventory.owner_references(owner, entry)
                        })
                })
            }
            EnumRole::Boundary => {
                (!(shape.component || shape.signal || inventory.enum_job(entry))
                    || shape.fixed_format)
                    && (inventory.entries.get(&self.owner).is_some_and(|owner| {
                        inventory.automatic_boundary(owner)
                            && inventory.owner_references(owner, entry)
                    }) || policy.boundaries.iter().any(|boundary| {
                        boundary.key == self.owner
                            && boundary.valid()
                            && inventory.matches(&boundary.key, &boundary.fingerprint)
                            && boundary.references_from(inventory, policy, &mut BTreeSet::new())
                            && inventory.entries.get(&boundary.key).is_some_and(|owner| {
                                owner.key == entry.key || inventory.owner_references(owner, entry)
                            })
                    }))
            }
            EnumRole::Phase | EnumRole::Scalar | EnumRole::Projection => {
                if shape.signal || self.role != EnumRole::Projection && shape.payloads > 0 {
                    return false;
                }
                let Some(owner) = inventory.entries.get(&self.owner) else {
                    return false;
                };
                if !matches!(owner.kind, Kind::Struct)
                    || owner.key == entry.key
                    || !(inventory.owner_references(owner, entry)
                        || self.role == EnumRole::Phase
                            && inventory.required_component(owner, entry))
                    || !inventory.canonical(owner, policy, &mut BTreeSet::new())
                {
                    return false;
                }
                if self.role == EnumRole::Projection {
                    return owner.projection && !entry.ecs;
                }
                owner.ecs && inventory.used(owner)
            }
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
enum EnumRole {
    Phase,
    Scalar,
    Projection,
    UiEncoder,
    Boundary,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BodyException {
    key: String,
    fingerprint: String,
    boundary: String,
    reason: String,
}

struct EnumShape {
    variants: usize,
    payloads: usize,
    signal: bool,
    component: bool,
    fixed_format: bool,
}

impl EnumShape {
    fn of(value: &syn::ItemEnum) -> Self {
        let mut variants = 0;
        let mut payloads = 0;
        for variant in &value.variants {
            if Usage::test_only(&variant.attrs) {
                continue;
            }
            variants += 1;
            if !matches!(variant.fields, syn::Fields::Unit) {
                payloads += 1;
            }
        }
        let mut signal = false;
        let mut component = false;
        let mut fixed_format = false;
        for attribute in Declarations::metadata(&value.attrs, true) {
            if attribute.path().segments.last().is_some_and(|part| {
                matches!(
                    part.ident.to_string().as_str(),
                    "ui_event" | "service_message" | "agent"
                )
            }) {
                signal = true;
            }
            fixed_format |= attribute.path().is_ident("repr") || attribute.path().is_ident("serde");
            if !attribute.path().is_ident("derive") {
                continue;
            }
            let syn::Meta::List(list) = attribute else {
                continue;
            };
            let parser = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated;
            let Ok(paths) = parser.parse2(list.tokens) else {
                continue;
            };
            for path in paths {
                component |= path
                    .segments
                    .last()
                    .is_some_and(|part| part.ident == "Component");
                fixed_format |= path.segments.last().is_some_and(|part| {
                    matches!(
                        part.ident.to_string().as_str(),
                        "Serialize" | "Deserialize" | "Archive"
                    )
                });
                signal |= path.segments.last().is_some_and(|part| {
                    matches!(
                        part.ident.to_string().as_str(),
                        "Message" | "Event" | "EntityEvent"
                    )
                });
            }
        }
        Self {
            variants,
            payloads,
            signal,
            component,
            fixed_format,
        }
    }
}

struct BodyShape {
    key: String,
    name: String,
    fingerprint: String,
    lines: usize,
    production: bool,
    composition: bool,
    bootstrap_defaults: Option<BTreeSet<String>>,
    bootstrap_app: Option<String>,
    returns_self: bool,
    default_markers: Option<Vec<String>>,
}

impl BodyShape {
    fn items(key: &str, item: Option<&Item>) -> Vec<Self> {
        let mut bodies = Vec::new();
        match item {
            Some(Item::Fn(value)) => bodies.push(Self::of(
                key.to_string(),
                value.sig.ident.to_string(),
                &value,
                &value.block,
                &value.attrs,
            )),
            Some(Item::Impl(value)) => {
                for item in &value.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };
                    let mut body_key = format!("{key}::{}", method.sig.ident);
                    let mut ordinal = 2;
                    while bodies.iter().any(|body| body.key == body_key) {
                        body_key = format!("{key}::{}#{ordinal}", method.sig.ident);
                        ordinal += 1;
                    }
                    bodies.push(Self::of(
                        body_key,
                        method.sig.ident.to_string(),
                        method,
                        &method.block,
                        &method.attrs,
                    ));
                }
            }
            Some(Item::Trait(value)) => {
                for item in &value.items {
                    let syn::TraitItem::Fn(method) = item else {
                        continue;
                    };
                    let Some(block) = &method.default else {
                        continue;
                    };
                    let mut body_key = format!("{key}::{}", method.sig.ident);
                    let mut ordinal = 2;
                    while bodies.iter().any(|body| body.key == body_key) {
                        body_key = format!("{key}::{}#{ordinal}", method.sig.ident);
                        ordinal += 1;
                    }
                    bodies.push(Self::of(
                        body_key,
                        method.sig.ident.to_string(),
                        method,
                        block,
                        &method.attrs,
                    ));
                }
            }
            _ => {}
        }
        bodies
    }

    fn registration(block: &syn::Block, argument: &str) -> bool {
        let mut footprint = Footprint::default();
        footprint.visit_block(block);
        !footprint.closure
            && block.stmts.iter().all(|statement| match statement {
                syn::Stmt::Expr(expression, _) => Self::installation(expression, argument),
                _ => false,
            })
    }

    fn installation(expression: &syn::Expr, argument: &str) -> bool {
        match expression {
            syn::Expr::Block(value) => Self::registration(&value.block, argument),
            syn::Expr::If(value) => {
                Self::condition(&value.cond, argument)
                    && Self::registration(&value.then_branch, argument)
                    && value
                        .else_branch
                        .as_ref()
                        .is_none_or(|(_, branch)| Self::installation(branch, argument))
            }
            syn::Expr::MethodCall(value) => {
                if !matches!(
                    value.method.to_string().as_str(),
                    "add_plugins"
                        | "add_systems"
                        | "add_observer"
                        | "add_message"
                        | "add_event"
                        | "init_resource"
                        | "insert_resource"
                        | "register_type"
                        | "configure_sets"
                        | "register_required_components"
                ) {
                    return false;
                }
                value.args.iter().all(Self::installation_argument)
                    && match value.receiver.as_ref() {
                        syn::Expr::Path(path) => path.path.is_ident(argument),
                        other => Self::installation(other, argument),
                    }
            }
            _ => false,
        }
    }

    fn installation_argument(expression: &syn::Expr) -> bool {
        match expression {
            syn::Expr::Path(_) | syn::Expr::Lit(_) => true,
            syn::Expr::Reference(value) => Self::installation_argument(&value.expr),
            syn::Expr::Field(value) => Self::installation_argument(&value.base),
            syn::Expr::Paren(value) => Self::installation_argument(&value.expr),
            syn::Expr::Group(value) => Self::installation_argument(&value.expr),
            syn::Expr::Tuple(value) => value.elems.iter().all(Self::installation_argument),
            syn::Expr::Array(value) => value.elems.iter().all(Self::installation_argument),
            syn::Expr::Struct(value) => {
                value
                    .fields
                    .iter()
                    .all(|field| Self::installation_argument(&field.expr))
                    && value
                        .rest
                        .as_ref()
                        .is_none_or(|rest| Self::installation_argument(rest))
            }
            syn::Expr::Call(value) => {
                Self::installation_argument(&value.func)
                    && value.args.iter().all(Self::installation_argument)
            }
            syn::Expr::MethodCall(value) => {
                Self::installation_argument(&value.receiver)
                    && value.args.iter().all(Self::installation_argument)
            }
            _ => false,
        }
    }

    fn condition(expression: &syn::Expr, argument: &str) -> bool {
        match expression {
            syn::Expr::Lit(_) | syn::Expr::Path(_) | syn::Expr::Field(_) => true,
            syn::Expr::Unary(value) => Self::condition(&value.expr, argument),
            syn::Expr::Binary(value) => {
                Self::condition(&value.left, argument) && Self::condition(&value.right, argument)
            }
            syn::Expr::Paren(value) => Self::condition(&value.expr, argument),
            syn::Expr::Macro(value) => value.mac.path.is_ident("cfg"),
            syn::Expr::MethodCall(value) => {
                value.method == "is_plugin_added"
                    && matches!(value.receiver.as_ref(), syn::Expr::Path(path) if path.path.is_ident(argument))
            }
            _ => false,
        }
    }

    fn app(signature: &syn::Signature) -> Option<String> {
        if signature.ident != "build" || signature.inputs.len() != 2 {
            return None;
        }
        let syn::FnArg::Typed(argument) = signature.inputs.iter().nth(1)? else {
            return None;
        };
        let Type::Reference(reference) = argument.ty.as_ref() else {
            return None;
        };
        let Type::Path(path) = reference.elem.as_ref() else {
            return None;
        };
        if reference.mutability.is_none() || path.qself.is_some() {
            return None;
        }
        Some(References::path(&path.path))
    }

    fn of(
        key: String,
        name: String,
        item: &impl ToTokens,
        block: &syn::Block,
        attributes: &[Attribute],
    ) -> Self {
        let mut footprint = Footprint::default();
        footprint.visit_block(block);
        footprint.tokens(block.to_token_stream());
        let tokens = item.to_token_stream();
        let method = syn::parse2::<syn::ImplItemFn>(tokens.clone()).ok();
        let argument = method.as_ref().and_then(|method| {
            method.sig.inputs.iter().find_map(|input| {
                let syn::FnArg::Typed(input) = input else {
                    return None;
                };
                let syn::Pat::Ident(pattern) = input.pat.as_ref() else {
                    return None;
                };
                Some(pattern.ident.to_string())
            })
        });
        let composition = argument.is_some_and(|argument| Self::registration(block, &argument));
        let mut calls = Footprint::default();
        if composition {
            calls.visit_block(block);
        }
        Self {
            key,
            name,
            composition,
            bootstrap_defaults: (composition && !calls.unsupported).then_some(calls.defaults),
            bootstrap_app: method.as_ref().and_then(|method| Self::app(&method.sig)),
            returns_self: method.as_ref().is_some_and(|method| {
                matches!(&method.sig.output, syn::ReturnType::Type(_, output)
                    if matches!(output.as_ref(), Type::Path(path)
                        if path.qself.is_none() && path.path.is_ident("Self")))
            }),
            default_markers: method
                .as_ref()
                .filter(|method| method.sig.ident == "default" && method.sig.inputs.is_empty())
                .and_then(|_| Self::default_markers(block)),
            fingerprint: Declaration::fingerprint(&tokens.to_string()),
            lines: footprint.lines.len(),
            production: !Usage::test_only(attributes)
                && !Declarations::metadata(attributes, false)
                    .iter()
                    .any(|meta| meta.path().is_ident("test")),
        }
    }

    fn default_markers(block: &syn::Block) -> Option<Vec<String>> {
        if block.stmts.len() != 1 {
            return None;
        }
        let syn::Stmt::Expr(expression, None) = &block.stmts[0] else {
            return None;
        };
        let mut markers = Vec::new();
        match expression {
            syn::Expr::Path(value) if value.qself.is_none() && value.path.is_ident("Self") => {}
            syn::Expr::Call(value) if matches!(value.func.as_ref(), syn::Expr::Path(path) if path.qself.is_none() && path.path.is_ident("Self")) => {
                for argument in &value.args {
                    let syn::Expr::Path(value) = argument else {
                        return None;
                    };
                    if value.qself.is_some() {
                        return None;
                    }
                    markers.push(References::path(&value.path));
                }
            }
            syn::Expr::Struct(value)
                if value.qself.is_none() && value.path.is_ident("Self") && value.rest.is_none() =>
            {
                for field in &value.fields {
                    let syn::Expr::Path(value) = &field.expr else {
                        return None;
                    };
                    if value.qself.is_some() {
                        return None;
                    }
                    markers.push(References::path(&value.path));
                }
            }
            _ => return None,
        }
        Some(markers)
    }
}

#[derive(Default)]
struct Footprint {
    defaults: BTreeSet<String>,
    unsupported: bool,
    lines: BTreeSet<usize>,
    test_spans: Vec<((usize, usize), (usize, usize))>,
    closure: bool,
}

impl Footprint {
    fn tokens(&mut self, tokens: proc_macro2::TokenStream) {
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Group(group) => {
                    if self.excluded(group.span()) {
                        continue;
                    }
                    self.record(group.span_open());
                    self.record(group.span_close());
                    self.tokens(group.stream());
                }
                other => self.record(other.span()),
            }
        }
    }

    fn excluded(&self, span: proc_macro2::Span) -> bool {
        let start = span.start();
        let end = span.end();
        self.test_spans.iter().any(|(first, last)| {
            (start.line, start.column) >= *first && (end.line, end.column) <= *last
        })
    }

    fn record(&mut self, span: proc_macro2::Span) {
        if !self.excluded(span) {
            self.lines.extend(span.start().line..=span.end().line);
        }
    }

    fn ignore(&mut self, value: &impl syn::spanned::Spanned) {
        let span = value.span();
        let start = span.start();
        let end = span.end();
        self.test_spans
            .push(((start.line, start.column), (end.line, end.column)));
    }
}

impl<'ast> Visit<'ast> for Footprint {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref()
            && path.qself.is_none()
            && path.path.segments.len() > 1
            && path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "default")
            && call.args.is_empty()
        {
            let parts = path
                .path
                .segments
                .iter()
                .take(path.path.segments.len() - 1)
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>();
            self.defaults.insert(parts.join("::"));
        } else {
            self.unsupported = true;
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if !matches!(
            call.method.to_string().as_str(),
            "add_plugins"
                | "add_systems"
                | "add_observer"
                | "add_message"
                | "add_event"
                | "register_type"
                | "configure_sets"
                | "register_required_components"
                | "is_plugin_added"
                | "chain"
                | "after"
                | "before"
                | "in_set"
                | "run_if"
        ) {
            self.unsupported = true;
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_closure(&mut self, value: &'ast syn::ExprClosure) {
        self.closure = true;
        visit::visit_expr_closure(self, value);
    }

    fn visit_item(&mut self, value: &'ast Item) {
        if Usage::test_only(Usage::item_attributes(value)) {
            self.ignore(value);
            return;
        }
        visit::visit_item(self, value);
    }

    fn visit_expr(&mut self, value: &'ast syn::Expr) {
        if Usage::test_only(Usage::expression_attributes(value)) {
            self.ignore(value);
            return;
        }
        visit::visit_expr(self, value);
    }

    fn visit_local(&mut self, value: &'ast syn::Local) {
        if Usage::test_only(&value.attrs) {
            self.ignore(value);
            return;
        }
        visit::visit_local(self, value);
    }

    fn visit_arm(&mut self, value: &'ast syn::Arm) {
        if Usage::test_only(&value.attrs) {
            self.ignore(value);
            return;
        }
        visit::visit_arm(self, value);
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Legacy {
    key: String,
    fingerprint: String,
    issue: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Upstream {
    package: String,
    version: String,
    archive_sha256: String,
    entries: BTreeMap<String, String>,
}

impl Upstream {
    fn digest(&self) -> String {
        let mut source = format!(
            "{}\0{}\0{}",
            self.package, self.version, self.archive_sha256
        );
        for (key, fingerprint) in &self.entries {
            source.push_str(key);
            source.push('\0');
            source.push_str(fingerprint);
            source.push('\0');
        }
        Declaration::fingerprint(&source)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Boundary {
    key: String,
    #[serde(default)]
    owner: Option<String>,
    fingerprint: String,
    interface: String,
    input: String,
    output: String,
    lifetime: String,
    bounds: String,
    teardown: String,
    architecture: String,
}

impl Boundary {
    fn valid(&self) -> bool {
        for value in [
            &self.interface,
            &self.input,
            &self.output,
            &self.lifetime,
            &self.architecture,
            &self.bounds,
            &self.teardown,
        ] {
            if value.trim().is_empty() || value.contains('*') {
                return false;
            }
        }
        self.architecture == "ECS ownership boundaries"
    }

    fn references(&self, inventory: &Inventory) -> bool {
        self.references_from(inventory, &Policy::default(), &mut BTreeSet::new())
    }

    fn references_from(
        &self,
        inventory: &Inventory,
        policy: &Policy,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(self.key.clone()) {
            return false;
        }
        let Some(mut adapter) = inventory.entries.get(&self.key) else {
            return false;
        };
        if self.interface == "Rust compiler input data" {
            let path = Workspace::current()
                .0
                .join(self.key.split('|').next().unwrap());
            return self.input.starts_with("pre-world: ")
                && self.output.starts_with("pre-world: ")
                && self.owner.is_some()
                && CompilerInputs::current().owner(&path) == self.owner.as_deref();
        }
        let mut canonical_owner = None;
        if let Some(owner) = &self.owner {
            let Some(parent) = inventory.entries.get(owner) else {
                return false;
            };
            if adapter.production
                && !parent.production
                && (!matches!(adapter.kind, Kind::Script)
                    || inventory.entries.values().any(|candidate| {
                        candidate.production
                            && !candidate.included_sources.is_empty()
                            && inventory.owner_references(candidate, adapter)
                    }))
            {
                return false;
            }
            let value = match &adapter.owner {
                Some(path) => inventory.resolve(&adapter.namespace, path, &mut BTreeSet::new()),
                None => Some(adapter),
            };
            let Some(value) = value else {
                return false;
            };
            let referenced = inventory.owner_references(parent, value)
                || parent.plugin
                    && inventory.boundary_references(parent, value, &mut BTreeSet::new());
            if !referenced {
                return false;
            }
            let mut current = owner.as_str();
            let mut owners = BTreeSet::new();
            loop {
                if !owners.insert(current.to_string()) {
                    return false;
                }
                let Some(parent) = inventory.entries.get(current) else {
                    return false;
                };
                let contract = policy
                    .boundaries
                    .iter()
                    .find(|boundary| boundary.key == current);
                let Some(contract) = contract else {
                    if let Some(value) = policy.values.iter().find(|value| value.key == current) {
                        let Some(owner) = inventory.entries.get(&value.owner) else {
                            return false;
                        };
                        if value.lifetime.trim().is_empty()
                            || !inventory.owner_references(owner, parent)
                            || !inventory.value_root(owner, policy, &mut BTreeSet::new())
                        {
                            return false;
                        }
                        current = &value.owner;
                        continue;
                    }
                    if !inventory.automatic_boundary(parent)
                        && (!parent.ecs
                            || !inventory.used(parent)
                            || !inventory.canonical(parent, policy, &mut BTreeSet::new()))
                    {
                        return false;
                    }
                    canonical_owner = Some(parent.key.clone());
                    adapter = parent;
                    break;
                };
                if !contract.valid()
                    || !inventory.matches(&contract.key, &contract.fingerprint)
                    || !contract.references_from(inventory, policy, &mut visiting.clone())
                {
                    return false;
                }
                adapter = parent;
                match contract.owner.as_deref() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
        let mut canonical = false;
        let mut pre_world = 0;
        for endpoint in [&self.input, &self.output] {
            if let Some(key) = endpoint.strip_prefix("ECS: ") {
                let Some(entry) = inventory.entries.get(key) else {
                    return false;
                };
                let owned_value = policy.values.iter().any(|value| {
                    value.key == entry.key
                        && !value.lifetime.trim().is_empty()
                        && inventory.entries.get(&value.owner).is_some_and(|owner| {
                            inventory.value_root(owner, policy, &mut BTreeSet::new())
                                && inventory.owner_references(owner, entry)
                                && inventory.canonical(owner, policy, &mut BTreeSet::new())
                        })
                });
                if !(entry.contract
                    || entry.ecs && inventory.used(entry)
                    || owned_value
                    || inventory.automatic_boundary(entry))
                {
                    return false;
                }
                if canonical_owner.as_deref() != Some(entry.key.as_str())
                    && !inventory.boundary_references(adapter, entry, &mut BTreeSet::new())
                    && !self.fixture_reference(adapter, endpoint, inventory, policy, visiting)
                {
                    return false;
                }
                canonical = true;
            } else if endpoint.starts_with("pre-world: ") {
                pre_world += 1;
            } else if !endpoint.starts_with("external: ") {
                return false;
            }
        }
        if canonical {
            return pre_world == 0;
        }
        if pre_world != 2 {
            return false;
        }
        let path = self.key.split('|').next().unwrap();
        (self.interface == "Rust compiler callback and Cargo regression interface"
            && inventory.compiler_boundary(adapter))
            || (!adapter.production
                && self.interface == "Dioxus VirtualDom fixture"
                && !adapter.local
                && !adapter.dynamic
                && !adapter.opaque
                && ((adapter.renderer
                    && adapter.rendered.iter().any(|path| {
                        inventory
                            .resolve(&adapter.namespace, path, &mut BTreeSet::new())
                            .is_some_and(|child| child.production && child.renderer)
                    }))
                    || adapter.test && inventory.fixture_dom_mount(adapter)))
            || inventory.manifest_source(adapter)
            || inventory.fixture_lock(adapter)
            || path == "fixture.rs"
            || path.starts_with("crates/util/vmux_macro/")
            || matches!(
                path,
                "crates/feature/vmux_dx/tests/policy.rs"
                    | "crates/feature/vmux_dx/tests/conventions.rs"
            )
            || matches!(
                path,
                "crates/feature/vmux_dx/build.rs"
                    | "crates/feature/vmux_dx/src/source.rs"
                    | "crates/app/vmux_cli/tests/dx.rs"
            )
            || !adapter.production && path == "crates/feature/vmux_dx/src/repository.rs"
            || path.starts_with("scripts/")
            || path.starts_with("website/")
            || matches!(
                path,
                "crates/build_cfg.rs"
                    | "crates/build_cfg_git.rs"
                    | "crates/build_git_env.rs"
                    | "crates/build_platform_cfg.rs"
                    | "crates/build_product.rs"
                    | "crates/build_cfg_product.rs"
            )
    }

    fn fixture_reference(
        &self,
        adapter: &Declaration,
        endpoint: &str,
        inventory: &Inventory,
        policy: &Policy,
        visiting: &BTreeSet<String>,
    ) -> bool {
        if adapter.production {
            return false;
        }
        if let Some(key) = endpoint.strip_prefix("ECS: ")
            && let Some(entry) = inventory.entries.get(key)
        {
            for parent in inventory.entries.values() {
                if inventory.automatic_boundary(parent)
                    && inventory.owner_references(adapter, parent)
                    && inventory.boundary_references(parent, entry, &mut BTreeSet::new())
                {
                    return true;
                }
            }
        }
        for boundary in &policy.boundaries {
            if boundary.key == self.key
                || (boundary.input != endpoint && boundary.output != endpoint)
                || !inventory.matches(&boundary.key, &boundary.fingerprint)
                || !boundary.valid()
            {
                continue;
            }
            let Some(parent) = inventory.entries.get(&boundary.key) else {
                continue;
            };
            let parent = match &parent.owner {
                Some(owner) => inventory.resolve(&parent.namespace, owner, &mut BTreeSet::new()),
                None => Some(parent),
            };
            let Some(parent) = parent else {
                continue;
            };
            if inventory.owner_references(adapter, parent)
                && boundary.references_from(inventory, policy, &mut visiting.clone())
            {
                return true;
            }
        }
        false
    }

    fn fixture(inventory: &Inventory, name: &str) -> Self {
        let key = inventory.key(name);
        Self {
            owner: None,
            fingerprint: inventory.entries[&key].fingerprint.clone(),
            key,
            interface: "Rust compiler AST / test harness".into(),
            input: "pre-world: synthetic source or framework registration".into(),
            output: "pre-world: diagnostics or installed ECS system".into(),
            lifetime: "one compiler/registration fixture invocation".into(),
            bounds: "one synthetic source tree".into(),
            teardown: "all parsed metadata drops with the fixture".into(),
            architecture: "ECS ownership boundaries".into(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ValueOwner {
    key: String,
    owner: String,
    lifetime: String,
}

#[derive(Clone, Copy)]
enum Kind {
    Struct,
    Enum,
    Alias,
    Trait,
    Impl,
    Function,
    Static,
    Const,
    Macro,
    Script,
}

struct Declaration {
    key: String,
    path: String,
    namespace: String,
    name: String,
    kind: Kind,
    fingerprint: String,
    enum_shape: Option<EnumShape>,
    bodies: Vec<BodyShape>,
    ecs: bool,
    parameter: bool,
    plugin: bool,
    plugin_path: Option<String>,
    parameter_path: Option<String>,
    plugin_children: BTreeSet<String>,
    plugin_arguments: BTreeSet<(String, Vec<String>)>,
    system_arguments: BTreeSet<(String, String, Vec<String>)>,
    registered_systems: BTreeSet<(String, String)>,
    field_types: Vec<Type>,
    associated_types: BTreeMap<String, Type>,
    implemented_type: Option<Type>,
    implemented_trait: Option<String>,
    relation_target: Option<String>,
    relation_source: Option<String>,
    component_hooks: BTreeSet<String>,
    component_path: Option<String>,
    required_components: BTreeSet<String>,
    contract: bool,
    contract_paths: BTreeSet<String>,
    component_attributes: BTreeSet<String>,
    projection: bool,
    test: bool,
    production: bool,
    private: bool,
    has_fields: bool,
    unit_mutex: bool,
    owner: Option<String>,
    reference_paths: BTreeSet<String>,
    included_sources: BTreeSet<String>,
    page_inputs: BTreeSet<(String, String)>,
    page_components: BTreeSet<(String, String)>,
    page_input_errors: BTreeSet<String>,
    inventory_inputs: BTreeSet<(String, String)>,
    dom_mounts: BTreeSet<(String, String)>,
    rendered: BTreeSet<String>,
    renderer: bool,
    fields: BTreeSet<String>,
    field_paths: BTreeSet<String>,
    generic_bindings: BTreeSet<String>,
    generic_parameters: Vec<String>,
    plugin_parameters: Vec<String>,
    component_bindings: BTreeMap<String, String>,
    local: bool,
    dynamic: bool,
    opaque: bool,
}

impl Declaration {
    fn shape_source(&self) -> bool {
        self.production && !self.test && !self.path.contains("/tests/")
    }

    fn unit_mutex(value: &Type) -> bool {
        let Type::Path(value) = value else {
            return false;
        };
        if value.qself.is_some() {
            return false;
        }
        let Some(segment) = value.path.segments.last() else {
            return false;
        };
        if segment.ident != "Mutex" {
            return false;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return false;
        };
        arguments.args.len() == 1
            && matches!(arguments.args.first(), Some(syn::GenericArgument::Type(Type::Tuple(unit))) if unit.elems.is_empty())
    }

    fn fingerprint(source: &str) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in source.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }
}

type Instantiation = (String, Vec<(String, String)>);

#[derive(Default)]
struct Instantiations {
    owners: BTreeMap<String, BTreeSet<Instantiation>>,
    children: BTreeMap<Instantiation, BTreeSet<Instantiation>>,
    systems: BTreeMap<Instantiation, BTreeMap<String, BTreeSet<String>>>,
}

impl Instantiations {
    fn context(key: &str, bindings: &BTreeMap<String, syn::GenericArgument>) -> Instantiation {
        let mut values = Vec::new();
        for (name, value) in bindings {
            values.push((name.clone(), value.to_token_stream().to_string()));
        }
        (key.into(), values)
    }

    fn callbacks(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut callbacks: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for systems in self.systems.values() {
            for (function, endpoints) in systems {
                callbacks
                    .entry(function.clone())
                    .or_default()
                    .extend(endpoints.iter().cloned());
            }
        }
        callbacks
    }

    fn references(&self, owner: &str, endpoint: &str) -> bool {
        let Some(roots) = self.owners.get(owner) else {
            return false;
        };
        let mut pending: Vec<_> = roots.iter().cloned().collect();
        let mut visited = BTreeSet::new();
        while let Some(context) = pending.pop() {
            if !visited.insert(context.clone()) {
                continue;
            }
            if self
                .systems
                .get(&context)
                .is_some_and(|systems| systems.values().any(|values| values.contains(endpoint)))
            {
                return true;
            }
            if let Some(children) = self.children.get(&context) {
                pending.extend(children.iter().cloned());
            }
        }
        false
    }
}

#[derive(Default)]
struct Inventory {
    sources: BTreeMap<String, String>,
    resolved_namespaces: BTreeMap<String, BTreeSet<String>>,
    entries: BTreeMap<String, Declaration>,
    usage: BTreeMap<String, Usage>,
    names: BTreeMap<String, Vec<String>>,
    symbols: BTreeMap<String, Vec<String>>,
    imports: BTreeMap<String, Imports>,
    module_namespaces: BTreeMap<String, BTreeSet<String>>,
    used_keys: BTreeSet<String>,
    roots: BTreeSet<String>,
    functions: BTreeMap<String, Vec<String>>,
    registered_keys: BTreeSet<String>,
    instantiated_systems: BTreeMap<String, BTreeSet<String>>,
    generic_usage: Instantiations,
    lookup_depth: Cell<usize>,
    root_bindings: RefCell<BTreeMap<(String, String), Option<String>>>,
    root_namespaces: RefCell<BTreeMap<(String, String), Option<String>>>,
    root_modules: RefCell<BTreeMap<(String, String), Option<String>>>,
    root_declarations: RefCell<BTreeMap<(String, String), DeclarationLookup>>,
    owner_reference_cache: RefCell<BTreeMap<(String, String), bool>>,
    implementation_index: RefCell<Option<BTreeMap<String, Vec<String>>>>,
}

struct DeclarationLookup {
    declaration: Option<String>,
    visited: BTreeSet<String>,
}

impl Inventory {
    fn enum_job(&self, entry: &Declaration) -> bool {
        self.entries.values().any(|implementation| {
            matches!(implementation.kind, Kind::Impl)
                && implementation
                    .owner
                    .as_deref()
                    .and_then(|owner| {
                        self.resolve(&implementation.namespace, owner, &mut BTreeSet::new())
                    })
                    .is_some_and(|owner| owner.key == entry.key)
                && implementation.reference_paths.iter().any(|path| {
                    self.resolve(&implementation.namespace, path, &mut BTreeSet::new())
                        .is_some_and(|target| {
                            target.key == "crates/util/vmux_ecs/src/jobs.rs||trait:Job"
                        })
                })
        })
    }

    fn body_owner(&self, key: &str) -> Option<&Declaration> {
        if let Some(entry) = self.entries.get(key)
            && entry.bodies.iter().any(|body| body.key == key)
        {
            return Some(entry);
        }
        let (owner, _) = key.rsplit_once("::")?;
        self.entries
            .get(owner)
            .filter(|entry| entry.bodies.iter().any(|body| body.key == key))
    }

    fn shape_matches(&self, key: &str, fingerprint: &str) -> bool {
        if let Some(entry) = self.entries.get(key)
            && entry.enum_shape.is_some()
        {
            return entry.fingerprint == fingerprint;
        }
        self.body_owner(key).is_some_and(|entry| {
            entry
                .bodies
                .iter()
                .any(|body| body.key == key && body.fingerprint == fingerprint)
        })
    }

    fn plugin_names(&self) -> Vec<String> {
        let mut owned = BTreeSet::new();
        let mut namespaces = BTreeSet::new();
        for (path, usage) in &self.usage {
            if path.starts_with("crates/")
                || path.starts_with("examples/")
                || path.starts_with("website/")
                || path.starts_with("scripts/")
                || path == "fixture.rs"
            {
                owned.insert(path.clone());
                namespaces.insert(usage.namespace.clone());
            }
        }
        let mut plugins = BTreeSet::new();
        for entry in self.entries.values() {
            if let Some(path) = entry.plugin_path.as_deref()
                && self
                    .framework_handle(&entry.namespace, path, &mut BTreeSet::new())
                    .as_deref()
                    == Some("Plugin")
                && let Some(path) = entry.owner.as_deref()
                && let Some(owner) = self.resolve(&entry.namespace, path, &mut BTreeSet::new())
                && let Some(owner) = self.nominal(owner)
            {
                plugins.insert(owner.key.clone());
            }
            if entry.contract_paths.iter().any(|path| {
                self.contract_kind(&entry.namespace, path, &mut BTreeSet::new())
                    .as_deref()
                    == Some("app_plugin")
            }) {
                plugins.insert(entry.key.clone());
            }
        }
        let mut violations = BTreeSet::new();
        for key in &plugins {
            let owner = &self.entries[key];
            if owned.contains(&owner.path)
                && (owner.name == "Plugin" || !owner.name.ends_with("Plugin"))
            {
                violations.insert(format!("{}: {}", owner.path, owner.name));
            }
        }
        for alias in self.entries.values() {
            if !owned.contains(&alias.path) || !matches!(alias.kind, Kind::Alias) {
                continue;
            }
            if let Some(owner) = self.nominal(alias)
                && plugins.contains(&owner.key)
                && (alias.name == "Plugin" || !alias.name.ends_with("Plugin"))
            {
                violations.insert(format!("{}: {}", alias.path, alias.name));
            }
        }
        for (namespace, imports) in &self.imports {
            if !namespaces
                .iter()
                .any(|root| namespace == root || namespace.starts_with(&format!("{root}::")))
            {
                continue;
            }
            for name in imports.aliases.keys() {
                if name != "Plugin" && name.ends_with("Plugin") {
                    continue;
                }
                if let Some(owner) = self.resolve(namespace, name, &mut BTreeSet::new())
                    && let Some(owner) = self.nominal(owner)
                    && plugins.contains(&owner.key)
                {
                    violations.insert(format!("{namespace}: {name}"));
                }
            }
            if imports.globs.is_empty() {
                continue;
            }
            for key in &plugins {
                let name = &self.entries[key].name;
                if name != "Plugin" && name.ends_with("Plugin") {
                    continue;
                }
                if let Some(owner) = self.resolve(namespace, name, &mut BTreeSet::new())
                    && let Some(owner) = self.nominal(owner)
                    && owner.key == *key
                {
                    violations.insert(format!("{namespace}: {name}"));
                }
            }
        }
        violations.into_iter().collect()
    }

    fn nominal<'a>(&'a self, mut entry: &'a Declaration) -> Option<&'a Declaration> {
        let mut visiting = BTreeSet::new();
        while matches!(entry.kind, Kind::Alias) {
            if !visiting.insert(entry.key.clone()) {
                return None;
            }
            let Type::Path(path) = entry.field_types.first()? else {
                return None;
            };
            let path = path
                .path
                .segments
                .iter()
                .map(|part| part.ident.unraw().to_string())
                .collect::<Vec<_>>()
                .join("::");
            entry = self.resolve(&entry.namespace, &path, &mut BTreeSet::new())?;
        }
        Some(entry)
    }

    fn workspace(root: &Path) -> Self {
        let mut inventory = Self::default();
        for directory in ["crates", "examples", "website", "patches", "scripts"] {
            inventory.walk(root, &root.join(directory));
        }
        inventory.resolve_usage();
        inventory
    }

    fn walk(&mut self, root: &Path, directory: &Path) {
        let mut paths = Vec::new();
        for item in std::fs::read_dir(directory).expect("ownership source directory") {
            paths.push(item.expect("ownership directory entry").path());
        }
        paths.sort_by_key(|path| {
            let order = match path.file_name().and_then(|name| name.to_str()) {
                Some("lib.rs" | "mod.rs") => 0,
                Some("main.rs") => 1,
                _ if path.is_dir() => 3,
                _ => 2,
            };
            (order, path.clone())
        });
        for path in paths {
            if path.is_dir() {
                if matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(
                        "target" | "node_modules" | ".git" | ".worktrees" | "dist" | ".build-tools"
                    )
                ) {
                    continue;
                }
                self.walk(root, &path);
            } else if matches!(
                path.extension().and_then(|name| name.to_str()),
                Some("rs" | "js" | "sh")
            ) || path
                .strip_prefix(root)
                .is_ok_and(|relative| relative == Path::new("scripts/hooks/pre-push"))
            {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .expect("UTF-8 source path");
                let source = std::fs::read_to_string(&path).expect("read ownership source");
                if CompilerInputs::current().owner(&path).is_some() {
                    self.script(relative, &source);
                } else {
                    self.source(relative, &source);
                }
            }
        }
    }

    fn fixture(source: &str) -> Self {
        let mut inventory = Self::default();
        inventory.source("fixture.rs", source);
        inventory.resolve_usage();
        inventory
    }

    fn source(&mut self, path: &str, source: &str) {
        self.sources.insert(path.into(), source.into());
        if path.ends_with(".rs") {
            let namespace = self.namespace(path, &[]).0;
            if let Some(root) = namespace.split("::").next() {
                self.roots.insert(root.to_string());
                self.imports.entry(root.to_string()).or_default();
            }
        }
        self.root_bindings.get_mut().clear();
        self.root_namespaces.get_mut().clear();
        self.root_modules.get_mut().clear();
        self.root_declarations.get_mut().clear();
        self.owner_reference_cache.get_mut().clear();
        *self.implementation_index.get_mut() = None;
        if !path.ends_with(".rs") {
            self.script(path, source);
            return;
        }
        let file = syn::parse_file(source).unwrap_or_else(|error| panic!("{path}: {error}"));
        let mut usage = Usage {
            testing: path.contains("/tests/"),
            namespace: self.namespace(path, &[]).0,
            ..Usage::default()
        };
        usage.visit_file(&file);
        usage.visit_file(&file);
        self.usage.insert(path.to_string(), usage);
        Declarations {
            path,
            scope: Vec::new(),
            inventory: self,
            production: true,
        }
        .visit_file(&file);
        self.root_namespaces.get_mut().clear();
        self.root_modules.get_mut().clear();
        self.root_declarations.get_mut().clear();
    }

    fn namespace(&self, path: &str, scope: &[String]) -> Namespace {
        let original = Namespace::of(path, scope);
        let Some(candidates) = self
            .resolved_namespaces
            .get(path)
            .or_else(|| self.module_namespaces.get(path))
        else {
            return original;
        };
        if candidates.len() != 1 {
            return original;
        }
        let root = Namespace::of(path, &[]).0;
        let suffix = original.0.strip_prefix(&root).unwrap();
        Namespace(format!("{}{suffix}", candidates.first().unwrap()))
    }

    fn script(&mut self, path: &str, source: &str) {
        self.owner_reference_cache.get_mut().clear();
        *self.implementation_index.get_mut() = None;
        let key = format!("{path}||script");
        self.entries.insert(
            key.clone(),
            Declaration {
                key,
                path: path.into(),
                namespace: String::new(),
                name: path.into(),
                kind: Kind::Script,
                fingerprint: Declaration::fingerprint(source),
                enum_shape: None,
                bodies: Vec::new(),
                ecs: false,
                parameter: false,
                plugin: false,
                plugin_path: None,
                parameter_path: None,
                plugin_children: BTreeSet::new(),
                plugin_arguments: BTreeSet::new(),
                system_arguments: BTreeSet::new(),
                registered_systems: BTreeSet::new(),
                field_types: Vec::new(),
                associated_types: BTreeMap::new(),
                implemented_type: None,
                implemented_trait: None,
                relation_target: None,
                relation_source: None,
                component_hooks: BTreeSet::new(),
                component_path: None,
                required_components: BTreeSet::new(),
                contract: false,
                contract_paths: BTreeSet::new(),
                component_attributes: BTreeSet::new(),
                projection: false,
                test: false,
                production: true,
                private: false,
                has_fields: false,
                unit_mutex: false,
                owner: None,
                reference_paths: BTreeSet::new(),
                included_sources: BTreeSet::new(),
                page_inputs: BTreeSet::new(),
                page_components: BTreeSet::new(),
                page_input_errors: BTreeSet::new(),
                inventory_inputs: BTreeSet::new(),
                dom_mounts: BTreeSet::new(),
                rendered: BTreeSet::new(),
                renderer: false,
                fields: BTreeSet::new(),
                field_paths: BTreeSet::new(),
                generic_bindings: BTreeSet::new(),
                generic_parameters: Vec::new(),
                plugin_parameters: Vec::new(),
                component_bindings: BTreeMap::new(),
                local: false,
                dynamic: false,
                opaque: true,
            },
        );
    }

    fn key(&self, suffix: &str) -> String {
        self.entries
            .keys()
            .find(|key| key.ends_with(suffix))
            .expect("fixture declaration")
            .clone()
    }

    fn matches(&self, key: &str, fingerprint: &str) -> bool {
        self.entries
            .get(key)
            .is_some_and(|entry| entry.fingerprint == fingerprint)
    }

    fn canonical(
        &self,
        entry: &Declaration,
        policy: &Policy,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(entry.key.clone()) {
            return false;
        }
        if self.automatic_boundary(entry) {
            return true;
        }
        if policy.boundaries.iter().any(|boundary| {
            boundary.key == entry.key
                && boundary.fingerprint == entry.fingerprint
                && boundary.valid()
        }) {
            return true;
        }
        if let Some(value) = policy.values.iter().find(|value| value.key == entry.key)
            && let Some(owner) = self.entries.get(&value.owner)
            && !value.lifetime.trim().is_empty()
            && !entry.local
            && !entry.dynamic
            && !entry.opaque
            && self.owner_references(owner, entry)
        {
            return self.value_root(owner, policy, &mut BTreeSet::new());
        }
        if entry.test {
            return true;
        }
        if entry.projection && entry.ecs {
            return false;
        }
        if entry.contract && !matches!(entry.kind, Kind::Impl) {
            return true;
        }
        if entry.local || entry.dynamic || entry.opaque {
            return false;
        }
        if let Some(owner) = &entry.owner {
            let Some(declaration) = self.resolve(&entry.namespace, owner, &mut BTreeSet::new())
            else {
                return false;
            };
            if declaration.projection {
                return false;
            }
            if !declaration.ecs
                && !policy
                    .values
                    .iter()
                    .any(|value| value.key == declaration.key)
            {
                return false;
            }
            return self.canonical(declaration, policy, visiting);
        }
        let owned = match entry.kind {
            Kind::Function => {
                self.registered_keys.contains(&entry.key)
                    || self.helper_owner(entry, &mut BTreeSet::new())
            }
            Kind::Struct | Kind::Enum => entry.ecs && self.used(entry),
            Kind::Alias => {
                self.used(entry)
                    && (self.component_alias(entry, policy, &mut visiting.clone())
                        || entry.fields.iter().any(|field| {
                            matches!(
                                field.as_str(),
                                "Query"
                                    | "ParamSet"
                                    | "With"
                                    | "Without"
                                    | "Changed"
                                    | "Added"
                                    | "Or"
                                    | "Res"
                                    | "ResMut"
                                    | "Single"
                                    | "MessageReader"
                                    | "MessageWriter"
                            )
                        }))
            }
            _ => false,
        };
        if !owned {
            return false;
        }
        let mut parameters = References {
            parameter_inventory: Some(self),
            parameter_namespace: &entry.namespace,
            valid: true,
            ..Default::default()
        };
        if entry.generic_bindings.is_empty()
            && (matches!(entry.kind, Kind::Function | Kind::Alias) || entry.parameter)
        {
            for field in &entry.field_types {
                parameters.visit_type(field);
            }
        }
        for key in &parameters.outputs {
            let field = &self.entries[key];
            if field.projection
                || field.path == "crates/util/vmux_ecs/src/ui_state.rs" && field.name == "UiState"
            {
                return false;
            }
            if policy
                .legacy
                .iter()
                .any(|legacy| legacy.key == field.key && legacy.fingerprint == field.fingerprint)
                || policy
                    .upstream
                    .iter()
                    .any(|upstream| upstream.entries.get(&field.key) == Some(&field.fingerprint))
            {
                continue;
            }
            if !self.canonical(field, policy, &mut visiting.clone()) {
                return false;
            }
        }
        for reference in &entry.field_paths {
            let name = reference.rsplit("::").next().unwrap();
            if name == entry.name {
                continue;
            }
            if let Some(bound) = entry.component_bindings.get(reference)
                && self
                    .resolve(&entry.namespace, bound, &mut BTreeSet::new())
                    .is_none()
                && self.external_type(entry, bound)
            {
                continue;
            }
            match self.resolve(&entry.namespace, reference, &mut BTreeSet::new()) {
                Some(field)
                    if parameters.valid
                        && parameters.indexed.contains(&field.key)
                        && !parameters.retained.contains(&field.key) => {}
                Some(field) if field.projection => {
                    return false;
                }
                Some(field)
                    if field.path == "crates/util/vmux_ecs/src/ui_state.rs"
                        && field.name == "UiState" =>
                {
                    return false;
                }
                Some(field)
                    if (matches!(entry.kind, Kind::Function | Kind::Alias) || entry.parameter)
                        && field.ecs
                        && self.used(field)
                        && (policy.legacy.iter().any(|legacy| {
                            legacy.key == field.key && legacy.fingerprint == field.fingerprint
                        }) || policy.upstream.iter().any(|upstream| {
                            upstream.entries.get(&field.key) == Some(&field.fingerprint)
                        })) => {}
                Some(field) if !self.canonical(field, policy, &mut visiting.clone()) => {
                    return false;
                }
                Some(_) => {}
                None if self.names.contains_key(name)
                    && !self.standard_result(entry, reference)
                    && !self.external_type(entry, reference) =>
                {
                    return false;
                }
                None => {}
            }
        }
        true
    }

    fn indexed_result(&self, entry: &Declaration) -> bool {
        if entry.key != "crates/util/vmux_ecs/src/jobs.rs||struct:JobResult"
            || !entry.production
            || !entry.ecs
            || entry.generic_parameters.len() != 1
            || entry.field_types.len() != 1
        {
            return false;
        }
        let Type::Path(result) = &entry.field_types[0] else {
            return false;
        };
        if result.qself.is_some() || !self.standard_result(entry, &References::path(&result.path)) {
            return false;
        }
        let Some(segment) = result.path.segments.last() else {
            return false;
        };
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return false;
        };
        if arguments.args.len() != 2 {
            return false;
        }
        let Some(syn::GenericArgument::Type(Type::Path(output))) = arguments.args.first() else {
            return false;
        };
        let Some(syn::GenericArgument::Type(Type::Path(error))) = arguments.args.last() else {
            return false;
        };
        let error_path = References::path(&error.path);
        let native_error = error.qself.is_none()
            && match self
                .external_path(&entry.namespace, &error_path, &mut BTreeSet::new())
                .as_deref()
            {
                Some("std::string::String" | "alloc::string::String") => true,
                None => error_path == "String" && !self.local_binding(&entry.namespace, "String"),
                _ => false,
            };
        if !native_error {
            return false;
        }

        output.qself.is_none()
            && output.path.segments.len() == 2
            && output.path.segments[0].ident == entry.generic_parameters[0]
            && output.path.segments[1].ident == "Output"
    }

    fn component_alias(
        &self,
        entry: &Declaration,
        policy: &Policy,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let Some(owner) = self.nominal(entry) else {
            return false;
        };
        owner.key != entry.key
            && owner.ecs
            && self.used(owner)
            && self.canonical(owner, policy, visiting)
    }

    fn fixture_dom_mount(&self, adapter: &Declaration) -> bool {
        for (dom, component) in &adapter.dom_mounts {
            if !self.fixture_dom_type(&adapter.namespace, dom, &mut BTreeSet::new()) {
                continue;
            }
            let Some(component) =
                self.function_reference(&adapter.namespace, component, &mut BTreeSet::new())
            else {
                continue;
            };
            if component.production || !component.renderer {
                continue;
            }
            if component.rendered.iter().any(|path| {
                self.resolve(&component.namespace, path, &mut BTreeSet::new())
                    .is_some_and(|child| child.production && child.renderer)
            }) {
                return true;
            }
        }
        false
    }

    fn automatic_boundary(&self, entry: &Declaration) -> bool {
        self.framework_bootstrap(entry) || self.manifest_source(entry) || self.typed_page(entry)
    }

    fn typed_page(&self, entry: &Declaration) -> bool {
        if !matches!(entry.kind, Kind::Struct)
            || entry.has_fields
            || entry.local
            || entry.dynamic
            || entry.opaque
            || entry.projection
            || entry.page_inputs.is_empty()
            || entry.page_components.is_empty()
            || !entry.page_input_errors.is_empty()
        {
            return false;
        }
        for (attribute, component) in &entry.page_components {
            let Some(attribute) =
                self.function_reference(&entry.namespace, attribute, &mut BTreeSet::new())
            else {
                return false;
            };
            if attribute.key != "crates/util/vmux_macro/src/lib.rs||fn:page"
                || !attribute.reference_paths.contains("proc_macro_attribute")
            {
                return false;
            }
            let Some(component) =
                self.function_reference(&entry.namespace, component, &mut BTreeSet::new())
            else {
                return false;
            };
            if entry.production && !component.production
                || !component.contract_paths.iter().any(|path| {
                    self.framework_handle(&component.namespace, path, &mut BTreeSet::new())
                        .as_deref()
                        == Some("component")
                })
            {
                return false;
            }
        }
        for (_, input) in &entry.page_inputs {
            let Some(input) = self.resolve(&entry.namespace, input, &mut BTreeSet::new()) else {
                return false;
            };
            if !(input.projection || input.contract || input.ecs && self.used(input)) {
                return false;
            }
            if !self.owner_references(entry, input) {
                return false;
            }
        }
        true
    }

    fn framework_bootstrap(&self, entry: &Declaration) -> bool {
        self.framework_bootstrap_from(entry, &mut BTreeSet::new())
    }

    fn framework_bootstrap_from(
        &self,
        entry: &Declaration,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(entry.key.clone()) {
            return false;
        }
        if entry.local || entry.dynamic || entry.opaque || entry.projection {
            return false;
        }
        let owner =
            match entry.kind {
                Kind::Struct => entry,
                Kind::Impl => {
                    let Some(owner) = entry.owner.as_ref().and_then(|owner| {
                        self.resolve(&entry.namespace, owner, &mut BTreeSet::new())
                    }) else {
                        return false;
                    };
                    owner
                }
                _ => return false,
            };
        if !matches!(owner.kind, Kind::Struct)
            || !self.plugin_markers(owner)
            || !owner.production
            || owner.local
            || owner.dynamic
            || owner.opaque
            || owner.projection
        {
            return false;
        }
        if matches!(entry.kind, Kind::Impl) && self.plugin_default(entry, owner) {
            return self.framework_child_interfaces(owner, visiting);
        }
        for implementation in self.implementations(owner) {
            if matches!(entry.kind, Kind::Impl) && implementation.key != entry.key {
                continue;
            }
            let interface = implementation.plugin_path.as_ref().and_then(|path| {
                self.framework_handle(&implementation.namespace, path, &mut BTreeSet::new())
            });
            if !implementation.plugin
                || !implementation.production
                || interface.as_deref() != Some("Plugin")
            {
                continue;
            }
            if implementation.bodies.is_empty()
                || implementation.bodies.iter().any(|body| {
                    !body.production
                        || body.bootstrap_defaults.as_ref().is_none_or(|defaults| {
                            defaults.iter().any(|path| {
                                let root = self
                                    .resolve(&implementation.namespace, path, &mut BTreeSet::new())
                                    .map(|declaration| declaration.namespace.clone())
                                    .or_else(|| {
                                        self.external_path(
                                            &implementation.namespace,
                                            path,
                                            &mut BTreeSet::new(),
                                        )
                                    });
                                !root.is_some_and(|root| {
                                    matches!(
                                        root.split("::").next(),
                                        Some(
                                            "bevy"
                                                | "bevy_app"
                                                | "bevy_ecs"
                                                | "bevy_cef"
                                                | "bevy_cef_core"
                                        )
                                    )
                                }) && !self.plugin_default_path(
                                    &implementation.namespace,
                                    path,
                                    &mut visiting.clone(),
                                )
                            })
                        })
                })
            {
                continue;
            }
            if implementation.registered_systems.is_empty() {
                if self.framework_children(implementation, visiting) {
                    return true;
                }
                continue;
            }
            let registered = implementation
                .registered_systems
                .iter()
                .all(|(app, callback)| {
                    if self
                        .framework_handle(&implementation.namespace, app, &mut BTreeSet::new())
                        .as_deref()
                        != Some("App")
                    {
                        return false;
                    }
                    self.function_reference(
                        &implementation.namespace,
                        callback,
                        &mut BTreeSet::new(),
                    )
                    .is_some_and(|function| {
                        function.production
                            && function.private
                            && self.registered_keys.contains(&function.key)
                    })
                });
            if registered {
                return true;
            }
        }
        false
    }

    fn phantom_data(&self, namespace: &str, path: &str) -> bool {
        self.external_path(namespace, path, &mut BTreeSet::new())
            .is_some_and(|path| {
                matches!(
                    path.as_str(),
                    "std::marker::PhantomData" | "core::marker::PhantomData"
                )
            })
    }

    fn plugin_markers(&self, owner: &Declaration) -> bool {
        if !owner.has_fields {
            return true;
        }
        if owner.field_types.is_empty() {
            return false;
        }
        for field in &owner.field_types {
            if !self.phantom_marker_type(&owner.namespace, field, &mut BTreeSet::new()) {
                return false;
            }
        }
        true
    }

    fn phantom_marker_type(
        &self,
        namespace: &str,
        value: &Type,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let Type::Path(path) = value else {
            return false;
        };
        if path.qself.is_some() {
            return false;
        }
        let name = References::path(&path.path);
        if self.phantom_data(namespace, &name) {
            return true;
        }
        let Some(alias) = self.resolve(namespace, &name, &mut BTreeSet::new()) else {
            return false;
        };
        if !matches!(alias.kind, Kind::Alias)
            || !alias.production
            || alias.field_types.len() != 1
            || !visiting.insert(alias.key.clone())
        {
            return false;
        }
        let arguments = References::arguments(&path.path);
        let Some(bindings) = self.type_bindings(
            namespace,
            &alias.generic_parameters,
            &alias.generic_bindings,
            &arguments,
            &BTreeMap::new(),
        ) else {
            return false;
        };
        let value = Bindings::apply(self, &alias.namespace, &bindings, &alias.field_types[0]);
        self.phantom_marker_type(&alias.namespace, &value, visiting)
    }

    fn plugin_default(&self, implementation: &Declaration, owner: &Declaration) -> bool {
        if !implementation.production || !self.plugin_markers(owner) {
            return false;
        }
        let Some(interface) = &implementation.implemented_trait else {
            return false;
        };
        let interface_path =
            self.external_path(&implementation.namespace, interface, &mut BTreeSet::new());
        let standard = match interface_path.as_deref() {
            Some("std::default::Default" | "core::default::Default") => true,
            None if interface == "Default"
                && !self.local_binding(&implementation.namespace, interface) =>
            {
                true
            }
            _ => false,
        };
        if !standard || implementation.bodies.len() != 1 {
            return false;
        }
        let body = &implementation.bodies[0];
        if !body.production || !body.returns_self {
            return false;
        }
        let Some(markers) = &body.default_markers else {
            return false;
        };
        markers.len() == owner.field_types.len()
            && markers
                .iter()
                .all(|path| self.phantom_data(&implementation.namespace, path))
    }

    fn plugin_default_path(
        &self,
        namespace: &str,
        path: &str,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let Some(owner) = self.resolve(namespace, path, &mut BTreeSet::new()) else {
            return false;
        };
        let mut methods = 0;
        for implementation in self.implementations(owner) {
            if !implementation.production {
                continue;
            }
            for body in &implementation.bodies {
                if body.production && body.name == "default" {
                    methods += 1;
                }
            }
        }
        if methods != 1 {
            return false;
        }
        if !self.framework_child_interfaces(owner, visiting) {
            return false;
        }
        let defaults: Vec<_> = self
            .implementations(owner)
            .into_iter()
            .filter(|implementation| self.plugin_default(implementation, owner))
            .collect();
        defaults.len() == 1
    }

    fn framework_children(
        &self,
        implementation: &Declaration,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if implementation.plugin_children.is_empty() {
            return false;
        }
        for body in &implementation.bodies {
            let Some(app) = &body.bootstrap_app else {
                return false;
            };
            if self
                .framework_handle(&implementation.namespace, app, &mut BTreeSet::new())
                .as_deref()
                != Some("App")
            {
                return false;
            }
        }
        if !implementation.plugin_arguments.is_empty()
            && !self.instantiated_plugin_defaults(implementation, visiting)
        {
            return false;
        }
        for path in &implementation.plugin_children {
            if !self.framework_child(&implementation.namespace, path, &mut visiting.clone()) {
                return false;
            }
        }
        true
    }

    fn instantiated_plugin_defaults(
        &self,
        implementation: &Declaration,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let contexts: Vec<_> = self
            .generic_usage
            .systems
            .keys()
            .filter(|context| context.0 == implementation.key)
            .collect();
        if contexts.is_empty() {
            return false;
        }
        for (constructor, arguments) in &implementation.plugin_arguments {
            if !self.plugin_default_path(
                &implementation.namespace,
                constructor,
                &mut visiting.clone(),
            ) {
                return false;
            }
            let Some(owner) =
                self.resolve(&implementation.namespace, constructor, &mut BTreeSet::new())
            else {
                return false;
            };
            for context in &contexts {
                let mut inherited = BTreeMap::new();
                for (name, value) in &context.1 {
                    let Ok(value) = syn::parse_str::<syn::GenericArgument>(value) else {
                        return false;
                    };
                    inherited.insert(name.clone(), value);
                }
                let mut resolved = Vec::new();
                for argument in arguments {
                    let Ok(value) = syn::parse_str::<syn::GenericArgument>(argument) else {
                        return false;
                    };
                    let value =
                        Bindings::argument(self, &implementation.namespace, &inherited, &value);
                    resolved.push(value.to_token_stream().to_string());
                }
                let mut matched = 0;
                for child in self.implementations(owner) {
                    if !self.actual_plugin(child) {
                        continue;
                    }
                    let Some(values) = self.type_bindings(
                        &child.namespace,
                        &child.plugin_parameters,
                        &child.generic_bindings,
                        &resolved,
                        &inherited,
                    ) else {
                        continue;
                    };
                    let expected = Instantiations::context(&child.key, &values);
                    if !self
                        .generic_usage
                        .children
                        .get(*context)
                        .is_some_and(|children| children.contains(&expected))
                    {
                        return false;
                    }
                    matched += 1;
                }
                if matched != 1 {
                    return false;
                }
            }
        }
        true
    }

    fn framework_child_interfaces(
        &self,
        child: &Declaration,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(child.key.clone()) || !child.production {
            return false;
        }
        let mut installed = false;
        for implementation in self.implementations(child) {
            if !implementation.production
                || implementation.plugin_path.as_ref().is_none_or(|path| {
                    self.framework_handle(&implementation.namespace, path, &mut BTreeSet::new())
                        .as_deref()
                        != Some("Plugin")
                })
            {
                continue;
            }
            if !implementation.bodies.iter().any(|body| {
                body.production
                    && body.bootstrap_app.as_ref().is_some_and(|app| {
                        self.framework_handle(&implementation.namespace, app, &mut BTreeSet::new())
                            .as_deref()
                            == Some("App")
                    })
            }) {
                return false;
            }
            installed = true;
            for path in &implementation.plugin_children {
                if !self.framework_child(&implementation.namespace, path, &mut visiting.clone()) {
                    return false;
                }
            }
        }
        installed
    }

    fn framework_child(
        &self,
        namespace: &str,
        path: &str,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if let Some(child) = self.resolve(namespace, path, &mut BTreeSet::new()) {
            return matches!(child.kind, Kind::Struct)
                && child.production
                && !child.local
                && !child.dynamic
                && self.framework_child_interfaces(child, visiting);
        }
        if let Some((owner, "default")) = path.rsplit_once("::")
            && self
                .resolve(namespace, owner, &mut BTreeSet::new())
                .is_some()
        {
            return self.plugin_default_path(namespace, owner, visiting);
        }
        self.external_path(namespace, path, &mut BTreeSet::new())
            .is_some_and(|path| {
                matches!(
                    path.split("::").next(),
                    Some("bevy" | "bevy_app" | "bevy_ecs" | "bevy_cef" | "bevy_cef_core")
                )
            })
    }

    fn fixture_dom_type(
        &self,
        namespace: &str,
        path: &str,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(format!("dom:{namespace}|{path}"))
            || self
                .resolve(namespace, path, &mut BTreeSet::new())
                .is_some()
        {
            return false;
        }
        if matches!(
            path,
            "dioxus::core::VirtualDom" | "dioxus::prelude::VirtualDom" | "dioxus_core::VirtualDom"
        ) {
            let root = path.split("::").next().unwrap();
            return self.root_binding(namespace, root).as_deref() == Some(root)
                && self.external_root(namespace, root);
        }
        let mut scope = Some(namespace);
        while let Some(namespace) = scope {
            if let Some(imports) = self.imports.get(namespace) {
                if imports.ambiguous.contains(path)
                    || imports.modules.contains(path)
                    || imports.opaque_modules.contains(path)
                {
                    return false;
                }
                if let Some(target) = imports.aliases.get(path) {
                    return self.fixture_dom_type(namespace, target, visiting);
                }
            }
            scope = namespace.rsplit_once("::").map(|(parent, _)| parent);
        }
        false
    }

    fn compiler_boundary(&self, adapter: &Declaration) -> bool {
        if matches!(adapter.kind, Kind::Macro) {
            let name = adapter.name.replace(' ', "");
            return self
                .external_path(&adapter.namespace, &name, &mut BTreeSet::new())
                .as_deref()
                == Some("dylint_linting::impl_late_lint");
        }
        let source = match &adapter.owner {
            Some(owner) => self.resolve(&adapter.namespace, owner, &mut BTreeSet::new()),
            None => Some(adapter),
        };
        let Some(source) = source else {
            return false;
        };
        if !matches!(source.kind, Kind::Struct) || source.dynamic || source.local || source.opaque {
            return false;
        }
        let mut cargo = false;
        let mut temporary = false;
        for field in &source.field_paths {
            if self
                .external_path(&source.namespace, field, &mut BTreeSet::new())
                .as_deref()
                == Some("tempfile::TempDir")
            {
                temporary = true;
            }
        }
        for implementation in self.entries.values() {
            let Some(owner) = &implementation.owner else {
                continue;
            };
            if self
                .resolve(&implementation.namespace, owner, &mut BTreeSet::new())
                .is_none_or(|owner| owner.key != source.key)
            {
                continue;
            }
            if let Some((interface, _)) = implementation.name.split_once(" for ") {
                let interface = interface.split('<').next().unwrap().replace(' ', "");
                if matches!(
                    self.external_path(&implementation.namespace, &interface, &mut BTreeSet::new())
                        .as_deref(),
                    Some(
                        "rustc_lint::LateLintPass"
                            | "rustc_hir::intravisit::Visitor"
                            | "syn::visit::Visit"
                    )
                ) {
                    return true;
                }
            }
            for reference in &implementation.reference_paths {
                if self
                    .external_path(&implementation.namespace, reference, &mut BTreeSet::new())
                    .as_deref()
                    == Some("std::process::Command::new")
                {
                    cargo = true;
                }
            }
        }
        if source.production || !temporary || !cargo {
            return false;
        }
        self.entries.values().any(|test| {
            test.test
                && test
                    .included_sources
                    .iter()
                    .any(|input| input.ends_with(".rs"))
                && self.owner_references(test, source)
        })
    }

    fn external_path(
        &self,
        namespace: &str,
        path: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        if !visiting.insert(format!("{namespace}|{path}"))
            || self
                .resolve(namespace, path, &mut BTreeSet::new())
                .is_some()
        {
            return None;
        }
        let (root, suffix) = path.split_once("::").unwrap_or((path, ""));
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if let Some(imports) = self.imports.get(current) {
                if imports.ambiguous.contains(root)
                    || imports.modules.contains(root)
                    || imports.opaque_modules.contains(root)
                {
                    return None;
                }
                if let Some(target) = imports.aliases.get(root)
                    && target != root
                {
                    let target = if suffix.is_empty() {
                        target.clone()
                    } else {
                        format!("{target}::{suffix}")
                    };
                    return self.external_path(current, &target, visiting);
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        if !path.contains("::") || !self.external_root(namespace, root) {
            return None;
        }
        Some(path.into())
    }

    fn fixture_lock(&self, adapter: &Declaration) -> bool {
        !adapter.production
            && matches!(adapter.kind, Kind::Static)
            && adapter.unit_mutex
            && adapter.fields == BTreeSet::from(["Mutex".into()])
            && adapter.field_paths == BTreeSet::from(["std::sync::Mutex".into()])
            && !self.local_binding(&adapter.namespace, "std")
    }

    fn manifest_source(&self, adapter: &Declaration) -> bool {
        if !matches!(adapter.kind, Kind::Struct | Kind::Impl)
            || adapter.local
            || adapter.dynamic
            || adapter.projection
        {
            return false;
        }
        let source = match &adapter.owner {
            Some(path) => self.resolve(&adapter.namespace, path, &mut BTreeSet::new()),
            None => Some(adapter),
        };
        let Some(source) = source else {
            return false;
        };
        if !matches!(source.kind, Kind::Struct) || source.has_fields {
            return false;
        }
        for implementation in self.implementations(source) {
            if implementation.production != source.production || !implementation.bodies.is_empty() {
                continue;
            }
            if matches!(adapter.kind, Kind::Impl) && implementation.key != adapter.key {
                continue;
            }
            let Some((interface, _)) = implementation.name.split_once(" for ") else {
                continue;
            };
            if self
                .resolve(
                    &implementation.namespace,
                    &interface.replace(' ', ""),
                    &mut BTreeSet::new(),
                )
                .is_some_and(|interface| {
                    interface.key
                        == "crates/feature/vmux_feature/src/manifest.rs||trait:FeatureManifestSource"
                })
            {
                return true;
            }
        }
        false
    }

    fn value_root(
        &self,
        owner: &Declaration,
        policy: &Policy,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(owner.key.clone()) {
            return false;
        }
        if owner.local || owner.dynamic || owner.opaque {
            return false;
        }
        if owner.ecs {
            return self.used(owner);
        }
        let Some(value) = policy.values.iter().find(|value| value.key == owner.key) else {
            return false;
        };
        let Some(parent) = self.entries.get(&value.owner) else {
            return false;
        };
        self.owner_references(parent, owner) && self.value_root(parent, policy, visiting)
    }

    fn helper_owner(&self, entry: &Declaration, visiting: &mut BTreeSet<String>) -> bool {
        if !entry.private || !visiting.insert(entry.key.clone()) {
            return false;
        }
        let mut names = BTreeSet::from([entry.name.clone()]);
        loop {
            let before = names.len();
            for imports in self.imports.values() {
                for (alias, target) in &imports.aliases {
                    if names.contains(target.rsplit("::").next().unwrap()) {
                        names.insert(alias.clone());
                    }
                }
            }
            if before == names.len() {
                break;
            }
        }
        let mut owner = None;
        for caller in self.entries.values() {
            if !caller.production {
                continue;
            }
            let mut referenced = false;
            for path in &caller.reference_paths {
                if !names.contains(path.rsplit("::").next().unwrap()) {
                    continue;
                }
                if self
                    .function_reference(&caller.namespace, path, &mut BTreeSet::new())
                    .is_some_and(|target| target.key == entry.key)
                {
                    referenced = true;
                    break;
                }
            }
            if !referenced {
                continue;
            }
            if owner.is_some() || !matches!(caller.kind, Kind::Function) {
                return false;
            }
            owner = Some(caller);
        }
        let Some(owner) = owner else {
            return false;
        };
        self.registered_keys.contains(&owner.key) || self.helper_owner(owner, visiting)
    }

    fn function_reference(
        &self,
        namespace: &str,
        reference: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<&Declaration> {
        if let Some(reference) = reference.strip_prefix("::") {
            let (root, reference) = reference.split_once("::")?;
            if !self.roots.contains(root) && !self.imports.contains_key(root) {
                return None;
            }
            return self.function_reference(root, reference, visiting);
        }
        if visiting.len() > 128 || !visiting.insert(format!("function:{namespace}|{reference}")) {
            return None;
        }
        if let Some((prefix, name)) = reference.rsplit_once("::") {
            let module = self.glob_namespace(namespace, prefix, &mut BTreeSet::new())?;
            return self.function_reference(&module, name, visiting);
        }
        let symbol = format!("{namespace}::{reference}");
        if let Some(keys) = self.functions.get(&symbol) {
            if keys.len() == 1 {
                return self.entries.get(&keys[0]);
            }
            return None;
        }
        let imports = self.imports.get(namespace)?;
        if imports.ambiguous.contains(reference) {
            return None;
        }
        if let Some(target) = imports.aliases.get(reference) {
            return self.function_reference(namespace, target, visiting);
        }
        let mut found: Option<&Declaration> = None;
        for prefix in &imports.globs {
            let Some(module) = self.glob_namespace(namespace, prefix, &mut BTreeSet::new()) else {
                continue;
            };
            if let Some(target) = self.function_reference(&module, reference, visiting) {
                if found.is_some_and(|previous| previous.key != target.key) {
                    return None;
                }
                found = Some(target);
            }
        }
        found
    }

    fn used(&self, entry: &Declaration) -> bool {
        self.used_keys.contains(&entry.key)
    }

    fn required_component(&self, owner: &Declaration, value: &Declaration) -> bool {
        if !self.used(owner) || !self.used(value) {
            return false;
        }
        for component in [owner, value] {
            let Some(path) = &component.component_path else {
                return false;
            };
            if self
                .framework_handle(&component.namespace, path, &mut BTreeSet::new())
                .as_deref()
                != Some("Component")
            {
                return false;
            }
        }
        for path in &owner.required_components {
            if self
                .resolve(&owner.namespace, path, &mut BTreeSet::new())
                .is_some_and(|required| required.key == value.key)
            {
                return true;
            }
        }
        false
    }

    fn owner_references(&self, owner: &Declaration, value: &Declaration) -> bool {
        let key = (owner.key.clone(), value.key.clone());
        if let Some(referenced) = self.owner_reference_cache.borrow().get(&key) {
            return *referenced;
        }
        let referenced = self.references_of(owner, value);
        self.owner_reference_cache
            .borrow_mut()
            .insert(key, referenced);
        referenced
    }

    fn references_of(&self, owner: &Declaration, value: &Declaration) -> bool {
        let mut candidates = vec![owner];
        for implementation in self.implementations(owner) {
            if owner.production && !implementation.production {
                continue;
            }
            candidates.push(implementation);
        }
        for candidate in candidates {
            if self.inventory_value(candidate, value) {
                return true;
            }
            for (attribute, input) in &candidate.page_inputs {
                let Some(attribute) =
                    self.function_reference(&candidate.namespace, attribute, &mut BTreeSet::new())
                else {
                    continue;
                };
                if attribute.key == "crates/util/vmux_macro/src/lib.rs||fn:page"
                    && attribute.reference_paths.contains("proc_macro_attribute")
                    && self
                        .resolve(&candidate.namespace, input, &mut BTreeSet::new())
                        .is_some_and(|input| input.key == value.key)
                {
                    return true;
                }
            }
            if matches!(value.kind, Kind::Script) {
                if candidate.included_sources.is_empty() {
                    continue;
                }
                let shadowed = self.local_binding(&candidate.namespace, "include_str")
                    || self.entries.values().any(|declaration| {
                        matches!(declaration.kind, Kind::Macro)
                            && declaration.name == "include_str"
                            && (candidate.namespace == declaration.namespace
                                || candidate
                                    .namespace
                                    .strip_prefix(&declaration.namespace)
                                    .is_some_and(|suffix| suffix.starts_with("::")))
                    });
                if shadowed {
                    continue;
                }
                for source in &candidate.included_sources {
                    if Path::new(source).is_absolute() {
                        continue;
                    }
                    let mut path = PathBuf::new();
                    let mut valid = true;
                    let included = Path::new(&candidate.path).parent().unwrap().join(source);
                    for part in included.components() {
                        match part {
                            std::path::Component::Normal(part) => path.push(part),
                            std::path::Component::ParentDir => {
                                if !path.pop() {
                                    valid = false;
                                    break;
                                }
                            }
                            std::path::Component::CurDir => {}
                            _ => {
                                valid = false;
                                break;
                            }
                        }
                    }
                    if valid && path == Path::new(&value.path) {
                        return true;
                    }
                }
                continue;
            }
            if value.renderer {
                for path in &candidate.rendered {
                    if self
                        .resolve(&candidate.namespace, path, &mut BTreeSet::new())
                        .is_some_and(|resolved| resolved.key == value.key)
                    {
                        return true;
                    }
                }
                continue;
            }
            for path in &candidate.reference_paths {
                let mut path = path.as_str();
                loop {
                    if let Some(resolved) = self
                        .resolve(&candidate.namespace, path, &mut BTreeSet::new())
                        .or_else(|| {
                            self.function_reference(
                                &candidate.namespace,
                                path,
                                &mut BTreeSet::new(),
                            )
                        })
                    {
                        if resolved.key == value.key {
                            return true;
                        }
                        break;
                    }
                    let Some((prefix, _)) = path.rsplit_once("::") else {
                        break;
                    };
                    path = prefix;
                }
            }
        }
        false
    }

    fn implementations(&self, owner: &Declaration) -> Vec<&Declaration> {
        if self.implementation_index.borrow().is_none() {
            let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for implementation in self.entries.values() {
                let Some(path) = &implementation.owner else {
                    continue;
                };
                let Some(resolved) =
                    self.resolve(&implementation.namespace, path, &mut BTreeSet::new())
                else {
                    continue;
                };
                index
                    .entry(resolved.key.clone())
                    .or_default()
                    .push(implementation.key.clone());
            }
            *self.implementation_index.borrow_mut() = Some(index);
        }
        let index = self.implementation_index.borrow();
        let mut implementations = Vec::new();
        if let Some(keys) = index.as_ref().unwrap().get(&owner.key) {
            for key in keys {
                implementations.push(&self.entries[key]);
            }
        }
        implementations
    }

    fn inventory_value(&self, adapter: &Declaration, value: &Declaration) -> bool {
        for (interface, input) in &adapter.inventory_inputs {
            let Some(interface) =
                self.external_path(&adapter.namespace, interface, &mut BTreeSet::new())
            else {
                continue;
            };
            if !matches!(
                interface.as_str(),
                "inventory::collect" | "inventory::submit"
            ) {
                continue;
            }
            if self
                .resolve(&adapter.namespace, input, &mut BTreeSet::new())
                .is_some_and(|input| input.key == value.key)
            {
                return true;
            }
        }
        false
    }

    fn boundary_references(
        &self,
        adapter: &Declaration,
        endpoint: &Declaration,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let owner = match &adapter.owner {
            Some(path)
                if matches!(adapter.kind, Kind::Impl)
                    && self
                        .framework_handle(&adapter.namespace, path, &mut BTreeSet::new())
                        .as_deref()
                        == Some("App") =>
            {
                Some(adapter)
            }
            Some(path) => self.resolve(&adapter.namespace, path, &mut BTreeSet::new()),
            None => Some(adapter),
        };
        let Some(owner) = owner else {
            return false;
        };
        if !visiting.insert(owner.key.clone()) {
            return false;
        }
        if self.owner_references(owner, endpoint) {
            return true;
        }
        if self.generic_usage.references(&owner.key, &endpoint.key) {
            return true;
        }
        if (owner.parameter
            || matches!(owner.kind, Kind::Function) && self.registered_keys.contains(&owner.key))
            && self.borrowed_endpoint(owner, endpoint)
        {
            return true;
        }
        for child in &owner.plugin_children {
            if let Some(child) = self.resolve(&owner.namespace, child, &mut BTreeSet::new())
                && !self.typed_child(owner, child)
                && self.boundary_references(child, endpoint, &mut visiting.clone())
            {
                return true;
            }
        }
        for implementation in self.implementations(owner) {
            if !implementation.plugin || owner.production && !implementation.production {
                continue;
            }
            let actual_plugin = implementation
                .plugin_path
                .as_deref()
                .and_then(|path| {
                    self.framework_handle(&implementation.namespace, path, &mut BTreeSet::new())
                })
                .as_deref()
                == Some("Plugin");
            if actual_plugin {
                for (app_type, callback) in &implementation.registered_systems {
                    if self
                        .framework_handle(&implementation.namespace, app_type, &mut BTreeSet::new())
                        .as_deref()
                        != Some("App")
                    {
                        continue;
                    }
                    let Some(function) = self.function_reference(
                        &implementation.namespace,
                        callback,
                        &mut BTreeSet::new(),
                    ) else {
                        continue;
                    };
                    if function.production
                        && function.private
                        && self.registered_keys.contains(&function.key)
                        && (self.owner_references(function, endpoint)
                            || self.direct_endpoint(function, endpoint))
                    {
                        return true;
                    }
                }
            }
            for child in &implementation.plugin_children {
                let mut path = child.as_str();
                loop {
                    if let Some(child) =
                        self.resolve(&implementation.namespace, path, &mut BTreeSet::new())
                    {
                        if !self.typed_child(implementation, child)
                            && self.boundary_references(child, endpoint, &mut visiting.clone())
                        {
                            return true;
                        }
                        break;
                    }
                    let Some((prefix, _)) = path.rsplit_once("::") else {
                        break;
                    };
                    path = prefix;
                }
            }
        }
        false
    }

    fn typed_child(&self, parent: &Declaration, child: &Declaration) -> bool {
        parent.plugin_arguments.iter().any(|(path, _)| {
            self.resolve(&parent.namespace, path, &mut BTreeSet::new())
                .is_some_and(|value| value.key == child.key)
        })
    }

    fn framework_handle(
        &self,
        namespace: &str,
        reference: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        if !visiting.insert(format!("handle:{namespace}|{reference}"))
            || self
                .resolve(namespace, reference, &mut BTreeSet::new())
                .is_some()
        {
            return None;
        }
        let parts = reference.split("::").collect::<Vec<_>>();
        if parts.len() > 1 {
            let root = self.root_binding(namespace, parts[0])?;
            if !self.external_root(namespace, parts[0]) {
                return None;
            }
            let suffix = parts[1..].join("::");
            let canonical = format!("{root}::{suffix}");
            let (prefix, name) = canonical.rsplit_once("::")?;
            let valid = match name {
                "component" => matches!(prefix, "dioxus" | "dioxus::prelude" | "dioxus_core_macro"),
                "App" | "Plugin" => matches!(
                    prefix,
                    "bevy_app" | "bevy_app::prelude" | "bevy::app" | "bevy::prelude"
                ),
                "Query" | "Single" | "Res" | "ResMut" | "NonSend" | "NonSendMut" | "ParamSet"
                | "SystemParam" | "SystemState" => {
                    matches!(
                        prefix,
                        "bevy_ecs::system"
                            | "bevy_ecs::prelude"
                            | "bevy::ecs::system"
                            | "bevy::ecs::prelude"
                            | "bevy::prelude"
                    )
                }
                "Component" => matches!(
                    prefix,
                    "bevy_ecs::component"
                        | "bevy_ecs::prelude"
                        | "bevy_ecs_macros"
                        | "bevy::ecs::component"
                        | "bevy::ecs::prelude"
                        | "bevy::prelude"
                ),
                "On" => matches!(
                    prefix,
                    "bevy_ecs::observer"
                        | "bevy_ecs::prelude"
                        | "bevy::ecs::observer"
                        | "bevy::ecs::prelude"
                        | "bevy::prelude"
                ),
                "MessageReader" | "MessageWriter" | "MessageMutator" => matches!(
                    prefix,
                    "bevy_ecs::message"
                        | "bevy_ecs::prelude"
                        | "bevy::ecs::message"
                        | "bevy::ecs::prelude"
                        | "bevy::prelude"
                ),
                "With" | "Without" | "Added" | "Changed" | "Or" | "Has" | "QueryData" => matches!(
                    prefix,
                    "bevy_ecs::query"
                        | "bevy_ecs::prelude"
                        | "bevy::ecs::query"
                        | "bevy::ecs::prelude"
                        | "bevy::prelude"
                ),
                "Ref" | "Mut" => matches!(
                    prefix,
                    "bevy_ecs::change_detection"
                        | "bevy_ecs::prelude"
                        | "bevy::ecs::change_detection"
                        | "bevy::ecs::prelude"
                        | "bevy::prelude"
                ),
                "Option" => matches!(
                    prefix,
                    "std::option" | "core::option" | "std::prelude::v1" | "core::prelude::v1"
                ),
                _ => false,
            };
            return valid.then(|| name.to_string());
        }
        let mut found = None;
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if let Some(imports) = self.imports.get(current) {
                if imports.ambiguous.contains(reference)
                    || imports.modules.contains(reference)
                    || imports.opaque_modules.contains(reference)
                {
                    return None;
                }
                if let Some(target) = imports.aliases.get(reference) {
                    return self.framework_handle(current, target, visiting);
                }
                for prefix in &imports.globs {
                    let target = format!("{prefix}::{reference}");
                    if let Some(kind) =
                        self.framework_handle(current, &target, &mut visiting.clone())
                    {
                        if found.as_ref().is_some_and(|existing| *existing != kind) {
                            return None;
                        }
                        found = Some(kind);
                    } else if !matches!(
                        prefix.as_str(),
                        "bevy::prelude"
                            | "bevy::ecs::prelude"
                            | "bevy_ecs::prelude"
                            | "bevy_app::prelude"
                    ) {
                        if reference == "Option"
                            && !self.local_binding(namespace, "Option")
                            && self
                                .glob_namespace(current, prefix, &mut BTreeSet::new())
                                .is_some()
                        {
                            continue;
                        }
                        return None;
                    }
                }
            }
            if found.is_some() {
                return found;
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        if found.is_none() && reference == "Option" && !self.local_binding(namespace, "Option") {
            return Some("Option".into());
        }
        found
    }

    fn borrowed_type(
        &self,
        namespace: &str,
        value: &Type,
        endpoint: &Declaration,
        inside: bool,
        direct: bool,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        match value {
            Type::Reference(value) => {
                self.borrowed_type(namespace, &value.elem, endpoint, inside, inside, visiting)
            }
            Type::Paren(value) => {
                self.borrowed_type(namespace, &value.elem, endpoint, inside, direct, visiting)
            }
            Type::Group(value) => {
                self.borrowed_type(namespace, &value.elem, endpoint, inside, direct, visiting)
            }
            Type::Tuple(value) => value.elems.iter().any(|value| {
                self.borrowed_type(
                    namespace,
                    value,
                    endpoint,
                    inside,
                    direct,
                    &mut visiting.clone(),
                )
            }),
            Type::Path(value) if value.qself.is_none() => {
                let reference = value
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.unraw().to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if let Some(entry) = self.resolve(namespace, &reference, &mut BTreeSet::new()) {
                    if inside && direct && entry.key == endpoint.key {
                        return true;
                    }
                    let parameter = entry.parameter
                        && entry
                            .parameter_path
                            .as_deref()
                            .and_then(|path| {
                                self.framework_handle(&entry.namespace, path, &mut BTreeSet::new())
                            })
                            .is_some_and(|kind| {
                                matches!(kind.as_str(), "SystemParam" | "QueryData")
                            });
                    if (parameter || matches!(entry.kind, Kind::Alias))
                        && self.used(entry)
                        && entry.production
                        && visiting.insert(entry.key.clone())
                    {
                        let arguments = References::arguments(&value.path);
                        let Some(bindings) = self.type_bindings(
                            namespace,
                            &entry.generic_parameters,
                            &entry.generic_bindings,
                            &arguments,
                            &BTreeMap::new(),
                        ) else {
                            return false;
                        };
                        return entry.field_types.iter().any(|value| {
                            let value = Bindings::apply(self, &entry.namespace, &bindings, value);
                            self.borrowed_type(
                                &entry.namespace,
                                &value,
                                endpoint,
                                inside,
                                direct,
                                &mut visiting.clone(),
                            )
                        });
                    }
                    return false;
                }
                let Some(handle) =
                    self.framework_handle(namespace, &reference, &mut BTreeSet::new())
                else {
                    return false;
                };
                let allowed = match handle.as_str() {
                    "Query" | "Single" | "Res" | "ResMut" | "NonSend" | "NonSendMut" | "On"
                    | "SystemState" | "MessageReader" | "MessageWriter" | "MessageMutator" => true,
                    "With" | "Without" | "Added" | "Changed" | "Or" | "Has" | "Ref" | "Mut" => {
                        inside
                    }
                    "Option" => inside,
                    "ParamSet" => false,
                    _ => return false,
                };
                if !allowed && handle != "ParamSet" {
                    return false;
                }
                let Some(segment) = value.path.segments.last() else {
                    return false;
                };
                let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                    return false;
                };
                let component = if handle == "Option" {
                    direct
                } else {
                    !matches!(
                        handle.as_str(),
                        "Query" | "Single" | "ParamSet" | "SystemState"
                    )
                };
                for argument in &arguments.args {
                    let syn::GenericArgument::Type(value) = argument else {
                        continue;
                    };
                    if self.borrowed_type(
                        namespace,
                        value,
                        endpoint,
                        allowed,
                        component,
                        &mut visiting.clone(),
                    ) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn borrowed_endpoint(&self, source: &Declaration, endpoint: &Declaration) -> bool {
        if self
            .instantiated_systems
            .get(&source.key)
            .is_some_and(|values| values.contains(&endpoint.key))
        {
            return true;
        }
        self.direct_endpoint(source, endpoint)
    }

    fn direct_endpoint(&self, source: &Declaration, endpoint: &Declaration) -> bool {
        for value in &source.field_types {
            if self.borrowed_type(
                &source.namespace,
                value,
                endpoint,
                false,
                false,
                &mut BTreeSet::new(),
            ) {
                return true;
            }
        }
        if matches!(source.kind, Kind::Function) && self.registered_keys.contains(&source.key) {
            for usage in self.usage.values() {
                for (function, namespace, state, reference) in &usage.states {
                    if self
                        .resolve_function(function, &mut BTreeSet::new())
                        .is_none_or(|entry| entry.key != source.key)
                        || !self.system_state(namespace, state, &mut BTreeSet::new())
                    {
                        continue;
                    }
                    let Some(parameter) = self.resolve(namespace, reference, &mut BTreeSet::new())
                    else {
                        continue;
                    };
                    if !parameter.parameter
                        || !parameter.production
                        || !self.used(parameter)
                        || parameter
                            .parameter_path
                            .as_deref()
                            .and_then(|path| {
                                self.framework_handle(
                                    &parameter.namespace,
                                    path,
                                    &mut BTreeSet::new(),
                                )
                            })
                            .as_deref()
                            != Some("SystemParam")
                    {
                        continue;
                    }
                    for value in &parameter.field_types {
                        if self.borrowed_type(
                            &parameter.namespace,
                            value,
                            endpoint,
                            false,
                            false,
                            &mut BTreeSet::new(),
                        ) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn system_state(&self, namespace: &str, path: &str, visiting: &mut BTreeSet<String>) -> bool {
        if !visiting.insert(format!("{namespace}|{path}"))
            || self
                .resolve(namespace, path, &mut BTreeSet::new())
                .is_some()
        {
            return false;
        }
        if matches!(
            path,
            "bevy_ecs::system::SystemState"
                | "bevy::ecs::system::SystemState"
                | "bevy::prelude::SystemState"
        ) {
            let root = path.split("::").next().unwrap();
            return self.root_binding(namespace, root).as_deref() == Some(root)
                && self.external_root(namespace, root);
        }
        if path.contains("::") {
            return false;
        }
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if let Some(imports) = self.imports.get(current) {
                if imports.ambiguous.contains(path) || imports.opaque_modules.contains(path) {
                    return false;
                }
                if let Some(target) = imports.aliases.get(path) {
                    return self.system_state(current, target, visiting);
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        false
    }

    fn contract_kind(
        &self,
        namespace: &str,
        path: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        if !visiting.insert(format!("{namespace}|{path}")) {
            return None;
        }
        let parts: Vec<_> = path.split("::").collect();
        if parts.len() == 1 {
            let mut scope = Some(namespace);
            while let Some(current) = scope {
                if let Some(imports) = self.imports.get(current) {
                    if imports.ambiguous.contains(path) || imports.opaque_modules.contains(path) {
                        return None;
                    }
                    if let Some(target) = imports.aliases.get(path) {
                        return self.contract_kind(current, target, visiting);
                    }
                }
                scope = current.rsplit_once("::").map(|(parent, _)| parent);
            }
            return None;
        }
        if parts.len() != 2
            || !matches!(
                parts[1],
                "ui_event"
                    | "ui_state"
                    | "ui_state_patch"
                    | "contract"
                    | "agent"
                    | "string_id"
                    | "app_plugin"
                    | "service_message"
                    | "ui_event_variants"
            )
        {
            return None;
        }
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if let Some(imports) = self.imports.get(current)
                && (imports.opaque_modules.contains(parts[0])
                    || imports.ambiguous.contains(parts[0]))
            {
                return None;
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        let root = if parts[0] == "crate" {
            namespace.split("::").next()?.to_string()
        } else {
            self.root_binding(namespace, parts[0])?
        };
        if root != "vmux_api" && root != "vmux_macro" {
            return None;
        }
        if parts[1] == "app_plugin" && root != "vmux_macro" {
            return None;
        }
        Some(parts[1].into())
    }

    fn standard_result(&self, entry: &Declaration, reference: &str) -> bool {
        let mut parts = Vec::new();
        for part in reference.split("::") {
            parts.push(part.strip_prefix("r#").unwrap_or(part));
        }
        let reference = parts.join("::");
        let reference = reference.as_str();
        if entry.key.split('|').nth(1).is_some_and(|scope| {
            scope
                .split("::")
                .any(|part| part.starts_with("fn ") || part.starts_with("impl "))
        }) {
            return false;
        }
        let namespace = entry.namespace.as_str();
        if entry
            .generic_bindings
            .contains(reference.split("::").next().unwrap())
        {
            return false;
        }
        if matches!(
            reference,
            "std::result::Result" | "core::result::Result" | "std::io::Result"
        ) {
            return !self.local_binding(namespace, reference.split("::").next().unwrap());
        }
        if reference == "io::Result" {
            let mut scope = Some(namespace);
            while let Some(current) = scope {
                if self.symbols.contains_key(&format!("{current}::io")) {
                    return false;
                }
                if let Some(imports) = self.imports.get(current) {
                    if imports.ambiguous.contains("io") || imports.opaque_modules.contains("io") {
                        return false;
                    }
                    if let Some(target) = imports.aliases.get("io") {
                        return target == "std::io" && !self.local_binding(current, "std");
                    }
                    if self.exported_binding(current, "io", &mut BTreeSet::new()) {
                        return false;
                    }
                }
                scope = current.rsplit_once("::").map(|(parent, _)| parent);
            }
            return false;
        }
        if reference != "Result" {
            return false;
        }
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if self.symbols.contains_key(&format!("{current}::Result"))
                || self.symbols.contains_key(&format!("{current}::r#Result"))
            {
                return false;
            }
            if let Some(imports) = self.imports.get(current) {
                if imports.ambiguous.contains("Result") || imports.modules.contains("Result") {
                    return false;
                }
                if let Some(target) = imports.aliases.get("Result") {
                    return matches!(
                        target.as_str(),
                        "std::result::Result" | "core::result::Result" | "std::io::Result"
                    ) && !self.local_binding(current, target.split("::").next().unwrap());
                }
                for prefix in &imports.globs {
                    let root = prefix.split("::").next().unwrap();
                    if matches!(root, "crate" | "self" | "super")
                        || self.roots.contains(root)
                        || self.local_binding(current, root)
                    {
                        return false;
                    }
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        true
    }

    fn external_root(&self, namespace: &str, root: &str) -> bool {
        if matches!(root, "crate" | "self" | "super") {
            return false;
        }
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if let Some(imports) = self.imports.get(current) {
                if imports.opaque_modules.contains(root) || imports.ambiguous.contains(root) {
                    return false;
                }
                if imports.modules.contains(root)
                    && self
                        .module_binding(current, root, &mut BTreeSet::new())
                        .is_none()
                {
                    return false;
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        let Some(binding) = self.root_binding(namespace, root) else {
            return false;
        };
        !self.roots.contains(binding.split("::").next().unwrap())
    }

    fn external_type(&self, entry: &Declaration, reference: &str) -> bool {
        let first = reference.split("::").next().unwrap();
        if reference.rsplit("::").next() == Some("Result")
            || entry.generic_bindings.contains(first)
            || entry.key.split('|').nth(1).is_some_and(|scope| {
                scope
                    .split("::")
                    .any(|part| part.starts_with("fn ") || part.starts_with("impl "))
            })
        {
            return false;
        }
        if reference.contains("::") {
            return self.external_root(&entry.namespace, first);
        }
        let mut external = false;
        let mut scope = Some(entry.namespace.as_str());
        while let Some(namespace) = scope {
            if self
                .symbols
                .contains_key(&format!("{namespace}::{reference}"))
            {
                return false;
            }
            if let Some(imports) = self.imports.get(namespace) {
                if imports.ambiguous.contains(reference)
                    || imports.modules.contains(reference)
                    || imports.opaque_modules.contains(reference)
                {
                    return false;
                }
                if let Some(target) = imports.aliases.get(reference) {
                    return self.external_root(namespace, target.split("::").next().unwrap());
                }
                for prefix in &imports.globs {
                    if !self.external_root(namespace, prefix.split("::").next().unwrap()) {
                        return false;
                    }
                    external = true;
                }
            }
            scope = namespace.rsplit_once("::").map(|(parent, _)| parent);
        }
        external
    }

    fn local_binding(&self, namespace: &str, name: &str) -> bool {
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if self.symbols.contains_key(&format!("{current}::{name}"))
                || self.symbols.contains_key(&format!("{current}::r#{name}"))
            {
                return true;
            }
            if let Some(imports) = self.imports.get(current)
                && (imports.aliases.contains_key(name) || imports.modules.contains(name))
            {
                return true;
            }
            if self.exported_binding(current, name, &mut BTreeSet::new()) {
                return true;
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        false
    }

    fn exported_binding(
        &self,
        namespace: &str,
        name: &str,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(format!("binding:{namespace}|{name}")) {
            return false;
        }
        if self.symbols.contains_key(&format!("{namespace}::{name}"))
            || self.symbols.contains_key(&format!("{namespace}::r#{name}"))
        {
            return true;
        }
        let Some(imports) = self.imports.get(namespace) else {
            return false;
        };
        if imports.aliases.contains_key(name) || imports.modules.contains(name) {
            return true;
        }
        for prefix in &imports.globs {
            let Some(source) = self.glob_namespace(namespace, prefix, &mut visiting.clone()) else {
                let first = prefix.split("::").next().unwrap();
                if matches!(first, "crate" | "self" | "super")
                    || self.roots.contains(first)
                    || self.exported_binding(namespace, first, visiting)
                {
                    return true;
                }
                continue;
            };
            if self.exported_binding(&source, name, visiting) {
                return true;
            }
        }
        false
    }

    fn glob_namespace(
        &self,
        namespace: &str,
        prefix: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        if !visiting.is_empty() {
            return self.lookup_namespace(namespace, prefix, visiting);
        }
        let key = (namespace.to_string(), prefix.to_string());
        if let Some(result) = self.root_namespaces.borrow().get(&key) {
            return result.clone();
        }
        let result = self.lookup_namespace(namespace, prefix, visiting);
        self.root_namespaces
            .borrow_mut()
            .insert(key, result.clone());
        result
    }

    fn lookup_namespace(
        &self,
        namespace: &str,
        prefix: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        let mut scoped = visiting.clone();
        let visiting = &mut scoped;
        if !visiting.insert(format!("path:{namespace}|{prefix}")) {
            return None;
        }
        let mut parts = prefix.split("::");
        let first = parts.next()?;
        let mut current = match first {
            "crate" => namespace.split("::").next()?.to_string(),
            "self" => namespace.to_string(),
            "super" => namespace.rsplit_once("::")?.0.to_string(),
            _ => {
                let mut scope = Some(namespace);
                let mut found = None;
                while let Some(parent) = scope {
                    if let Some(module) = self.module_binding(parent, first, visiting) {
                        found = Some(module);
                        break;
                    }
                    if self.exported_binding(parent, first, &mut visiting.clone()) {
                        return None;
                    }
                    scope = parent.rsplit_once("::").map(|(parent, _)| parent);
                }
                match found {
                    Some(module) => module,
                    None if self.roots.contains(first) => first.to_string(),
                    None => return None,
                }
            }
        };
        for part in parts {
            if part == "super" {
                current = current.rsplit_once("::")?.0.to_string();
            } else {
                current = self.module_binding(&current, part, visiting)?;
            }
        }
        Some(current)
    }

    fn module_binding(
        &self,
        namespace: &str,
        name: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        if !visiting.is_empty() {
            return self.lookup_module(namespace, name, visiting);
        }
        let key = (namespace.to_string(), name.to_string());
        if let Some(result) = self.root_modules.borrow().get(&key) {
            return result.clone();
        }
        let result = self.lookup_module(namespace, name, visiting);
        self.root_modules.borrow_mut().insert(key, result.clone());
        result
    }

    fn lookup_module(
        &self,
        namespace: &str,
        name: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        let mut scoped = visiting.clone();
        let visiting = &mut scoped;
        if !visiting.insert(format!("module:{namespace}|{name}")) {
            return None;
        }
        let imports = self.imports.get(namespace)?;
        if imports.ambiguous.contains(name) || imports.opaque_modules.contains(name) {
            return None;
        }
        if imports.modules.contains(name) {
            if let Some(paths) = imports.module_sources.get(name) {
                let mut bound = None;
                for path in paths {
                    let Some(source) = self.usage.get(path) else {
                        continue;
                    };
                    let candidates = self.module_namespaces.get(path)?;
                    if candidates.len() != 1
                        || candidates.first() != Some(&source.namespace)
                        || bound.is_some()
                    {
                        return None;
                    }
                    bound = Some(source.namespace.clone());
                }
                return bound;
            }
            return Some(format!("{namespace}::{name}"));
        }
        if let Some(target) = imports.aliases.get(name) {
            return self.glob_namespace(namespace, target, visiting);
        }
        let mut found = None;
        for prefix in &imports.globs {
            let Some(source) = self.glob_namespace(namespace, prefix, &mut visiting.clone()) else {
                continue;
            };
            if let Some(module) = self.module_binding(&source, name, visiting) {
                if found.as_ref().is_some_and(|previous| *previous != module) {
                    return None;
                }
                found = Some(module);
            }
        }
        found
    }

    fn resolve_usage(&mut self) {
        self.root_namespaces.get_mut().clear();
        self.root_modules.get_mut().clear();
        self.root_declarations.get_mut().clear();
        if self.usage.iter().any(|(path, usage)| {
            path.ends_with(".rs") && usage.namespace != self.namespace(path, &[]).0
        }) {
            let sources = std::mem::take(&mut self.sources);
            let mut bindings = std::mem::take(&mut self.module_namespaces);
            for _ in 0..=sources.len() {
                let mut inventory = Self {
                    resolved_namespaces: bindings.clone(),
                    ..Default::default()
                };
                for (path, source) in &sources {
                    inventory.source(path, source);
                }
                let stable = inventory.module_namespaces == bindings;
                bindings = inventory.module_namespaces.clone();
                *self = inventory;
                if stable {
                    break;
                }
            }
        }
        self.owner_reference_cache.get_mut().clear();
        *self.implementation_index.get_mut() = None;
        let mut keys = BTreeSet::new();
        let mut registered = BTreeSet::new();
        for usage in self.usage.values() {
            for symbol in &usage.systems {
                if let Some(entry) = self.resolve_function(symbol, &mut BTreeSet::new()) {
                    if !entry.production {
                        continue;
                    }
                    registered.insert(entry.key.clone());
                    for parameter in &entry.field_paths {
                        if let Some(value) =
                            self.resolve(&entry.namespace, parameter, &mut BTreeSet::new())
                        {
                            keys.insert(value.key.clone());
                        }
                    }
                }
            }
            for (namespace, reference) in &usage.references {
                let mut path = reference.as_str();
                loop {
                    if let Some(entry) = self.resolve(namespace, path, &mut BTreeSet::new()) {
                        keys.insert(entry.key.clone());
                        break;
                    }
                    let Some((prefix, _)) = path.rsplit_once("::") else {
                        break;
                    };
                    path = prefix;
                }
            }
        }
        for usage in self.usage.values() {
            for (function, namespace, state, reference) in &usage.states {
                if self
                    .resolve_function(function, &mut BTreeSet::new())
                    .is_some_and(|entry| registered.contains(&entry.key))
                    && self.system_state(namespace, state, &mut BTreeSet::new())
                    && let Some(entry) = self.resolve(namespace, reference, &mut BTreeSet::new())
                {
                    keys.insert(entry.key.clone());
                }
            }
        }
        let mut pending: Vec<_> = keys.iter().cloned().collect();
        while let Some(key) = pending.pop() {
            let entry = &self.entries[&key];
            if !entry.parameter && !matches!(entry.kind, Kind::Alias) {
                continue;
            }
            for path in &entry.field_paths {
                if let Some(field) = self.resolve(&entry.namespace, path, &mut BTreeSet::new())
                    && keys.insert(field.key.clone())
                {
                    pending.push(field.key.clone());
                }
            }
        }
        for entry in self.entries.values_mut() {
            entry.contract = false;
            entry.projection = false;
        }
        let mut contracts = Vec::new();
        for entry in self.entries.values() {
            for path in &entry.contract_paths {
                if let Some(kind) = self.contract_kind(&entry.namespace, path, &mut BTreeSet::new())
                {
                    contracts.push((entry.key.clone(), kind));
                }
            }
        }
        for (key, kind) in contracts {
            let entry = self.entries.get_mut(&key).unwrap();
            if kind == "app_plugin" {
                entry.plugin = true;
                entry.plugin_children.extend(entry.field_paths.clone());
            } else if kind == "service_message" {
                entry.ecs = true;
            } else {
                entry.contract = true;
                entry.projection |= matches!(kind.as_str(), "ui_state" | "ui_state_patch");
            }
        }
        let mut emitted_components = Vec::new();
        for entry in self.entries.values() {
            for attribute in &entry.component_attributes {
                let Some(attribute) =
                    self.function_reference(&entry.namespace, attribute, &mut BTreeSet::new())
                else {
                    continue;
                };
                if attribute.key == "crates/util/vmux_macro/src/lib.rs||fn:command"
                    && attribute.reference_paths.contains("proc_macro_attribute")
                {
                    emitted_components.push(entry.key.clone());
                }
            }
        }
        for key in emitted_components {
            self.entries.get_mut(&key).unwrap().ecs = true;
        }
        self.used_keys = keys.clone();
        self.registered_keys = registered.clone();
        self.generic_usage = self.generic_system_usage();
        self.instantiated_systems = self.generic_usage.callbacks();
        for (system, endpoints) in &self.instantiated_systems {
            registered.insert(system.clone());
            keys.extend(endpoints.iter().cloned());
        }
        for source in self.entries.values() {
            if !source.ecs || !keys.contains(&source.key) {
                continue;
            }
            let Some(path) = &source.relation_target else {
                continue;
            };
            let Some(target) = self.resolve(&source.namespace, path, &mut BTreeSet::new()) else {
                continue;
            };
            let Some(inverse) = &target.relation_source else {
                continue;
            };
            if target.ecs
                && self
                    .resolve(&target.namespace, inverse, &mut BTreeSet::new())
                    .is_some_and(|inverse| inverse.key == source.key)
            {
                keys.insert(target.key.clone());
            }
        }
        for component in self.entries.values() {
            if !component.ecs || !keys.contains(&component.key) {
                continue;
            }
            for path in &component.component_hooks {
                if let Some(hook) =
                    self.function_reference(&component.namespace, path, &mut BTreeSet::new())
                    && hook.production
                {
                    registered.insert(hook.key.clone());
                }
            }
        }
        for key in &registered {
            let Some(function) = self.entries.get(key) else {
                continue;
            };
            if !function.generic_parameters.is_empty() {
                continue;
            }
            for parameter in &function.field_types {
                keys.extend(self.job_output_components(&function.namespace, parameter));
            }
        }
        self.used_keys = keys;
        self.registered_keys = registered;
        self.root_namespaces.get_mut().clear();
        self.root_modules.get_mut().clear();
        self.root_declarations.get_mut().clear();
    }

    fn actual_plugin(&self, implementation: &Declaration) -> bool {
        implementation.production
            && implementation.plugin
            && implementation
                .plugin_path
                .as_deref()
                .and_then(|path| {
                    self.framework_handle(&implementation.namespace, path, &mut BTreeSet::new())
                })
                .as_deref()
                == Some("Plugin")
    }

    fn type_bindings(
        &self,
        namespace: &str,
        patterns: &[String],
        variables: &BTreeSet<String>,
        arguments: &[String],
        inherited: &BTreeMap<String, syn::GenericArgument>,
    ) -> Option<BTreeMap<String, syn::GenericArgument>> {
        if patterns.len() != arguments.len() {
            return None;
        }
        let mut bindings = BTreeMap::new();
        for (pattern, argument) in patterns.iter().zip(arguments) {
            let pattern = syn::parse_str::<syn::GenericArgument>(pattern).ok()?;
            let argument = syn::parse_str::<syn::GenericArgument>(argument).ok()?;
            let argument = Bindings::argument(self, namespace, inherited, &argument);
            if !self.bind_argument(namespace, &pattern, &argument, variables, &mut bindings) {
                return None;
            }
        }
        Some(bindings)
    }

    fn bind_argument(
        &self,
        namespace: &str,
        pattern: &syn::GenericArgument,
        argument: &syn::GenericArgument,
        variables: &BTreeSet<String>,
        bindings: &mut BTreeMap<String, syn::GenericArgument>,
    ) -> bool {
        let variable = match pattern {
            syn::GenericArgument::Type(Type::Path(value)) if value.qself.is_none() => {
                value.path.get_ident()
            }
            syn::GenericArgument::Const(syn::Expr::Path(value)) if value.qself.is_none() => {
                value.path.get_ident()
            }
            _ => None,
        };
        if let Some(variable) = variable
            && variables.contains(&variable.unraw().to_string())
        {
            let key = variable.unraw().to_string();
            if let Some(previous) = bindings.get(&key) {
                return previous.to_token_stream().to_string()
                    == argument.to_token_stream().to_string();
            }
            bindings.insert(key, argument.clone());
            return true;
        }
        match (pattern, argument) {
            (syn::GenericArgument::Type(pattern), syn::GenericArgument::Type(argument)) => {
                self.bind_type(namespace, pattern, argument, variables, bindings)
            }
            (syn::GenericArgument::Const(pattern), syn::GenericArgument::Const(argument)) => {
                pattern.to_token_stream().to_string() == argument.to_token_stream().to_string()
            }
            _ => false,
        }
    }

    fn bind_type(
        &self,
        namespace: &str,
        pattern: &Type,
        argument: &Type,
        variables: &BTreeSet<String>,
        bindings: &mut BTreeMap<String, syn::GenericArgument>,
    ) -> bool {
        match (pattern, argument) {
            (Type::Path(pattern), Type::Path(argument))
                if pattern.qself.is_none() && argument.qself.is_none() =>
            {
                let mut expected = pattern.clone();
                let mut actual = argument.clone();
                for segment in &mut expected.path.segments {
                    segment.arguments = syn::PathArguments::None;
                }
                for segment in &mut actual.path.segments {
                    segment.arguments = syn::PathArguments::None;
                }
                let expected =
                    Bindings::apply(self, namespace, &BTreeMap::new(), &Type::Path(expected));
                let actual =
                    Bindings::apply(self, namespace, &BTreeMap::new(), &Type::Path(actual));
                if expected.to_token_stream().to_string() != actual.to_token_stream().to_string() {
                    return false;
                }
                let patterns = References::arguments(&pattern.path);
                let arguments = References::arguments(&argument.path);
                if patterns.len() != arguments.len() {
                    return false;
                }
                for (pattern, argument) in patterns.iter().zip(arguments) {
                    let (Ok(pattern), Ok(argument)) =
                        (syn::parse_str(pattern), syn::parse_str(&argument))
                    else {
                        return false;
                    };
                    if !self.bind_argument(namespace, &pattern, &argument, variables, bindings) {
                        return false;
                    }
                }
                true
            }
            (Type::Reference(pattern), Type::Reference(argument))
                if pattern.mutability == argument.mutability =>
            {
                self.bind_argument(
                    namespace,
                    &syn::GenericArgument::Type((*pattern.elem).clone()),
                    &syn::GenericArgument::Type((*argument.elem).clone()),
                    variables,
                    bindings,
                )
            }
            (Type::Tuple(pattern), Type::Tuple(argument))
                if pattern.elems.len() == argument.elems.len() =>
            {
                for (pattern, argument) in pattern.elems.iter().zip(&argument.elems) {
                    if !self.bind_argument(
                        namespace,
                        &syn::GenericArgument::Type(pattern.clone()),
                        &syn::GenericArgument::Type(argument.clone()),
                        variables,
                        bindings,
                    ) {
                        return false;
                    }
                }
                true
            }
            _ => pattern.to_token_stream().to_string() == argument.to_token_stream().to_string(),
        }
    }

    fn generic_system_endpoints(
        &self,
        implementation: &Declaration,
        bindings: &BTreeMap<String, syn::GenericArgument>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut systems: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (app, callback, arguments) in &implementation.system_arguments {
            if self
                .framework_handle(&implementation.namespace, app, &mut BTreeSet::new())
                .as_deref()
                != Some("App")
            {
                continue;
            }
            let Some(function) =
                self.function_reference(&implementation.namespace, callback, &mut BTreeSet::new())
            else {
                continue;
            };
            if !function.production || !function.private || function.generic_parameters.is_empty() {
                continue;
            }
            let Some(values) = self.type_bindings(
                &implementation.namespace,
                &function.generic_parameters,
                &function.generic_bindings,
                arguments,
                bindings,
            ) else {
                continue;
            };
            let mut references = References::default();
            for value in values.values() {
                references.visit_generic_argument(value);
            }
            let mut endpoints = self.parameter_endpoints(function);
            for path in references.paths {
                let Some(endpoint) =
                    self.resolve(&implementation.namespace, &path, &mut BTreeSet::new())
                else {
                    continue;
                };
                if !endpoint.ecs {
                    continue;
                }
                endpoints.insert(endpoint.key.clone());
            }
            for parameter in &function.field_types {
                let parameter = Bindings::apply(self, &function.namespace, &values, parameter);
                let outputs = self.job_output_components(&function.namespace, &parameter);
                if !outputs.is_empty() {
                    systems
                        .entry(function.key.clone())
                        .or_default()
                        .extend(outputs);
                }
            }
            for endpoint in endpoints {
                let endpoint = &self.entries[&endpoint];
                for parameter in &function.field_types {
                    let parameter = Bindings::apply(self, &function.namespace, &values, parameter);
                    if self.borrowed_type(
                        &function.namespace,
                        &parameter,
                        endpoint,
                        false,
                        false,
                        &mut BTreeSet::new(),
                    ) {
                        systems
                            .entry(function.key.clone())
                            .or_default()
                            .insert(endpoint.key.clone());
                        break;
                    }
                }
            }
        }
        systems
    }

    fn job_output_components(&self, namespace: &str, parameter: &Type) -> BTreeSet<String> {
        let Some(result) = self
            .entries
            .get("crates/util/vmux_ecs/src/jobs.rs||struct:JobResult")
        else {
            return BTreeSet::new();
        };
        if !result.ecs
            || !self.borrowed_type(
                namespace,
                parameter,
                result,
                false,
                false,
                &mut BTreeSet::new(),
            )
        {
            return BTreeSet::new();
        }
        let mut jobs = JobOutputs {
            inventory: self,
            namespace,
            jobs: BTreeSet::new(),
            visited: BTreeSet::new(),
            inside: false,
            direct: false,
        };
        jobs.visit_type(parameter);
        let mut outputs = BTreeSet::new();
        for (_, concrete) in jobs.jobs {
            let Ok(concrete) = syn::parse_str::<Type>(&concrete) else {
                continue;
            };
            if let Some((implementation, output)) = self.job_output(namespace, &concrete) {
                outputs.extend(self.job_output_values(
                    implementation,
                    &output,
                    &mut BTreeSet::new(),
                ));
            }
        }
        outputs
    }

    fn job_output<'a>(&'a self, namespace: &str, value: &Type) -> Option<(&'a Declaration, Type)> {
        let (key, concrete) = self.concrete_job(namespace, value, &mut BTreeSet::new())?;
        let job = &self.entries[&key];
        let mut matched = None;
        for implementation in self.implementations(job) {
            if !implementation.production
                || !implementation
                    .implemented_trait
                    .as_ref()
                    .is_some_and(|interface| {
                        self.resolve(&implementation.namespace, interface, &mut BTreeSet::new())
                            .is_some_and(|interface| {
                                interface.key == "crates/util/vmux_ecs/src/jobs.rs||trait:Job"
                            })
                    })
            {
                continue;
            }
            let Some(pattern) = &implementation.implemented_type else {
                continue;
            };
            let mut bindings = BTreeMap::new();
            if !self.bind_type(
                &implementation.namespace,
                pattern,
                &concrete,
                &implementation.generic_bindings,
                &mut bindings,
            ) {
                continue;
            }
            if matched.is_some() {
                return None;
            }
            matched = Some((implementation, bindings));
        }
        let (implementation, mut bindings) = matched?;
        bindings.insert("Self".into(), syn::GenericArgument::Type(concrete));
        let output = implementation.associated_types.get("Output")?;
        let output = Bindings::apply(self, &implementation.namespace, &bindings, output);
        Some((implementation, output))
    }

    fn job_output_values(
        &self,
        source: &Declaration,
        value: &Type,
        visiting: &mut BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut outputs = BTreeSet::new();
        let Type::Path(path) = value else {
            return outputs;
        };
        if path.qself.is_some() {
            return outputs;
        }
        let name = References::path(&path.path);
        if let Some(owner) = self.resolve(&source.namespace, &name, &mut BTreeSet::new()) {
            if !owner.production || owner.projection {
                return outputs;
            }
            if owner.component_path.as_deref().is_some_and(|path| {
                self.framework_handle(&owner.namespace, path, &mut BTreeSet::new())
                    .as_deref()
                    == Some("Component")
            }) {
                outputs.insert(owner.key.clone());
            } else if matches!(owner.kind, Kind::Alias)
                && visiting.insert(owner.key.clone())
                && owner.field_types.len() == 1
            {
                let arguments = References::arguments(&path.path);
                if let Some(bindings) = self.type_bindings(
                    &source.namespace,
                    &owner.generic_parameters,
                    &owner.generic_bindings,
                    &arguments,
                    &BTreeMap::new(),
                ) {
                    let value =
                        Bindings::apply(self, &owner.namespace, &bindings, &owner.field_types[0]);
                    outputs.extend(self.job_output_values(owner, &value, visiting));
                }
            }
            return outputs;
        }
        let option = self
            .framework_handle(&source.namespace, &name, &mut BTreeSet::new())
            .as_deref()
            == Some("Option");
        if !option && !self.standard_result(source, &name) {
            return outputs;
        }
        if let Some(segment) = path.path.segments.last()
            && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
        {
            for argument in &arguments.args {
                if let syn::GenericArgument::Type(value) = argument {
                    outputs.extend(self.job_output_values(source, value, &mut visiting.clone()));
                }
            }
        }
        outputs
    }

    fn concrete_job(
        &self,
        namespace: &str,
        value: &Type,
        visiting: &mut BTreeSet<String>,
    ) -> Option<(String, Type)> {
        let value = Bindings::apply(self, namespace, &BTreeMap::new(), value);
        let Type::Path(path) = &value else {
            return None;
        };
        if path.qself.is_some() {
            return None;
        }
        let owner = self.resolve(
            namespace,
            &References::path(&path.path),
            &mut BTreeSet::new(),
        )?;
        if !owner.production {
            return None;
        }
        if !matches!(owner.kind, Kind::Alias) {
            return Some((owner.key.clone(), value));
        }
        if !visiting.insert(owner.key.clone()) || owner.field_types.len() != 1 {
            return None;
        }
        let arguments = References::arguments(&path.path);
        let bindings = self.type_bindings(
            namespace,
            &owner.generic_parameters,
            &owner.generic_bindings,
            &arguments,
            &BTreeMap::new(),
        )?;
        let value = Bindings::apply(self, &owner.namespace, &bindings, &owner.field_types[0]);
        self.concrete_job(&owner.namespace, &value, visiting)
    }

    fn parameter_endpoints(&self, function: &Declaration) -> BTreeSet<String> {
        let mut endpoints = BTreeSet::new();
        let mut pending = Vec::new();
        for path in &function.field_paths {
            if !function
                .generic_bindings
                .contains(path.split("::").next().unwrap())
            {
                pending.push((function.namespace.clone(), path.clone()));
            }
        }
        let mut visited = BTreeSet::new();
        while let Some((namespace, path)) = pending.pop() {
            let Some(entry) = self.resolve(&namespace, &path, &mut BTreeSet::new()) else {
                continue;
            };
            if entry.ecs {
                endpoints.insert(entry.key.clone());
            }
            if (entry.parameter || matches!(entry.kind, Kind::Alias))
                && visited.insert(entry.key.clone())
            {
                for path in &entry.field_paths {
                    if !entry
                        .generic_bindings
                        .contains(path.split("::").next().unwrap())
                    {
                        pending.push((entry.namespace.clone(), path.clone()));
                    }
                }
            }
        }
        endpoints
    }

    fn generic_system_usage(&self) -> Instantiations {
        let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut pending: Vec<(String, BTreeMap<String, syn::GenericArgument>)> = Vec::new();
        for implementation in self.entries.values() {
            if !self.actual_plugin(implementation) {
                continue;
            }
            let Some(owner) = implementation.owner.as_deref().and_then(|path| {
                self.resolve(&implementation.namespace, path, &mut BTreeSet::new())
            }) else {
                continue;
            };
            index
                .entry(owner.key.clone())
                .or_default()
                .push(implementation.key.clone());
            if implementation.generic_parameters.is_empty() {
                pending.push((implementation.key.clone(), BTreeMap::new()));
            }
        }
        let mut usage = Instantiations::default();
        while let Some((key, bindings)) = pending.pop() {
            let context = Instantiations::context(&key, &bindings);
            if usage.systems.contains_key(&context) {
                continue;
            }
            let implementation = &self.entries[&key];
            usage.owners.entry(key).or_default().insert(context.clone());
            if let Some(owner) = implementation.owner.as_deref().and_then(|path| {
                self.resolve(&implementation.namespace, path, &mut BTreeSet::new())
            }) {
                usage
                    .owners
                    .entry(owner.key.clone())
                    .or_default()
                    .insert(context.clone());
            }
            usage.systems.insert(
                context.clone(),
                self.generic_system_endpoints(implementation, &bindings),
            );
            for (constructor, arguments) in &implementation.plugin_arguments {
                let Some(owner) =
                    self.resolve(&implementation.namespace, constructor, &mut BTreeSet::new())
                else {
                    continue;
                };
                let Some(children) = index.get(&owner.key) else {
                    continue;
                };
                let mut resolved = Vec::new();
                for argument in arguments {
                    let Ok(value) = syn::parse_str::<syn::GenericArgument>(argument) else {
                        continue;
                    };
                    let value =
                        Bindings::argument(self, &implementation.namespace, &bindings, &value);
                    resolved.push(value.to_token_stream().to_string());
                }
                if resolved.len() != arguments.len() {
                    continue;
                }
                for key in children {
                    let child = &self.entries[key];
                    let Some(values) = self.type_bindings(
                        &child.namespace,
                        &child.plugin_parameters,
                        &child.generic_bindings,
                        &resolved,
                        &bindings,
                    ) else {
                        continue;
                    };
                    let child_context = Instantiations::context(key, &values);
                    usage
                        .children
                        .entry(context.clone())
                        .or_default()
                        .insert(child_context);
                    pending.push((key.clone(), values));
                }
            }
        }
        usage
    }

    fn resolve_function(
        &self,
        symbol: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<&Declaration> {
        if !visiting.insert(symbol.into()) {
            return None;
        }
        if let Some(keys) = self.functions.get(symbol) {
            if keys.len() == 1 {
                return self.entries.get(&keys[0]);
            }
            return None;
        }
        let parts: Vec<_> = symbol.split("::").collect();
        for index in (1..parts.len()).rev() {
            let namespace = parts[..index].join("::");
            if self.imports.contains_key(&namespace) || index == 1 && self.roots.contains(parts[0])
            {
                return self.function_reference(&namespace, &parts[index..].join("::"), visiting);
            }
        }
        None
    }

    fn resolve(
        &self,
        namespace: &str,
        reference: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<&Declaration> {
        if !visiting.is_empty() || self.lookup_depth.get() != 0 {
            return self.lookup_declaration(namespace, reference, visiting);
        }
        let key = (namespace.to_string(), reference.to_string());
        if let Some(result) = self.root_declarations.borrow().get(&key) {
            visiting.extend(result.visited.iter().cloned());
            return result
                .declaration
                .as_ref()
                .and_then(|key| self.entries.get(key));
        }
        let declaration = self.lookup_declaration(namespace, reference, visiting);
        self.root_declarations.borrow_mut().insert(
            key,
            DeclarationLookup {
                declaration: declaration.map(|entry| entry.key.clone()),
                visited: visiting.clone(),
            },
        );
        declaration
    }

    fn lookup_declaration(
        &self,
        namespace: &str,
        reference: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<&Declaration> {
        let reference = reference
            .split("::")
            .map(|part| part.strip_prefix("r#").unwrap_or(part))
            .collect::<Vec<_>>()
            .join("::");
        let reference = reference.as_str();
        let mut scope = namespace;
        while let Some((parent, name)) = scope.rsplit_once("::") {
            if self
                .imports
                .get(parent)
                .is_some_and(|imports| imports.modules.contains(name))
                && self
                    .module_binding(parent, name, &mut BTreeSet::new())
                    .as_deref()
                    != Some(scope)
            {
                return None;
            }
            scope = parent;
        }
        let marker = format!("{namespace}|{reference}");
        let depth = self.lookup_depth.get();
        if depth > 128 || !visiting.insert(marker) {
            return None;
        }
        self.lookup_depth.set(depth + 1);
        let entry = self.resolve_path(namespace, reference, visiting);
        self.lookup_depth.set(depth);
        entry
    }

    fn resolve_path(
        &self,
        namespace: &str,
        reference: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<&Declaration> {
        let parts = reference.split("::").collect::<Vec<_>>();
        let first = parts.first().copied()?;
        if first == "bevy"
            && parts.get(1) == Some(&"window")
            && !self.local_binding(namespace, "bevy")
        {
            return self.resolve("bevy_window", &parts[2..].join("::"), visiting);
        }
        if first == "crate" {
            let root = namespace.split("::").next()?;
            return self.resolve(root, &parts[1..].join("::"), visiting);
        }
        if first == "self" {
            return self.resolve(namespace, &parts[1..].join("::"), visiting);
        }
        if first == "super" {
            let parent = namespace.rsplit_once("::")?.0;
            return self.resolve(parent, &parts[1..].join("::"), visiting);
        }
        if parts.len() > 1 && self.roots.contains(first) {
            let shadowed = self.names.contains_key(first)
                || self.imports.iter().any(|(scope, imports)| {
                    imports.modules.contains(first)
                        || imports
                            .aliases
                            .get(first)
                            .is_some_and(|target| target != "self" || scope != first)
                });
            let module = if shadowed {
                self.root_binding(namespace, first)?
            } else {
                first.to_string()
            };
            return self.resolve(&module, &parts[1..].join("::"), visiting);
        }
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if self.imports.get(current).is_some_and(|imports| {
                imports.ambiguous.contains(first) || imports.opaque_modules.contains(first)
            }) {
                return None;
            }
            if parts.len() > 1
                && self
                    .imports
                    .get(current)
                    .is_some_and(|imports| imports.modules.contains(first))
            {
                let module = self.module_binding(current, first, visiting)?;
                return self.resolve(&module, &parts[1..].join("::"), visiting);
            }
            let symbol = format!("{current}::{reference}");
            if self.symbols.contains_key(&symbol) {
                return self.symbol(&symbol, visiting);
            }
            if let Some(entry) = self.symbol(&symbol, visiting) {
                return Some(entry);
            }
            if let Some(imports) = self.imports.get(current) {
                if imports.ambiguous.contains(first) || imports.opaque_modules.contains(first) {
                    return None;
                }
                if parts.len() > 1 && imports.modules.contains(first) {
                    let module = self.module_binding(current, first, visiting)?;
                    return self.resolve(&module, &parts[1..].join("::"), visiting);
                }
                if let Some(target) = imports.aliases.get(first) {
                    let suffix = parts[1..].join("::");
                    let target = if suffix.is_empty() {
                        target.clone()
                    } else {
                        format!("{target}::{suffix}")
                    };
                    return self.resolve(current, &target, visiting);
                }
                if parts.len() > 1 {
                    scope = current.rsplit_once("::").map(|(parent, _)| parent);
                    continue;
                }
                let mut found: Option<&Declaration> = None;
                for prefix in &imports.globs {
                    if let Some(entry) =
                        self.resolve(current, &format!("{prefix}::{reference}"), visiting)
                    {
                        if found.is_some_and(|previous| previous.key != entry.key) {
                            return None;
                        }
                        found = Some(entry);
                    }
                }
                if found.is_some() {
                    return found;
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        self.symbol(reference, visiting)
    }

    fn root_binding(&self, namespace: &str, name: &str) -> Option<String> {
        let key = (namespace.to_string(), name.to_string());
        if let Some(binding) = self.root_bindings.borrow().get(&key) {
            return binding.clone();
        }
        let binding = self.lexical_root(namespace, name);
        self.root_bindings.borrow_mut().insert(key, binding.clone());
        binding
    }

    fn lexical_root(&self, namespace: &str, name: &str) -> Option<String> {
        let mut scope = Some(namespace);
        while let Some(current) = scope {
            if let Some(imports) = self.imports.get(current) {
                if imports.ambiguous.contains(name) || imports.opaque_modules.contains(name) {
                    return None;
                }
                if imports
                    .aliases
                    .get(name)
                    .is_some_and(|target| target == name)
                {
                    return Some(name.to_string());
                }
                if let Some(module) = self.module_binding(current, name, &mut BTreeSet::new()) {
                    return Some(module);
                }
                if imports.aliases.contains_key(name)
                    || self.symbols.contains_key(&format!("{current}::{name}"))
                {
                    return None;
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
        }
        Some(name.to_string())
    }

    fn symbol(&self, symbol: &str, visiting: &mut BTreeSet<String>) -> Option<&Declaration> {
        if let Some(keys) = self.symbols.get(symbol) {
            if keys.len() == 1 {
                return self.entries.get(&keys[0]);
            }
            return None;
        }
        if let Some(keys) = self.functions.get(symbol) {
            if keys.len() != 1 {
                return None;
            }
            let entry = self.entries.get(&keys[0])?;
            if entry.renderer {
                return Some(entry);
            }
        }
        let (namespace, name) = symbol.rsplit_once("::")?;
        let imports = self.imports.get(namespace)?;
        if imports.ambiguous.contains(name) || imports.opaque_modules.contains(name) {
            return None;
        }
        let target = imports.aliases.get(name)?;
        self.resolve(namespace, target, visiting)
    }
}

struct Declarations<'a> {
    path: &'a str,
    scope: Vec<String>,
    inventory: &'a mut Inventory,
    production: bool,
}

struct Namespace(String);

impl Namespace {
    fn of(path: &str, scope: &[String]) -> Self {
        let parts = path.split('/').collect::<Vec<_>>();
        let mut module = Vec::new();
        if let Some(source) = parts.iter().position(|part| *part == "src") {
            let package = if parts[0] == "crates" {
                parts[2]
            } else {
                parts[1]
            };
            let mut package = package.split("-0.").next().unwrap().replace('-', "_");
            if parts[source + 1..] == ["main.rs"] {
                package.push_str("__bin");
            } else if parts.get(source + 1) == Some(&"bin")
                && let Some(binary) = parts.get(source + 2)
            {
                package.push_str("__bin_");
                package.push_str(binary.trim_end_matches(".rs"));
            }
            module.push(package);
            let start = if parts.get(source + 1) == Some(&"bin") {
                source + 3
            } else {
                source + 1
            };
            for part in &parts[start..] {
                let name = part.trim_end_matches(".rs");
                if !matches!(name, "lib" | "main" | "mod") {
                    module.push(name.into());
                }
            }
        } else {
            module.push(
                path.trim_end_matches(".rs")
                    .replace('/', "::")
                    .replace('-', "_"),
            );
        }
        for part in scope {
            if !part.starts_with("fn ") && !part.starts_with("impl ") {
                module.push(part.strip_prefix("r#").unwrap_or(part).to_string());
            }
        }
        Self(module.join("::"))
    }
}

struct JobOutputs<'a> {
    inventory: &'a Inventory,
    namespace: &'a str,
    jobs: BTreeSet<(String, String)>,
    visited: BTreeSet<String>,
    inside: bool,
    direct: bool,
}

impl<'ast> Visit<'ast> for JobOutputs<'_> {
    fn visit_type(&mut self, value: &'ast Type) {
        match value {
            Type::Reference(value) => {
                let direct = self.direct;
                self.direct = self.inside;
                self.visit_type(&value.elem);
                self.direct = direct;
            }
            Type::Paren(value) => self.visit_type(&value.elem),
            Type::Group(value) => self.visit_type(&value.elem),
            Type::Tuple(value) => {
                for value in &value.elems {
                    self.visit_type(value);
                }
            }
            Type::Path(value) => self.visit_type_path(value),
            _ => {}
        }
    }

    fn visit_type_path(&mut self, value: &'ast syn::TypePath) {
        if value.qself.is_some() {
            return;
        }
        let name = References::path(&value.path);
        if let Some(owner) = self
            .inventory
            .resolve(self.namespace, &name, &mut BTreeSet::new())
        {
            if owner.key == "crates/util/vmux_ecs/src/jobs.rs||struct:JobResult" {
                if !self.inside || !self.direct {
                    return;
                }
                let arguments = References::arguments(&value.path);
                if arguments.len() == 1
                    && let Ok(Type::Path(job)) = syn::parse_str::<Type>(&arguments[0])
                    && let Some((key, concrete)) = self.inventory.concrete_job(
                        self.namespace,
                        &Type::Path(job),
                        &mut BTreeSet::new(),
                    )
                {
                    self.jobs
                        .insert((key, concrete.to_token_stream().to_string()));
                }
            } else if (matches!(owner.kind, Kind::Alias)
                || owner.parameter
                    && owner.parameter_path.as_deref().is_some_and(|path| {
                        self.inventory
                            .framework_handle(&owner.namespace, path, &mut BTreeSet::new())
                            .is_some_and(|kind| {
                                matches!(kind.as_str(), "SystemParam" | "QueryData")
                            })
                    }))
                && owner.production
                && self.inventory.used(owner)
                && self.visited.insert(owner.key.clone())
            {
                let arguments = References::arguments(&value.path);
                if let Some(bindings) = self.inventory.type_bindings(
                    self.namespace,
                    &owner.generic_parameters,
                    &owner.generic_bindings,
                    &arguments,
                    &BTreeMap::new(),
                ) {
                    let fields: Vec<_> = owner
                        .field_types
                        .iter()
                        .map(|field| {
                            Bindings::apply(self.inventory, &owner.namespace, &bindings, field)
                        })
                        .collect();
                    let mut nested = JobOutputs {
                        inventory: self.inventory,
                        namespace: &owner.namespace,
                        jobs: BTreeSet::new(),
                        visited: self.visited.clone(),
                        inside: self.inside,
                        direct: self.direct,
                    };
                    for field in &fields {
                        nested.visit_type(field);
                    }
                    self.jobs.extend(nested.jobs);
                }
                self.visited.remove(&owner.key);
            }
            return;
        }
        let Some(handle) =
            self.inventory
                .framework_handle(self.namespace, &name, &mut BTreeSet::new())
        else {
            return;
        };
        let inside = match handle.as_str() {
            "Query" | "Single" | "Res" | "ResMut" | "NonSend" | "NonSendMut" | "On"
            | "SystemState" | "MessageReader" | "MessageWriter" | "MessageMutator" => true,
            "Ref" | "Mut" | "Option" => self.inside,
            "ParamSet" => false,
            _ => return,
        };
        if !inside && handle != "ParamSet" {
            return;
        }
        let direct = if handle == "Option" {
            self.direct
        } else {
            !matches!(
                handle.as_str(),
                "Query" | "Single" | "ParamSet" | "SystemState"
            )
        };
        let previous = (self.inside, self.direct);
        self.inside = inside;
        self.direct = direct;
        if let Some(segment) = value.path.segments.last()
            && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
        {
            for argument in &arguments.args {
                if let syn::GenericArgument::Type(value) = argument {
                    self.visit_type(value);
                    if matches!(handle.as_str(), "Query" | "Single") {
                        break;
                    }
                }
            }
        }
        (self.inside, self.direct) = previous;
    }
}

struct Bindings<'a> {
    inventory: &'a Inventory,
    namespace: &'a str,
    values: &'a BTreeMap<String, syn::GenericArgument>,
}

impl<'a> Bindings<'a> {
    fn argument(
        inventory: &'a Inventory,
        namespace: &'a str,
        values: &'a BTreeMap<String, syn::GenericArgument>,
        value: &syn::GenericArgument,
    ) -> syn::GenericArgument {
        let mut value = value.clone();
        Self {
            inventory,
            namespace,
            values,
        }
        .visit_generic_argument_mut(&mut value);
        value
    }

    fn apply(
        inventory: &'a Inventory,
        namespace: &'a str,
        values: &'a BTreeMap<String, syn::GenericArgument>,
        value: &Type,
    ) -> Type {
        let mut value = value.clone();
        Self {
            inventory,
            namespace,
            values,
        }
        .visit_type_mut(&mut value);
        value
    }
}

impl VisitMut for Bindings<'_> {
    fn visit_type_mut(&mut self, value: &mut Type) {
        if let Type::Path(path) = value
            && path.qself.is_none()
            && let Some(name) = path.path.get_ident()
            && let Some(syn::GenericArgument::Type(bound)) =
                self.values.get(&name.unraw().to_string())
        {
            *value = bound.clone();
            return;
        }
        visit_mut::visit_type_mut(self, value);
        let Type::Path(path) = value else {
            return;
        };
        if path.qself.is_some() {
            return;
        }
        let name = References::path(&path.path);
        if let Some(owner) = self
            .inventory
            .resolve(self.namespace, &name, &mut BTreeSet::new())
        {
            let arguments = path.path.segments.last().unwrap().arguments.clone();
            path.path = syn::parse_str(&format!("{}::{}", owner.namespace, owner.name))
                .expect("resolved type path");
            path.path.segments.last_mut().unwrap().arguments = arguments;
        }
    }

    fn visit_generic_argument_mut(&mut self, value: &mut syn::GenericArgument) {
        let name = match value {
            syn::GenericArgument::Type(Type::Path(value)) if value.qself.is_none() => {
                value.path.get_ident()
            }
            syn::GenericArgument::Const(syn::Expr::Path(value)) if value.qself.is_none() => {
                value.path.get_ident()
            }
            _ => None,
        };
        if let Some(name) = name
            && let Some(bound) = self.values.get(&name.unraw().to_string())
        {
            *value = bound.clone();
            return;
        }
        visit_mut::visit_generic_argument_mut(self, value);
    }

    fn visit_expr_mut(&mut self, value: &mut syn::Expr) {
        if let syn::Expr::Path(path) = value
            && path.qself.is_none()
            && let Some(name) = path.path.get_ident()
            && let Some(syn::GenericArgument::Const(bound)) =
                self.values.get(&name.unraw().to_string())
        {
            *value = bound.clone();
            return;
        }
        visit_mut::visit_expr_mut(self, value);
    }
}

#[derive(Default)]
struct Imports {
    aliases: BTreeMap<String, String>,
    ambiguous: BTreeSet<String>,
    globs: BTreeSet<String>,
    modules: BTreeSet<String>,
    module_sources: BTreeMap<String, Vec<String>>,
    opaque_modules: BTreeSet<String>,
}

impl Imports {
    fn bind_alias(&mut self, name: String, target: String) {
        if self
            .aliases
            .get(&name)
            .is_some_and(|previous| *previous != target)
        {
            self.ambiguous.insert(name.clone());
        }
        self.aliases.insert(name, target);
    }

    fn add(&mut self, tree: &syn::UseTree, prefix: &str) {
        match tree {
            syn::UseTree::Path(path) => {
                let prefix = if prefix.is_empty() {
                    path.ident.unraw().to_string()
                } else {
                    format!("{prefix}::{}", path.ident.unraw())
                };
                self.add(&path.tree, &prefix);
            }
            syn::UseTree::Name(name) => {
                if name.ident == "self" && !prefix.is_empty() {
                    self.bind_alias(
                        prefix.rsplit("::").next().unwrap().to_string(),
                        prefix.to_string(),
                    );
                    return;
                }
                let target = if prefix.is_empty() {
                    name.ident.unraw().to_string()
                } else {
                    format!("{prefix}::{}", name.ident.unraw())
                };
                self.bind_alias(name.ident.unraw().to_string(), target);
            }
            syn::UseTree::Rename(name) => {
                let target = if name.ident == "self" && !prefix.is_empty() {
                    prefix.to_string()
                } else if prefix.is_empty() {
                    name.ident.unraw().to_string()
                } else {
                    format!("{prefix}::{}", name.ident.unraw())
                };
                self.bind_alias(name.rename.unraw().to_string(), target);
            }
            syn::UseTree::Glob(_) => {
                self.globs.insert(prefix.into());
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.add(item, prefix);
                }
            }
        }
    }
}

impl Declarations<'_> {
    fn metadata(attributes: &[Attribute], possible: bool) -> Vec<syn::Meta> {
        let mut pending = Vec::new();
        for attribute in attributes {
            pending.push(attribute.meta.clone());
        }
        let mut expanded = Vec::new();
        while let Some(meta) = pending.pop() {
            if !meta.path().is_ident("cfg_attr") {
                expanded.push(meta);
                continue;
            }
            let syn::Meta::List(list) = meta else {
                continue;
            };
            let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
            let Ok(items) = parser.parse2(list.tokens) else {
                continue;
            };
            let Some(condition) = items.first() else {
                continue;
            };
            let Ok(expression) = syn::parse2::<syn::Expr>(condition.to_token_stream()) else {
                continue;
            };
            if possible && !Configuration(&expression).production()
                || !possible && Configuration(&expression).definite() != Some(true)
            {
                continue;
            }
            for nested in items.into_iter().skip(1) {
                pending.push(nested);
            }
        }
        expanded
    }

    fn path_override(meta: &syn::Meta) -> bool {
        if meta.path().segments.len() == 1 && meta.path().segments[0].ident.unraw() == "path" {
            return true;
        }
        let syn::Meta::List(list) = meta else {
            return false;
        };
        if list.path.segments.len() != 1 || list.path.segments[0].ident.unraw() != "cfg_attr" {
            return false;
        }
        let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
        let Ok(attributes) = parser.parse2(list.tokens.clone()) else {
            return true;
        };
        for attribute in attributes.iter().skip(1) {
            if Self::path_override(attribute) {
                return true;
            }
        }
        false
    }

    fn add(
        &mut self,
        name: String,
        kind: Kind,
        node: &impl ToTokens,
        attrs: &[Attribute],
        fields: &[Type],
        owner: Option<String>,
    ) {
        let mut references = References {
            include_tests: !self.production || Usage::test_only(attrs),
            ..Default::default()
        };
        let tokens = node.to_token_stream();
        let mut generic_bindings = BTreeSet::new();
        let mut generic_parameters = Vec::new();
        let mut plugin_parameters = Vec::new();
        let mut component_bindings = BTreeMap::new();
        let mut plugin = false;
        let mut plugin_path = None;
        let mut parameter_path = None;
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            references.visit_file(&file);
            for item in &file.items {
                if let Item::Impl(implementation) = item {
                    if let Type::Path(owner) = implementation.self_ty.as_ref() {
                        plugin_parameters = References::arguments(&owner.path);
                    }
                    plugin_path = implementation.trait_.as_ref().map(|(_, path, _)| {
                        path.segments
                            .iter()
                            .map(|segment| segment.ident.unraw().to_string())
                            .collect::<Vec<_>>()
                            .join("::")
                    });
                    plugin = implementation.trait_.as_ref().is_some_and(|(_, path, _)| {
                        path.segments.last().is_some_and(|segment| {
                            matches!(segment.ident.to_string().as_str(), "Plugin" | "BevyPlugin")
                        })
                    });
                }
                let generics = match item {
                    Item::Struct(value) => &value.generics,
                    Item::Enum(value) => &value.generics,
                    Item::Type(value) => &value.generics,
                    Item::Fn(value) => &value.sig.generics,
                    Item::Impl(value) => &value.generics,
                    _ => continue,
                };
                for parameter in &generics.params {
                    match parameter {
                        syn::GenericParam::Type(value) => {
                            generic_bindings.insert(value.ident.unraw().to_string());
                            generic_parameters.push(value.ident.unraw().to_string());
                            for bound in &value.bounds {
                                let syn::TypeParamBound::Trait(bound) = bound else {
                                    continue;
                                };
                                if bound.path.segments.last().is_some_and(|segment| {
                                    matches!(
                                        segment.ident.to_string().as_str(),
                                        "Component" | "Resource" | "SystemParam"
                                    )
                                }) {
                                    let mut path = Vec::new();
                                    for segment in &bound.path.segments {
                                        path.push(segment.ident.unraw().to_string());
                                    }
                                    component_bindings
                                        .insert(value.ident.unraw().to_string(), path.join("::"));
                                }
                            }
                        }
                        syn::GenericParam::Const(value) => {
                            generic_bindings.insert(value.ident.unraw().to_string());
                            generic_parameters.push(value.ident.unraw().to_string());
                        }
                        syn::GenericParam::Lifetime(_) => {}
                    }
                }
            }
        }
        let mut field_names = References::default();
        for field in fields {
            field_names.visit_type(field);
        }
        let mut ecs = false;
        let mut parameter = false;
        let mut relation_target = None;
        let mut relation_source = None;
        let mut component = false;
        let mut component_hooks = BTreeSet::new();
        let mut component_path = None;
        let mut required_components = BTreeSet::new();
        let mut contract_paths = BTreeSet::new();
        let test = Self::metadata(attrs, false)
            .iter()
            .any(|attr| attr.path().is_ident("test"));
        for attr in Self::metadata(attrs, true) {
            let mut attribute = Vec::new();
            for segment in &attr.path().segments {
                attribute.push(segment.ident.unraw().to_string());
            }
            contract_paths.insert(attribute.join("::"));
            if attr.path().is_ident("require")
                && let syn::Meta::List(list) = &attr
            {
                let parser =
                    syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
                if let Ok(inputs) = parser.parse2(list.tokens.clone()) {
                    for input in inputs {
                        let path = match input {
                            syn::Expr::Path(path) => path,
                            syn::Expr::Assign(value) => {
                                let syn::Expr::Path(path) = *value.left else {
                                    continue;
                                };
                                path
                            }
                            _ => continue,
                        };
                        required_components
                            .insert(path.path.to_token_stream().to_string().replace(' ', ""));
                    }
                }
            }
            if attr.path().is_ident("component")
                && let syn::Meta::List(list) = &attr
            {
                let parser =
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
                if let Ok(items) = parser.parse2(list.tokens.clone()) {
                    for item in items {
                        let syn::Meta::NameValue(value) = item else {
                            continue;
                        };
                        if !value.path.get_ident().is_some_and(|name| {
                            matches!(
                                name.to_string().as_str(),
                                "on_add"
                                    | "on_insert"
                                    | "on_discard"
                                    | "on_replace"
                                    | "on_remove"
                                    | "on_despawn"
                            )
                        }) {
                            continue;
                        }
                        let callback = match value.value {
                            syn::Expr::Path(callback) => callback,
                            syn::Expr::Call(call) => {
                                let syn::Expr::Path(callback) = *call.func else {
                                    continue;
                                };
                                callback
                            }
                            _ => continue,
                        };
                        if callback.path.leading_colon.is_some() {
                            continue;
                        }
                        let mut path = Vec::new();
                        for segment in callback.path.segments {
                            path.push(segment.ident.unraw().to_string());
                        }
                        component_hooks.insert(path.join("::"));
                    }
                }
            }
            if (attr.path().is_ident("relationship") || attr.path().is_ident("relationship_target"))
                && let syn::Meta::List(list) = &attr
            {
                let parser =
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
                if let Ok(items) = parser.parse2(list.tokens.clone()) {
                    for item in items {
                        let syn::Meta::NameValue(value) = item else {
                            continue;
                        };
                        let syn::Expr::Path(target) = value.value else {
                            continue;
                        };
                        let name = target.path.to_token_stream().to_string().replace(' ', "");
                        if attr.path().is_ident("relationship")
                            && value.path.is_ident("relationship_target")
                        {
                            relation_target = Some(name);
                        } else if attr.path().is_ident("relationship_target")
                            && value.path.is_ident("relationship")
                        {
                            relation_source = Some(name);
                        }
                    }
                }
            }
            if attr.path().is_ident("derive") {
                let syn::Meta::List(list) = attr else {
                    continue;
                };
                let parser =
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated;
                let Ok(paths) = parser.parse2(list.tokens) else {
                    continue;
                };
                for path in paths {
                    if path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "Component")
                    {
                        component = true;
                        component_path = Some(path.to_token_stream().to_string().replace(' ', ""));
                    }
                    if path.segments.last().is_some_and(|segment| {
                        matches!(
                            segment.ident.to_string().as_str(),
                            "Component"
                                | "Resource"
                                | "Event"
                                | "EntityEvent"
                                | "Message"
                                | "Bundle"
                                | "SystemParam"
                                | "QueryData"
                                | "SystemSet"
                        )
                    }) {
                        ecs = true;
                    }
                    if path.segments.last().is_some_and(|segment| {
                        matches!(
                            segment.ident.to_string().as_str(),
                            "SystemParam" | "QueryData"
                        )
                    }) {
                        parameter = true;
                        parameter_path = Some(
                            path.segments
                                .iter()
                                .map(|segment| segment.ident.unraw().to_string())
                                .collect::<Vec<_>>()
                                .join("::"),
                        );
                    }
                }
            }
        }
        if !component {
            component_hooks.clear();
        }
        let label = match kind {
            Kind::Struct => "struct",
            Kind::Enum => "enum",
            Kind::Alias => "alias",
            Kind::Trait => "trait",
            Kind::Impl => "impl",
            Kind::Function => "fn",
            Kind::Static => "static",
            Kind::Const => "const",
            Kind::Macro => "macro",
            Kind::Script => "script",
        };
        let base = format!("{}|{}|{label}:{name}", self.path, self.scope.join("::"));
        let mut key = base.clone();
        let mut ordinal = 2;
        while self.inventory.entries.contains_key(&key) {
            key = format!("{base}#{ordinal}");
            ordinal += 1;
        }
        let parsed_item = syn::parse2::<Item>(tokens.to_token_stream()).ok();
        let mut component_attributes = BTreeSet::new();
        if matches!(&parsed_item, Some(Item::Struct(value)) if matches!(value.fields, syn::Fields::Unit) && value.generics.params.is_empty())
        {
            for attribute in Self::metadata(attrs, true) {
                if matches!(attribute, syn::Meta::Path(_)) {
                    component_attributes.insert(
                        attribute
                            .path()
                            .to_token_stream()
                            .to_string()
                            .replace(' ', ""),
                    );
                }
            }
        }
        let declaration = Declaration {
            key: key.clone(),
            path: self.path.into(),
            namespace: self.inventory.namespace(self.path, &self.scope).0,
            name,
            kind,
            fingerprint: Declaration::fingerprint(&tokens.to_string()),
            enum_shape: match &parsed_item {
                Some(Item::Enum(value)) => Some(EnumShape::of(value)),
                _ => None,
            },
            bodies: BodyShape::items(&key, parsed_item.as_ref()),
            ecs,
            parameter,
            plugin,
            plugin_path,
            parameter_path,
            plugin_children: references.plugins,
            plugin_arguments: references.plugin_arguments,
            system_arguments: references.system_arguments,
            registered_systems: references.registered_systems,
            field_types: match &parsed_item {
                Some(Item::Struct(value)) => value
                    .fields
                    .iter()
                    .filter(|field| !Usage::test_only(&field.attrs))
                    .map(|field| field.ty.clone())
                    .collect(),
                Some(Item::Enum(value)) => value
                    .variants
                    .iter()
                    .filter(|variant| !Usage::test_only(&variant.attrs))
                    .flat_map(|variant| variant.fields.iter())
                    .filter(|field| !Usage::test_only(&field.attrs))
                    .map(|field| field.ty.clone())
                    .collect(),
                Some(Item::Type(value)) => vec![*value.ty.clone()],
                Some(Item::Fn(value)) => value
                    .sig
                    .inputs
                    .iter()
                    .filter_map(|argument| match argument {
                        syn::FnArg::Typed(value) if !Usage::test_only(&value.attrs) => {
                            Some(value.ty.as_ref().clone())
                        }
                        _ => None,
                    })
                    .collect(),
                _ => fields.to_vec(),
            },
            implemented_trait: match &parsed_item {
                Some(Item::Impl(value)) => value
                    .trait_
                    .as_ref()
                    .map(|(_, path, _)| References::path(path)),
                _ => None,
            },
            implemented_type: match &parsed_item {
                Some(Item::Impl(value)) => Some(*value.self_ty.clone()),
                _ => None,
            },
            associated_types: match &parsed_item {
                Some(Item::Impl(value)) => value
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        syn::ImplItem::Type(value) if !Usage::test_only(&value.attrs) => {
                            Some((value.ident.to_string(), value.ty.clone()))
                        }
                        _ => None,
                    })
                    .collect(),
                _ => BTreeMap::new(),
            },
            relation_target,
            relation_source,
            component_hooks,
            component_path,
            required_components,
            contract: false,
            contract_paths,
            component_attributes,
            projection: false,
            test,
            production: self.production && !Usage::test_only(attrs),
            private: false,
            has_fields: !fields.is_empty(),
            unit_mutex: fields.len() == 1 && Declaration::unit_mutex(&fields[0]),
            owner,
            reference_paths: references.paths,
            included_sources: references.included_sources,
            page_inputs: references.page_inputs,
            page_components: references.page_components,
            page_input_errors: references.page_input_errors,
            inventory_inputs: references.inventory_inputs,
            dom_mounts: references.dom_mounts,
            rendered: references.rendered,
            renderer: matches!(kind, Kind::Function)
                && Self::metadata(attrs, true)
                    .iter()
                    .any(|attribute| attribute.path().is_ident("component")),
            fields: field_names.names,
            field_paths: field_names.paths,
            generic_bindings,
            generic_parameters,
            plugin_parameters,
            component_bindings,
            local: references.local,
            dynamic: references.dynamic,
            opaque: references.opaque,
        };
        if matches!(
            kind,
            Kind::Struct | Kind::Enum | Kind::Alias | Kind::Trait | Kind::Const | Kind::Static
        ) {
            self.inventory.roots.insert(
                declaration
                    .namespace
                    .split("::")
                    .next()
                    .unwrap()
                    .to_string(),
            );
            self.inventory
                .symbols
                .entry(format!("{}::{}", declaration.namespace, declaration.name))
                .or_default()
                .push(key.clone());
            if !matches!(kind, Kind::Const) {
                self.inventory
                    .names
                    .entry(declaration.name.clone())
                    .or_default()
                    .push(key.clone());
            }
        }
        if matches!(kind, Kind::Function) {
            self.inventory
                .functions
                .entry(format!("{}::{}", declaration.namespace, declaration.name))
                .or_default()
                .push(key.clone());
        }
        self.inventory.entries.insert(key, declaration);
    }
}

impl<'ast> Visit<'ast> for Declarations<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::ExternCrate(value) => {
                let name = value.rename.as_ref().map_or(&value.ident, |(_, name)| name);
                self.inventory
                    .imports
                    .entry(self.inventory.namespace(self.path, &self.scope).0)
                    .or_default()
                    .bind_alias(name.unraw().to_string(), value.ident.unraw().to_string());
            }
            Item::Use(value)
                if !self
                    .scope
                    .iter()
                    .any(|scope| scope.starts_with("fn ") || scope.starts_with("impl ")) =>
            {
                let namespace = self.inventory.namespace(self.path, &self.scope).0;
                self.inventory
                    .imports
                    .entry(namespace)
                    .or_default()
                    .add(&value.tree, "");
            }
            Item::Struct(value) => {
                let mut fields = Vec::new();
                for field in &value.fields {
                    if !Usage::test_only(&field.attrs) {
                        fields.push(field.ty.clone());
                    }
                }
                self.add(
                    value.ident.to_string(),
                    Kind::Struct,
                    value,
                    &value.attrs,
                    &fields,
                    None,
                );
            }
            Item::Enum(value) => {
                let mut fields = Vec::new();
                for variant in &value.variants {
                    if !Usage::test_only(&variant.attrs) {
                        for field in &variant.fields {
                            if !Usage::test_only(&field.attrs) {
                                fields.push(field.ty.clone());
                            }
                        }
                    }
                }
                self.add(
                    value.ident.to_string(),
                    Kind::Enum,
                    value,
                    &value.attrs,
                    &fields,
                    None,
                );
            }
            Item::Type(value) => self.add(
                value.ident.to_string(),
                Kind::Alias,
                value,
                &value.attrs,
                &[*value.ty.clone()],
                None,
            ),
            Item::Trait(value) => self.add(
                value.ident.to_string(),
                Kind::Trait,
                value,
                &value.attrs,
                &[],
                None,
            ),
            Item::Static(value) => self.add(
                value.ident.to_string(),
                Kind::Static,
                value,
                &value.attrs,
                &[*value.ty.clone()],
                None,
            ),
            Item::Const(value) => {
                let name = if value.ident == "_" {
                    format!("_{}", value.expr.to_token_stream())
                } else {
                    value.ident.to_string()
                };
                self.add(name, Kind::Const, value, &value.attrs, &[], None);
            }
            Item::Macro(value) => self.add(
                value.ident.as_ref().map_or_else(
                    || value.mac.path.to_token_stream().to_string(),
                    ToString::to_string,
                ),
                Kind::Macro,
                value,
                &value.attrs,
                &[],
                None,
            ),
            Item::Fn(value) => {
                let mut fields = Vec::new();
                for argument in &value.sig.inputs {
                    if let syn::FnArg::Typed(argument) = argument
                        && !Usage::test_only(&argument.attrs)
                    {
                        fields.push(*argument.ty.clone());
                    }
                }
                self.add(
                    value.sig.ident.to_string(),
                    Kind::Function,
                    value,
                    &value.attrs,
                    &fields,
                    None,
                );
                let namespace = self.inventory.namespace(self.path, &self.scope).0;
                let symbol = format!("{namespace}::{}", value.sig.ident);
                let key = self.inventory.functions[&symbol].last().unwrap();
                self.inventory.entries.get_mut(key).unwrap().private =
                    matches!(value.vis, syn::Visibility::Inherited);
                self.scope.push(format!("fn {}", value.sig.ident));
                visit::visit_item(self, item);
                self.scope.pop();
                return;
            }
            Item::Impl(value) => {
                let owner = value.self_ty.to_token_stream().to_string();
                let name = match &value.trait_ {
                    Some((_, path, _)) => format!("{} for {owner}", path.to_token_stream()),
                    None => owner.clone(),
                };
                let owner = match value.self_ty.as_ref() {
                    Type::Path(path) if path.qself.is_none() => {
                        let mut segments = Vec::with_capacity(path.path.segments.len());
                        for segment in &path.path.segments {
                            segments.push(segment.ident.unraw().to_string());
                        }
                        Some(segments.join("::"))
                    }
                    _ => None,
                };
                self.add(name.clone(), Kind::Impl, value, &value.attrs, &[], owner);
                self.scope.push(format!("impl {name}"));
                visit::visit_item(self, item);
                self.scope.pop();
                return;
            }
            Item::Mod(value) => {
                let previous = self.production;
                self.production &= !Usage::test_only(&value.attrs);
                if value
                    .attrs
                    .iter()
                    .any(|attribute| Self::path_override(&attribute.meta))
                {
                    self.inventory
                        .imports
                        .entry(self.inventory.namespace(self.path, &self.scope).0)
                        .or_default()
                        .opaque_modules
                        .insert(value.ident.unraw().to_string());
                }
                let namespace = self.inventory.namespace(self.path, &self.scope).0;
                let name = value.ident.unraw().to_string();
                if value.content.is_none()
                    && !value
                        .attrs
                        .iter()
                        .any(|attribute| Self::path_override(&attribute.meta))
                {
                    let file = Path::new(self.path);
                    let mut directory = file.parent().unwrap().to_path_buf();
                    let crate_root =
                        match file.components().position(|part| part.as_os_str() == "src") {
                            Some(root) => {
                                let relative = file
                                    .components()
                                    .skip(root + 1)
                                    .map(|part| part.as_os_str())
                                    .collect::<Vec<_>>();
                                match relative.as_slice() {
                                    [name] => *name == "lib.rs" || *name == "main.rs",
                                    [bin, _] => *bin == "bin",
                                    [bin, _, name] => *bin == "bin" && *name == "main.rs",
                                    _ => false,
                                }
                            }
                            None => false,
                        };
                    let crate_root = crate_root
                        && self.inventory.namespace(self.path, &[]).0
                            == Namespace::of(self.path, &[]).0;
                    let module_root =
                        file.file_name().is_some_and(|name| name == "mod.rs") || crate_root;
                    if !module_root {
                        directory.push(file.file_stem().unwrap());
                    }
                    for scope in &self.scope {
                        if let Ok(name) = syn::parse_str::<syn::Ident>(scope) {
                            directory.push(name.unraw().to_string());
                        }
                    }
                    let paths = [
                        directory.join(format!("{name}.rs")),
                        directory.join(&name).join("mod.rs"),
                    ]
                    .into_iter()
                    .map(|path| path.to_str().unwrap().to_string())
                    .collect::<Vec<_>>();
                    for path in &paths {
                        self.inventory
                            .module_namespaces
                            .entry(path.clone())
                            .or_default()
                            .insert(format!("{namespace}::{name}"));
                    }
                    self.inventory
                        .imports
                        .entry(namespace.clone())
                        .or_default()
                        .module_sources
                        .insert(name.clone(), paths);
                }
                self.inventory
                    .imports
                    .entry(namespace)
                    .or_default()
                    .modules
                    .insert(name);
                self.scope.push(value.ident.to_string());
                self.inventory
                    .imports
                    .entry(self.inventory.namespace(self.path, &self.scope).0)
                    .or_default();
                visit::visit_item(self, item);
                self.scope.pop();
                self.production = previous;
                return;
            }
            _ => {}
        }
        visit::visit_item(self, item);
    }
}

#[derive(Default)]
struct References<'a> {
    parameter_inventory: Option<&'a Inventory>,
    parameter_namespace: &'a str,
    parameter_inside: bool,
    parameter_read: bool,
    parameter_unbound: BTreeSet<String>,
    indexed: BTreeSet<String>,
    retained: BTreeSet<String>,
    outputs: BTreeSet<String>,
    visiting: BTreeSet<String>,
    inside: bool,
    payload: bool,
    valid: bool,
    include_tests: bool,
    plugins: BTreeSet<String>,
    plugin_arguments: BTreeSet<(String, Vec<String>)>,
    system_arguments: BTreeSet<(String, String, Vec<String>)>,
    registered_systems: BTreeSet<(String, String)>,
    inside_build: bool,
    build_app: Option<(String, String)>,
    names: BTreeSet<String>,
    local: bool,
    dynamic: bool,
    opaque: bool,
    paths: BTreeSet<String>,
    rendered: BTreeSet<String>,
    included_sources: BTreeSet<String>,
    page_inputs: BTreeSet<(String, String)>,
    page_components: BTreeSet<(String, String)>,
    page_input_errors: BTreeSet<String>,
    inventory_inputs: BTreeSet<(String, String)>,
    dom_mounts: BTreeSet<(String, String)>,
}

impl<'ast> Visit<'ast> for References<'_> {
    fn visit_type(&mut self, value: &'ast Type) {
        if self.parameter_inventory.is_some()
            && let Type::Reference(reference) = value
        {
            let previous = self.parameter_read;
            self.parameter_read = self.parameter_inside;
            self.visit_type(&reference.elem);
            self.parameter_read = previous;
            return;
        }
        visit::visit_type(self, value);
    }

    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = expression.func.as_ref()
            && let Some(last) = function.path.segments.last()
            && matches!(last.ident.to_string().as_str(), "new" | "new_with_props")
            && let Some(syn::Expr::Path(component)) = expression.args.first()
        {
            let mut parts = Vec::new();
            for segment in &function.path.segments {
                parts.push(segment.ident.unraw().to_string());
            }
            parts.pop();
            let component = component
                .path
                .to_token_stream()
                .to_string()
                .replace(' ', "");
            self.dom_mounts.insert((parts.join("::"), component));
        }
        visit::visit_expr_call(self, expression);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "page")
            && let syn::Meta::List(arguments) = &attribute.meta
        {
            let name = attribute
                .path()
                .to_token_stream()
                .to_string()
                .replace(' ', "");
            let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
            match parser.parse2(arguments.tokens.clone()) {
                Ok(arguments) => {
                    for argument in arguments {
                        let syn::Meta::NameValue(argument) = argument else {
                            continue;
                        };
                        if argument.path.is_ident("states") || argument.path.is_ident("events") {
                            let syn::Expr::Array(inputs) = argument.value else {
                                self.page_input_errors.insert(name.clone());
                                continue;
                            };
                            for input in inputs.elems {
                                match input {
                                    syn::Expr::Path(input) => {
                                        self.page_inputs.insert((
                                            name.clone(),
                                            input
                                                .path
                                                .to_token_stream()
                                                .to_string()
                                                .replace(' ', ""),
                                        ));
                                    }
                                    _ => {
                                        self.page_input_errors.insert(name.clone());
                                    }
                                }
                            }
                        } else if argument.path.is_ident("takes") {
                            match argument.value {
                                syn::Expr::Path(input) => {
                                    self.page_inputs.insert((
                                        name.clone(),
                                        input.path.to_token_stream().to_string().replace(' ', ""),
                                    ));
                                }
                                _ => {
                                    self.page_input_errors.insert(name.clone());
                                }
                            }
                        } else if argument.path.is_ident("component") {
                            match argument.value {
                                syn::Expr::Path(component) => {
                                    self.page_components.insert((
                                        name.clone(),
                                        component
                                            .path
                                            .to_token_stream()
                                            .to_string()
                                            .replace(' ', ""),
                                    ));
                                }
                                _ => {
                                    self.page_input_errors.insert(name.clone());
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    self.page_input_errors.insert(name);
                }
            }
        }
        visit::visit_attribute(self, attribute);
    }

    fn visit_fn_arg(&mut self, argument: &'ast syn::FnArg) {
        let attributes = match argument {
            syn::FnArg::Receiver(argument) => &argument.attrs,
            syn::FnArg::Typed(argument) => &argument.attrs,
        };
        if self.include_tests || !Usage::test_only(attributes) {
            visit::visit_fn_arg(self, argument);
        }
    }

    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        let interface = value.path.to_token_stream().to_string().replace(' ', "");
        if value
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "collect")
        {
            if let Ok(input) = syn::parse2::<syn::TypePath>(value.tokens.clone()) {
                self.inventory_inputs.insert((
                    interface,
                    input.path.to_token_stream().to_string().replace(' ', ""),
                ));
            }
        } else if value
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "submit")
        {
            let input = match syn::parse2::<syn::Expr>(value.tokens.clone()) {
                Ok(syn::Expr::Call(call)) => match *call.func {
                    syn::Expr::Path(mut constructor) if constructor.path.segments.len() > 1 => {
                        constructor.path.segments.pop();
                        constructor.path.segments.pop_punct();
                        Some(constructor.path)
                    }
                    _ => None,
                },
                Ok(syn::Expr::Struct(value)) => Some(value.path),
                _ => None,
            };
            if let Some(input) = input {
                self.inventory_inputs.insert((
                    interface,
                    input.to_token_stream().to_string().replace(' ', ""),
                ));
            }
        }
        if value.path.is_ident("include_str")
            && let Ok(path) = syn::parse2::<syn::LitStr>(value.tokens.clone())
        {
            self.included_sources.insert(path.value());
        }
        if value.path.is_ident("rsx") {
            self.renderer_tokens(value.tokens.clone());
        }
        visit::visit_macro(self, value);
    }

    fn visit_type_path(&mut self, value: &'ast syn::TypePath) {
        if self.parameter_inventory.is_some() {
            self.parameter_type_path(value);
            return;
        }
        self.local |= value
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Local");
        visit::visit_type_path(self, value);
    }

    fn visit_type_bare_fn(&mut self, value: &'ast syn::TypeBareFn) {
        if self.parameter_inventory.is_some() && self.payload {
            self.dynamic = true;
        }
        visit::visit_type_bare_fn(self, value);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if self.include_tests || !Usage::test_only(&local.attrs) {
            visit::visit_local(self, local);
        }
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if self.include_tests || !Usage::test_only(&arm.attrs) {
            visit::visit_arm(self, arm);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attributes: &[Attribute] = match item {
            syn::ImplItem::Const(value) => &value.attrs,
            syn::ImplItem::Fn(value) => &value.attrs,
            syn::ImplItem::Type(value) => &value.attrs,
            syn::ImplItem::Macro(value) => &value.attrs,
            _ => &[],
        };
        if self.include_tests || !Usage::test_only(attributes) {
            visit::visit_impl_item(self, item);
        }
    }
    fn visit_item(&mut self, item: &'ast Item) {
        if self.include_tests || !Usage::test_only(Usage::item_attributes(item)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        if self.include_tests || !Usage::test_only(Usage::expression_attributes(expression)) {
            visit::visit_expr(self, expression);
        }
    }

    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        if self.include_tests || !Usage::test_only(&implementation.attrs) {
            visit::visit_item_impl(self, implementation);
        }
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        if self.include_tests || !Usage::test_only(&field.attrs) {
            visit::visit_field(self, field);
        }
    }
    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if self.include_tests || !Usage::test_only(&function.attrs) {
            let previous = self.inside_build;
            let previous_app = self.build_app.clone();
            self.inside_build = function.sig.ident == "build";
            self.build_app = None;
            if self.inside_build {
                for argument in &function.sig.inputs {
                    let syn::FnArg::Typed(argument) = argument else {
                        continue;
                    };
                    let syn::Pat::Ident(name) = argument.pat.as_ref() else {
                        continue;
                    };
                    let Type::Reference(reference) = argument.ty.as_ref() else {
                        continue;
                    };
                    let Type::Path(value) = reference.elem.as_ref() else {
                        continue;
                    };
                    if reference.mutability.is_some()
                        && value
                            .path
                            .segments
                            .last()
                            .is_some_and(|segment| segment.ident == "App")
                    {
                        self.build_app = Some((
                            name.ident.unraw().to_string(),
                            value
                                .path
                                .segments
                                .iter()
                                .map(|segment| segment.ident.unraw().to_string())
                                .collect::<Vec<_>>()
                                .join("::"),
                        ));
                    }
                }
            }
            visit::visit_impl_item_fn(self, function);
            self.inside_build = previous;
            self.build_app = previous_app;
        }
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let previous = self.inside_build;
        self.inside_build = false;
        visit::visit_item_fn(self, function);
        self.inside_build = previous;
    }

    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        if self.inside_build
            && let syn::Expr::Lit(value) = expression.cond.as_ref()
            && let syn::Lit::Bool(value) = &value.lit
        {
            if value.value {
                self.visit_block(&expression.then_branch);
            } else if let Some((_, branch)) = &expression.else_branch {
                self.visit_expr(branch);
            }
            return;
        }
        visit::visit_expr_if(self, expression);
    }

    fn visit_expr_async(&mut self, block: &'ast syn::ExprAsync) {
        let previous = self.inside_build;
        self.inside_build = false;
        visit::visit_expr_async(self, block);
        self.inside_build = previous;
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let previous = self.inside_build;
        self.inside_build = false;
        visit::visit_expr_closure(self, closure);
        self.inside_build = previous;
    }

    fn visit_expr_block(&mut self, block: &'ast syn::ExprBlock) {
        if self.include_tests || !Usage::test_only(&block.attrs) {
            visit::visit_expr_block(self, block);
        }
    }

    fn visit_expr_method_call(&mut self, value: &'ast syn::ExprMethodCall) {
        if value.method == "add_plugins" {
            for argument in &value.args {
                self.plugin_values(argument);
            }
        }
        if self.inside_build && self.app_receiver(&value.receiver) {
            if value.method == "add_plugins" {
                for argument in &value.args {
                    self.typed_plugins(argument);
                }
            }
            let argument = match value.method.to_string().as_str() {
                "add_systems" => value.args.iter().nth(1),
                "add_observer" | "observe" => value.args.first(),
                _ => None,
            };
            if let Some(argument) = argument {
                self.system_values(argument);
            }
        }
        visit::visit_expr_method_call(self, value);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let mut parts = Vec::new();
        for part in &path.segments {
            parts.push(part.ident.to_string());
        }
        self.paths.insert(parts.join("::"));
        if let Some(last) = path.segments.last() {
            let name = last.ident.to_string();
            self.names.insert(name);
        }
        visit::visit_path(self, path);
    }

    fn visit_type_trait_object(&mut self, node: &'ast syn::TypeTraitObject) {
        self.dynamic = true;
        visit::visit_type_trait_object(self, node);
    }

    fn visit_lit_str(&mut self, value: &'ast syn::LitStr) {
        if value.value().contains('\n') && value.value().len() > 200 {
            self.opaque = true;
        }
        visit::visit_lit_str(self, value);
    }
}

impl References<'_> {
    fn parameter_type_path(&mut self, value: &syn::TypePath) {
        let inventory = self.parameter_inventory.unwrap();
        let namespace = self.parameter_namespace;
        let name = References::path(&value.path);
        if self.payload && self.unresolved_projection(value) {
            self.valid = false;
            return;
        }
        let Some(owner) = inventory.resolve(namespace, &name, &mut BTreeSet::new()) else {
            if self.parameter_handle(value) {
                return;
            }
            let previous = (self.parameter_inside, self.parameter_read);
            self.parameter_inside = false;
            self.parameter_read = false;
            visit::visit_type_path(self, value);
            (self.parameter_inside, self.parameter_read) = previous;
            return;
        };
        if self.inside {
            self.indexed.insert(owner.key.clone());
            visit::visit_type_path(self, value);
            return;
        }
        if !self.payload || !matches!(owner.kind, Kind::Alias) {
            self.retained.insert(owner.key.clone());
        }
        if self.parameter_read
            && inventory.indexed_result(owner)
            && value.qself.is_none()
            && let Some(segment) = value.path.segments.last()
            && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
            && arguments.args.len() == 1
            && let Some(syn::GenericArgument::Type(input)) = arguments.args.first()
            && let Some((implementation, output)) = inventory.job_output(namespace, input)
        {
            let identity = format!("{}|{}", implementation.key, input.to_token_stream());
            if self.visiting.len() >= 64 || !self.visiting.insert(identity.clone()) {
                self.valid = false;
                return;
            }
            let mut payload = References {
                parameter_inventory: Some(inventory),
                parameter_namespace: &implementation.namespace,
                parameter_inside: true,
                parameter_read: true,
                parameter_unbound: implementation.generic_bindings.clone(),
                visiting: self.visiting.clone(),
                payload: true,
                valid: true,
                ..Default::default()
            };
            payload.visit_type(&output);
            payload.valid &= !payload.dynamic && !payload.local;
            self.valid &= payload.valid;
            if payload.valid {
                self.inside = true;
                self.visit_type(input);
                self.inside = false;
                self.outputs.extend(payload.retained);
                self.outputs.extend(payload.outputs);
            } else {
                visit::visit_type_path(self, value);
            }
            self.visiting.remove(&identity);
            return;
        }
        if (matches!(owner.kind, Kind::Alias)
            || owner.parameter
                && owner.parameter_path.as_deref().is_some_and(|path| {
                    inventory
                        .framework_handle(&owner.namespace, path, &mut BTreeSet::new())
                        .is_some_and(|kind| matches!(kind.as_str(), "SystemParam" | "QueryData"))
                }))
        {
            if self.visiting.len() >= 64 || !self.visiting.insert(owner.key.clone()) {
                self.valid = false;
                return;
            }
            let arguments = References::arguments(&value.path);
            if let Some(bindings) = inventory.type_bindings(
                namespace,
                &owner.generic_parameters,
                &owner.generic_bindings,
                &arguments,
                &BTreeMap::new(),
            ) {
                let fields: Vec<_> = owner
                    .field_types
                    .iter()
                    .map(|field| Bindings::apply(inventory, &owner.namespace, &bindings, field))
                    .collect();
                let mut nested = References {
                    parameter_inventory: Some(inventory),
                    parameter_namespace: &owner.namespace,
                    parameter_inside: self.parameter_inside,
                    parameter_read: self.parameter_read,
                    parameter_unbound: self.parameter_unbound.clone(),
                    visiting: self.visiting.clone(),
                    payload: self.payload,
                    valid: true,
                    ..Default::default()
                };
                for field in &fields {
                    nested.visit_type(field);
                }
                self.valid &= nested.valid;
                self.dynamic |= nested.dynamic;
                self.local |= nested.local;
                self.indexed.extend(nested.indexed);
                self.retained.extend(nested.retained);
                self.outputs.extend(nested.outputs);
                self.visiting.remove(&owner.key);
                return;
            }
            self.valid = false;
            self.visiting.remove(&owner.key);
        }
        visit::visit_type_path(self, value);
    }

    fn parameter_handle(&mut self, value: &syn::TypePath) -> bool {
        let inventory = self.parameter_inventory.unwrap();
        let name = Self::path(&value.path);
        let Some(handle) =
            inventory.framework_handle(self.parameter_namespace, &name, &mut BTreeSet::new())
        else {
            return false;
        };
        let previous = (self.parameter_inside, self.parameter_read);
        match handle.as_str() {
            "Query" | "Single" => {
                self.parameter_inside = true;
                self.parameter_read = false;
            }
            "ParamSet" => {
                self.parameter_inside = false;
                self.parameter_read = false;
            }
            "Res" | "ResMut" | "NonSend" | "NonSendMut" => {
                self.parameter_inside = true;
                self.parameter_read = true;
            }
            "Option" | "Ref" | "Mut" => {}
            _ => return false,
        }
        if let Some(segment) = value.path.segments.last()
            && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
        {
            for argument in &arguments.args {
                if let syn::GenericArgument::Type(value) = argument {
                    self.visit_type(value);
                    if matches!(handle.as_str(), "Query" | "Single") {
                        break;
                    }
                }
            }
        }
        (self.parameter_inside, self.parameter_read) = previous;
        true
    }

    fn unresolved_projection(&self, value: &syn::TypePath) -> bool {
        if value.qself.is_some() {
            return true;
        }
        if value.path.segments.first().is_some_and(|segment| {
            self.parameter_unbound
                .contains(&segment.ident.unraw().to_string())
        }) {
            return true;
        }
        let name = Self::path(&value.path);
        let Some((owner, _)) = name.rsplit_once("::") else {
            return false;
        };
        self.parameter_inventory
            .unwrap()
            .resolve(self.parameter_namespace, owner, &mut BTreeSet::new())
            .is_some()
    }

    fn app_receiver(&self, value: &syn::Expr) -> bool {
        match value {
            syn::Expr::Path(value) if value.qself.is_none() && value.path.segments.len() == 1 => {
                self.build_app
                    .as_ref()
                    .is_some_and(|(name, _)| value.path.segments[0].ident == name.as_str())
            }
            syn::Expr::Paren(value) => self.app_receiver(&value.expr),
            syn::Expr::Group(value) => self.app_receiver(&value.expr),
            syn::Expr::MethodCall(value)
                if matches!(
                    value.method.to_string().as_str(),
                    "add_systems"
                        | "add_observer"
                        | "add_plugins"
                        | "add_message"
                        | "init_resource"
                        | "insert_resource"
                        | "register_type"
                        | "configure_sets"
                ) =>
            {
                self.app_receiver(&value.receiver)
            }
            _ => false,
        }
    }

    fn system_values(&mut self, value: &syn::Expr) {
        if !self.include_tests && Usage::test_only(Usage::expression_attributes(value)) {
            return;
        }
        match value {
            syn::Expr::Path(value) if value.qself.is_none() => {
                let path = value
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.unraw().to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if let Some((_, app_type)) = &self.build_app {
                    self.registered_systems
                        .insert((app_type.clone(), path.clone()));
                    self.system_arguments.insert((
                        app_type.clone(),
                        path,
                        Self::arguments(&value.path),
                    ));
                }
            }
            syn::Expr::Tuple(value) => {
                for value in &value.elems {
                    self.system_values(value);
                }
            }
            syn::Expr::Paren(value) => self.system_values(&value.expr),
            syn::Expr::Group(value) => self.system_values(&value.expr),
            syn::Expr::MethodCall(value)
                if matches!(
                    value.method.to_string().as_str(),
                    "chain"
                        | "chain_ignore_deferred"
                        | "after"
                        | "before"
                        | "after_ignore_deferred"
                        | "before_ignore_deferred"
                        | "in_set"
                        | "run_if"
                        | "distributive_run_if"
                        | "ambiguous_with"
                        | "ambiguous_with_all"
                ) =>
            {
                self.system_values(&value.receiver)
            }
            _ => {}
        }
    }

    fn path(path: &syn::Path) -> String {
        let mut parts = Vec::with_capacity(path.segments.len());
        for segment in &path.segments {
            parts.push(segment.ident.unraw().to_string());
        }
        parts.join("::")
    }

    fn renderer_tokens(&mut self, tokens: proc_macro2::TokenStream) {
        let body = syn::parse2::<dioxus_rsx::CallBody>(tokens).expect("valid Dioxus RSX source");
        self.renderer_nodes(&body.body.roots);
    }

    fn renderer_nodes(&mut self, nodes: &[dioxus_rsx::BodyNode]) {
        for node in nodes {
            match node {
                dioxus_rsx::BodyNode::Component(component) => {
                    if component.brace.is_some() {
                        self.rendered.insert(Self::path(&component.name));
                    }
                    if let Some(generics) = &component.generics {
                        self.visit_angle_bracketed_generic_arguments(generics);
                    }
                    for field in &component.fields {
                        self.renderer_attribute(&field.value);
                    }
                    for spread in &component.spreads {
                        self.visit_expr(&spread.expr);
                    }
                    self.renderer_nodes(&component.children.roots);
                }
                dioxus_rsx::BodyNode::Element(element) => {
                    for attribute in &element.raw_attributes {
                        self.renderer_attribute(&attribute.value);
                    }
                    for spread in &element.spreads {
                        self.visit_expr(&spread.expr);
                    }
                    self.renderer_nodes(&element.children);
                }
                dioxus_rsx::BodyNode::Text(text) => {
                    self.renderer_format(&text.input.formatted_input);
                }
                dioxus_rsx::BodyNode::RawExpr(value) => {
                    self.visit_expr(&value.expr.as_expr().expect("valid RSX Rust expression"));
                }
                dioxus_rsx::BodyNode::ForLoop(value) => {
                    self.visit_pat(&value.pat);
                    self.visit_expr(&value.expr);
                    self.renderer_nodes(&value.body.roots);
                }
                dioxus_rsx::BodyNode::IfChain(value) => self.renderer_condition(value),
            }
        }
    }

    fn renderer_attribute(&mut self, value: &dioxus_rsx::AttributeValue) {
        match value {
            dioxus_rsx::AttributeValue::Shorthand(name) => {
                self.visit_path(&syn::parse2(name.to_token_stream()).expect("RSX shorthand path"));
            }
            dioxus_rsx::AttributeValue::AttrExpr(value) => {
                self.visit_expr(&value.as_expr().expect("valid RSX property expression"));
            }
            dioxus_rsx::AttributeValue::EventTokens(value) => {
                self.visit_expr(
                    &syn::parse2(value.to_token_stream()).expect("valid RSX event closure"),
                );
            }
            dioxus_rsx::AttributeValue::IfExpr(value) => {
                self.visit_expr(&value.condition);
                self.renderer_attribute(&value.then_value);
                if let Some(value) = &value.else_value {
                    self.renderer_attribute(value);
                }
            }
            dioxus_rsx::AttributeValue::AttrLiteral(dioxus_rsx::HotLiteral::Fmted(value)) => {
                self.renderer_format(&value.formatted_input);
            }
            dioxus_rsx::AttributeValue::AttrLiteral(_) => {}
        }
    }

    fn renderer_condition(&mut self, value: &dioxus_rsx::IfChain) {
        self.visit_expr(&value.cond);
        self.renderer_nodes(&value.then_branch.roots);
        if let Some(value) = &value.else_if_branch {
            self.renderer_condition(value);
        }
        if let Some(value) = &value.else_branch {
            self.renderer_nodes(&value.roots);
        }
    }

    fn renderer_format(&mut self, value: &dioxus_rsx::IfmtInput) {
        for segment in &value.segments {
            let dioxus_rsx::Segment::Formatted(segment) = segment else {
                continue;
            };
            match &segment.segment {
                dioxus_rsx::FormattedSegmentType::Expr(value) => self.visit_expr(value),
                dioxus_rsx::FormattedSegmentType::Ident(value) => {
                    self.visit_path(
                        &syn::parse2(value.to_token_stream()).expect("RSX formatted identifier"),
                    );
                }
            }
        }
    }

    fn arguments(path: &syn::Path) -> Vec<String> {
        let mut values = Vec::new();
        for segment in &path.segments {
            if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                for argument in &arguments.args {
                    if !matches!(argument, syn::GenericArgument::Lifetime(_)) {
                        values.push(argument.to_token_stream().to_string());
                    }
                }
            }
        }
        values
    }

    fn typed_plugins(&mut self, value: &syn::Expr) {
        if !self.include_tests && Usage::test_only(Usage::expression_attributes(value)) {
            return;
        }
        match value {
            syn::Expr::Path(value) if value.qself.is_none() => {
                let mut path = Vec::new();
                for segment in &value.path.segments {
                    path.push(segment.ident.unraw().to_string());
                    if !matches!(segment.arguments, syn::PathArguments::None) {
                        let arguments = Self::arguments(&value.path);
                        self.plugin_arguments.insert((path.join("::"), arguments));
                        break;
                    }
                }
            }
            syn::Expr::Struct(value) => {
                let arguments = Self::arguments(&value.path);
                if !arguments.is_empty() {
                    self.plugin_arguments
                        .insert((Self::path(&value.path), arguments));
                }
            }
            syn::Expr::Tuple(value) => {
                for child in &value.elems {
                    self.typed_plugins(child);
                }
            }
            syn::Expr::Call(value) => self.typed_plugins(&value.func),
            syn::Expr::Paren(value) => self.typed_plugins(&value.expr),
            syn::Expr::Group(value) => self.typed_plugins(&value.expr),
            _ => {}
        }
    }

    fn plugin_values(&mut self, value: &syn::Expr) {
        if !self.include_tests && Usage::test_only(Usage::expression_attributes(value)) {
            return;
        }
        match value {
            syn::Expr::Path(value) if value.qself.is_none() => {
                let mut parts = Vec::new();
                for segment in &value.path.segments {
                    parts.push(segment.ident.unraw().to_string());
                }
                self.plugins.insert(parts.join("::"));
            }
            syn::Expr::Tuple(value) => {
                for value in &value.elems {
                    self.plugin_values(value);
                }
            }
            syn::Expr::Struct(value) => {
                let mut parts = Vec::new();
                for segment in &value.path.segments {
                    parts.push(segment.ident.unraw().to_string());
                }
                self.plugins.insert(parts.join("::"));
            }
            syn::Expr::Call(value) => self.plugin_values(&value.func),
            syn::Expr::MethodCall(value) => self.plugin_values(&value.receiver),
            syn::Expr::Paren(value) => self.plugin_values(&value.expr),
            syn::Expr::Group(value) => self.plugin_values(&value.expr),
            _ => {}
        }
    }
}

#[derive(Default)]
struct Usage {
    types: BTreeSet<String>,
    systems: BTreeSet<String>,
    active: bool,
    testing: bool,
    plugin: bool,
    owner: Option<String>,
    owners: BTreeMap<(String, String), BTreeSet<String>>,
    registration: bool,
    states: BTreeSet<(String, String, String, String)>,
    function: Option<String>,
    namespace: String,
    references: BTreeSet<(String, String)>,
}

struct Configuration<'a>(&'a syn::Expr);

impl Configuration<'_> {
    fn production(&self) -> bool {
        self.definite() != Some(false)
    }

    fn definite(&self) -> Option<bool> {
        match self.0 {
            syn::Expr::Path(value) if value.path.is_ident("test") => Some(false),
            syn::Expr::Call(call) => {
                let syn::Expr::Path(function) = call.func.as_ref() else {
                    return None;
                };
                if function.path.is_ident("not") && call.args.len() == 1 {
                    return Self(call.args.first().unwrap())
                        .definite()
                        .map(|value| !value);
                }
                let all = function.path.is_ident("all");
                if !all && !function.path.is_ident("any") {
                    return None;
                }
                let mut unknown = false;
                for argument in &call.args {
                    match Self(argument).definite() {
                        Some(value) if value != all => return Some(!all),
                        Some(_) => {}
                        None => unknown = true,
                    }
                }
                if unknown { None } else { Some(all) }
            }
            _ => None,
        }
    }
}

impl Usage {
    fn item_attributes(item: &Item) -> &[Attribute] {
        match item {
            Item::Const(value) => &value.attrs,
            Item::Enum(value) => &value.attrs,
            Item::ExternCrate(value) => &value.attrs,
            Item::Fn(value) => &value.attrs,
            Item::ForeignMod(value) => &value.attrs,
            Item::Impl(value) => &value.attrs,
            Item::Macro(value) => &value.attrs,
            Item::Mod(value) => &value.attrs,
            Item::Static(value) => &value.attrs,
            Item::Struct(value) => &value.attrs,
            Item::Trait(value) => &value.attrs,
            Item::TraitAlias(value) => &value.attrs,
            Item::Type(value) => &value.attrs,
            Item::Union(value) => &value.attrs,
            Item::Use(value) => &value.attrs,
            _ => &[],
        }
    }

    fn expression_attributes(expression: &syn::Expr) -> &[Attribute] {
        match expression {
            syn::Expr::Array(value) => &value.attrs,
            syn::Expr::Assign(value) => &value.attrs,
            syn::Expr::Async(value) => &value.attrs,
            syn::Expr::Await(value) => &value.attrs,
            syn::Expr::Binary(value) => &value.attrs,
            syn::Expr::Block(value) => &value.attrs,
            syn::Expr::Break(value) => &value.attrs,
            syn::Expr::Call(value) => &value.attrs,
            syn::Expr::Cast(value) => &value.attrs,
            syn::Expr::Closure(value) => &value.attrs,
            syn::Expr::Const(value) => &value.attrs,
            syn::Expr::Continue(value) => &value.attrs,
            syn::Expr::Field(value) => &value.attrs,
            syn::Expr::ForLoop(value) => &value.attrs,
            syn::Expr::Group(value) => &value.attrs,
            syn::Expr::If(value) => &value.attrs,
            syn::Expr::Index(value) => &value.attrs,
            syn::Expr::Infer(value) => &value.attrs,
            syn::Expr::Let(value) => &value.attrs,
            syn::Expr::Lit(value) => &value.attrs,
            syn::Expr::Loop(value) => &value.attrs,
            syn::Expr::Macro(value) => &value.attrs,
            syn::Expr::Match(value) => &value.attrs,
            syn::Expr::MethodCall(value) => &value.attrs,
            syn::Expr::Paren(value) => &value.attrs,
            syn::Expr::Path(value) => &value.attrs,
            syn::Expr::Range(value) => &value.attrs,
            syn::Expr::RawAddr(value) => &value.attrs,
            syn::Expr::Reference(value) => &value.attrs,
            syn::Expr::Repeat(value) => &value.attrs,
            syn::Expr::Return(value) => &value.attrs,
            syn::Expr::Struct(value) => &value.attrs,
            syn::Expr::Try(value) => &value.attrs,
            syn::Expr::TryBlock(value) => &value.attrs,
            syn::Expr::Tuple(value) => &value.attrs,
            syn::Expr::Unary(value) => &value.attrs,
            syn::Expr::Unsafe(value) => &value.attrs,
            syn::Expr::While(value) => &value.attrs,
            syn::Expr::Yield(value) => &value.attrs,
            _ => &[],
        }
    }

    fn test_only(attributes: &[Attribute]) -> bool {
        for attribute in attributes {
            if Self::test_meta(&attribute.meta) {
                return true;
            }
        }
        false
    }

    fn test_meta(meta: &syn::Meta) -> bool {
        if meta.path().is_ident("test") {
            return true;
        }
        let syn::Meta::List(list) = meta else {
            return false;
        };
        if list.path.is_ident("cfg") {
            return syn::parse2::<syn::Expr>(list.tokens.clone())
                .is_ok_and(|expression| !Configuration(&expression).production());
        }
        if !list.path.is_ident("cfg_attr") {
            return false;
        }
        let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
        let Ok(items) = parser.parse2(list.tokens.clone()) else {
            return false;
        };
        let Some(condition) = items.first() else {
            return false;
        };
        let Ok(condition) = syn::parse2::<syn::Expr>(condition.to_token_stream()) else {
            return false;
        };
        if Configuration(&condition).definite() != Some(true) {
            return false;
        }
        for item in items.iter().skip(1) {
            if Self::test_meta(item) {
                return true;
            }
        }
        false
    }

    fn arguments(&mut self, arguments: &syn::AngleBracketedGenericArguments) {
        let mut references = References::default();
        for argument in &arguments.args {
            if let syn::GenericArgument::Type(value) = argument {
                references.visit_type(value);
            }
        }
        self.types.extend(references.names);
        for path in references.paths {
            self.references.insert((self.namespace.clone(), path));
        }
    }
}

impl<'ast> Visit<'ast> for Usage {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if !Self::test_only(&local.attrs) {
            visit::visit_local(self, local);
        }
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if !Self::test_only(&arm.attrs) {
            visit::visit_arm(self, arm);
        }
    }
    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        if !Self::test_only(Self::expression_attributes(expression)) {
            visit::visit_expr(self, expression);
        }
    }

    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        let previous = self.namespace.clone();
        self.namespace = format!("{}::{}", self.namespace, module.ident);
        visit::visit_item_mod(self, module);
        self.namespace = previous;
    }
    fn visit_item(&mut self, item: &'ast Item) {
        let previous = self.testing;
        self.testing |= Self::test_only(Self::item_attributes(item));
        visit::visit_item(self, item);
        self.testing = previous;
    }

    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        let previous = self.plugin;
        let previous_owner = self.owner.clone();
        self.plugin = implementation.trait_.as_ref().is_some_and(|(_, path, _)| {
            path.segments.last().is_some_and(|part| {
                matches!(part.ident.to_string().as_str(), "Plugin" | "BevyPlugin")
            })
        });
        self.owner = if self.plugin {
            match implementation.self_ty.as_ref() {
                Type::Path(value) if value.qself.is_none() => Some(References::path(&value.path)),
                _ => None,
            }
        } else {
            None
        };
        visit::visit_item_impl(self, implementation);
        self.plugin = previous;
        self.owner = previous_owner;
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        let previous_testing = self.testing;
        self.testing |= Self::test_only(&function.attrs);
        let previous = self.registration;
        self.registration = self.plugin && function.sig.ident == "build";
        visit::visit_impl_item_fn(self, function);
        self.registration = previous;
        self.testing = previous_testing;
    }

    fn visit_expr_block(&mut self, block: &'ast syn::ExprBlock) {
        let previous = self.testing;
        self.testing |= Self::test_only(&block.attrs);
        visit::visit_expr_block(self, block);
        self.testing = previous;
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let previous_function = self
            .function
            .replace(format!("{}::{}", self.namespace, function.sig.ident));
        let previous = self.active;
        let previous_registration = self.registration;
        self.registration = !self.testing && function.sig.ident == "main";
        self.active = self
            .systems
            .contains(&format!("{}::{}", self.namespace, function.sig.ident))
            || function.sig.ident == "main";
        self.active &= !self.testing;
        if self.active {
            let mut references = References::default();
            references.visit_signature(&function.sig);
            self.types.extend(references.names);
            for path in references.paths {
                self.references.insert((self.namespace.clone(), path));
            }
        }
        visit::visit_item_fn(self, function);
        self.function = previous_function;
        self.active = previous;
        self.registration = previous_registration;
    }

    fn visit_type_path(&mut self, value: &'ast syn::TypePath) {
        if self.active
            && let Some(part) = value.path.segments.last()
            && matches!(
                part.ident.to_string().as_str(),
                "Query"
                    | "Single"
                    | "Res"
                    | "ResMut"
                    | "NonSend"
                    | "NonSendMut"
                    | "MessageReader"
                    | "MessageWriter"
                    | "On"
            )
            && let syn::PathArguments::AngleBracketed(arguments) = &part.arguments
        {
            self.arguments(arguments);
        }
        visit::visit_type_path(self, value);
    }

    fn visit_expr_method_call(&mut self, value: &'ast syn::ExprMethodCall) {
        if self.testing {
            return;
        }
        let method = value.method.to_string();
        if self.registration
            && matches!(
                method.as_str(),
                "configure_sets" | "in_set" | "before" | "after"
            )
        {
            for argument in &value.args {
                let mut references = References::default();
                references.visit_expr(argument);
                for path in references.paths {
                    self.references
                        .insert((self.namespace.clone(), path.clone()));
                    if let Some((owner, _)) = path.rsplit_once("::") {
                        self.references
                            .insert((self.namespace.clone(), owner.to_string()));
                    }
                }
            }
        }
        if self.registration
            && matches!(method.as_str(), "add_systems" | "add_observer" | "observe")
        {
            for argument in &value.args {
                let mut references = References::default();
                references.visit_expr(argument);
                for reference in references.paths {
                    let mut namespace = self.namespace.as_str();
                    let mut path = reference.as_str();
                    while let Some(rest) = path.strip_prefix("super::") {
                        namespace = namespace
                            .rsplit_once("::")
                            .map_or(namespace, |(parent, _)| parent);
                        path = rest;
                    }
                    let symbol = if let Some(rest) = path.strip_prefix("crate::") {
                        let root = namespace.split("::").next().unwrap();
                        format!("{root}::{rest}")
                    } else {
                        let path = path.strip_prefix("self::").unwrap_or(path);
                        format!("{namespace}::{path}")
                    };
                    self.systems.insert(symbol.clone());
                    if let Some(owner) = &self.owner {
                        self.owners
                            .entry((self.namespace.clone(), owner.clone()))
                            .or_default()
                            .insert(symbol);
                    }
                }
            }
        }
        if (self.registration || self.active)
            && matches!(
                method.as_str(),
                "spawn" | "insert" | "insert_resource" | "configure_sets"
            )
        {
            for argument in &value.args {
                let mut references = References::default();
                references.visit_expr(argument);
                self.types.extend(references.names);
                for path in references.paths {
                    self.references.insert((self.namespace.clone(), path));
                }
            }
        }
        if (self.registration || self.active)
            && matches!(
                method.as_str(),
                "init_resource"
                    | "insert_resource"
                    | "init_non_send"
                    | "add_message"
                    | "get"
                    | "get_mut"
                    | "query"
                    | "query_filtered"
                    | "resource"
                    | "resource_mut"
            )
            && let Some(arguments) = &value.turbofish
        {
            self.arguments(arguments);
        }
        visit::visit_expr_method_call(self, value);
    }

    fn visit_expr_call(&mut self, value: &'ast syn::ExprCall) {
        if !self.testing
            && self.function.is_some()
            && let syn::Expr::Path(function) = value.func.as_ref()
            && function
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "new")
        {
            let mut state = Vec::new();
            for segment in &function.path.segments {
                state.push(segment.ident.unraw().to_string());
                if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                    let mut references = References::default();
                    for argument in &arguments.args {
                        if let syn::GenericArgument::Type(value) = argument {
                            references.visit_type(value);
                        }
                    }
                    for reference in references.paths {
                        self.states.insert((
                            self.function.as_ref().unwrap().clone(),
                            self.namespace.clone(),
                            state.join("::"),
                            reference,
                        ));
                    }
                }
            }
        }
        visit::visit_expr_call(self, value);
    }
}

#[test]
fn local_job_readers_keep_their_verified_framework_imports() {
    for (imports, expected) in [
        ("use bevy_ecs::prelude::*;", true),
        ("use bevy::prelude::*;", true),
        ("use bevy::ecs::prelude::*;", true),
        ("use bevy_ecs::prelude::*; struct Query<T>(T);", false),
        ("use bevy_ecs::prelude::*; use unknown::*;", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source("crates/util/vmux_ecs/src/jobs.rs", "use bevy_ecs::prelude::*; pub trait Job { type Output; } #[derive(Component)] pub struct JobResult<J:Job>(pub Result<J::Output,String>);");
        inventory.source(
            "crates/feature/receiver/src/lib.rs",
            "mod child; mod unrelated; pub use unrelated::*;",
        );
        inventory.source(
            "crates/feature/receiver/src/unrelated.rs",
            "pub struct Unrelated;",
        );
        inventory.source("crates/feature/receiver/src/child.rs", &format!("use bevy_app::{{App,Plugin,Update}}; {imports} use vmux_ecs::jobs::{{Job,JobResult}}; struct Input; #[derive(Component)] struct Bytes(Vec<u8>); impl Job for Input {{ type Output=Bytes; }} fn receive(input:Query<&JobResult<Input>>) {{}} struct ReceiverPlugin; impl Plugin for ReceiverPlugin {{ fn build(&self,app:&mut App) {{ app.add_systems(Update,receive); }} }}"));
        inventory.resolve_usage();
        let output = &inventory.entries["crates/feature/receiver/src/child.rs||struct:Bytes"];
        assert_eq!(inventory.used(output), expected, "{imports}");
    }
}
