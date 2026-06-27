use vmux_docs::model::ItemKind;
use vmux_docs::translate::translate;

fn load() -> vmux_docs::model::CrateDoc {
    let raw = std::fs::read_to_string("tests/data/fixture.json").unwrap();
    let krate: rustdoc_types::Crate = serde_json::from_str(&raw).unwrap();
    translate(&krate)
}

#[test]
fn captures_public_items_only() {
    let doc = load();
    let names: Vec<&str> = doc.root.items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Widget"));
    assert!(names.contains(&"make"));
    assert!(names.contains(&"Mode"));
    assert!(!names.contains(&"Hidden"));
    assert!(!names.contains(&"AlsoHidden"));
}

#[test]
fn carries_docs_and_kind() {
    let doc = load();
    let widget = doc.root.items.iter().find(|i| i.name == "Widget").unwrap();
    assert_eq!(widget.kind, ItemKind::Struct);
    assert_eq!(widget.docs_md, "A documented struct.");
    assert!(widget.members.iter().any(|m| m.name == "width"));
}

#[test]
fn function_signature_rendered() {
    let doc = load();
    let make = doc.root.items.iter().find(|i| i.name == "make").unwrap();
    assert_eq!(make.kind, ItemKind::Function);
    assert_eq!(make.signature, "pub fn make(width: u32) -> Widget");
}

#[test]
fn captures_submodule_and_root_docs() {
    let doc = load();
    assert_eq!(doc.root.docs_md, "Fixture crate root docs.");
    let inner = doc
        .root
        .submodules
        .iter()
        .find(|m| m.path.ends_with("inner"))
        .unwrap();
    assert!(inner.items.iter().any(|i| i.name == "ANSWER"));
}
