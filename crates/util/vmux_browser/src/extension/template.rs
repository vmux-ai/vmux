pub(crate) fn render(source: &str, replacements: &[(&str, String)]) -> Result<String, String> {
    let mut rendered = source.to_string();
    for (placeholder, value) in replacements {
        let count = rendered.matches(placeholder).count();
        if count != 1 {
            return Err(format!(
                "template placeholder {placeholder} occurred {count} times"
            ));
        }
        rendered = rendered.replace(placeholder, value);
    }
    if rendered.contains("__VMUX_") {
        return Err("template contains unresolved vmux placeholder".into());
    }
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_placeholder_exactly_once() {
        assert_eq!(
            render("x=__VMUX_X__", &[("__VMUX_X__", "1".into())]).unwrap(),
            "x=1"
        );
        assert!(render("x", &[("__VMUX_X__", "1".into())]).is_err());
        assert!(render("__VMUX_X____VMUX_X__", &[("__VMUX_X__", "1".into())]).is_err());
        assert!(render("__VMUX_Y__", &[]).is_err());
    }
}
