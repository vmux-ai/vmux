use std::path::Path;

use crate::lsp::client::path_from_uri;

#[derive(Debug)]
pub struct WorkspaceEditPlan {
    pub documents: Vec<PlannedDocument>,
}

impl WorkspaceEditPlan {
    pub fn within(root: &Path, edit: &lsp_types::WorkspaceEdit) -> Result<Self, PlanRefusal> {
        let mut documents = Vec::new();
        match &edit.document_changes {
            Some(lsp_types::DocumentChanges::Edits(edits)) => {
                for doc in edits {
                    documents.push(PlannedDocument::within(root, doc)?);
                }
            }
            Some(lsp_types::DocumentChanges::Operations(ops)) => {
                for op in ops {
                    let lsp_types::DocumentChangeOperation::Edit(doc) = op else {
                        return Err(PlanRefusal::ResourceOperation);
                    };
                    documents.push(PlannedDocument::within(root, doc)?);
                }
            }
            None => {
                let changes = edit.changes.iter().flatten();
                for (uri, edits) in changes {
                    let Some(path) = path_from_uri(uri.as_str()) else {
                        return Err(PlanRefusal::UnsupportedUri);
                    };
                    let path = vmux_path::ScopedPath::resolve(root, path)
                        .map_err(|_| PlanRefusal::OutsideWorkspace)?;
                    documents.push(PlannedDocument {
                        path,
                        edits: edits.clone(),
                        version: None,
                    });
                }
            }
        }
        documents.sort_by(|a, b| a.path.as_path().cmp(b.path.as_path()));
        Ok(Self { documents })
    }
}

#[derive(Debug)]
pub struct PlannedDocument {
    pub path: vmux_path::ScopedPath,
    pub edits: Vec<lsp_types::TextEdit>,
    pub version: Option<i32>,
}

