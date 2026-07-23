//! Detects clickable URLs and file paths in terminal lines and annotates
//! [`TermLine`]s with column-ranged [`LinkRange`]s for the page to render.

use std::path::Path;

use unicode_width::UnicodeWidthChar;
use vmux_command::event::{is_data_uri, looks_like_path};
use vmux_core::event::{LinkRange, TermLine};

/// Punctuation trimmed from the end of a detected token (trailing characters
/// that are almost never part of the link).
const TRAILING_TRIM: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '>'];

/// Punctuation trimmed from the start of a detected token.
const LEADING_TRIM: &[char] = &['(', '[', '{', '<', '"', '\''];

const MAX_LINKS_PER_LINE: usize = 16;

/// Annotate `line` with the links found in its visible text.
///
/// `cwd` is the terminal's working directory, used to resolve relative file
/// paths. When `None`, relative paths are skipped (URLs and absolute paths are
/// still detected).
pub fn annotate_links(line: &mut TermLine, cwd: Option<&Path>) {
    line.links.clear();

    let mut text = String::with_capacity(line.spans.iter().map(|span| span.text.len()).sum());
    for span in &line.spans {
        text.push_str(&span.text);
    }
    if text.is_empty() {
        return;
    }

    let detected = detect_links_in_text(&text, cwd);
    if detected.is_empty() {
        return;
    }

    let mut cols: Vec<(u16, u16)> = Vec::with_capacity(text.chars().count());
    for span in &line.spans {
        let mut col = span.col;
        for ch in span.text.chars() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1) as u16;
            cols.push((col, width));
            col = col.saturating_add(width);
        }
    }

    for (char_start, char_end, url) in detected.into_iter().take(MAX_LINKS_PER_LINE) {
        let Some(&(start_col, _)) = cols.get(char_start) else {
            continue;
        };
        let Some(&(last_col, last_w)) = cols.get(char_end - 1) else {
            continue;
        };
        line.links.push(LinkRange {
            start_col,
            end_col: last_col + last_w - 1,
            url,
        });
    }
}

/// Find link tokens in `text`. Returns `(char_start, char_end_exclusive, url)`
/// in char-index coordinates.
pub fn detect_links_in_text(text: &str, cwd: Option<&Path>) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let mut start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let mut end = i;
        while start < end && LEADING_TRIM.contains(&chars[start]) {
            start += 1;
        }
        while end > start && TRAILING_TRIM.contains(&chars[end - 1]) {
            end -= 1;
        }
        if end <= start {
            continue;
        }
        let token: String = chars[start..end].iter().collect();
        if let Some(url) = resolve_target(&token, cwd) {
            out.push((start, end, url));
        }
    }
    out
}

/// Resolve a token to a ready-to-open URL, or `None` if it is not a link.
///
/// URLs require an explicit scheme (`://`) or a `data:` URI — this avoids
/// misreading bare filenames like `foo.txt` as `https://foo.txt`. Everything
/// else is tested as a file path.
fn resolve_target(token: &str, cwd: Option<&Path>) -> Option<String> {
    if is_data_uri(token) || token.contains("://") {
        return Some(token.to_string());
    }
    if looks_like_path(token) && path_token_has_name(token) {
        return resolve_path(token, cwd);
    }
    None
}

fn path_token_has_name(token: &str) -> bool {
    !token.contains('\\')
        && token
            .chars()
            .any(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-'))
}

/// Resolve a file path token to a `file://` URL, expanding `~/` and resolving
/// relative paths against `cwd`. Returns `None` for a relative path when `cwd`
/// is unknown.
fn resolve_path(token: &str, cwd: Option<&Path>) -> Option<String> {
    let expanded = if let Some(rest) = token.strip_prefix("~/") {
        let home = std::env::var_os("HOME")?;
        Path::new(&home).join(rest)
    } else {
        let p = Path::new(token);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd?.join(p)
        }
    };
    Some(format!("file://{}", expanded.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::event::TermSpan;

    fn line_of(text: &str) -> TermLine {
        TermLine {
            spans: vec![TermSpan {
                text: text.to_string(),
                col: 0,
                grid_cols: text.chars().count() as u16,
                ..Default::default()
            }],
            links: Vec::new(),
        }
    }

    #[test]
    fn detects_https_url() {
        let mut l = line_of("see https://vmux.ai/docs now");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].url, "https://vmux.ai/docs");
        assert_eq!(l.links[0].start_col, 4);
        assert_eq!(l.links[0].end_col, 23);
    }

    #[test]
    fn trims_trailing_punctuation() {
        let mut l = line_of("docs at https://vmux.ai/docs.");
        annotate_links(&mut l, None);
        assert_eq!(l.links[0].url, "https://vmux.ai/docs");
    }

    #[test]
    fn trims_wrapping_parens() {
        let mut l = line_of("(https://vmux.ai/x)");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].url, "https://vmux.ai/x");
    }

    #[test]
    fn detects_absolute_path() {
        let mut l = line_of("edit /Users/me/main.rs please");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].url, "file:///Users/me/main.rs");
    }

    #[test]
    fn ignores_ascii_art_path_punctuation() {
        let mut line = line_of(r"/ \/ /\\ \\// |/\\| ////");

        annotate_links(&mut line, None);

        assert!(line.links.is_empty());
    }

    #[test]
    fn resolves_relative_path_against_cwd() {
        let mut l = line_of("see crates/foo.rs");
        annotate_links(&mut l, Some(Path::new("/work")));
        assert_eq!(l.links[0].url, "file:///work/crates/foo.rs");
    }

    #[test]
    fn skips_relative_path_without_cwd() {
        let mut l = line_of("see crates/foo.rs");
        annotate_links(&mut l, None);
        assert!(l.links.is_empty());
    }

    #[test]
    fn does_not_treat_bare_filename_as_url() {
        let mut l = line_of("opened foo.txt and Cargo.toml");
        annotate_links(&mut l, None);
        assert!(l.links.is_empty());
    }

    #[test]
    fn ignores_bare_words() {
        let mut l = line_of("hello world this is prose");
        annotate_links(&mut l, None);
        assert!(l.links.is_empty());
    }

    #[test]
    fn multiple_links_one_line() {
        let mut l = line_of("https://a.com and https://b.com");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 2);
        assert_eq!(l.links[0].url, "https://a.com");
        assert_eq!(l.links[1].url, "https://b.com");
    }

    #[test]
    fn wide_chars_shift_columns() {
        // 'あ' is width 2; the URL starts at col 0 + 2 + 1(space) = 3.
        let mut l = line_of("あ https://x.io");
        annotate_links(&mut l, None);
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].start_col, 3);
    }
}
