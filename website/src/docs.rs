pub struct Doc {
    pub slug: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub content: &'static str,
}

pub const DOCS: &[Doc] = &[
    Doc {
        slug: "getting-things-done",
        title: "Get something done",
        group: "UX",
        content: include_str!("../../docs/ux/getting-things-done.md"),
    },
    Doc {
        slug: "workspace",
        title: "Projects and Sessions",
        group: "UX",
        content: include_str!("../../docs/ux/workspace.md"),
    },
    Doc {
        slug: "browser",
        title: "Browser",
        group: "UX",
        content: include_str!("../../docs/ux/browser.md"),
    },
    Doc {
        slug: "code-and-git",
        title: "Code and Git",
        group: "UX",
        content: include_str!("../../docs/ux/code-and-git.md"),
    },
    Doc {
        slug: "terminal",
        title: "Terminal",
        group: "UX",
        content: include_str!("../../docs/ux/terminal.md"),
    },
    Doc {
        slug: "agents",
        title: "Agents",
        group: "UX",
        content: include_str!("../../docs/ux/agents.md"),
    },
    Doc {
        slug: "remote",
        title: "Remote access",
        group: "UX",
        content: include_str!("../../docs/ux/remote.md"),
    },
    Doc {
        slug: "getting-started",
        title: "Getting started",
        group: "DX",
        content: include_str!("../../docs/dx/getting-started.md"),
    },
    Doc {
        slug: "customization",
        title: "Make your own Vmux",
        group: "DX",
        content: include_str!("../../docs/dx/customization.md"),
    },
    Doc {
        slug: "architecture",
        title: "Architecture",
        group: "DX",
        content: include_str!("../../docs/dx/architecture.md"),
    },
    Doc {
        slug: "feature-development",
        title: "Feature development",
        group: "DX",
        content: include_str!("../../docs/dx/feature-development.md"),
    },
    Doc {
        slug: "validation-and-release",
        title: "Validation and release",
        group: "DX",
        content: include_str!("../../docs/dx/validation-and-release.md"),
    },
    Doc {
        slug: "framework",
        title: "Vmux Framework",
        group: "DX",
        content: include_str!("../../docs/dx/framework.md"),
    },
];

impl Doc {
    pub fn find(slug: &str) -> Option<&'static Self> {
        DOCS.iter().find(|doc| doc.slug == slug)
    }

    pub fn neighbors(slug: &str) -> (Option<&'static Self>, Option<&'static Self>) {
        let Some(index) = DOCS.iter().position(|doc| doc.slug == slug) else {
            return (None, None);
        };
        let previous = index.checked_sub(1).and_then(|index| DOCS.get(index));
        (previous, DOCS.get(index + 1))
    }

    pub fn groups() -> Vec<(&'static str, Vec<usize>)> {
        let mut groups: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (index, doc) in DOCS.iter().enumerate() {
            if let Some((_, entries)) = groups.iter_mut().find(|(group, _)| *group == doc.group) {
                entries.push(index);
            } else {
                groups.push((doc.group, vec![index]));
            }
        }
        groups
    }

    pub fn target(href: &str) -> String {
        if href.starts_with("http") || href.starts_with('#') || href.starts_with('/') {
            return href.to_string();
        }
        let (path, fragment) = match href.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment)),
            None => (href, None),
        };
        let Some(path) = path.strip_suffix(".md") else {
            return href.to_string();
        };
        let slug = path.rsplit('/').next().unwrap_or(path);
        let route = if slug == "README" {
            "/docs".to_string()
        } else if let Some(doc) = Self::find(slug) {
            format!("/docs/{}", doc.slug)
        } else {
            return href.to_string();
        };
        match fragment {
            Some(fragment) => format!("{route}#{fragment}"),
            None => route,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Doc;

    #[test]
    fn guide_links_resolve_across_categories_without_losing_fragments() {
        for (source, expected) in [
            (
                "../dx/architecture.md#invariants",
                "/docs/architecture#invariants",
            ),
            ("../ux/workspace.md", "/docs/workspace"),
            ("../README.md", "/docs"),
            ("#design-vocabulary", "#design-vocabulary"),
            ("/docs/getting-started", "/docs/getting-started"),
            (
                "https://github.com/vmux-ai/vmux/blob/main/AGENTS.md",
                "https://github.com/vmux-ai/vmux/blob/main/AGENTS.md",
            ),
            ("../not-published.md", "../not-published.md"),
            (
                "../../crates/app/vmux_cli/src/main.rs",
                "../../crates/app/vmux_cli/src/main.rs",
            ),
        ] {
            assert_eq!(Doc::target(source), expected, "{source}");
        }
    }
}
