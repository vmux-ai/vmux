use crate::event::{DiffKind, DiffLine, FileStatus, StyledSpan};

pub struct ParsedStatus {
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    pub file_status: FileStatus,
    pub staged_count: u32,
}

fn entry_path(line: &str, kind_tokens: usize) -> &str {
    line.splitn(kind_tokens + 1, ' ')
        .nth(kind_tokens)
        .unwrap_or("")
        .trim()
}

fn xy_status(xy: &str) -> FileStatus {
    let mut c = xy.chars();
    let staged = c.next().unwrap_or('.');
    let unstaged = c.next().unwrap_or('.');
    match (staged, unstaged) {
        ('.', 'D') | ('D', _) => FileStatus::Deleted,
        (s, u) if s != '.' && u != '.' => FileStatus::StagedModified,
        (s, '.') if s != '.' => FileStatus::Staged,
        ('.', u) if u != '.' => FileStatus::Modified,
        _ => FileStatus::Clean,
    }
}

pub fn parse_porcelain_v2(out: &str, target_rel: &str) -> ParsedStatus {
    let mut branch = String::new();
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut has_upstream = false;
    let mut staged_count = 0u32;
    let mut file_status = FileStatus::Clean;

    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = rest.trim().to_string();
        } else if line.starts_with("# branch.upstream ") {
            has_upstream = true;
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for tok in rest.split_whitespace() {
                if let Some(a) = tok.strip_prefix('+') {
                    ahead = a.parse().unwrap_or(0);
                } else if let Some(b) = tok.strip_prefix('-') {
                    behind = b.parse().unwrap_or(0);
                }
            }
        } else if let Some(rest) = line.strip_prefix("1 ").or_else(|| line.strip_prefix("2 ")) {
            let xy = rest.split_whitespace().next().unwrap_or("..");
            if xy.starts_with(|c: char| c != '.') {
                staged_count += 1;
            }
            let kind_tokens = if line.starts_with("2 ") { 9 } else { 8 };
            let path = entry_path(line, kind_tokens)
                .split('\t')
                .next()
                .unwrap_or("");
            if path == target_rel {
                file_status = xy_status(xy);
            }
        } else if let Some(rest) = line.strip_prefix("u ") {
            let _ = rest;
            let path = entry_path(line, 10);
            if path == target_rel {
                file_status = FileStatus::Conflicted;
            }
        } else if let Some(path) = line.strip_prefix("? ")
            && path.trim() == target_rel
        {
            file_status = FileStatus::Untracked;
        }
    }

    ParsedStatus {
        branch,
        ahead,
        behind,
        has_upstream,
        file_status,
        staged_count,
    }
}

/// Repo-relative paths of every changed entry in `git status --porcelain=v2`
/// output — one per `1 `/`2 `/`u `/`? ` line (untracked files included).
pub fn changed_paths(out: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for line in out.lines() {
        let path = if line.starts_with("1 ") || line.starts_with("2 ") {
            let kind_tokens = if line.starts_with("2 ") { 9 } else { 8 };
            entry_path(line, kind_tokens)
                .split('\t')
                .next()
                .unwrap_or("")
                .to_string()
        } else if line.starts_with("u ") {
            entry_path(line, 10).to_string()
        } else if let Some(rest) = line.strip_prefix("? ") {
            rest.trim().to_string()
        } else {
            continue;
        };
        if !path.is_empty() {
            set.insert(path);
        }
    }
    set
}

