//! The document a render describes, kept in Rust instead of a browser.
//!
//! [`MutationState`](dioxus_interpreter_js::MutationState) turns a render into the byte batch a
//! webview applies; this turns the same render into a tree that can be asked questions. Both are
//! [`WriteMutations`] sinks, so what a test sees is what the page would have been told to build —
//! not a second rendering path that could agree with the components while disagreeing with the
//! document.
//!
//! The mutations are a stack machine, and that is the whole of the implementation. Nodes are
//! created onto a stack; `append_children`, `replace_*` and `insert_*` pop from it; and
//! `assign_node_id` walks a path from whatever is on top to give a node the [`ElementId`] an event
//! will later name it by. A node reachable here but holding no `ElementId` is one dioxus never
//! needed to address, and it cannot be the target of an event.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use dioxus_core::{
    AttributeValue, ElementId, Template, TemplateAttribute, TemplateNode, WriteMutations,
};

use crate::selector::Selector;

/// The rendered document, addressable by [`Selector`] and by [`ElementId`].
#[derive(Debug, Default)]
pub struct ShadowTree {
    nodes: Vec<Node>,
    ids: HashMap<ElementId, Key>,
    stack: Vec<Key>,
}

/// A node's place in [`ShadowTree::nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key(usize);

/// The document root, which dioxus addresses without ever creating.
const ROOT: Key = Key(0);

#[derive(Debug)]
struct Node {
    parent: Option<Key>,
    children: Vec<Key>,
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    Element {
        tag: &'static str,
        attributes: BTreeMap<String, String>,
        listeners: BTreeSet<&'static str>,
    },
    Text(String),
    /// A slot dioxus holds open so it can put something there later.
    Placeholder,
}

impl ShadowTree {
    /// The `ElementId` of the first element in document order that `selector` names.
    ///
    /// `None` covers both "nothing matched" and "what matched was never assigned an id"; a caller
    /// that must tell those apart asks [`ShadowTree::exists`] as well.
    pub fn find(&self, selector: &Selector) -> Option<ElementId> {
        let key = self.locate(selector)?;
        self.id_of(key)
    }

    /// Whether `selector` names anything at all, addressable or not.
    pub fn exists(&self, selector: &Selector) -> bool {
        self.locate(selector).is_some()
    }

    /// How many elements `selector` names, which is how a test asserts a list's length.
    pub fn count(&self, selector: &Selector) -> usize {
        let mut found = 0;
        self.walk(ROOT, &mut |tree, key| {
            if tree.matches(key, selector) {
                found += 1;
            }
        });

        found
    }

    /// Every character of text under the first element `selector` names.
    pub fn text(&self, selector: &Selector) -> Option<String> {
        let key = self.locate(selector)?;
        let mut text = String::new();
        self.walk(key, &mut |tree, key| {
            if let Kind::Text(value) = &tree.nodes[key.0].kind {
                text.push_str(value);
            }
        });

        Some(text)
    }

    /// One attribute of the first element `selector` names.
    pub fn attribute(&self, selector: &Selector, name: &str) -> Option<String> {
        let key = self.locate(selector)?;
        self.attribute_of(key, name)
    }

    /// The events the first element `selector` names is listening for.
    ///
    /// An element with no listener for an event will not react to it, so a harness that dispatches
    /// one blindly asserts nothing. This is what lets it refuse instead.
    pub fn listeners(&self, selector: &Selector) -> BTreeSet<&'static str> {
        let Some(key) = self.locate(selector) else {
            return BTreeSet::new();
        };
        let Kind::Element { listeners, .. } = &self.nodes[key.0].kind else {
            return BTreeSet::new();
        };

