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
            if !inventory.matches(&boundary.key, &boundary.fingerprint)
                || !boundary.valid()
                || !boundary.references(inventory)
            {
                errors.push(format!(
                    "stale/incomplete adapter boundary {}",
                    boundary.key
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
                || !owner.references.contains(&entry.name)
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
                    "{}: declare its canonical owner or actual external boundary",
                    entry.key
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
        let mut canonical = false;
        let mut pre_world = 0;
        for endpoint in [&self.input, &self.output] {
            if let Some(key) = endpoint.strip_prefix("ECS: ") {
                let Some(entry) = inventory.entries.get(key) else {
                    return false;
                };
                if !(entry.contract || entry.ecs && inventory.used(entry)) {
                    return false;
                }
                let Some(adapter) = inventory.entries.get(&self.key) else {
                    return false;
                };
                let mut connected = adapter.references.contains(&entry.name);
                for implementation in inventory.entries.values() {
                    if implementation.path == adapter.path
                        && implementation.namespace == adapter.namespace
                        && implementation.owner.as_deref()
                            == Some(adapter.owner.as_deref().unwrap_or(adapter.name.as_str()))
                        && implementation.references.contains(&entry.name)
                    {
                        connected = true;
                    }
                }
                if !connected {
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
        path == "fixture.rs"
            || path.starts_with("crates/util/vmux_macro/")
            || path.starts_with("crates/app/vmux_builder/")
            || path.starts_with("scripts/")
            || path.starts_with("website/")
            || matches!(
                path,
                "crates/build_cfg.rs"
                    | "crates/build_cfg_git.rs"
                    | "crates/build_git_env.rs"
                    | "crates/build_platform_cfg.rs"
            )
    }

    fn fixture(inventory: &Inventory, name: &str) -> Self {
        let key = inventory.key(name);
        Self {
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
    contract: bool,
    projection: bool,
    test: bool,
    owner: Option<String>,
    references: BTreeSet<String>,
    fields: BTreeSet<String>,
    field_paths: BTreeSet<String>,
    generic_bindings: BTreeSet<String>,
    local: bool,
    dynamic: bool,
    opaque: bool,
}

impl Declaration {
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
        }
        .visit_file(&file);
    }

    fn script(&mut self, path: &str, source: &str) {
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
                contract: false,
                projection: false,
                test: false,
                owner: None,
                references: BTreeSet::new(),
                fields: BTreeSet::new(),
                field_paths: BTreeSet::new(),
                generic_bindings: BTreeSet::new(),
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
            && owner.references.contains(&entry.name)
        {
            return owner.ecs
                && self.used(owner)
                && !owner.local
                && !owner.dynamic
                && !owner.opaque;
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
            Kind::Function => self.registered_keys.contains(&entry.key),
            Kind::Struct | Kind::Enum => entry.ecs && self.used(entry),
            Kind::Alias => {
                self.used(entry)
                    && entry.fields.iter().any(|field| {
                        matches!(
                            field.as_str(),
                            "Query"
                                | "ParamSet"
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
            match self.resolve(&entry.namespace, reference, &mut BTreeSet::new()) {
                Some(field) if field.projection => return false,
                Some(field)
                    if field.path == "crates/util/vmux_ecs/src/ui_state.rs"
                        && field.name == "UiState" =>
                {
                    return false;
                }
                Some(field) if !self.canonical(field, policy, &mut visiting.clone()) => {
                    return false;
                }
                Some(_) => {}
                None if self.names.contains_key(name)
                    && !self.standard_result(entry, reference) =>
                {
                    return false;
                }
                None => {}
            }
        }
        true
    }

    fn used(&self, entry: &Declaration) -> bool {
        self.used_keys.contains(&entry.key)
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
        let mut keys = BTreeSet::new();
        let mut registered = BTreeSet::new();
        for usage in self.usage.values() {
            for symbol in &usage.systems {
                if let Some(entry) = self.resolve_function(symbol, &mut BTreeSet::new()) {
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
                if let Some(entry) = self.resolve(namespace, reference, &mut BTreeSet::new()) {
                    keys.insert(entry.key.clone());
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
        let (namespace, name) = symbol.rsplit_once("::")?;
        let target = self.imports.get(namespace)?.aliases.get(name)?;
        let mut namespace = namespace;
        let mut target = target.as_str();
        while let Some(rest) = target.strip_prefix("super::") {
            namespace = namespace.rsplit_once("::")?.0;
            target = rest;
        }
        if let Some(rest) = target.strip_prefix("crate::") {
            let root = namespace.split("::").next()?;
            return self.resolve_function(&format!("{root}::{rest}"), visiting);
        }
        let target = target.strip_prefix("self::").unwrap_or(target);
        self.resolve_function(&format!("{namespace}::{target}"), visiting)
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
        let mut references = References::default();
        let tokens = node.to_token_stream();
        let mut generic_bindings = BTreeSet::new();
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            references.visit_file(&file);
            for item in &file.items {
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
        let mut contract = false;
        let mut projection = false;
        let mut test = false;
        for attr in attrs {
            if attr.path().is_ident("test") {
                test = true;
            }
            if attr.path().segments.first().is_some_and(|segment| {
                matches!(
                    segment.ident.to_string().as_str(),
                    "vmux_api" | "vmux_macro"
                )
            }) && attr.path().segments.last().is_some_and(|segment| {
                matches!(
                    segment.ident.to_string().as_str(),
                    "ui_event" | "ui_state" | "ui_state_patch" | "contract" | "agent" | "string_id"
                )
            }) {
                contract = true;
                projection = attr.path().segments.last().is_some_and(|segment| {
                    matches!(
                        segment.ident.to_string().as_str(),
                        "ui_state" | "ui_state_patch"
                    )
                });
            }
            if attr.path().is_ident("derive") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.segments.last().is_some_and(|segment| {
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
                    Ok(())
                });
            }
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
            contract,
            projection,
            test,
            owner,
            references: references.names,
            fields: field_names.names,
            field_paths: field_names.paths,
            generic_bindings,
            local: references.local,
            dynamic: references.dynamic,
            opaque: references.opaque,
        };
        if matches!(kind, Kind::Struct | Kind::Enum | Kind::Alias | Kind::Trait) {
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
            self.inventory
                .names
                .entry(declaration.name.clone())
                .or_default()
                .push(key.clone());
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
                    fields.push(field.ty.clone());
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
                    for field in &variant.fields {
                        fields.push(field.ty.clone());
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
                &[],
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
                    if let syn::FnArg::Typed(argument) = argument {
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
                    Type::Path(path) => {
                        path.path.segments.last().map(|part| part.ident.to_string())
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
                return;
            }
            _ => {}
        }
        visit::visit_item(self, item);
    }
}

#[derive(Default)]
struct References {
    names: BTreeSet<String>,
    local: bool,
    dynamic: bool,
    opaque: bool,
    paths: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for References {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let mut parts = Vec::new();
        for part in &path.segments {
            parts.push(part.ident.to_string());
        }
        self.paths.insert(parts.join("::"));
        if let Some(last) = path.segments.last() {
            let name = last.ident.to_string();
            self.local |= name == "Local";
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

#[derive(Default)]
struct Usage {
    types: BTreeSet<String>,
    systems: BTreeSet<String>,
    active: bool,
    testing: bool,
    plugin: bool,
    registration: bool,
    namespace: String,
    references: BTreeSet<(String, String)>,
}

struct Configuration<'a>(&'a syn::Expr);

impl Configuration<'_> {
    fn production(&self) -> bool {
        match self.0 {
            syn::Expr::Path(value) => !value.path.is_ident("test"),
            syn::Expr::Call(call) => {
                let syn::Expr::Path(function) = call.func.as_ref() else {
                    return true;
                };
                if function.path.is_ident("all") {
                    for argument in &call.args {
                        if !Self(argument).production() {
                            return false;
                        }
                    }
                    return true;
                }
                if function.path.is_ident("any") {
                    for argument in &call.args {
                        if Self(argument).production() {
                            return true;
                        }
                    }
                    return false;
                }
                if function.path.is_ident("not")
                    && let Some(argument) = call.args.first()
                {
                    return !Self(argument).production();
                }
                true
            }
            _ => true,
        }
    }
}

impl Usage {
    fn test_only(attributes: &[Attribute]) -> bool {
        for attribute in attributes {
            if attribute.path().is_ident("test") {
                return true;
            }
            if attribute.path().is_ident("cfg")
                && let syn::Meta::List(meta) = &attribute.meta
                && let Ok(expression) = syn::parse2::<syn::Expr>(meta.tokens.clone())
                && !Configuration(&expression).production()
            {
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
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        let previous = self.namespace.clone();
        self.namespace = format!("{}::{}", self.namespace, module.ident);
        visit::visit_item_mod(self, module);
        self.namespace = previous;
    }
    fn visit_item(&mut self, item: &'ast Item) {
        let previous = self.testing;
        let attributes: &[Attribute] = match item {
            Item::Fn(value) => &value.attrs,
            Item::Mod(value) => &value.attrs,
            Item::Impl(value) => &value.attrs,
            _ => &[],
        };
        for attribute in attributes {
            if attribute.path().is_ident("test") {
                self.testing = true;
            }
            if attribute.path().is_ident("cfg")
                && let syn::Meta::List(meta) = &attribute.meta
                && let Ok(value) = syn::parse2::<syn::Expr>(meta.tokens.clone())
            {
                self.testing |= !Configuration(&value).production();
            }
        }
        visit::visit_item(self, item);
        self.testing = previous;
    }

    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        let previous = self.plugin;
        self.plugin = implementation.trait_.as_ref().is_some_and(|(_, path, _)| {
            path.segments.last().is_some_and(|part| {
                matches!(part.ident.to_string().as_str(), "Plugin" | "BevyPlugin")
            })
        });
        visit::visit_item_impl(self, implementation);
        self.plugin = previous;
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
                    if let Some(rest) = path.strip_prefix("crate::") {
                        let root = namespace.split("::").next().unwrap();
                        self.systems.insert(format!("{root}::{rest}"));
                    } else {
                        let path = path.strip_prefix("self::").unwrap_or(path);
                        self.systems.insert(format!("{namespace}::{path}"));
                    }
                }
            }
        }
        if (self.registration || self.active)
            && matches!(method.as_str(), "spawn" | "insert" | "insert_resource")
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
            )
            && let Some(arguments) = &value.turbofish
        {
            self.arguments(arguments);
        }
        visit::visit_expr_method_call(self, value);
    }
}
