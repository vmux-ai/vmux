//! What the next prompt will carry, for a host with no filesystem under it.
//!
//! The desktop reads a file to describe an attachment. A phone never had the file — it is on the
//! Mac — so the host resolves a path against whatever the last `@`-mention answer offered and hands
//! the result here. What this owns is the pile that builds up across several mentions: which paths
//! are already on it, and the payload the composer draws its pills from.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::page::PageEmit;
use vmux_wire::prompt_media::{CHAT_ATTACHMENTS_EVENT, ChatAttachment, ChatAttachments};

/// Keeps [`Attachments`] current with what a host has attached, and hands it to the composer.
pub struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Attach>()
            .add_message::<PageEmit>()
            .init_resource::<Attachments>()
            .add_systems(
                Update,
                (
                    Attachments::fold.in_set(PromptProjection),
                    Attachments::emit
                        .after(PromptProjection)
                        .run_if(resource_changed::<Attachments>),
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
}
