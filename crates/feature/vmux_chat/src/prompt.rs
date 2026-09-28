use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use std::collections::HashMap;
use vmux_api::prompt_media::{ChatAttachment, ChatAttachments, ChatMediaEntry};
use vmux_api::room::RemoteMediaEntry;

use crate::event::ChatMediaState;
use crate::room::Submitted;
use crate::state::{ChatRuntime, ChatUiStatePlugin, ChatUiStateProjection, RepublishChatUiState};

pub struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ChatUiStatePlugin>() {
            app.add_plugins(ChatUiStatePlugin);
        }
        app.add_message::<Attach>()
            .add_message::<RemoveAttachment>()
            .add_message::<Submitted>()
            .add_message::<Browsed>()
            .add_systems(
                Update,
                (
                    (fold_attachments, remove_attachments, spend_attachments)
                        .chain()
                        .in_set(PromptProjection),
                    receive_browsed.before(project_media),
                    project_media.in_set(PromptProjection),
                    emit_attachments.after(PromptProjection),
                    emit_media.after(PromptProjection),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PromptProjection;

#[derive(Message)]
pub struct Attach(pub Vec<ChatAttachment>);

#[derive(Message)]
pub struct RemoveAttachment(pub String);

#[derive(Component, Default, PartialEq)]
pub struct Attachments(pub Vec<ChatAttachment>);

#[derive(Component, Default, PartialEq)]
pub(crate) struct AttachmentPreviews(HashMap<String, ChatAttachment>);

impl AttachmentPreviews {
    pub(crate) fn hydrate(&self, attachments: &mut [ChatAttachment]) -> bool {
        let mut changed = false;
        for attachment in attachments {
            if !attachment.preview_data_url.is_empty() {
                continue;
            }
            let Some(preview) = self.0.get(&attachment.path) else {
                continue;
            };
            if preview.preview_data_url.is_empty() {
                continue;
            }
            attachment
                .preview_data_url
                .clone_from(&preview.preview_data_url);
            changed = true;
        }
        changed
    }
}

#[derive(Component, Message, Clone, Default, PartialEq)]
pub struct Browsed {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<RemoteMediaEntry>,
}

#[derive(Component, Default)]
pub struct Media(pub ChatMediaState);

type ChangedMediaPicker<'w, 's> =
    Query<'w, 's, (&'static Browsed, &'static mut Media), (With<ChatRuntime>, Changed<Browsed>)>;

fn receive_browsed(
    mut messages: MessageReader<Browsed>,
    mut runtimes: Query<&mut Browsed, With<ChatRuntime>>,
) {
    let Ok(mut browsed) = runtimes.single_mut() else {
        return;
    };
    for update in messages.read() {
        if *browsed != *update {
            *browsed = update.clone();
        }
    }
}

fn project_media(mut runtimes: ChangedMediaPicker) {
    let Ok((browsed, mut media)) = runtimes.single_mut() else {
        return;
    };
    let mut entries = Vec::with_capacity(browsed.entries.len());
    for entry in &browsed.entries {
        entries.push(ChatMediaEntry {
            path: entry.path.clone(),
            name: entry.name.clone(),
            parent: entry.parent.clone(),
            mime_type: entry.mime_type.clone(),
            is_dir: entry.is_dir,
            preview_data_url: entry.preview_data_url.clone(),
        });
    }
    media.0 = ChatMediaState {
        request_id: browsed.request_id,
        query: browsed.query.clone(),
        entries,
        loading: false,
    };
}

fn emit_media(
    mut refreshes: MessageReader<RepublishChatUiState>,
    mut runtimes: Query<(Ref<Media>, &mut ChatUiStateProjection), With<ChatRuntime>>,
) {
    let refresh = refreshes.read().next().is_some();
    let Ok((media, mut projection)) = runtimes.single_mut() else {
        return;
    };
    if media.0.request_id == 0 || (!refresh && !media.is_changed()) {
        return;
    }
    projection.write(&media.0);
}

fn emit_attachments(
    mut refreshes: MessageReader<RepublishChatUiState>,
    mut runtimes: Query<(Ref<Attachments>, &mut ChatUiStateProjection), With<ChatRuntime>>,
) {
    let refresh = refreshes.read().next().is_some();
    let Ok((attachments, mut projection)) = runtimes.single_mut() else {
        return;
    };
    if !refresh && !attachments.is_changed() {
        return;
    }
    let payload = ChatAttachments {
        attachments: attachments.0.clone(),
    };
    projection.write(&payload);
}

fn spend_attachments(
    mut submitted: MessageReader<Submitted>,
    mut runtimes: Query<&mut Attachments, With<ChatRuntime>>,
) {
    let Ok(mut attachments) = runtimes.single_mut() else {
        return;
    };
    if submitted.read().count() == 0 || attachments.0.is_empty() {
        return;
    }
    attachments.0.clear();
}

fn fold_attachments(
    mut asked: MessageReader<Attach>,
    mut runtimes: Query<(&mut Attachments, &mut AttachmentPreviews), With<ChatRuntime>>,
) {
    let Ok((mut attachments, mut previews)) = runtimes.single_mut() else {
        return;
    };
    for Attach(added) in asked.read() {
        for attachment in added {
            if attachment.preview_data_url.is_empty() {
                continue;
            }
            previews
                .0
                .insert(attachment.path.clone(), attachment.clone());
        }
        ChatAttachments {
            attachments: added.clone(),
        }
        .merge_into(&mut attachments.0);
    }
}

fn remove_attachments(
    mut removed: MessageReader<RemoveAttachment>,
    mut runtimes: Query<&mut Attachments, With<ChatRuntime>>,
) {
    let Ok(mut attachments) = runtimes.single_mut() else {
        return;
    };
    for RemoveAttachment(path) in removed.read() {
        attachments.0.retain(|attachment| attachment.path != *path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Started(App);

    impl Started {
        fn empty() -> Self {
            let mut app = App::new();
            app.add_plugins(ChatPromptPlugin);
            app.update();
            Self(app)
        }

        fn attach(&mut self, paths: &[&str]) {
            let mut added = Vec::with_capacity(paths.len());
            for path in paths {
                added.push(ChatAttachment {
                    path: path.to_string(),
                    name: path.to_string(),
                    mime_type: String::new(),
                    size: 0,
                    preview_data_url: String::new(),
                });
            }
            self.0.world_mut().write_message(Attach(added));
            self.0.update();
        }

        fn submit(&mut self) {
            self.0.world_mut().write_message(Submitted);
            self.0.update();
        }

        fn remove(&mut self, path: &str) {
            self.0
                .world_mut()
                .write_message(RemoveAttachment(path.to_string()));
            self.0.update();
        }

        fn paths(&self) -> Vec<&str> {
            let mut paths = Vec::new();
            let attachments = self
                .0
                .world()
                .iter_entities()
                .find_map(|entity| entity.get::<Attachments>())
                .expect("chat attachments");
            for attachment in &attachments.0 {
                paths.push(attachment.path.as_str());
            }
            paths
        }
    }

    #[test]
    fn attaching_accumulates_without_repeating_a_path() {
        let mut started = Started::empty();
        started.attach(&["a.png"]);
        started.attach(&["b.png", "a.png"]);

        assert_eq!(started.paths(), ["a.png", "b.png"]);
    }

    #[test]
    fn submitting_spends_the_pile() {
        let mut started = Started::empty();
        started.attach(&["a.png", "b.png"]);
        started.submit();

        assert!(started.paths().is_empty());
    }

    #[test]
    fn removing_the_last_attachment_empties_the_pile() {
        let mut started = Started::empty();
        started.attach(&["a.png"]);
        started.remove("a.png");

        assert!(started.paths().is_empty());
    }
}
