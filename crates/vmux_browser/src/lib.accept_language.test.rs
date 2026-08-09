use super::browser_accept_language_list;

#[test]
fn selected_locale_leads_browser_accept_language() {
    assert_eq!(
        browser_accept_language_list("ja"),
        "ja,en-US;q=0.9,en;q=0.8"
    );
    assert_eq!(
        browser_accept_language_list("pt-BR"),
        "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7"
    );
    assert_eq!(browser_accept_language_list("en-US"), "en-US,en;q=0.9");
}
