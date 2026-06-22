use dioxus::prelude::*;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Clone, PartialEq)]
enum Node {
    Heading(u8, Vec<Node>),
    Paragraph(Vec<Node>),
    BlockQuote(Vec<Node>),
    CodeBlock(String),
    List(Option<u64>, Vec<Vec<Node>>),
    Rule,
    Table(Vec<Vec<Node>>, Vec<Vec<Vec<Node>>>),
    Text(String),
    Code(String),
    Strong(Vec<Node>),
    Emphasis(Vec<Node>),
    Strikethrough(Vec<Node>),
    Link(String, Vec<Node>),
    SoftBreak,
    HardBreak,
}

#[derive(Default)]
struct Frame {
    children: Vec<Node>,
    items: Vec<Vec<Node>>,
    header: Vec<Vec<Node>>,
    rows: Vec<Vec<Vec<Node>>>,
    list_start: Option<u64>,
    link_href: String,
    heading: u8,
    is_code: bool,
    code: String,
}

fn hlevel(h: HeadingLevel) -> u8 {
    match h {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn push_node(stack: &mut [Frame], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

fn parse(md: &str) -> Vec<Node> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    let mut stack: Vec<Frame> = vec![Frame::default()];

    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(tag) => {
                let mut f = Frame::default();
                match &tag {
                    Tag::Heading { level, .. } => f.heading = hlevel(*level),
                    Tag::List(start) => f.list_start = *start,
                    Tag::Link { dest_url, .. } => f.link_href = dest_url.to_string(),
                    Tag::CodeBlock(kind) => {
                        f.is_code = true;
                        if let CodeBlockKind::Fenced(_) = kind {}
                    }
                    _ => {}
                }
                stack.push(f);
            }
            Event::End(end) => {
                let f = stack.pop().unwrap_or_default();
                match end {
                    TagEnd::Heading(_) => push_node(&mut stack, Node::Heading(f.heading, f.children)),
                    TagEnd::Paragraph => push_node(&mut stack, Node::Paragraph(f.children)),
                    TagEnd::BlockQuote(_) => push_node(&mut stack, Node::BlockQuote(f.children)),
                    TagEnd::Strong => push_node(&mut stack, Node::Strong(f.children)),
                    TagEnd::Emphasis => push_node(&mut stack, Node::Emphasis(f.children)),
                    TagEnd::Strikethrough => push_node(&mut stack, Node::Strikethrough(f.children)),
                    TagEnd::Link => push_node(&mut stack, Node::Link(f.link_href, f.children)),
                    TagEnd::CodeBlock => push_node(&mut stack, Node::CodeBlock(f.code)),
                    TagEnd::Item => {
                        if let Some(top) = stack.last_mut() {
                            top.items.push(f.children);
                        }
                    }
                    TagEnd::List(_) => push_node(&mut stack, Node::List(f.list_start, f.items)),
                    TagEnd::TableCell => {
                        if let Some(top) = stack.last_mut() {
                            top.items.push(f.children);
                        }
                    }
                    TagEnd::TableHead => {
                        if let Some(top) = stack.last_mut() {
                            top.header = f.items;
                        }
                    }
                    TagEnd::TableRow => {
                        if let Some(top) = stack.last_mut() {
                            top.rows.push(f.items);
                        }
                    }
                    TagEnd::Table => push_node(&mut stack, Node::Table(f.header, f.rows)),
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    if top.is_code {
                        top.code.push_str(&t);
                    } else {
                        top.children.push(Node::Text(t.to_string()));
                    }
                }
            }
            Event::Code(t) => push_node(&mut stack, Node::Code(t.to_string())),
            Event::SoftBreak => push_node(&mut stack, Node::SoftBreak),
            Event::HardBreak => push_node(&mut stack, Node::HardBreak),
            Event::Rule => push_node(&mut stack, Node::Rule),
            _ => {}
        }
    }

    stack.pop().map(|f| f.children).unwrap_or_default()
}

#[derive(Clone, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub id: String,
}

pub fn headings(content: &str) -> Vec<Heading> {
    parse(content)
        .into_iter()
        .filter_map(|n| match n {
            Node::Heading(level, ch) if level == 2 || level == 3 => {
                let text = node_text(&ch);
                let id = slugify(&text);
                Some(Heading { level, text, id })
            }
            _ => None,
        })
        .collect()
}

fn node_text(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            Node::Text(t) | Node::Code(t) => s.push_str(t),
            Node::Strong(c) | Node::Emphasis(c) | Node::Strikethrough(c) | Node::Link(_, c) => {
                s.push_str(&node_text(c))
            }
            Node::SoftBreak | Node::HardBreak => s.push(' '),
            _ => {}
        }
    }
    s
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

fn render_nodes(nodes: &[Node]) -> Element {
    rsx! {
        for n in nodes.iter() {
            {render_node(n)}
        }
    }
}

