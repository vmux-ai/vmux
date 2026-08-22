//! Turning a server's `WorkspaceEdit` into per-document work this editor can carry out.
//!
//! The two shapes a server may send — the legacy `changes` map and the newer
//! `documentChanges` — collapse to the same thing here, and the operations this client cannot
//! honour are refused whole rather than applied in part.

use std::path::PathBuf;

use crate::lsp::client::path_from_uri;

#[derive(Debug)]
pub struct WorkspaceEditPlan {
    pub documents: Vec<PlannedDocument>,
}

impl WorkspaceEditPlan {
    pub fn of(edit: &lsp_types::WorkspaceEdit) -> Result<Self, PlanRefusal> {
        let mut documents = Vec::new();
        // `documentChanges` supersedes `changes` when both are present, so never merge them.
        match &edit.document_changes {
            Some(lsp_types::DocumentChanges::Edits(edits)) => {
                for doc in edits {
                    documents.push(PlannedDocument::of(doc)?);
                }
            }
            Some(lsp_types::DocumentChanges::Operations(ops)) => {
                for op in ops {
                    let lsp_types::DocumentChangeOperation::Edit(doc) = op else {
                        return Err(PlanRefusal::ResourceOperation);
                    };
                    documents.push(PlannedDocument::of(doc)?);
                }
            }
            None => {
                let changes = edit.changes.iter().flatten();
                for (uri, edits) in changes {
                    let Some(path) = path_from_uri(uri.as_str()) else {
                        return Err(PlanRefusal::UnsupportedUri);
                    };
                    documents.push(PlannedDocument {
                        path,
                        edits: edits.clone(),
                        version: None,
                    });
                }
            }
        }
        documents.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { documents })
    }
}

#[derive(Debug)]
pub struct PlannedDocument {
    pub path: PathBuf,
    pub edits: Vec<lsp_types::TextEdit>,
    /// The document version the server computed against, when it said.
    pub version: Option<i32>,
}

impl PlannedDocument {
    fn of(doc: &lsp_types::TextDocumentEdit) -> Result<Self, PlanRefusal> {
        let Some(path) = path_from_uri(doc.text_document.uri.as_str()) else {
            return Err(PlanRefusal::UnsupportedUri);
        };
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

/// Why a whole `WorkspaceEdit` was rejected before any of it was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanRefusal {
    /// Creating, renaming or deleting files. Correspondingly not advertised in
    /// `workspace.workspaceEdit.resourceOperations`, so a conforming server will not ask.
    ResourceOperation,
    UnsupportedUri,
}

impl std::fmt::Display for PlanRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceOperation => f.write_str("resource operations are not supported"),
            Self::UnsupportedUri => f.write_str("only file:// documents can be edited"),
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

    fn uri(path: &str) -> lsp_types::Uri {
        format!("file://{path}").parse().unwrap()
    }

    /// `clippy::mutable_key_type` fires on `Uri`'s internal cache, but `WorkspaceEdit.changes`
    /// is keyed that way by `lsp-types` and nothing here mutates a key.
    #[allow(clippy::mutable_key_type)]
    fn changes(
        entries: Vec<(&str, &str)>,
    ) -> std::collections::HashMap<lsp_types::Uri, Vec<lsp_types::TextEdit>> {
        let mut changes = std::collections::HashMap::new();
        for (path, text) in entries {
            changes.insert(uri(path), vec![edit(text)]);
        }
        changes
    }

    fn document(path: &str, version: Option<i32>) -> lsp_types::TextDocumentEdit {
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
        let plan = WorkspaceEditPlan::of(&lsp_types::WorkspaceEdit {
            changes: Some(changes(vec![("/tmp/b.rs", "b"), ("/tmp/a.rs", "a")])),
            ..Default::default()
        })
        .unwrap();
        let paths: Vec<_> = plan.documents.iter().map(|d| d.path.clone()).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("/tmp/a.rs"), PathBuf::from("/tmp/b.rs")],
            "sorted, so a multi-document reply is deterministic"
        );
    }

    #[test]
    fn document_changes_carry_the_version() {
        let plan = WorkspaceEditPlan::of(&lsp_types::WorkspaceEdit {
            document_changes: Some(lsp_types::DocumentChanges::Edits(vec![document(
                "/tmp/a.rs",
                Some(4),
            )])),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(plan.documents[0].version, Some(4));
    }

    /// Merging the two would apply the same edit twice.
    #[test]
    fn document_changes_supersede_the_changes_map() {
        let plan = WorkspaceEditPlan::of(&lsp_types::WorkspaceEdit {
            changes: Some(changes(vec![("/tmp/legacy.rs", "legacy")])),
            document_changes: Some(lsp_types::DocumentChanges::Edits(vec![document(
                "/tmp/modern.rs",
                None,
            )])),
            ..Default::default()
        })
        .unwrap();
        let paths: Vec<_> = plan.documents.iter().map(|d| d.path.clone()).collect();
        assert_eq!(paths, vec![PathBuf::from("/tmp/modern.rs")]);
    }

    #[test]
    fn a_rename_operation_is_refused_whole() {
        let rename = lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Rename(
            lsp_types::RenameFile {
                old_uri: uri("/tmp/a.rs"),
                new_uri: uri("/tmp/b.rs"),
                options: None,
                annotation_id: None,
            },
        ));
        let ops = vec![
            lsp_types::DocumentChangeOperation::Edit(document("/tmp/a.rs", None)),
            rename,
        ];
        let refusal = WorkspaceEditPlan::of(&lsp_types::WorkspaceEdit {
            document_changes: Some(lsp_types::DocumentChanges::Operations(ops)),
            ..Default::default()
        })
        .unwrap_err();
        assert_eq!(refusal, PlanRefusal::ResourceOperation);
    }

    #[test]
    fn annotated_edits_are_unwrapped() {
        let annotated = lsp_types::OneOf::Right(lsp_types::AnnotatedTextEdit {
            text_edit: edit("annotated"),
            annotation_id: "a1".to_string(),
        });
        let mut doc = document("/tmp/a.rs", None);
        doc.edits = vec![annotated];
        let plan = WorkspaceEditPlan::of(&lsp_types::WorkspaceEdit {
            document_changes: Some(lsp_types::DocumentChanges::Edits(vec![doc])),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(plan.documents[0].edits[0].new_text, "annotated");
    }
}
