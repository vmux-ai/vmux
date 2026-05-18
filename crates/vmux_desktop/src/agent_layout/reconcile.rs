#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use vmux_service::protocol::layout::{Focus, LayoutNode, LayoutSnapshot, NodeKind, Tab, parse_id};

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateId(String),
    InvalidIdFormat(String),
    WrongKindForPosition {
        id: String,
        expected: NodeKind,
        got: NodeKind,
    },
    NewTabMissingUrl,
    NewTabMissingKind,
    NewPaneMissingTabs,
    NewSpaceMissingName,
    FlexWeightsLengthMismatch {
        children: usize,
        weights: usize,
    },
    FocusReferencesUnknownId(String),
}

pub fn validate(snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_ids: HashSet<String> = HashSet::new();

    for space in &snapshot.spaces {
        if let Some(id) = &space.id {
            let (kind, _) =
                parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
            if kind != NodeKind::Space {
                return Err(ValidationError::WrongKindForPosition {
                    id: id.clone(),
                    expected: NodeKind::Space,
                    got: kind,
                });
            }
            if !seen.insert(id.clone()) {
                return Err(ValidationError::DuplicateId(id.clone()));
            }
            all_ids.insert(id.clone());
        } else if space.name.is_empty() {
            return Err(ValidationError::NewSpaceMissingName);
        }
        validate_node(&space.root, &mut seen, &mut all_ids)?;
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
        LayoutNode::Pane { id, tabs, .. } => {
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
            } else if tabs.is_empty() {
                return Err(ValidationError::NewPaneMissingTabs);
            }
            for tab in tabs {
                validate_tab(tab, seen, all_ids)?;
            }
            Ok(())
        }
    }
}

fn validate_tab(
    tab: &Tab,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    if let Some(id) = &tab.id {
        let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
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
    } else {
        if tab.url.is_empty() {
            return Err(ValidationError::NewTabMissingUrl);
        }
        if tab.kind.is_empty() {
            return Err(ValidationError::NewTabMissingKind);
        }
    }
    Ok(())
}

