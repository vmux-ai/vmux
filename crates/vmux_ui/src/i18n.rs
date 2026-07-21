use crate::i18n_catalogs::{AVAILABLE_LOCALES, EMBEDDED_CATALOGS};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use std::cell::RefCell;
use std::collections::HashMap;
use std::str::FromStr;
use unic_langid::{CharacterDirection, LanguageIdentifier};

pub const DEFAULT_LOCALE: &str = "en-US";

thread_local! {
    static CURRENT_LOCALE: RefCell<String> = RefCell::new(preferred_locale());
    static BUNDLES: RefCell<HashMap<String, FluentBundle<FluentResource>>> =
        RefCell::new(HashMap::new());
    static EXTERNAL_CATALOGS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TranslationValue<'a> {
    String(&'a str),
    Number(i64),
}

pub fn preferred_locale() -> String {
    normalize_locale(&platform_locale().unwrap_or_else(|| DEFAULT_LOCALE.to_string()))
}

pub fn requested_locale(override_locale: Option<&str>) -> String {
    override_locale
        .filter(|locale| {
            let locale = locale.trim();
            !locale.is_empty() && !matches!(locale, "system" | "auto" | "device")
        })
        .map(normalize_locale)
        .unwrap_or_else(preferred_locale)
}

pub fn set_current_locale(locale: &str) {
    CURRENT_LOCALE.with_borrow_mut(|current| *current = normalize_locale(locale));
}

pub fn current_locale() -> String {
    CURRENT_LOCALE.with_borrow(Clone::clone)
}

pub fn text_direction(locale: &str) -> CharacterDirection {
    parse_locale(locale)
        .map(|locale| locale.character_direction())
        .unwrap_or(CharacterDirection::LTR)
}

pub fn translate(id: &str) -> String {
    let locale = current_locale();
    translate_for(&locale, id)
}

pub fn translate_with(id: &str, args: &[(&str, TranslationValue<'_>)]) -> String {
    let locale = current_locale();
    translate_for_with(&locale, id, args)
}

pub fn translate_for(locale: &str, id: &str) -> String {
    translate_for_with(locale, id, &[])
}

pub fn translate_for_with(locale: &str, id: &str, args: &[(&str, TranslationValue<'_>)]) -> String {
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        match value {
            TranslationValue::String(value) => fluent_args.set(*name, *value),
            TranslationValue::Number(value) => fluent_args.set(*name, *value),
        }
    }

    let selected = catalog_for(locale);
    format_message(&selected, id, &fluent_args)
        .or_else(|| {
            (selected != DEFAULT_LOCALE)
                .then(|| format_message(DEFAULT_LOCALE, id, &fluent_args))
                .flatten()
        })
        .unwrap_or_else(|| id.to_string())
}

pub fn available_locales() -> &'static [&'static str] {
    AVAILABLE_LOCALES
}

pub fn register_catalog(locale: &str, source: &str) -> Result<(), String> {
    let locale = normalize_locale(locale);
    let bundle = build_bundle_from_source(&locale, source)?;
    EXTERNAL_CATALOGS.with_borrow_mut(|catalogs| {
        catalogs.insert(locale.clone(), source.to_string());
    });
    BUNDLES.with_borrow_mut(|bundles| {
        bundles.insert(locale, bundle);
    });
    Ok(())
}

fn catalog_for(locale: &str) -> String {
    let Some(locale) = parse_locale(locale) else {
        return DEFAULT_LOCALE.to_string();
    };
    let exact = locale.to_string();
    if has_external_catalog(&exact) {
        return exact;
    }
    let language = locale.language.as_str();
    if has_external_catalog(language) {
        return language.to_string();
    }
    if has_embedded_catalog(&exact) {
        return exact;
    }
    if has_embedded_catalog(language) {
        return language.to_string();
    }
    DEFAULT_LOCALE.to_string()
}

fn embedded_catalog_source(locale: &str) -> Option<&'static str> {
    EMBEDDED_CATALOGS
        .iter()
        .find_map(|(tag, source)| (*tag == locale).then_some(*source))
}

fn has_embedded_catalog(locale: &str) -> bool {
    embedded_catalog_source(locale).is_some()
}

fn has_external_catalog(locale: &str) -> bool {
    EXTERNAL_CATALOGS.with_borrow(|catalogs| catalogs.contains_key(locale))
}

fn format_message(locale: &str, id: &str, args: &FluentArgs<'_>) -> Option<String> {
    BUNDLES.with_borrow_mut(|bundles| {
        let bundle = bundles
            .entry(locale.to_string())
            .or_insert_with(|| build_bundle(locale));
        let message = bundle.get_message(id)?;
        let pattern = message.value()?;
        let mut errors = Vec::new();
        let value = bundle.format_pattern(pattern, Some(args), &mut errors);
        errors.is_empty().then(|| value.into_owned())
    })
}

fn build_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let source = EXTERNAL_CATALOGS
        .with_borrow(|catalogs| catalogs.get(locale).cloned())
        .or_else(|| embedded_catalog_source(locale).map(str::to_string))
        .unwrap_or_else(|| embedded_catalog_source(DEFAULT_LOCALE).unwrap().to_string());
    build_bundle_from_source(locale, &source)
        .unwrap_or_else(|error| panic!("invalid {locale} Fluent catalog: {error}"))
}

fn build_bundle_from_source(
    locale: &str,
    source: &str,
) -> Result<FluentBundle<FluentResource>, String> {
    let language = LanguageIdentifier::from_str(locale).expect("embedded locale must be valid");
    let resource =
        FluentResource::try_new(source.to_string()).map_err(|(_, errors)| format!("{errors:?}"))?;
    let mut bundle = FluentBundle::new(vec![language]);
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("{errors:?}"))?;
    Ok(bundle)
}

