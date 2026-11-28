#[path = "src/source.rs"]
#[cfg(feature = "development")]
mod source;
#[path = "src/source_io.rs"]
#[cfg(feature = "development")]
mod source_io;

fn main() {
    println!(
        "cargo:rustc-env=VMUX_DX_HOST_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    println!("cargo:rerun-if-env-changed=VMUX_UPDATE_PUBLIC_KEY");
    #[cfg(feature = "development")]
    development();
}

#[cfg(feature = "development")]
fn development() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"),
    );
    let root = manifest_dir.ancestors().nth(3).unwrap();
    let key = source::Source::key(root, || false).expect("DX compiler inputs");
    println!("cargo:rustc-env=VMUX_DX_SOURCE_KEY={key}");
    for path in source::Source::paths(root, &|| false).expect("DX compiler paths") {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    for (group, name) in [
        ("feature", "vmux_dx"),
        ("app", "vmux_cli"),
        ("util", "vmux_ecs"),
        ("util", "vmux_api"),
        ("util", "vmux_channel"),
        ("util", "vmux_job"),
        ("util", "vmux_store"),
        ("util", "vmux_watch"),
        ("util", "vmux_task"),
        ("util", "vmux_time"),
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