fn span(text: &str, fg: [u8; 3]) -> Vec<StyledSpan> {
    vec![StyledSpan {
        text: text.to_string(),
        fg,
        bold: false,
        italic: false,
    }]
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let body = line.strip_prefix("@@ ")?;
    let end = body.find(" @@")?;
    let ranges = &body[..end];
    let mut parts = ranges.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let old_start: u32 = old.split(',').next()?.parse().ok()?;
    let new_start: u32 = new.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

pub fn parse_unified_diff(diff: &str) -> Vec<DiffLine> {
    const ADD: [u8; 3] = [80, 200, 120];
    const REM: [u8; 3] = [220, 80, 80];
    const CTX: [u8; 3] = [200, 200, 200];
    const HUNK: [u8; 3] = [120, 140, 170];

    let mut lines = Vec::new();
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut saw_hunk = false;

    for raw in diff.lines() {
        if raw.starts_with("\\ No newline") {
            continue;
        }
        if !saw_hunk
            && (raw.starts_with("diff --git")
                || raw.starts_with("index ")
                || raw.starts_with("--- ")
                || raw.starts_with("+++ "))
        {
            continue;
        }
        if raw.starts_with("@@") {
            saw_hunk = true;
            if let Some((o, n)) = parse_hunk_header(raw) {
                old_no = o;
                new_no = n;
            }
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                old_no: None,
                new_no: None,
                hunk: None,
                spans: span(raw, HUNK),
            });
            continue;
        }
        match raw.chars().next() {
            Some('+') => {
                lines.push(DiffLine {
                    kind: DiffKind::Add,
                    old_no: None,
                    new_no: Some(new_no),
                    hunk: None,
                    spans: span(&raw[1..], ADD),
                });
                new_no += 1;
            }
            Some('-') => {
                lines.push(DiffLine {
                    kind: DiffKind::Remove,
                    old_no: Some(old_no),
                    new_no: None,
                    hunk: None,
                    spans: span(&raw[1..], REM),
                });
                old_no += 1;
            }
            _ => {
                let text = raw.strip_prefix(' ').unwrap_or(raw);
                lines.push(DiffLine {
                    kind: DiffKind::Context,
                    old_no: Some(old_no),
                    new_no: Some(new_no),
                    hunk: None,
                    spans: span(text, CTX),
                });
                old_no += 1;
                new_no += 1;
            }
        }
    }
    lines
}

pub fn window(lines: &[DiffLine], top_line: u32, rows: u32) -> (u32, Vec<DiffLine>) {
    let total = lines.len() as u32;
    let start = top_line.min(total) as usize;
    let end = (top_line.saturating_add(rows)).min(total) as usize;
    (total, lines[start..end].to_vec())
}

pub struct HunkRange {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
}

fn parse_range(s: &str) -> Option<(u32, u32)> {
    let mut it = s.split(',');
    let start: u32 = it.next()?.parse().ok()?;
    let count: u32 = it.next().map(|c| c.parse().unwrap_or(1)).unwrap_or(1);
    Some((start, count))
}

