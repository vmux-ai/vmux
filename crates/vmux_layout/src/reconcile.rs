#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::protocol::{Focus, LayoutNode, LayoutSnapshot, NodeKind, Stack as StackDto, parse_id};

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateId(String),
    InvalidIdFormat(String),
    WrongKindForPosition {
        id: String,
        expected: NodeKind,
        got: NodeKind,
    },
    NewStackMissingUrl,
    NewStackMissingKind,
    NewPaneMissingStacks,
    NewTabMissingName,
    FlexWeightsLengthMismatch {
        children: usize,
        weights: usize,
    },
    FocusReferencesUnknownId(String),
    MissingReferencedEntity(Vec<String>),
}

pub fn validate(snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_ids: HashSet<String> = HashSet::new();

    for tab in &snapshot.tabs {
        if let Some(id) = &tab.id {
            let (kind, _) =
                parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
            if kind != NodeKind::Tab {
                return Err(ValidationError::WrongKindForPosition {
                    id: id.clone(),
                    expected: NodeKind::Tab,
                    got: kind,
                });
            }
            if !seen.insert(id.clone()) {
                return Err(ValidationError::DuplicateId(id.clone()));
            }
            all_ids.insert(id.clone());
        } else if tab.name.is_empty() {
            return Err(ValidationError::NewTabMissingName);
        }
        validate_node(&tab.root, &mut seen, &mut all_ids)?;
    }

    validate_focus(&snapshot.focused, &all_ids)?;
    Ok(())
}

fn validate_node(
    node: &LayoutNode,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    match node {
        LayoutNode::Split {
            id,
            flex_weights,
            children,
            ..
        } => {
            if let Some(id) = id {
                let (kind, _) =
                    parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
                if kind != NodeKind::Split {
                    return Err(ValidationError::WrongKindForPosition {
                        id: id.clone(),
                        expected: NodeKind::Split,
                        got: kind,
                    });
                }
                if !seen.insert(id.clone()) {
                    return Err(ValidationError::DuplicateId(id.clone()));
                }
                all_ids.insert(id.clone());
            }
            if !flex_weights.is_empty() && flex_weights.len() != children.len() {
                return Err(ValidationError::FlexWeightsLengthMismatch {
                    children: children.len(),
                    weights: flex_weights.len(),
                });
            }
            for child in children {
                validate_node(child, seen, all_ids)?;
            }
            Ok(())
        }
        LayoutNode::Pane { id, stacks, .. } => {
            if let Some(id) = id {
                let (kind, _) =
                    parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
                if kind != NodeKind::Pane {
                    return Err(ValidationError::WrongKindForPosition {
                        id: id.clone(),
                        expected: NodeKind::Pane,
                        got: kind,
                    });
                }
                if !seen.insert(id.clone()) {
                    return Err(ValidationError::DuplicateId(id.clone()));
                }
                all_ids.insert(id.clone());
            } else if stacks.is_empty() {
                return Err(ValidationError::NewPaneMissingStacks);
            }
            for stack in stacks {
                validate_stack(stack, seen, all_ids)?;
            }
            Ok(())
        }
    }
}

fn validate_stack(
    stack: &StackDto,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    if let Some(id) = &stack.id {
        let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
        if kind != NodeKind::Stack {
            return Err(ValidationError::WrongKindForPosition {
                id: id.clone(),
                expected: NodeKind::Stack,
                got: kind,
            });
        }
        if !seen.insert(id.clone()) {
            return Err(ValidationError::DuplicateId(id.clone()));
        }
        all_ids.insert(id.clone());
    } else {
        if stack.url.is_empty() {
            return Err(ValidationError::NewStackMissingUrl);
        }
        if stack.kind.is_empty() {
            return Err(ValidationError::NewStackMissingKind);
        }
    }
    Ok(())
}

fn validate_focus(focus: &Focus, all_ids: &HashSet<String>) -> Result<(), ValidationError> {
    for id in [&focus.tab, &focus.pane, &focus.stack]
        .into_iter()
        .flatten()
    {
        if !all_ids.contains(id) {
            return Err(ValidationError::FocusReferencesUnknownId(id.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum NodeAction {
    Match {
        existing: u64,
        desired_kind: NodeKind,
    },
    Create,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DiffPlan {
    pub actions_by_id: HashMap<String, NodeAction>,
    pub closes: Vec<String>,
    pub focus: Focus,
}

pub fn plan_diff(
    snapshot: &LayoutSnapshot,
    existing_ids: &HashSet<String>,
) -> Result<DiffPlan, ValidationError> {
    validate(snapshot)?;
    let mut actions_by_id: HashMap<String, NodeAction> = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();

    for tab in &snapshot.tabs {
        if let Some(id) = &tab.id {
            referenced.insert(id.clone());
            let (_, value) = parse_id(id).expect("validated above");
            actions_by_id.insert(
                id.clone(),
                NodeAction::Match {
                    existing: value,
                    desired_kind: NodeKind::Tab,
                },
            );
        }
        plan_node(&tab.root, &mut actions_by_id, &mut referenced);
    }

    let mut missing: Vec<String> = referenced.difference(existing_ids).cloned().collect();
    if !missing.is_empty() {
        missing.sort();
        return Err(ValidationError::MissingReferencedEntity(missing));
    }

    let closes: Vec<String> = existing_ids.difference(&referenced).cloned().collect();

    Ok(DiffPlan {
        actions_by_id,
        closes,
        focus: snapshot.focused.clone(),
    })
}

fn plan_node(
    node: &LayoutNode,
    actions_by_id: &mut HashMap<String, NodeAction>,
    referenced: &mut HashSet<String>,
) {
    match node {
        LayoutNode::Split { id, children, .. } => {
            if let Some(id) = id {
                referenced.insert(id.clone());
                let (_, value) = parse_id(id).expect("validated");
                actions_by_id.insert(
                    id.clone(),
                    NodeAction::Match {
                        existing: value,
                        desired_kind: NodeKind::Split,
                    },
                );
            }
            for c in children {
                plan_node(c, actions_by_id, referenced);
            }
        }
        LayoutNode::Pane { id, stacks, .. } => {
            if let Some(id) = id {
                referenced.insert(id.clone());
                let (_, value) = parse_id(id).expect("validated");
                actions_by_id.insert(
                    id.clone(),
                    NodeAction::Match {
                        existing: value,
                        desired_kind: NodeKind::Pane,
                    },
                );
            }
            for t in stacks {
                if let Some(tid) = &t.id {
                    referenced.insert(tid.clone());
                    let (_, value) = parse_id(tid).expect("validated");
                    actions_by_id.insert(
                        tid.clone(),
                        NodeAction::Match {
                            existing: value,
                            desired_kind: NodeKind::Stack,
                        },
                    );
                }
            }
        }
    }
}

#[cfg(not(web))]
mod apply;
#[cfg(not(web))]
pub use apply::*;
