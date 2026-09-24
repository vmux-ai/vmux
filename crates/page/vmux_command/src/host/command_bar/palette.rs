use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::UiEventPlugin;
use vmux_api::command_bar::{
    CommandPaletteBranchesRequest, CommandPaletteDraftRequest, CommandPalettePromptHistoryRequest,
    CommandPaletteRemoveAttachmentRequest, CommandPaletteSelectionRequest, CommandPaletteState,
    OpenId,
};
use vmux_core::host::{UiState, UiStateWrite};
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};

mod branch;
mod media;
mod prompt;
mod resume;
mod search;

pub(super) struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiEventPlugin::<(
                CommandPaletteDraftRequest,
                CommandPaletteSelectionRequest,
                CommandPalettePromptHistoryRequest,
                CommandPaletteBranchesRequest,
                CommandPaletteRemoveAttachmentRequest,
            )>::default(),
            vmux_core::host::UiStatePlugin::<CommandPaletteState>::default(),
            search::PaletteSearchPlugin,
            prompt::PalettePromptPlugin,
            branch::PaletteBranchPlugin,
            media::PaletteMediaPlugin,
            resume::PaletteResumePlugin,
        ))
        .add_systems(
            PreUpdate,
            (attach_palette_snapshot, detach_palette_snapshot),
        )
        .add_systems(PostUpdate, publish_palette_snapshot)
        .add_systems(Last, keep_palette_frames_coming);
    }
}

#[derive(Component, Default)]
struct PaletteSnapshot(CommandPaletteState);

fn attach_palette_snapshot(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PaletteSnapshot>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert((
            PaletteSnapshot::default(),
            UiState::<CommandPaletteState>::default(),
        ));
    }
}

fn publish_palette_snapshot(
    snapshots: Query<(Entity, &PaletteSnapshot), Changed<PaletteSnapshot>>,
    mut commands: Commands,
) {
    for (target, snapshot) in &snapshots {
        commands.trigger(UiStateWrite::<CommandPaletteState>::from_event(
            target,
            &snapshot.0,
        ));
    }
}

fn detach_palette_snapshot(
    pages: Query<
        Entity,
        (
            With<PaletteSnapshot>,
            Without<RendersLauncherPanel>,
            Without<HostsLauncher>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).remove::<(
            PaletteSnapshot,
            UiState<CommandPaletteState>,
            search::PaletteSearch,
            prompt::PalettePrompt,
            branch::PaletteBranch,
            media::PaletteMedia,
            resume::PaletteResume,
        )>();
    }
}

#[derive(Default)]
struct OpenVersion {
    initialized: bool,
    open_id: OpenId,
    generation: RequestGeneration,
}

impl OpenVersion {
    fn accept(&mut self, open_id: OpenId) -> Option<bool> {
        if self.initialized && self.open_id == open_id {
            return Some(false);
        }
        if self.initialized && open_id.0 < self.open_id.0 {
            return None;
        }
        self.initialized = true;
        self.open_id = open_id;
        self.generation.advance();
        Some(true)
    }

    fn matches(&self, open_id: OpenId) -> bool {
        self.initialized && self.open_id == open_id
    }

    fn generation(&self) -> u64 {
        self.generation.current()
    }
}

#[derive(Default)]
struct RequestGeneration(u64);

impl RequestGeneration {
    fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }

    fn current(&self) -> u64 {
        self.0
    }

    fn matches(&self, generation: u64) -> bool {
        generation != 0 && self.0 == generation
    }
}

struct RequestDelay {
    target: Entity,
    generation: u64,
    query: String,
    due: std::time::Instant,
}

impl RequestDelay {
    fn new(target: Entity, generation: u64, query: String, delay: Duration) -> Self {
        Self {
            target,
            generation,
            query,
            due: std::time::Instant::now() + delay,
        }
    }

    fn ready(&self) -> bool {
        std::time::Instant::now() >= self.due
    }
}

#[derive(Component)]
struct PendingPaletteRequest;

fn keep_palette_frames_coming(
    pending: Query<(), With<PendingPaletteRequest>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    if pending.is_empty() {
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_versions_reject_older_inputs() {
        let mut version = OpenVersion::default();

        assert_eq!(version.accept(OpenId(4)), Some(true));
        assert_eq!(version.accept(OpenId(4)), Some(false));
        assert_eq!(version.accept(OpenId(3)), None);
        assert_eq!(version.accept(OpenId(5)), Some(true));
    }

    #[test]
    fn request_generations_reject_previous_responses() {
        let mut generation = RequestGeneration::default();
        let first = generation.advance();
        let second = generation.advance();

        assert!(!generation.matches(first));
        assert!(generation.matches(second));
    }
}
