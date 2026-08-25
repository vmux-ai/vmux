use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::page::PageEmit;
use vmux_wire::prompt_media::{
    CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT, ChatAttachment, ChatAttachments,
    ChatMediaEntries, ChatMediaEntry,
};
use vmux_wire::room::RemoteMediaEntry;

use crate::room::Submitted;

pub struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Attach>()
            .add_message::<Submitted>()
            .add_message::<PageEmit>()
            .init_resource::<Attachments>()
            .init_resource::<Browsed>()
            .init_resource::<Media>()
            .add_systems(
                Update,
                (
                    Attachments::fold.in_set(PromptProjection),
                    Attachments::spend.in_set(PromptProjection),
                    Attachments::emit
                        .after(PromptProjection)
                        .run_if(resource_changed::<Attachments>),
                    Media::project
                        .in_set(PromptProjection)
                        .run_if(resource_changed::<Browsed>),
                    Media::emit
                        .after(PromptProjection)
                        .run_if(resource_changed::<Media>),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PromptProjection;

#[derive(Message)]
pub struct Attach(pub Vec<ChatAttachment>);

#[derive(Resource, Default, PartialEq)]
pub struct Attachments(pub Vec<ChatAttachment>);

#[derive(Resource, Default, PartialEq)]
pub struct Browsed {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<RemoteMediaEntry>,
}

#[derive(Resource, Default)]
pub struct Media(pub ChatMediaEntries);

impl Media {
    fn project(browsed: Res<Browsed>, mut media: ResMut<Media>) {
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
        media.0 = ChatMediaEntries {
            request_id: browsed.request_id,
            query: browsed.query.clone(),
            entries,
        };
    }

    fn emit(media: Res<Media>, mut emits: MessageWriter<PageEmit>) {
        if media.0.request_id == 0 {
            return;
        }
        let Some(emit) = PageEmit::of(CHAT_MEDIA_ENTRIES_EVENT, &media.0) else {
            return;
        };
        emits.write(emit);
    }
}

impl Attachments {
    fn emit(attachments: Res<Attachments>, mut emits: MessageWriter<PageEmit>) {
        if attachments.0.is_empty() {
            return;
        }
        let payload = ChatAttachments {
            attachments: attachments.0.clone(),
        };
        let Some(emit) = PageEmit::of(CHAT_ATTACHMENTS_EVENT, &payload) else {
            return;
        };
        emits.write(emit);
    }

    fn spend(mut submitted: MessageReader<Submitted>, mut attachments: ResMut<Attachments>) {
        if submitted.read().count() == 0 || attachments.0.is_empty() {
            return;
        }
        attachments.0.clear();
    }

    fn fold(mut asked: MessageReader<Attach>, mut attachments: ResMut<Attachments>) {
        for Attach(added) in asked.read() {
            for attachment in added {
                if attachments
                    .0
                    .iter()
                    .any(|held| held.path == attachment.path)
                {
                    continue;
                }
                attachments.0.push(attachment.clone());
            }
        }
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

        fn paths(&self) -> Vec<&str> {
            let mut paths = Vec::new();
            for attachment in &self.0.world().resource::<Attachments>().0 {
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
}