impl PlannedDocument {
    fn within(root: &Path, doc: &lsp_types::TextDocumentEdit) -> Result<Self, PlanRefusal> {
        let Some(path) = path_from_uri(doc.text_document.uri.as_str()) else {
            return Err(PlanRefusal::UnsupportedUri);
        };
        let path = vmux_path::ScopedPath::resolve(root, path)
            .map_err(|_| PlanRefusal::OutsideWorkspace)?;
        let mut edits = Vec::with_capacity(doc.edits.len());
        for one in &doc.edits {
            let edit = match one {
                lsp_types::OneOf::Left(edit) => edit.clone(),
                lsp_types::OneOf::Right(annotated) => annotated.text_edit.clone(),
            };
            edits.push(edit);
        }
        Ok(Self {
            path,
            edits,
            version: doc.text_document.version,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanRefusal {
    ResourceOperation,
    UnsupportedUri,
    OutsideWorkspace,
}

impl std::fmt::Display for PlanRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceOperation => f.write_str("resource operations are not supported"),
            Self::UnsupportedUri => f.write_str("only file:// documents can be edited"),
            Self::OutsideWorkspace => f.write_str("document is outside the language server root"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(new_text: &str) -> lsp_types::TextEdit {
        lsp_types::TextEdit {
            range: lsp_types::Range::default(),
            new_text: new_text.to_string(),
        }
    }

    fn uri(path: &Path) -> lsp_types::Uri {
        url::Url::from_file_path(path)
            .unwrap()
            .as_str()
            .parse()
            .unwrap()
    }

    #[allow(clippy::mutable_key_type)]
    fn changes(
        entries: Vec<(&Path, &str)>,
    ) -> std::collections::HashMap<lsp_types::Uri, Vec<lsp_types::TextEdit>> {
        let mut changes = std::collections::HashMap::new();
        for (path, text) in entries {
            changes.insert(uri(path), vec![edit(text)]);
        }
        changes
    }

    fn document(path: &Path, version: Option<i32>) -> lsp_types::TextDocumentEdit {
        lsp_types::TextDocumentEdit {
            text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                uri: uri(path),
                version,
            },
            edits: vec![lsp_types::OneOf::Left(edit("x"))],
        }
    }

    #[test]
    fn legacy_changes_map_is_understood() {
        let root = tempfile::tempdir().unwrap();
        let canonical_root = root.path().canonicalize().unwrap();
        let a = canonical_root.join("a.rs");
        let b = canonical_root.join("b.rs");
        let plan = WorkspaceEditPlan::within(
            root.path(),
            &lsp_types::WorkspaceEdit {
                changes: Some(changes(vec![(b.as_path(), "b"), (a.as_path(), "a")])),
                ..Default::default()
            },
        )
        .unwrap();
        let paths: Vec<_> = plan
            .documents
            .iter()
            .map(|document| document.path.as_path().to_path_buf())
            .collect();
        assert_eq!(
            paths,
            vec![a, b],
            "sorted, so a multi-document reply is deterministic"
        );
    }

    #[test]
    fn document_changes_carry_the_version() {
        let root = tempfile::tempdir().unwrap();
        let plan = WorkspaceEditPlan::within(
            root.path(),
            &lsp_types::WorkspaceEdit {
                document_changes: Some(lsp_types::DocumentChanges::Edits(vec![document(
                    &root.path().join("a.rs"),
                    Some(4),
                )])),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(plan.documents[0].version, Some(4));
    }

    #[test]
    fn document_changes_supersede_the_changes_map() {
        let root = tempfile::tempdir().unwrap();
        let canonical_root = root.path().canonicalize().unwrap();
        let legacy = canonical_root.join("legacy.rs");
        let modern = canonical_root.join("modern.rs");
        let plan = WorkspaceEditPlan::within(
            root.path(),
            &lsp_types::WorkspaceEdit {
                changes: Some(changes(vec![(legacy.as_path(), "legacy")])),
                document_changes: Some(lsp_types::DocumentChanges::Edits(vec![document(
                    &modern, None,
                )])),
                ..Default::default()
            },
        )
        .unwrap();
        let paths: Vec<_> = plan
            .documents
            .iter()
            .map(|document| document.path.as_path().to_path_buf())
            .collect();
        assert_eq!(paths, vec![modern]);
    }

    #[test]
    fn a_rename_operation_is_refused_whole() {
        let root = tempfile::tempdir().unwrap();
        let rename = lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Rename(
            lsp_types::RenameFile {
                old_uri: uri(&root.path().join("a.rs")),
                new_uri: uri(&root.path().join("b.rs")),
                options: None,
                annotation_id: None,
            },
        ));
        let ops = vec![
            lsp_types::DocumentChangeOperation::Edit(document(&root.path().join("a.rs"), None)),
            rename,
        ];
        let refusal = WorkspaceEditPlan::within(
            root.path(),
            &lsp_types::WorkspaceEdit {
                document_changes: Some(lsp_types::DocumentChanges::Operations(ops)),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(refusal, PlanRefusal::ResourceOperation);
    }

    #[test]
    fn annotated_edits_are_unwrapped() {
        let root = tempfile::tempdir().unwrap();
        let annotated = lsp_types::OneOf::Right(lsp_types::AnnotatedTextEdit {
            text_edit: edit("annotated"),
            annotation_id: "a1".to_string(),
        });
        let mut doc = document(&root.path().join("a.rs"), None);
        doc.edits = vec![annotated];
        let plan = WorkspaceEditPlan::within(
            root.path(),
            &lsp_types::WorkspaceEdit {
                document_changes: Some(lsp_types::DocumentChanges::Edits(vec![doc])),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(plan.documents[0].edits[0].new_text, "annotated");
    }

    #[test]
    fn document_outside_server_root_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let refusal = WorkspaceEditPlan::within(
            root.path(),
            &lsp_types::WorkspaceEdit {
                document_changes: Some(lsp_types::DocumentChanges::Edits(vec![document(
                    &outside.path().join("escape.rs"),
                    None,
                )])),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(refusal, PlanRefusal::OutsideWorkspace);
    }
}
