//! What the next prompt will carry, for a host with no filesystem under it.
//!
//! The desktop reads a file to describe an attachment. A phone never had the file — it is on the
//! Mac — so the host resolves a path against whatever the last `@`-mention answer offered and hands
//! the result here. What this owns is the pile that builds up across several mentions: which paths
//! are already on it, and the payload the composer draws its pills from.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::page::PageEmit;
use vmux_wire::prompt_media::{
    CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT, ChatAttachment, ChatAttachments,
    ChatMediaEntries, ChatMediaEntry,
};
use vmux_wire::room::RemoteMediaEntry;

use crate::room::Submitted;

/// Keeps [`Attachments`] current with what a host has attached, and hands it to the composer.
pub struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Attach>()
            // Registered here as well as by `ChatRoomPlugin`, because this plugin reads it and a
            // plugin that depends on a message registering itself elsewhere only works while the
            // two are always added together. `add_message` is idempotent.
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

/// When [`Attachments`] settles for the turn.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PromptProjection;

/// Attachments a host has resolved and wants on the next prompt.
#[derive(Message)]
pub struct Attach(pub Vec<ChatAttachment>);

/// What the composer draws its pills from.
#[derive(Resource, Default, PartialEq)]
pub struct Attachments(pub Vec<ChatAttachment>);

/// What the Mac answered the last `@`-mention with. Written by the app, read by nothing else.
///
/// The request is carried alongside the entries because the answer has to echo it: a composer that
/// has typed on since asking matches `request_id` to know the reply is still the one it wants.
#[derive(Resource, Default, PartialEq)]
pub struct Browsed {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<RemoteMediaEntry>,
}

/// The browse answer, as the composer expects to be told it.
#[derive(Resource, Default)]
pub struct Media(pub ChatMediaEntries);

impl Media {
    /// Describe what was found the way the shared composer expects to be told about it.
    ///
    /// `size` is the one field dropped: the mention list shows names and previews, and a byte count
    /// for a directory listing is noise the page has nowhere to put.
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

    /// Hand the answer to whichever composer is listening for it.
    ///
    /// The default is not pushed. `request_id` 0 is the resource before anything was asked, and a
    /// composer told about a request it never made would close its own mention list.
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
    /// Hand the pile to the composer, if there is one to draw.
    ///
    /// An empty pile is not pushed. The page clears its own pills when it submits, so saying so
    /// would only repeat what it already did — and on mount an empty payload would be noise.
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

    /// Empty the pile once the prompt carrying it has gone.
    ///
    /// A submitted attachment belongs to that turn, so leaving it would put the same file on the
    /// next prompt too. Guarded rather than assigned: an empty pile is the common case — most
    /// prompts carry nothing — and `ResMut` marks its resource changed on `DerefMut` whatever the
    /// value, which would redraw the composer on every send.
    fn spend(mut submitted: MessageReader<Submitted>, mut attachments: ResMut<Attachments>) {
        if submitted.read().count() == 0 || attachments.0.is_empty() {
            return;
        }
        attachments.0.clear();
    }

    /// Append what has not been attached already.
    ///
    /// A mention can name a path that is already on the pile — the page offers the same directory
    /// again — and nothing derefs unless something is actually appended, so a mention that adds
    /// nothing does not make the composer redraw.
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

    /// A world running the plugin, so what is asserted is what the schedule produced.
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

    /// Mentions accumulate, and the same path twice is one pill. Duplicating it would send the Mac
    /// the same file twice and show the reader two identical pills.
    #[test]
    fn attaching_accumulates_without_repeating_a_path() {
        let mut started = Started::empty();
        started.attach(&["a.png"]);
        started.attach(&["b.png", "a.png"]);

        assert_eq!(started.paths(), ["a.png", "b.png"]);
    }

    /// An attachment belongs to the turn that carried it. Left on the pile it would ride the next
    /// prompt as well, which is the Mac being sent a file the reader did not attach to it.
    #[test]
    fn submitting_spends_the_pile() {
        let mut started = Started::empty();
        started.attach(&["a.png", "b.png"]);
        started.submit();

        assert!(started.paths().is_empty());
    }
}
