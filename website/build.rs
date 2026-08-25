use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FENCE: &str = "```mermaid";

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let docs = manifest.join("..").join("docs");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("mermaid.rs");

    let mut rendered: BTreeMap<String, String> = BTreeMap::new();
    for source in sources(&docs) {
        println!("cargo::rerun-if-changed={}", source.display());
        let markdown = std::fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("read {}: {error}", source.display()));
        for diagram in Diagram::all(&markdown) {
            if rendered.contains_key(&diagram) {
                continue;
            }
            rendered.insert(diagram.clone(), Diagram::render(&diagram, &source));
        }
    }

    let mut generated = String::from("pub static DIAGRAMS: &[(&str, &str)] = &[\n");
    for (diagram, svg) in &rendered {
        generated.push_str(&format!("    ({:?}, {:?}),\n", diagram, svg));
    }
    generated.push_str("];\n");
    std::fs::write(&out, generated).unwrap_or_else(|error| panic!("write {out:?}: {error}"));
}

fn sources(docs: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(docs)
        .unwrap_or_else(|error| panic!("read {}: {error}", docs.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    found.sort();
    found
}

struct Theme;

impl Theme {
    fn site() -> mermaid_rs_renderer::Theme {
        let mut theme = mermaid_rs_renderer::Theme::dark();
        theme.background = "transparent".into();
        theme.primary_color = "#1a1a2e".into();
        theme.primary_border_color = "#2a2a2a".into();
        theme.primary_text_color = "#e0e0e0".into();
        theme.text_color = "#e0e0e0".into();
        theme.line_color = "#888".into();
        theme.edge_label_background = "transparent".into();
        theme.cluster_background = "#151515".into();
        theme.cluster_border = "#2a2a2a".into();
        theme.sequence_actor_fill = "#1a1a2e".into();
        theme.sequence_actor_border = "#2a2a2a".into();
        theme.sequence_actor_line = "#888".into();
        theme.sequence_note_fill = "#151515".into();
        theme.sequence_note_border = "#2a2a2a".into();
        theme
    }
}

struct Diagram;

impl Diagram {
    fn all(markdown: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = markdown;
        while let Some(start) = rest.find(FENCE) {
            let after = &rest[start + FENCE.len()..];
            let Some(body) = after.strip_prefix('\n') else {
                rest = after;
                continue;
            };
            let Some(end) = body.find("\n```") else {
                break;
            };
            found.push(body[..end].to_string());
            rest = &body[end..];
        }
        found
    }

    fn render(diagram: &str, source: &Path) -> String {
        let options = mermaid_rs_renderer::RenderOptions {
            theme: Theme::site(),
            ..Default::default()
        };
        match mermaid_rs_renderer::render_with_options(diagram, options) {
            Ok(svg) => svg,
            Err(error) => panic!(
                "{}: mermaid diagram did not render: {error}\n---\n{diagram}\n---",
                source.display()
            ),
        }
    }
}
