use std::collections::HashSet;

use ropey::Rope;
use vmux_core::event::FoldGutter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldRegion {
    pub start: u32,
    pub end: u32,
}

impl FoldRegion {
    pub fn contains_body(self, line: u32) -> bool {
        line > self.start && line <= self.end
    }

    pub fn contains(self, line: u32) -> bool {
        line >= self.start && line <= self.end
    }
}

#[derive(Default, Clone, Debug)]
pub struct FoldState {
    pub regions: Vec<FoldRegion>,
    pub collapsed: HashSet<u32>,
}

impl FoldState {
    pub fn set_regions(&mut self, regions: Vec<FoldRegion>) {
        self.regions = regions;
        self.reconcile();
    }

    fn region_at_start(&self, start: u32) -> Option<FoldRegion> {
        self.regions.iter().copied().find(|r| r.start == start)
    }

    pub fn enclosing(&self, line: u32) -> Option<FoldRegion> {
        self.regions
            .iter()
            .copied()
            .filter(|r| r.contains(line))
            .min_by_key(|r| r.end - r.start)
    }

    pub fn sticky(&self, line: u32, depth: usize) -> Vec<u32> {
        let mut headers = Vec::new();
        for region in &self.regions {
            if region.contains_body(line) && !self.collapsed.contains(&region.start) {
                headers.push(region.start);
            }
        }
        headers.sort_unstable();
        headers.dedup();
        if headers.len() > depth {
            headers.drain(..headers.len() - depth);
        }
        headers
    }

    pub fn gutter(&self, line: u32) -> FoldGutter {
        match self.region_at_start(line) {
            Some(_) if self.collapsed.contains(&line) => FoldGutter::Collapsed,
            Some(_) => FoldGutter::Open,
            None => FoldGutter::None,
        }
    }

    pub fn toggle(&mut self, line: u32) {
        if let Some(r) = self.enclosing(line)
            && !self.collapsed.remove(&r.start)
        {
            self.collapsed.insert(r.start);
        }
    }

    pub fn toggle_header(&mut self, start: u32) {
        if self.region_at_start(start).is_none() {
            return;
        }
        if !self.collapsed.remove(&start) {
            self.collapsed.insert(start);
        }
    }

    pub fn open(&mut self, line: u32) {
        if let Some(r) = self.enclosing(line) {
            self.collapsed.remove(&r.start);
        }
    }

    pub fn close(&mut self, line: u32) {
        if let Some(r) = self.enclosing(line) {
            self.collapsed.insert(r.start);
        }
    }

    pub fn toggle_recursive(&mut self, line: u32) {
        let Some(top) = self.enclosing(line) else {
            return;
        };
        let want_collapse = !self.collapsed.contains(&top.start);
        let inner: Vec<u32> = self
            .regions
            .iter()
            .filter(|r| top.contains(r.start) && top.contains(r.end))
            .map(|r| r.start)
            .collect();
        for s in inner {
            if want_collapse {
                self.collapsed.insert(s);
            } else {
                self.collapsed.remove(&s);
            }
        }
    }

    pub fn fold_all(&mut self) {
        self.collapsed = self.regions.iter().map(|r| r.start).collect();
    }

    pub fn unfold_all(&mut self) {
        self.collapsed.clear();
    }

    pub fn hiding_header(&self, line: u32) -> Option<u32> {
        self.regions
            .iter()
            .filter(|r| self.collapsed.contains(&r.start) && r.contains_body(line))
            .min_by_key(|r| r.end - r.start)
            .map(|r| r.start)
    }

    pub fn reveal(&mut self, line: u32) {
        let open: Vec<u32> = self
            .collapsed
            .iter()
            .copied()
            .filter(|s| {
                self.region_at_start(*s)
                    .is_some_and(|r| r.contains_body(line))
            })
            .collect();
        for s in open {
            self.collapsed.remove(&s);
        }
    }

    pub fn shift(&mut self, at_line: u32, delta: i64) {
        if delta == 0 {
            return;
        }
        self.collapsed = self
            .collapsed
            .iter()
            .map(|&s| {
                if s >= at_line {
                    (s as i64 + delta).max(0) as u32
                } else {
                    s
                }
            })
            .collect();
    }

    pub fn reconcile(&mut self) {
        let starts: HashSet<u32> = self.regions.iter().map(|r| r.start).collect();
        self.collapsed.retain(|s| starts.contains(s));
    }

    pub fn view(&self, total: u32) -> FoldView {
        let mut spans: Vec<(u32, u32)> = self
            .collapsed
            .iter()
            .filter_map(|s| self.region_at_start(*s))
            .map(|r| (r.start + 1, r.end.min(total.saturating_sub(1))))
            .filter(|(a, b)| a <= b)
            .collect();
        spans.sort_unstable();
        let mut hidden: Vec<(u32, u32)> = Vec::new();
        for (a, b) in spans {
            match hidden.last_mut() {
                Some(last) if a <= last.1 + 1 => last.1 = last.1.max(b),
                _ => hidden.push((a, b)),
            }
        }
        FoldView { hidden, total }
    }
}

