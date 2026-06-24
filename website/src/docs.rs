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
        title: "Rust for React JS developers",
        group: "Architecture",
        content: include_str!("../../docs/architecture/rust-without-the-headaches.md"),
    },
    Doc {
        slug: "built-to-scale",
        title: "ECS, explained",
        group: "Architecture",
        content: include_str!("../../docs/architecture/built-to-scale.md"),
    },
    Doc {
        slug: "plugins",
        title: "Plugins",
        group: "Architecture",
        content: include_str!("../../docs/architecture/plugins.md"),
    },
    Doc {
        slug: "agent-first",
        title: "MCP Integration",
        group: "Architecture",
        content: include_str!("../../docs/architecture/agent-first.md"),
    },
    Doc {
        slug: "background-service",
        title: "Background Service",
        group: "Architecture",
        content: include_str!("../../docs/architecture/background-service.md"),
    },
    Doc {
        slug: "layout-model",
        title: "Layout",
        group: "Architecture",
        content: include_str!("../../docs/architecture/layout-model.md"),
    },
    Doc {
        slug: "render-stack",
        title: "2D / 3D renderer",
        group: "Architecture",
        content: include_str!("../../docs/architecture/render-stack.md"),
    },
];

pub fn find(slug: &str) -> Option<&'static Doc> {
    DOCS.iter().find(|d| d.slug == slug)
}

pub fn neighbors(slug: &str) -> (Option<&'static Doc>, Option<&'static Doc>) {
    match DOCS.iter().position(|d| d.slug == slug) {
        Some(i) => {
            let prev = if i > 0 { Some(&DOCS[i - 1]) } else { None };
            (prev, DOCS.get(i + 1))
        }
        None => (None, None),
    }
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
