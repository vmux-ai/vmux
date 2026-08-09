use ropey::Rope;

use crate::edit::command::Selection;

/// One document state in the undo tree.
#[derive(Clone)]
struct Node {
    rope: Rope,
    selections: Vec<Selection>,
    rev: u64,
    /// Creation order, which is what `g-` and `g+` walk.
    seq: u64,
    parent: Option<usize>,
    children: Vec<usize>,
}

/// A branching undo history.
///
/// Nodes hold the document *after* each change, and the node marked current is kept in sync with
/// the live buffer lazily — the buffer is the source of truth until something navigates away.
/// Undoing then making a new edit branches instead of discarding the redo path, which is what
/// makes `g-`/`g+` able to reach states `Ctrl-r` cannot.
pub struct UndoTree {
    nodes: Vec<Node>,
    current: usize,
    next_seq: u64,
}

/// The state to restore after navigating the tree.
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

    /// Fold the live buffer back into the current node before reading history.
    fn sync(&mut self, rope: &Rope, selections: &[Selection], rev: u64) {
        let node = &mut self.nodes[self.current];
        node.rope = rope.clone();
        node.selections = selections.to_vec();
        node.rev = rev;
    }

    /// Open a new state as a child of the current one.
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
        // The newest branch is the one `Ctrl-r` follows.
        let child = *self.nodes[self.current].children.last()?;
        Some(self.restore(child))
    }

    /// Move to the state created just before or just after the current one, ignoring branches.
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
#[path = "undo.test.rs"]
mod tests;
