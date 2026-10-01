use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
    CommandPaletteMediaActivateRequest, CommandPaletteMediaDismissRequest,
    CommandPaletteMediaHighlightRequest, CommandPaletteMediaMoveRequest,
    CommandPaletteRemoveAttachmentRequest, CommandPaletteState,
};
use vmux_api::prompt_media::{
    ChatAttachPaths, ChatAttachments, ChatMediaEntries, ChatMediaListRequest,
    PromptComposerAttachment, PromptMediaOption, inline_media_query, replace_inline_media_query,
};
use vmux_core::host::UiStateWrite;
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};
use vmux_core::prompt_media::{AttachmentSelection, MediaPath};
use vmux_ui::file_icon::FilePath;

use crate::{BindCommands, CommandDispatch, CommandRegistry};

use super::{
    OpenVersion, PaletteDraftInput, PaletteSnapshot, PendingPaletteRequest, RequestDelay,
    RequestGeneration,
};

const MEDIA_DEBOUNCE: Duration = Duration::from_millis(300);

impl PaletteSnapshot {
    fn project_media(&mut self) {
        let mut options = Vec::with_capacity(self.0.media_entries.len());
        for entry in &self.0.media_entries {
            options.push(PromptMediaOption {
                key: format!("media-{}", entry.path),
                name: entry.name.clone(),
                display_path: MediaPath::new(entry).display(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FilePath(&entry.name).extension_label(),
                is_dir: entry.is_dir,
            });
        }
        self.0.media_options = options;
    }

    fn project_attachments(&mut self) {
        let mut attachments = Vec::with_capacity(self.0.attachments.len());
        for (index, attachment) in self.0.attachments.iter().enumerate() {
            attachments.push(PromptComposerAttachment {
                key: format!("attachment-{}", attachment.path),
                name: attachment.name.clone(),
                label: FilePath(&attachment.name).extension_label(),
                preview_data_url: attachment.preview_data_url.clone(),
                remove_index: Some(index as u32),
            });
        }
        self.0.composer_attachments = attachments;
    }
}

pub(super) struct PaletteMediaPlugin;

impl Plugin for PaletteMediaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            CommandPaletteMediaMoveRequest,
            CommandPaletteMediaHighlightRequest,
            CommandPaletteMediaActivateRequest,
            CommandPaletteMediaDismissRequest,
        )>::default())
            .add_systems(Startup, bind_commands.in_set(BindCommands))
            .add_observer(update_draft)
            .add_observer(remove_attachment)
            .add_observer(receive_attachments)
            .add_observer(receive_entries)
            .add_observer(move_from)
            .add_observer(activate_from)
            .add_observer(dismiss_from)
            .add_observer(navigate)
            .add_observer(highlight)
            .add_observer(activate)
            .add_observer(dismiss)
            .add_systems(PreUpdate, attach)
            .add_systems(Update, dispatch_request);
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteMedia {
    open: OpenVersion,
    start: bool,
    query: Option<String>,
    generation: RequestGeneration,
}

impl PaletteMedia {
    fn update_query(
        &mut self,
        target: Entity,
        query: Option<String>,
        snapshot: &mut CommandPaletteState,
        commands: &mut Commands,
    ) {
        if self.query == query {
            return;
        }
        self.query = query.clone();
        let generation = self.generation.advance();
        snapshot.media_query = query.clone();
        snapshot.media_entries.clear();
        snapshot.media_options.clear();
        snapshot.media_loading = query.is_some();
        snapshot.media_selected = 0;
        let Some(query) = query else {
            return;
        };
        commands.spawn((
            Name::new("Command Palette Media Request"),
            MediaRequestDelay(RequestDelay::new(target, generation, query, MEDIA_DEBOUNCE)),
            PendingPaletteRequest,
        ));
    }
}

#[vmux_command::command(id = "command_bar_media_next")]
struct PaletteMediaNextBinding;

#[vmux_command::command(id = "command_bar_media_previous")]
struct PaletteMediaPreviousBinding;

#[vmux_command::command(id = "command_bar_media_choose")]
struct PaletteMediaActivateBinding;

#[vmux_command::command(id = "command_bar_media_dismiss")]
struct PaletteMediaDismissBinding;

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<PaletteMediaNextBinding>(&mut commands);
    registry.bind::<PaletteMediaPreviousBinding>(&mut commands);
    registry.bind::<PaletteMediaActivateBinding>(&mut commands);
    registry.bind::<PaletteMediaDismissBinding>(&mut commands);
}

fn attach(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PaletteMedia>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert(PaletteMedia::default());
    }
}