fn normalize_locale(locale: &str) -> String {
    let locale = locale
        .split(['.', '@'])
        .next()
        .unwrap_or(locale)
        .replace('_', "-");
    parse_locale(&locale)
        .map(|locale| locale.to_string())
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string())
}

fn parse_locale(locale: &str) -> Option<LanguageIdentifier> {
    LanguageIdentifier::from_str(locale).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn platform_locale() -> Option<String> {
    sys_locale::get_locale()
}

#[cfg(target_arch = "wasm32")]
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
        let english = embedded_catalog_source(DEFAULT_LOCALE).unwrap();
        for &(locale, source) in EMBEDDED_CATALOGS {
            build_bundle(locale);
            assert_eq!(
                message_ids(english),
                message_ids(source),
                "message IDs differ for {locale}"
            );
        }
    }

    #[test]
    fn resolves_region_variants_to_language_catalog() {
        assert_eq!(translate_for("ja-JP", "common-open"), "開く");
        assert_eq!(translate_for("en-GB", "common-open"), "Open");
    }

    #[test]
    fn falls_back_to_english_for_unknown_locale_and_missing_message() {
        assert_eq!(translate_for("zz-ZZ", "common-open"), "Open");
        register_catalog("de", "common-open = Öffnen").unwrap();
        assert_eq!(translate_for("de", "common-close"), "Close");
    }

    #[test]
    fn formats_variables_and_plurals() {
        assert_eq!(
            visible(translate_for_with(
                "en-US",
                "common-items",
                &[("count", TranslationValue::Number(2))],
            )),
            "2 items"
        );
        assert_eq!(
            visible(translate_for_with(
                "ja",
                "common-items",
                &[("count", TranslationValue::Number(2))],
            )),
            "2 件"
        );
    }

    #[test]
    fn reports_script_direction() {
        assert_eq!(text_direction("en-US"), CharacterDirection::LTR);
        assert_eq!(text_direction("ar"), CharacterDirection::RTL);
    }

    #[test]
    fn registered_catalog_overrides_english_and_keeps_fallback() {
        register_catalog("fr", "common-open = Ouvrir").unwrap();
        assert_eq!(translate_for("fr-FR", "common-open"), "Ouvrir");
        assert_eq!(translate_for("fr-FR", "common-close"), "Close");
    }
}
