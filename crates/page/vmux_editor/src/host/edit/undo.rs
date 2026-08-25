use ropey::Rope;

use crate::edit::command::Selection;

#[derive(Clone)]
struct Node {
    rope: Rope,
    selections: Vec<Selection>,
    rev: u64,
    seq: u64,
    parent: Option<usize>,
    children: Vec<usize>,
}

pub struct UndoTree {
    nodes: Vec<Node>,
    current: usize,
    next_seq: u64,
}

pub struct Restored {
    pub rope: Rope,
    pub selections: Vec<Selection>,
    pub rev: u64,
}

impl UndoTree {
    pub fn new(rope: Rope, selections: Vec<Selection>, rev: u64) -> Self {
        Self {
            nodes: vec![Node {
                rope,
                selections,
                rev,
                seq: 0,
                parent: None,
                children: Vec::new(),
            }],
            current: 0,
            next_seq: 1,
        }
    }

    fn sync(&mut self, rope: &Rope, selections: &[Selection], rev: u64) {
        let node = &mut self.nodes[self.current];
        node.rope = rope.clone();
        node.selections = selections.to_vec();
        node.rev = rev;
    }

    pub fn push(&mut self, rope: &Rope, selections: &[Selection], rev: u64) {
        self.sync(rope, selections, rev);
        let seq = self.next_seq;
        self.next_seq += 1;
        let node = Node {
            rope: rope.clone(),
            selections: selections.to_vec(),
            rev,
            seq,
            parent: Some(self.current),
            children: Vec::new(),
        };
        self.nodes.push(node);
        let index = self.nodes.len() - 1;
        self.nodes[self.current].children.push(index);
        self.current = index;
    }

    fn restore(&mut self, index: usize) -> Restored {
        self.current = index;
        let node = &self.nodes[index];
        Restored {
            rope: node.rope.clone(),
            selections: node.selections.clone(),
            rev: node.rev,
        }
    }

    pub fn undo(&mut self, rope: &Rope, selections: &[Selection], rev: u64) -> Option<Restored> {
        self.sync(rope, selections, rev);
        let parent = self.nodes[self.current].parent?;
        Some(self.restore(parent))
    }

    pub fn redo(&mut self, rope: &Rope, selections: &[Selection], rev: u64) -> Option<Restored> {
        self.sync(rope, selections, rev);
        let child = *self.nodes[self.current].children.last()?;
        Some(self.restore(child))
    }

    pub fn step_time(
        &mut self,
        rope: &Rope,
        selections: &[Selection],
        rev: u64,
        forward: bool,
        count: usize,
    ) -> Option<Restored> {
        self.sync(rope, selections, rev);
        let mut index = self.current;
        for _ in 0..count.max(1) {
            let seq = self.nodes[index].seq;
            let next = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| if forward { n.seq > seq } else { n.seq < seq })
                .min_by_key(|(_, n)| if forward { n.seq } else { u64::MAX - n.seq })
                .map(|(i, _)| i);
            match next {
                Some(next) => index = next,
                None => break,
            }
        }
        (index != self.current).then(|| self.restore(index))
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(text: &str) -> Rope {
        Rope::from_str(text)
    }

    fn sel() -> Vec<Selection> {
        vec![Selection::caret(0)]
    }

    struct Doc {
        tree: UndoTree,
        text: String,
    }

    impl Doc {
        fn new(text: &str) -> Self {
            Self {
                tree: UndoTree::new(rope(text), sel(), 0),
                text: text.to_string(),
            }
        }

        fn edit(&mut self, next: &str) {
            self.tree.push(&rope(&self.text), &sel(), 0);
            self.text = next.to_string();
        }

        fn undo(&mut self) -> bool {
            self.apply(UndoTree::undo)
        }

        fn redo(&mut self) -> bool {
            self.apply(UndoTree::redo)
        }

        fn apply(
            &mut self,
            f: fn(&mut UndoTree, &Rope, &[Selection], u64) -> Option<Restored>,
        ) -> bool {
            match f(&mut self.tree, &rope(&self.text), &sel(), 0) {
                Some(state) => {
                    self.text = state.rope.to_string();
                    true
                }
                None => false,
            }
        }

        fn time(&mut self, forward: bool, count: usize) -> bool {
            match self
                .tree
                .step_time(&rope(&self.text), &sel(), 0, forward, count)
            {
                Some(state) => {
                    self.text = state.rope.to_string();
                    true
                }
                None => false,
            }
        }
    }

    #[test]
    fn undo_and_redo_walk_a_linear_history() {
        let mut doc = Doc::new("a");
        doc.edit("ab");
        doc.edit("abc");

        assert!(doc.undo());
        assert_eq!(doc.text, "ab");
        assert!(doc.undo());
        assert_eq!(doc.text, "a");
        assert!(!doc.undo(), "the root has no parent");

        assert!(doc.redo());
        assert_eq!(doc.text, "ab");
    }

    #[test]
    fn editing_after_an_undo_branches_rather_than_discarding() {
        let mut doc = Doc::new("");
        doc.edit("first");
        doc.undo();
        doc.edit("second");

        doc.undo();
        assert_eq!(doc.text, "");
        doc.redo();
        assert_eq!(doc.text, "second", "redo follows the newest branch");
    }

    #[test]
    fn time_travel_reaches_an_abandoned_branch() {
        let mut doc = Doc::new("");
        doc.edit("first");
        doc.undo();
        doc.edit("second");

        assert!(doc.time(false, 1));
        assert_eq!(doc.text, "first", "g- reaches what redo cannot");
        assert!(doc.time(true, 1));
        assert_eq!(doc.text, "second");
    }

    #[test]
    fn time_travel_takes_a_count_and_stops_at_the_ends() {
        let mut doc = Doc::new("0");
        doc.edit("1");
        doc.edit("2");
        assert!(doc.time(false, 2));
        assert_eq!(doc.text, "0");
        assert!(!doc.time(false, 1), "already at the oldest state");
    }
}
