use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::ext::IdentExt;
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::{Attribute, Item, Type};

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
fn new_declarations_have_canonical_ownership_or_an_explicit_boundary() {
    let root = Workspace::current();
    let source = std::fs::read_to_string(root.0.join("ecs-policy.ron")).expect("ownership policy");
    let policy: Policy = ron::from_str(&source).expect("valid ownership policy");
    let architecture = std::fs::read_to_string(root.0.join("docs/architecture.md"))
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
        ("", "vmux_ecs::manifest::FeatureManifestSource", true),
        (
            "{ state: String }",
            "vmux_ecs::manifest::FeatureManifestSource",
            false,
        ),
        (
            "{ callback: fn() }",
            "vmux_ecs::manifest::FeatureManifestSource",
            false,
        ),
        (
            "{ unit: () }",
            "vmux_ecs::manifest::FeatureManifestSource",
            false,
        ),
        ("", "LocalSource", false),
    ] {
        let mut inventory = Inventory::default();
        inventory.source(
            "crates/util/vmux_ecs/src/manifest.rs",
            "pub trait FeatureManifestSource { const SOURCE: &'static str; }",
        );
        let terminator = if fields.is_empty() { ";" } else { "" };
        inventory.source("crates/feature/example/src/lib.rs", &format!("trait LocalSource {{ const SOURCE: &'static str; }} struct Feature {fields}{terminator} impl {interface} for Feature {{ const SOURCE: &'static str = \"manifest\"; }}"));
        inventory.resolve_usage();
        let marker = &inventory.entries["crates/feature/example/src/lib.rs||struct:Feature"];
        assert_eq!(inventory.manifest_source(marker), allowed);
    }
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
#[serde(deny_unknown_fields)]
struct Policy {
    revision: String,
    legacy: Vec<Legacy>,
    boundaries: Vec<Boundary>,
    values: Vec<ValueOwner>,
    upstream: Vec<Upstream>,
}

