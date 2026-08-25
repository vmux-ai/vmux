use bevy::prelude::Resource;
use vmux_ui::i18n::Locale;

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ResolvedLocale(pub Locale);

impl Default for ResolvedLocale {
    fn default() -> Self {
        Self(Locale::preferred())
    }
}
