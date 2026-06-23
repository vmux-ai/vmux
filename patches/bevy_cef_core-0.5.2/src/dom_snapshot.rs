use crate::util::IntoString;
use cef::rc::Rc;
use cef::{
    Domdocument, Domnode, Domvisitor, Frame, ImplDomdocument, ImplDomnode, ImplDomvisitor, ImplFrame,
    WrapDomvisitor, wrap_domvisitor,
};
use std::cell::RefCell;

const SNAPSHOT_ATTRS: &[&str] = &[
    "role",
    "aria-label",
    "aria-expanded",
    "aria-selected",
    "alt",
    "title",
    "placeholder",
    "type",
    "name",
    "href",
    "id",
    "tabindex",
    "disabled",
    "required",
    "checked",
];

const NAME_CAP: usize = 200;
const EMPTY_SNAPSHOT: &str = "{\"url\":\"\",\"title\":\"\",\"nodes\":[]}";

pub fn capture_snapshot_json(frame: &Frame) -> String {
    let sink = std::rc::Rc::new(RefCell::new(String::new()));
    let mut visitor = SnapshotVisitor::new(sink.clone());
    frame.visit_dom(Some(&mut visitor));
    let json = sink.borrow().clone();
    if json.is_empty() {
        EMPTY_SNAPSHOT.to_string()
    } else {
        json
    }
}

fn build_json(document: Option<&mut Domdocument>) -> String {
    let Some(document) = document else {
        return String::new();
    };
    let url = document.base_url().into_string();
    let title = document.title().into_string();
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    if let Some(body) = document.body() {
        walk(&body, &mut nodes);
    }
    let value = serde_json::json!({
        "url": url,
        "title": title,
        "nodes": nodes,
    });
    serde_json::to_string(&value).unwrap_or_default()
}

fn walk(node: &Domnode, out: &mut Vec<serde_json::Value>) {
    if node.is_element() != 0 {
        out.push(node_json(node));
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        walk(&current, out);
        child = current.next_sibling();
    }
}

fn node_json(node: &Domnode) -> serde_json::Value {
    let tag = node.element_tag_name().into_string().to_lowercase();
    let mut text = node.element_inner_text().into_string();
    if text.chars().count() > NAME_CAP {
        text = text.chars().take(NAME_CAP).collect();
    }
    let value = node.value().into_string();
    let mut attrs: Vec<(String, String)> = Vec::new();
    for key in SNAPSHOT_ATTRS {
        let cef_key: cef::CefString = (*key).into();
        if node.has_element_attribute(Some(&cef_key)) != 0 {
            let v = node.element_attribute(Some(&cef_key)).into_string();
            attrs.push(((*key).to_string(), v));
        }
    }
    let bounds = node.element_bounds();
    serde_json::json!({
        "tag": tag,
        "text": text,
        "value": value,
        "attrs": attrs,
        "bounds": [bounds.x, bounds.y, bounds.width, bounds.height],
    })
}

wrap_domvisitor! {
    struct SnapshotVisitor {
        sink: std::rc::Rc<RefCell<String>>,
    }
    impl Domvisitor {
        fn visit(&self, document: Option<&mut Domdocument>) {
            *self.sink.borrow_mut() = build_json(document);
        }
    }
}