        listeners.clone()
    }

    /// Whether an event dispatched at `id` would reach a handler, here or on the way up.
    ///
    /// Dispatching at an element that listens for nothing is silently a no-op, so a harness that
    /// does not ask this can assert on a click that never happened.
    pub fn has_listener(&self, id: ElementId, event: &str) -> bool {
        let Some(key) = self.ids.get(&id).copied() else {
            return false;
        };

        let mut at = Some(key);
        while let Some(key) = at {
            if let Kind::Element { listeners, .. } = &self.nodes[key.0].kind
                && listeners.contains(event)
            {
                return true;
            }
            at = self.nodes[key.0].parent;
        }

        false
    }

    /// The whole document as indented text, for when a failing test has to say what it did see.
    pub fn outline(&self) -> String {
        let mut out = String::new();
        self.describe(ROOT, 0, &mut out);

        out
    }

    fn locate(&self, selector: &Selector) -> Option<Key> {
        let mut found = None;
        self.walk(ROOT, &mut |tree, key| {
            if found.is_none() && tree.matches(key, selector) {
                found = Some(key);
            }
        });

        found
    }

    fn matches(&self, key: Key, selector: &Selector) -> bool {
        let Kind::Element { tag, .. } = &self.nodes[key.0].kind else {
            return false;
        };
        if key == ROOT {
            return false;
        }

        selector.matches(tag, |name| self.attribute_of(key, name))
    }

    fn attribute_of(&self, key: Key, name: &str) -> Option<String> {
        let Kind::Element { attributes, .. } = &self.nodes[key.0].kind else {
            return None;
        };

        attributes.get(name).cloned()
    }

    fn id_of(&self, key: Key) -> Option<ElementId> {
        for (id, assigned) in &self.ids {
            if *assigned == key {
                return Some(*id);
            }
        }

        None
    }

    fn walk(&self, from: Key, visit: &mut impl FnMut(&Self, Key)) {
        visit(self, from);
        for child in &self.nodes[from.0].children {
            self.walk(*child, visit);
        }
    }

    fn describe(&self, key: Key, depth: usize, out: &mut String) {
        let pad = "  ".repeat(depth);
        match &self.nodes[key.0].kind {
            Kind::Element {
                tag, attributes, ..
            } => {
                out.push_str(&format!("{pad}<{tag}"));
                for (name, value) in attributes {
                    out.push_str(&format!(" {name}={value:?}"));
                }
                out.push_str(">\n");
            }
            Kind::Text(value) => out.push_str(&format!("{pad}{value:?}\n")),
            Kind::Placeholder => out.push_str(&format!("{pad}<!--slot-->\n")),
        }

        for child in &self.nodes[key.0].children {
            self.describe(*child, depth + 1, out);
        }
    }

    fn push_node(&mut self, kind: Kind) -> Key {
        let key = Key(self.nodes.len());
        self.nodes.push(Node {
            parent: None,
            children: Vec::new(),
            kind,
        });

        key
    }

    /// The root exists before any mutation arrives, because `append_children` addresses it.
    fn rooted(&mut self) {
        if self.nodes.is_empty() {
            let root = self.push_node(Kind::Element {
                tag: "#document",
                attributes: BTreeMap::new(),
                listeners: BTreeSet::new(),
            });
            self.ids.insert(ElementId(0), root);
        }
    }

    fn instantiate(&mut self, node: &TemplateNode) -> Key {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut attributes = BTreeMap::new();
                for attr in *attrs {
                    if let TemplateAttribute::Static { name, value, .. } = attr {
                        attributes.insert((*name).to_string(), (*value).to_string());
                    }
                }

                let key = self.push_node(Kind::Element {
                    tag,
                    attributes,
                    listeners: BTreeSet::new(),
                });
                for child in *children {
                    let child = self.instantiate(child);
                    self.adopt(key, child);
                }

                key
            }
            TemplateNode::Text { text } => self.push_node(Kind::Text((*text).to_string())),
            TemplateNode::Dynamic { .. } => self.push_node(Kind::Placeholder),
        }
    }

    fn adopt(&mut self, parent: Key, child: Key) {
        self.detach(child);
        self.nodes[child.0].parent = Some(parent);
        self.nodes[parent.0].children.push(child);
    }

    fn detach(&mut self, child: Key) {
        let Some(parent) = self.nodes[child.0].parent.take() else {
            return;
        };
        self.nodes[parent.0].children.retain(|key| *key != child);
    }

    /// Where a node sits among its siblings, which every insert and replace is expressed against.
    fn position(&self, key: Key) -> Option<(Key, usize)> {
        let parent = self.nodes[key.0].parent?;
        let at = self.nodes[parent.0]
            .children
            .iter()
            .position(|child| *child == key)?;

        Some((parent, at))
    }

    fn insert(&mut self, parent: Key, at: usize, taken: Vec<Key>) {
        for (offset, child) in taken.into_iter().enumerate() {
            self.detach(child);
            self.nodes[child.0].parent = Some(parent);
            self.nodes[parent.0].children.insert(at + offset, child);
        }
    }

    fn take(&mut self, m: usize) -> Vec<Key> {
        let at = self.stack.len().saturating_sub(m);

        self.stack.split_off(at)
    }

    /// The node `path` names, walked from whatever is on top of the stack.
    fn child_at(&self, path: &[u8]) -> Option<Key> {
        let mut key = *self.stack.last()?;
        for step in path {
            key = *self.nodes[key.0].children.get(*step as usize)?;
        }

        Some(key)
    }
}

