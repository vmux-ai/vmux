use web_sys::{Document, Element, ShadowRoot, ShadowRootInit, ShadowRootMode};

pub const HOST_ID: &str = "__vmux_vimium_host";

pub fn shadow(doc: &Document) -> ShadowRoot {
    if let Some(host) = doc.get_element_by_id(HOST_ID) {
        if let Some(sr) = host.shadow_root() {
            return sr;
        }
    }
    let host: Element = doc.create_element("div").unwrap();
    host.set_id(HOST_ID);
    doc.document_element().unwrap().append_child(&host).unwrap();
    let sr = host
        .attach_shadow(&ShadowRootInit::new(ShadowRootMode::Open))
        .unwrap();
    let style = doc.create_element("style").unwrap();
    style.set_text_content(Some(BASE_CSS));
    sr.append_child(&style).unwrap();
    sr
}

pub fn clear(doc: &Document) {
    if let Some(host) = doc.get_element_by_id(HOST_ID) {
        if let Some(sr) = host.shadow_root() {
            while let Some(last) = sr.last_element_child() {
                if last.tag_name().eq_ignore_ascii_case("style") {
                    break;
                }
                last.remove();
            }
        }
    }
}

const BASE_CSS: &str = "\
.vmux-hint{position:fixed;z-index:2147483647;background:#fffa65;color:#202020;\
border:1px solid #c8a000;border-radius:3px;padding:0 3px;font:bold 11px monospace;\
box-shadow:0 1px 2px rgba(0,0,0,.4);}\
.vmux-hint .typed{color:#b00;}\
.vmux-bar{position:fixed;left:0;right:0;bottom:0;z-index:2147483647;background:#202124;\
color:#eee;font:14px system-ui;padding:8px 12px;display:flex;gap:8px;}\
.vmux-bar input{flex:1;background:#303134;color:#eee;border:0;outline:0;padding:6px 8px;\
border-radius:6px;font:14px system-ui;}\
.vmux-find-hit{background:#fffa65;color:#000;}\
.vmux-find-active{background:#ff9632;color:#000;}";
