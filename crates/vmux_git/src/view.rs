use std::collections::{HashMap, HashSet};

use crate::event::{DiffKind, DiffLine};

pub const DEFAULT_CONTEXT_LINES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffViewRow {
    Line(usize),
    Gap { start: usize, end: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorDiffMarker {
    Added,
    Modified,
    Deleted,
    Staged,
}

fn insert_marker(
    markers: &mut HashMap<u32, EditorDiffMarker>,
    line: u32,
    marker: EditorDiffMarker,
) {
    let priority = |marker| match marker {
        EditorDiffMarker::Staged => 0,
        EditorDiffMarker::Deleted => 1,
        EditorDiffMarker::Added => 2,
        EditorDiffMarker::Modified => 3,
    };
    match markers.get(&line) {
        Some(current) if priority(*current) >= priority(marker) => {}
        _ => {
            markers.insert(line, marker);
        }
    }
}

pub fn editor_diff_markers(lines: &[DiffLine]) -> HashMap<u32, EditorDiffMarker> {
    let mut markers = HashMap::new();
    let mut hunk_kinds = HashMap::<u32, (bool, bool)>::new();
    for line in lines {
        let Some(hunk) = line.hunk else {
            continue;
        };
        let kinds = hunk_kinds.entry(hunk).or_default();
        match line.kind {
            DiffKind::Add => kinds.0 = true,
            DiffKind::Remove => kinds.1 = true,
            _ => {}
        }
    }
    let mut replacement_lines = HashSet::new();
    let mut i = 0;
    while i < lines.len() {
        if matches!(lines[i].kind, DiffKind::Context | DiffKind::Staged) || lines[i].hunk.is_some()
        {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len()
            && !matches!(lines[i].kind, DiffKind::Context | DiffKind::Staged)
            && lines[i].hunk.is_none()
        {
            i += 1;
        }
        let range = start..i;
        let has_add = range
            .clone()
            .any(|index| matches!(lines[index].kind, DiffKind::Add));
        let has_remove = range
            .clone()
            .any(|index| matches!(lines[index].kind, DiffKind::Remove));
        if has_add && has_remove {
            replacement_lines.extend(range);
        }
    }

    for (i, line) in lines.iter().enumerate() {
        match line.kind {
            DiffKind::Add => {
                let Some(new_no) = line.new_no else {
                    continue;
                };
                let modified = line
                    .hunk
                    .and_then(|hunk| hunk_kinds.get(&hunk))
                    .is_some_and(|(added, removed)| *added && *removed)
                    || replacement_lines.contains(&i);
                insert_marker(
                    &mut markers,
                    new_no,
                    if modified {
                        EditorDiffMarker::Modified
                    } else {
                        EditorDiffMarker::Added
                    },
                );
            }
            DiffKind::Remove => {
                let next = lines[i + 1..].iter().find_map(|next| next.new_no);
                let previous = lines[..i].iter().rev().find_map(|previous| previous.new_no);
                if let Some(anchor) = next.or(previous) {
                    insert_marker(&mut markers, anchor, EditorDiffMarker::Deleted);
                }
            }
            DiffKind::Staged => {
                if let Some(new_no) = line.new_no {
                    insert_marker(&mut markers, new_no, EditorDiffMarker::Staged);
                }
            }
            DiffKind::Context | DiffKind::Hunk => {}
        }
    }
    markers
}

pub fn diff_view_rows(lines: &[DiffLine], expanded: &HashSet<(usize, usize)>) -> Vec<DiffViewRow> {
    let mut visible = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if matches!(line.kind, DiffKind::Context) {
            continue;
        }
        let start = i.saturating_sub(DEFAULT_CONTEXT_LINES);
        let end = (i + DEFAULT_CONTEXT_LINES + 1).min(lines.len());
        visible[start..end].fill(true);
    }

    let mut rows = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if visible[i] {
            rows.push(DiffViewRow::Line(i));
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && !visible[i] {
            i += 1;
        }
        let end = i;
        if expanded.contains(&(start, end)) {
            rows.extend((start..end).map(DiffViewRow::Line));
        } else {
            rows.push(DiffViewRow::Gap { start, end });
        }
    }
    rows
}

#[cfg(test)]
#[path = "view.test.rs"]
mod tests;
