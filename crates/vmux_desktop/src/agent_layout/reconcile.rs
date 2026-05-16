#![allow(dead_code)]

use std::collections::HashSet;
use vmux_service::protocol::layout::{
    FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, TabDto, parse_id,
};

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
    node: &LayoutNodeDto,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    match node {
        LayoutNodeDto::Split {
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
        LayoutNodeDto::Pane { id, tabs, .. } => {
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
    tab: &TabDto,
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

fn validate_focus(focus: &FocusDto, all_ids: &HashSet<String>) -> Result<(), ValidationError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::layout::{SpaceDto, SplitDirectionDto};

    fn pane(id: Option<&str>, tabs: Vec<TabDto>) -> LayoutNodeDto {
        LayoutNodeDto::Pane {
            id: id.map(str::to_string),
            is_zoomed: false,
            tabs,
        }
    }

    fn split(id: Option<&str>, children: Vec<LayoutNodeDto>, weights: Vec<f32>) -> LayoutNodeDto {
        LayoutNodeDto::Split {
            id: id.map(str::to_string),
            direction: SplitDirectionDto::Row,
            flex_weights: weights,
            children,
        }
    }

    fn snapshot(root: LayoutNodeDto, focus: FocusDto) -> LayoutSnapshot {
        LayoutSnapshot {
            spaces: vec![SpaceDto {
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
                vec![TabDto {
                    id: Some("tab:3".into()),
                    ..Default::default()
                }],
            ),
            FocusDto {
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
            FocusDto::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::DuplicateId(_))
        ));
    }

    #[test]
    fn validate_rejects_new_pane_without_tabs() {
        let snap = snapshot(pane(None, vec![]), FocusDto::default());
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
                vec![TabDto {
                    id: None,
                    url: String::new(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            ),
            FocusDto::default(),
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
                vec![TabDto {
                    id: None,
                    url: "https://x".into(),
                    kind: String::new(),
                    ..Default::default()
                }],
            ),
            FocusDto::default(),
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
                vec![TabDto {
                    id: Some("tab:3".into()),
                    ..Default::default()
                }],
            ),
            FocusDto {
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
        let snap = snapshot(pane(Some("tab:2"), vec![]), FocusDto::default());
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
                    vec![TabDto {
                        id: Some("tab:3".into()),
                        ..Default::default()
                    }],
                )],
                vec![1.0, 2.0],
            ),
            FocusDto::default(),
        );
        assert!(matches!(
            validate(&snap),
            Err(ValidationError::FlexWeightsLengthMismatch { .. })
        ));
    }
}
