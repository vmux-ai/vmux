use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
    CommandPaletteRemoveAttachmentRequest,
};
use vmux_api::prompt_media::{
    ChatAttachments, ChatMediaEntries, ChatMediaListRequest, inline_media_query,
};
use vmux_core::host::UiStateWrite;
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};

use super::{OpenVersion, PaletteSnapshot, PendingPaletteRequest, RequestDelay, RequestGeneration};

const MEDIA_DEBOUNCE: Duration = Duration::from_millis(300);

pub(super) struct PaletteMediaPlugin;

impl Plugin for PaletteMediaPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(update_palette_media_draft)
            .add_observer(remove_palette_attachment)
            .add_observer(receive_palette_attachments)
            .add_observer(receive_palette_media_entries)
            .add_systems(PreUpdate, attach_palette_media)
            .add_systems(Update, dispatch_media_request);
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteMedia {
    open: OpenVersion,
    start: bool,
    query: Option<String>,
    generation: RequestGeneration,
}

fn attach_palette_media(
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

fn update_palette_media_draft(
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
        snapshot.0.media_loading = false;
        snapshot.0.attachments.clear();
        snapshot.0.attachment_sequence = 0;
    }
    media.start = request.start;
    let query = if request.start {
        inline_media_query(&request.query).map(|query| query.query.to_string())
    } else {
        None
    };
    if media.query == query {
        return;
    }
    media.query = query.clone();
    let generation = media.generation.advance();
    snapshot.0.media_query = query.clone();
    snapshot.0.media_entries.clear();
    snapshot.0.media_loading = query.is_some();
    let Some(query) = query else {
        return;
    };
    spawn_media_request_delay(target, generation, query, &mut commands);
}

fn remove_palette_attachment(
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
}

fn receive_palette_attachments(
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
    if response.merge_into(&mut snapshot.0.attachments) {
        snapshot.0.attachment_sequence = snapshot.0.attachment_sequence.wrapping_add(1).max(1);
    }
}

fn receive_palette_media_entries(
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
    snapshot.0.media_loading = false;
}

#[derive(Component)]
struct MediaRequestDelay(RequestDelay);

fn spawn_media_request_delay(
    target: Entity,
    generation: u64,
    query: String,
    commands: &mut Commands,
) {
    commands.spawn((
        Name::new("Command Palette Media Request"),
        MediaRequestDelay(RequestDelay::new(target, generation, query, MEDIA_DEBOUNCE)),
        PendingPaletteRequest,
    ));
}

fn dispatch_media_request(
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