fn update_draft(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&mut PaletteMedia, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut media, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(opened) = media.open.accept(request.open_id) else {
        return;
    };
    if opened {
        media.query = None;
        media.generation.advance();
        snapshot.0.open_id = request.open_id;
        snapshot.0.media_query = None;
        snapshot.0.media_entries.clear();
        snapshot.0.media_options.clear();
        snapshot.0.media_loading = false;
        snapshot.0.media_selected = 0;
        snapshot.0.attachments.clear();
        snapshot.0.composer_attachments.clear();
        snapshot.0.attachment_sequence = 0;
    }
    media.start = request.start;
    let query = if request.start {
        inline_media_query(&request.query).map(|query| query.query.to_string())
    } else {
        None
    };
    media.update_query(target, query, &mut snapshot.0, &mut commands);
}

fn move_from(
    trigger: On<CommandDispatch>,
    next: Query<(), With<PaletteMediaNextBinding>>,
    previous: Query<(), With<PaletteMediaPreviousBinding>>,
    palettes: Query<&PaletteSnapshot>,
    mut commands: Commands,
) {
    let command = trigger.event().command();
    let next = if next.contains(command) {
        true
    } else if previous.contains(command) {
        false
    } else {
        return;
    };
    let target = trigger.event().invocation().caller;
    let Ok(snapshot) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMediaMoveRequest {
            open_id: snapshot.0.open_id,
            next,
        },
    });
}

fn activate_from(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<PaletteMediaActivateBinding>>,
    palettes: Query<&PaletteSnapshot>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok(snapshot) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMediaActivateRequest {
            open_id: snapshot.0.open_id,
            index: None,
        },
    });
}

fn dismiss_from(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<PaletteMediaDismissBinding>>,
    palettes: Query<&PaletteSnapshot>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok(snapshot) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMediaDismissRequest {
            open_id: snapshot.0.open_id,
        },
    });
}

