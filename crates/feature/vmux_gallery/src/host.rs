use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::manifest::{FeatureManifestSource, FeaturePlugin};
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};
use vmux_layout::hosted_page::HostedUiPlugin;

use crate::{
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryDialogRequest,
    GalleryDisabledRequest, GalleryProgressRequest, GalleryResetRequest, GallerySelectRequest,
    GalleryTextRequest, GalleryUiState, Story,
};

#[vmux_page::page]
pub struct GalleryPlugin;

impl Plugin for GalleryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<Self>::default(),
            HostedUiPlugin::<Preview>::new(Self::MANIFEST),
            UiEventPlugin::<(
                GallerySelectRequest,
                GalleryDisabledRequest,
                GalleryTextRequest,
                GalleryCheckedRequest,
                GalleryChoiceRequest,
                GalleryDialogRequest,
                GalleryProgressRequest,
                GalleryClickRequest,
                GalleryResetRequest,
            )>::default(),
            PreviewPlugin,
        ));
        #[cfg(ui)]
        app.add_plugins(crate::ui::GalleryPage::plugin());
    }
}

impl FeatureManifestSource for GalleryPlugin {
    const SOURCE: &'static str = include_str!("feature.ron");
}

struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<GalleryUiState>::default())
            .add_observer(select)
            .add_observer(disable)
            .add_observer(edit)
            .add_observer(check)
            .add_observer(choose)
            .add_observer(open)
            .add_observer(progress)
            .add_observer(click)
            .add_observer(reset)
            .add_systems(
                Update,
                publish.after(bevy_cef::prelude::UiEventSystems::Flush),
            );
    }
}

#[derive(Component)]
#[require(UiState<GalleryUiState>)]
struct Preview {
    story: Story,
    disabled: bool,
    text: String,
    checked: bool,
    choice: String,
    dialog_open: bool,
    progress: f64,
    interaction_count: u64,
    last_event: String,
    revision: u64,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            story: Story::Button,
            disabled: false,
            text: String::new(),
            checked: false,
            choice: "one".to_string(),
            dialog_open: false,
            progress: 40.0,
            interaction_count: 0,
            last_event: String::new(),
            revision: 0,
        }
    }
}

impl Preview {
    fn reset(&mut self, story: Story) {
        let revision = self.revision.wrapping_add(1);
        *self = Self::default();
        self.story = story;
        self.revision = revision;
    }

    fn record(&mut self, event: &str) {
        self.interaction_count = self.interaction_count.saturating_add(1);
        self.last_event = event.to_string();
    }

    fn project(&self) -> GalleryUiState {
        GalleryUiState {
            story: self.story,
            disabled: self.disabled,
            text: self.text.clone(),
            checked: self.checked,
            choice: self.choice.clone(),
            dialog_open: self.dialog_open,
            progress: self.progress,
            interaction_count: self.interaction_count,
            last_event: self.last_event.clone(),
            revision: self.revision,
        }
    }
}

fn select(event: On<UiInput<GallerySelectRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.story != event.payload.story {
        preview.reset(event.payload.story);
    }
}

fn disable(event: On<UiInput<GalleryDisabledRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    preview.disabled = event.payload.disabled;
}

fn edit(event: On<UiInput<GalleryTextRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.disabled
        || !matches!(
            preview.story,
            Story::Input | Story::Textarea | Story::Avatar
        )
    {
        return;
    }
    preview.text = event.payload.text.chars().take(4096).collect();
    preview.record("GalleryTextRequest");
}

fn check(event: On<UiInput<GalleryCheckedRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.disabled
        || !matches!(preview.story, Story::Checkbox | Story::Switch)
    {
        return;
    }
    preview.checked = event.payload.checked;
    preview.record("GalleryCheckedRequest");
}

fn choose(event: On<UiInput<GalleryChoiceRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.disabled
        || preview.story != Story::Select
    {
        return;
    }
    if !matches!(event.payload.choice.as_str(), "one" | "two" | "three") {
        return;
    }
    preview.choice = event.payload.choice.clone();
    preview.record("GalleryChoiceRequest");
}

fn open(event: On<UiInput<GalleryDialogRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.story != Story::Dialog
        || (preview.disabled && event.payload.open)
    {
        return;
    }
    preview.dialog_open = event.payload.open;
    preview.record("GalleryDialogRequest");
}

fn progress(event: On<UiInput<GalleryProgressRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.disabled
        || preview.story != Story::Progress
        || !event.payload.value.is_finite()
    {
        return;
    }
    preview.progress = event.payload.value.clamp(0.0, 100.0);
    preview.record("GalleryProgressRequest");
}

fn click(event: On<UiInput<GalleryClickRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.disabled
        || !matches!(preview.story, Story::Button | Story::Card)
    {
        return;
    }
    preview.record(&format!(
        "GalleryClickRequest: {}",
        event.payload.label.chars().take(64).collect::<String>()
    ));
}

fn reset(event: On<UiInput<GalleryResetRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    let story = preview.story;
    preview.reset(story);
}

fn publish(previews: Query<(Entity, &Preview), Changed<Preview>>, mut commands: Commands) {
    for (entity, preview) in &previews {
        commands.trigger(UiStateWrite::<GalleryUiState>::from_event(
            entity,
            &preview.project(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_requests_are_scoped_and_disabled_controls_refuse_input() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let first = app.world_mut().spawn(Preview::default()).id();
        let second = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GallerySelectRequest {
                story: Story::Input,
            },
        });
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryTextRequest {
                revision: 1,
                text: "hello".to_string(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryDisabledRequest { disabled: true },
        });
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryTextRequest {
                revision: 1,
                text: "ignored".to_string(),
            },
        });
        app.update();

        let view = app.world().get::<Preview>(first).unwrap();
        assert_eq!(view.text, "hello");
        assert_eq!(view.interaction_count, 1);
        assert!(app.world().get::<Preview>(second).unwrap().text.is_empty());
    }

    #[test]
    fn switching_stories_clears_previous_interactions_and_reset_keeps_selection() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryClickRequest {
                revision: 0,
                label: "Primary/Md".to_string(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GallerySelectRequest {
                story: Story::Checkbox,
            },
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryCheckedRequest {
                revision: 1,
                checked: true,
            },
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryClickRequest {
                revision: 0,
                label: "stale button".to_string(),
            },
        });
        app.update();
        let view = app.world().get::<Preview>(page).unwrap();
        assert!(view.checked);
        assert_eq!(view.interaction_count, 1);
        assert_eq!(view.last_event, "GalleryCheckedRequest");

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryResetRequest,
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryCheckedRequest {
                revision: 1,
                checked: true,
            },
        });
        app.update();
        let view = app.world().get::<Preview>(page).unwrap();
        assert_eq!(view.story, Story::Checkbox);
        assert!(!view.checked);
        assert_eq!(view.interaction_count, 0);
        assert!(view.last_event.is_empty());
    }

    #[test]
    fn progress_refuses_non_finite_values_and_clamps_finite_input() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GallerySelectRequest {
                story: Story::Progress,
            },
        });
        for (value, expected) in [
            (150.0, 100.0),
            (f64::NAN, 100.0),
            (f64::INFINITY, 100.0),
            (-10.0, 0.0),
        ] {
            app.world_mut().trigger(UiInput {
                webview: page,
                payload: GalleryProgressRequest { revision: 1, value },
            });
            app.update();
            assert_eq!(app.world().get::<Preview>(page).unwrap().progress, expected);
        }
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().interaction_count,
            2
        );
    }
}
