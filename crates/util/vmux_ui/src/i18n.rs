use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use unic_langid::{CharacterDirection, LanguageIdentifier};

pub const DEFAULT_LOCALE: &str = "en-US";

pub fn translate(id: &str) -> String {
    Locale::current().translate(id)
}

pub fn translate_with(id: &str, args: &[(&str, TranslationValue<'_>)]) -> String {
    Locale::current().translate_with(id, args)
}

pub fn translate_humanized(id: &str) -> String {
    Locale::current().translate_humanized(id)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Locale(String);

impl Locale {
    const EMBEDDED_CATALOGS: &'static [(&'static str, &'static str)] = &[
        ("ar", include_str!("../locales/ar.ftl")),
        ("de", include_str!("../locales/de.ftl")),
        ("en-US", include_str!("../locales/en-US.ftl")),
        ("es", include_str!("../locales/es.ftl")),
        ("fr", include_str!("../locales/fr.ftl")),
        ("it", include_str!("../locales/it.ftl")),
        ("ja", include_str!("../locales/ja.ftl")),
        ("ko", include_str!("../locales/ko.ftl")),
        ("nl", include_str!("../locales/nl.ftl")),
        ("pl", include_str!("../locales/pl.ftl")),
        ("pt-BR", include_str!("../locales/pt-BR.ftl")),
        ("ru", include_str!("../locales/ru.ftl")),
        ("uk", include_str!("../locales/uk.ftl")),
        ("zh-Hans", include_str!("../locales/zh-Hans.ftl")),
        ("zh-Hant", include_str!("../locales/zh-Hant.ftl")),
    ];

    fn with_current_slot<R>(f: impl FnOnce(&mut Locale) -> R) -> R {
        thread_local! {
            static CURRENT_LOCALE: RefCell<Locale> = RefCell::new(Locale::preferred());
        }
        CURRENT_LOCALE.with_borrow_mut(f)
    }

    fn with_bundles<R>(
        f: impl FnOnce(&mut HashMap<String, FluentBundle<FluentResource>>) -> R,
    ) -> R {
        thread_local! {
            static BUNDLES: RefCell<HashMap<String, FluentBundle<FluentResource>>> =
                RefCell::new(HashMap::new());
        }
        BUNDLES.with_borrow_mut(f)
    }

    fn with_external_catalogs<R>(f: impl FnOnce(&mut HashMap<String, String>) -> R) -> R {
        thread_local! {
            static EXTERNAL_CATALOGS: RefCell<HashMap<String, String>> =
                RefCell::new(HashMap::new());
        }
        EXTERNAL_CATALOGS.with_borrow_mut(f)
    }

    pub fn preferred() -> Self {
        let Some(tag) = sys_locale::get_locale() else {
            return Self::from(DEFAULT_LOCALE);
        };
        Self::from(tag.as_str())
    }

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

    pub fn current() -> Self {
        Self::with_current_slot(|current| current.clone())
    }

    pub fn available() -> impl Iterator<Item = Self> {
        Self::EMBEDDED_CATALOGS
            .iter()
            .map(|(tag, _)| Self::from(*tag))
    }

    pub fn make_current(&self) {
        Self::with_current_slot(|current| *current = self.clone());
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn direction(&self) -> CharacterDirection {
        let Some(identifier) = self.identifier() else {
            return CharacterDirection::LTR;
        };
        identifier.character_direction()
    }

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

    pub fn translate_humanized(&self, id: &str) -> String {
        let translated = self.translate(id);
        if translated != id {
            return translated;
        }
        Self::humanize(id)
    }

    fn humanize(id: &str) -> String {
        let mut value = id
            .split(['_', '-'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(first) = value.chars().next() {
            let upper = first.to_uppercase().collect::<String>();
            value.replace_range(..first.len_utf8(), &upper);
        }
        value
    }

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

    pub fn register_catalog(&self, source: &str) -> Result<(), String> {
        let bundle = self.bundle_from_source(source)?;
        Self::with_external_catalogs(|catalogs| {
            catalogs.insert(self.0.clone(), source.to_string());
        });
        Self::with_bundles(|bundles| {
            bundles.insert(self.0.clone(), bundle);
        });
        Ok(())
    }

    fn default_locale() -> Self {
        Self(DEFAULT_LOCALE.to_string())
    }

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
        for &(tag, source) in Self::EMBEDDED_CATALOGS {
            if tag == self.0 {
                return Some(source);
            }
        }
        None
    }

    fn has_external_catalog(&self) -> bool {
        Self::with_external_catalogs(|catalogs| catalogs.contains_key(&self.0))
    }

    fn format(&self, id: &str, args: &FluentArgs<'_>) -> Option<String> {
        Self::with_bundles(|bundles| {
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
        let source = Self::with_external_catalogs(|catalogs| catalogs.get(&self.0).cloned())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn bundled_catalogs_parse_and_have_identical_message_ids() {
        let message_ids = |source: &'static str| -> BTreeSet<&'static str> {
            source
                .lines()
                .filter(|line| {
                    !line.chars().next().is_some_and(char::is_whitespace) && !line.starts_with('#')
                })
                .filter_map(|line| line.split_once('=').map(|(id, _)| id.trim()))
                .collect()
        };
        let english = Locale::default_locale().embedded_source().unwrap();
        let disk_locales: BTreeSet<String> =
            std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales"))
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "ftl"))
                .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
                .collect();
        let embedded_locales: BTreeSet<String> = Locale::EMBEDDED_CATALOGS
            .iter()
            .map(|(locale, _)| (*locale).to_string())
            .collect();
        assert_eq!(disk_locales, embedded_locales);
        for &(locale, source) in Locale::EMBEDDED_CATALOGS {
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
    fn missing_dynamic_labels_are_humanized() {
        let english = Locale::from("en-US");

        assert_eq!(
            english.translate_humanized("session-stage-in-progress"),
            "In progress"
        );
        assert_eq!(english.translate_humanized("in_progress"), "In progress");
        assert_eq!(english.translate_humanized("needs-review"), "Needs review");
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
        let visible = |value: String| value.replace(['\u{2068}', '\u{2069}'], "");
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
