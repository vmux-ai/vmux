use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::event::{FileErrorEvent, FileOpenEvent, KnowledgeLinkOpen};

use crate::host::note::NoteRevealLine;
use crate::host::plugin::{FileView, FileViewport};

pub(crate) struct EditorNavigationPlugin;

impl Plugin for EditorNavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_file_open)
            .add_observer(on_knowledge_link_open);
    }
}

fn on_file_open(
    trigger: On<BinReceive<FileOpenEvent>>,
    mut views: Query<(&mut FileView, &mut FileViewport, &mut PageMetadata)>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok((mut view, mut viewport, mut metadata)) = views.get_mut(entity) else {
        return;
    };
    view.navigate(
        entity,
        path,
        0,
        &mut viewport,
        &mut metadata,
        &mut manager,
        &mut commands,
    );
}

fn on_knowledge_link_open(
    trigger: On<BinReceive<KnowledgeLinkOpen>>,
    mut goto: MessageWriter<crate::lsp::manager::LspGoto>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let root = vmux_core::knowledge::KnowledgeVault::user().into_root();
    let requested = PathBuf::from(&request.path);
    let path = if request.create {
        let Ok(relative) = requested.strip_prefix(&root) else {
            return;
        };
        if requested.exists() {
            let Ok(canonical_root) = root.canonicalize() else {
                return;
            };
            let Ok(metadata) = std::fs::symlink_metadata(&requested) else {
                return;
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return;
            }
            let Ok(path) = requested.canonicalize() else {
                return;
            };
            if !path.starts_with(canonical_root) {
                return;
            }
            path
        } else {
            let relative = relative.to_string_lossy();
            match vmux_core::knowledge::KnowledgeVault::user().write_note(
                Some(&relative),
                &request.title,
                &format!("# {}", request.title),
            ) {
                Ok(path) => path,
                Err(error) => {
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(BinHostEmitEvent::from_event(
                            entity,
                            &FileErrorEvent {
                                message: error,
                                undecodable: false,
                            },
                        ));
                    }
                    return;
                }
            }
        }
    } else {
        let Ok(root) = root.canonicalize() else {
            return;
        };
        if std::fs::symlink_metadata(&requested)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return;
        }
        let Ok(path) = requested.canonicalize() else {
            return;
        };
        if !path.starts_with(root) {
            return;
        }
        path
    };
    if let Some(line) = request.line {
        commands.entity(entity).insert(NoteRevealLine(line));
    }
    goto.write(crate::lsp::manager::LspGoto {
        entity,
        path,
        line: request.line.unwrap_or(0),
        utf16_col: 0,
    });
}
