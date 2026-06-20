pub struct Doc {
    pub slug: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub content: &'static str,
}

pub const DOCS: &[Doc] = &[
    Doc {
        slug: "experience",
        title: "UX Philosophy",
        group: "Overview",
        content: include_str!("../../docs/experience.md"),
    },
    Doc {
        slug: "architecture",
        title: "Architecture",
        group: "Overview",
        content: include_str!("../../docs/architecture.md"),
    },
    Doc {
        slug: "why-rust-cef",
        title: "Why Rust + CEF",
        group: "Architecture",
        content: include_str!("../../docs/architecture/why-rust-cef.md"),
    },
    Doc {
        slug: "rust-without-the-headaches",
        title: "Rust without the headaches",
        group: "Architecture",
        content: include_str!("../../docs/architecture/rust-without-the-headaches.md"),
    },
    Doc {
        slug: "built-to-scale",
        title: "Built to scale",
        group: "Architecture",
        content: include_str!("../../docs/architecture/built-to-scale.md"),
    },
    Doc {
        slug: "agent-first",
        title: "Agent-first API",
        group: "Architecture",
        content: include_str!("../../docs/architecture/agent-first.md"),
    },
    Doc {
        slug: "layout-model",
        title: "The layout model",
        group: "Architecture",
        content: include_str!("../../docs/architecture/layout-model.md"),
    },
    Doc {
        slug: "render-stack",
        title: "The render stack",
        group: "Architecture",
        content: include_str!("../../docs/architecture/render-stack.md"),
    },
];

pub fn find(slug: &str) -> Option<&'static Doc> {
    DOCS.iter().find(|d| d.slug == slug)
}

pub fn groups() -> Vec<(&'static str, Vec<usize>)> {
    let mut out: Vec<(&'static str, Vec<usize>)> = Vec::new();
    for (i, d) in DOCS.iter().enumerate() {
        match out.iter_mut().find(|(g, _)| *g == d.group) {
            Some((_, idxs)) => idxs.push(i),
            None => out.push((d.group, vec![i])),
        }
    }
    out
}
