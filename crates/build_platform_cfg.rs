pub fn emit() {
    println!("cargo::rustc-check-cfg=cfg(host)");
    println!("cargo::rustc-check-cfg=cfg(ui)");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let phone = os == "ios";

    if phone || os == "macos" {
        println!("cargo::rustc-cfg=ui");
    }
    if !phone {
        println!("cargo::rustc-cfg=host");
    }
}
