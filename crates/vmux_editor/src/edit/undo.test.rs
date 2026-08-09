use super::*;

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}
fn sel() -> Vec<Selection> {
    vec![Selection::caret(0)]
}

/// Mirrors how the editor drives the tree: `push` happens before the buffer changes, and the
/// live text is handed back in on every navigation.
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
