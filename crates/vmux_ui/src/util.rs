//! Merge default Tailwind classes with optional overrides.

pub fn merge_class(base: &str, extra: Option<&str>) -> String {
    match extra {
        None => base.to_string(),
        Some(e) => {
            let e = e.trim();
            if e.is_empty() {
                base.to_string()
            } else if base.is_empty() {
                e.to_string()
            } else {
                format!("{base} {e}")
            }
        }
    }
}
