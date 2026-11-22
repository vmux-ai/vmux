use std::collections::{HashMap, HashSet};

use bevy::prelude::Entity;
use vmux_ecs::OperationTargetRevision;

use crate::protocol::{Focus, LayoutNode, NodeKind};

use super::{Reconcile, current::CurrentLayout};

impl Reconcile {
    pub(super) fn validate(&self) -> Result<HashSet<Entity>, String> {
        let mut seen = HashSet::new();
        let mut active = 0;
        for tab in &self.snapshot.tabs {
            Self::validate_id(tab.id.as_deref(), NodeKind::Tab, &mut seen)?;
            if tab.id.is_none() && tab.name.is_empty() {
                return Err("update_layout: NewTabMissingName".into());
            }
            active += usize::from(tab.is_active);
            let zoomed = Self::validate_node(&tab.root, &mut seen)?;
            if zoomed > 1 {
                return Err("update_layout: multiple zoomed panes in one tab".into());
            }
        }
        if active > 1 {
            return Err("update_layout: multiple active tabs".into());
        }
        self.validate_focus(&self.snapshot.focused, &seen)?;
        Ok(seen)
    }

    fn validate_id(
        id: Option<&str>,
        expected: NodeKind,
        seen: &mut HashSet<Entity>,
    ) -> Result<(), String> {
        let Some(id) = id else {
            return Ok(());
        };
        let (kind, entity) = Self::parse(id)?;
        if kind != expected {
            return Err(format!("update_layout: WrongKindForPosition {id}"));
        }
        if !seen.insert(entity) {
            return Err(format!("update_layout: DuplicateId {id}"));
        }
        Ok(())
    }

    fn validate_node(node: &LayoutNode, seen: &mut HashSet<Entity>) -> Result<usize, String> {
        match node {
            LayoutNode::Split {
                id,
                flex_weights,
                children,
                ..
            } => {
                Self::validate_id(id.as_deref(), NodeKind::Split, seen)?;
                if !flex_weights.is_empty() && flex_weights.len() != children.len() {
                    return Err("update_layout: FlexWeightsLengthMismatch".into());
                }
                for &weight in flex_weights {
                    if !weight.is_finite() || weight < 0.0 {
                        return Err("update_layout: invalid flex weight".into());
                    }
                }
                let mut zoomed = 0;
                for child in children {
                    zoomed += Self::validate_node(child, seen)?;
                }
                Ok(zoomed)
            }
            LayoutNode::Pane {
                id,
                stacks,
                is_zoomed,
            } => {
                Self::validate_id(id.as_deref(), NodeKind::Pane, seen)?;
                if id.is_none() && stacks.is_empty() {
                    return Err("update_layout: NewPaneMissingStacks".into());
                }
                for stack in stacks {
                    Self::validate_id(stack.id.as_deref(), NodeKind::Stack, seen)?;
                    if stack.id.is_none() && stack.url.is_empty() {
                        return Err("update_layout: NewStackMissingUrl".into());
                    }
                    if stack.id.is_none() && stack.kind.is_empty() {
                        return Err("update_layout: NewStackMissingKind".into());
                    }
                }
                Ok(usize::from(*is_zoomed))
            }
        }
    }

    fn validate_focus(&self, focus: &Focus, seen: &HashSet<Entity>) -> Result<(), String> {
        for (id, expected) in [
            (focus.tab.as_deref(), NodeKind::Tab),
            (focus.pane.as_deref(), NodeKind::Pane),
            (focus.stack.as_deref(), NodeKind::Stack),
        ] {
            if let Some(id) = id {
                let (kind, entity) = Self::parse(id)?;
                if kind != expected || !seen.contains(&entity) {
                    return Err(format!("update_layout: FocusReferencesUnknownId {id}"));
                }
            }
        }
        for tab in &self.snapshot.tabs {
            if let Some(tab_id) = focus.tab.as_deref()
                && tab.id.as_deref() != Some(tab_id)
            {
                continue;
            }
            if let Some(pane) = focus.pane.as_deref() {
                if !Self::contains(&tab.root, pane, focus.stack.as_deref()) {
                    continue;
                }
            } else if let Some(stack) = focus.stack.as_deref()
                && !Self::contains(&tab.root, stack, None)
            {
                continue;
            }
            return Ok(());
        }
        if focus.tab.is_none() && focus.pane.is_none() && focus.stack.is_none() {
            return Ok(());
        }
        Err("update_layout: inconsistent focus ancestry".into())
    }

