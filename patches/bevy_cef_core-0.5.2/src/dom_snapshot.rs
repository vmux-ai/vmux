use crate::prelude::{IntoString, PROCESS_MESSAGE_SNAPSHOT_RESULT};
use cef::rc::Rc;
use cef::{
    CefString, Domdocument, Domnode, Domvisitor, Frame, ImplDomdocument, ImplDomnode,
    ImplDomvisitor, ImplFrame, ImplListValue, ImplProcessMessage, ImplV8Context, ImplV8Value,
    ProcessId, V8Value, WrapDomvisitor, process_message_create, wrap_domvisitor,
};
use cef_dll_sys::cef_process_id_t;

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
const RAW_NODE_CAP: usize = 3000;
const MAX_WALK_DEPTH: usize = 2048;
const EMPTY_SNAPSHOT: &str = "{\"url\":\"\",\"title\":\"\",\"nodes\":[]}";

pub fn request_dom_snapshot(frame: &Frame, request_id: &str) {
    let mut visitor = SnapshotVisitor::new(frame.clone(), request_id.to_string());
    frame.visit_dom(Some(&mut visitor));
}

const VIEWPORT_SCRIPT: &str = "(function(){try{var d=document.documentElement||{},b=document.body||{};return [(window.scrollX||0)|0,(window.scrollY||0)|0,(window.innerWidth||0)|0,(window.innerHeight||0)|0,Math.max(d.scrollWidth||0,b.scrollWidth||0)|0,Math.max(d.scrollHeight||0,b.scrollHeight||0)|0].join(',');}catch(e){return '';}})()";

fn viewport_json(frame: &Frame) -> serde_json::Value {
    parse_viewport_from_frame(frame).unwrap_or(serde_json::Value::Null)
}

fn parse_viewport_from_frame(frame: &Frame) -> Option<serde_json::Value> {
    let context = frame.v8_context()?;
    let script: CefString = VIEWPORT_SCRIPT.into();
    context.enter();
    let mut retval: Option<V8Value> = None;
    let ok = context.eval(Some(&script), None, 0, Some(&mut retval), None);
    let csv = if ok != 0 {
        retval
            .filter(|v| v.is_string() != 0)
            .map(|v| v.string_value().into_string())
    } else {
        None
    };
    context.exit();
    let nums: Vec<i32> = csv?
        .split(',')
        .filter_map(|part| part.trim().parse::<i32>().ok())
        .collect();
    if nums.len() != 6 {
        return None;
    }
    Some(serde_json::json!({
        "scrollX": nums[0],
        "scrollY": nums[1],
        "width": nums[2],
        "height": nums[3],
        "pageWidth": nums[4],
        "pageHeight": nums[5],
    }))
}

fn build_json(frame: &Frame, document: Option<&mut Domdocument>) -> String {
    let Some(document) = document else {
        return EMPTY_SNAPSHOT.to_string();
    };
    let url = document.base_url().into_string();
    let title = document.title().into_string();
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    if let Some(body) = document.body() {
        walk(&body, &mut nodes, 0);
    }
    let value = serde_json::json!({
        "url": url,
        "title": title,
        "nodes": nodes,
        "viewport": viewport_json(frame),
    });
    serde_json::to_string(&value).unwrap_or_else(|_| EMPTY_SNAPSHOT.to_string())
}

fn walk(node: &Domnode, out: &mut Vec<serde_json::Value>, depth: usize) {
    if out.len() >= RAW_NODE_CAP || depth >= MAX_WALK_DEPTH {
        return;
    }
    if node.is_element() != 0 {
        out.push(node_json(node));
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if out.len() >= RAW_NODE_CAP {
            break;
        }
        walk(&current, out, depth + 1);
        child = current.next_sibling();
    }
}

fn node_json(node: &Domnode) -> serde_json::Value {
    let tag = node.element_tag_name().into_string().to_lowercase();
    let mut text = node.element_inner_text().into_string();
    if text.chars().count() > NAME_CAP {
        text = text.chars().take(NAME_CAP).collect();
    }
    let mut attrs: Vec<(String, String)> = Vec::new();
    for key in SNAPSHOT_ATTRS {
        let cef_key: cef::CefString = (*key).into();
        if node.has_element_attribute(Some(&cef_key)) != 0 {
            let v = node.element_attribute(Some(&cef_key)).into_string();
            attrs.push(((*key).to_string(), v));
        }
    }
    let is_password = tag == "input"
        && attrs
            .iter()
            .any(|(k, v)| k == "type" && v.eq_ignore_ascii_case("password"));
    let value = if is_password {
        String::new()
    } else {
        node.value().into_string()
    };
    let bounds = node.element_bounds();
    serde_json::json!({
        "tag": tag,
        "text": text,
        "value": value,
        "attrs": attrs,
        "bounds": [bounds.x, bounds.y, bounds.width, bounds.height],
    })
}

fn send_result(frame: &Frame, request_id: &str, json: &str) {
    if let Some(mut message) = process_message_create(Some(&PROCESS_MESSAGE_SNAPSHOT_RESULT.into()))
        && let Some(args) = message.argument_list()
    {
        args.set_string(0, Some(&request_id.into()));
        args.set_string(1, Some(&json.into()));
        frame.send_process_message(
            ProcessId::from(cef_process_id_t::PID_BROWSER),
            Some(&mut message),
        );
    }
}

wrap_domvisitor! {
    struct SnapshotVisitor {
        frame: Frame,
        request_id: String,
    }
    impl Domvisitor {
        fn visit(&self, document: Option<&mut Domdocument>) {
            let json = build_json(&self.frame, document);
            send_result(&self.frame, &self.request_id, &json);
        }
    }
}
