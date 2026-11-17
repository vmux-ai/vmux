use std::marker::PhantomData;

use crate::manifest::{FeatureId, FeatureManifest, FeatureManifestSource, Page};

pub struct FeaturePageBinding<M> {
    selector: Option<&'static str>,
    owner: PhantomData<fn() -> M>,
}

impl<M: FeatureManifestSource> FeaturePageBinding<M> {
    pub const fn new(selector: Option<&'static str>) -> Self {
        Self {
            selector,
            owner: PhantomData,
        }
    }

    pub fn declaration(&self) -> Result<&'static Page, String> {
        let manifest = FeatureManifest::of::<M>();
        let Some(selector) = self.selector else {
            return manifest
                .pages
                .first()
                .ok_or_else(|| format!("feature {} has no pages", std::any::type_name::<M>()));
        };
        let mut selected = None;
        for page in &manifest.pages {
            if page.name != selector {
                continue;
            }
            if selected.is_some() {
                return Err(format!(
                    "feature {} contains duplicate page {selector}",
                    std::any::type_name::<M>()
                ));
            }
            selected = Some(page);
        }
        selected.ok_or_else(|| {
            format!(
                "feature {} has no page named {selector}",
                std::any::type_name::<M>()
            )
        })
    }

    pub fn validate(&self, states: &[&str]) -> Result<(), String> {
        let page = self.declaration()?;
        for state in states {
            if !page
                .permissions
                .iter()
                .any(|permission| permission == state)
            {
                return Err(format!(
                    "page {} does not permit subscribed UI state {state}",
                    page.url
                ));
            }
        }
        Ok(())
    }
}

pub struct PageBindingRegistration {
    owner: fn() -> FeatureId,
    validate: fn(Option<&'static str>, &[&str]) -> Result<(), String>,
    selector: Option<&'static str>,
    states: &'static [&'static str],
}

impl PageBindingRegistration {
    pub const fn of<M: FeatureManifestSource>(
        selector: Option<&'static str>,
        states: &'static [&'static str],
    ) -> Self {
        Self {
            owner: FeatureId::of::<M>,
            validate: Self::validate::<M>,
            selector,
            states,
        }
    }

    fn validate<M: FeatureManifestSource>(
        selector: Option<&'static str>,
        states: &[&str],
    ) -> Result<(), String> {
        FeaturePageBinding::<M>::new(selector).validate(states)
    }

    pub fn validate_owner(&self, owner: FeatureId) -> Result<(), String> {
        if (self.owner)() != owner {
            return Ok(());
        }
        (self.validate)(self.selector, self.states)
    }
}

inventory::collect!(PageBindingRegistration);

#[derive(Clone, Copy)]
pub struct CommandBindingRegistration {
    owner: fn() -> FeatureId,
    declaration: fn() -> &'static FeatureManifest,
    selector: Option<&'static str>,
}

impl CommandBindingRegistration {
    pub fn owner(&self) -> FeatureId {
        (self.owner)()
    }

    pub const fn of<M: FeatureManifestSource>(selector: Option<&'static str>) -> Self {
        Self {
            owner: FeatureId::of::<M>,
            declaration: FeatureManifest::of::<M>,
            selector,
        }
    }

    pub fn declaration(&self) -> &'static FeatureManifest {
        (self.declaration)()
    }

    pub fn validate(&self) -> Result<(), String> {
        let Some(selector) = self.selector else {
            return Ok(());
        };
        let mut matches = 0;
        for command in &self.declaration().commands {
            if command.id == selector {
                matches += 1;
            }
        }
        if matches != 1 {
            return Err(format!(
                "feature must declare command {selector} exactly once"
            ));
        }
        Ok(())
    }

    pub fn validate_owner(&self, owner: FeatureId) -> Result<(), String> {
        if (self.owner)() != owner {
            return Ok(());
        }
        self.validate()
    }
}

inventory::collect!(CommandBindingRegistration);

#[cfg(test)]
mod tests {
    use super::*;

    struct Feature;
    struct Ambiguous;

    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = r#"(pages: [
            (name: "main", url: "vmux://example/", title: "Example", permissions: ["Allowed"]),
            (name: "detail", url: "vmux://example/detail", title: "Detail", permissions: ["Detail"]),
        ])"#;
    }

    impl FeatureManifestSource for Ambiguous {
        const SOURCE: &'static str = r#"(pages: [
            (name: "main", url: "vmux://one/", title: "One"),
            (name: "main", url: "vmux://two/", title: "Two"),
        ])"#;
    }

    #[test]
    fn selectors_share_cached_declarations_and_refuse_missing_ambiguous_or_unpermitted_bindings() {
        let binding = FeaturePageBinding::<Feature>::new(Some("detail"));
        binding.validate(&["Detail"]).unwrap();
        assert!(std::ptr::eq(
            binding.declaration().unwrap(),
            &FeatureManifest::of::<Feature>().pages[1]
        ));
        assert!(binding.validate(&["Allowed"]).is_err());
        assert!(
            FeaturePageBinding::<Feature>::new(Some("missing"))
                .declaration()
                .is_err()
        );
        assert!(
            FeaturePageBinding::<Ambiguous>::new(Some("main"))
                .declaration()
                .is_err()
        );
    }
}