impl Policy {
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
        errors
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
            loop {
                let Some(parent) = inventory.entries.get(current) else {
                    return false;
                };
                let contract = policy
                    .boundaries
                    .iter()
                    .find(|boundary| boundary.key == current);
                let Some(contract) = contract else {
                    if !parent.ecs
                        || !inventory.used(parent)
                        || !inventory.canonical(parent, policy, &mut BTreeSet::new())
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
                if !(entry.contract || entry.ecs && inventory.used(entry) || owned_value) {
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
        (!adapter.production
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
    ecs: bool,
    parameter: bool,
    plugin: bool,
    plugin_path: Option<String>,
    parameter_path: Option<String>,
    plugin_children: BTreeSet<String>,
    registered_systems: BTreeSet<(String, String)>,
    field_types: Vec<Type>,
    relation_target: Option<String>,
    relation_source: Option<String>,
    component_hooks: BTreeSet<String>,
    contract: bool,
    contract_paths: BTreeSet<String>,
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
    dom_mounts: BTreeSet<(String, String)>,
    rendered: BTreeSet<String>,
    renderer: bool,
    fields: BTreeSet<String>,
    field_paths: BTreeSet<String>,
    generic_bindings: BTreeSet<String>,
    component_bindings: BTreeMap<String, String>,
    local: bool,
    dynamic: bool,
    opaque: bool,
}

impl Declaration {
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

#[derive(Default)]
struct Inventory {
    entries: BTreeMap<String, Declaration>,
    usage: BTreeMap<String, Usage>,
    names: BTreeMap<String, Vec<String>>,
    symbols: BTreeMap<String, Vec<String>>,
    imports: BTreeMap<String, Imports>,
    used_keys: BTreeSet<String>,
    roots: BTreeSet<String>,
    functions: BTreeMap<String, Vec<String>>,
    registered_keys: BTreeSet<String>,
    lookup_depth: Cell<usize>,
    root_bindings: RefCell<BTreeMap<(String, String), Option<String>>>,
    owner_reference_cache: RefCell<BTreeMap<(String, String), bool>>,
}

impl Inventory {
    fn workspace(root: &Path) -> Self {
        let mut inventory = Self::default();
        for directory in ["crates", "examples", "website", "patches", "scripts"] {
            inventory.walk(root, &root.join(directory));
        }
        inventory.resolve_usage();
        inventory
    }

    fn walk(&mut self, root: &Path, directory: &Path) {
        for item in std::fs::read_dir(directory).expect("ownership source directory") {
            let path = item.expect("ownership directory entry").path();
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
                self.source(relative, &source);
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
        self.root_bindings.get_mut().clear();
        self.owner_reference_cache.get_mut().clear();
        if !path.ends_with(".rs") {
            self.script(path, source);
            return;
        }
        let file = syn::parse_file(source).unwrap_or_else(|error| panic!("{path}: {error}"));
        let mut usage = Usage {
            testing: path.contains("/tests/"),
            namespace: Namespace::of(path, &[]).0,
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
    }

    fn script(&mut self, path: &str, source: &str) {
        self.owner_reference_cache.get_mut().clear();
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
                ecs: false,
                parameter: false,
                plugin: false,
                plugin_path: None,
                parameter_path: None,
                plugin_children: BTreeSet::new(),
                registered_systems: BTreeSet::new(),
                field_types: Vec::new(),
                relation_target: None,
                relation_source: None,
                component_hooks: BTreeSet::new(),
                contract: false,
                contract_paths: BTreeSet::new(),
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
                dom_mounts: BTreeSet::new(),
                rendered: BTreeSet::new(),
                renderer: false,
                fields: BTreeSet::new(),
                field_paths: BTreeSet::new(),
                generic_bindings: BTreeSet::new(),
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
                    && entry.fields.iter().any(|field| {
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
                    })
            }
            _ => false,
        };
        if !owned {
            return false;
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

    fn fixture_lock(&self, adapter: &Declaration) -> bool {
        !adapter.production
            && matches!(adapter.kind, Kind::Static)
            && adapter.unit_mutex
            && adapter.fields == BTreeSet::from(["Mutex".into()])
            && adapter.field_paths == BTreeSet::from(["std::sync::Mutex".into()])
            && !self.local_binding(&adapter.namespace, "std")
    }

    fn manifest_source(&self, adapter: &Declaration) -> bool {
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
                        == "crates/util/vmux_ecs/src/manifest.rs||trait:FeatureManifestSource"
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
        for implementation in self.entries.values() {
            if owner.production && !implementation.production {
                continue;
            }
            let Some(path) = &implementation.owner else {
                continue;
            };
            if self
                .resolve(&implementation.namespace, path, &mut BTreeSet::new())
                .is_some_and(|resolved| resolved.key == owner.key)
            {
                candidates.push(implementation);
            }
        }
        for candidate in candidates {
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
                    if let Some(resolved) =
                        self.resolve(&candidate.namespace, path, &mut BTreeSet::new())
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

    fn boundary_references(
        &self,
        adapter: &Declaration,
        endpoint: &Declaration,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let owner = match &adapter.owner {
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
        if (owner.parameter
            || matches!(owner.kind, Kind::Function) && self.registered_keys.contains(&owner.key))
            && self.borrowed_endpoint(owner, endpoint)
        {
            return true;
        }
        for child in &owner.plugin_children {
            if let Some(child) = self.resolve(&owner.namespace, child, &mut BTreeSet::new())
                && self.boundary_references(child, endpoint, &mut visiting.clone())
            {
                return true;
            }
        }
        for implementation in self.entries.values() {
            if !implementation.plugin || owner.production && !implementation.production {
                continue;
            }
            let Some(path) = &implementation.owner else {
                continue;
            };
            if self
                .resolve(&implementation.namespace, path, &mut BTreeSet::new())
                .is_none_or(|resolved| resolved.key != owner.key)
            {
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
                            || self.borrowed_endpoint(function, endpoint))
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
                        if self.boundary_references(child, endpoint, &mut visiting.clone()) {
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
                "App" | "Plugin" => matches!(
                    prefix,
                    "bevy_app" | "bevy_app::prelude" | "bevy::app" | "bevy::prelude"
                ),
                "Query" | "Single" | "Res" | "ResMut" | "NonSend" | "NonSendMut" | "ParamSet"
                | "SystemParam" => {
                    matches!(
                        prefix,
                        "bevy_ecs::system"
                            | "bevy_ecs::prelude"
                            | "bevy::ecs::system"
                            | "bevy::ecs::prelude"
                            | "bevy::prelude"
                    )
                }
                "On" => matches!(
                    prefix,
                    "bevy_ecs::observer"
                        | "bevy_ecs::prelude"
                        | "bevy::ecs::observer"
                        | "bevy::ecs::prelude"
                        | "bevy::prelude"
                ),
                "With" | "Without" | "Added" | "Changed" | "Or" | "Has" => matches!(
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
                        return None;
                    }
                }
            }
            scope = current.rsplit_once("::").map(|(parent, _)| parent);
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
                            .as_deref()
                            == Some("SystemParam");
                    if (parameter || matches!(entry.kind, Kind::Alias))
                        && self.used(entry)
                        && entry.production
                        && visiting.insert(entry.key.clone())
                    {
                        return entry.field_types.iter().any(|value| {
                            self.borrowed_type(
                                &entry.namespace,
                                value,
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
                    "Query" | "Single" | "Res" | "ResMut" | "NonSend" | "NonSendMut" | "On" => true,
                    "With" | "Without" | "Added" | "Changed" | "Or" | "Has" | "Ref" | "Mut" => {
                        inside
                    }
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
                let component = !matches!(handle.as_str(), "Query" | "Single" | "ParamSet");
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
        if matches!(reference, "std::result::Result" | "core::result::Result") {
            return !self.local_binding(namespace, reference.split("::").next().unwrap());
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
                        "std::result::Result" | "core::result::Result"
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
        if !visiting.insert(format!("module:{namespace}|{name}")) {
            return None;
        }
        let imports = self.imports.get(namespace)?;
        if imports.ambiguous.contains(name) || imports.opaque_modules.contains(name) {
            return None;
        }
        if imports.modules.contains(name) {
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
        self.owner_reference_cache.get_mut().clear();
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
        self.used_keys = keys;
        self.registered_keys = registered;
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
                    return self.resolve(
                        &format!("{current}::{first}"),
                        &parts[1..].join("::"),
                        visiting,
                    );
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
            let package = package.split("-0.").next().unwrap().replace('-', "_");
            module.push(package);
            for part in &parts[source + 1..] {
                let name = part.trim_end_matches(".rs");
                if !matches!(name, "lib" | "main" | "mod") {
                    module.push(name.into());
                }
            }
        } else {
            module.push(path.trim_end_matches(".rs").replace('/', "::"));
        }
        for part in scope {
            if !part.starts_with("fn ") && !part.starts_with("impl ") {
                module.push(part.strip_prefix("r#").unwrap_or(part).to_string());
            }
        }
        Self(module.join("::"))
    }
}

#[derive(Default)]
struct Imports {
    aliases: BTreeMap<String, String>,
    ambiguous: BTreeSet<String>,
    globs: BTreeSet<String>,
    modules: BTreeSet<String>,
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
        let mut component_bindings = BTreeMap::new();
        let mut plugin = false;
        let mut plugin_path = None;
        let mut parameter_path = None;
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            references.visit_file(&file);
            for item in &file.items {
                if let Item::Impl(implementation) = item {
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
                                "on_add" | "on_insert" | "on_replace" | "on_remove" | "on_despawn"
                            )
                        }) {
                            continue;
                        }
                        let syn::Expr::Path(callback) = value.value else {
                            continue;
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
                    component |= path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "Component");
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
                                | "SystemSet"
                        )
                    }) {
                        ecs = true;
                    }
                    if path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "SystemParam")
                    {
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
        let declaration = Declaration {
            key: key.clone(),
            path: self.path.into(),
            namespace: Namespace::of(self.path, &self.scope).0,
            name,
            kind,
            fingerprint: Declaration::fingerprint(&tokens.to_string()),
            ecs,
            parameter,
            plugin,
            plugin_path,
            parameter_path,
            plugin_children: references.plugins,
            registered_systems: references.registered_systems,
            field_types: match syn::parse2::<Item>(tokens.to_token_stream()) {
                Ok(Item::Struct(value)) => value
                    .fields
                    .iter()
                    .filter(|field| !Usage::test_only(&field.attrs))
                    .map(|field| field.ty.clone())
                    .collect(),
                Ok(Item::Enum(value)) => value
                    .variants
                    .iter()
                    .filter(|variant| !Usage::test_only(&variant.attrs))
                    .flat_map(|variant| variant.fields.iter())
                    .filter(|field| !Usage::test_only(&field.attrs))
                    .map(|field| field.ty.clone())
                    .collect(),
                Ok(Item::Type(value)) => vec![*value.ty],
                Ok(Item::Fn(value)) => value
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
            relation_target,
            relation_source,
            component_hooks,
            contract: false,
            contract_paths,
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
            dom_mounts: references.dom_mounts,
            rendered: references.rendered,
            renderer: matches!(kind, Kind::Function)
                && Self::metadata(attrs, true)
                    .iter()
                    .any(|attribute| attribute.path().is_ident("component")),
            fields: field_names.names,
            field_paths: field_names.paths,
            generic_bindings,
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
                    .entry(Namespace::of(self.path, &self.scope).0)
                    .or_default()
                    .bind_alias(name.unraw().to_string(), value.ident.unraw().to_string());
            }
            Item::Use(value)
                if !self
                    .scope
                    .iter()
                    .any(|scope| scope.starts_with("fn ") || scope.starts_with("impl ")) =>
            {
                let namespace = Namespace::of(self.path, &self.scope).0;
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
                let namespace = Namespace::of(self.path, &self.scope).0;
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
                        .entry(Namespace::of(self.path, &self.scope).0)
                        .or_default()
                        .opaque_modules
                        .insert(value.ident.unraw().to_string());
                }
                self.inventory
                    .imports
                    .entry(Namespace::of(self.path, &self.scope).0)
                    .or_default()
                    .modules
                    .insert(value.ident.unraw().to_string());
                self.scope.push(value.ident.to_string());
                self.inventory
                    .imports
                    .entry(Namespace::of(self.path, &self.scope).0)
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
struct References {
    include_tests: bool,
    plugins: BTreeSet<String>,
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
    dom_mounts: BTreeSet<(String, String)>,
}

impl<'ast> Visit<'ast> for References {
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
            let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
            if let Ok(arguments) = parser.parse2(arguments.tokens.clone()) {
                for argument in arguments {
                    let syn::Meta::NameValue(argument) = argument else {
                        continue;
                    };
                    if !argument.path.is_ident("takes") {
                        continue;
                    }
                    let syn::Expr::Path(input) = argument.value else {
                        continue;
                    };
                    let attribute = attribute
                        .path()
                        .to_token_stream()
                        .to_string()
                        .replace(' ', "");
                    let input = input.path.to_token_stream().to_string().replace(' ', "");
                    self.page_inputs.insert((attribute, input));
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
        self.local |= value
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Local");
        visit::visit_type_path(self, value);
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

impl References {
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
                    self.registered_systems.insert((app_type.clone(), path));
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
