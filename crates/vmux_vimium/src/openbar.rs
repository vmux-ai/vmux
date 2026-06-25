pub fn to_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let looks_like_url = trimmed.contains("://")
        || (trimmed.contains('.') && !trimmed.contains(' ') && !trimmed.starts_with('.'));
    if looks_like_url {
        if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        }
    } else {
        format!("https://duckduckgo.com/?q={}", urlencode(trimmed))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_url_passthrough() {
        assert_eq!(to_url("https://example.com/x"), "https://example.com/x");
    }

    #[test]
    fn bare_domain_gets_https() {
        assert_eq!(to_url("example.com"), "https://example.com");
    }

    #[test]
    fn query_becomes_search() {
        assert_eq!(
            to_url("hello world"),
            "https://duckduckgo.com/?q=hello+world"
        );
    }

    #[test]
    fn single_word_with_no_dot_is_search() {
        assert_eq!(to_url("rustlang"), "https://duckduckgo.com/?q=rustlang");
    }
}

#[cfg(target_arch = "wasm32")]
mod dom {
    use super::to_url;
    use crate::overlay;
    use wasm_bindgen::JsCast;
    use web_sys::{Document, HtmlElement, HtmlInputElement};

    pub struct OpenBar;

    impl OpenBar {
        pub fn open(doc: &Document) -> OpenBar {
            let sr = overlay::shadow(doc);
            let bar = doc.create_element("div").unwrap();
            bar.set_class_name("vmux-bar");
            bar.set_inner_html(
                "<input class=\"vmux-open-input\" placeholder=\"open url or search\u{2026}\"/>",
            );
            sr.append_child(&bar).unwrap();
            if let Some(input) = sr.query_selector(".vmux-open-input").unwrap()
                && let Some(h) = input.dyn_ref::<HtmlElement>()
            {
                let _ = h.focus();
            }
            OpenBar
        }

        pub fn submit(&self, doc: &Document) {
            let val = overlay::shadow(doc)
                .query_selector(".vmux-open-input")
                .unwrap()
                .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
                .map(|i| i.value())
                .unwrap_or_default();
            let url = to_url(&val);
            overlay::clear(doc);
            if !url.is_empty() {
                let _ = web_sys::window().unwrap().location().set_href(&url);
            }
        }

        pub fn close(&self, doc: &Document) {
            overlay::clear(doc);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use dom::OpenBar;
