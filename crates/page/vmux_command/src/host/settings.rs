//! Settings the command vocabulary needs to answer with.

use bevy::prelude::Resource;
use vmux_ui::i18n::Locale;

/// The locale the app resolved to, once the settings file and the system preference have had their
/// say.
///
/// Written by whoever owns settings and read by anything that names a command to a person. It sits
/// here rather than with the settings page because every reader is downstream of this crate and
/// none of them are downstream of that one.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ResolvedLocale(pub Locale);

impl Default for ResolvedLocale {
    fn default() -> Self {
        Self(Locale::preferred())
    }
}
