use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use vmux_docs::model::{ApiIndex, CrateMeta};
use vmux_docs::translate::translate;

const NIGHTLY: &str = "nightly-2026-06-26";

const CRATES: &[&str] = &[
    "vmux_core",
    "vmux_browser",
    "vmux_terminal",
    "vmux_agent",
    "vmux_editor",
    "vmux_layout",
    "vmux_git",
    "vmux_space",
    "vmux_history",
    "vmux_command",
    "vmux_service",
    "vmux_setting",
    "vmux_mcp",
    "vmux_team",
    "vmux_ui",
    "vmux_server",
    "vmux_desktop",
    "vmux_cli",
    "vmux_macro",
];

#[derive(Parser)]
struct Args {
    /// Output directory for the committed model. Defaults to <repo>/docs/api.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Optional subset of crate names; defaults to all.
    #[arg(long)]
    only: Vec<String>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("vmux_docs has a parent dir")
        .to_path_buf()
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = repo_root();
    let out = args.out.unwrap_or_else(|| root.join("docs/api"));
    std::fs::create_dir_all(&out)?;

    let wanted: Vec<&str> = if args.only.is_empty() {
        CRATES.to_vec()
    } else {
        for requested in &args.only {
            if !CRATES.contains(&requested.as_str()) {
                anyhow::bail!("unknown crate in --only: {requested}");
            }
        }
        CRATES
            .iter()
            .copied()
            .filter(|c| args.only.iter().any(|o| o == c))
            .collect()
    };

    let mut metas = Vec::new();
    let mut failed = Vec::new();
    for name in wanted {
        eprintln!("doc: {name}");
        let manifest = root.join(format!("crates/{name}/Cargo.toml"));
        let mut builder = rustdoc_json::Builder::default()
            .toolchain(NIGHTLY)
            .manifest_path(&manifest)
            .document_private_items(false)
            .cap_lints(Some("allow"));
        if let Some(bin) = bin_target(name) {
            builder = builder.package_target(rustdoc_json::PackageTarget::Bin(bin.to_string()));
        }
        let built = builder.build();
        let json_path = match built {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  FAILED {name}: {e}");
                failed.push(name);
                continue;
            }
        };
        let raw = std::fs::read_to_string(&json_path)?;
        let krate: rustdoc_types::Crate = serde_json::from_str(&raw)?;
        let doc = translate(&krate);
        metas.push(CrateMeta {
            name: name.to_string(),
            version: doc.version.clone(),
            blurb_md: first_paragraph(&doc.root.docs_md),
        });
        std::fs::write(
            out.join(format!("{name}.json")),
            serde_json::to_string_pretty(&doc)?,
        )?;
    }

    let index = ApiIndex {
        generated_with: NIGHTLY.to_string(),
        crates: metas,
    };
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&index)?,
    )?;

    if failed.is_empty() {
        eprintln!("done: {} crates", index.crates.len());
        Ok(())
    } else {
        eprintln!(
            "done: {} ok, {} FAILED: {}",
            index.crates.len(),
            failed.len(),
            failed.join(", ")
        );
        anyhow::bail!("{} crate(s) failed to document", failed.len())
    }
}

fn first_paragraph(md: &str) -> String {
    md.split("\n\n").next().unwrap_or("").trim().to_string()
}

fn bin_target(crate_name: &str) -> Option<&'static str> {
    match crate_name {
        "vmux_cli" => Some("vmux"),
        _ => None,
    }
}