fn validate_focus(focus: &Focus, all_ids: &HashSet<String>) -> Result<(), ValidationError> {
    for id in [&focus.space, &focus.pane, &focus.tab]
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

    for space in &snapshot.spaces {
        if let Some(id) = &space.id {
            referenced.insert(id.clone());
            let (_, value) = parse_id(id).expect("validated above");
            actions_by_id.insert(
                id.clone(),
                NodeAction::Match {
                    existing: value,
                    desired_kind: NodeKind::Space,
                },
            );
        }
        plan_node(&space.root, &mut actions_by_id, &mut referenced);
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
        LayoutNode::Pane { id, tabs, .. } => {
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
            for t in tabs {
                if let Some(tid) = &t.id {
                    referenced.insert(tid.clone());
                    let (_, value) = parse_id(tid).expect("validated");
                    actions_by_id.insert(
                        tid.clone(),
                        NodeAction::Match {
                            existing: value,
                            desired_kind: NodeKind::Tab,
                        },
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::layout::{Space, SplitDirection};

    fn pane(id: Option<&str>, tabs: Vec<Tab>) -> LayoutNode {
        LayoutNode::Pane {
            id: id.map(str::to_string),
            is_zoomed: false,
            tabs,
        }
    }

    fn split(id: Option<&str>, children: Vec<LayoutNode>, weights: Vec<f32>) -> LayoutNode {
        LayoutNode::Split {
            id: id.map(str::to_string),
            direction: SplitDirection::Row,
            flex_weights: weights,
            children,
        }
    }

    fn snapshot(root: LayoutNode, focus: Focus) -> LayoutSnapshot {
        LayoutSnapshot {
            spaces: vec![Space {
                id: Some("space:1".into()),
                name: "S".into(),
                is_active: true,
                root,
            }],
            focused: focus,
        }
    }

    #[test]
    fn validate_accepts_minimal_existing_layout() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![Tab {
                    id: Some("tab:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                space: Some("space:1".into()),
                pane: Some("pane:2".into()),
                tab: Some("tab:3".into()),
            },
        );
        assert!(validate(&snap).is_ok());
    }

    #[test]
    fn validate_rejects_duplicate_pane_id() {
        let snap = snapshot(
            split(
                Some("split:1"),
                vec![pane(Some("pane:2"), vec![]), pane(Some("pane:2"), vec![])],
                vec![1.0, 1.0],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::DuplicateId(_))
        ));
    }

    #[test]
    fn validate_rejects_new_pane_without_tabs() {
        let snap = snapshot(pane(None, vec![]), Focus::default());
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::NewPaneMissingTabs)
        ));
    }

    #[test]
    fn validate_rejects_new_tab_without_url() {
        let snap = snapshot(
            pane(
                None,
                vec![Tab {
                    id: None,
                    url: String::new(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::NewTabMissingUrl)
        ));
    }

    #[test]
    fn validate_rejects_new_tab_without_kind() {
        let snap = snapshot(
            pane(
                None,
                vec![Tab {
                    id: None,
                    url: "https://x".into(),
                    kind: String::new(),
                    ..Default::default()
                }],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::NewTabMissingKind)
        ));
    }

    #[test]
    fn validate_rejects_focus_to_unknown_id() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![Tab {
                    id: Some("tab:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                space: Some("space:1".into()),
                pane: Some("pane:99".into()),
                tab: None,
            },
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::FocusReferencesUnknownId(_))
        ));
    }

    #[test]
    fn validate_rejects_wrong_kind_in_position() {
        let snap = snapshot(pane(Some("tab:2"), vec![]), Focus::default());
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::WrongKindForPosition { .. })
        ));
    }

    #[test]
    fn validate_rejects_flex_weights_length_mismatch() {
        let snap = snapshot(
            split(
                Some("split:1"),
                vec![pane(
                    Some("pane:2"),
                    vec![Tab {
                        id: Some("tab:3".into()),
                        ..Default::default()
                    }],
                )],
                vec![1.0, 2.0],
            ),
            Focus::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::FlexWeightsLengthMismatch { .. })
        ));
    }

    #[test]
    fn plan_marks_existing_ids_as_matches() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![Tab {
                    id: Some("tab:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                space: Some("space:1".into()),
                pane: Some("pane:2".into()),
                tab: Some("tab:3".into()),
            },
        );
        let existing: HashSet<String> = ["space:1", "pane:2", "tab:3"]
            .into_iter()
            .map(String::from)
            .collect();
        let plan = plan_diff(&snap, &existing).unwrap();
        assert!(plan.actions_by_id.contains_key("pane:2"));
        assert!(plan.actions_by_id.contains_key("tab:3"));
        assert!(plan.closes.is_empty());
    }

    #[test]
    fn plan_lists_unreferenced_ids_for_close() {
        let snap = snapshot(
            pane(
                Some("pane:2"),
                vec![Tab {
                    id: Some("tab:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                space: Some("space:1".into()),
                pane: Some("pane:2".into()),
                tab: Some("tab:3".into()),
            },
        );
        let existing: HashSet<String> = ["space:1", "pane:2", "tab:3", "tab:4"]
            .into_iter()
            .map(String::from)
            .collect();
        let plan = plan_diff(&snap, &existing).unwrap();
        assert_eq!(plan.closes, vec!["tab:4".to_string()]);
    }

    #[test]
    fn plan_treats_id_omission_as_create() {
        let snap = snapshot(
            pane(
                None,
                vec![Tab {
                    id: None,
                    url: "https://x".into(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            ),
            Focus {
                space: Some("space:1".into()),
                pane: None,
                tab: None,
            },
        );
        let existing: HashSet<String> = ["space:1"].into_iter().map(String::from).collect();
        let plan = plan_diff(&snap, &existing).unwrap();
        assert!(plan.closes.is_empty());
        assert_eq!(plan.actions_by_id.len(), 1);
    }
}
