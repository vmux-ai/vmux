use crate::event::{DiffKind, DiffLine};

pub const DEFAULT_CONTEXT_LINES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffViewRow {
    Line(usize),
    Gap { start: usize, end: usize },
}

pub const GAP_REVEAL_CHUNK: usize = 20;

pub fn diff_view_rows(lines: &[DiffLine], revealed: &[(usize, usize)]) -> Vec<DiffViewRow> {
    let mut visible = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if matches!(line.kind, DiffKind::Context) {
            continue;
        }
        let start = i.saturating_sub(DEFAULT_CONTEXT_LINES);
        let end = (i + DEFAULT_CONTEXT_LINES + 1).min(lines.len());
        visible[start..end].fill(true);
    }
    for (start, end) in revealed {
        let start = (*start).min(lines.len());
        let end = (*end).min(lines.len());
        if start < end {
            visible[start..end].fill(true);
        }
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
        rows.push(DiffViewRow::Gap { start, end: i });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::StyledSpan;

    fn line(kind: DiffKind, no: u32) -> DiffLine {
        DiffLine {
            kind,
            old_no: Some(no),
            new_no: Some(no),
            hunk: None,
            spans: Vec::<StyledSpan>::new(),
        }
    }

    #[test]
    fn collapses_context_outside_changed_hunks() {
        let mut lines = (1..=20)
            .map(|no| line(DiffKind::Context, no))
            .collect::<Vec<_>>();
        lines[9].kind = DiffKind::Add;

        let rows = diff_view_rows(&lines, &[]);

        assert_eq!(rows.first(), Some(&DiffViewRow::Gap { start: 0, end: 6 }));
        assert_eq!(rows.last(), Some(&DiffViewRow::Gap { start: 13, end: 20 }));
        assert!(rows.contains(&DiffViewRow::Line(9)));
    }

    #[test]
    fn revealing_a_chunk_leaves_the_rest_of_the_gap_collapsed() {
        let mut lines = (1..=60)
            .map(|no| line(DiffKind::Context, no))
            .collect::<Vec<_>>();
        lines[49].kind = DiffKind::Add;

        let rows = diff_view_rows(&lines, &[(0, GAP_REVEAL_CHUNK)]);

        assert!(rows.contains(&DiffViewRow::Line(0)));
        assert!(rows.contains(&DiffViewRow::Gap {
            start: GAP_REVEAL_CHUNK,
            end: 46
        }));
    }

    #[test]
    fn expands_selected_context_gap() {
        let mut lines = (1..=20)
            .map(|no| line(DiffKind::Context, no))
            .collect::<Vec<_>>();
        lines[9].kind = DiffKind::Add;
        let rows = diff_view_rows(&lines, &[(0, 6)]);

        assert_eq!(rows.first(), Some(&DiffViewRow::Line(0)));
        assert!(!rows.contains(&DiffViewRow::Gap { start: 0, end: 6 }));
        assert!(rows.contains(&DiffViewRow::Gap { start: 13, end: 20 }));
    }
}