fn render_node(n: &Node) -> Element {
    match n {
        Node::Heading(level, ch) => {
            let id = slugify(&node_text(ch));
            match level {
                1 => rsx! { h1 { id: "{id}", class: "scroll-mt-6 text-3xl sm:text-4xl font-bold tracking-tight mt-10 mb-4", {render_nodes(ch)} } },
                2 => rsx! { h2 { id: "{id}", class: "scroll-mt-6 text-2xl font-semibold tracking-tight mt-10 mb-3 pb-2 border-b border-border", {render_nodes(ch)} } },
                3 => rsx! { h3 { id: "{id}", class: "scroll-mt-6 text-lg font-semibold mt-6 mb-2 text-accent", {render_nodes(ch)} } },
                4 => rsx! { h4 { id: "{id}", class: "scroll-mt-6 text-base font-semibold mt-5 mb-2", {render_nodes(ch)} } },
                5 => rsx! { h5 { id: "{id}", class: "scroll-mt-6 text-sm font-semibold uppercase tracking-wide text-text-muted mt-4 mb-1", {render_nodes(ch)} } },
                _ => rsx! { h6 { id: "{id}", class: "scroll-mt-6 text-sm font-semibold text-text-muted mt-4 mb-1", {render_nodes(ch)} } },
            }
        }
        Node::Paragraph(ch) => rsx! { p { class: "my-4 leading-relaxed", {render_nodes(ch)} } },
        Node::BlockQuote(ch) => rsx! {
            blockquote { class: "border-l-4 border-accent bg-surface rounded-r-lg px-4 py-2 my-5 text-text-muted",
                {render_nodes(ch)}
            }
        },
        Node::CodeBlock(code) => rsx! {
            pre { class: "bg-code-bg border border-border rounded-lg p-4 my-5 overflow-x-auto",
                code { class: "font-mono text-sm leading-relaxed", "{code}" }
            }
        },
        Node::List(start, items) => {
            if start.is_some() {
                rsx! {
                    ol { class: "list-decimal pl-6 my-4 space-y-1.5",
                        for it in items.iter() {
                            li { class: "leading-relaxed", {render_nodes(it)} }
                        }
                    }
                }
            } else {
                rsx! {
                    ul { class: "list-disc pl-6 my-4 space-y-1.5",
                        for it in items.iter() {
                            li { class: "leading-relaxed", {render_nodes(it)} }
                        }
                    }
                }
            }
        }
        Node::Rule => rsx! { hr { class: "border-0 border-t border-border my-8" } },
        Node::Table(header, rows) => rsx! {
            div { class: "my-6 overflow-x-auto",
                table { class: "w-full text-sm border-collapse",
                    thead {
                        tr {
                            for c in header.iter() {
                                th { class: "text-left font-semibold text-text border-b border-border px-3 py-2",
                                    {render_nodes(c)}
                                }
                            }
                        }
                    }
                    tbody {
                        for row in rows.iter() {
                            tr {
                                for c in row.iter() {
                                    td { class: "align-top border-b border-border px-3 py-2 text-text-muted",
                                        {render_nodes(c)}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        Node::Text(t) => rsx! { "{t}" },
        Node::Code(t) => rsx! {
            code { class: "font-mono text-[0.85em] bg-code-bg text-accent rounded-md border border-border px-1.5 py-0.5", "{t}" }
        },
        Node::Strong(ch) => rsx! { strong { class: "font-semibold text-text", {render_nodes(ch)} } },
        Node::Emphasis(ch) => rsx! { em { class: "italic", {render_nodes(ch)} } },
        Node::Strikethrough(ch) => rsx! { s { class: "line-through opacity-70", {render_nodes(ch)} } },
        Node::Link(href, ch) => {
            let resolved = resolve_link(href);
            let external = resolved.starts_with("http");
            rsx! {
                a {
                    class: "text-accent underline underline-offset-2 hover:text-accent-hover",
                    href: "{resolved}",
                    target: if external { "_blank" },
                    rel: if external { "noopener noreferrer" },
                    {render_nodes(ch)}
                }
            }
        }
        Node::SoftBreak => rsx! { " " },
        Node::HardBreak => rsx! { br {} },
    }
}

fn resolve_link(href: &str) -> String {
    if href.starts_with("http") || href.starts_with('#') || href.starts_with('/') {
        return href.to_string();
    }
    let (path, frag) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (href, None),
    };
    match path.strip_suffix(".md") {
        Some(stripped) => {
            let slug = stripped.rsplit('/').next().unwrap_or(stripped);
            match frag {
                Some(f) => format!("/docs/{slug}#{f}"),
                None => format!("/docs/{slug}"),
            }
        }
        None => href.to_string(),
    }
}

#[component]
pub fn Markdown(content: String) -> Element {
    let nodes = parse(&content);
    rsx! {
        div { class: "text-text text-[15px]", {render_nodes(&nodes)} }
    }
}
