pub struct Doc {
    pub slug: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub content: &'static str,
}

pub const DOCS: &[Doc] = &[Doc {
    slug: "architecture",
    title: "Architecture",
    group: "Overview",
    content: include_str!("../../docs/architecture.md"),
}];

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
