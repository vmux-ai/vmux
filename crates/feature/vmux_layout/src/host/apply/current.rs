use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::{Active, PageMetadata};
use vmux_history::LastActivatedAt;

use crate::CloseRequiresConfirmation;
use crate::pane::{Pane, PaneSize, PaneSplit, Zoomed};
use crate::presentation::Retiring;
use crate::protocol::NodeKind;
use crate::space::Space;
use crate::stack::Stack;
use crate::tab::Tab;

#[derive(SystemParam)]
pub(super) struct CurrentLayout<'w, 's> {
    active: Query<'w, 's, Entity, (With<Space>, With<Active>)>,
    spaces: Query<'w, 's, (), With<Space>>,
    focused: crate::stack::FocusedStack<'w, 's>,
    host_windows: Query<'w, 's, &'static bevy_cef::prelude::HostWindow>,
    surfaces: crate::hosted_page::StackSurface<'w, 's>,
    browsers: Query<'w, 's, (), With<crate::Browser>>,
    tabs: Query<'w, 's, Entity, With<Tab>>,
    nodes: Query<
        'w,
        's,
        (
            Option<&'static Children>,
            Option<&'static ChildOf>,
            Option<&'static Tab>,
            Option<&'static PaneSplit>,
            Has<Pane>,
            Has<Stack>,
            Has<Retiring>,
            Has<CloseRequiresConfirmation>,
            Option<&'static PageMetadata>,
            Option<&'static PaneSize>,
            Option<&'static LastActivatedAt>,
            Option<&'static Zoomed>,
            Option<&'static crate::page_slot::Current>,
        ),
    >,
}

impl CurrentLayout<'_, '_> {
    pub(super) fn scope(&self) -> Result<Option<Entity>, String> {
        if let Some(tab) = self.focused.tab {
            if !self.tabs.contains(tab) || self.retiring(tab) {
                return Err("update_layout: focused owner retired".into());
            }
            let mut current = tab;
            while let Some(parent) = self.nodes.get(current).ok().and_then(|node| node.1) {
                current = parent.parent();
                if self.spaces.contains(current) {
                    return self.validate_scope(Some(current));
                }
            }
            let parent = self
                .nodes
                .get(tab)
                .ok()
                .and_then(|node| node.1)
                .map(ChildOf::parent);
            return self.validate_scope(parent);
        }
        let mut active = self.active.iter();
        if let Some(scope) = active.next() {
            if active.next().is_some() {
                return Err("update_layout: ambiguous active Space".into());
            }
            return self.validate_scope(Some(scope));
        }
        let mut scope = None;
        let mut found = false;
        for tab in &self.tabs {
            let parent = self
                .nodes
                .get(tab)
                .ok()
                .and_then(|node| node.1)
                .map(ChildOf::parent);
            if found && parent != scope {
                return Err("update_layout: ambiguous layout scope".into());
            }
            scope = parent;
            found = true;
        }
        self.validate_scope(scope)
    }

    fn validate_scope(&self, scope: Option<Entity>) -> Result<Option<Entity>, String> {
        if let Some(scope) = scope
            && let Some(window) = self.window(scope)
            && !self.nodes.contains(window)
        {
            return Err("update_layout: window owner retired".into());
        }
        Ok(scope)
    }

