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
fn bundled_locales_expose_native_names() {
    for locale in available_locales() {
        assert_ne!(locale_name(locale), *locale, "missing autonym for {locale}");
    }
    assert_eq!(locale_name("en-US"), "English (US)");
    assert_eq!(locale_name("ja"), "日本語");
    assert_eq!(locale_name("fr"), "français");
    assert_eq!(locale_name("ar"), "العربية");
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