fn navigate(
    trigger: On<UiInput<CommandPaletteMediaMoveRequest>>,
    mut palettes: Query<(&PaletteMedia, &mut PaletteSnapshot)>,
) {
    let request = &trigger.event().payload;
    let Ok((media, mut snapshot)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    if !media.open.matches(request.open_id) || !media.start {
        return;
    }
    let last = snapshot.0.media_entries.len().saturating_sub(1) as u32;
    snapshot.0.media_selected = match request.next {
        true => snapshot.0.media_selected.saturating_add(1).min(last),
        false => snapshot.0.media_selected.saturating_sub(1),
    };
}

fn highlight(
    trigger: On<UiInput<CommandPaletteMediaHighlightRequest>>,
    mut palettes: Query<(&PaletteMedia, &mut PaletteSnapshot)>,
) {
    let request = &trigger.event().payload;
    let Ok((media, mut snapshot)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    if !media.open.matches(request.open_id) || !media.start {
        return;
    }
    snapshot.0.media_selected = request
        .index
        .min(snapshot.0.media_entries.len().saturating_sub(1) as u32);
}

fn activate(
    trigger: On<UiInput<CommandPaletteMediaActivateRequest>>,
    mut palettes: Query<(
        &mut PaletteMedia,
        &mut PaletteDraftInput,
        &mut PaletteSnapshot,
    )>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut media, mut draft, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if !media.open.matches(request.open_id) || !media.start {
        return;
    }
    let index = request.index.unwrap_or(snapshot.0.media_selected) as usize;
    let Some(entry) = snapshot.0.media_entries.get(index).cloned() else {
        return;
    };
    let Some(query) = inline_media_query(&draft.query) else {
        return;
    };
    let reference = MediaPath::new(&entry).reference();
    let replacement = if entry.is_dir {
        format!("@{reference}/")
    } else {
        commands.trigger(UiInput {
            webview: target,
            payload: ChatAttachPaths {
                paths: vec![entry.path],
            },
        });
        String::new()
    };
    draft.query = replace_inline_media_query(&draft.query, query, &replacement);
    draft.selected = 0;
    draft.navigating = false;
    draft.input_revision = draft.input_revision.wrapping_add(1).max(1);
    let next_query = inline_media_query(&draft.query).map(|query| query.query.to_string());
    media.update_query(target, next_query, &mut snapshot.0, &mut commands);
}

fn dismiss(
    trigger: On<UiInput<CommandPaletteMediaDismissRequest>>,
    mut palettes: Query<(
        &mut PaletteMedia,
        &mut PaletteDraftInput,
        &mut PaletteSnapshot,
    )>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut media, mut draft, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if !media.open.matches(request.open_id) || !media.start {
        return;
    }
    let Some(query) = inline_media_query(&draft.query) else {
        return;
    };
    draft.query = replace_inline_media_query(&draft.query, query, "");
    draft.selected = 0;
    draft.navigating = false;
    draft.input_revision = draft.input_revision.wrapping_add(1).max(1);
    media.update_query(target, None, &mut snapshot.0, &mut commands);
}

fn remove_attachment(
    trigger: On<UiInput<CommandPaletteRemoveAttachmentRequest>>,
    mut palettes: Query<(&PaletteMedia, &mut PaletteSnapshot)>,
) {
    let request = &trigger.event().payload;
    let Ok((media, mut snapshot)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    if !media.open.matches(request.open_id) {
        return;
    }
    snapshot
        .0
        .attachments
        .retain(|attachment| attachment.path != request.path);
    snapshot.project_attachments();
}

fn receive_attachments(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&PaletteMedia, &mut PaletteSnapshot)>,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<ChatAttachments>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let Ok((media, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    if !media.start {
        return;
    }
    if AttachmentSelection::new(&mut snapshot.0.attachments).merge(response) {
        snapshot.0.attachment_sequence = snapshot.0.attachment_sequence.wrapping_add(1).max(1);
        snapshot.project_attachments();
    }
}

fn receive_entries(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&PaletteMedia, &mut PaletteSnapshot)>,
    pending: Query<(Entity, &MediaResponsePending)>,
    mut commands: Commands,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<ChatMediaEntries>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let target = trigger.event().webview();
    for (entity, request) in &pending {
        if request.matches(target, response) {
            commands.entity(entity).despawn();
        }
    }
    let Ok((media, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if !media.start
        || !media.generation.matches(response.request_id)
        || media.query.as_deref() != Some(response.query.as_str())
    {
        return;
    }
    snapshot.0.media_entries.clone_from(&response.entries);
    snapshot.project_media();
    snapshot.0.media_loading = false;
    snapshot.0.media_selected = snapshot
        .0
        .media_selected
        .min(snapshot.0.media_entries.len().saturating_sub(1) as u32);
}

#[derive(Component)]
struct MediaRequestDelay(RequestDelay);

fn dispatch_request(
    delays: Query<(Entity, &MediaRequestDelay)>,
    media: Query<&PaletteMedia>,
    mut commands: Commands,
) {
    for (entity, delay) in &delays {
        if !delay.0.ready() {
            continue;
        }
        let Ok(media) = media.get(delay.0.target) else {
            commands.entity(entity).despawn();
            continue;
        };
        if !media.generation.matches(delay.0.generation)
            || media.query.as_deref() != Some(delay.0.query.as_str())
        {
            commands.entity(entity).despawn();
            continue;
        }
        commands
            .entity(entity)
            .remove::<MediaRequestDelay>()
            .insert(MediaResponsePending {
                target: delay.0.target,
                generation: delay.0.generation,
                query: delay.0.query.clone(),
            });
        commands.trigger(UiInput {
            webview: delay.0.target,
            payload: ChatMediaListRequest {
                request_id: delay.0.generation,
                query: delay.0.query.clone(),
            },
        });
    }
}

#[derive(Component)]
struct MediaResponsePending {
    target: Entity,
    generation: u64,
    query: String,
}

impl MediaResponsePending {
    fn matches(&self, target: Entity, response: &ChatMediaEntries) -> bool {
        self.target == target
            && self.generation == response.request_id
            && self.query == response.query
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::OpenId;
    use vmux_api::prompt_media::ChatMediaEntry;

    #[test]
    fn navigation_and_dismiss_update_host_state() {
        let mut app = App::new();
        app.add_observer(navigate).add_observer(dismiss);
        let open_id = OpenId(7);
        let mut media = PaletteMedia::default();
        assert_eq!(media.open.accept(open_id), Some(true));
        media.start = true;
        media.query = Some("pic".to_string());
        let page = app
            .world_mut()
            .spawn((
                media,
                PaletteDraftInput {
                    open_id,
                    query: "show @pic".to_string(),
                    ..Default::default()
                },
                PaletteSnapshot(CommandPaletteState {
                    open_id,
                    media_query: Some("pic".to_string()),
                    media_entries: vec![
                        ChatMediaEntry {
                            path: "/tmp/one.png".to_string(),
                            name: "one.png".to_string(),
                            ..Default::default()
                        },
                        ChatMediaEntry {
                            path: "/tmp/two.png".to_string(),
                            name: "two.png".to_string(),
                            ..Default::default()
                        },
                    ],
                    media_selected: 1,
                    ..Default::default()
                }),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteMediaMoveRequest {
                open_id,
                next: false,
            },
        });

        assert_eq!(
            app.world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .0
                .media_selected,
            0
        );

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteMediaDismissRequest { open_id },
        });

        let input = app.world().get::<PaletteDraftInput>(page).unwrap();
        assert_eq!(input.query, "show ");
        assert_eq!(input.input_revision, 1);
        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert_eq!(snapshot.0.media_query, None);
        assert!(snapshot.0.media_entries.is_empty());
        assert_eq!(snapshot.0.media_selected, 0);
    }
}
