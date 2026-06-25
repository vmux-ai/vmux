const ALPHABET: &[u8] = b"sadfjklewcmpgh";

pub fn generate_labels(count: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    let n = ALPHABET.len();
    let mut len = 1;
    let mut cap = n;
    while cap < count {
        len += 1;
        cap = cap.saturating_mul(n);
    }
    (0..count).map(|i| encode_label(i, len)).collect()
}

fn encode_label(mut idx: usize, len: usize) -> String {
    let n = ALPHABET.len();
    let mut chars = vec![ALPHABET[0]; len];
    for slot in chars.iter_mut().rev() {
        *slot = ALPHABET[idx % n];
        idx /= n;
    }
    chars.into_iter().map(|b| b as char).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum HintMatch {
    Activate(usize),
    Filtering,
    NoMatch,
}

pub fn match_hint(labels: &[String], typed: &str) -> HintMatch {
    if let Some(i) = labels.iter().position(|l| l == typed) {
        return HintMatch::Activate(i);
    }
    if labels.iter().any(|l| l.starts_with(typed)) {
        return HintMatch::Filtering;
    }
    HintMatch::NoMatch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_char_labels_for_small_counts() {
        let labels = generate_labels(3);
        assert_eq!(labels, vec!["s", "a", "d"]);
    }

    #[test]
    fn two_char_labels_when_exhausted() {
        let labels = generate_labels(20);
        assert_eq!(labels.len(), 20);
        assert!(labels.iter().all(|l| l.len() == 2));
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 20);
    }

    #[test]
    fn exact_match_activates() {
        let labels = generate_labels(3);
        assert_eq!(match_hint(&labels, "a"), HintMatch::Activate(1));
    }

    #[test]
    fn prefix_filters() {
        let labels = generate_labels(20);
        assert_eq!(match_hint(&labels, "s"), HintMatch::Filtering);
    }

    #[test]
    fn unknown_is_no_match() {
        let labels = generate_labels(3);
        assert_eq!(match_hint(&labels, "z"), HintMatch::NoMatch);
    }

    #[test]
    fn handles_counts_above_alphabet_squared() {
        let labels = generate_labels(300);
        assert_eq!(labels.len(), 300);
        assert!(labels.iter().all(|l| l.chars().count() == 3));
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 300);
    }
}

#[cfg(target_arch = "wasm32")]
mod dom {
    use super::{HintMatch, generate_labels, match_hint};
    use crate::overlay;
    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, HtmlElement};

    const SELECTOR: &str = "a[href], button, input:not([type=hidden]), \
        textarea, select, [role=button], [onclick], [tabindex]";

    pub struct Hints {
        labels: Vec<String>,
        targets: Vec<Element>,
        typed: String,
    }

    impl Hints {
        pub fn show(doc: &Document) -> Option<Hints> {
            let nodes = doc.query_selector_all(SELECTOR).ok()?;
            let mut targets = Vec::new();
            for i in 0..nodes.length() {
                let node = nodes.get(i).unwrap();
                let el: Element = node.dyn_into().ok()?;
                if is_visible(&el) {
                    targets.push(el);
                }
            }
            if targets.is_empty() {
                return None;
            }
            let labels = generate_labels(targets.len());
            let sr = overlay::shadow(doc);
            for (label, el) in labels.iter().zip(targets.iter()) {
                let rect = el.get_bounding_client_rect();
                let tag = doc.create_element("div").unwrap();
                tag.set_class_name("vmux-hint");
                tag.set_text_content(Some(label));
                let h: HtmlElement = tag.dyn_into().unwrap();
                let st = h.style();
                let _ = st.set_property("left", &format!("{}px", rect.left().max(0.0)));
                let _ = st.set_property("top", &format!("{}px", rect.top().max(0.0)));
                sr.append_child(&h).unwrap();
            }
            Some(Hints {
                labels,
                targets,
                typed: String::new(),
            })
        }

        pub fn feed(&mut self, doc: &Document, ch: &str) -> bool {
            self.typed.push_str(&ch.to_lowercase());
            match match_hint(&self.labels, &self.typed) {
                HintMatch::Activate(i) => {
                    activate(&self.targets[i]);
                    overlay::clear(doc);
                    false
                }
                HintMatch::Filtering => {
                    redraw(doc, &self.labels, &self.typed);
                    true
                }
                HintMatch::NoMatch => {
                    overlay::clear(doc);
                    false
                }
            }
        }

        pub fn cancel(&self, doc: &Document) {
            overlay::clear(doc);
        }
    }

    fn activate(el: &Element) {
        if let Some(h) = el.dyn_ref::<HtmlElement>() {
            let tag = el.tag_name().to_lowercase();
            if tag == "input" || tag == "textarea" || tag == "select" {
                let _ = h.focus();
            } else {
                h.click();
            }
        }
    }

    fn redraw(doc: &Document, labels: &[String], typed: &str) {
        let sr = overlay::shadow(doc);
        let tags = sr.query_selector_all(".vmux-hint").unwrap();
        for (i, label) in labels.iter().enumerate() {
            let Some(node) = tags.get(i as u32) else {
                continue;
            };
            let Ok(h) = node.dyn_into::<HtmlElement>() else {
                continue;
            };
            if let Some(rest) = label.strip_prefix(typed) {
                let _ = h.style().set_property("display", "block");
                h.set_inner_html(&format!("<span class=\"typed\">{typed}</span>{rest}"));
            } else {
                let _ = h.style().set_property("display", "none");
            }
        }
    }

    fn is_visible(el: &Element) -> bool {
        let rect = el.get_bounding_client_rect();
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return false;
        }
        let win = web_sys::window().unwrap();
        let vw = win
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let vh = win
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        rect.bottom() > 0.0 && rect.top() < vh && rect.right() > 0.0 && rect.left() < vw
    }
}

#[cfg(target_arch = "wasm32")]
pub use dom::Hints;