fn parse_hunk_range_line(line: &str) -> Option<HunkRange> {
    let body = line.strip_prefix("@@ ")?;
    let end = body.find(" @@")?;
    let mut parts = body[..end].split_whitespace();
    let (old_start, old_count) = parse_range(parts.next()?.strip_prefix('-')?)?;
    let (new_start, new_count) = parse_range(parts.next()?.strip_prefix('+')?)?;
    Some(HunkRange {
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

pub fn hunk_ranges(diff: &str) -> Vec<HunkRange> {
    diff.lines().filter_map(parse_hunk_range_line).collect()
}

pub fn hunk_patches(diff: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in diff.lines() {
        if line.starts_with("@@") {
            hunks.push(String::new());
        }
        match hunks.last_mut() {
            Some(h) => {
                h.push_str(line);
                h.push('\n');
            }
            None => {
                header.push_str(line);
                header.push('\n');
            }
        }
    }
    (header, hunks)
}

#[cfg(test)]
mod porcelain_tests {
    use super::*;

    const OUT: &str = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 aaa bbb src/main.rs\n1 M. N... 100644 100644 100644 ccc ddd src/lib.rs\n? notes.txt\n";

    #[test]
    fn parses_branch_and_ahead_behind() {
        let p = parse_porcelain_v2(OUT, "src/main.rs");
        assert_eq!(p.branch, "main");
        assert_eq!(p.ahead, 2);
        assert_eq!(p.behind, 1);
        assert!(p.has_upstream);
    }

    #[test]
    fn target_unstaged_modified() {
        assert_eq!(
            parse_porcelain_v2(OUT, "src/main.rs").file_status,
            FileStatus::Modified
        );
    }

    #[test]
    fn target_staged() {
        assert_eq!(
            parse_porcelain_v2(OUT, "src/lib.rs").file_status,
            FileStatus::Staged
        );
    }

    #[test]
    fn target_untracked() {
        assert_eq!(
            parse_porcelain_v2(OUT, "notes.txt").file_status,
            FileStatus::Untracked
        );
    }

    #[test]
    fn target_clean_when_absent() {
        assert_eq!(
            parse_porcelain_v2(OUT, "README.md").file_status,
            FileStatus::Clean
        );
    }

    #[test]
    fn staged_count_counts_staged_column() {
        assert_eq!(parse_porcelain_v2(OUT, "src/main.rs").staged_count, 1);
    }

    #[test]
    fn no_upstream_header() {
        let out = "# branch.head feature\n";
        let p = parse_porcelain_v2(out, "x");
        assert!(!p.has_upstream);
        assert_eq!(p.ahead, 0);
        assert_eq!(p.behind, 0);
    }

    #[test]
    fn changed_paths_collects_all_entry_kinds() {
        let out = "# branch.head main\n\
1 .M N... 100644 100644 100644 aaa bbb src/main.rs\n\
1 M. N... 100644 100644 100644 ccc ddd src/lib.rs\n\
2 R. N... 100644 100644 100644 eee fff R100 new.rs\told.rs\n\
u UU N... 100644 100644 100644 100644 ggg hhh iii conflict.rs\n\
? notes.txt\n";
        let set = changed_paths(out);
        assert!(set.contains("src/main.rs"));
        assert!(set.contains("src/lib.rs"));
        assert!(set.contains("new.rs"));
        assert!(!set.contains("old.rs"));
        assert!(set.contains("conflict.rs"));
        assert!(set.contains("notes.txt"));
        assert_eq!(set.len(), 5);
    }
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    const DIFF: &str = "diff --git a/f.rs b/f.rs\nindex 1..2 100644\n--- a/f.rs\n+++ b/f.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let x = 1;\n+    let x = 2;\n }\n";

    #[test]
    fn skips_file_headers() {
        let lines = parse_unified_diff(DIFF);
        assert!(
            !lines
                .iter()
                .any(|l| l.spans[0].text.starts_with("diff --git"))
        );
        assert!(!lines.iter().any(|l| l.spans[0].text.starts_with("+++")));
    }

    #[test]
    fn classifies_kinds_and_numbers() {
        let lines = parse_unified_diff(DIFF);
        let hunk = &lines[0];
        assert!(matches!(hunk.kind, DiffKind::Hunk));

        let ctx = &lines[1];
        assert!(matches!(ctx.kind, DiffKind::Context));
        assert_eq!(ctx.old_no, Some(1));
        assert_eq!(ctx.new_no, Some(1));

        let rem = &lines[2];
        assert!(matches!(rem.kind, DiffKind::Remove));
        assert_eq!(rem.old_no, Some(2));
        assert_eq!(rem.new_no, None);

        let add = &lines[3];
        assert!(matches!(add.kind, DiffKind::Add));
        assert_eq!(add.old_no, None);
        assert_eq!(add.new_no, Some(2));
    }

    #[test]
    fn empty_diff_yields_no_lines() {
        assert!(parse_unified_diff("").is_empty());
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    fn lines(n: u32) -> Vec<DiffLine> {
        (0..n)
            .map(|i| DiffLine {
                kind: DiffKind::Context,
                old_no: Some(i),
                new_no: Some(i),
                hunk: None,
                spans: vec![],
            })
            .collect()
    }

    #[test]
    fn returns_total_and_slice() {
        let (total, win) = window(&lines(10), 2, 3);
        assert_eq!(total, 10);
        assert_eq!(win.len(), 3);
        assert_eq!(win[0].old_no, Some(2));
    }

    #[test]
    fn clamps_at_bottom() {
        let (total, win) = window(&lines(10), 8, 5);
        assert_eq!(total, 10);
        assert_eq!(win.len(), 2);
    }

    #[test]
    fn top_past_end_is_empty() {
        let (_, win) = window(&lines(3), 99, 5);
        assert!(win.is_empty());
    }

    #[test]
    fn empty_input() {
        let (total, win) = window(&[], 0, 5);
        assert_eq!(total, 0);
        assert!(win.is_empty());
    }
}
