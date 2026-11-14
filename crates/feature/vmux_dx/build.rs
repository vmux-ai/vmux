#[path = "src/source.rs"]
mod source;

fn main() {
    println!(
        "cargo:rustc-env=VMUX_DX_HOST_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    println!("cargo:rerun-if-env-changed=VMUX_UPDATE_PUBLIC_KEY");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let key = source::Source::key(root).expect("DX compiler inputs");
    println!("cargo:rustc-env=VMUX_DX_SOURCE_KEY={key}");
    for path in source::Source::paths(root).expect("DX compiler paths") {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    for (group, name) in [
        ("feature", "vmux_dx"),
        ("app", "vmux_cli"),
        ("util", "vmux_ecs"),
        ("util", "vmux_api"),
        ("util", "vmux_task"),
        ("util", "vmux_macro"),
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            root.join("crates")
                .join(group)
                .join(name)
                .join("src")
                .display()
        );
    }
}
