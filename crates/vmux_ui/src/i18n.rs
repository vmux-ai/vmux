//! Fluent-backed translation.
//!
//! [`Locale`] owns every lookup that depends on which language is in play; [`translate`] and
//! [`translate_with`] are the ambient shorthand a page reaches for inside `rsx!`.

use crate::i18n_catalogs::{AVAILABLE_LOCALES, EMBEDDED_CATALOGS};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use unic_langid::{CharacterDirection, LanguageIdentifier};

pub const DEFAULT_LOCALE: &str = "en-US";

thread_local! {
    static CURRENT_LOCALE: RefCell<Locale> = RefCell::new(Locale::preferred());
    static BUNDLES: RefCell<HashMap<String, FluentBundle<FluentResource>>> =
        RefCell::new(HashMap::new());
    static EXTERNAL_CATALOGS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Look `id` up in the locale this thread is currently rendering in.
///
/// This and [`translate_with`] are the module's deliberate exceptions to behaviour hanging off a
/// type: they derive nothing from a value, they read ambient thread-local state, and they are the
/// entry point every `rsx!` block in the workspace calls. Spelling each of those sites
/// `Locale::current().translate(id)` would name the same thing at more length.
pub fn translate(id: &str) -> String {
    Locale::current().translate(id)
}

/// [`translate`] with Fluent arguments, and the same exception for the same reason.
pub fn translate_with(id: &str, args: &[(&str, TranslationValue<'_>)]) -> String {
    Locale::current().translate_with(id, args)
}

/// A canonical BCP-47 language tag together with the catalog it resolves messages against.
///
/// Construction always normalizes, so the tag is parseable and in canonical case for the whole
/// life of the value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Locale(String);

impl Locale {
    /// The locale the host platform reports, falling back to [`DEFAULT_LOCALE`].
    pub fn preferred() -> Self {
        let Some(tag) = platform_locale() else {
            return Self::from(DEFAULT_LOCALE);
        };
        Self::from(tag.as_str())
    }

    /// Resolve an explicit override from settings, where an empty tag and the sentinels `system`,
    /// `auto` and `device` all mean "follow the platform".
    pub fn requested(override_locale: Option<&str>) -> Self {
        let Some(tag) = override_locale else {
            return Self::preferred();
        };
        let trimmed = tag.trim();
        if trimmed.is_empty() || matches!(trimmed, "system" | "auto" | "device") {
            return Self::preferred();
        }
        Self::from(tag)
    }

    /// The locale [`translate`] resolves against on this thread.
    pub fn current() -> Self {
        CURRENT_LOCALE.with_borrow(Clone::clone)
    }

    /// Every locale whose catalog is compiled into the binary.
    pub fn available() -> impl Iterator<Item = Self> {
        AVAILABLE_LOCALES.iter().map(|tag| Self::from(*tag))
    }

    /// Make this the locale [`translate`] resolves against on this thread.
    pub fn make_current(&self) {
        CURRENT_LOCALE.with_borrow_mut(|current| *current = self.clone());
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    /// The direction this locale's script runs in, for the `dir` attribute and RTL layout.
    pub fn direction(&self) -> CharacterDirection {
        let Some(identifier) = self.identifier() else {
            return CharacterDirection::LTR;
        };
        identifier.character_direction()
    }

    /// The autonym — the language's name written in itself, for a language picker. Falls back to
    /// the tag when no catalog declares one.
    pub fn name(&self) -> &str {
        self.embedded_source()
            .and_then(|source| {
                source
                    .lines()
                    .find_map(|line| line.strip_prefix("locale-name = "))
            })
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.0)
    }

    pub fn translate(&self, id: &str) -> String {
        self.translate_with(id, &[])
    }

    /// Format `id` with Fluent arguments, falling back to [`DEFAULT_LOCALE`] and then to `id`
    /// itself when the message is missing.
    pub fn translate_with(&self, id: &str, args: &[(&str, TranslationValue<'_>)]) -> String {
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            match value {
                TranslationValue::String(value) => fluent_args.set(*name, *value),
                TranslationValue::Number(value) => fluent_args.set(*name, *value),
            }
        }

        let catalog = self.catalog();
        if let Some(message) = catalog.format(id, &fluent_args) {
            return message;
        }
        let english = Self::default_locale();
        if catalog != english
            && let Some(message) = english.format(id, &fluent_args)
        {
            return message;
        }
        id.to_string()
    }

    /// Install a Fluent catalog for this locale at runtime, overriding any embedded one.
    pub fn register_catalog(&self, source: &str) -> Result<(), String> {
        let bundle = self.bundle_from_source(source)?;
        EXTERNAL_CATALOGS.with_borrow_mut(|catalogs| {
            catalogs.insert(self.0.clone(), source.to_string());
        });
        BUNDLES.with_borrow_mut(|bundles| {
            bundles.insert(self.0.clone(), bundle);
        });
        Ok(())
    }

    fn default_locale() -> Self {
        Self(DEFAULT_LOCALE.to_string())
    }

    /// The locale whose catalog actually backs this one: an exact match wins over the bare
    /// language, a runtime catalog over an embedded one, and English over nothing.
    fn catalog(&self) -> Self {
        let Some(identifier) = self.identifier() else {
            return Self::default_locale();
        };
        let exact = Self(identifier.to_string());
        if exact.has_external_catalog() {
            return exact;
        }
        let language = Self(identifier.language.as_str().to_string());
        if language.has_external_catalog() {
            return language;
        }
        if exact.embedded_source().is_some() {
            return exact;
        }
        if language.embedded_source().is_some() {
            return language;
        }
        Self::default_locale()
    }

    fn identifier(&self) -> Option<LanguageIdentifier> {
        LanguageIdentifier::from_str(&self.0).ok()
    }

    fn embedded_source(&self) -> Option<&'static str> {
        for &(tag, source) in EMBEDDED_CATALOGS {
            if tag == self.0 {
                return Some(source);
            }
        }
        None
    }

    fn has_external_catalog(&self) -> bool {
        EXTERNAL_CATALOGS.with_borrow(|catalogs| catalogs.contains_key(&self.0))
    }

    fn format(&self, id: &str, args: &FluentArgs<'_>) -> Option<String> {
        BUNDLES.with_borrow_mut(|bundles| {
            let bundle = bundles
                .entry(self.0.clone())
                .or_insert_with(|| self.bundle());
            let message = bundle.get_message(id)?;
            let pattern = message.value()?;
            let mut errors = Vec::new();
            let value = bundle.format_pattern(pattern, Some(args), &mut errors);
            errors.is_empty().then(|| value.into_owned())
        })
    }

    fn bundle(&self) -> FluentBundle<FluentResource> {
        let source = EXTERNAL_CATALOGS
            .with_borrow(|catalogs| catalogs.get(&self.0).cloned())
            .or_else(|| self.embedded_source().map(str::to_string))
            .unwrap_or_else(|| {
                Self::default_locale()
                    .embedded_source()
                    .unwrap()
                    .to_string()
            });
        self.bundle_from_source(&source)
            .unwrap_or_else(|error| panic!("invalid {self} Fluent catalog: {error}"))
    }

    fn bundle_from_source(&self, source: &str) -> Result<FluentBundle<FluentResource>, String> {
        let identifier = self.identifier().expect("normalized locale must parse");
        let resource = FluentResource::try_new(source.to_string())
            .map_err(|(_, errors)| format!("{errors:?}"))?;
        let mut bundle = FluentBundle::new(vec![identifier]);
        bundle
            .add_resource(resource)
            .map_err(|errors| format!("{errors:?}"))?;
        Ok(bundle)
    }
}

impl From<&str> for Locale {
    /// Accepts what an OS or a config file hands over — `en_US.UTF-8`, `fr@euro`, `JA-jp` — and
    /// canonicalizes it, falling back to [`DEFAULT_LOCALE`] when nothing parses.
    fn from(tag: &str) -> Self {
        let tag = tag
            .split(['.', '@'])
            .next()
            .unwrap_or(tag)
            .replace('_', "-");
        let Ok(identifier) = LanguageIdentifier::from_str(&tag) else {
            return Self::default_locale();
        };
        Self(identifier.to_string())
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TranslationValue<'a> {
    String(&'a str),
    Number(i64),
}

#[cfg(not(web))]
fn platform_locale() -> Option<String> {
    sys_locale::get_locale()
}

#[cfg(web)]
fn platform_locale() -> Option<String> {
    let navigator = web_sys::window()?.navigator();
    navigator
        .languages()
        .get(0)
        .as_string()
        .or_else(|| navigator.language())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn visible(value: String) -> String {
        value.replace(['\u{2068}', '\u{2069}'], "")
    }

    fn message_ids(source: &str) -> BTreeSet<&str> {
        source
            .lines()
            .filter(|line| {
                !line.chars().next().is_some_and(char::is_whitespace) && !line.starts_with('#')
            })
            .filter_map(|line| line.split_once('=').map(|(id, _)| id.trim()))
            .collect()
    }

    #[test]
    fn bundled_catalogs_parse_and_have_identical_message_ids() {
        let english = Locale::default_locale().embedded_source().unwrap();
        for &(locale, source) in EMBEDDED_CATALOGS {
            Locale::from(locale).bundle();
            assert_eq!(
                message_ids(english),
                message_ids(source),
                "message IDs differ for {locale}"
            );
        }
    }

    #[test]
    fn bundled_locales_expose_native_names() {
        for locale in Locale::available() {
            assert_ne!(
                locale.name(),
                locale.as_str(),
                "missing autonym for {locale}"
            );
        }
        assert_eq!(Locale::from("en-US").name(), "English (US)");
        assert_eq!(Locale::from("ja").name(), "日本語");
        assert_eq!(Locale::from("fr").name(), "français");
        assert_eq!(Locale::from("ar").name(), "العربية");
    }

    #[test]
    fn resolves_region_variants_to_language_catalog() {
        assert_eq!(Locale::from("ja-JP").translate("common-open"), "開く");
        assert_eq!(Locale::from("en-GB").translate("common-open"), "Open");
    }

    #[test]
    fn falls_back_to_english_for_unknown_locale_and_missing_message() {
        assert_eq!(Locale::from("zz-ZZ").translate("common-open"), "Open");
        Locale::from("de")
            .register_catalog("common-open = Öffnen")
            .unwrap();
        assert_eq!(Locale::from("de").translate("common-close"), "Close");
    }

    #[test]
    fn formats_variables_and_plurals() {
        assert_eq!(
            visible(
                Locale::from("en-US")
                    .translate_with("common-items", &[("count", TranslationValue::Number(2))],)
            ),
            "2 items"
        );
        assert_eq!(
            visible(
                Locale::from("ja")
                    .translate_with("common-items", &[("count", TranslationValue::Number(2))],)
            ),
            "2 件"
        );
    }

    #[test]
    fn reports_script_direction() {
        assert_eq!(Locale::from("en-US").direction(), CharacterDirection::LTR);
        assert_eq!(Locale::from("ar").direction(), CharacterDirection::RTL);
    }

    #[test]
    fn registered_catalog_overrides_english_and_keeps_fallback() {
        Locale::from("fr")
            .register_catalog("common-open = Ouvrir")
            .unwrap();
        assert_eq!(Locale::from("fr-FR").translate("common-open"), "Ouvrir");
        assert_eq!(Locale::from("fr-FR").translate("common-close"), "Close");
    }
}
