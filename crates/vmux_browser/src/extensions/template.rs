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
#[path = "template.test.rs"]
mod tests;
