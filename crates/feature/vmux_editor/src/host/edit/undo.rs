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

#[derive(bevy::prelude::Component)]
pub struct History {
    nodes: Vec<Node>,
    current: usize,
    next_seq: u64,
}

pub struct Restored {
    pub rope: Rope,
    pub selections: Vec<Selection>,
    pub rev: u64,
}

impl History {
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

    use crate::edit::{Document, EditCommand, EditMode};
    use crate::host::editing::EditRequest;
    use bevy::prelude::*;

    struct Doc {
        app: App,
        entity: Entity,
    }

    impl Doc {
        fn new(text: &str) -> Self {
            let mut app = App::new();
            app.add_plugins(crate::host::editing::document::DocumentPlugin);
            let entity = app
                .world_mut()
                .spawn(Document::new(
                    std::path::PathBuf::from("history.txt"),
                    "Plain Text".into(),
                    text,
                    EditMode::Insert,
                ))
                .id();
            Self { app, entity }
        }

        fn text(&self) -> String {
            self.app
                .world()
                .get::<crate::edit::buffer::TextBuffer>(self.entity)
                .unwrap()
                .text()
        }

        fn edit(&mut self, next: &str) {
            self.apply(EditCommand::ReplaceText(next.into()));
        }

        fn undo(&mut self) -> bool {
            self.apply(EditCommand::Undo)
        }

        fn redo(&mut self) -> bool {
            self.apply(EditCommand::Redo)
        }

        fn apply(&mut self, command: EditCommand) -> bool {
            let before = self.text();
            EditRequest::new(self.entity, vec![command]).emit_world(self.app.world_mut());
            self.text() != before
        }

        fn time(&mut self, forward: bool, count: usize) -> bool {
            self.apply(EditCommand::UndoTime { forward, count })
        }
    }

    #[test]
    fn undo_and_redo_walk_a_linear_history() {
        let mut doc = Doc::new("a");
        doc.edit("ab");
        doc.edit("abc");

        assert!(doc.undo());
        assert_eq!(doc.text(), "ab");
        assert!(doc.undo());
        assert_eq!(doc.text(), "a");
        assert!(!doc.undo(), "the root has no parent");

        assert!(doc.redo());
        assert_eq!(doc.text(), "ab");
    }

    #[test]
    fn editing_after_an_undo_branches_rather_than_discarding() {
        let mut doc = Doc::new("");
        doc.edit("first");
        doc.undo();
        doc.edit("second");

        doc.undo();
        assert_eq!(doc.text(), "");
        doc.redo();
        assert_eq!(doc.text(), "second", "redo follows the newest branch");
    }

    #[test]
    fn time_travel_reaches_an_abandoned_branch() {
        let mut doc = Doc::new("");
        doc.edit("first");
        doc.undo();
        doc.edit("second");

        assert!(doc.time(false, 1));
        assert_eq!(doc.text(), "first", "g- reaches what redo cannot");
        assert!(doc.time(true, 1));
        assert_eq!(doc.text(), "second");
    }

    #[test]
    fn time_travel_takes_a_count_and_stops_at_the_ends() {
        let mut doc = Doc::new("0");
        doc.edit("1");
        doc.edit("2");
        assert!(doc.time(false, 2));
        assert_eq!(doc.text(), "0");
        assert!(!doc.time(false, 1), "already at the oldest state");
    }
}
