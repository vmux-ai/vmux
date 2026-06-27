use dioxus::prelude::*;
use vmux_website::api::model::{Item, ItemKind, Member};
use vmux_website::api::{RenderItemProbe, RenderItemProbeProps};

fn sample() -> Item {
    Item {
        kind: ItemKind::Function,
        name: "make".into(),
        path: "fixture::make".into(),
        signature: "pub fn make(width: u32) -> Widget".into(),
        docs_md: "A documented function.".into(),
        members: vec![Member {
            name: "ignored".into(),
            signature: "ignored: ()".into(),
            docs_md: String::new(),
        }],
        links: vec![],
    }
}

#[test]
fn item_renders_signature_and_docs() {
    let props = RenderItemProbeProps { item: sample() };
    let mut dom = VirtualDom::new_with_props(RenderItemProbe, props);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("make"), "missing name: {html}");
    assert!(html.contains("documented function"), "missing docs: {html}");
    assert!(html.contains("width"), "missing signature param: {html}");
    assert!(html.contains("Widget"), "missing signature return: {html}");
}
