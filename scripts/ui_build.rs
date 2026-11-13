use std::fs;
use std::path::PathBuf;

#[path = "../crates/build_platform_cfg.rs"]
mod build_platform_cfg;

mod ui_stylesheet;

fn main() {
    println!("cargo:rerun-if-changed=../../../scripts/ui_build.rs");
    println!("cargo:rerun-if-changed=../../../scripts/ui_stylesheet.rs");
    build_platform_cfg::emit();
    generate_i18n_catalogs();
    ui_stylesheet::Bundle::build();
}

fn generate_i18n_catalogs() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let locales_dir = manifest_dir.join("locales");
    println!("cargo:rerun-if-changed={}", locales_dir.display());
    let mut locales = fs::read_dir(&locales_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
        .filter_map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    locales.sort();
    assert!(locales.iter().any(|locale| locale == "en-US"));

    let catalogs = locales
        .iter()
        .map(|locale| {
            format!(
                "    (\"{locale}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/locales/{locale}.ftl\"))),\n"
            )
        })
        .collect::<String>();
    let available = locales
        .iter()
        .map(|locale| format!("    \"{locale}\",\n"))
        .collect::<String>();
    let generated = format!(
        "pub const EMBEDDED_CATALOGS: &[(&str, &str)] = &[\n{catalogs}];\n\npub const AVAILABLE_LOCALES: &[&str] = &[\n{available}];\n"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("i18n_catalogs.rs");
    if fs::read_to_string(&output).ok().as_deref() != Some(generated.as_str()) {
        fs::write(output, generated).unwrap();
    }
}
