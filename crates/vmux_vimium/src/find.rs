pub fn cycle(current: Option<usize>, total: usize, forward: bool) -> Option<usize> {
    if total == 0 {
        return None;
    }
    let next = match current {
        None => {
            if forward {
                0
            } else {
                total - 1
            }
        }
        Some(i) => {
            if forward {
                (i + 1) % total
            } else {
                (i + total - 1) % total
            }
        }
    };
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_forward_is_zero() {
        assert_eq!(cycle(None, 3, true), Some(0));
    }

    #[test]
    fn first_backward_is_last() {
        assert_eq!(cycle(None, 3, false), Some(2));
    }

    #[test]
    fn forward_wraps() {
        assert_eq!(cycle(Some(2), 3, true), Some(0));
    }

    #[test]
    fn backward_wraps() {
        assert_eq!(cycle(Some(0), 3, false), Some(2));
    }

    #[test]
    fn no_matches_is_none() {
        assert_eq!(cycle(None, 0, true), None);
        assert_eq!(cycle(Some(0), 0, false), None);
    }
}

#[cfg(target_arch = "wasm32")]
mod dom {
    use super::cycle;
    use crate::overlay;
    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, HtmlElement, HtmlInputElement};

    pub struct Find {
        matches: Vec<Element>,
        active: Option<usize>,
        input_open: bool,
    }

    impl Find {
        pub fn open(doc: &Document) -> Find {
            let sr = overlay::shadow(doc);
            let bar = doc.create_element("div").unwrap();
            bar.set_class_name("vmux-bar");
            bar.set_inner_html("<input class=\"vmux-find-input\" placeholder=\"find\u{2026}\"/>");
            sr.append_child(&bar).unwrap();
            if let Some(input) = sr.query_selector(".vmux-find-input").unwrap() {
                if let Some(h) = input.dyn_ref::<HtmlElement>() {
                    let _ = h.focus();
                }
            }
            Find {
                matches: Vec::new(),
                active: None,
                input_open: true,
            }
        }

        pub fn input_open(&self) -> bool {
            self.input_open
        }

        fn query(&self, doc: &Document) -> String {
            overlay::shadow(doc)
                .query_selector(".vmux-find-input")
                .unwrap()
                .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
                .map(|i| i.value())
                .unwrap_or_default()
        }

        pub fn search(&mut self, doc: &Document) {
            self.clear_highlights();
            self.matches.clear();
            self.active = None;
            let q = self.query(doc).to_lowercase();
            overlay::clear(doc);
            self.input_open = false;
            if q.is_empty() {
                return;
            }
            self.matches = super::dom_search::highlight(doc, &q);
            self.next(doc, true);
        }

        pub fn next(&mut self, _doc: &Document, forward: bool) {
            let total = self.matches.len();
            if let Some(prev) = self.active {
                if let Some(el) = self.matches.get(prev) {
                    el.set_class_name("vmux-find-hit");
                }
            }
            self.active = cycle(self.active, total, forward);
            if let Some(i) = self.active {
                let el = &self.matches[i];
                el.set_class_name("vmux-find-hit vmux-find-active");
                if let Some(h) = el.dyn_ref::<HtmlElement>() {
                    h.scroll_into_view();
                }
            }
        }

        fn clear_highlights(&self) {
            for el in &self.matches {
                if let Some(parent) = el.parent_node() {
                    let text = el.text_content().unwrap_or_default();
                    if let Some(owner) = el.owner_document() {
                        let tn = owner.create_text_node(&text);
                        let _ = parent.replace_child(&tn, el);
                    }
                }
            }
        }

        pub fn close(&self, doc: &Document) {
            self.clear_highlights();
            overlay::clear(doc);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod dom_search {
    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, Node};

    pub fn highlight(doc: &Document, query: &str) -> Vec<Element> {
        let mut hits = Vec::new();
        let Some(body) = doc.body() else {
            return hits;
        };
        collect(doc, body.unchecked_ref::<Node>(), query, &mut hits);
        hits
    }

    fn collect(doc: &Document, node: &Node, query: &str, hits: &mut Vec<Element>) {
        let children = node.child_nodes();
        for i in 0..children.length() {
            let child = children.get(i).unwrap();
            match child.node_type() {
                Node::TEXT_NODE => {
                    let text = child.text_content().unwrap_or_default();
                    if text.to_lowercase().contains(query) {
                        if let Some(parent) = child.parent_node() {
                            let span = doc.create_element("span").unwrap();
                            span.set_class_name("vmux-find-hit");
                            span.set_text_content(Some(&text));
                            if parent.replace_child(&span, &child).is_ok() {
                                hits.push(span);
                            }
                        }
                    }
                }
                Node::ELEMENT_NODE => {
                    let tag = child.node_name().to_lowercase();
                    if tag != "script" && tag != "style" && tag != "noscript" {
                        collect(doc, &child, query, hits);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use dom::Find;