#[derive(Default, Clone, Debug)]
pub struct FoldView {
    hidden: Vec<(u32, u32)>,
    total: u32,
}

impl FoldView {
    pub fn is_hidden(&self, line: u32) -> bool {
        self.hidden.iter().any(|(a, b)| line >= *a && line <= *b)
    }

    pub fn hidden_before(&self, line: u32) -> u32 {
        let mut n = 0;
        for (a, b) in &self.hidden {
            if *b < line {
                n += b - a + 1;
            } else if *a < line {
                n += line - a;
            }
        }
        n
    }

    pub fn buffer_to_row(&self, line: u32) -> u32 {
        line - self.hidden_before(line)
    }

    pub fn visible_count(&self) -> u32 {
        let hidden: u32 = self.hidden.iter().map(|(a, b)| b - a + 1).sum();
        self.total.saturating_sub(hidden).max(1)
    }

    pub fn next_visible(&self, line: u32) -> u32 {
        let mut l = line;
        while l + 1 < self.total && self.is_hidden(l) {
            l += 1;
        }
        l
    }

    pub fn step_rows(&self, line: u32, delta: i64) -> u32 {
        if self.total == 0 {
            return 0;
        }
        let last = self.total - 1;
        let mut l = line as i64;
        let dir = delta.signum();
        let mut steps = delta.abs();
        while steps > 0 {
            let mut n = l + dir;
            while n >= 0 && (n as u32) <= last && self.is_hidden(n as u32) {
                n += dir;
            }
            if n < 0 || (n as u32) > last {
                break;
            }
            l = n;
            steps -= 1;
        }
        (l.max(0) as u32).min(last)
    }

    pub fn lines_for_window(&self, first_row: u32, rows: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut count = 0u32;
        let mut l = 0u32;
        while l < self.total && (out.len() as u32) < rows {
            if !self.is_hidden(l) {
                if count >= first_row {
                    out.push(l);
                }
                count += 1;
            }
            l += 1;
        }
        out
    }
}

fn indent_width(line: &str) -> Option<usize> {
    let mut w = 0;
    for c in line.chars() {
        match c {
            ' ' => w += 1,
            '\t' => w += 4,
            _ => return Some(w),
        }
    }
    None
}

pub struct IndentGuides<'a> {
    rope: &'a Rope,
}

impl<'a> IndentGuides<'a> {
    const COLUMNS: usize = 4;

    pub fn of(rope: &'a Rope) -> Self {
        Self { rope }
    }

    pub fn levels(&self, line: usize) -> u16 {
        let total = self.rope.len_lines();
        let mut probe = line;
        while probe < total {
            if let Some(width) = self.width(probe) {
                return (width / Self::COLUMNS) as u16;
            }
            probe += 1;
        }
        0
    }

    fn width(&self, line: usize) -> Option<usize> {
        let text: String = self
            .rope
            .line(line)
            .chars()
            .filter(|c| *c != '\n' && *c != '\r')
            .collect();
        indent_width(&text)
    }
}

pub fn indent_regions(rope: &Rope) -> Vec<FoldRegion> {
    let total = rope.len_lines();
    let indents: Vec<Option<usize>> = (0..total)
        .map(|i| {
            let s: String = rope
                .line(i)
                .chars()
                .filter(|c| *c != '\n' && *c != '\r')
                .collect();
            indent_width(&s)
        })
        .collect();
    let mut regions = Vec::new();
    for i in 0..total {
        let Some(cur) = indents[i] else { continue };
        let mut j = i + 1;
        let mut last = i;
        while j < total {
            match indents[j] {
                None => j += 1,
                Some(d) if d > cur => {
                    last = j;
                    j += 1;
                }
                Some(_) => break,
            }
        }
        if last > i {
            regions.push(FoldRegion {
                start: i as u32,
                end: last as u32,
            });
        }
    }
    regions
}

#[cfg(test)]
mod indent_tests {
    use super::*;

    #[test]
    fn folds_indented_block() {
        let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
        let regs = indent_regions(&r);
        assert!(regs.contains(&FoldRegion { start: 0, end: 2 }));
    }

    #[test]
    fn excludes_trailing_blanks() {
        let r = Rope::from_str("a:\n  b\n\n\nc\n");
        let regs = indent_regions(&r);
        assert_eq!(regs, vec![FoldRegion { start: 0, end: 1 }]);
    }

    #[test]
    fn nests_deeper_blocks() {
        let r = Rope::from_str("a:\n  b:\n    c\n  d\ne\n");
        let regs = indent_regions(&r);
        assert!(regs.contains(&FoldRegion { start: 0, end: 3 }));
        assert!(regs.contains(&FoldRegion { start: 1, end: 2 }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_names_the_enclosing_headers_outermost_first() {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 0, end: 20 },
            FoldRegion { start: 4, end: 12 },
            FoldRegion { start: 6, end: 8 },
            FoldRegion { start: 30, end: 40 },
        ]);

        assert_eq!(s.sticky(7, 5), vec![0, 4, 6]);
        assert_eq!(
            s.sticky(4, 5),
            vec![0],
            "a header is not pinned above itself, it is already the top line"
        );
        assert_eq!(s.sticky(25, 5), Vec::<u32>::new());
    }

