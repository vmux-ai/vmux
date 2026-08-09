use super::*;

fn os_menu_tokens() -> String {
    let input: DeriveInput = syn::parse_quote! {
        enum SampleMenu {
            #[menu(label = "Sample")]
            Sample(SampleCommand),
        }
    };

    impl_os_menu(input)
        .expect("os menu should generate")
        .to_string()
}

#[test]
fn os_menu_about_metadata_displays_version_under_title() {
    let tokens = os_menu_tokens();

    assert!(tokens.contains(":: muda :: AboutMetadata"));
    assert!(tokens.contains("env ! (\"CARGO_PKG_VERSION\")"));
    assert!(tokens.contains("env ! (\"VMUX_GIT_HASH\")"));
    assert!(tokens.contains("\"local\" => format ! (\"v{} ({})\""));
    assert!(tokens.contains("\"dev\" => format ! (\"v{} ({})\""));
    assert!(tokens.contains("version : Some (about_version)"));
    assert!(tokens.contains("copyright : Some (:: std :: string :: String :: new ())"));
    assert!(!tokens.contains("short_version"));
    assert!(
        tokens.contains(":: muda :: PredefinedMenuItem :: about (None , Some (about_metadata))")
    );
}

#[test]
fn os_menu_release_about_metadata_omits_commit_hash() {
    let tokens = os_menu_tokens();

    assert!(tokens.contains("\"release\" => format ! (\"v{}\" , env ! (\"CARGO_PKG_VERSION\"))"));
}

#[test]
fn os_menu_about_metadata_keeps_native_title() {
    let tokens = os_menu_tokens();

    assert!(!tokens.contains("name : Some"));
}
