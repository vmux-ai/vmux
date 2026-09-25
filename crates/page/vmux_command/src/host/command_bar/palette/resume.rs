use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::chat::{ResumableSessions, ResumeListRequest};
use vmux_api::command_bar::{
    CommandBarQuery, CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
    CommandPaletteSelectionRequest,
};
use vmux_core::host::UiStateWrite;
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};

use super::{OpenVersion, PaletteSnapshot, RequestGeneration};

pub(super) struct PaletteResumePlugin;

impl Plugin for PaletteResumePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(update_palette_resume_draft)
            .add_observer(update_palette_resume_selection)
            .add_observer(receive_palette_resume_page)
            .add_systems(PreUpdate, attach_palette_resume);
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteResume {
    open: OpenVersion,
    active: bool,
    selected: u32,
    generation: RequestGeneration,
    inflight: Option<ResumeFlight>,
}

fn attach_palette_resume(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PaletteResume>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert(PaletteResume::default());
    }
}

fn update_palette_resume_draft(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&mut PaletteResume, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut resume, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(opened) = resume.open.accept(request.open_id) else {
        return;
    };
    if opened {
        resume.active = false;
        resume.selected = 0;
        resume.generation.advance();
        snapshot.0.open_id = request.open_id;
        snapshot.0.sessions.clear();
        snapshot.0.sessions_total = 0;
        snapshot.0.sessions_loading = false;
    }
    let wants_resume = request.start && ResumeQuery::matches(&request.query);
    if wants_resume == resume.active {
        return;
    }
    resume.active = wants_resume;
    resume.generation.advance();
    if !wants_resume {
        snapshot.0.sessions_loading = false;
        return;
    }
    snapshot.0.sessions.clear();
    snapshot.0.sessions_total = 0;
    snapshot.0.sessions_loading = true;
    if let Some(request) = resume.request_page(target, 0, &mut snapshot) {
        commands.trigger(request);
    }
}

fn update_palette_resume_selection(
    trigger: On<UiInput<CommandPaletteSelectionRequest>>,
    mut palettes: Query<(&mut PaletteResume, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut resume, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if !resume.open.matches(request.open_id) {
        return;
    }
    resume.selected = request.selected;
    if let Some(request) = resume.maybe_request_page(target, &mut snapshot) {
        commands.trigger(request);
    }
}

fn receive_palette_resume_page(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&mut PaletteResume, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<ResumableSessions>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let target = trigger.event().webview();
    let Ok((mut resume, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if let Some(request) = resume.apply_page(target, response, &mut snapshot) {
        commands.trigger(request);
    }
}

impl PaletteResume {
    fn request_page(
        &mut self,
        target: Entity,
        offset: u32,
        snapshot: &mut PaletteSnapshot,
    ) -> Option<UiInput<ResumeListRequest>> {
        if !self.active || self.inflight.is_some() {
            return None;
        }
        snapshot.0.sessions_loading = true;
        let request_id = self.generation.current();
        self.inflight = Some(ResumeFlight {
            open_generation: self.open.generation(),
            generation: self.generation.current(),
            request_id,
            offset,
        });
        Some(UiInput {
            webview: target,
            payload: ResumeListRequest {
                request_id,
                query: String::new(),
                offset,
            },
        })
    }

    fn apply_page(
        &mut self,
        target: Entity,
        response: &ResumableSessions,
        snapshot: &mut PaletteSnapshot,
    ) -> Option<UiInput<ResumeListRequest>> {
        let flight = self.inflight.take()?;
        let accepted = flight.open_generation == self.open.generation()
            && self.generation.matches(flight.generation)
            && self.active
            && flight.request_id == response.request_id
            && response.query.is_empty()
            && flight.offset == response.offset;
        if accepted {
            snapshot.0.sessions_total = response.total;
            snapshot.0.sessions_loading = false;
            if response.offset == 0 {
                snapshot.0.sessions.clone_from(&response.sessions);
            } else {
                snapshot.0.sessions.extend(response.sessions.clone());
            }
            return self.maybe_request_page(target, snapshot);
        }
        if self.active {
            let offset = snapshot.0.sessions.len() as u32;
            return self.request_page(target, offset, snapshot);
        }
        None
    }

    fn maybe_request_page(
        &mut self,
        target: Entity,
        snapshot: &mut PaletteSnapshot,
    ) -> Option<UiInput<ResumeListRequest>> {
        if !self.active || self.inflight.is_some() {
            return None;
        }
        let loaded = snapshot.0.sessions.len() as u32;
        if loaded == 0 || loaded >= snapshot.0.sessions_total {
            return None;
        }
        if self.selected.saturating_add(10) < loaded {
            return None;
        }
        self.request_page(target, loaded, snapshot)
    }
}

struct ResumeFlight {
    open_generation: u64,
    generation: u64,
    request_id: u64,
    offset: u32,
}

struct ResumeQuery;

impl ResumeQuery {
    fn matches(query: &str) -> bool {
        CommandBarQuery(query)
            .slash_token()
            .is_some_and(|(name, _)| "resume".starts_with(&name.to_lowercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_query_accepts_partial_command_names() {
        assert!(ResumeQuery::matches("/r"));
        assert!(ResumeQuery::matches("/resume"));
        assert!(!ResumeQuery::matches("/upload"));
        assert!(!ResumeQuery::matches("resume"));
    }
}