    #[test]
    fn sticky_keeps_the_innermost_headers_when_nesting_runs_deep() {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 0, end: 50 },
            FoldRegion { start: 1, end: 40 },
            FoldRegion { start: 2, end: 30 },
        ]);

        assert_eq!(
            s.sticky(10, 2),
            vec![1, 2],
            "the closest scopes are the ones worth the space"
        );
    }

    #[test]
    fn a_collapsed_scope_is_not_pinned() {
        let mut s = FoldState::default();
        s.set_regions(vec![FoldRegion { start: 0, end: 20 }]);
        s.toggle(0);

        assert_eq!(s.sticky(5, 5), Vec::<u32>::new());
    }

    #[test]
    fn a_guide_counts_indent_levels_and_carries_them_through_a_blank_line() {
        let rope = Rope::from_str("fn a() {\n    let x = 1;\n\n        deep();\n}\n");
        let guides = IndentGuides::of(&rope);

        assert_eq!(guides.levels(0), 0);
        assert_eq!(guides.levels(1), 1);
        assert_eq!(
            guides.levels(2),
            2,
            "a blank line takes the next non-blank line's depth so the guide does not break"
        );
        assert_eq!(guides.levels(3), 2);
    }

    #[test]
    fn trailing_blank_lines_have_no_guide_to_carry() {
        let rope = Rope::from_str("fn a() {\n    x;\n}\n\n\n");
        let guides = IndentGuides::of(&rope);

        assert_eq!(guides.levels(3), 0);
        assert_eq!(guides.levels(4), 0);
    }

    fn state() -> FoldState {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 1, end: 4 },
            FoldRegion { start: 6, end: 8 },
        ]);
        s
    }

    #[test]
    fn gutter_marks_headers() {
        let mut s = state();
        assert_eq!(s.gutter(1), FoldGutter::Open);
        assert_eq!(s.gutter(2), FoldGutter::None);
        s.close(1);
        assert_eq!(s.gutter(1), FoldGutter::Collapsed);
    }

    #[test]
    fn clicking_a_header_toggles_that_header_not_an_overlapping_region() {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 3, end: 9 },
            FoldRegion { start: 4, end: 5 },
        ]);

        s.toggle_header(4);

        assert!(s.collapsed.contains(&4));
        assert!(!s.collapsed.contains(&3));
        assert_eq!(s.gutter(4), FoldGutter::Collapsed);
    }

    #[test]
    fn clicking_a_line_that_starts_no_region_folds_nothing() {
        let mut s = state();
        s.toggle_header(2);
        assert!(s.collapsed.is_empty());
    }

    #[test]
    fn view_hides_body_only() {
        let mut s = state();
        s.close(1);
        let v = s.view(10);
        assert!(!v.is_hidden(1));
        assert!(v.is_hidden(2) && v.is_hidden(4));
        assert!(!v.is_hidden(5));
        assert_eq!(v.visible_count(), 10 - 3);
        assert_eq!(v.buffer_to_row(5), 5 - 3);
    }

    #[test]
    fn step_rows_skips_hidden() {
        let mut s = state();
        s.close(1);
        let v = s.view(10);
        assert_eq!(v.step_rows(1, 1), 5);
        assert_eq!(v.step_rows(5, -1), 1);
    }

    #[test]
    fn window_returns_visible_lines() {
        let mut s = state();
        s.close(1);
        let v = s.view(10);
        assert_eq!(v.lines_for_window(0, 4), vec![0, 1, 5, 6]);
    }

    #[test]
    fn toggle_recursive_folds_nested() {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 0, end: 9 },
            FoldRegion { start: 2, end: 4 },
        ]);
        s.toggle_recursive(0);
        assert!(s.collapsed.contains(&0) && s.collapsed.contains(&2));
        s.toggle_recursive(0);
        assert!(s.collapsed.is_empty());
    }

    #[test]
    fn reveal_opens_enclosing() {
        let mut s = state();
        s.close(1);
        s.reveal(3);
        assert!(!s.collapsed.contains(&1));
    }

    #[test]
    fn hiding_header_returns_innermost() {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 0, end: 9 },
            FoldRegion { start: 2, end: 5 },
        ]);
        s.close(0);
        s.close(2);
        assert_eq!(s.hiding_header(3), Some(2));
        assert_eq!(s.hiding_header(7), Some(0));
        assert_eq!(s.hiding_header(0), None);
    }

    #[test]
    fn shift_moves_collapsed_starts() {
        let mut s = state();
        s.close(6);
        s.shift(2, 3);
        assert!(s.collapsed.contains(&9));
    }

    #[test]
    fn reconcile_drops_stale() {
        let mut s = state();
        s.close(6);
        s.set_regions(vec![FoldRegion { start: 1, end: 4 }]);
        assert!(!s.collapsed.contains(&6));
    }
}
