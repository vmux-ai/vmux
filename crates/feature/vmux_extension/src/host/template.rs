pub(crate) struct Template(String);

impl Template {
    pub(crate) fn new(source: impl Into<String>) -> Self {
        Self(source.into())
    }

    pub(crate) fn render(mut self, replacements: &[(&str, String)]) -> Result<String, String> {
        for (placeholder, value) in replacements {
            let count = self.0.matches(placeholder).count();
            if count != 1 {
                return Err(format!(
                    "template placeholder {placeholder} occurred {count} times"
                ));
            }
            self.0 = self.0.replace(placeholder, value);
        }
        if self.0.contains("__VMUX_") {
            return Err("template contains unresolved vmux placeholder".into());
        }
        Ok(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_placeholder_exactly_once() {
        assert_eq!(
            Template::new("x=__VMUX_X__")
                .render(&[("__VMUX_X__", "1".into())])
                .unwrap(),
            "x=1"
        );
        assert!(
            Template::new("x")
                .render(&[("__VMUX_X__", "1".into())])
                .is_err()
        );
        assert!(
            Template::new("__VMUX_X____VMUX_X__")
                .render(&[("__VMUX_X__", "1".into())])
                .is_err()
        );
        assert!(Template::new("__VMUX_Y__").render(&[]).is_err());
    }
}
