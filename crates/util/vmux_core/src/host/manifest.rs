#[derive(Clone, Copy)]
pub struct FeatureManifestSource(&'static str);

impl FeatureManifestSource {
    pub const fn new(source: &'static str) -> Self {
        Self(source)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}