impl WriteMutations for ShadowTree {
    fn append_children(&mut self, id: ElementId, m: usize) {
        self.rooted();
        let taken = self.take(m);
        let Some(parent) = self.ids.get(&id).copied() else {
            return;
        };
        for child in taken {
            self.adopt(parent, child);
        }
    }

    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId) {
        self.rooted();
        let Some(key) = self.child_at(path) else {
            return;
        };
        self.ids.insert(id, key);
    }

    fn create_placeholder(&mut self, id: ElementId) {
        self.rooted();
        let key = self.push_node(Kind::Placeholder);
        self.ids.insert(id, key);
        self.stack.push(key);
    }

    fn create_text_node(&mut self, value: &str, id: ElementId) {
        self.rooted();
        let key = self.push_node(Kind::Text(value.to_string()));
        self.ids.insert(id, key);
        self.stack.push(key);
    }

    fn load_template(&mut self, template: Template, index: usize, id: ElementId) {
        self.rooted();
        let Some(root) = template.roots.get(index) else {
            return;
        };
        let key = self.instantiate(root);
        self.ids.insert(id, key);
        self.stack.push(key);
    }

    fn replace_node_with(&mut self, id: ElementId, m: usize) {
        self.rooted();
        let taken = self.take(m);
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        let Some((parent, at)) = self.position(key) else {
            return;
        };
        self.detach(key);
        self.insert(parent, at, taken);
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        self.rooted();
        let taken = self.take(m);
        let Some(key) = self.child_at(path) else {
            return;
        };
        let Some((parent, at)) = self.position(key) else {
            return;
        };
        self.detach(key);
        self.insert(parent, at, taken);
    }

    fn insert_nodes_after(&mut self, id: ElementId, m: usize) {
        self.rooted();
        let taken = self.take(m);
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        let Some((parent, at)) = self.position(key) else {
            return;
        };
        self.insert(parent, at + 1, taken);
    }

    fn insert_nodes_before(&mut self, id: ElementId, m: usize) {
        self.rooted();
        let taken = self.take(m);
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        let Some((parent, at)) = self.position(key) else {
            return;
        };
        self.insert(parent, at, taken);
    }

    fn set_attribute(
        &mut self,
        name: &'static str,
        _ns: Option<&'static str>,
        value: &AttributeValue,
        id: ElementId,
    ) {
        self.rooted();
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        let Kind::Element { attributes, .. } = &mut self.nodes[key.0].kind else {
            return;
        };

        match value {
            AttributeValue::Text(text) => attributes.insert(name.to_string(), text.clone()),
            AttributeValue::Float(number) => {
                attributes.insert(name.to_string(), number.to_string())
            }
            AttributeValue::Int(number) => attributes.insert(name.to_string(), number.to_string()),
            AttributeValue::Bool(flag) => attributes.insert(name.to_string(), flag.to_string()),
            // A `None` value is dioxus asking for the attribute to be taken off, and the other two
            // never reach a document: a listener arrives through `create_event_listener`, and an
            // `Any` is a renderer-private payload with no textual form.
            _ => attributes.remove(name),
        };
    }

    fn set_node_text(&mut self, value: &str, id: ElementId) {
        self.rooted();
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        match &mut self.nodes[key.0].kind {
            Kind::Text(text) => *text = value.to_string(),
            kind => *kind = Kind::Text(value.to_string()),
        }
    }

    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.rooted();
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        if let Kind::Element { listeners, .. } = &mut self.nodes[key.0].kind {
            listeners.insert(name);
        }
    }

    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.rooted();
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        if let Kind::Element { listeners, .. } = &mut self.nodes[key.0].kind {
            listeners.remove(name);
        }
    }

    fn remove_node(&mut self, id: ElementId) {
        self.rooted();
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        self.detach(key);
    }

    fn push_root(&mut self, id: ElementId) {
        self.rooted();
        let Some(key) = self.ids.get(&id).copied() else {
            return;
        };
        self.stack.push(key);
    }
}