    fn window(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if let Ok(host) = self.host_windows.get(current) {
                return Some(host.0);
            }
            current = self.nodes.get(current).ok()?.1?.parent();
        }
    }

    pub(super) fn entities(&self, scope: Option<Entity>) -> Vec<Entity> {
        let mut entities = Vec::new();
        for tab in &self.tabs {
            let parent = self
                .nodes
                .get(tab)
                .ok()
                .and_then(|node| node.1)
                .map(ChildOf::parent);
            if parent == scope {
                self.collect(tab, &mut entities);
            }
        }
        entities
    }

    fn collect(&self, entity: Entity, output: &mut Vec<Entity>) {
        let Ok(node) = self.nodes.get(entity) else {
            return;
        };
        if node.2.is_some() || node.4 || node.5 {
            output.push(entity);
        }
        if let Some(children) = node.0 {
            for child in children.iter() {
                self.collect(child, output);
            }
        }
    }

    pub(super) fn revision(&self, scope: Option<Entity>) -> u64 {
        let mut hash = DefaultHasher::new();
        scope.hash(&mut hash);
        if let Some(scope) = scope {
            let mut ancestor = scope;
            loop {
                ancestor.hash(&mut hash);
                self.host_windows
                    .get(ancestor)
                    .ok()
                    .map(|host| host.0)
                    .hash(&mut hash);
                let Some(parent) = self.nodes.get(ancestor).ok().and_then(|node| node.1) else {
                    break;
                };
                ancestor = parent.parent();
            }
            self.hash(scope, &mut hash);
        } else {
            let mut roots: Vec<Entity> = self.tabs.iter().collect();
            roots.sort();
            for root in roots {
                self.hash(root, &mut hash);
            }
        }
        hash.finish()
    }

    fn hash(&self, entity: Entity, hash: &mut DefaultHasher) {
        entity.hash(hash);
        let Ok(node) = self.nodes.get(entity) else {
            return;
        };
        node.1.map(ChildOf::parent).hash(hash);
        self.host_windows
            .get(entity)
            .ok()
            .map(|host| host.0)
            .hash(hash);
        if let Some(tab) = node.2 {
            tab.name.hash(hash);
            tab.startup_dir.hash(hash);
        }
        node.3.map(|split| split.direction as u8).hash(hash);
        (node.4, node.5, node.6, node.7).hash(hash);
        if let Some(page) = node.8 {
            page.url.hash(hash);
            page.title.hash(hash);
        }
        node.9.map(|size| size.flex_grow.to_bits()).hash(hash);
        node.10.map(|time| time.0).hash(hash);
        if let Some(zoomed) = node.11 {
            zoomed.leaf.hash(hash);
            zoomed.hidden.hash(hash);
        }
        if let Some(current) = node.12 {
            current.surface().hash(hash);
            current.metadata().url.hash(hash);
        }
        if let Some(children) = node.0 {
            children.len().hash(hash);
            for child in children.iter() {
                self.hash(child, hash);
            }
        }
    }

    pub(super) fn matches(&self, entity: Entity, kind: NodeKind) -> bool {
        let Ok(node) = self.nodes.get(entity) else {
            return false;
        };
        match kind {
            NodeKind::Tab => node.2.is_some(),
            NodeKind::Split => node.4 && node.3.is_some(),
            NodeKind::Pane => node.4 && node.3.is_none(),
            NodeKind::Stack => node.5,
        }
    }

    pub(super) fn is_stack(&self, entity: Entity) -> bool {
        self.matches(entity, NodeKind::Stack)
    }

    pub(super) fn root_split(&self, tab: Entity) -> Option<Entity> {
        let children = self.nodes.get(tab).ok()?.0?;
        children
            .iter()
            .find(|&child| self.matches(child, NodeKind::Split) && !self.retiring(child))
    }

    pub(super) fn uncaptured(&self, entity: Entity) -> bool {
        let Ok(node) = self.nodes.get(entity) else {
            return true;
        };
        let Some(source) = self.surfaces.current(entity) else {
            return node.0.is_some_and(|children| !children.is_empty());
        };
        if self
            .nodes
            .get(source)
            .ok()
            .and_then(|source| source.8)
            .is_none_or(|page| page.url.is_empty())
        {
            return true;
        }
        if let Some(children) = node.0 {
            for child in children.iter() {
                if child != source && self.browsers.contains(child) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn retiring(&self, entity: Entity) -> bool {
        self.nodes.get(entity).is_ok_and(|node| node.6)
    }

    pub(super) fn protected(&self, entity: Entity) -> bool {
        let Ok(node) = self.nodes.get(entity) else {
            return false;
        };
        if node.7 || node.6 {
            return true;
        }
        if let Some(children) = node.0 {
            for child in children.iter() {
                if self.protected(child) {
                    return true;
                }
            }
        }
        false
    }
}