    fn contains(node: &LayoutNode, target: &str, stack: Option<&str>) -> bool {
        match node {
            LayoutNode::Split { children, .. } => {
                for child in children {
                    if Self::contains(child, target, stack) {
                        return true;
                    }
                }
                false
            }
            LayoutNode::Pane { id, stacks, .. } => {
                if id.as_deref() == Some(target) {
                    return stack
                        .is_none_or(|id| stacks.iter().any(|item| item.id.as_deref() == Some(id)));
                }
                stack.is_none() && stacks.iter().any(|item| item.id.as_deref() == Some(target))
            }
        }
    }

    pub(super) fn parse(id: &str) -> Result<(NodeKind, Entity), String> {
        let (kind, bits) =
            NodeKind::parse_id(id).map_err(|_| format!("update_layout: InvalidIdFormat {id}"))?;
        let entity = Entity::try_from_bits(bits)
            .ok_or_else(|| format!("update_layout: InvalidIdFormat {id}"))?;
        Ok((kind, entity))
    }
}

impl CurrentLayout<'_, '_> {
    pub(super) fn preflight(
        &self,
        intent: &Reconcile,
        scope: Option<Entity>,
        revision: OperationTargetRevision,
    ) -> Result<(HashSet<Entity>, HashMap<Entity, Entity>), String> {
        if self.scope()? != scope || !revision.accepts(Some(self.revision(scope))) {
            return Err("update_layout: layout revision or scope changed".into());
        }
        let mut retained = intent.validate()?;
        let current = self.entities(scope);
        for &entity in &retained {
            if !current.contains(&entity) || self.retiring(entity) {
                return Err(format!("update_layout: MissingReferencedEntity {entity:?}"));
            }
        }
        let mut roots = HashMap::new();
        for tab in &intent.snapshot.tabs {
            if matches!(tab.root, LayoutNode::Split { id: None, .. })
                && let Some(id) = tab.id.as_deref()
            {
                let tab = Reconcile::parse(id)?.1;
                if let Some(root) = self.root_split(tab)
                    && retained.insert(root)
                {
                    roots.insert(tab, root);
                }
            }
        }
        for &entity in &current {
            if !retained.contains(&entity)
                && (self.retiring(entity)
                    || self.is_stack(entity) && (self.protected(entity) || self.uncaptured(entity)))
            {
                return Err(
                    "update_layout: close requires confirmation or presentation drain".into(),
                );
            }
        }
        for tab in &intent.snapshot.tabs {
            self.check_kind(tab.id.as_deref(), NodeKind::Tab)?;
            self.check_node(&tab.root)?;
        }
        Ok((retained, roots))
    }

    fn check_kind(&self, id: Option<&str>, expected: NodeKind) -> Result<(), String> {
        if let Some(id) = id {
            let (_, entity) = Reconcile::parse(id)?;
            if !self.matches(entity, expected) {
                return Err(format!("update_layout: WrongKindForPosition {id}"));
            }
        }
        Ok(())
    }

    fn check_node(&self, node: &LayoutNode) -> Result<(), String> {
        match node {
            LayoutNode::Split { id, children, .. } => {
                self.check_kind(id.as_deref(), NodeKind::Split)?;
                for child in children {
                    self.check_node(child)?;
                }
            }
            LayoutNode::Pane { id, stacks, .. } => {
                self.check_kind(id.as_deref(), NodeKind::Pane)?;
                for stack in stacks {
                    self.check_kind(stack.id.as_deref(), NodeKind::Stack)?;
                }
            }
        }
        Ok(())
    }
}
